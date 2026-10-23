// Channel DSP view-models + write helpers:
//
// - EQ edits (eqParamFromDrag / eqNodeFromPos / reset tables)
// - Dyn edits (specFromDrag / dynResetDefaults / DYN_RESET tables)
// - View shapes (EQ_GRAPH_W/H/N, band colours/names;
//   DYN_SIZE/LO/COLOR, 81-sample transfer curves)
//
// Pure Dart: no Flutter, no Area A store dependency. Writes go through an
// injected [ChannelWrite] fn so the owner can attach its 60 Hz coalescer
// (see [ChannelWrites]); this file never imports lib/core/store.

import 'dart:async';
import 'dart:math' as math;

/// Raw engine write: leaf + value (cmdParam equivalent).
typedef ChannelWrite = void Function(String leaf, Object value);

/// 60 Hz coalescing writer: at most one flush per [interval]; the last
/// queued value per leaf wins. The owner passes its cmdParam as [write].
class ChannelWrites {
  final ChannelWrite _write;
  final Duration interval;
  final Map<String, Object> _pending = {};
  Timer? _timer;

  ChannelWrites(this._write, {this.interval = const Duration(milliseconds: 16)});

  void queue(String leaf, Object value) {
    _pending[leaf] = value;
    _timer ??= Timer(interval, flush);
  }

  void flush() {
    _timer?.cancel();
    _timer = null;
    if (_pending.isEmpty) return;
    final batch = Map<String, Object>.of(_pending);
    _pending.clear();
    for (final e in batch.entries) {
      _write(e.key, e.value);
    }
  }

  void dispose() {
    _timer?.cancel();
    _timer = null;
    _pending.clear();
  }
}

/// EQ drag math port (eq.ts eqParamFromDrag).
double eqParamFromDrag(
  String param,
  int band,
  double start,
  double dyPx,
  bool fine, {
  List<double>? range,
}) {
  final dy = dyPx * (fine ? 0.25 : 1.0);
  final lo = range != null ? range[0] : (param == 'f' ? 20.0 : param == 'g' ? -15.0 : 0.3);
  final hi = range != null
      ? range[1]
      : (param == 'f' ? (band == 0 ? 1000.0 : 20000.0) : param == 'g' ? 15.0 : 16.0);
  double clamp(double v) => v < lo ? lo : (v > hi ? hi : v);
  if (param == 'f') return clamp(start * math.pow(2, dy / 30).toDouble());
  if (param == 'g') return (clamp(start + dy * 0.1) * 10).round() / 10;
  return (clamp(start * math.pow(2, dy / 60).toDouble()) * 100).round() / 100;
}

/// Graph-node drag port (eq.ts eqNodeFromPos): unit plot pos -> freq/gain.
({double f, double g}) eqNodeFromPos(double x, double y) {
  final cx = x.clamp(0.0, 1.0);
  final cy = y.clamp(0.0, 1.0);
  return (f: 20 * math.pow(1000, cx).toDouble(), g: ((15 - 30 * cy) * 10).round() / 10);
}

/// Spec-knob drag port (inputDyn.ts specFromDrag): normalised drag -> value.
double specFromDrag(double min, double max, double startN, double dyPx, bool fine) {
  final n = (startN + dyPx / 180 * (fine ? 0.25 : 1.0)).clamp(0.0, 1.0);
  return min + (max - min) * n;
}

/// Normalised 0..1 position of [v] in [min]..[max].
double toN(double min, double max, double v) => ((v - min) / (max - min)).clamp(0.0, 1.0);

/// Denormalise [n] (0..1) into [min]..[max].
double fromN(double min, double max, double n) => min + (max - min) * n.clamp(0.0, 1.0);

enum EqBandType { low, mid, high, lowCut, highCut }

/// One EQ band view-model (bands 1..4 parametric + HPF handled separately).
class EqBandVm {
  final int band; // 1..4
  final String name;
  final double f; // Hz
  final double g; // dB
  final double q;
  final bool on;
  final List<double>? fRange;
  final List<double>? gRange;
  final List<double>? qRange;

  const EqBandVm({
    required this.band,
    required this.name,
    required this.f,
    required this.g,
    required this.q,
    this.on = true,
    this.fRange,
    this.gRange,
    this.qRange,
  });

  EqBandVm copyWith({double? f, double? g, double? q, bool? on}) => EqBandVm(
        band: band,
        name: name,
        f: f ?? this.f,
        g: g ?? this.g,
        q: q ?? this.q,
        on: on ?? this.on,
        fRange: fRange,
        gRange: gRange,
        qRange: qRange,
      );
}

/// Full channel EQ snapshot the painters + cards render.
class ChannelEqVm {
  final bool on;
  final double hpfF; // Hz
  final bool hpfOn;
  final int hpfSlope; // 12 | 24
  final List<EqBandVm> bands; // 4 parametric bands
  final int selectedBand; // 0 = HPF, 1..4 bands

  const ChannelEqVm({
    this.on = true,
    this.hpfF = 80,
    this.hpfOn = false,
    this.hpfSlope = 12,
    this.bands = const [],
    this.selectedBand = 1,
  });
}

/// EQ graph geometry (eqModel.ts: viewBox 1000x300, N = 180 segments).
class EqGraphGeom {
  static const double w = 1000;
  static const double h = 300;
  static const int n = 180;
  static const double fMin = 20;
  static const double fMax = 20000;
  static const double dbTop = 15;
  static const double dbBottom = -15;

  /// Band node colours, index 0 = HPF (eqModel.ts EQ_BAND_COLORS).
  static const bandColors = <int>[
    0xFFD7DDE8,
    0xFFFF9F43,
    0xFFF6D743,
    0xFF3ED0E0,
    0xFFB28CFF,
  ];
  static const bandNames = <String>['HPF', 'Low', 'Lo-Mid', 'Hi-Mid', 'High'];

  static double xOf(double f) =>
      math.log(f / fMin) / math.log(fMax / fMin) * w;

  static double yOf(double db) => (dbTop - db) / (dbTop - dbBottom) * h;

  static double fOfX(double x) => fMin * math.pow(fMax / fMin, x / w).toDouble();

  static double dbOfY(double y) => dbTop - y / h * (dbTop - dbBottom);
}

// --- RBJ biquad magnitude response @ 48 kHz (designData.ts filters/resp) ---

const double _fs = 48000;

double _biquadDb(
  double b0, double b1, double b2, double a0, double a1, double a2, double f,
) {
  final w = 2 * math.pi * f / _fs;
  final cw = math.cos(w);
  final cw2 = math.cos(2 * w);
  final sw = math.sin(w);
  final sw2 = math.sin(2 * w);
  final numRe = b0 + b1 * cw + b2 * cw2;
  final numIm = -(b1 * sw + b2 * sw2);
  final denRe = a0 + a1 * cw + a2 * cw2;
  final denIm = -(a1 * sw + a2 * sw2);
  final num = numRe * numRe + numIm * numIm;
  final den = denRe * denRe + denIm * denIm;
  if (num <= 0 || den <= 0) return 0;
  return 10 * math.log(num / den) / math.ln10;
}

double _peakingDb(double f0, double gainDb, double q, double f) {
  final a = math.pow(10, gainDb / 40).toDouble();
  final w0 = 2 * math.pi * f0 / _fs;
  final alpha = math.sin(w0) / (2 * q);
  final cw = math.cos(w0);
  return _biquadDb(1 + alpha * a, -2 * cw, 1 - alpha * a, 1 + alpha / a,
      -2 * cw, 1 - alpha / a, f);
}

double _shelfDb(bool high, double f0, double gainDb, double f) {
  // S = 0.9 shelf slope (RBJ shelf, fixed slope for display).
  const s = 0.9;
  final a = math.pow(10, gainDb / 40).toDouble();
  final w0 = 2 * math.pi * f0 / _fs;
  final cw = math.cos(w0);
  final alpha = math.sin(w0) / 2 * math.sqrt((a + 1 / a) * (1 / s - 1) + 2);
  final sq = 2 * math.sqrt(a) * alpha;
  if (high) {
    return _biquadDb(a * ((a + 1) + (a - 1) * cw + sq), -2 * a * ((a - 1) + (a + 1) * cw),
        a * ((a + 1) + (a - 1) * cw - sq), (a + 1) - (a - 1) * cw + sq,
        2 * ((a - 1) - (a + 1) * cw), (a + 1) - (a - 1) * cw - sq, f);
  }
  return _biquadDb(a * ((a + 1) - (a - 1) * cw + sq), 2 * a * ((a - 1) - (a + 1) * cw),
      a * ((a + 1) - (a - 1) * cw - sq), (a + 1) + (a - 1) * cw + sq,
      -2 * ((a - 1) + (a + 1) * cw), (a + 1) + (a - 1) * cw - sq, f);
}

double _highpassDb(double f0, double f) {
  final w0 = 2 * math.pi * f0 / _fs;
  final alpha = math.sin(w0) / (2 * 0.7071);
  final cw = math.cos(w0);
  return _biquadDb((1 + cw) / 2, -(1 + cw), (1 + cw) / 2, 1 + alpha, -2 * cw,
      1 - alpha, f);
}

/// Summed EQ magnitude curve in dB, [EqGraphGeom.n] samples log 20..20k.
List<double> eqResponseDb(ChannelEqVm eq) {
  final out = List<double>.filled(EqGraphGeom.n + 1, 0.0);
  if (!eq.on) return out;
  for (var i = 0; i <= EqGraphGeom.n; i++) {
    final f = EqGraphGeom.fOfX(EqGraphGeom.w * i / EqGraphGeom.n);
    var db = 0.0;
    if (eq.hpfOn) {
      db += _highpassDb(eq.hpfF, f);
      if (eq.hpfSlope == 24) db += _highpassDb(eq.hpfF, f);
    }
    for (final b in eq.bands) {
      if (!b.on) continue;
      if (b.band == 1) {
        db += _shelfDb(false, b.f, b.g, f);
      } else if (b.band == 4) {
        db += _shelfDb(true, b.f, b.g, f);
      } else {
        db += _peakingDb(b.f, b.g, b.q <= 0 ? 1 : b.q, f);
      }
    }
    out[i] = db.clamp(-18.0, 18.0);
  }
  return out;
}

// --- Dynamics (dynModel.ts port) ---

enum DynKind { gate, comp }

/// Dynamics snapshot for one gate/comp section.
class ChannelDynVm {
  final DynKind kind;
  final bool on;
  final double thr; // dB
  final double range; // gate only, dB
  final double att; // ms
  final double hold; // gate only, ms
  final double rel; // ms
  final double ratio; // comp only
  final double knee; // comp only, dB
  final double makeup; // comp only, dB

  const ChannelDynVm({
    required this.kind,
    this.on = true,
    this.thr = -50,
    this.range = 40,
    this.att = 0.5,
    this.hold = 50,
    this.rel = 150,
    this.ratio = 3,
    this.knee = 3,
    this.makeup = 0,
  });

  double get lo => kind == DynKind.gate ? -80 : -60;

  /// Static defaults (inputDyn.ts DYN_RESET tables).
  static ChannelDynVm resetDefault(DynKind kind) =>
      ChannelDynVm(kind: kind, on: true);
}

const dynResetGate = <String, double>{
  'thr': -50,
  'range': 40,
  'att': 0.5,
  'hold': 50,
  'rel': 150,
};
const dynResetComp = <String, double>{
  'thr': -20,
  'ratio': 3,
  'knee': 3,
  'att': 10,
  'rel': 120,
  'makeup': 0,
};

/// Gate output level for input [x] dB (downward expander with floor).
double gateOut(ChannelDynVm p, double x) {
  if (x >= p.thr) return x;
  final floor = (p.thr - p.range).clamp(p.lo, 0.0);
  final t = ((x - p.lo) / (p.thr - p.lo)).clamp(0.0, 1.0);
  return (floor + (p.thr - floor) * t * t).clamp(p.lo, x);
}

/// Comp output level for input [x] dB (soft-knee + makeup).
double compOut(ChannelDynVm p, double x) {
  final k = p.knee <= 0 ? 0.0 : p.knee;
  final yNoMake = x < p.thr - k / 2
      ? x
      : x < p.thr + k / 2
          ? x +
              (1 / p.ratio - 1) *
                  (x - p.thr + k / 2) *
                  (x - p.thr + k / 2) /
                  (2 * k)
          : p.thr + (x - p.thr) / p.ratio;
  return yNoMake + p.makeup;
}

double dynOut(ChannelDynVm p, double x) =>
    p.kind == DynKind.gate ? gateOut(p, x) : compOut(p, x);

/// 81-sample transfer curve, unit plot coords (dynModel.ts pts loop).
List<(double, double)> dynCurve(ChannelDynVm p) {
  final lo = p.lo;
  final pts = <(double, double)>[];
  for (var i = 0; i <= 80; i++) {
    final x = lo + (-lo) * i / 80;
    final y = dynOut(p, x).clamp(lo - 4, 4.0);
    pts.add((x, y));
  }
  return pts;
}

/// Gain reduction in dB for an input level (negative or zero).
double dynReductionDb(ChannelDynVm p, double levelDb) {
  if (!p.on) return 0;
  return (dynOut(p, levelDb) - levelDb).clamp(-40.0, 0.0);
}

// --- Sends / groups / source ---

/// One aux/FX send row (channelModel.ts sendRowsVm shape).
class ChannelSendVm {
  final int slot; // 1..16 desk slot
  final String destName;
  final double level; // 0..1 fader pos
  final bool muted;
  final bool? grpon; // null = unsupported leaf
  final int? tap; // desk tap enum 0..5, null = unknown
  final List<int> tapOptions;

  const ChannelSendVm({
    required this.slot,
    required this.destName,
    this.level = 0.75,
    this.muted = false,
    this.grpon,
    this.tap,
    this.tapOptions = const [0, 1, 2, 3, 4, 5],
  });
}

const tapLabels = <int, String>{
  0: 'IN',
  1: 'PRE-EQ',
  2: 'POST-EQ',
  3: 'PRE',
  4: 'POST',
  5: 'GRP',
};

/// Preamp snapshot (preamp input-VM subset).
class ChannelPreampVm {
  final bool hasInput;
  final double gain; // dB
  final double trim; // dB
  final bool phantom;
  final bool phantomArmed;
  final bool invert;
  final double delayMs;
  final bool delayOn;

  const ChannelPreampVm({
    this.hasInput = true,
    this.gain = 30,
    this.trim = 0,
    this.phantom = false,
    this.phantomArmed = false,
    this.invert = false,
    this.delayMs = 0,
    this.delayOn = false,
  });
}

/// Source card snapshot (source input-VM subset).
class ChannelSourceVm {
  final String srcName;
  final String srcGroup;
  final double levelDb;

  const ChannelSourceVm({
    this.srcName = '—',
    this.srcGroup = '',
    this.levelDb = -90,
  });
}

// --- Write helpers (leaf names match the desk param plane) ---

/// Queue an EQ band param write through the coalescer.
void writeEqParam(ChannelWrites w, int band, String param, double v) =>
    w.queue('eq${band}_$param', v);

/// Queue a dynamics param write (gate_thr / comp_ratio / ...).
void writeDynParam(ChannelWrites w, DynKind kind, String prop, double v) =>
    w.queue('${kind.name}_$prop', v);

/// Queue a send level write.
void writeSendLevel(ChannelWrites w, int slot, double v) =>
    w.queue('send${slot}_level', v);

/// Queue a send tap write.
void writeSendTap(ChannelWrites w, int slot, int tap) =>
    w.queue('send${slot}_tap', tap);

/// Leaf names for subgroup / mute-group assignment (cmdSubgroupAssign /
/// cmdMuteGroupAssign equivalents on XR-style desks).
String subgroupLeaf(int g) => 'subgroup_$g';
String muteGroupLeaf(int g) => 'mute_group_$g';
