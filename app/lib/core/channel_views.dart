// Channel DSP view-models and write helpers (EQ, dynamics, sends, source).
//
// Pure Dart: no Flutter, no Area A store. Writes go through an injected
// [ChannelWrite] so the owner can attach its own coalescing. This file never
// imports lib/core/store.

import 'dart:async';
import 'dart:math' as math;
import 'destination_controls.dart';

/// Raw engine write: leaf plus value.
typedef ChannelWrite = void Function(String leaf, Object value);

/// Coalescing writer: at most one flush per [interval]; the last queued
/// value per leaf wins.
class ChannelWrites {
  final ChannelWrite _write;
  final Duration interval;
  final Map<String, Object> _pending = {};
  Timer? _timer;

  ChannelWrites(this._write,
      {this.interval = const Duration(milliseconds: 16)});

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

/// Finite engineering-unit limits advertised by the active driver.
List<double>? parameterRange(Map<String, dynamic>? metadata) {
  final min = metadata?['min'];
  final max = metadata?['max'];
  if (min is! num ||
      max is! num ||
      !min.isFinite ||
      !max.isFinite ||
      min >= max) {
    return null;
  }
  return [min.toDouble(), max.toDouble()];
}

typedef ParameterChoice = ({double value, String label});

/// Provider-declared linear presentation; commands remain in native units.
class ParameterNumericDisplay {
  final double nativeMin, nativeMax, min, max;
  final String unit;
  const ParameterNumericDisplay(
      this.nativeMin, this.nativeMax, this.min, this.max, this.unit);

  static ParameterNumericDisplay? fromMetadata(Map<String, dynamic>? metadata) {
    final native = parameterRange(metadata);
    final display = metadata?['numeric_display'];
    if (native == null || display is! Map) return null;
    final min = display['min'], max = display['max'], unit = display['unit'];
    if (min is! num ||
        max is! num ||
        unit is! String ||
        !min.isFinite ||
        !max.isFinite ||
        min >= max) {
      return null;
    }
    return ParameterNumericDisplay(
        native.first, native.last, min.toDouble(), max.toDouble(), unit);
  }

  double toDisplay(double value) =>
      min + (value - nativeMin) / (nativeMax - nativeMin) * (max - min);
  double toNative(double value) =>
      nativeMin + (value - min) / (max - min) * (nativeMax - nativeMin);
  String label(double value) {
    final display = toDisplay(value);
    return '${display >= 0 ? '+' : ''}${display.toStringAsFixed(1)} $unit';
  }
}

List<ParameterChoice> parameterChoices(Map<String, dynamic>? metadata) {
  final rows = metadata?['choices'];
  if (rows is! List) return const [];
  return [
    for (final row in rows)
      if (row is Map &&
          row['value'] is num &&
          (row['value'] as num).isFinite &&
          row['label'] is String)
        (
          value: (row['value'] as num).toDouble(),
          label: row['label'] as String
        ),
  ];
}

/// Special numeric labels do not turn a continuous field into an enum.
String? parameterValueLabel(Map<String, dynamic>? metadata, double? value) {
  if (value == null) return null;
  for (final label
      in parameterChoices({'choices': metadata?['value_labels']})) {
    if ((label.value - value).abs() < .0001) return label.label;
  }
  return null;
}

// Three-valued evaluation: an unread dependency never authorizes an edit.
// False branches of All and true branches of Any settle without requiring
// values from inactive source slots.
bool? _presentationCondition(
    Object? condition, double? Function(String) value) {
  if (condition is! Map) return null;
  for (final key in ['all', 'any']) {
    final children = condition[key];
    if (children is! List) continue;
    if (children.isEmpty) return null;
    var unknown = false;
    for (final child in children) {
      final match = _presentationCondition(child, value);
      if (key == 'all' && match == false) return false;
      if (key == 'any' && match == true) return true;
      unknown |= match == null;
    }
    return unknown ? null : key == 'all';
  }
  final id = condition['id'];
  if (id is! String) return null;
  final actual = value(id);
  if (actual == null || !actual.isFinite) return null;
  final expected = condition['value'];
  if (expected is num && expected.isFinite) {
    return (actual - expected).abs() <= .0001;
  }
  final min = condition['min'], max = condition['max'];
  if (min is num && max is num && min.isFinite && max.isFinite && min <= max) {
    return actual >= min && actual <= max;
  }
  return null;
}

/// Resolve provider presentations against this strip's current values.
/// Unknown dependencies disable edits until hydration supplies them.
Map<String, dynamic>? parameterPresentation(
    Map<String, dynamic>? metadata, double? Function(String) value) {
  if (metadata == null) return null;
  final rows = metadata['presentations'];
  if (rows is! List || rows.isEmpty) return metadata;
  final result = Map<String, dynamic>.of(metadata);
  for (final row in rows) {
    if (row is! Map) continue;
    final when = row['when'];
    if (when != null) {
      final match = _presentationCondition(when, value);
      if (match == null) {
        result['available'] = false;
        continue;
      }
      if (!match) continue;
    }
    result['hidden'] = row['hidden'] == true;
    if (row.containsKey('numeric_display')) {
      result['numeric_display'] = row['numeric_display'];
    }
    if (row.containsKey('swatches')) result['swatches'] = row['swatches'];
    if (row.containsKey('symbols')) result['symbols'] = row['symbols'];
    if (row.containsKey('text')) result['text'] = row['text'];
    if (row.containsKey('destination')) {
      result['destination'] = row['destination'];
    }
    result['choices'] = row['choices'];
    result['value_labels'] = row['value_labels'];
    result['limiter_value'] = row['limiter_value'];
    result['curve'] = row['curve'];
  }
  return result;
}

Map<String, dynamic>? parameterTextRules(Map<String, dynamic>? metadata) {
  final presentations = metadata?['presentations'];
  final text = metadata?['text'] ??
      (presentations is List
          ? presentations
              .whereType<Map>()
              .map((p) => p['text'])
              .whereType<Map>()
              .firstOrNull
          : null);
  return text is Map ? Map<String, dynamic>.from(text) : null;
}

/// Offline simulator DSP limits, matching crates/core/src/params.rs.
/// These are simulator catalogue values, never a hardware fallback.
List<double>? canonicalSimDspRange(String leaf) {
  if (RegExp(r'^eq[1-6]_freq$').hasMatch(leaf)) return [20, 20000];
  if (RegExp(r'^eq[1-6]_gain$').hasMatch(leaf)) return [-15, 15];
  if (RegExp(r'^eq[1-6]_q$').hasMatch(leaf)) return [.3, 10];
  return switch (leaf) {
    'gain' => [-12, 60],
    'trim' => [-18, 18],
    'hpf_freq' => [20, 400],
    'gate_thr' => [-80, 0],
    'comp_thr' => [-60, 0],
    'gate_range' => [3, 60],
    'gate_att' || 'comp_att' => [0, 120],
    'gate_hold' || 'comp_hold' => [.02, 2000],
    'gate_rel' || 'comp_rel' => [5, 4000],
    'comp_ratio' => [1.1, 100],
    'comp_knee' => [0, 5],
    'comp_makeup' => [0, 24],
    _ => null,
  };
}

/// EQ drag math port (eq.ts eqParamFromDrag).
double eqParamFromDrag(
  String param,
  int band,
  double start,
  double dyPx,
  bool fine, {
  required List<double> range,
}) {
  final dy = dyPx * (fine ? 0.25 : 1.0);
  final lo = range.first;
  final hi = range.last;
  double clamp(double v) => v < lo ? lo : (v > hi ? hi : v);
  if (param == 'f') return clamp(start * math.pow(2, dy / 30).toDouble());
  if (param == 'g') return clamp((clamp(start + dy * 0.1) * 10).round() / 10);
  return clamp(
      (clamp(start * math.pow(2, dy / 60).toDouble()) * 100).round() / 100);
}

/// Graph-node drag port (eq.ts eqNodeFromPos): unit plot pos -> freq/gain.
({double f, double g}) eqNodeFromPos(double x, double y) {
  final cx = x.clamp(0.0, 1.0);
  final cy = y.clamp(0.0, 1.0);
  return (
    f: 20 * math.pow(1000, cx).toDouble(),
    g: ((15 - 30 * cy) * 10).round() / 10
  );
}

/// Spec-knob drag port (inputDyn.ts specFromDrag): normalised drag -> value.
double specFromDrag(
    double min, double max, double startN, double dyPx, bool fine) {
  final n = (startN + dyPx / 180 * (fine ? 0.25 : 1.0)).clamp(0.0, 1.0);
  return min + (max - min) * n;
}

double toN(double min, double max, double v) =>
    ((v - min) / (max - min)).clamp(0.0, 1.0);

double fromN(double min, double max, double n) =>
    min + (max - min) * n.clamp(0.0, 1.0);

enum EqBandType { low, mid, high, lowCut, highCut }

/// Independently selected display model; never the mixer's sample rate.
enum EqCutModel { unavailable, xairButterworth96k }

/// Native shape identities, using canonical/XAir presentation values.
/// A selectable shape does not imply that its response model is available.
/// Driver-native values are translated only at the store boundary.
enum EqFilterShape {
  lowCut(0),
  lowShelf(1),
  bell(2),
  veq(3),
  highShelf(4),
  highCut(5);

  const EqFilterShape(this.value);
  final int value;

  bool hasResponseModel({EqCutModel cutModel = EqCutModel.unavailable}) =>
      switch (this) {
        lowShelf || bell || highShelf => true,
        lowCut || highCut => cutModel == EqCutModel.xairButterworth96k,
        _ => false,
      };

  static EqFilterShape? fromValue(int? value) {
    for (final shape in values) {
      if (shape.value == value) return shape;
    }
    return null;
  }
}

enum EqShelfModel { standard, xair96k }

/// One EQ band view-model (bands 1..4 parametric; HPF handled separately).
class EqBandVm {
  final int band; // 1..4
  final String name;
  final double f; // Hz
  final double g; // dB
  final double q;

  /// Whether shelf response uses Q. XR18 and DL32S shelves have fixed slope;
  /// their stored bell Q must not change the shelf preview.
  final bool shelfUsesQ;

  /// Provider-qualified shelf display response.
  final EqShelfModel shelfModel;

  /// Provider-qualified display model for cut bands, independent of gain/Q.
  final EqCutModel cutModel;
  final bool on;
  final List<ParameterChoice> typeChoices;

  /// Canonical/XAir presentation enum: 1 = low shelf, 2 = bell,
  /// 4 = high shelf. Other canonical values retain their driver meaning;
  /// null means unreported or an unverified native shape. Never a DL32S enum.
  final int? type;
  final List<double>? fRange;
  final List<double>? gRange;
  final List<double>? qRange;

  bool get fixedCut =>
      cutModel == EqCutModel.xairButterworth96k &&
      (type == EqFilterShape.lowCut.value ||
          type == EqFilterShape.highCut.value);

  double get graphGain => fixedCut ? 0 : g;

  const EqBandVm({
    required this.band,
    required this.name,
    required this.f,
    required this.g,
    required this.q,
    this.shelfUsesQ = true,
    this.shelfModel = EqShelfModel.standard,
    this.cutModel = EqCutModel.unavailable,
    this.on = true,
    this.type,
    this.typeChoices = const [],
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
        shelfUsesQ: shelfUsesQ,
        shelfModel: shelfModel,
        cutModel: cutModel,
        on: on ?? this.on,
        type: type,
        typeChoices: typeChoices,
        fRange: fRange,
        gRange: gRange,
        qRange: qRange,
      );
}

class ChannelEqVm {
  final bool on;
  final double hpfF; // Hz
  final bool hpfOn;
  final List<double>? hpfRange;
  final int hpfSlope;
  final bool hpfInResponse;
  final String? responseNote;
  final List<EqBandVm> bands; // Provider-described parametric bands
  final int selectedBand; // 0 = HPF, then the provider band index

  const ChannelEqVm({
    this.on = true,
    this.hpfF = 80,
    this.hpfOn = false,
    this.hpfRange,
    this.hpfSlope = 12,
    this.hpfInResponse = true,
    this.responseNote,
    this.bands = const [],
    this.selectedBand = 1,
  });
}

/// EQ graph geometry (eqModel.ts viewBox 1000x300, N = 180 segments).
class EqGraphGeom {
  static const double w = 1000;
  static const double h = 300;
  static const int n = 180;
  static const double fMin = 20;
  static const double fMax = 20000;
  static const double dbTop = 15;
  static const double dbBottom = -15;

  /// Band node colours, index 0 is HPF (eqModel.ts EQ_BAND_COLORS).
  static const bandColors = <int>[
    0xFFD7DDE8,
    0xFFFF9F43,
    0xFFF6D743,
    0xFF3ED0E0,
    0xFFB28CFF,
  ];
  static const bandNames = <String>['HPF', 'Low', 'Lo-Mid', 'Hi-Mid', 'High'];

  static double xOf(double f) => math.log(f / fMin) / math.log(fMax / fMin) * w;

  static double yOf(double db) => (dbTop - db) / (dbTop - dbBottom) * h;

  static double fOfX(double x) =>
      fMin * math.pow(fMax / fMin, x / w).toDouble();

  static double dbOfY(double y) => dbTop - y / h * (dbTop - dbBottom);
}

// --- RBJ biquad magnitude response at 48 kHz (designData.ts filters/resp) ---

const double _fs = 48000;

double _biquadDb(
  double b0,
  double b1,
  double b2,
  double a0,
  double a1,
  double a2,
  double f,
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

double _shelfDb(bool high, double f0, double gainDb, double? q, double f) {
  // RBJ shelf response, display only. Q controls transition/resonance where
  // supported; retain the fixed S = 0.9 approximation for fixed-slope desks.
  const s = 0.9;
  final a = math.pow(10, gainDb / 40).toDouble();
  final w0 = 2 * math.pi * f0 / _fs;
  final cw = math.cos(w0);
  final alpha = q == null
      ? math.sin(w0) / 2 * math.sqrt((a + 1 / a) * (1 / s - 1) + 2)
      : math.sin(w0) / (2 * (q.isFinite && q > 0 ? q : 1));
  final sq = 2 * math.sqrt(a) * alpha;
  if (high) {
    return _biquadDb(
        a * ((a + 1) + (a - 1) * cw + sq),
        -2 * a * ((a - 1) + (a + 1) * cw),
        a * ((a + 1) + (a - 1) * cw - sq),
        (a + 1) - (a - 1) * cw + sq,
        2 * ((a - 1) - (a + 1) * cw),
        (a + 1) - (a - 1) * cw - sq,
        f);
  }
  return _biquadDb(
      a * ((a + 1) - (a - 1) * cw + sq),
      2 * a * ((a - 1) - (a + 1) * cw),
      a * ((a + 1) - (a - 1) * cw - sq),
      (a + 1) + (a - 1) * cw + sq,
      -2 * ((a - 1) + (a + 1) * cw),
      (a + 1) + (a - 1) * cw - sq,
      f);
}

// X AIR Edit's graph uses a 96 kHz bilinear mapping and fixed damping.
double _xairShelfDb(bool high, double f0, double gainDb, double f) {
  if (!f0.isFinite || f0 <= 0 || f0 >= 48000 || !gainDb.isFinite) return 0;
  final ratio = math.tan(math.pi * f / 96000) / math.tan(math.pi * f0 / 96000);
  final t = high ? ratio : 1 / ratio;
  final t2 = t * t;
  final t4 = t2 * t2;
  final gainPower = math.pow(10, gainDb.abs() / 10).toDouble();
  return gainDb.sign * 10 * math.log((1 + gainPower * t4) / (1 + t4)) / math.ln10;
}

double _highpassDb(double f0, double f) {
  final w0 = 2 * math.pi * f0 / _fs;
  final alpha = math.sin(w0) / (2 * 0.7071);
  final cw = math.cos(w0);
  return _biquadDb(
      (1 + cw) / 2, -(1 + cw), (1 + cw) / 2, 1 + alpha, -2 * cw, 1 - alpha, f);
}

// Fixed second-order Butterworth cut, using the vendor graph's 96 kHz
// bilinear frequency mapping. This is display maths, not the desk clock rate.
double _xairCutDb(bool highpass, double f0, double f) {
  if (!f0.isFinite || f0 <= 0 || f0 >= 48000) return 0;
  final ratio = math.tan(math.pi * f / 96000) / math.tan(math.pi * f0 / 96000);
  final power4 = ratio * ratio * ratio * ratio;
  final power = highpass ? power4 / (1 + power4) : 1 / (1 + power4);
  return 10 * math.log(power) / math.ln10;
}

/// Summed EQ magnitude curve in dB over [EqGraphGeom.n] log-spaced samples.
List<double> eqResponseDb(ChannelEqVm eq) {
  final out = List<double>.filled(EqGraphGeom.n + 1, 0.0);
  if (!eq.on) return out;
  for (var i = 0; i <= EqGraphGeom.n; i++) {
    final f = EqGraphGeom.fOfX(EqGraphGeom.w * i / EqGraphGeom.n);
    var db = 0.0;
    if (eq.hpfOn && eq.hpfInResponse) {
      db += _highpassDb(eq.hpfF, f);
      if (eq.hpfSlope == 24) db += _highpassDb(eq.hpfF, f);
    }
    for (final b in eq.bands) {
      if (!b.on) continue;
      switch (EqFilterShape.fromValue(b.type)) {
        case EqFilterShape.lowShelf:
          db += b.shelfModel == EqShelfModel.xair96k
              ? _xairShelfDb(false, b.f, b.g, f)
              : _shelfDb(false, b.f, b.g, b.shelfUsesQ ? b.q : null, f);
        case EqFilterShape.bell:
          db += _peakingDb(b.f, b.g, b.q <= 0 ? 1 : b.q, f);
        case EqFilterShape.highShelf:
          db += b.shelfModel == EqShelfModel.xair96k
              ? _xairShelfDb(true, b.f, b.g, f)
              : _shelfDb(true, b.f, b.g, b.shelfUsesQ ? b.q : null, f);
        case EqFilterShape.lowCut:
          if (b.cutModel == EqCutModel.xairButterworth96k) {
            db += _xairCutDb(true, b.f, f);
          }
        case EqFilterShape.highCut:
          if (b.cutModel == EqCutModel.xairButterworth96k) {
            db += _xairCutDb(false, b.f, f);
          }
        case EqFilterShape.veq:
        case null:
          // No verified response to draw until the shape is known.
          break;
      }
    }
    out[i] = db.clamp(-18.0, 18.0);
  }
  return out;
}

// --- Dynamics (dynModel.ts port) ---

enum DynKind { gate, comp }

/// Independent measured processor input/output and positive reduction dB.
class ChannelDynamicsMetersVm {
  final double gateIn, gateReduction, gateOut, compIn, compReduction, compOut;
  const ChannelDynamicsMetersVm(
      {required this.gateIn,
      required this.gateReduction,
      required this.gateOut,
      required this.compIn,
      required this.compReduction,
      required this.compOut});
}

/// Dynamics snapshot for one gate or comp section.
class ChannelDynVm {
  final DynKind kind;
  final String? responseCurve;
  final Map<String, List<double>> ranges;
  final bool on;
  final double thr; // dB
  final double range; // gate only, dB
  final double att; // ms
  final double hold; // gate only, ms
  final double rel; // ms
  final bool ratioIsLimit;
  final double ratio; // comp only
  final double knee; // comp only, dB
  final double makeup; // comp only, dB
  final int? mode;
  final int? detector;
  final int? envelope;
  final int? keySource;
  final bool? filterOn;
  final int? filterType;
  final double? filterFreq;
  final double? mix;
  final bool? auto;
  final bool? softKnee; // Driver-specific enum interpretation.

  const ChannelDynVm({
    required this.kind,
    this.responseCurve,
    this.ranges = const {},
    this.on = true,
    this.thr = -50,
    this.range = 40,
    this.att = 0.5,
    this.hold = 50,
    this.rel = 150,
    this.ratioIsLimit = false,
    this.ratio = 3,
    this.knee = 3,
    this.makeup = 0,
    this.mode,
    this.detector,
    this.envelope,
    this.keySource,
    this.filterOn,
    this.filterType,
    this.filterFreq,
    this.mix,
    this.auto,
    this.softKnee,
  });

  double get lo => kind == DynKind.gate ? -80 : -60;

  /// Static defaults (inputDyn.ts DYN_RESET tables).
  static ChannelDynVm resetDefault(DynKind kind) => ChannelDynVm(
        kind: kind,
        thr: kind == DynKind.gate ? -50 : -20,
        att: kind == DynKind.gate ? 0.5 : 10,
        rel: kind == DynKind.gate ? 150 : 120,
      );
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

/// Configured Gate output for input [x] dB. Providers may advertise a
/// verified attenuation response; otherwise retain the legacy expander preview.
double gateOut(ChannelDynVm p, double x) {
  if (x >= p.thr) return x;
  if (p.responseCurve == 'gate_attenuation') return x - p.range;
  final floor = (p.thr - p.range).clamp(p.lo, 0.0);
  final t = ((x - p.lo) / (p.thr - p.lo)).clamp(0.0, 1.0);
  return (floor + (p.thr - floor) * t * t).clamp(p.lo, x);
}

/// Comp output level for input [x] dB (soft-knee + makeup).
double compOut(ChannelDynVm p, double x) {
  final k = p.responseCurve == 'hard_knee' || p.knee <= 0 ? 0.0 : p.knee;
  final inverseRatio = p.ratioIsLimit ? 0.0 : 1 / p.ratio;
  final yNoMake = x < p.thr - k / 2
      ? x
      : x < p.thr + k / 2
          ? x +
              (inverseRatio - 1) *
                  (x - p.thr + k / 2) *
                  (x - p.thr + k / 2) /
                  (2 * k)
          : p.thr + (x - p.thr) * inverseRatio;
  return yNoMake + p.makeup;
}

double dynOut(ChannelDynVm p, double x) =>
    p.kind == DynKind.gate ? gateOut(p, x) : compOut(p, x);

/// Transfer curve in dB. Verified step responses preserve the threshold jump;
/// other responses use the existing 81-sample curve.
List<(double, double)> dynCurve(ChannelDynVm p) {
  final lo = p.lo;
  if (p.kind == DynKind.gate && p.responseCurve == 'gate_attenuation') {
    final start = lo + p.range;
    return [
      if (start < p.thr) ...[(start, lo), (p.thr, p.thr - p.range)] else
        (p.thr, lo),
      (p.thr, p.thr),
      (0, 0),
    ];
  }
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

/// One provider-described send row (bus, matrix, main or FX).
class ChannelSendVm {
  final List<DestinationControlVm> destinationControls;
  final int slot; // Provider-defined 1-based send slot
  final String destName;
  final double level; // 0..1 fader pos
  final bool muted;
  final bool canMute;
  final double? pan; // -1..1; null unless a stereo destination is known.
  final bool? grpon; // null = unsupported leaf
  final int? tap; // desk tap enum 0..5, null = unknown
  final List<int> tapOptions;

  const ChannelSendVm({
    this.destinationControls = const [],
    required this.slot,
    required this.destName,
    this.level = 0.75,
    this.muted = false,
    this.canMute = false,
    this.pan,
    this.grpon,
    this.tap,
    this.tapOptions = const [],
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

/// Captured shared source and mode, bound to the current input/connection.
class ChannelSourceModeVm {
  final Object context;
  final String target, sourceLabel;
  final int mode;
  final bool editable;
  const ChannelSourceModeVm(
      {required this.context,
      required this.target,
      required this.sourceLabel,
      required this.mode,
      required this.editable});
  String get code => const ['M', 'ST', 'M/S'][mode];
  String get label => const ['Mono', 'Stereo', 'Mid/side'][mode];
}

/// One coherent native delay snapshot, bound to strip, unit and live domain.
class ChannelDelayVm {
  final Object context;
  final int unit, steps;
  final double value, min, max;
  final bool on, editable;
  const ChannelDelayVm(
      {required this.context,
      required this.unit,
      required this.steps,
      required this.value,
      required this.min,
      required this.max,
      required this.on,
      required this.editable});
  String get code => const ['M', 'FT', 'MS', 'SMP'][unit];
  String get unitLabel => const ['m', 'ft', 'ms', 'samples'][unit];
}

class ChannelPreampVm {
  final bool hasInput;
  final double gain; // dB
  final double trim; // dB
  final bool phantom;
  final bool invert;
  final double delayMs;
  final bool delayOn;

  const ChannelPreampVm({
    this.hasInput = true,
    this.gain = 30,
    this.trim = 0,
    this.phantom = false,
    this.invert = false,
    this.delayMs = 0,
    this.delayOn = false,
  });
}

class ChannelSourceVm {
  final int? sourceA; // Opaque native routing IDs; do not infer patch labels.
  final int? sourceB;
  final int? selected; // DL32S: 0 = A, 1 = B.
  final int? usbReturn; // XR18 USB return channel; not an A/B selector.
  final String srcName;
  final String srcGroup;
  final double levelDb;

  const ChannelSourceVm({
    this.sourceA,
    this.sourceB,
    this.selected,
    this.usbReturn,
    this.srcName = '—',
    this.srcGroup = '',
    this.levelDb = -90,
  });
}

// Write helpers. Leaf names match the desk param plane.

void writeEqParam(ChannelWrites w, int band, String param, double v) =>
    w.queue('eq${band}_$param', v);

void writeDynParam(ChannelWrites w, DynKind kind, String prop, double v) =>
    w.queue('${kind.name}_$prop', v);

void writeSendLevel(ChannelWrites w, int slot, double v) =>
    w.queue('send${slot}_level', v);

void writeSendTap(ChannelWrites w, int slot, int tap) =>
    w.queue('send${slot}_tap', tap);

/// Leaf names for subgroup and mute-group assignment (cmdSubgroupAssign and
/// cmdMuteGroupAssign equivalents on XR-style desks).
String subgroupLeaf(int g) => 'subgroup_$g';
String muteGroupLeaf(int g) => 'mute_group_$g';
