// Shell adapter: fx responsibility. The shell owns polling.
import 'dart:math' as math;
import 'package:flutter/foundation.dart';
import '../engine.dart';
import '../core/access_limits.dart';
import '../screens/fx.dart';
import 'store_adapter.dart';

class ShellFxStore extends ShellStoreAdapter implements FxStore {
  ShellFxStore(super.app) {
    // FX meters ride the shell poll loop, so the FX page needs no timer.

    app.addListener(_syncFxMeters);
  }

  int _sel = 0;
  final Map<int, ValueNotifier<double>> _meters = {};
  final Stopwatch _tapClock = Stopwatch()..start();
  final Map<String, int> _tapTimes = {};
  final Set<String> _tapWrites = {};
  int? _tapSession;

  bool get _globalAccessAllowed =>
      app.accessFamilyAllowed(AccessFamily.fx) &&
      (_cur == null ||
          app.accessTargetAllowed(
              'fx/${_cur!.slot.toString().padLeft(2, '0')}'));

  @override
  int get controlAccessEpoch => app.controlAccessEpoch;

  @override
  Object get controlSession =>
      (engine.driver, engine.session, engine.connected);

  @override
  bool get stateControlsSupported =>
      _globalAccessAllowed && engine.fxStateControls;

  @override
  List<DestinationControlVm> get destinationControls => List.unmodifiable(
      engine.destinationControls.where((control) => control.surface == 'fx'));
  @override
  bool destinationControlIsCurrent(DestinationControlVm control) =>
      app.destinationControlIsCurrent(control);
  @override
  Future<String?> setDestinationControl(
          DestinationControlVm control, double value) =>
      app.setDestinationControl(control, value);

  @override
  Future<String?> tapDestinationControl(DestinationControlVm control) async {
    if (isDisposed ||
        !control.canTapTempo ||
        !destinationControlIsCurrent(control)) {
      return 'Connection or setting changed. Reopen this control.';
    }
    if (_tapSession != engine.session) {
      _tapSession = engine.session;
      _tapTimes.clear();
      _tapWrites.clear();
    }
    final key =
        '${engine.session}/${control.epoch}/${control.target}/${control.id}'
        '/${control.targetName}/${control.min}/${control.max}/${control.unit}';
    final now = _tapClock.elapsedMilliseconds * 1000;
    final previous = _tapTimes[key];
    _tapTimes[key] = now;
    if (previous == null) return null;
    final value = control.valueForTapPeriod(now - previous);
    // A long pause starts a new pair. Keep recording taps during a write
    // so the next accepted interval never includes a skipped beat.
    if (value == null || !_tapWrites.add(key)) return null;
    try {
      return await setDestinationControl(control, value);
    } finally {
      _tapWrites.remove(key);
    }
  }

  @override
  void dispose() {
    app.removeListener(_syncFxMeters);
    _tapClock.stop();
    _tapTimes.clear();
    _tapWrites.clear();
    for (final m in _meters.values) {
      m.dispose();
    }
    super.dispose();
  }

  List<FxSlot> get _raw => engine.fx;

  @override
  // A provider-native catalogue owns the rack presentation, including sim.
  // Keep legacy numbered/design racks for providers without that contract.
  List<FxSlotVm> get slots => engine.destinationControls.any(
          (control) => control.surface == 'fx' && control.id == 'algorithm')
      ? const []
      : [
          for (final s in _raw)
            FxSlotVm(
              slot: s.slot,
              name: 'FX ${s.slot}',
              type: _typeMetadata(s.type)?['id'] as String? ??
                  (s.type.isEmpty ? '—' : s.type),
              on: s.on,
              bypass: s.bypass,
              ret: s.returnLabel,
            ),
        ];

  @override
  int get selected => _sel.clamp(0, _raw.isEmpty ? 0 : _raw.length - 1);

  @override
  List<String> get typeOptions => !_globalAccessAllowed
      ? const []
      : [
          for (final type in engine.fxTypes)
            if (type['slots'] is! List ||
                (type['slots'] as List).contains(selected + 1))
              (type['id'] ?? type['label']) as String
        ];

  Map<String, dynamic>? _typeMetadata(String type) {
    for (final metadata in engine.fxTypes) {
      final id = metadata['id'] as String?;
      final label = metadata['label'] as String?;
      if (id?.toLowerCase() == type.toLowerCase() ||
          label?.toLowerCase() == type.toLowerCase()) {
        return metadata;
      }
    }
    return null;
  }

  @override
  String typeLabel(String type) =>
      _typeMetadata(type)?['label'] as String? ?? type;

  String _parameterText(double normalized, Map<String, dynamic>? metadata) {
    final choices = metadata?['choices'];
    if (choices is List && choices.isNotEmpty) {
      final index = (normalized * (choices.length - 1))
          .round()
          .clamp(0, choices.length - 1);
      final choice = choices[index];
      if (choice is Map && choice['label'] is String) {
        return choice['label'] as String;
      }
    }
    final min = (metadata?['min'] as num?)?.toDouble();
    final max = (metadata?['max'] as num?)?.toDouble();
    if (min == null ||
        max == null ||
        !min.isFinite ||
        !max.isFinite ||
        max == min) {
      return '${(normalized * 100).toStringAsFixed(0)}%';
    }
    final scale = metadata?['scale'];
    if (scale != 'lin' && scale != 'log' || scale == 'log' && min <= 0) {
      return '${(normalized * 100).toStringAsFixed(0)}%';
    }
    final value = scale == 'log'
        ? min * math.pow(max / min, normalized)
        : min + (max - min) * normalized;
    final unit = metadata?['unit'] as String? ?? '';
    if (unit == 'Hz' && value >= 1000) {
      return '${(value / 1000).toStringAsFixed(2)} kHz';
    }
    final text = value.toStringAsFixed(
        unit == '%' || unit == 'ms' || value.abs() >= 100 ? 0 : 1);
    return unit.isEmpty ? text : '$text${unit == '%' ? '' : ' '}$unit';
  }

  FxSlot? get _cur =>
      _raw.isEmpty ? null : _raw[selected.clamp(0, _raw.length - 1)];

  @override
  List<FxKnobVm> get knobs {
    final c = _cur;
    if (c == null || !_globalAccessAllowed) return const [];
    final definitions = <String, Map<String, dynamic>>{
      for (final parameter
          in _typeMetadata(c.type)?['params'] as List? ?? const [])
        if (parameter is Map<String, dynamic> && parameter['id'] is String)
          parameter['id'] as String: parameter,
    };
    final unknownIds = c.params.keys
        .where((id) => !definitions.containsKey(id))
        .toList()
      ..sort();
    final ids = [
      for (final id in definitions.keys)
        if (c.params.containsKey(id)) id,
      ...unknownIds,
    ];
    FxKnobVm makeKnob(String id) {
      final meta = definitions[id];
      final normalized = c.params[id]!.clamp(0.0, 1.0);
      final choices = (meta?['choices'] as List? ?? const [])
          .whereType<Map>()
          .map((v) => v['label']?.toString() ?? '')
          .toList();
      final rawMin = (meta?['min'] as num?)?.toDouble();
      final rawMax = (meta?['max'] as num?)?.toDouble();
      final scale = meta?['scale'];
      final valid = rawMin != null &&
          rawMax != null &&
          rawMin.isFinite &&
          rawMax.isFinite &&
          rawMax > rawMin &&
          (scale == 'lin' || scale == 'log' && rawMin > 0);
      final min = choices.isNotEmpty
          ? 0.0
          : valid
              ? rawMin
              : 0.0;
      final max = choices.isNotEmpty
          ? (choices.length - 1).toDouble()
          : valid
              ? rawMax
              : 100.0;
      final value = choices.isNotEmpty
          ? (normalized * max).roundToDouble()
          : valid && scale == 'log'
              ? min * math.pow(max / min, normalized)
              : min + (max - min) * normalized;
      return FxKnobVm(
        id: id,
        label: definitions[id]?['label'] as String? ?? id,
        min: 0,
        max: 1,
        value: c.params[id]!.clamp(0.0, 1.0),
        text: _parameterText(normalized, meta),
        entryMin: min,
        entryMax: max,
        entryValue: value,
        choices: choices,
        normalize: (value) => max == min
            ? 0
            : choices.isEmpty && valid && scale == 'log'
                ? math.log(value / min) / math.log(max / min)
                : (value - min) / (max - min),
      );
    }

    return [for (final id in ids) makeKnob(id)];
  }

  @override
  // The engine has no tap command/capability yet; do not offer a no-op.
  bool get tapVisible => false;

  @override
  String get tapLabel => 'TAP';

  @override
  ValueNotifier<double> meterOf(int slot) {
    final n = _meters.putIfAbsent(slot, () => ValueNotifier(0));
    final i = _raw.indexWhere((s) => s.slot == slot);
    if (i >= 0 && i < engine.meters.length) {
      n.value = _raw[i].level.clamp(0.0, 1.0);
    }
    return n;
  }

  @override
  Future<void> pollMeters() async {
    await app.pollMeters();
    _syncFxMeters();
  }

  void _syncFxMeters() {
    if (isDisposed) return;
    for (final e in _meters.entries) {
      final i = _raw.indexWhere((s) => s.slot == e.key);
      if (i >= 0 && i < engine.meters.length) {
        e.value.value = _raw[i].level.clamp(0.0, 1.0);
      }
    }
  }

  @override
  void selectSlot(int slot) {
    final i = _raw.indexWhere((s) => s.slot == slot);
    _sel = i < 0 ? slot : i;
    notifyListeners();
  }

  @override
  void setType(String type) {
    final c = _cur;
    if (c == null) return;
    send(Cmds.fxSetType(c.slot, type));
  }

  @override
  void setParam(String id, double value) {
    final c = _cur;
    if (c == null ||
        !_globalAccessAllowed ||
        !engine.connected ||
        !value.isFinite ||
        value < 0 ||
        value > 1 ||
        !c.params.containsKey(id)) {
      return;
    }
    send(Cmds.fxParam(c.slot, id, value));
  }

  @override
  void setOn(bool on) {
    final c = _cur;
    if (c == null || !stateControlsSupported) return;
    send(Cmds.fxOn(c.slot, on));
  }

  @override
  void setBypass(bool bypass) {
    final c = _cur;
    if (c == null || !stateControlsSupported) return;
    send(Cmds.fxBypass(c.slot, bypass));
  }

  // No tap-tempo cmd exists, so the TAP tile stays a no-op until the protocol grows one.

  @override
  void tap() {}
}
