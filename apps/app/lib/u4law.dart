// Pure fader/pan/meter math for the ui4 layer.
import 'dart:math' as math;

/// Fader law table (designData.ts `law`): [pos, dB] control points.
const List<List<double>> _law = [
  [0, -90],
  [0.125, -50],
  [0.25, -30],
  [0.375, -20],
  [0.5, -10],
  [0.625, -5],
  [0.75, 0],
  [0.875, 5],
  [1, 10],
];

double clamp01(double v) => v.clamp(0.0, 1.0);

/// Position 0..1 -> dB (piecewise-linear through the law table).
double posToDb(double p) {
  if (p <= 0.003) return double.negativeInfinity;
  for (var i = 1; i < _law.length; i++) {
    if (p <= _law[i][0]) {
      final a = _law[i - 1], b = _law[i];
      return a[1] + (b[1] - a[1]) * (p - a[0]) / (b[0] - a[0]);
    }
  }
  return 10;
}

/// dB -> position 0..1 (inverse of the law table). dbToPos(0) == 0.75.
double dbToPos(double db) {
  if (db <= -90) return 0;
  for (var i = 1; i < _law.length; i++) {
    if (db <= _law[i][1]) {
      final a = _law[i - 1], b = _law[i];
      return a[0] + (b[0] - a[0]) * (db - a[1]) / (b[1] - a[1]);
    }
  }
  return 1;
}

/// dB readout with one decimal, e.g. `-4.3`, `+2.0`; `-∞` below -89.
String fmtDb(double p) {
  if (!p.isFinite) return '—';
  final d = posToDb(p);
  if (!d.isFinite || d < -89) return '-∞';
  return (d > 0.04 ? '+' : '') + d.toStringAsFixed(1);
}

/// Pan readout: `C`, `L28`, `R45`, or '' when the strip has no pan.
String panText(double? pan) {
  if (pan == null) return '';
  if (pan == 0) return 'C';
  return (pan < 0 ? 'L' : 'R') + (pan.abs() * 100).round().toString();
}

/// Port of the fader branch of move(): drag pixels -> new 0..1 value.
/// dyPx is up-positive (finger travel upward), matching the fader spec.
double faderFromDrag(double start, double dyPx, double trackH, bool fine) {
  final h = math.max(1.0, trackH);
  return clamp01(start + (dyPx / h) * (fine ? 0.25 : 1.0));
}

/// Port of the pan branch of move(): horizontal drag -> -1..1, snap to 0.
double panFromDrag(double start, double dxPx, double trackW, bool fine) {
  final w = math.max(1.0, trackW);
  var v = (start + (dxPx / w) * 2 * (fine ? 0.25 : 1.0)).clamp(-1.0, 1.0);
  if (v.abs() < 0.03) v = 0;
  return (v * 100).round() / 100;
}

/// Pan marker geometry (signalGeometry.ts): left = 50 + pan * 44 (percent).
double panMarkerLeft(double pan) => 50 + pan * 44;

/// Fader scale marks: pct = pos * 100.
const List<({double pct, String label})> scaleMarks = [
  (pct: 100, label: '10'),
  (pct: 87.5, label: '5'),
  (pct: 75, label: '0'),
  (pct: 62.5, label: '-5'),
  (pct: 50, label: '-10'),
  (pct: 37.5, label: '-20'),
  (pct: 25, label: '-30'),
  (pct: 12.5, label: '-50'),
];

/// Strip-variant fader anatomy (signalGeometry.ts `strip` branch).
abstract final class StripFaderGeo {
  /// Insets of the track inside the fader area (below the readout).
  static const insetT = 26.0;
  static const insetB = 28.0;

  /// Cap size (strip variant 36x50) + grip insets.
  static const capW = 36.0;
  static const capH = 50.0;
  static const gripLR = 4.0;
  static const gripTB = 5.0;
  static const gripH = 14.0;

  /// Readout strip above the track.
  static const readH = 15.0;
  static const readFont = 12.0;

  /// Grab pad around the cap (pad = 8).
  static const grabPad = 8.0;

  /// Direction-lock slop (10px vertical, 12px horizontal).
  static const lockV = 10.0;
  static const lockH = 12.0;
}
