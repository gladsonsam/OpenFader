import 'package:flutter/foundation.dart';

/// A measured output level, independent of input-strip indices or DSP controls.
@immutable
class OutputMeterVm {
  final List<double> levels;
  final bool inferred;
  const OutputMeterVm({this.levels = const [], this.inferred = false});
  bool get available => levels.isNotEmpty;
  double get peak => levels.fold(0, (a, b) => a > b ? a : b);

  @override
  bool operator ==(Object other) =>
      other is OutputMeterVm &&
      inferred == other.inferred &&
      listEquals(levels, other.levels);
  @override
  int get hashCode => Object.hash(inferred, Object.hashAll(levels));
}

/// Original sample age survives relays; repeated cached frames do not renew it.
class OutputMeterSample {
  final OutputMeterVm meter;
  final DateTime sampledAt;
  const OutputMeterSample(this.meter, this.sampledAt);
  bool get isFresh =>
      DateTime.now().difference(sampledAt) < const Duration(seconds: 1);
}

/// Validates the optional target-keyed R2 stream independently of widget state.
class OutputMeterTelemetry {
  final Map<String, OutputMeterSample> samples = {};
  int? _generation;

  void reset() {
    samples.clear();
    _generation = null;
  }

  void apply(Map<String, dynamic> event,
      {required int? epoch, required List<String> targets}) {
    final generation = event['source_generation'];
    final age = event['age_ms'];
    final frames = event['frames'];
    if (epoch == null ||
        event['epoch'] != epoch ||
        generation is! int ||
        generation < 0 ||
        age is! int ||
        age < 0 ||
        frames is! List) {
      return;
    }
    if (_generation != null && generation < _generation!) return;
    _generation = generation;
    // Explicit unavailability also clears during a suspended provider switch.
    if (frames.isEmpty) {
      samples.clear();
      return;
    }
    samples.clear();
    if (age >= 1000) return;
    final sampledAt = DateTime.now().subtract(Duration(milliseconds: age));
    for (final frame in frames) {
      if (frame is! Map || frame['target'] is! String) continue;
      final target = frame['target'] as String;
      final levels = frame['levels'];
      if (!targets.contains(target) ||
          levels is! List ||
          levels.isEmpty ||
          levels.length > 2 ||
          levels.any((v) => v is! num || !v.toDouble().isFinite)) {
        continue;
      }
      samples[target] = OutputMeterSample(
          OutputMeterVm(
            levels: List.unmodifiable(
                levels.map((v) => (v as num).toDouble().clamp(0.0, 1.0))),
            inferred: frame['inferred'] == true,
          ),
          sampledAt);
    }
  }
}
