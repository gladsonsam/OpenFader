// Minimal central store: the live engine binding for the app, reduced to
// what the app renders: connection state,
// strips, meters and scenes over [EngineApi], with optimistic-write+confirm
// (see `lib/core/store.dart`) and 60 Hz
// coalesced sends (see `lib/core/coalescer.dart`).
//
// One [AppStore] implements the [MixerStore], [MetersStore] and
// [ChannelStore] contracts (constructor injection — no global singleton,
// so this never fights the screen workers). Freshness comes from the
// single shell-owned 80 ms ticker (AppShell calls [pollMeters]); screens
// own no timer and only listen to already-updating notifiers. Per-strip
// control state (fader/mute/pan) rides stable [ValueNotifier]s updated in
// place and never rebuilds sibling widgets, while structural state
// (strips/bank/bus/connection, solo banner) notifies through
// [ChangeNotifier]. Meter levels ride the same stable-notifier pattern.
//
// Mapping gaps (honest, not faked):
// - Channel writes use canonical desk catalogue leaves
//   (crates/core/src/params.rs: eqN_freq/gain/q, hpf_freq/hpf_on/hpf_slope,
//   gate_/comp_*, sendN_level/tap/on, polarity — never invert/eqN_f/eqN_g/
//   sendN_mute). Short UI leaves are translated in [AppStore.deskLeaf], the
//   single choke point covering both direct cmdParam calls and the
//   ChannelWrites queue. Neither hardware driver has per-band EQ bypass.
// - EQ/dyn view fallbacks keep getter interfaces stable; hardware edits
//   remain disabled until the corresponding desk value is received.
// - Bus rows render the MAIN bus (+ known engine buses); the bus popup
//   picks the master bus shown, it does not re-patch routing.
// - Binary meter/GR/tap frames ride the hot path ([EngineApi.pollBinary]
//   + [applyBinaryFrame]); [EngineApi.poll] stays the JSON fallback for
//   control frames — and for meters when binary is unavailable.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';

import '../core/channel_views.dart';
import '../core/coalescer.dart';
import '../core/fader_law.dart' show lawForDriver;
import '../core/store.dart' as ps;
import '../engine.dart';
import '../palette.dart';
import '../u4law.dart';
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
  // Binary meter path armed. A stale native library without `pollBinary`
  // latches this off for the session (JSON meters from `poll` keep
  // rendering); attach() re-arms so an upgraded library is picked up.
  bool _binaryMeters = true;
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

  // --- per-strip control notifiers (stable, updated in place) ---
  // Hot-path writes (fader drags, mute taps, pan drags) update these in
  // place and NEVER call notifyListeners(), so one strip rebuilds instead
  // of the whole shell + all strips. Global notifyListeners() is reserved
  // for structural changes (strips add/remove, bank/bus select, connection).
  // Solo keeps its notifier AND a global notify (infrequent taps; the page
  // solo banner reads hasSolo/soloCount off the store).
  final Map<int, ValueNotifier<double>> _faderNotes = {};
  final Map<int, ValueNotifier<bool>> _muteNotes = {};
  final Map<int, ValueNotifier<double>> _panNotes = {};
  final Map<int, ValueNotifier<bool>> _soloNotes = {};
  final Map<int, ValueNotifier<int>> _controlReadyNotes = {};

  // Solo is a dedicated protocol event, absent from the param catalogue.
  // A real solo echo establishes availability; caps alone never enables it.
  bool canEditControl(int ch, String field) =>
      engine.driver == 'sim' ||
      (engine.connected &&
          engine.capsReady &&
          (engine.supportedParams.contains(field) || field == 'solo') &&
          engine.valueKnown(ch, field));

  int _controlReady(int ch) => ch < 0
      ? ((masterControlKnown('fader') ? 1 : 0) |
          (masterControlKnown('mute') ? 2 : 0))
      : (canEditControl(ch, 'fader') ? 1 : 0) |
          (canEditControl(ch, 'mute') ? 2 : 0) |
          (canEditControl(ch, 'pan') ? 4 : 0) |
          (canEditControl(ch, 'solo') ? 8 : 0);

  ValueNotifier<int> controlReadyNote(int ch) => _controlReadyNotes.putIfAbsent(
      ch, () => ValueNotifier(_controlReady(ch)));

  bool masterControlKnown(String field) =>
      engine.driver == 'sim' ||
      (engine.connected &&
          (_busId == 'main'
              ? (field == 'fader'
                  ? engine.mainFader != null
                  : engine.mainMuted != null)
              : (field == 'fader'
                  ? engine.busFaders
                      .containsKey(int.tryParse(_busId.replaceFirst('bus', '')))
                  : engine.busMuted.containsKey(
                      int.tryParse(_busId.replaceFirst('bus', ''))))));
  final ValueNotifier<double> _masterFaderNote = ValueNotifier(0.75);
  final ValueNotifier<bool> _masterMutedNote = ValueNotifier(false);

  // Raw engineering values, keyed by channel + canonical send field.
  final Map<(int, String), ValueNotifier<double?>> _sendParamNotes = {};

  // --- channel page state ---
  int _channel = 0;
  ChannelTab _tab = ChannelTab.input;
  int _eqBand = 1;

  final ValueNotifier<double> _levelDb = ValueNotifier(double.negativeInfinity);
  final ValueNotifier<List<double>> _spectrum = ValueNotifier(const []);
  final ValueNotifier<AudioSpectrumFrame?> _audioSpectrum = ValueNotifier(null);
  final List<String> _presets = [];
  int? _focusCh;

  // --- derived-list cache (no per-build allocations) ---
  // strips/banks/busRows/groups/sends (+_bankRanges) rebuild only after a
  // control-state mutation nulls them via [_invalidateDerived]. Cached lists
  // are unmodifiable snapshots: callers only read them (verified: no call
  // site sorts/adds/removes on these getters), so sharing one instance across
  // builds is safe and fail-fast if a future caller tries to mutate.
  Map<String, (int, int)>? _bankRangesCache;
  List<Strip>? _stripsCache;
  List<MixerBank>? _banksCache;
  List<MixerBusRow>? _busRowsCache;
  List<BridgeGroup>? _groupsCache;
  List<ChannelSendVm>? _sendsCache;

  /// Drop every derived-list cache. Call from any mutator that can affect
  /// what the cached getters return (see call sites).
  void _invalidateDerived() {
    _bankRangesCache = null;
    _stripsCache = null;
    _banksCache = null;
    _busRowsCache = null;
    _groupsCache = null;
    _sendsCache = null;
  }

  // ------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------

  /// Attach to a booted client id (after `api.boot()`), then resync.
  Future<void> attach(String clientId) async {
    _clientId = clientId;
    _binaryMeters = true; // re-arm: a fresh attach may carry a new library
    try {
      await _api.resync(clientId);
    } catch (e) {
      // transports may not implement resync; anything else is visible.
      _noteTransportError(e);
    }
    _invalidateDerived();
  }

  void setOnline(bool v) {
    if (_online == v) return;
    _online = v;
    _message = v ? 'online' : 'Not connected';
    _invalidateDerived(); // groups renders an offline placeholder
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
  void sendRaw(String json) {
    _prepareConnectionCommand(json);
    _send(json);
  }

  void _clearSessionValues() {
    _coalescer.clear();
    _maps.confirmed.clear();
    _maps.pending.clear();
    _confirmedFader.clear();
    _confirmedMute.clear();
    _bank = 'ch1';
    _busId = 'main';
    _focusCh = null;
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _invalidateDerived();
  }

  void _prepareConnectionCommand(String json) {
    final command = jsonDecode(json);
    if (command is! Map ||
        (command['cmd'] != 'connect' && command['cmd'] != 'disconnect')) {
      return;
    }
    engine.resetSession();
    _clearSessionValues();
    _syncControlNotes();
    notifyListeners();
  }

  Future<void> sendCommand(String json) async {
    _prepareConnectionCommand(json);
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
  // Single shared 80 ms ticker target (driven ONLY by the AppShell loop).
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
      // Confirm-timeout revert: caches + strip notifiers stay in sync,
      // then one structural notify (rare path, not the drag hot path).
      _invalidateDerived();
      _syncControlNotes();
      notifyListeners();
    }
    List<String> frames;
    // Hot meter path (20 Hz): compact binary frames fan straight into
    // the meter notifiers with no JSON parse/alloc. Non-meter events seen
    // here are set aside server-side and come back through poll() below,
    // so the JSON fallback stays lossless. A missing `pollBinary` (stale
    // native library) latches binary off; JSON meters keep rendering.
    if (_binaryMeters) {
      try {
        final blob = await _api.pollBinary(_clientId, 512);
        for (final frame in splitBinaryBatch(blob)) {
          applyBinaryFrame(frame);
        }
      } catch (_) {
        _binaryMeters = false;
      }
    }
    try {
      frames = await _api.poll(_clientId, 512);
    } catch (e) {
      _noteTransportError(e);
      _syncSpectrum();
      return; // never crash the meter loop on a transport hiccup
    }
    var dirty = false;
    var controlsTouched = false;
    for (final f in frames) {
      try {
        final e = jsonDecode(f);
        if (e is! Map<String, dynamic>) continue;
        final previousSession = engine.session;
        final controlOnly = _controlOnlyFrame(e);
        final frameDirty = engine.applyFrame(e);
        if (previousSession != engine.session) {
          _clearSessionValues();
          dirty = true;
        }
        _confirmEcho(e);
        if (controlOnly) {
          // Control echo (strip/send/main/bus): the strip
          // notifiers pick it up below, no global rebuild.
          controlsTouched = true;
        } else if (frameDirty) {
          dirty = true;
        }
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
    // Push any control traffic (local optimistic writes already did; echoes
    // land here) into the stable per-strip notifiers. ValueNotifiers only
    // wake their own strip; the shell/page ListenableBuilders stay quiet.
    if (dirty) _invalidateDerived();
    if (controlsTouched || dirty) _syncControlNotes();
    if (dirty) notifyListeners();
  }

  /// Binary fan-in for the meter hot path (`pollBinary` frames) and the
  /// future WS-transport path (SPEC 2a seq/t_ms). Version-mismatched or
  /// short frames decode to null and are skipped — JSON meters from the
  /// fallback poll keep the ladders alive.
  void applyBinaryFrame(Uint8List bytes) {
    final f = parseBinary(bytes);
    if (f == null) return;
    engine.applyBinary(f);
    _invalidateDerived();
    _syncMeters(_nowMs());
  }

  /// True for engine frames that only move per-strip/master control state
  /// (fader/mute/pan/send level/send pan/main/bus). These ride notifiers via
  /// [_syncControlNotes] and must not set the global `dirty` rebuild flag.
  /// Everything else (state/caps/link/scenes/..., plus param frames carrying
  /// name/param-plane fields, plus solo which feeds the page solo banner)
  /// stays structural and keeps the global notify.
  static const _stripControlFields = {'fader', 'mute', 'muted', 'pan'};

  static final _sendHotField = RegExp(r'^send[1-9]\d*_(pan|level)$');

  bool _isHotControlField(String field) =>
      _stripControlFields.contains(field) || _sendHotField.hasMatch(field);

  bool _controlOnlyFrame(Map<String, dynamic> e) {
    switch (e['type']) {
      case 'param':
        final field = e['field'] as String?;
        return field != null && _isHotControlField(field);
      case 'params':
        final values = e['values'];
        if (values is! Map || values.isEmpty) return false;
        return values.keys.every((k) => k is String && _isHotControlField(k));
      case 'main':
      case 'bus':
        return true;
      default:
        return false;
    }
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
        _invalidateDerived();
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
        final mute = pv['mute'] ?? pv['muted'];
        if (mute is bool) _confirmedMute[ch] = mute;
        for (final kv in pv.entries) {
          ps.confirmInto(_maps.pending, ps.pendingKey(ch, kv.key), kv.value);
        }
        _invalidateDerived();
        break;
      case 'state':
        final raw = (e['strips'] as List?) ?? const [];
        for (final s in raw) {
          if (s is! Map) continue;
          final ch = (s['ch'] as num?)?.toInt();
          if (ch == null) continue;
          final f = s['fader'], m = (s['muted'] ?? s['mute']);
          ps.applyParams(_maps, ch, {
            if (f is num) 'fader': f,
            if (m is bool) 'mute': m,
          });
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
        _invalidateDerived();
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
    _syncSpectrum();
  }

  void _syncSpectrum() {
    // Legacy synthetic bins remain exclusively on the demo path.
    final bins = engine.driver == 'sim' ? engine.spectrum[_channel] : null;
    _spectrum.value =
        engine.connected && bins != null ? List.of(bins) : const [];
    final frame = engine.audioSpectrum;
    if (frame != null && !frame.isFresh) engine.clearAudioSpectrum();
    _audioSpectrum.value = engine.connected &&
            frame != null &&
            frame.isFresh &&
            (frame.ch == null || frame.ch == _channel)
        ? frame
        : null;
  }

  // ------------------------------------------------------------------
  // MixerStore
  // ------------------------------------------------------------------
  @override
  List<Strip> get strips {
    final cached = _stripsCache;
    if (cached != null) return cached;
    final range = _bankRanges[_bank];
    final built = range == null
        ? List<Strip>.unmodifiable(engine.strips)
        : List<Strip>.unmodifiable(
            engine.strips.where((s) => s.ch >= range.$1 && s.ch < range.$2));
    _stripsCache = built;
    return built;
  }

  Map<String, (int, int)> get _bankRanges {
    final cached = _bankRangesCache;
    if (cached != null) return cached;
    final Map<String, (int, int)> built = {
      for (final f in engine.topology)
        for (var offset = 0; offset < (f['count'] as num).toInt(); offset += 16)
          f['key'] == 'ch'
              ? 'ch${offset ~/ 16 + 1}'
              : "${f['key']}:${(f['first'] as num).toInt() + offset}": (
            (f['first'] as num).toInt() + offset,
            (f['first'] as num).toInt() +
                math.min(offset + 16, (f['count'] as num).toInt())
          ),
    };
    _bankRangesCache = built;
    return built;
  }

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

  // ------------------------------------------------------------------
  // Per-strip control notifiers: stable ValueNotifiers updated in place.
  // Strips listen to their own ch only; the hot drag path never calls
  // notifyListeners() so the shell + sibling strips are not rebuilt.
  // ------------------------------------------------------------------
  // Engine state, confirmations and pending edits stay in protocol units.
  // Only UI exposure/writes cross this boundary. SIM already uses the UI
  // law; lawForDriver('sim') would incorrectly select the X-Air fallback.
  double _protocolFaderToUi(double value) => engine.driver == 'sim'
      ? value
      : dbToPos(lawForDriver(engine.driver).toDb(value));

  double _uiFaderToProtocol(double value) => engine.driver == 'sim'
      ? value
      : lawForDriver(engine.driver).fromDb(posToDb(value));

  void _ensureStripNotes(int ch) {
    _faderNotes.putIfAbsent(ch, () {
      double v = 0;
      for (final s in engine.strips) {
        if (s.ch == ch) v = s.fader;
      }
      final opt = ps.optimisticValue(_maps, ch, 'fader');
      if (opt is num) v = opt.toDouble().clamp(0.0, 1.0);
      return ValueNotifier(_protocolFaderToUi(v));
    });
    _muteNotes.putIfAbsent(ch, () {
      bool m = false;
      for (final s in engine.strips) {
        if (s.ch == ch) m = s.muted;
      }
      final opt = ps.optimisticValue(_maps, ch, 'mute');
      if (opt is bool) m = opt;
      return ValueNotifier(m);
    });
    _panNotes.putIfAbsent(ch, () => ValueNotifier(panOf(ch)));
    _soloNotes.putIfAbsent(ch, () => ValueNotifier(engine.solos.contains(ch)));
  }

  @override
  ValueNotifier<double> faderOf(int ch) {
    _ensureStripNotes(ch);
    return _faderNotes[ch]!;
  }

  @override
  ValueNotifier<bool> muteOf(int ch) {
    _ensureStripNotes(ch);
    return _muteNotes[ch]!;
  }

  @override
  ValueNotifier<double> panNoteOf(int ch) {
    _ensureStripNotes(ch);
    return _panNotes[ch]!;
  }

  @override
  ValueNotifier<bool> soloNoteOf(int ch) {
    _ensureStripNotes(ch);
    return _soloNotes[ch]!;
  }

  @override
  ValueNotifier<double> get masterFaderNote => _masterFaderNote;

  @override
  ValueNotifier<bool> get masterMutedNote => _masterMutedNote;

  void _pushStripNote(int ch) {
    controlReadyNote(ch).value = _controlReady(ch);
    _ensureStripNotes(ch);
    for (final s in engine.strips) {
      if (s.ch != ch) continue;
      final f = _faderNotes[ch]!;
      final uiFader = _protocolFaderToUi(s.fader);
      if (f.value != uiFader) f.value = uiFader;
      final m = _muteNotes[ch]!;
      if (m.value != s.muted) m.value = s.muted;
    }
    final p = _panNotes[ch]!;
    final pv = panOf(ch);
    if (p.value != pv) p.value = pv;
    final so = _soloNotes[ch]!;
    final sv = engine.solos.contains(ch);
    if (so.value != sv) so.value = sv;
  }

  double? _sendParamValue(int ch, String field) {
    final value = ps.optimisticValue(_maps, ch, field);
    return value is num ? value.toDouble() : null;
  }

  @override
  ValueNotifier<double?> sendParamNote(int slot, String param) {
    assert(param == 'pan' || param == 'level');
    final key = (_channel, 'send${slot}_$param');
    return _sendParamNotes.putIfAbsent(
        key, () => ValueNotifier(_sendParamValue(key.$1, key.$2)));
  }

  void _syncSendParamNotes() {
    for (final entry in _sendParamNotes.entries) {
      entry.value.value = _sendParamValue(entry.key.$1, entry.key.$2);
    }
  }

  void _syncControlNotes() {
    _syncSendParamNotes();
    for (final entry in _controlReadyNotes.entries) {
      entry.value.value = _controlReady(entry.key);
    }
    for (final s in engine.strips) {
      _pushStripNote(s.ch);
    }
    final mf = masterFader;
    if (_masterFaderNote.value != mf) _masterFaderNote.value = mf;
    final mm = masterMuted;
    if (_masterMutedNote.value != mm) _masterMutedNote.value = mm;
  }

  @override
  double get masterFader => _protocolFaderToUi(_busId == 'main'
      ? engine.mainFader ?? _masterFader
      : engine.busFaders[int.tryParse(_busId.replaceFirst('bus', ''))] ?? 0);

  @override
  bool get masterMuted => _busId == 'main'
      ? engine.mainMuted ?? _masterMuted
      : engine.busMuted[int.tryParse(_busId.replaceFirst('bus', ''))] ?? false;

  @override
  String get masterName => _busId == 'main'
      ? 'MAIN'
      : 'BUS ${(int.tryParse(_busId.replaceFirst('bus', '')) ?? 0) + 1}';

  @override
  List<MixerBank> get banks {
    final cached = _banksCache;
    if (cached != null) return cached;
    final built = List<MixerBank>.unmodifiable([
      for (final entry in _bankRanges.entries)
        MixerBank(
            key: entry.key,
            label: entry.key.startsWith('ch')
                ? 'Ch ${entry.value.$1 + 1}–${entry.value.$2}'
                : engine.topology.firstWhere((f) =>
                    (f['first'] as num).toInt() <= entry.value.$1 &&
                    (f['first'] as num).toInt() + (f['count'] as num).toInt() >
                        entry.value.$1)['label'] as String,
            color: const ConsolePalette().lime,
            text: const ConsolePalette().onAccent),
    ]);
    _banksCache = built;
    return built;
  }

  @override
  String get bank => _bank;

  @override
  List<MixerBusRow> get busRows {
    final cached = _busRowsCache;
    if (cached != null) return cached;
    final built = List<MixerBusRow>.unmodifiable([
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
    ]);
    _busRowsCache = built;
    return built;
  }

  @override
  void setFader(int ch, double v) {
    if (!canEditControl(ch, 'fader')) return;
    final nv = v.clamp(0.0, 1.0);
    final protocolValue = _uiFaderToProtocol(nv);
    for (final s in engine.strips) {
      if (s.ch == ch) s.fader = protocolValue;
    }
    ps.noteOptimistic(_maps, ch, 'fader', protocolValue, _nowMs());
    _ensureStripNotes(ch);
    final n = _faderNotes[ch]!;
    if (n.value != nv) n.value = nv;
    _coalescer.offer('fader:$ch', () => _send(Cmds.fader(ch, protocolValue)));
    // Hot path: no notifyListeners(). The dragged strip rebuilds from
    // faderOf(ch) alone; shell + siblings stay quiet. Caches invalidated
    // so the next structural rebuild sees fresh values.
    _invalidateDerived();
  }

  @override
  void setMute(int ch, bool muted) {
    if (!canEditControl(ch, 'mute')) return;
    for (final s in engine.strips) {
      if (s.ch == ch) s.muted = muted;
    }
    ps.noteOptimistic(_maps, ch, 'mute', muted, _nowMs());
    _ensureStripNotes(ch);
    final n = _muteNotes[ch]!;
    if (n.value != muted) n.value = muted;
    _coalescer.offer('mute:$ch', () => _send(Cmds.mute(ch, muted)));
    // Hot path: no notifyListeners(). The tapped strip rebuilds from
    // muteOf(ch) alone. Caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void toggleSolo(int ch) {
    if (!canEditControl(ch, 'solo')) return;
    final on = !engine.solos.contains(ch);
    if (on) {
      engine.solos.add(ch);
    } else {
      engine.solos.remove(ch);
    }
    _ensureStripNotes(ch);
    final n = _soloNotes[ch]!;
    if (n.value != on) n.value = on;
    _coalescer.offer('solo:$ch', () => _send(Cmds.solo(ch, on)));
    // Solo stays global: the page solo banner reads hasSolo/soloCount.
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void clearSolos() {
    if (engine.driver != 'sim' &&
        (!engine.connected ||
            !engine.knownValues.keys
                .any((ch) => engine.valueKnown(ch, 'solo')))) {
      return;
    }
    engine.solos.clear();
    for (final n in _soloNotes.values) {
      if (n.value) n.value = false;
    }
    _send(Cmds.clearSolo());
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void setPan(int ch, double v) {
    if (!canEditControl(ch, 'pan')) return;
    final nv = v.clamp(-1.0, 1.0);
    ps.noteOptimistic(_maps, ch, 'pan', nv * 100, _nowMs());
    _ensureStripNotes(ch);
    final n = _panNotes[ch]!;
    if (n.value != nv) n.value = nv;
    _coalescer.offer('pan:$ch', () => _send(Cmds.param(ch, 'pan', nv * 100)));
    // Hot path: no notifyListeners(). The dragged strip rebuilds from
    // panNoteOf(ch) alone. Caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void setMasterFader(double v) {
    if (!masterControlKnown('fader')) return;
    final nv = v.clamp(0.0, 1.0);
    // Capture the driver's conversion now, before a queued send can run
    // after a driver/bus change. The UI bottom maps to the driver's floor.
    final protocolValue = _uiFaderToProtocol(nv);
    _masterFader = protocolValue;
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busFaders[bus] = _masterFader;
      if (_masterFaderNote.value != nv) {
        _masterFaderNote.value = nv;
      }
      _coalescer.offer(
          'bus_fader:$bus', () => _send(Cmds.busFader(bus, protocolValue)));
      // Hot path: master strip rebuilds from masterFaderNote alone.
      // No global notify; caches invalidated for the next structural pass.
      _invalidateDerived();
      return;
    }
    engine.mainFader = _masterFader;
    if (_masterFaderNote.value != nv) {
      _masterFaderNote.value = nv;
    }
    _coalescer.offer('main_fader', () => _send(Cmds.mainFader(protocolValue)));
    // Hot path: master strip rebuilds from masterFaderNote alone.
    // No global notify; caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void setMasterMuted(bool muted) {
    if (!masterControlKnown('mute')) return;
    _masterMuted = muted;
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busMuted[bus] = muted;
      if (_masterMutedNote.value != muted) _masterMutedNote.value = muted;
      _coalescer.offer('bus_mute:$bus', () => _send(Cmds.busMute(bus, muted)));
      // Hot path: no global notify; caches invalidated.
      _invalidateDerived();
      return;
    }
    engine.mainMuted = muted;
    if (_masterMutedNote.value != muted) _masterMutedNote.value = muted;
    _coalescer.offer('main_mute', () => _send(Cmds.mainMute(muted)));
    // Hot path: no global notify; caches invalidated for the next pass.
    _invalidateDerived();
  }

  @override
  void selectBus(String id) {
    _busId = id;
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void setBank(String key) {
    _bank = key;
    _invalidateDerived();
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
    final cached = _groupsCache;
    if (cached != null) return cached;
    final built = (!_online || engine.strips.isEmpty)
        ? const [
            BridgeGroup(
                key: 'in',
                title: 'Inputs',
                ladders: [],
                collapsedReason: 'offline'),
          ]
        : List<BridgeGroup>.unmodifiable([
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
          ]);
    _groupsCache = built;
    return built;
  }

  @override
  BridgeTap tapOf(int ch) => _taps.putIfAbsent(ch, () => BridgeTap());

  @override
  void setSpectrumTarget(int? ch) {
    // DL32S analysis is passive: neither entry nor meter navigation acquires
    // a named source or writes the desk's shared analyser selector.
    if (engine.driver == 'dl32s') return;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _send(Cmds.spectrumTarget(ch));
  }

  // ------------------------------------------------------------------
  // ChannelStore
  // ------------------------------------------------------------------
  /// Current open channel index (shell prev/next).
  int get channelIndex => _channel;

  /// Select the open channel (called by the shell on navigation).
  void openChannel(int ch) {
    if (_channel == ch) return;
    _channel = ch;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    cmdFocus();
    _invalidateDerived(); // sends are channel-scoped (sendN_on)
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
  String get channelNumber => '${_channel + 1}';

  @override
  ValueNotifier<double> faderNote() => faderOf(_channel);

  @override
  void setFaderDb(double db) => setFader(_channel, dbToPos(db));

  @override
  double? paramNum(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, deskLeaf(leaf));
    return v is num ? v.toDouble() : null;
  }

  @override
  bool? paramBool(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, deskLeaf(leaf));
    return v is bool ? v : null;
  }

  @override
  bool get capabilitiesReady => engine.capsReady;

  @override
  Map<String, dynamic>? paramMetadata(String leaf) =>
      engine.parameterMetadata[deskLeaf(leaf)];

  @override
  bool supports(String leaf) {
    final desk = deskLeaf(leaf);
    if (!engine.capsReady) return false;
    return engine.supportedParams.contains(desk) &&
        (engine.driver == 'sim' ||
            (engine.connected && engine.valueKnown(_channel, desk)));
  }

  @override
  void cmdParam(String leaf, Object value) {
    final desk = deskLeaf(leaf);
    if (!supports(desk)) return;
    if (value is num && !value.isFinite) return;
    final coerced = value;
    // Capture at enqueue time: navigation must not retarget a queued edit.
    final ch = _channel;
    _maps.confirmed.putIfAbsent(ch, () => {});
    ps.noteOptimistic(_maps, ch, desk, coerced, _nowMs());
    _coalescer.offer(
        'param:$ch:$desk', () => _send(Cmds.param(ch, desk, coerced)));
    _invalidateDerived(); // optimistic send reads feed sends
    if (_sendHotField.hasMatch(desk)) {
      _syncSendParamNotes();
    } else {
      notifyListeners();
    }
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
    if (!engine.connected ||
        g < 0 ||
        g >= subgroupCount ||
        subgroupAssigned(g) == null) {
      return;
    }
    engine.subgroupAssign['$_channel:$g'] = on;
    _send(Cmds.subgroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void cmdMuteGroupAssign(int g, bool on) {
    if (!engine.connected ||
        g < 0 ||
        g >= muteGroupCount ||
        muteGroupAssigned(g) == null) {
      return;
    }
    engine.muteGroupAssign['$_channel:$g'] = on;
    _send(Cmds.muteGroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  @override
  ValueNotifier<double> get levelDb => _levelDb;

  @override
  ValueNotifier<List<double>> get spectrum => _spectrum;

  @override
  ValueNotifier<AudioSpectrumFrame?> get audioSpectrum => _audioSpectrum;

  @override
  double? grDb(DynKind kind) {
    final m = kind == DynKind.gate
        ? engine.grGate[_channel]
        : engine.grComp[_channel];
    return m != null && m.isFinite ? m : null;
  }

  @override
  ChannelPreampVm get preamp => ChannelPreampVm(
        hasInput: supports('gain'),
        gain: paramNum('gain') ?? 0,
        trim: paramNum('trim') ?? 0,
        phantom: paramBool('phantom') ?? false,
        invert: paramBool('polarity') ?? false,
        delayMs: paramNum('delay_ms') ?? 0,
        delayOn: paramBool('delay_on') ?? false,
      );

  @override
  ChannelSourceVm get source {
    final raw = ps.optimisticValue(_maps, _channel, 'source');
    final selected = paramNum('source_select')?.toInt();
    return ChannelSourceVm(
      srcName: raw is String ? raw : '—',
      sourceA: paramNum('source_a')?.toInt(),
      sourceB: paramNum('source_b')?.toInt(),
      // DL32S wire selector is 1=A / 0=B, verified against manufacturer mixer software.
      // The view model uses the displayed A/B index, 0=A / 1=B.
      selected: selected == null
          ? null
          : engine.driver == 'dl32s'
              ? 1 - selected
              : selected,
      usbReturn: paramNum('usbreturn')?.toInt(),
      levelDb: _levelDb.value,
    );
  }

  double? sendPan(int slot) {
    if (!supports('send${slot}_pan')) return null;
    final value = paramNum('send${slot}_pan');
    return value == null ? null : value.clamp(-100, 100) / 100;
  }

  /// DL32S source IDs remain opaque; only switch the verified A/B selector.
  @override
  void setSourceSelected(int selected) {
    if (selected == 0 || selected == 1) {
      cmdParam(
          'source_select', engine.driver == 'dl32s' ? 1 - selected : selected);
    }
  }

  @override
  void setSendPan(int slot, double pan) =>
      cmdParam('send${slot}_pan', pan.clamp(-1.0, 1.0) * 100);

  @override
  bool? get mainAssigned => paramBool('lr_assign');
  @override
  void setMainAssigned(bool on) => cmdParam('lr_assign', on);
  @override
  int get dcaCount => [
        for (var n = 1; n <= 16; n++)
          if (engine.supportedParams.contains('dca${n}_assign')) n
      ].fold(0, math.max);
  @override
  bool? dcaAssigned(int group) => paramBool('dca${group + 1}_assign');
  @override
  void setDcaAssigned(int group, bool on) =>
      cmdParam('dca${group + 1}_assign', on);

  @override
  void setCompSoftKnee(bool soft) => cmdParam(
      'comp_knee', engine.driver == 'dl32s' ? (soft ? 0 : 1) : (soft ? 3 : 0));

  int? _eqPresentationType(int band) {
    final native = paramNum('eq${band}_type')?.toInt();
    if (engine.driver != 'dl32s') return native;
    // Only live-verified DL32S shapes. Preserve raw values in the engine.
    return switch (native) {
      2 => EqFilterShape.bell.value,
      3 => EqFilterShape.lowShelf.value,
      4 => EqFilterShape.highShelf.value,
      _ => null,
    };
  }

  @override
  ChannelEqVm get eq => ChannelEqVm(
        on: paramBool('eq_on') ?? false,
        hpfF: paramNum('hpf_freq') ?? 80,
        hpfOn: paramBool('hpf_on') ?? false,
        hpfSlope: 12,
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
              on: true,
              type: _eqPresentationType(b),
            ),
        ],
      );

  ChannelDynVm _dyn(DynKind kind) {
    final p = kind == DynKind.gate ? 'gate' : 'comp';
    return ChannelDynVm(
      kind: kind,
      on: paramBool('${p}_on') ?? false,
      thr: paramNum('${p}_thr') ?? (kind == DynKind.gate ? -50 : -20),
      range: paramNum('${p}_range') ?? 40,
      att: paramNum('${p}_att') ?? (kind == DynKind.gate ? 0.5 : 10),
      hold: paramNum('${p}_hold') ?? 50,
      rel: paramNum('${p}_rel') ?? (kind == DynKind.gate ? 150 : 120),
      ratio: paramNum('${p}_ratio') ?? 3,
      knee: paramNum('${p}_knee') ?? 3,
      makeup: paramNum('${p}_makeup') ?? 0,
      mode: paramNum('${p}_mode')?.toInt(),
      detector: paramNum('${p}_det')?.toInt(),
      envelope: paramNum('${p}_env')?.toInt(),
      keySource: paramNum('${p}_key_src')?.toInt(),
      filterOn: paramBool('${p}_filter_on'),
      filterType: paramNum('${p}_filter_type')?.toInt(),
      filterFreq: paramNum('${p}_filter_freq'),
      mix: paramNum('${p}_mix'),
      auto: paramBool('${p}_auto'),
      softKnee: kind == DynKind.comp && paramNum('comp_knee') != null
          ? (engine.driver == 'dl32s'
              ? paramNum('comp_knee') == 0
              : paramNum('comp_knee')! > 0)
          : null,
    );
  }

  @override
  ChannelDynVm get gate => _dyn(DynKind.gate);

  @override
  ChannelDynVm get comp => _dyn(DynKind.comp);

  @override
  List<ChannelSendVm> get sends {
    final cached = _sendsCache;
    if (cached != null) return cached;
    final built = List<ChannelSendVm>.unmodifiable([
      for (var slot = 1; slot <= 16; slot++)
        if (engine.supportedParams.contains('send${slot}_level'))
          ChannelSendVm(
            slot: slot,
            destName: _sendName(slot),
            level: dbToPos(paramNum('send${slot}_level') ?? -90),
            muted: supports('send${slot}_on')
                ? !(paramBool('send${slot}_on') ?? true)
                : paramBool(_sendMuteLeaf(slot)) ?? false,
            canMute:
                supports('send${slot}_on') || supports(_sendMuteLeaf(slot)),
            pan: sendPan(slot),
            tap: paramNum('send${slot}_tap')?.toInt(),
            tapOptions: supports('send${slot}_tap')
                ? const [0, 1, 2, 3, 4, 5]
                : const [],
            grpon: paramBool('send${slot}_grpon'),
          ),
    ]);
    _sendsCache = built;
    return built;
  }

  String _sendMuteLeaf(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
    return slot <= auxCount ? 'aux${slot}_mute' : 'fx${slot - auxCount}_mute';
  }

  String _sendName(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
    final kind = slot <= auxCount ? 'bus' : 'fxsnd';
    final index = slot <= auxCount ? slot - 1 : slot - auxCount - 1;
    for (final family in engine.topology) {
      if (family['key'] != kind || index >= (family['count'] as num).toInt()) {
        continue;
      }
      final ch = (family['first'] as num).toInt() + index;
      for (final strip in engine.strips) {
        if (strip.ch == ch) return strip.name;
      }
    }
    return slot <= auxCount ? 'Aux $slot' : 'FX ${slot - auxCount}';
  }

  @override
  int get subgroupCount => engine.subgroupFaders.isEmpty
      ? 0
      : engine.subgroupFaders.keys.reduce(math.max) + 1;

  @override
  int get muteGroupCount => engine.muteGroupCount;

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
  void setEqBandType(int band, EqFilterShape shape) {
    if (band != 1 && band != 4) return;
    if (shape != EqFilterShape.bell &&
        shape !=
            (band == 1 ? EqFilterShape.lowShelf : EqFilterShape.highShelf)) {
      return;
    }
    final leaf = 'eq${band}_type';
    if (!supports(leaf) ||
        EqFilterShape.fromValue(_eqPresentationType(band)) == null) {
      return;
    }
    final native = engine.driver == 'dl32s' && shape == EqFilterShape.lowShelf
        ? 3
        : shape.value;
    cmdParam(leaf, native);
  }

  @override
  void setEqOn(bool on) => cmdParam('eq_on', on);

  @override
  void setEqBandOn(int band, bool on) {
    if (band == 0) {
      cmdParam('hpf_on', on);
      return;
    }
    // Neither hardware driver has a per-band bypass leaf.
  }

  @override
  void setHpfSlope(int slope) {
    // The public VM uses dB/oct, while the catalogue uses a native enum.
    // Neither supported hardware driver advertises a verified slope control.
  }

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
    final defaults = kind == DynKind.gate ? dynResetGate : dynResetComp;
    for (final entry in defaults.entries) {
      if (kind == DynKind.comp && entry.key == 'knee') {
        setCompSoftKnee(true);
      } else {
        cmdParam('${kind.name}_${entry.key}', entry.value);
      }
    }
  }

  @override
  void setSendLevel(int slot, double value) {
    final db = posToDb(value.clamp(0.0, 1.0));
    cmdParam('send${slot}_level', db.isFinite ? db : -90.0);
  }

  @override
  void setSendTap(int slot, int tap) => cmdParam('send${slot}_tap', tap);

  @override
  void setSendMute(int slot, bool muted) {
    if (supports('send${slot}_on')) {
      cmdParam('send${slot}_on', !muted);
    } else {
      cmdParam(_sendMuteLeaf(slot), muted);
    }
  }

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
    for (final n in _faderNotes.values) {
      n.dispose();
    }
    for (final n in _muteNotes.values) {
      n.dispose();
    }
    for (final n in _panNotes.values) {
      n.dispose();
    }
    for (final n in _soloNotes.values) {
      n.dispose();
    }
    for (final n in _sendParamNotes.values) {
      n.dispose();
    }
    _masterFaderNote.dispose();
    _masterMutedNote.dispose();
    for (final t in _taps.values) {
      t.dispose();
    }
    for (final b in _busMeters.values) {
      b.dispose();
    }
    _levelDb.dispose();
    _spectrum.dispose();
    _audioSpectrum.dispose();
    for (final note in _controlReadyNotes.values) {
      note.dispose();
    }
    super.dispose();
  }
}
