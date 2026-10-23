// Minimal central store: the live engine binding for the app, reduced to
// what the app renders: connection state,
// strips, meters and scenes over [EngineApi], with optimistic-write+confirm
// (see `lib/core/store.dart`) and 60 Hz
// coalesced sends (see `lib/core/coalescer.dart`).
//
// One [AppStore] implements the [MixerStore], [MetersStore] and
// [ChannelStore] contracts (constructor injection — no global singleton,
// so this never fights the screen workers). The screens own their 80 ms
// meter loops and call [pollMeters]; control state notifies through
// [ChangeNotifier], meter levels ride stable [ValueNotifier]s updated in
// place and never rebuild widgets.
//
// Mapping gaps (honest, not faked):
// - Channel writes use canonical desk catalogue leaves
//   (crates/core/src/params.rs: eqN_freq/gain/q, hpf_freq/hpf_on/hpf_slope,
//   gate_/comp_*, sendN_level/tap/on, polarity — never invert/eqN_f/eqN_g/
//   sendN_mute). Short UI leaves are translated in [AppStore.deskLeaf], the
//   single choke point covering both direct cmdParam calls and the
//   ChannelWrites queue. Per-band EQ bypass stays fixture-local: the
//   catalogue has no eqN_on leaf (web parity).
// - EQ/dyn/send snapshots still start from fixture defaults until the
//   param-plane cache fills them from desk echoes.
// - Bus rows render the MAIN bus (+ known engine buses); the bus popup
//   picks the master bus shown, it does not re-patch routing.
// - Binary meter/GR/tap frames apply via [applyBinaryFrame] for the future
//   WS-transport path; `EngineApi.poll` only carries JSON text today.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';

import '../core/channel_views.dart';
import '../core/coalescer.dart';
import '../core/store.dart' as ps;
import '../engine.dart';
import '../theme.dart';
import '../screens/channel.dart';
import '../screens/meters.dart';
import '../mixer/store.dart';

double _linToDb(double l) => (l <= 0 || !l.isFinite)
    ? double.negativeInfinity
    : 20 * math.log(l) / math.ln10;

class AppStore extends ChangeNotifier
    implements MixerStore, MetersStore, ChannelStore {
  AppStore(this._api, this._clientId)
      : _coalescer = WriteCoalescer(TimerCoalescerClock());

  final EngineApi _api;
  String _clientId;
  final WriteCoalescer _coalescer;

  final EngineState engine = EngineState();
  bool _polling = false;
  final ps.StoreMaps _maps = ps.StoreMaps();

  // --- connection (phase/message/host, reduced) ---
  bool _online = false;
  bool _demo = true;
  String _deskName = 'sim';
  String _message = 'Not connected';

  // --- visible transport errors (never silently swallowed) ---
  // Engine `error` frames already land in engine.error; poll/cmd transport
  // failures (socket/FFI hiccups) previously vanished in catch (_). They
  // now surface here so the Link tab can render them with a timestamp.
  String _transportError = '';
  DateTime? _transportErrorAt;
  String _lastEngineError = '';
  DateTime? _engineErrorAt;

  // --- mixer control state ---
  final Map<int, double> _confirmedFader = {};
  final Map<int, bool> _confirmedMute = {};
  String _bank = 'ch1';
  String _busId = 'main';
  double _masterFader = 0.75;
  bool _masterMuted = false;

  // --- meter notifiers (stable, updated in place) ---
  final List<ValueNotifier<double>> _meters = [];
  final ValueNotifier<double> _masterMeter = ValueNotifier(0);
  final Map<int, BridgeTap> _taps = {};
  final Map<String, ValueNotifier<double>> _busMeters = {};
  final Map<int, double> _peakAtMs = {};

  // --- channel page state ---
  int _channel = 0;
  ChannelTab _tab = ChannelTab.input;
  int _eqBand = 1;

  /// Per-band EQ bypass is fixture-local: the desk catalogue has a global
  /// `eq_on` but no per-band `eqN_on` leaf (web parity: per-band bypass
  /// never hits the wire). An echo of `eqN_on` still wins if a driver
  /// ever sends one.
  final Map<int, bool> _eqBandOn = {};
  final ValueNotifier<double> _levelDb = ValueNotifier(double.negativeInfinity);
  final ValueNotifier<List<double>> _spectrum = ValueNotifier(const []);
  final List<String> _presets = [];
  int? _focusCh;

  // ------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------

  /// Attach to a booted client id (after `api.boot()`), then resync.
  Future<void> attach(String clientId) async {
    _clientId = clientId;
    try {
      await _api.resync(clientId);
    } catch (e) {
      // transports may not implement resync; anything else is visible.
      _noteTransportError(e);
    }
  }

  void setOnline(bool v) {
    if (_online == v) return;
    _online = v;
    _message = v ? 'online' : 'Not connected';
    notifyListeners();
  }

  bool get onlineFlag => _online;
  String get statusMessage => _message;

  /// Most recent poll/cmd transport failure, '' when none. Shown on the
  /// Link tab alongside engine `error` frames.
  String get transportError => _transportError;
  DateTime? get transportErrorAt => _transportErrorAt;

  /// Preferred error string for UI: transport failure wins (it explains
  /// silent probes), else the engine `error` frame. '' when healthy.
  String get visibleError =>
      _transportError.isNotEmpty ? _transportError : engine.error;

  /// Timestamp of [visibleError], null when no error is shown.
  DateTime? get visibleErrorAt =>
      _transportError.isNotEmpty ? _transportErrorAt : _engineErrorAt;

  void _noteTransportError(Object e) {
    final s = '$e';
    if (s == _transportError) return;
    _transportError = s;
    _transportErrorAt = DateTime.now();
    notifyListeners();
  }

  /// Clear both transport and engine error displays (engine re-sets its own
  /// on the next `error` frame).
  void clearErrors() {
    _transportError = '';
    _transportErrorAt = null;
    engine.error = '';
    notifyListeners();
  }

  void _trackEngineError() {
    if (engine.error != _lastEngineError) {
      _lastEngineError = engine.error;
      _engineErrorAt = engine.error.isEmpty ? null : DateTime.now();
    }
  }

  void _send(String json) {
    unawaited(_api.cmd(_clientId, json).then((_) {}, onError: (e) {
      _noteTransportError(e);
    }));
  }

  /// Raw Cmd JSON write for the Area A router (D-screen commands that have
  /// no dedicated AppStore method: scan/probe/connect/fx/scene/...).
  void sendRaw(String json) => _send(json);

  Future<void> sendCommand(String json) async {
    await _api.cmd(_clientId, json);
    await pollMeters();
  }

  /// Short UI leaves (as queued by ChannelWrites/channel.dart) translated
  /// to canonical desk catalogue ids (crates/core/src/params.rs, mirrored
  /// by web engineBridge.ts bridgeEq: band 0 = HPF; 1..6 = eqN_freq/gain/q;
  /// phase invert rides `polarity`).
  static final RegExp _eqShortLeaf = RegExp(r'^eq(\d+)_([fg])$');

  static String deskLeaf(String leaf) {
    if (leaf == 'invert') return 'polarity';
    if (leaf == 'eq_hpf_f') return 'hpf_freq';
    final m = _eqShortLeaf.firstMatch(leaf);
    if (m != null) return 'eq${m[1]}_${m[2] == 'f' ? 'freq' : 'gain'}';
    return leaf;
  }

  double _nowMs() => DateTime.now().microsecondsSinceEpoch / 1000.0;

  // ------------------------------------------------------------------
  // Shared 80 ms meter-loop target (all three screens call this).
  // ------------------------------------------------------------------
  @override
  Future<void> pollMeters() async {
    if (_polling) return;
    _polling = true;
    try {
      await _pollOnce();
    } finally {
      _polling = false;
    }
  }

  Future<void> _pollOnce() async {
    final now = _nowMs();
    final expired = ps.sweepPending(_maps.pending, now, ps.confirmTimeoutMs);
    if (expired.isNotEmpty) {
      for (final key in expired) {
        final split = ps.splitKey(key);
        if (split == null) continue;
        if (split.id == 'fader') {
          final confirmed = _confirmedFader[split.ch];
          if (confirmed != null) {
            for (final s in engine.strips) {
              if (s.ch == split.ch) s.fader = confirmed;
            }
          }
        } else if (split.id == 'mute') {
          final confirmed = _confirmedMute[split.ch];
          if (confirmed != null) {
            for (final s in engine.strips) {
              if (s.ch == split.ch) s.muted = confirmed;
            }
          }
        }
      }
      notifyListeners();
    }
    List<String> frames;
    try {
      frames = await _api.poll(_clientId, 512);
    } catch (e) {
      _noteTransportError(e);
      return; // never crash the meter loop on a transport hiccup
    }
    var dirty = false;
    for (final f in frames) {
      try {
        final e = jsonDecode(f);
        if (e is! Map<String, dynamic>) continue;
        final previousDriver = engine.driver;
        dirty = engine.applyFrame(e) || dirty;
        if (previousDriver != engine.driver) {
          _coalescer.clear();
          _maps.confirmed.clear();
          _maps.pending.clear();
          _confirmedFader.clear();
          _confirmedMute.clear();
          _bank = 'ch1';
          _busId = 'main';
          _eqBandOn.clear();
          _focusCh = null;
        }
        _confirmEcho(e);
      } catch (error) {
        _noteTransportError('Invalid engine frame: $error');
      }
    }
    if (_online != engine.connected) {
      _online = engine.connected;
      dirty = true;
    }
    _trackEngineError();
    // A successful poll carrying control traffic clears a stale transport
    // failure so a transient hiccup does not stick forever.
    if (_transportError.isNotEmpty && frames.isNotEmpty) {
      _transportError = '';
      _transportErrorAt = null;
      dirty = true;
    }
    if (_online && engine.strips.isNotEmpty) {
      _demo = engine.driver == 'sim';
      _deskName = engine.linkName.isNotEmpty ? engine.linkName : engine.driver;
    }
    _syncMeters(now);
    if (dirty) notifyListeners();
  }

  /// Binary fan-in for the future WS-transport path (SPEC 2a seq/t_ms).
  void applyBinaryFrame(Uint8List bytes) {
    final f = parseBinary(bytes);
    if (f == null) return;
    engine.applyBinary(f);
    _syncMeters(_nowMs());
  }

  void _confirmEcho(Map<String, dynamic> e) {
    switch (e['type']) {
      case 'param':
        final ch = (e['ch'] as num?)?.toInt();
        final field = e['field'] as String?;
        final v = e['value'];
        if (ch == null || field == null || v == null) return;
        final pv = v as ps.ParamValue;
        ps.confirmInto(_maps.pending, ps.pendingKey(ch, field), pv);
        ps.applyParams(_maps, ch, {field: pv});
        if (field == 'fader' && v is num) {
          _confirmedFader[ch] = v.toDouble();
        } else if ((field == 'mute' || field == 'muted') && v is bool) {
          _confirmedMute[ch] = v;
        }
        break;
      case 'params':
        final ch = (e['ch'] as num?)?.toInt();
        final values = e['values'];
        if (ch == null || values is! Map) return;
        final pv = <String, ps.ParamValue>{
          for (final kv in values.entries)
            if (kv.key is String && ps.validValue(kv.value))
              kv.key as String: kv.value as ps.ParamValue,
        };
        ps.applyParams(_maps, ch, pv);
        if (pv['fader'] is num) {
          _confirmedFader[ch] = (pv['fader'] as num).toDouble();
        }
        if (pv['mute'] is bool) _confirmedMute[ch] = pv['mute'] as bool;
        for (final kv in pv.entries) {
          ps.confirmInto(_maps.pending, ps.pendingKey(ch, kv.key), kv.value);
        }
        break;
      case 'state':
        final raw = (e['strips'] as List?) ?? const [];
        for (final s in raw) {
          if (s is! Map) continue;
          final ch = (s['ch'] as num?)?.toInt();
          if (ch == null) continue;
          final f = s['fader'], m = (s['muted'] ?? s['mute']);
          if (f is num) {
            _confirmedFader[ch] = f.toDouble();
            ps.confirmInto(
                _maps.pending, ps.pendingKey(ch, 'fader'), f.toDouble());
          }
          if (m is bool) {
            _confirmedMute[ch] = m;
            ps.confirmInto(_maps.pending, ps.pendingKey(ch, 'mute'), m);
          }
        }
        break;
    }
  }

  void _syncMeters(double nowMs) {
    while (_meters.length < engine.strips.length) {
      _meters.add(ValueNotifier(0));
    }
    while (_meters.length > engine.strips.length) {
      _meters.removeLast().dispose();
    }
    for (var i = 0; i < engine.strips.length; i++) {
      final lv =
          i < engine.meters.length ? engine.meters[i].clamp(0.0, 1.0) : 0.0;
      _meters[i].value = lv;
      final ch = engine.strips[i].ch;
      final tap = _taps.putIfAbsent(ch, () => BridgeTap());
      final db = _linToDb(lv);
      tap.db.value = db;
      final prevPeak = tap.peakDb.value;
      if (!prevPeak.isFinite || db > prevPeak) {
        tap.peakDb.value = db;
        _peakAtMs[ch] = nowMs;
      } else if (nowMs - (_peakAtMs[ch] ?? nowMs) > 1200) {
        // ~12 dB/s peak fall after a 1.2 s hold.
        tap.peakDb.value = prevPeak - 1.0;
      }
      tap.clip.value = db.isFinite && db >= -0.5;
    }
    final masterKey = _busId == 'main'
        ? 'main'
        : 'bus/${((int.tryParse(_busId.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';
    final masterIndex = engine.strips.indexWhere((s) => s.key == masterKey);
    _masterMeter.value = masterIndex >= 0 && masterIndex < engine.meters.length
        ? engine.meters[masterIndex].clamp(0.0, 1.0)
        : 0;
    // Open-channel feeds.
    final ci = engine.strips.indexWhere((s) => s.ch == _channel);
    final lv = ci >= 0 && ci < engine.meters.length
        ? engine.meters[ci].clamp(0.0, 1.0)
        : 0.0;
    _levelDb.value = _linToDb(lv);
    final bins = engine.spectrum[_channel];
    if (bins != null) _spectrum.value = List.of(bins);
  }

  // ------------------------------------------------------------------
  // MixerStore
  // ------------------------------------------------------------------
  @override
  List<Strip> get strips {
    final ranges = _bankRanges;
    final range = ranges[_bank];
    if (range == null) return engine.strips;
    return engine.strips
        .where((s) => s.ch >= range.$1 && s.ch < range.$2)
        .toList();
  }

  Map<String, (int, int)> get _bankRanges => {
        for (final f in engine.topology)
          for (var offset = 0;
              offset < (f['count'] as num).toInt();
              offset += 16)
            f['key'] == 'ch'
                ? 'ch${offset ~/ 16 + 1}'
                : "${f['key']}:${(f['first'] as num).toInt() + offset}": (
              (f['first'] as num).toInt() + offset,
              (f['first'] as num).toInt() +
                  math.min(offset + 16, (f['count'] as num).toInt())
            ),
      };

  @override
  double panOf(int ch) {
    final v = ps.optimisticValue(_maps, ch, 'pan');
    return v is num ? (v.toDouble() / 100).clamp(-1.0, 1.0) : 0.0;
  }

  @override
  bool soloOf(int ch) => engine.solos.contains(ch);

  @override
  bool get hasSolo => engine.solos.isNotEmpty;

  @override
  int get soloCount => engine.solos.length;

  @override
  ValueNotifier<double> meterOf(int ch) {
    final i = engine.strips.indexWhere((s) => s.ch == ch);
    if (i < 0) return _masterMeter;
    return _meters[i];
  }

  @override
  ValueNotifier<double> get masterMeter => _masterMeter;

  @override
  double get masterFader => _busId == 'main'
      ? engine.mainFader ?? _masterFader
      : engine.busFaders[int.tryParse(_busId.replaceFirst('bus', ''))] ?? 0;

  @override
  bool get masterMuted => _busId == 'main'
      ? engine.mainMuted ?? _masterMuted
      : engine.busMuted[int.tryParse(_busId.replaceFirst('bus', ''))] ?? false;

  @override
  String get masterName => _busId == 'main'
      ? 'MAIN'
      : 'BUS ${(int.tryParse(_busId.replaceFirst('bus', '')) ?? 0) + 1}';

  @override
  List<MixerBank> get banks => [
        for (final entry in _bankRanges.entries)
          MixerBank(
              key: entry.key,
              label: entry.key.startsWith('ch')
                  ? 'Ch ${entry.value.$1 + 1}–${entry.value.$2}'
                  : engine.topology.firstWhere((f) =>
                      (f['first'] as num).toInt() <= entry.value.$1 &&
                      (f['first'] as num).toInt() +
                              (f['count'] as num).toInt() >
                          entry.value.$1)['label'] as String,
              color: U4.cyan,
              text: U4.page),
      ];

  @override
  String get bank => _bank;

  @override
  List<MixerBusRow> get busRows => [
        MixerBusRow(
          id: 'main',
          name: 'MAIN',
          sub: 'LR master',
          muted: masterMuted,
          active: _busId == 'main',
          meter: _masterMeter,
        ),
        for (final b in engine.busFaders.keys.toList()..sort())
          MixerBusRow(
            id: 'bus$b',
            name: 'BUS ${b + 1}',
            sub: engine.busMuted[b] == true ? 'muted' : 'open',
            muted: engine.busMuted[b] ?? false,
            active: _busId == 'bus$b',
            meter: _busMeters.putIfAbsent('bus$b', () => ValueNotifier(0)),
          ),
      ];

  @override
  void setFader(int ch, double v) {
    final nv = v.clamp(0.0, 1.0);
    for (final s in engine.strips) {
      if (s.ch == ch) s.fader = nv;
    }
    ps.noteOptimistic(_maps, ch, 'fader', nv, _nowMs());
    _coalescer.offer('fader:$ch', () => _send(Cmds.fader(ch, nv)));
    notifyListeners();
  }

  @override
  void setMute(int ch, bool muted) {
    for (final s in engine.strips) {
      if (s.ch == ch) s.muted = muted;
    }
    ps.noteOptimistic(_maps, ch, 'mute', muted, _nowMs());
    _coalescer.offer('mute:$ch', () => _send(Cmds.mute(ch, muted)));
    notifyListeners();
  }

  @override
  void toggleSolo(int ch) {
    final on = !engine.solos.contains(ch);
    if (on) {
      engine.solos.add(ch);
    } else {
      engine.solos.remove(ch);
    }
    _coalescer.offer('solo:$ch', () => _send(Cmds.solo(ch, on)));
    notifyListeners();
  }

  @override
  void clearSolos() {
    engine.solos.clear();
    _send(Cmds.clearSolo());
    notifyListeners();
  }

  @override
  void setPan(int ch, double v) {
    final nv = v.clamp(-1.0, 1.0);
    ps.noteOptimistic(_maps, ch, 'pan', nv * 100, _nowMs());
    _coalescer.offer('pan:$ch', () => _send(Cmds.param(ch, 'pan', nv * 100)));
    notifyListeners();
  }

  @override
  void setMasterFader(double v) {
    _masterFader = v.clamp(0.0, 1.0);
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busFaders[bus] = _masterFader;
      _coalescer.offer('bus_fader:$bus', () => _send(Cmds.busFader(bus, v)));
      notifyListeners();
      return;
    }
    engine.mainFader = _masterFader;
    _coalescer.offer('main_fader', () => _send(Cmds.mainFader(_masterFader)));
    notifyListeners();
  }

  @override
  void setMasterMuted(bool muted) {
    _masterMuted = muted;
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busMuted[bus] = muted;
      _coalescer.offer('bus_mute:$bus', () => _send(Cmds.busMute(bus, muted)));
      notifyListeners();
      return;
    }
    engine.mainMuted = muted;
    _coalescer.offer('main_mute', () => _send(Cmds.mainMute(muted)));
    notifyListeners();
  }

  @override
  void selectBus(String id) {
    _busId = id;
    notifyListeners();
  }

  @override
  void setBank(String key) {
    _bank = key;
    notifyListeners();
  }

  // ------------------------------------------------------------------
  // MetersStore
  // ------------------------------------------------------------------
  @override
  bool get online => _online;

  @override
  bool get demo => _demo;

  @override
  String? get deskName => _deskName;

  @override
  List<BridgeGroup> get groups {
    if (!_online || engine.strips.isEmpty) {
      return const [
        BridgeGroup(
            key: 'in',
            title: 'Inputs',
            ladders: [],
            collapsedReason: 'offline'),
      ];
    }
    return [
      BridgeGroup(
        key: 'in',
        title: 'Inputs',
        ladders: [
          for (final s in engine.strips)
            BridgeLadder(
                ch: s.ch,
                label: s.name.isEmpty ? null : s.name,
                sub: '${s.ch + 1}'),
        ],
      ),
    ];
  }

  @override
  BridgeTap tapOf(int ch) => _taps.putIfAbsent(ch, () => BridgeTap());

  @override
  void setSpectrumTarget(int? ch) => _send(Cmds.spectrumTarget(ch));

  // ------------------------------------------------------------------
  // ChannelStore
  // ------------------------------------------------------------------
  /// Current open channel index (shell prev/next).
  int get channelIndex => _channel;

  /// Select the open channel (called by the shell on navigation).
  void openChannel(int ch) {
    if (_channel == ch) return;
    _channel = ch;
    cmdFocus();
    notifyListeners();
  }

  @override
  String get stripKey => 'ch/${(_channel + 1).toString().padLeft(2, '0')}';

  @override
  String get displayName {
    for (final s in engine.strips) {
      if (s.ch == _channel && s.name.isNotEmpty) return s.name;
    }
    return 'CH${(_channel + 1).toString().padLeft(2, '0')}';
  }

  @override
  double? paramNum(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, leaf);
    return v is num ? v.toDouble() : null;
  }

  @override
  bool? paramBool(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, leaf);
    return v is bool ? v : null;
  }

  static const _coreLeaves = {
    'fader',
    'mute',
    'pan',
    'gain',
    'trim',
    'phantom',
    'polarity',
  };

  @override
  bool supports(String leaf) {
    final desk = deskLeaf(leaf);
    if (_coreLeaves.contains(desk)) return true;
    if (!engine.capsReady) return false;
    return engine.supportedParams.contains(desk);
  }

  @override
  void cmdParam(String leaf, Object value) {
    final desk = deskLeaf(leaf);
    final coerced = value;
    _maps.confirmed.putIfAbsent(_channel, () => {});
    ps.noteOptimistic(_maps, _channel, desk, coerced, _nowMs());
    _coalescer.offer('param:$_channel:$desk',
        () => _send(Cmds.param(_channel, desk, coerced)));
    notifyListeners();
  }

  @override
  void cmdFocus() {
    _focusCh = _channel;
    _send(Cmds.focus([_channel]));
  }

  @override
  void cmdLock() => _send(Cmds.lock(_channel));

  @override
  void cmdUnlock() => _send(Cmds.unlock(_channel));

  @override
  void cmdSubgroupAssign(int g, bool on) {
    engine.subgroupAssign['$_channel:$g'] = on;
    _send(Cmds.subgroupAssign(_channel, g, on));
    notifyListeners();
  }

  @override
  void cmdMuteGroupAssign(int g, bool on) {
    engine.muteGroupAssign['$_channel:$g'] = on;
    _send(Cmds.muteGroupAssign(_channel, g, on));
    notifyListeners();
  }

  @override
  ValueNotifier<double> get levelDb => _levelDb;

  @override
  ValueNotifier<List<double>> get spectrum => _spectrum;

  @override
  double? grDb(DynKind kind) {
    final m = kind == DynKind.gate
        ? engine.grGate[_channel]
        : engine.grComp[_channel];
    return m != null && m.isFinite ? m : null;
  }

  @override
  ChannelPreampVm get preamp => ChannelPreampVm(
        gain: paramNum('gain') ?? 30,
        trim: paramNum('trim') ?? 0,
        phantom: paramBool('phantom') ?? false,
        invert: paramBool('polarity') ?? false,
      );

  @override
  ChannelSourceVm get source =>
      ChannelSourceVm(srcName: displayName, srcGroup: 'local');

  @override
  ChannelEqVm get eq => ChannelEqVm(
        on: paramBool('eq_on') ?? true,
        hpfF: paramNum('hpf_freq') ?? 80,
        hpfOn: paramBool('hpf_on') ?? false,
        hpfSlope: paramNum('hpf_slope')?.toInt() ?? 12,
        selectedBand: _eqBand,
        bands: [
          for (var b = 1; b <= 4; b++)
            EqBandVm(
              band: b,
              name: const ['Low', 'Lo-Mid', 'Hi-Mid', 'High'][b - 1],
              f: paramNum('eq${b}_freq') ??
                  [80.0, 400.0, 1200.0, 6000.0][b - 1],
              g: paramNum('eq${b}_gain') ?? 0,
              q: paramNum('eq${b}_q') ?? 1,
              on: _eqBandOn[b] ?? paramBool('eq${b}_on') ?? true,
            ),
        ],
      );

  ChannelDynVm _dyn(DynKind kind) {
    final p = kind == DynKind.gate ? 'gate' : 'comp';
    return ChannelDynVm(
      kind: kind,
      on: paramBool('${p}_on') ?? true,
      thr: paramNum('${p}_thr') ?? (kind == DynKind.gate ? -50 : -20),
      range: paramNum('${p}_range') ?? 40,
      att: paramNum('${p}_att') ?? (kind == DynKind.gate ? 0.5 : 10),
      hold: paramNum('${p}_hold') ?? 50,
      rel: paramNum('${p}_rel') ?? (kind == DynKind.gate ? 150 : 120),
      ratio: paramNum('${p}_ratio') ?? 3,
      knee: paramNum('${p}_knee') ?? 3,
      makeup: paramNum('${p}_makeup') ?? 0,
    );
  }

  @override
  ChannelDynVm get gate => _dyn(DynKind.gate);

  @override
  ChannelDynVm get comp => _dyn(DynKind.comp);

  @override
  List<ChannelSendVm> get sends {
    final buses = engine.busFaders.keys.toList()..sort();
    if (buses.isEmpty) return const [];
    return [
      for (final b in buses)
        ChannelSendVm(
          slot: b + 1,
          destName: 'Mix ${b + 1}',
          level: 0.75,
          muted: !(paramBool('send${b + 1}_on') ?? true),
        ),
    ];
  }

  @override
  int get subgroupCount => engine.subgroupFaders.isEmpty
      ? 0
      : engine.subgroupFaders.keys.reduce(math.max) + 1;

  @override
  int get muteGroupCount => engine.muteGroups.isEmpty
      ? 0
      : engine.muteGroups.keys.reduce(math.max) + 1;

  @override
  bool? subgroupAssigned(int g) => engine.subgroupAssign['$_channel:$g'];

  @override
  bool? muteGroupAssigned(int g) => engine.muteGroupAssign['$_channel:$g'];

  @override
  List<String> get presets => List.unmodifiable(_presets);

  @override
  bool get locked => engine.locks.containsKey(_channel);

  @override
  bool get focused => _focusCh == _channel;

  @override
  ChannelTab get tab => _tab;

  @override
  void setTab(ChannelTab tab) {
    _tab = tab;
    notifyListeners();
  }

  @override
  void selectEqBand(int band) {
    _eqBand = band;
    notifyListeners();
  }

  @override
  void setEqParam(int band, String param, double value) =>
      cmdParam('eq${band}_$param', value);

  @override
  void setEqOn(bool on) => cmdParam('eq_on', on);

  @override
  void setEqBandOn(int band, bool on) {
    if (band == 0) {
      cmdParam('hpf_on', on);
      return;
    }
    // No per-band desk leaf: fixture-local bypass (web parity).
    _eqBandOn[band] = on;
    notifyListeners();
  }

  @override
  void setHpfSlope(int slope) => cmdParam('hpf_slope', slope);

  @override
  void resetEq() {
    for (var b = 1; b <= 4; b++) {
      cmdParam('eq${b}_g', 0.0);
    }
    notifyListeners();
  }

  @override
  void setDynParam(DynKind kind, String prop, double value) =>
      cmdParam('${kind.name}_$prop', value);

  @override
  void setDynOn(DynKind kind, bool on) => cmdParam('${kind.name}_on', on);

  @override
  void resetDyn(DynKind kind) {
    final d = ChannelDynVm.resetDefault(kind);
    cmdParam('${kind.name}_thr', d.thr);
    cmdParam('${kind.name}_ratio', d.ratio);
    cmdParam('${kind.name}_att', d.att);
    cmdParam('${kind.name}_rel', d.rel);
  }

  @override
  void setSendLevel(int slot, double value) =>
      cmdParam('send${slot}_level', value);

  @override
  void setSendTap(int slot, int tap) => cmdParam('send${slot}_tap', tap);

  @override
  void setSendMute(int slot, bool muted) => cmdParam('send${slot}_on', !muted);

  @override
  void togglePhantom() => cmdParam('phantom', !(paramBool('phantom') ?? false));

  @override
  void toggleInvert() =>
      cmdParam('polarity', !(paramBool('polarity') ?? false));

  @override
  void setPreamp(String leaf, double value) => cmdParam(leaf, value);

  @override
  void recallPreset(String name) {
    // Fixture-local until the preset library (Area B/C) lands.
    notifyListeners();
  }

  @override
  void storePreset(String name) {
    if (name.isNotEmpty && !_presets.contains(name)) {
      _presets.add(name);
      notifyListeners();
    }
  }

  @override
  void dispose() {
    _coalescer.clear();
    for (final m in _meters) {
      m.dispose();
    }
    _masterMeter.dispose();
    for (final t in _taps.values) {
      t.dispose();
    }
    for (final b in _busMeters.values) {
      b.dispose();
    }
    _levelDb.dispose();
    _spectrum.dispose();
    super.dispose();
  }
}
