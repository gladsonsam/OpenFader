import 'dart:typed_data';
import 'dart:math' as math;
import 'fx_position_scale.dart';

// mixer control software's de-esser display uses stored positions, not a DSP unit.
// Preserve the float32 receive arithmetic before truncating the position.
double xairDeesserPosition(double normalized) {
  final value = Float32List(1)..[0] = normalized;
  value[0] = value[0] * 50.5;
  return value[0].truncate().clamp(0, 50).toDouble();
}

// Display-space numeric entry rounds to the nearest stored position.
// Rounding occurs after conversion to the vendor's float32 input domain.
double xairDeesserEntry(double display) {
  final value = Float32List(1)..[0] = display;
  value[0] = value[0] + 0.5;
  return value[0].truncate().clamp(0, 50) / 50;
}

double _f32(double v) => (Float32List(1)..[0] = v)[0];

class XairHallScale implements FxPositionScale {
  const XairHallScale(
    this.min,
    this.max,
    this.steps,
    this.logarithmic,
    this.unit,
    this.precision,
  );
  @override
  final double min;
  @override
  final double max;
  @override
  final int steps;
  final bool logarithmic;
  @override
  final String unit;
  final int precision;

  static XairHallScale? fromMetadata(Map? metadata) {
    final legacy = forMarker(metadata?['scale']);
    if (legacy != null) return legacy;
    if (metadata?['scale'] == 'xair_deesser_steps') {
      return const XairHallScale(0, 50, 50, false, '', 1);
    }
    if (metadata?['scale'] != 'xair_positions') return null;
    final position = metadata?['position'];
    final min = metadata?['min'];
    final max = metadata?['max'];
    final unit = metadata?['unit'];
    if (position is! Map || min is! num || max is! num || unit is! String) {
      return null;
    }
    final steps = position['steps'];
    final precision = position['precision'];
    final curve = position['curve'];
    if (!min.isFinite ||
        !max.isFinite ||
        max <= min ||
        steps is! int ||
        steps < 1 ||
        steps > 65536 ||
        precision is! int ||
        precision < 0 ||
        precision > 6 ||
        (curve != 'linear' && curve != 'log') ||
        (curve == 'log' && min <= 0)) {
      return null;
    }
    final low = _f32(min.toDouble());
    final high = _f32(max.toDouble());
    final span = _f32(high - low);
    if (!low.isFinite || !high.isFinite || !span.isFinite || span <= 0) {
      return null;
    }
    if (curve == 'log') {
      final ratio = _f32(high / low);
      final step = _f32(_f32(math.log(ratio)) / steps);
      if (low <= 0 || !ratio.isFinite || !step.isFinite || step <= 0) {
        return null;
      }
    }
    return XairHallScale(
      min.toDouble(),
      max.toDouble(),
      steps,
      curve == 'log',
      unit,
      precision,
    );
  }

  static XairHallScale? forMarker(Object? marker) => switch (marker) {
        'xair_hall_predelay' =>
          const XairHallScale(0, 200, 100, false, 'ms', 0),
        'xair_hall_decay' => const XairHallScale(0.2, 5, 50, true, 's', 2),
        'xair_hall_size' => const XairHallScale(2, 100, 49, false, '', 0),
        'xair_hall_damping' =>
          const XairHallScale(1000, 20000, 24, true, 'Hz', 0),
        'xair_hall_hicut' => const XairHallScale(200, 20000, 50, true, 'Hz', 0),
        'xair_hall_diffusion' => const XairHallScale(1, 30, 29, false, '', 0),
        'xair_hall_level' => const XairHallScale(-12, 12, 48, false, 'dB', 1),
        'xair_hall_locut' => const XairHallScale(10, 500, 50, true, 'Hz', 0),
        'xair_hall_bassmult' => const XairHallScale(0.5, 2, 50, true, '', 2),
        'xair_hall_spread' => const XairHallScale(0, 50, 50, false, '', 0),
        'xair_hall_shape' => const XairHallScale(0, 250, 50, false, '', 0),
        'xair_hall_modspeed' => const XairHallScale(0, 100, 20, false, '', 0),
        _ => null,
      };

  @override
  int position(double normalized) =>
      _f32(_f32(normalized) * _f32(steps + 0.5)).truncate().clamp(0, steps);
  double get _logStep =>
      _f32(_f32(math.log(_f32(_f32(max) / _f32(min)))) / steps);
  @override
  double display(int p) => logarithmic
      ? _f32(_f32(min) * _f32(math.exp(_f32(_logStep * p))))
      : _f32(_f32(min) + _f32(_f32(_f32(_f32(max) - _f32(min)) * p) / steps));
  @override
  double entry(double value) {
    if (!logarithmic && min == 0 && max == 50 && steps == 50) {
      return xairDeesserEntry(value);
    }
    final v = _f32(value);
    final raw = logarithmic
        ? _f32(_f32(math.log(_f32(v / _f32(min)))) / _logStep)
        : _f32(_f32(_f32(v - _f32(min)) * steps) / _f32(_f32(max) - _f32(min)));
    return _f32(raw + 0.5).truncate().clamp(0, steps) / steps;
  }

  @override
  String text(int p) {
    final number = display(p).toStringAsFixed(precision);
    return unit.isEmpty ? number : '$number $unit';
  }
}
