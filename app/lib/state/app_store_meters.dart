part of 'app_store.dart';

// Meter/spectrum snapshots and stable meter notifiers. Owns no timer.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreMetersContract {
  ValueNotifier<List<double>> get _spectrum;
  ValueNotifier<AudioSpectrumFrame?> get _audioSpectrum;
  void _clearAudioSpectrumPeaks();
  void _syncMeters(double nowMs);
  void _syncSpectrum();
  ValueNotifier<double> get _masterMeter;
  Map<String, ValueNotifier<double>> get _busMeters;
  ValueNotifier<double> get _levelDb;
  ValueNotifier<double> get levelDb;
}

mixin _AppStoreMeters on _AppStoreContext {
  // --- meter notifiers (stable, updated in place) ---
  final List<ValueNotifier<double>> _meters = [];
  @override
  final ValueNotifier<double> _masterMeter = ValueNotifier(0);
  final Map<int, BridgeTap> _taps = {};
  @override
  final Map<String, ValueNotifier<double>> _busMeters = {};
  final Map<int, double> _peakAtMs = {};

  final _dynamicsMeters = ValueNotifier<ChannelDynamicsMetersVm?>(null);
  @override
  final ValueNotifier<double> _levelDb = ValueNotifier(double.negativeInfinity);
  @override
  final ValueNotifier<List<double>> _spectrum = ValueNotifier(const []);
  @override
  final ValueNotifier<AudioSpectrumFrame?> _audioSpectrum = ValueNotifier(null);
  final ValueNotifier<AudioSpectrumPeaks> _audioSpectrumPeaks =
      ValueNotifier(const AudioSpectrumPeaks.empty());
  AudioSpectrumFrame? _lastPeakFrame;

  @override
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
        : _selectedSubgroup != null
            ? 'subgroup/${(_selectedSubgroup! + 1).toString().padLeft(2, '0')}'
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
    final dynamics = engine.dynamicsMeters[_channel];
    _dynamicsMeters.value = dynamics == null
        ? null
        : ChannelDynamicsMetersVm(
            gateIn: dynamics['gate_in']!,
            gateReduction: dynamics['gate_reduction']!,
            gateOut: dynamics['gate_out']!,
            compIn: dynamics['comp_in']!,
            compReduction: dynamics['comp_reduction']!,
            compOut: dynamics['comp_out']!);

    _syncSpectrum();
  }

  @override
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
    final measured = _audioSpectrum.value;
    if (measured == null) {
      _clearAudioSpectrumPeaks();
    } else if (!identical(measured, _lastPeakFrame)) {
      final previous = _lastPeakFrame;
      // A gap can outlast expiry between polls even if a fresh frame has
      // already replaced the old one. Do not carry peaks across that gap.
      if (previous != null &&
          measured.receivedAt.difference(previous.receivedAt) >=
              AudioSpectrumFrame.expiry) {
        _clearAudioSpectrumPeaks();
      }
      _lastPeakFrame = measured;
      _audioSpectrumPeaks.value = _audioSpectrumPeaks.value.include(measured);
    }
  }

  @override
  void _clearAudioSpectrumPeaks() {
    _lastPeakFrame = null;
    _audioSpectrumPeaks.value = const AudioSpectrumPeaks.empty();
  }

  @override
  void resetAudioSpectrumPeaks() {
    _clearAudioSpectrumPeaks();
    // A visibility change starts a new hold with the next incoming frame,
    // rather than seeding it again from the cached pre-toggle measurement.
    _lastPeakFrame = engine.audioSpectrum;
  }

  @override
  ValueNotifier<double> meterOf(int ch) {
    final i = engine.strips.indexWhere((s) => s.ch == ch);
    if (i < 0) return _masterMeter;
    return _meters[i];
  }

  @override
  ValueNotifier<double> get masterMeter => _masterMeter;

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
    _clearAudioSpectrumPeaks();
    _send(Cmds.spectrumTarget(ch));
  }

  @override
  ValueNotifier<double> get levelDb => _levelDb;

  @override
  ValueNotifier<List<double>> get spectrum => _spectrum;

  @override
  ValueNotifier<AudioSpectrumFrame?> get audioSpectrum => _audioSpectrum;

  @override
  ValueNotifier<AudioSpectrumPeaks> get audioSpectrumPeaks =>
      _audioSpectrumPeaks;

  @override
  bool get hasDynamicsMeters => engine.dynamicsMetersSupported;

  @override
  ValueListenable<ChannelDynamicsMetersVm?> get dynamicsMeters =>
      _dynamicsMeters;

  @override
  double? grDb(DynKind kind) {
    final m = kind == DynKind.gate
        ? engine.grGate[_channel]
        : engine.grComp[_channel];
    return m != null && m.isFinite ? -math.max(0.0, m) : null;
  }
}
