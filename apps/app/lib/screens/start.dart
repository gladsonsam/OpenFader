// Connections starting screen: the app's front door (see
// openfader-design-html/connections-*.html). Centered fader brand above one
// compact recent-mixers box; corner help/settings; Work offline at bottom.
//
// Wiring: everything flows through [ConnectionStore] (same contract as the
// Link tab) plus a small persisted recents list (SharedPreferences). No
// fake devices: rows come from recents + live engine `devices`, Connect
// probes with retry and surfaces real errors, manual add validates and
// probes, Refresh/Discover call the real scan. Entering the mixer happens
// only via an explicit user action or a user-initiated connect success.
library;

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../design.dart';
import 'connection.dart';

/// One persisted recent mixer (name + IP/hostname).
class RecentMixer {
  final String name;
  final String addr;
  const RecentMixer({required this.name, required this.addr});

  Map<String, String> toJson() => {'name': name, 'addr': addr};
  static RecentMixer fromJson(Map<String, dynamic> j) =>
      RecentMixer(name: '${j['name'] ?? ''}', addr: '${j['addr'] ?? ''}');
}

class ConnectionsStartScreen extends StatefulWidget {
  final ConnectionStore store;

  /// Row connected / already-connected tapped, or Work offline chosen.
  final VoidCallback onEnterMixer;

  /// Work offline chosen: the owner connects the demo desk, then enters.
  final VoidCallback onWorkOffline;

  const ConnectionsStartScreen(
      {super.key,
      required this.store,
      required this.onEnterMixer,
      required this.onWorkOffline});

  @override
  State<ConnectionsStartScreen> createState() => _ConnectionsStartState();
}

class _Row {
  final String key;
  final String name;
  final String addr;
  final bool discovered;
  final bool connected;
  final bool reachable;
  const _Row(
      {required this.key,
      required this.name,
      required this.addr,
      required this.discovered,
      required this.connected,
      required this.reachable});
}

class _ConnectionsStartState extends State<ConnectionsStartScreen> {
  List<RecentMixer> _recents = [];
  bool _remember = true;
  bool _autoFind = true;

  /// Pending user-initiated connect target (addr), or null when idle.
  String? _pending;
  String? _pendingName;
  bool _wasProbing = false;
  String? _lastError;
  bool _scannedOnce = false;

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_onStore);
    _loadPrefs();
  }

  Future<void> _loadPrefs() async {
    final prefs = await SharedPreferences.getInstance();
    if (!mounted) return;
    final raw = prefs.getString('openfader.recentMixers');
    var recents = <RecentMixer>[];
    if (raw != null && raw.isNotEmpty) {
      try {
        final list = jsonDecode(raw) as List;
        recents = [
          for (final e in list)
            if (e is Map<String, dynamic>) RecentMixer.fromJson(e)
        ];
      } catch (_) {
        recents = [];
      }
    }
    setState(() {
      _recents = recents
          .where((r) => r.addr.isNotEmpty)
          .toList(growable: true);
      _remember = prefs.getBool('openfader.rememberRecents') ?? true;
      _autoFind = prefs.getBool('openfader.autoFind') ?? true;
    });
    if (_autoFind && !_scannedOnce) {
      _scannedOnce = true;
      widget.store.scan();
    }
  }

  Future<void> _saveRecents() async {
    if (!_remember) return;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('openfader.recentMixers',
        jsonEncode([for (final r in _recents) r.toJson()]));
  }

  Future<void> _setRemember(bool v) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool('openfader.rememberRecents', v);
    if (!mounted) return;
    setState(() => _remember = v);
    if (!v) {
      await prefs.remove('openfader.recentMixers');
    } else {
      await _saveRecents();
    }
  }

  Future<void> _setAutoFind(bool v) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool('openfader.autoFind', v);
    if (!mounted) return;
    setState(() => _autoFind = v);
  }

  @override
  void dispose() {
    widget.store.removeListener(_onStore);
    super.dispose();
  }

  ConnectionStore get _store => widget.store;

  void _toast(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
        content: Text(msg,
            textAlign: TextAlign.center,
            style: AppDesign.type(15, weight: FontWeight.w500)),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3)));
  }

  /// Merged rows: recents first, then engine-known devices not already
  /// listed (marked discovered). Reachability comes from the live devices
  /// list; the connected row opens the mixer.
  List<_Row> _rows() {
    final devices = _store.devices;
    final byHost = <String, ConnDevice>{};
    for (final d in devices) {
      final host = _store.hostOf(d.id);
      if (host.isNotEmpty) byHost[host] = d;
    }
    final seen = <String>{};
    final rows = <_Row>[];
    for (final r in _recents) {
      seen.add(r.addr);
      final d = byHost[r.addr];
      rows.add(_Row(
          key: 'recent:${r.addr}',
          name: r.name.isEmpty ? r.addr : r.name,
          addr: r.addr,
          discovered: false,
          connected: d?.connected ?? false,
          reachable: d != null));
    }
    for (final d in devices) {
      final host = _store.hostOf(d.id);
      if (host.isEmpty || seen.contains(host)) continue;
      seen.add(host);
      rows.add(_Row(
          key: 'dev:${d.id}',
          name: d.name.isEmpty ? host : d.name,
          addr: host,
          discovered: true,
          connected: d.connected,
          reachable: true));
    }
    return rows;
  }

  void _onStore() {
    if (!mounted) return;
    final err = _store.scanError;
    if (err != null && err.isNotEmpty && err != _lastError) {
      _lastError = err;
      if (_pending != null) {
        final target = _pendingName ?? _pending!;
        _pending = null;
        _pendingName = null;
        _toast('Could not reach $target');
      } else {
        _toast(err);
      }
    }
    final probing = _store.probing;
    // A user-initiated probe that settles without reaching its target and
    // without an engine error means the host stayed silent: say so.
    if (_wasProbing && !probing && _pending != null) {
      final addr = _pending!;
      final name = _pendingName ?? addr;
      final hit = _store.connectedHost == addr ||
          _store.devices.any((d) => d.connected);
      if (!hit && (_store.scanError?.isEmpty ?? true)) {
        _pending = null;
        _pendingName = null;
        _toast('No response from $name');
      }
    }
    _wasProbing = probing;
    // Successful user-initiated connect: persist + enter the mixer.
    if (_pending != null && !probing) {
      final addr = _pending!;
      final name = _pendingName ?? addr;
      final hit = _store.connectedHost == addr ||
          _store.devices.any((d) => d.connected);
      if (hit) {
        _pending = null;
        _pendingName = null;
        _rememberRecent(name, addr);
        widget.onEnterMixer();
        return;
      }
    }
    setState(() {});
  }

  void _rememberRecent(String name, String addr) {
    if (!_remember || addr.isEmpty) return;
    final i = _recents.indexWhere((r) => r.addr == addr);
    if (i >= 0) {
      if (_recents[i].name != name && name.isNotEmpty) {
        _recents[i] = RecentMixer(name: name, addr: addr);
      }
    } else {
      _recents.add(RecentMixer(name: name, addr: addr));
    }
    _saveRecents();
  }

  void _connectRow(_Row row) {
    if (row.connected) {
      widget.onEnterMixer();
      return;
    }
    final host = row.addr;
    if (host.isEmpty) {
      // Engine-known device without an IP: connect by id, then enter when
      // the link reports it (errors surface via the store listener).
      _pending = 'id:${row.key}';
      _pendingName = row.name;
      _store.connectDevice(_deviceIdFor(row));
      _toast('Connecting to ${row.name}…');
      return;
    }
    _pending = host;
    _pendingName = row.name;
    _store.probe(host);
  }

  String _deviceIdFor(_Row row) {
    for (final d in _store.devices) {
      if (_store.hostOf(d.id) == row.addr && row.addr.isNotEmpty) return d.id;
    }
    return '';
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 700;
    return Scaffold(
      backgroundColor: AppDesign.background,
      body: SafeArea(
        child: Stack(
          children: [
            Center(
              child: SingleChildScrollView(
                padding: EdgeInsets.symmetric(
                    horizontal: wide ? 32 : 24, vertical: 24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _Brand(wide: wide),
                    SizedBox(height: wide ? 56 : 36),
                    _Box(
                      wide: wide,
                      title: _rows().any((r) => r.discovered)
                          ? 'Mixers'
                          : 'Recent mixers',
                      onRefresh:
                          _store.scanning ? null : () => _store.scan(),
                      refreshing: _store.scanning,
                      error: (_store.scanError?.isEmpty ?? true)
                          ? null
                          : _store.scanError,
                      rows: _rows(),
                      pending: _pending,
                      onConnect: _connectRow,
                      onDiscover: () => _store.scan(),
                      onAdd: _showManualAdd,
                    ),
                    SizedBox(height: wide ? 100 : 72),
                    _WorkOffline(onTap: widget.onWorkOffline),
                  ],
                ),
              ),
            ),
            Positioned(
              top: 8,
              right: wide ? 16 : 8,
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  _CornerIcon(
                      tooltip: 'Connection help',
                      onTap: _showHelp,
                      child: const _HelpGlyph()),
                  const SizedBox(width: 6),
                  _CornerIcon(
                      tooltip: 'Settings',
                      onTap: _showSettings,
                      child: const _GearGlyph()),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  void _showHelp() {
    showDialog<void>(
      context: context,
      builder: (context) => _Dialog(
        title: 'Connection help',
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _DialogCopy(
                'Refresh checks your known mixers and looks for desks on this network.'),
            _DialogCopy(
                'Discover finds mixers on the local network and merges them into the same list.'),
            _DialogCopy(
                'Use Add manually to enter an IP address or hostname when the mixer is on another network.'),
            _DialogCopy(
                'Work offline opens the built-in demo desk with no network connection.'),
          ],
        ),
      ),
    );
  }

  void _showSettings() {
    showDialog<void>(
      context: context,
      builder: (context) => _PrefsDialog(
        remember: _remember,
        autoFind: _autoFind,
        onRemember: _setRemember,
        onAutoFind: _setAutoFind,
      ),
    );
  }

  void _showManualAdd() {
    showDialog<void>(
      context: context,
      builder: (context) => _ManualAddDialog(
        onSubmit: (addr, name) {
          _pending = addr;
          _pendingName = name.isEmpty ? addr : name;
          _store.probe(addr);
        },
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Brand: approved logo mark + OpenFader name (Barlow Semibold).
// ---------------------------------------------------------------------------

class _Brand extends StatelessWidget {
  final bool wide;
  const _Brand({required this.wide});

  @override
  Widget build(BuildContext context) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
              width: wide ? 76 : 50,
              height: wide ? 76 : 50,
              child: SvgPicture.asset('assets/brand/open-fader-logo.svg',
                  semanticsLabel: 'OpenFader logo')),
          SizedBox(width: wide ? 24 : 16),
          Text('OpenFader',
              style: AppDesign.type(wide ? 48 : 34,
                  weight: FontWeight.w600)),
        ],
      );
}

// ---------------------------------------------------------------------------
// Recent-mixers box.
// ---------------------------------------------------------------------------

class _Box extends StatelessWidget {
  final bool wide;
  final String title;
  final VoidCallback? onRefresh;
  final bool refreshing;
  final String? error;
  final List<_Row> rows;
  final String? pending;
  final ValueChanged<_Row> onConnect;
  final VoidCallback onDiscover;
  final VoidCallback onAdd;

  const _Box(
      {required this.wide,
      required this.title,
      required this.onRefresh,
      required this.refreshing,
      required this.error,
      required this.rows,
      required this.pending,
      required this.onConnect,
      required this.onDiscover,
      required this.onAdd});

  @override
  Widget build(BuildContext context) => ConstrainedBox(
        constraints: BoxConstraints(maxWidth: wide ? 680 : 560),
        child: Container(
          decoration: BoxDecoration(
            color: const Color(0xff262626),
            border: Border.all(color: const Color(0xff383838)),
            borderRadius: BorderRadius.circular(10),
          ),
          clipBehavior: Clip.antiAlias,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: EdgeInsets.symmetric(
                    horizontal: wide ? 28 : 20,
                    vertical: wide ? 20 : 14),
                child: Row(
                  children: [
                    Expanded(
                        child: Text(title,
                            style: AppDesign.type(wide ? 27 : 21,
                                weight: FontWeight.w500))),
                    _IconButton(
                        tooltip: 'Refresh recent mixers',
                        onTap: onRefresh,
                        child: _RefreshGlyph(spinning: refreshing)),
                  ],
                ),
              ),
              const Divider(color: Color(0xff3b3b3b), height: 1),
              if (error != null)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20, vertical: 10),
                  child: Text(error!,
                      style: AppDesign.type(wide ? 18 : 15,
                          color: AppDesign.mute)),
                ),
              if (rows.isEmpty)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20,
                      vertical: wide ? 24 : 18),
                  child: Text('No mixers yet — Discover or Add manually.',
                      style: AppDesign.type(wide ? 20 : 17,
                          color: AppDesign.mutedText)),
                )
              else
                for (var i = 0; i < rows.length; i++) ...[
                  _DeviceRow(
                      row: rows[i],
                      wide: wide,
                      busy: pending != null,
                      onConnect: () => onConnect(rows[i])),
                  if (i < rows.length - 1)
                    const Divider(
                        color: Color(0xff3b3b3b), height: 1, indent: 0),
                ],
              const Divider(color: Color(0xff3b3b3b), height: 1),
              _BoxActions(
                  wide: wide, onDiscover: onDiscover, onAdd: onAdd),
            ],
          ),
        ),
      );
}

class _DeviceRow extends StatelessWidget {
  final _Row row;
  final bool wide;
  final bool busy;
  final VoidCallback onConnect;
  const _DeviceRow(
      {required this.row,
      required this.wide,
      required this.busy,
      required this.onConnect});

  @override
  Widget build(BuildContext context) {
    final primary = row.connected || row.reachable;
    final label = row.connected
        ? 'Open mixer'
        : row.reachable
            ? 'Connect'
            : 'Retry';
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 28 : 20, vertical: wide ? 20 : 16),
      child: Row(
        children: [
          Expanded(
              child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                Text(row.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppDesign.type(wide ? 24 : 21,
                        weight: FontWeight.w500)),
                const SizedBox(height: 4),
                Text(
                    row.discovered
                        ? 'Discovered — ${row.addr}'
                        : row.addr,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppDesign.type(wide ? 20 : 17,
                        color: AppDesign.mutedText)),
              ])),
          const SizedBox(width: 12),
          SizedBox(
            height: wide ? 56 : 44,
            child: ElevatedButton(
              onPressed: busy ? null : onConnect,
              style: ElevatedButton.styleFrom(
                  backgroundColor:
                      primary ? AppDesign.lime : AppDesign.control,
                  foregroundColor: primary
                      ? const Color(0xff171717)
                      : AppDesign.text,
                  disabledBackgroundColor: AppDesign.control,
                  shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(6)),
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 22 : 14),
                  textStyle: AppDesign.type(wide ? 22 : 18,
                      weight: FontWeight.w600)),
              child: Text(label),
            ),
          ),
        ],
      ),
    );
  }
}

class _BoxActions extends StatelessWidget {
  final bool wide;
  final VoidCallback onDiscover;
  final VoidCallback onAdd;
  const _BoxActions(
      {required this.wide, required this.onDiscover, required this.onAdd});

  @override
  Widget build(BuildContext context) {
    final style = TextButton.styleFrom(
        foregroundColor: AppDesign.text,
        backgroundColor: Colors.transparent,
        disabledBackgroundColor: Colors.transparent,
        minimumSize: Size(64, wide ? 60 : 52),
        padding: const EdgeInsets.symmetric(horizontal: 6),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        textStyle:
            AppDesign.type(wide ? 21 : 16, weight: FontWeight.w500));
    Widget label(String text) => Flexible(
        fit: FlexFit.loose,
        child: Text(text,
            maxLines: 1,
            softWrap: false,
            textAlign: TextAlign.center,
            style: AppDesign.type(wide ? 21 : 16,
                weight: FontWeight.w500)));
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 16 : 10, vertical: wide ? 12 : 10),
      child: Row(
        children: [
          Expanded(
              child: TextButton(
                  style: style,
                  onPressed: onDiscover,
                  child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const _ScanGlyph(),
                        const SizedBox(width: 8),
                        label('Discover mixers'),
                      ]))),
          const SizedBox(width: 12),
          Container(width: 1, height: 36, color: const Color(0xff3b3b3b)),
          const SizedBox(width: 12),
          Expanded(
              child: TextButton(
                  style: style,
                  onPressed: onAdd,
                  child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const _PlusGlyph(),
                        const SizedBox(width: 8),
                        label('Add manually'),
                      ]))),
        ],
      ),
    );
  }
}

class _WorkOffline extends StatelessWidget {
  final VoidCallback onTap;
  const _WorkOffline({required this.onTap});

  @override
  Widget build(BuildContext context) => TextButton(
        onPressed: onTap,
        style: TextButton.styleFrom(
            foregroundColor: AppDesign.text,
            backgroundColor: Colors.transparent,
            disabledBackgroundColor: Colors.transparent,
            minimumSize: const Size(200, 56),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle:
                AppDesign.type(22, weight: FontWeight.w500)),
        child: const Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Work offline'),
            SizedBox(width: 8),
            _ChevronGlyph(),
          ],
        ),
      );
}

// ---------------------------------------------------------------------------
// Glyphs (Barlow-era line icons matching the mock SVGs).
// ---------------------------------------------------------------------------

class _HelpGlyph extends StatelessWidget {
  const _HelpGlyph();
  @override
  Widget build(BuildContext context) => const Text('?',
      style: TextStyle(
          fontSize: 24,
          fontWeight: FontWeight.w600,
          color: Color(0xffb5b5b5)));
}

class _GearGlyph extends StatelessWidget {
  const _GearGlyph();
  @override
  Widget build(BuildContext context) => Icon(Icons.settings_outlined,
      size: 28, color: const Color(0xffb5b5b5));
}

class _RefreshGlyph extends StatelessWidget {
  final bool spinning;
  const _RefreshGlyph({required this.spinning});
  @override
  Widget build(BuildContext context) => Icon(Icons.refresh,
      size: 26,
      color: spinning
          ? AppDesign.lime
          : const Color(0xffb5b5b5));
}

class _ScanGlyph extends StatelessWidget {
  const _ScanGlyph();
  @override
  Widget build(BuildContext context) => const Icon(
      Icons.filter_center_focus,
      size: 24,
      color: Color(0xffb5b5b5));
}

class _PlusGlyph extends StatelessWidget {
  const _PlusGlyph();
  @override
  Widget build(BuildContext context) =>
      const Icon(Icons.add, size: 24, color: Color(0xffb5b5b5));
}

class _ChevronGlyph extends StatelessWidget {
  const _ChevronGlyph();
  @override
  Widget build(BuildContext context) => const Icon(Icons.chevron_right,
      size: 22, color: Color(0xffb5b5b5));
}

class _CornerIcon extends StatelessWidget {
  final String tooltip;
  final VoidCallback onTap;
  final Widget child;
  const _CornerIcon(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: tooltip,
        child: Tooltip(
          message: tooltip,
          child: SizedBox(
            width: 56,
            height: 56,
            child: InkWell(
              borderRadius: BorderRadius.circular(28),
              onTap: onTap,
              child: Center(child: child),
            ),
          ),
        ),
      );
}

class _IconButton extends StatelessWidget {
  final String tooltip;
  final VoidCallback? onTap;
  final Widget child;
  const _IconButton(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: tooltip,
        child: Tooltip(
          message: tooltip,
          child: SizedBox(
            width: 50,
            height: 50,
            child: InkWell(
              borderRadius: BorderRadius.circular(8),
              onTap: onTap,
              child: Center(child: child),
            ),
          ),
        ),
      );
}

// ---------------------------------------------------------------------------
// Dialogs.
// ---------------------------------------------------------------------------

class _Dialog extends StatelessWidget {
  final String title;
  final Widget child;
  const _Dialog({required this.title, required this.child});

  @override
  Widget build(BuildContext context) => Dialog(
        backgroundColor: const Color(0xff262626),
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
            side: const BorderSide(color: Color(0xff383838))),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(children: [
                  Expanded(
                      child: Text(title,
                          style: AppDesign.type(24,
                              weight: FontWeight.w600))),
                  _IconButton(
                      tooltip: 'Close',
                      onTap: () => Navigator.pop(context),
                      child: const Icon(Icons.close,
                          size: 24, color: Color(0xffb5b5b5))),
                ]),
                const SizedBox(height: 12),
                child,
              ],
            ),
          ),
        ),
      );
}

class _DialogCopy extends StatelessWidget {
  final String text;
  const _DialogCopy(this.text);
  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 6),
        child: Text(text,
            style: AppDesign.type(18, color: const Color(0xffd5d5d5))),
      );
}

class _PrefsDialog extends StatelessWidget {
  final bool remember;
  final bool autoFind;
  final ValueChanged<bool> onRemember;
  final ValueChanged<bool> onAutoFind;
  const _PrefsDialog(
      {required this.remember,
      required this.autoFind,
      required this.onRemember,
      required this.onAutoFind});

  @override
  Widget build(BuildContext context) => _Dialog(
        title: 'Settings',
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            _SettingRow(label: 'Appearance', value: 'Dark', onTap: null),
            _ToggleRow(
                label: 'Remember recent mixers',
                value: remember,
                onChanged: (v) {
                  onRemember(v);
                  Navigator.pop(context);
                }),
            _ToggleRow(
                label: 'Find mixers automatically',
                value: autoFind,
                onChanged: (v) {
                  onAutoFind(v);
                  Navigator.pop(context);
                }),
          ],
        ),
      );
}

class _SettingRow extends StatelessWidget {
  final String label;
  final String value;
  final VoidCallback? onTap;
  const _SettingRow(
      {required this.label, required this.value, required this.onTap});

  @override
  Widget build(BuildContext context) => Padding(
        padding: const EdgeInsets.symmetric(vertical: 14),
        child: Row(children: [
          Expanded(
              child: Text(label,
                  style: AppDesign.type(19, color: AppDesign.text))),
          Text(value,
              style:
                  AppDesign.type(19, color: AppDesign.mutedText)),
        ]),
      );
}

class _ToggleRow extends StatelessWidget {
  final String label;
  final bool value;
  final ValueChanged<bool> onChanged;
  const _ToggleRow(
      {required this.label, required this.value, required this.onChanged});

  @override
  Widget build(BuildContext context) => InkWell(
        onTap: () => onChanged(!value),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 14),
          child: Row(children: [
            Expanded(
                child: Text(label,
                    style: AppDesign.type(19, color: AppDesign.text))),
            Checkbox(
                value: value,
                onChanged: (v) => onChanged(v ?? false),
                activeColor: AppDesign.lime,
                checkColor: const Color(0xff171717),
                side: const BorderSide(color: Color(0xffb5b5b5))),
          ]),
        ),
      );
}

bool _validHost(String v) {
  final ip = RegExp(r'^(?:\d{1,3}\.){3}\d{1,3}$').hasMatch(v);
  if (ip) {
    return v.split('.').every((p) {
      final n = int.tryParse(p);
      return n != null && n <= 255;
    });
  }
  return RegExp(r'^[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?$').hasMatch(v);
}

class _ManualAddDialog extends StatefulWidget {
  final void Function(String addr, String name) onSubmit;
  const _ManualAddDialog({required this.onSubmit});

  @override
  State<_ManualAddDialog> createState() => _ManualAddDialogState();
}

class _ManualAddDialogState extends State<_ManualAddDialog> {
  final _addr = TextEditingController();
  final _name = TextEditingController();
  String? _error;

  @override
  void dispose() {
    _addr.dispose();
    _name.dispose();
    super.dispose();
  }

  void _submit() {
    final addr = _addr.text.trim();
    if (!_validHost(addr)) {
      setState(() => _error = 'Enter a valid IP address or hostname.');
      return;
    }
    widget.onSubmit(addr, _name.text.trim());
    Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _Dialog(
        title: 'Add mixer manually',
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text('IP address or hostname',
                style:
                    AppDesign.type(18, color: AppDesign.mutedText)),
            const SizedBox(height: 6),
            TextField(
              controller: _addr,
              keyboardType: TextInputType.url,
              autocorrect: false,
              style: AppDesign.type(19),
              decoration: InputDecoration(
                filled: true,
                fillColor: const Color(0xff101010),
                border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        const BorderSide(color: Color(0xff414141))),
                enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        const BorderSide(color: Color(0xff414141))),
                focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide: const BorderSide(color: AppDesign.lime)),
                contentPadding: const EdgeInsets.symmetric(
                    horizontal: 14, vertical: 14),
              ),
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: 14),
            Text('Name (optional)',
                style:
                    AppDesign.type(18, color: AppDesign.mutedText)),
            const SizedBox(height: 6),
            TextField(
              controller: _name,
              style: AppDesign.type(19),
              decoration: InputDecoration(
                filled: true,
                fillColor: const Color(0xff101010),
                border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        const BorderSide(color: Color(0xff414141))),
                enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        const BorderSide(color: Color(0xff414141))),
                focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide: const BorderSide(color: AppDesign.lime)),
                contentPadding: const EdgeInsets.symmetric(
                    horizontal: 14, vertical: 14),
              ),
              onSubmitted: (_) => _submit(),
            ),
            if (_error != null) ...[
              const SizedBox(height: 10),
              Text(_error!,
                  style:
                      AppDesign.type(17, color: AppDesign.mute)),
            ],
            const SizedBox(height: 18),
            SizedBox(
              height: 52,
              child: ElevatedButton(
                onPressed: _submit,
                style: ElevatedButton.styleFrom(
                    backgroundColor: AppDesign.lime,
                    foregroundColor: const Color(0xff171717),
                    shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(6)),
                    textStyle: AppDesign.type(19,
                        weight: FontWeight.w600)),
                child: const Text('Add mixer'),
              ),
            ),
          ],
        ),
      );
}
