import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';

import 'engine.dart';
import 'ffi_engine.dart';
import 'core/wifi_net.dart';
import 'shell/app_router.dart';
import 'state/app_store.dart';
import 'theme.dart';
import 'widgets/header.dart';

// ---------------------------------------------------------------------------
// Fake 8-ch sim: answers the same Cmd/Event JSON the real engine speaks.
// (Untouched: same protocol surface as before, fallback when FFI is missing.)
// ---------------------------------------------------------------------------
class FakeSimEngine implements EngineApi {
  final List<Strip> _strips = List.generate(
      8,
      (i) => Strip(
          ch: i,
          key: 'ch/${(i + 1).toString().padLeft(2, '0')}',
          name: 'CH${(i + 1).toString().padLeft(2, '0')}',
          fader: 0.75,
          muted: false));
  final List<List<String>> _mailbox = [];
  int _tick = 0;

  @override
  Future<String> boot() async {
    _mailbox.add([
      jsonEncode({'type': 'you', 'id': 'client-1'})
    ]);
    _snapshot();
    return 'client-1';
  }

  void _snapshot() {
    _mailbox.add([
      jsonEncode({
        'type': 'state',
        'driver': 'sim',
        'strips': [
          for (final s in _strips)
            {
              'ch': s.ch,
              'key': s.key,
              'name': s.name,
              'fader': s.fader,
              'muted': s.muted
            }
        ],
      })
    ]);
  }

  @override
  Future<void> cmd(String clientId, String json) async {
    final c = jsonDecode(json) as Map<String, dynamic>;
    switch (c['cmd']) {
      case 'connect':
      case 'scan':
      case 'probe':
        _snapshot();
        return;
      case 'fader':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].fader = ((c['value'] as num)).toDouble();
        _echo(ch, 'fader', _strips[ch].fader);
        return;
      case 'mute':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].muted = c['muted'] as bool;
        _echo(ch, 'mute', _strips[ch].muted);
        return;
    }
  }

  void _echo(int ch, String field, Object value) {
    _mailbox.add([
      jsonEncode({'type': 'param', 'ch': ch, 'field': field, 'value': value})
    ]);
    _snapshot();
  }

  @override
  Future<List<String>> poll(String clientId, int cap) async {
    // One fake 20 Hz meter frame per poll, then the queued control frames.
    _tick++;
    final out = <String>[
      jsonEncode({
        'type': 'meters',
        'levels': [
          for (var i = 0; i < _strips.length; i++)
            _strips[i].muted
                ? 0.0
                : (_strips[i].fader *
                    (0.55 + 0.35 * ((_tick * (i + 3)) % 10) / 10)
                        .clamp(0.0, 1.0))
        ]
      })
    ];
    for (final batch in _mailbox) {
      out.addAll(batch);
      if (out.length >= cap) break;
    }
    _mailbox.clear();
    return out.take(cap).toList();
  }

  @override
  Future<void> resync(String clientId) async => _snapshot();
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------
// Prefer the real embedded engine; fall back to the fake sim when the
// native library is missing (desktop `flutter run` without jniLibs, or
// any FFI init failure) so the demo always boots.
Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final ffi = await FfiEngine.create();
  runApp(OpenmixApp(
    api: ffi ?? FakeSimEngine(),
    isRealEngine: ffi != null,
  ));
}

class OpenmixApp extends StatelessWidget {
  final EngineApi api;
  final bool isRealEngine;
  const OpenmixApp(
      {super.key, required this.api, required this.isRealEngine});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'OpenMix',
      debugShowCheckedModeBanner: false,
      theme: u4Theme(),
      home: ConnectScreen(api: api, isRealEngine: isRealEngine),
    );
  }
}

/// Dark ui4 button (grey button #2b3038, h48 touch target).
class _U4Button extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;

  const _U4Button({required this.label, this.onPressed});

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    return GestureDetector(
      onTap: onPressed,
      child: Opacity(
        opacity: enabled ? 1 : 0.45,
        child: Container(
          height: 48,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: U4.control,
            border: Border.all(color: U4.lineCtl),
            borderRadius: U4.rBtn,
          ),
          child: Text(label,
              style: const TextStyle(color: U4.text1, fontSize: 15)),
        ),
      ),
    );
  }
}

/// Pre-boot connect: Boot sim, or Boot + Probe a desk IP, then enter the
/// full-port shell (mixer/meters/channel + scenes/FX/routing/link/setup).
/// Scan/probe/device pick continue inside the Link tab after boot.
class ConnectScreen extends StatefulWidget {
  final EngineApi api;
  final bool isRealEngine;
  const ConnectScreen(
      {super.key, required this.api, required this.isRealEngine});

  @override
  State<ConnectScreen> createState() => _ConnectScreenState();
}

class _ConnectScreenState extends State<ConnectScreen> {
  final _ip = TextEditingController();
  String _status = 'not booted';
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    // Optional developer launch shortcut; it only boots the demo engine.
    if (const bool.fromEnvironment('OPENMIX_START_DEMO')) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _enter();
      });
    }
  }

  @override
  void dispose() {
    _ip.dispose();
    super.dispose();
  }

  /// Boot the transport, attach the central store, then replace this page
  /// with the shell. When [probeIp] is set the probe is sent first and the
  /// shell opens on the Link tab so the result is visible.
  Future<void> _enter({String? probeIp}) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _status = 'booting…';
    });
    try {
      final wantDesk = probeIp != null && probeIp.trim().isNotEmpty;
      // Desk discovery needs runtime grants (location + nearby devices)
      // and Wi-Fi binding; the sim path skips both so the demo always boots.
      if (wantDesk) {
        final ok = await WifiNet.ensureScanPermissions();
        if (!ok) {
          final hint = await WifiNet.permissionHint() ??
              'Location + Nearby Wi-Fi devices permission required.';
          if (!mounted) return;
          setState(() {
            _busy = false;
            _status = 'permission denied: $hint';
          });
          return;
        }
        await WifiNet.bindWifi();
      }
      final id = await widget.api.boot();
      final store = AppStore(widget.api, id);
      await store.attach(id);
      store.setOnline(true);
      final tab = wantDesk ? AppTab.connect : AppTab.mixer;
      if (wantDesk) {
        // Best-effort: re-send the unicast probe a couple of times — the
        // first datagram is often lost (ARP/interface race). The Link tab
        // Retry continues from here with the exact engine error visible.
        final addr = probeIp.trim();
        store.sendRaw(Cmds.probe(addr));
      }
      if (!mounted) return;
      Navigator.of(context).pushReplacement(MaterialPageRoute(
          builder: (_) => AppShell(
              api: widget.api,
              store: store,
              initialTab: tab,
              isRealEngine: widget.isRealEngine)));
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _status = 'boot failed: $e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final real = widget.isRealEngine;
    final engineTag = real
        ? 'ENGINE: embedded openmix-engine (real)'
        : 'ENGINE: FAKE sim fallback — native lib missing, desk impossible';
    final ffiErr = real ? null : FfiEngine.lastError?.toString();
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            const U4Header(title: 'OpenMix', connected: false),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              decoration: BoxDecoration(
                color: real ? const Color(0xFF0E2A12) : const Color(0xFF3A0D0D),
                border: Border(
                    bottom:
                        BorderSide(color: real ? U4.meterGreen : U4.meterRed)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(engineTag,
                      style: TextStyle(
                          color: real ? U4.meterGreen : U4.meterRed,
                          fontSize: 13,
                          fontWeight: FontWeight.w700)),
                  if (ffiErr != null && ffiErr.isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Text('FFI: $ffiErr',
                          style:
                              const TextStyle(color: U4.text4, fontSize: 11)),
                    ),
                ],
              ),
            ),
            Expanded(
              child: SingleChildScrollView(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Text(_status,
                        style: const TextStyle(color: U4.text4, fontSize: 13)),
                    const SizedBox(height: 12),
                    _U4Button(
                        label: _busy ? 'Booting…' : 'Boot sim',
                        onPressed: _busy ? null : () => _enter()),
                    const SizedBox(height: 8),
                    TextField(
                      controller: _ip,
                      enabled: !_busy,
                      style: const TextStyle(color: U4.text1),
                      decoration: const InputDecoration(
                        labelText: 'Desk IP',
                        labelStyle: TextStyle(color: U4.text7),
                        filled: true,
                        fillColor: U4.panel,
                        border: OutlineInputBorder(
                          borderRadius: BorderRadius.all(Radius.circular(3)),
                          borderSide: BorderSide(color: U4.linePanel),
                        ),
                        enabledBorder: OutlineInputBorder(
                          borderRadius: BorderRadius.all(Radius.circular(3)),
                          borderSide: BorderSide(color: U4.linePanel),
                        ),
                      ),
                    ),
                    const SizedBox(height: 8),
                    _U4Button(
                        label: _busy ? 'Booting…' : 'Boot + Probe IP',
                        onPressed:
                            _busy ? null : () => _enter(probeIp: _ip.text)),
                    const SizedBox(height: 12),
                    const Text(
                        'Scan / device pick continue in the Link tab after boot.',
                        style: TextStyle(color: U4.text7, fontSize: 12)),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
