// W2a mapping: fader laws (canonical implementation).
//
// The World exposes faderToDb/dbToFader from the caps-named law (default
// X-Air); UI dB readouts bound to the engine must follow the World's law so
// the readout agrees with the engine round-trip exactly.

const double topDb = 10;

/// Design and X-Air floor in dB (DL32S uses -120, carried on its own law).
const double floorDb = -90;

class FaderLaw {
  final String id;
  final String label;
  final double Function(double f) toDb;
  final double Function(double db) fromDb;
  final double floorDb;
  final double topDb;

  const FaderLaw({
    required this.id,
    required this.label,
    required this.toDb,
    required this.fromDb,
    required this.floorDb,
    required this.topDb,
  });
}

const List<List<double>> _designPoints = [
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

double _designToDb(double f) {
  if (!f.isFinite || f <= 0.003) return double.negativeInfinity;
  for (var i = 1; i < _designPoints.length; i++) {
    if (f <= _designPoints[i][0]) {
      final a = _designPoints[i - 1];
      final b = _designPoints[i];
      return a[1] + ((b[1] - a[1]) * (f - a[0])) / (b[0] - a[0]);
    }
  }
  return topDb;
}

double _designFromDb(double db) {
  if (!db.isFinite || db <= floorDb) return 0;
  for (var i = 1; i < _designPoints.length; i++) {
    if (db <= _designPoints[i][1]) {
      final a = _designPoints[i - 1];
      final b = _designPoints[i];
      return a[0] + ((b[0] - a[0]) * (db - a[1])) / (b[1] - a[1]);
    }
  }
  return 1;
}

/// X-Air / X32 "level" curve (joints 0.0625/-60 .. 1/+10).
double _xairToDb(double f) {
  if (!f.isFinite || f <= 0) return double.negativeInfinity;
  if (f >= 0.5) return 40 * f - 30;
  if (f >= 0.25) return 80 * f - 50;
  if (f >= 0.0625) return 160 * f - 70;
  return 480 * f - 90;
}

double _xairFromDb(double db) {
  if (!db.isFinite || db <= floorDb) return 0;
  if (db >= topDb) return 1;
  if (db >= -10) return 0.5 + (db + 10) / 40;
  if (db >= -30) return 0.25 + (db + 30) / 80;
  if (db >= -60) return 0.0625 + (db + 60) / 160;
  return (db - floorDb) / 480;
}

const List<List<double>> _dl32sPoints = [
  [0, -120],
  [1 / 9, -60],
  [2 / 9, -40],
  [3 / 9, -30],
  [4 / 9, -20],
  [5 / 9, -10],
  [6 / 9, -5],
  [7 / 9, 0],
  [8 / 9, 5],
  [1, 10],
];

double _dl32sToDb(double f) {
  if (!f.isFinite || f <= 0) return double.negativeInfinity;
  for (var i = 1; i < _dl32sPoints.length; i++) {
    if (f <= _dl32sPoints[i][0]) {
      final a = _dl32sPoints[i - 1];
      final b = _dl32sPoints[i];
      return a[1] + ((b[1] - a[1]) * (f - a[0])) / (b[0] - a[0]);
    }
  }
  return topDb;
}

double _dl32sFromDb(double db) {
  if (!db.isFinite || db <= -120) return 0;
  if (db >= topDb) return 1;
  for (var i = 1; i < _dl32sPoints.length; i++) {
    if (db <= _dl32sPoints[i][1]) {
      final a = _dl32sPoints[i - 1];
      final b = _dl32sPoints[i];
      return a[0] + ((b[0] - a[0]) * (db - a[1])) / (b[1] - a[1]);
    }
  }
  return 1;
}

/// Design fixture law (fixture mode only; never bound to the engine).
const FaderLaw designLaw = FaderLaw(
  id: 'design',
  label: 'Design fixture law',
  toDb: _designToDb,
  fromDb: _designFromDb,
  floorDb: floorDb,
  topDb: topDb,
);

/// X-Air / X32 law (the default engine law).
const FaderLaw xairLaw = FaderLaw(
  id: 'xair',
  label: 'X-Air / X32',
  toDb: _xairToDb,
  fromDb: _xairFromDb,
  floorDb: floorDb,
  topDb: topDb,
);

/// Mackie DL32S law (floor -120 dB).
const FaderLaw dl32sLaw = FaderLaw(
  id: 'dl32s',
  label: 'Mackie DL32S',
  toDb: _dl32sToDb,
  fromDb: _dl32sFromDb,
  floorDb: -120,
  topDb: topDb,
);

// OpenFader WING protocol normalization is linear over the full dB range.
// The provider sends engineering dB, never WING's normalized reply argument.
double _wingToDb(double f) => !f.isFinite || f <= 0
    ? double.negativeInfinity
    : -144 + f.clamp(0, 1) * 154;
double _wingFromDb(double db) =>
    !db.isFinite || db <= -144 ? 0 : ((db + 144) / 154).clamp(0, 1).toDouble();

const FaderLaw wingLaw = FaderLaw(
  id: 'wing',
  label: 'Behringer WING',
  toDb: _wingToDb,
  fromDb: _wingFromDb,
  floorDb: -144,
  topDb: 10,
);

const Map<String, FaderLaw> _laws = {
  'wing': wingLaw,
  'design': designLaw,
  'xair': xairLaw,
  'dl32s': dl32sLaw,
};

/// Law named by caps (driver id -> law). Unknown drivers fall back to X-Air.
FaderLaw lawForDriver(String? driver) =>
    (driver != null && _laws.containsKey(driver)) ? _laws[driver]! : xairLaw;

String formatDb(double db) {
  if (!db.isFinite || db <= floorDb) return '-inf';
  final rounded = (db * 10).round() / 10;
  if (rounded == 0) return '0.0';
  return '${db >= 0 ? '+' : ''}${db.toStringAsFixed(1)}';
}

/// Max design-to-xair deviation in dB over a sweep (the mismatch report).
({double maxDb, double atF}) maxLawDeviation([int samples = 1001]) {
  var maxDb = 0.0;
  var atF = 0.0;
  for (var i = 1; i <= samples; i++) {
    final f = i / samples;
    final a = designLaw.toDb(f);
    final b = xairLaw.toDb(f);
    if (!a.isFinite || !b.isFinite) continue;
    final d = (a - b).abs();
    if (d > maxDb) {
      maxDb = d;
      atF = f;
    }
  }
  return (maxDb: maxDb, atF: atF);
}
