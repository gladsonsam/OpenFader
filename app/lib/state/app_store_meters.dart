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
  final ValueNotifier<double> _masterMeter = ValueNotifier(double.nan);
  final Map<int, BridgeTap> _taps = {};
  @override
  final Map<String, ValueNotifier<double>> _busMeters = {};
  final Map<int, double> _peakAtMs = {};

  final Map<String, ValueNotifier<OutputMeterVm>> _outputMeters = {};

  @override
  ValueListenable<OutputMeterVm> outputMeterFor(String target) => _outputMeters
      .putIfAbsent(target, () => ValueNotifier(engine.outputMeter(target)));

  final _automixGain = ValueNotifier<double?>(null);
  final _dynamicsMeters = ValueNotifier<ChannelDynamicsMetersVm?>(null);
  @override
  final ValueNotifier<double> _levelDb = ValueNotifier(double.nan);
  @override
  final ValueNotifier<List<double>> _spectrum = ValueNotifier(const []);
  List<double>? _lastSyntheticBins;
  @override
  final ValueNotifier<AudioSpectrumFrame?> _audioSpectrum = ValueNotifier(null);
  final ValueNotifier<AudioSpectrumPeaks> _audioSpectrumPeaks = ValueNotifier(
    const AudioSpectrumPeaks.empty(),
  );
  AudioSpectrumFrame? _lastPeakFrame;

  @override
  void _syncMeters(double nowMs) {
    while (_meters.length < engine.strips.length) {
      _meters.add(ValueNotifier(double.nan));
    }
    while (_meters.length > engine.strips.length) {
      _meters.removeLast().dispose();
    }
    for (var i = 0; i < engine.strips.length; i++) {
      final measured = engine.outputMeter(engine.strips[i].key);
      final lv = measured.available
          ? measured.peak
          : i < engine.meters.length && engine.meters[i].isFinite
          ? engine.meters[i].clamp(0.0, 1.0)
          : double.nan;
      _meters[i].value = lv;
      final ch = engine.strips[i].ch;
      final tap = _taps.putIfAbsent(ch, () => BridgeTap());
      final db = _linToDb(lv);
      if (lv.isNaN) {
        tap.db.value = double.nan;
        tap.peakDb.value = double.nan;
        tap.clip.value = false;
        _peakAtMs.remove(ch);
        continue;
      }
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
    final masterKey = masterProcessingTarget;
    final master = engine.outputMeter(masterKey);
    _masterMeter.value = master.available ? master.peak : double.nan;
    for (final entry in _busMeters.entries) {
      final id = entry.key;
      final target = id.startsWith('solo:')
          ? id.split(':')[1]
          : id.startsWith('subgroup')
          ? 'subgroup/${((int.tryParse(id.substring(8)) ?? 0) + 1).toString().padLeft(2, '0')}'
          : 'bus/${((int.tryParse(id.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';
      final measured = engine.outputMeter(target);
      entry.value.value = measured.available ? measured.peak : double.nan;
    }
    for (final entry in _outputMeters.entries) {
      entry.value.value = engine.outputMeter(entry.key);
    }
    // Open-channel feeds.
    final ci = engine.strips.indexWhere((s) => s.ch == _channel);
    final lv = ci >= 0 && ci < _meters.length ? _meters[ci].value : double.nan;
    _levelDb.value = _linToDb(lv);
    _automixGain.value = engine.freshAutomixGain(_channel);
    final dynamics = engine.dynamicsMeters[_channel];
    _dynamicsMeters.value = dynamics == null
        ? null
        : ChannelDynamicsMetersVm(
            gateIn: dynamics['gate_in']!,
            gateReduction: dynamics['gate_reduction']!,
            gateOut: dynamics['gate_out']!,
            compIn: dynamics['comp_in']!,
            compReduction: dynamics['comp_reduction']!,
            compOut: dynamics['comp_out']!,
          );

    _syncSpectrum();
  }

  @override
  void _syncSpectrum() {
    // Legacy synthetic bins remain exclusively on the demo path.
    final bins = mixerProviderPolicy(engine.driver).syntheticSpectrum
        ? engine.spectrum[_channel]
        : null;
    final visibleBins = engine.connected ? bins : null;
    // Engine spectrum frames replace their list. Repeated meter/GR updates
    // can reuse that snapshot without allocating or rebuilding the graph.
    if (!identical(visibleBins, _lastSyntheticBins)) {
      _lastSyntheticBins = visibleBins;
      _spectrum.value = visibleBins == null
          ? const []
          : List.unmodifiable(visibleBins);
    }
    final frame = engine.audioSpectrum;
    if (frame != null && !frame.isFresh) engine.clearAudioSpectrum();
    _audioSpectrum.value =
        engine.connected &&
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
    while (_meters.length <= i) {
      _meters.add(ValueNotifier(double.nan));
    }
    return _meters[i];
  }

  @override
  ValueNotifier<double> get masterMeter => _masterMeter;

  // ------------------------------------------------------------------
  @override
  List<DestinationControlVm> get monitorControls => List.unmodifiable(
    engine.destinationControls.where((row) => row.surface == 'output_monitor'),
  );

  @override
  bool monitorControlIsCurrent(DestinationControlVm control) =>
      destinationControlIsCurrent(control);

  @override
  Future<String?> setMonitorControl(
    DestinationControlVm control,
    double value,
  ) => setDestinationControl(control, value);

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
              title: 'Channels',
              ladders: [],
              collapsedReason: 'offline',
            ),
          ]
        : List<BridgeGroup>.unmodifiable([
            BridgeGroup(
              key: 'in',
              title: 'Channels',
              ladders: [
                for (final s in engine.strips)
                  BridgeLadder(
                    ch: s.ch,
                    label: s.name.isEmpty ? null : s.name,
                    sub: '${s.ch + 1}',
                  ),
              ],
            ),
          ]);
    _groupsCache = built;
    return built;
  }

  @override
  BridgeTap tapOf(int ch) => _taps.putIfAbsent(ch, () => BridgeTap());

  @override
  List<BridgeOutputGroup> get physicalOutputGroups {
    // Providers advertise the available targets, including unfamiliar families.
    final groups = <String, List<BridgeOutputMeter>>{};
    for (final target in engine.outputMeterTargets) {
      final parts = target.split('/');
      if (parts.length < 2 ||
          (parts.first != 'input' && parts.first != 'physical')) {
        continue;
      }
      final family = parts[1];
      final group = switch ((parts.first, family)) {
        ('input', 'mic') => 'Physical mic inputs',
        ('input', 'aux') => 'Physical aux inputs',
        ('input', 'usb') => 'USB return inputs',
        ('physical', 'aux') => 'Physical aux outputs',
        ('physical', 'main' || 'phones') => 'Physical main and phones',
        ('physical', 'p16') => 'Ultranet outputs',
        ('physical', 'usb') => 'USB outputs',
        _ => '${parts.first == 'input' ? 'Input' : 'Physical'} $family',
      };
      final label = switch ((parts.first, family)) {
        ('input', 'mic') => 'Mic',
        ('input', 'aux') => 'Aux in',
        ('input', 'usb') => 'USB in',
        ('physical', 'aux') => 'Aux out',
        ('physical', 'main') => 'Main output L/R',
        ('physical', 'phones') => 'Phones L/R',
        ('physical', 'p16') => 'P16',
        ('physical', 'usb') => 'USB',
        _ => family,
      };
      final suffix = parts.length > 2
          ? ' ${int.tryParse(parts.last) ?? parts.last}'
          : '';
      groups
          .putIfAbsent(group, () => [])
          .add(BridgeOutputMeter(target, '$label$suffix'));
    }
    return [
      for (final entry in groups.entries)
        BridgeOutputGroup(entry.key, entry.value),
    ];
  }

  @override
  void setSpectrumTarget(int? ch) {
    if (!mixerProviderPolicy(engine.driver).meterSpectrumTarget) return;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    _send(Cmds.spectrumTarget(ch));
  }

  Object? _channelSpectrumOwner;

  @override
  void setChannelSpectrumActive(bool active, {Object? owner}) {
    // New and retiring EQ pages can overlap for one frame when swiping
    // channels. Only the current owner may stop the analyzer subscription.
    if (!active && owner != null && !identical(owner, _channelSpectrumOwner)) {
      return;
    }
    _channelSpectrumOwner = active ? owner : null;
    if (!mixerProviderPolicy(engine.driver).channelSpectrumTarget) return;
    _send(Cmds.spectrumTarget(active ? _channel : null));
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
  ValueListenable<double?> get automixGain => _automixGain;

  @override
  double? grDb(DynKind kind) {
    final m = kind == DynKind.gate
        ? engine.grGate[_channel]
        : engine.grComp[_channel];
    return m != null && m.isFinite ? -math.max(0.0, m) : null;
  }
}
