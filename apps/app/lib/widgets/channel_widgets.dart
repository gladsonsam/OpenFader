import '../palette.dart';
// Channel DSP widgets: custom-painted EQ curve + RTA overlay, dyn transfer
// curves + GR meter, touch knobs and section cards. No Material form
// widgets: every DSP control is a CustomPainter + GestureDetector in ui4
// dark tokens (see lib/theme.dart).
//
// Contents:
// - EQ graph + RTA overlay (log-freq curve, RTA area overlay)
// - dyn transfer graph + GR bar (transfer curve, GR bar)
// - Preamp / Band / Hpf knob rows (vertical-drag knobs)

import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../core/channel_views.dart';
import '../design.dart';
import '../theme.dart';
import 'console_button.dart';

// ---------------------------------------------------------------------------
// Section card + small building blocks
// ---------------------------------------------------------------------------

/// Shared rounded processing card on a graphite surface.
class U4Section extends StatelessWidget {
  final String title;
  final Widget child;
  final Widget? trailing;

  const U4Section(
      {super.key, required this.title, required this.child, this.trailing});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(color: ConsolePalette.of(context).linePanel),
        borderRadius: U4.rBtn,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(title,
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 12,
                        color: ConsolePalette.of(context).text7)),
              ),
              if (trailing != null) trailing!,
            ],
          ),
          const SizedBox(height: 10),
          child,
        ],
      ),
    );
  }
}

/// Small on/off chip (custom painted toggle — no Switch widget).
class U4Chip extends StatelessWidget {
  final String label;
  final bool on;
  final bool enabled;
  final VoidCallback? onTap;
  final Color? accent;

  const U4Chip({
    super.key,
    required this.label,
    required this.on,
    this.enabled = true,
    this.onTap,
    this.accent,
  });

  @override
  Widget build(BuildContext context) {
    return Semantics(
        toggled: on,
        child: ConsoleButton(
            label: label,
            selected: on,
            accent: accent,
            onPressed: enabled ? onTap : null));
  }
}

// ---------------------------------------------------------------------------
// Touch knob (vertical drag + double-tap reset, no Slider)
// ---------------------------------------------------------------------------

/// Touch knob: vertical drag edits (spec math via [specFromDrag]), double-tap
/// resets to [defaultValue]. Paints arc + pointer + value text.
class U4Knob extends StatefulWidget {
  final String label;
  final double value;
  final double min;
  final double max;
  final String text;
  final double defaultValue;
  final Color? color;
  final bool fine;
  final bool enabled;
  final ValueChanged<double> onInput;

  /// Reports user focus (tap / drag start / double-tap) so the page's top
  /// readout can track this knob's numeric value. Never changes the value.
  final VoidCallback? onFocus;

  const U4Knob({
    super.key,
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    required this.text,
    required this.defaultValue,
    required this.onInput,
    this.color,
    this.fine = false,
    this.enabled = true,
    this.onFocus,
  });

  @override
  State<U4Knob> createState() => _U4KnobState();
}

class _U4KnobState extends State<U4Knob> {
  double? _startN;
  double _dragDy = 0;

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: '${widget.label} knob, ${widget.text}',
      onTap: widget.enabled ? widget.onFocus : null,
      child: GestureDetector(
        onTap: widget.enabled ? widget.onFocus : null,
        onVerticalDragStart: widget.enabled
            ? (d) {
                widget.onFocus?.call();
                _startN = toN(widget.min, widget.max, widget.value);
                _dragDy = 0;
              }
            : null,
        onVerticalDragUpdate: widget.enabled
            ? (d) {
                final n0 = _startN ?? toN(widget.min, widget.max, widget.value);
                _dragDy -= d.delta.dy;
                final v = specFromDrag(
                    widget.min, widget.max, n0, _dragDy, widget.fine);
                widget.onInput(v);
              }
            : null,
        onVerticalDragEnd: widget.enabled ? (_) => _startN = null : null,
        onVerticalDragCancel: widget.enabled ? () => _startN = null : null,
        onDoubleTap: widget.enabled
            ? () {
                widget.onFocus?.call();
                widget
                    .onInput(widget.defaultValue.clamp(widget.min, widget.max));
              }
            : null,
        child: Opacity(
          opacity: widget.enabled ? 1 : 0.45,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CustomPaint(
                size: Size(AppDesign.largeTargets ? 80 : 68,
                    AppDesign.largeTargets ? 80 : 68),
                painter: _KnobPainter(
                  palette: ConsolePalette.of(context),
                  n: toN(widget.min, widget.max, widget.value),
                  color: widget.color ?? ConsolePalette.of(context).meterGreen,
                ),
              ),
              const SizedBox(height: 4),
              Text(widget.label,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 11,
                      color: ConsolePalette.of(context).text7)),
              Text(widget.text,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 12,
                      color: ConsolePalette.of(context).text1,
                      fontFeatures: [FontFeature.tabularFigures()])),
            ],
          ),
        ),
      ),
    );
  }
}

class _KnobPainter extends CustomPainter {
  final ConsolePalette palette;
  final double n;
  final Color color;

  _KnobPainter({required this.palette, required this.n, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final r = size.width / 2 - 4;
    const sweep0 = math.pi * 0.75;
    const sweep1 = math.pi * 2.25;
    final bg = Paint()
      ..color = palette.railBorder
      ..style = PaintingStyle.stroke
      ..strokeWidth = 5
      ..strokeCap = StrokeCap.round;
    canvas.drawArc(Rect.fromCircle(center: c, radius: r), sweep0,
        sweep1 - sweep0, false, bg);
    final fg = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 5
      ..strokeCap = StrokeCap.round;
    final double nn = n.clamp(0.0, 1.0).toDouble();
    canvas.drawArc(Rect.fromCircle(center: c, radius: r), sweep0,
        (sweep1 - sweep0) * nn, false, fg);
    final a = sweep0 + (sweep1 - sweep0) * nn;
    final p = Paint()
      ..color = color
      ..strokeWidth = 3
      ..strokeCap = StrokeCap.round;
    canvas.drawLine(c, c + Offset(math.cos(a), math.sin(a)) * (r - 4), p);
    canvas.drawCircle(c, 3, Paint()..color = palette.text1);
  }

  @override
  bool shouldRepaint(covariant _KnobPainter old) =>
      old.palette != palette || old.n != n || old.color != color;
}

// ---------------------------------------------------------------------------
// EQ curve + RTA overlay
// ---------------------------------------------------------------------------

/// EQ response painter: grid + summed curve + band nodes + live RTA area
/// overlay (rtaOverlay.ts: peak-hold handled by the owner feeding [rtaBins]).
class EqCurvePainter extends CustomPainter {
  final ConsolePalette palette;
  final List<double> responseDb; // EqGraphGeom.n + 1 samples
  final ChannelEqVm eq;
  final List<double>? rtaBins; // 0..1 magnitudes, RTA_BINS wide
  final bool rtaActive;

  EqCurvePainter({
    required this.palette,
    required this.responseDb,
    required this.eq,
    this.rtaBins,
    this.rtaActive = false,
  });

  static const _freqTicks = <double>[
    20,
    50,
    100,
    200,
    500,
    1000,
    2000,
    5000,
    10000,
    20000
  ];
  static const _dbTicks = <double>[15, 10, 5, 0, -5, -10, -15];

  @override
  void paint(Canvas canvas, Size size) {
    final sx = size.width / EqGraphGeom.w;
    final sy = size.height / EqGraphGeom.h;
    void label(String text, Offset position,
        {Color? color, double font = 10, bool centered = false}) {
      final painter = TextPainter(
          text: TextSpan(
              text: text,
              style: AppDesign.type(font,
                  color: color ?? palette.text7, weight: FontWeight.w500)),
          textDirection: TextDirection.ltr)
        ..layout();
      painter.paint(
          canvas,
          position -
              Offset(centered ? painter.width / 2 : 0, painter.height / 2));
    }

    final grid = Paint()
      ..color = palette.linePanel
      ..strokeWidth = 1;
    for (var decade = 20.0; decade < 20000; decade *= 10) {
      for (var step = 1; step < 10; step++) {
        final f = decade * step;
        if (f > 20000) continue;
        final x = EqGraphGeom.xOf(f) * sx;
        canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
      }
    }
    for (final db in _dbTicks) {
      final y = EqGraphGeom.yOf(db) * sy;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), grid);
      label(db.toStringAsFixed(0), Offset(-24, y));
    }
    for (final f in _freqTicks) {
      if (size.width < 500 && ![20, 100, 1000, 10000, 20000].contains(f)) {
        continue;
      }
      final x = (EqGraphGeom.xOf(f) * sx).clamp(6.0, size.width - 8);
      label(
          f >= 1000
              ? '${(f / 1000).toStringAsFixed(0)}k'
              : f.toStringAsFixed(0),
          Offset(x, size.height + 16),
          centered: true);
    }
    // Individual band contributions use the same DSP response as the sum.
    for (final band in eq.bands.where((b) => b.on)) {
      final samples = eqResponseDb(ChannelEqVm(bands: [band]));
      final area = Path()..moveTo(0, size.height / 2);
      for (var i = 0; i < samples.length; i++) {
        area.lineTo(i / (samples.length - 1) * size.width,
            EqGraphGeom.yOf(samples[i]) * sy);
      }
      area.lineTo(size.width, size.height / 2);
      area.close();
      canvas.drawPath(area,
          Paint()..color = palette.bandColor(band.band).withValues(alpha: .25));
    }
    // RTA overlay under the curve (verified audio_spectrum frames only;
    // the owner hides it when bins carry no signal).
    final bins = rtaBins;
    if (rtaActive && bins != null && bins.isNotEmpty) {
      final path = Path()..moveTo(0, size.height);
      for (var i = 0; i < bins.length; i++) {
        final f = 20 * math.pow(1000, i / (bins.length - 1)).toDouble();
        final x = EqGraphGeom.xOf(f.clamp(20, 20000)) * sx;
        final h = bins[i].clamp(0.0, 1.0) * size.height * 0.63;
        path.lineTo(x, size.height - h);
      }
      path.lineTo(size.width, size.height);
      path.close();
      canvas.drawPath(
          path, Paint()..color = palette.meter.withValues(alpha: .2));
    }
    // Summed curve + fill to 0 dB.
    if (responseDb.isNotEmpty) {
      final curve = Path();
      final fill = Path();
      final zeroY = EqGraphGeom.yOf(0) * sy;
      for (var i = 0; i < responseDb.length; i++) {
        final x = EqGraphGeom.w * i / (responseDb.length - 1) * sx;
        final y = EqGraphGeom.yOf(responseDb[i]) * sy;
        if (i == 0) {
          curve.moveTo(x, y);
          fill.moveTo(x, zeroY);
          fill.lineTo(x, y);
        } else {
          curve.lineTo(x, y);
          fill.lineTo(x, y);
        }
      }
      final lastX = size.width;
      fill.lineTo(lastX, zeroY);
      fill.close();
      canvas.drawPath(
          fill, Paint()..color = palette.text7.withValues(alpha: .12));
      canvas.drawPath(
        curve,
        Paint()
          ..color = palette.text1
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    void node(Offset point, int band, bool on) {
      final color = palette.bandColor(band);
      final radius = size.width < 500 ? 15.0 : 22.0;
      canvas.drawCircle(point, radius, Paint()..color = palette.chip);
      canvas.drawCircle(
          point,
          radius,
          Paint()
            ..color = color.withValues(alpha: on ? 1 : .35)
            ..style = PaintingStyle.stroke
            ..strokeWidth = eq.selectedBand == band ? 3 : 2);
      label(band == 0 ? 'HPF' : '$band', point,
          color: palette.text1, font: band == 0 ? 9 : 14, centered: true);
    }

    for (final band in eq.bands) {
      node(
          Offset(EqGraphGeom.xOf(band.f.clamp(20, 20000)) * sx,
              EqGraphGeom.yOf(band.g.clamp(-15, 15)) * sy),
          band.band,
          band.on);
    }
    node(
        Offset(EqGraphGeom.xOf(eq.hpfF.clamp(20, 1000)) * sx,
            EqGraphGeom.yOf(0) * sy),
        0,
        eq.hpfOn);
  }

  @override
  bool shouldRepaint(covariant EqCurvePainter old) =>
      old.palette != palette ||
      old.responseDb != responseDb ||
      old.eq != eq ||
      old.rtaBins != rtaBins ||
      old.rtaActive != rtaActive;
}

/// Interactive EQ graph: tap to select, drag frequency/gain, pinch Q.
/// HPF drags edit frequency only.
class U4EqGraph extends StatelessWidget {
  final ChannelEqVm eq;
  final List<double> responseDb;
  final List<double>? rtaBins;
  final bool rtaActive;
  final double height;
  final ValueChanged<int> onSelectBand;
  final void Function(int band, double f, double g) onNode;
  final void Function(int band, double q) onQ;

  const U4EqGraph({
    super.key,
    required this.eq,
    required this.responseDb,
    this.rtaBins,
    this.rtaActive = false,
    this.height = 344,
    required this.onSelectBand,
    required this.onNode,
    required this.onQ,
  });

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final outerWidth =
            constraints.maxWidth.isFinite ? constraints.maxWidth : 340.0;
        final w = outerWidth - 44;
        final h = height - 44;
        return Semantics(
            container: true,
            label: 'EQ graph',
            hint:
                'Drag a band to change frequency and gain. Pinch to change bandwidth.',
            child: Container(
                height: height,
                padding: const EdgeInsets.fromLTRB(28, 14, 16, 30),
                decoration: BoxDecoration(
                    color: ConsolePalette.of(context).panel,
                    borderRadius: U4.rWell,
                    border: Border.all(
                        color: ConsolePalette.of(context).linePanel)),
                child: _EqGraphGesture(
                  width: w,
                  height: h,
                  eq: eq,
                  onSelectBand: onSelectBand,
                  onNode: onNode,
                  onQ: onQ,
                  child: CustomPaint(
                    size: Size(w, h),
                    painter: EqCurvePainter(
                      palette: ConsolePalette.of(context),
                      responseDb: responseDb,
                      eq: eq,
                      rtaBins: rtaBins,
                      rtaActive: rtaActive,
                    ),
                  ),
                )));
      },
    );
  }
}

class _EqGraphGesture extends StatefulWidget {
  final double width;
  final double height;
  final ChannelEqVm eq;
  final ValueChanged<int> onSelectBand;
  final void Function(int band, double f, double g) onNode;
  final void Function(int band, double q) onQ;
  final Widget child;

  const _EqGraphGesture({
    required this.width,
    required this.height,
    required this.eq,
    required this.onSelectBand,
    required this.onNode,
    required this.onQ,
    required this.child,
  });

  @override
  State<_EqGraphGesture> createState() => _EqGraphGestureState();
}

class _EqGraphGestureState extends State<_EqGraphGesture> {
  final Map<int, Offset> _pointers = {};
  int? _dragBand;
  double _f = 20;
  double _g = 0;
  double _q = 1;
  double? _span;

  EqBandVm? get _band {
    for (final band in widget.eq.bands) {
      if (band.band == _dragBand) return band;
    }
    return null;
  }

  int? _hitBand(Offset local) {
    int? best;
    var bestD = 30.0;
    for (final b in widget.eq.bands) {
      final node = Offset(
        EqGraphGeom.xOf(b.f.clamp(20, 20000)) / EqGraphGeom.w * widget.width,
        EqGraphGeom.yOf(b.g.clamp(-15, 15)) / EqGraphGeom.h * widget.height,
      );
      final d = (node - local).distance;
      if (d < bestD) {
        bestD = d;
        best = b.band;
      }
    }
    final hpf = Offset(
      EqGraphGeom.xOf(widget.eq.hpfF.clamp(20, 1000)) /
          EqGraphGeom.w *
          widget.width,
      widget.height / 2,
    );
    if ((hpf - local).distance < bestD) best = 0;
    return best;
  }

  void _focus(int? band) {
    _dragBand = band;
    if (band == null) return;
    if (band != 0 && _band == null) {
      _dragBand = null;
      return;
    }
    _f = band == 0 ? widget.eq.hpfF : (_band?.f ?? 20);
    _g = band == 0 ? 0 : (_band?.g ?? 0);
    _q = _band?.q ?? 1;
    widget.onSelectBand(band);
  }

  double? get _pointerSpan {
    if (_pointers.length != 2) return null;
    return (_pointers.values.first - _pointers.values.last).distance;
  }

  void _down(PointerDownEvent event) {
    if (_pointers.isEmpty) _focus(_hitBand(event.localPosition));
    _pointers[event.pointer] = event.localPosition;
    if (_pointers.length == 2 && _dragBand == null) {
      _focus(widget.eq.selectedBand);
    }
    _span = _pointerSpan;
  }

  void _move(PointerMoveEvent event) {
    final previous = _pointers[event.pointer];
    if (previous == null) return;
    _pointers[event.pointer] = event.localPosition;
    final band = _dragBand;
    if (band == null) return;
    if (_pointers.length == 2) {
      final span = _pointerSpan!;
      final oldSpan = _span;
      _span = span;
      if (band == 0 || oldSpan == null || oldSpan < 1 || span < 1) return;
      final range = _band?.qRange;
      // Spread fingers to broaden the band (lower Q).
      _q = (_q * oldSpan / span).clamp(range?[0] ?? 0.3, range?[1] ?? 16.0);
      widget.onQ(band, _q);
    } else if (_pointers.length == 1 && widget.width > 0 && widget.height > 0) {
      final delta = event.localPosition - previous;
      final fRange = _band?.fRange;
      final gRange = _band?.gRange;
      _f = (_f * math.pow(1000, delta.dx / widget.width))
          .clamp(
            band == 0 ? 20.0 : (fRange?[0] ?? 20.0),
            band == 0 ? 1000.0 : (fRange?[1] ?? 20000.0),
          )
          .toDouble();
      _g = band == 0
          ? 0
          : (_g - delta.dy / widget.height * 30)
              .clamp(gRange?[0] ?? -15.0, gRange?[1] ?? 15.0);
      widget.onNode(band, _f, _g);
    }
  }

  void _up(PointerUpEvent event) {
    _pointers.remove(event.pointer);
    _span = _pointerSpan;
    if (_pointers.isEmpty) _dragBand = null;
  }

  void _cancel(PointerCancelEvent event) {
    _pointers.clear();
    _dragBand = null;
    _span = null;
  }

  @override
  Widget build(BuildContext context) {
    return U4GraphTouchSurface(
      onDown: _down,
      onMove: _move,
      onUp: _up,
      onCancel: _cancel,
      child: widget.child,
    );
  }
}

/// Graph touches belong to the graph from pointer-down, so an enclosing
/// scroll view cannot win the gesture arena halfway through a node edit.
class U4GraphTouchSurface extends StatelessWidget {
  final PointerDownEventListener onDown;
  final PointerMoveEventListener onMove;
  final PointerUpEventListener onUp;
  final PointerCancelEventListener onCancel;
  final Widget child;

  const U4GraphTouchSurface({
    super.key,
    required this.onDown,
    required this.onMove,
    required this.onUp,
    required this.onCancel,
    required this.child,
  });

  @override
  Widget build(BuildContext context) => RawGestureDetector(
        behavior: HitTestBehavior.opaque,
        gestures: {
          EagerGestureRecognizer:
              GestureRecognizerFactoryWithHandlers<EagerGestureRecognizer>(
            EagerGestureRecognizer.new,
            (_) {},
          ),
        },
        child: Listener(
          behavior: HitTestBehavior.opaque,
          onPointerDown: onDown,
          onPointerMove: onMove,
          onPointerUp: onUp,
          onPointerCancel: onCancel,
          child: child,
        ),
      );
}

// ---------------------------------------------------------------------------
// Dynamics transfer curve + GR bar
// ---------------------------------------------------------------------------

/// Transfer-curve painter (dynModel.ts: lo..0 axes, gold curve, threshold
/// line + tag, operating dot, unity diagonal).
class DynCurvePainter extends CustomPainter {
  final ConsolePalette palette;
  final ChannelDynVm dyn;
  final double? levelDb; // null when frozen: dot parks at lo
  final Color color;

  DynCurvePainter(
      {required this.palette,
      required this.dyn,
      this.levelDb,
      required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final lo = dyn.lo;
    double px(double v) => (v - lo) / -lo * size.width;
    double py(double v) => size.height - (v - lo) / -lo * size.height;

    // Grid every 10 dB.
    final grid = Paint()
      ..color = palette.linePanel
      ..strokeWidth = 1;
    for (var v = lo + 10; v < 0; v += 10) {
      canvas.drawLine(Offset(px(v), 0), Offset(px(v), size.height), grid);
      canvas.drawLine(Offset(0, py(v)), Offset(size.width, py(v)), grid);
    }
    // Unity diagonal.
    canvas.drawLine(
        Offset(px(lo), py(lo)),
        Offset(px(0), py(0)),
        Paint()
          ..color = palette.lineCtl
          ..strokeWidth = 1);
    // Fill + curve.
    final pts = dynCurve(dyn);
    if (pts.isNotEmpty) {
      final curve = Path();
      final fill = Path();
      for (var i = 0; i < pts.length; i++) {
        final o = Offset(px(pts[i].$1), py(pts[i].$2.clamp(lo, 0)));
        if (i == 0) {
          curve.moveTo(o.dx, o.dy);
          fill.moveTo(o.dx, size.height);
          fill.lineTo(o.dx, o.dy);
        } else {
          curve.lineTo(o.dx, o.dy);
          fill.lineTo(o.dx, o.dy);
        }
      }
      fill.lineTo(size.width, size.height);
      fill.close();
      canvas.drawPath(fill, Paint()..color = color.withValues(alpha: 0.11));
      canvas.drawPath(
        curve,
        Paint()
          ..color = palette.cyan
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    // Threshold line + tag.
    if (dyn.thr.isFinite) {
      final x = px(dyn.thr.clamp(lo, 0));
      canvas.drawLine(
          Offset(x, 0),
          Offset(x, size.height),
          Paint()
            ..color = color
            ..strokeWidth = 1.5);
      final tp = TextPainter(
        text: TextSpan(
            text: '${dyn.thr.toStringAsFixed(1)} dB',
            style: TextStyle(
                fontFamily: U4.font, fontSize: 10, color: palette.text1)),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, Offset((x + 4).clamp(0, size.width - tp.width), 2));
    }
    // Operating dot (SPEC 2a: live level reader; frozen parks at lo).
    final lv = (levelDb ?? lo).clamp(lo, 0.0);
    final dot = Offset(px(lv), py(dynOut(dyn, lv).clamp(lo, 0)));
    canvas.drawCircle(dot, 6, Paint()..color = color);
    canvas.drawCircle(
        dot,
        6,
        Paint()
          ..color = palette.text1
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5);
  }

  @override
  bool shouldRepaint(covariant DynCurvePainter old) =>
      old.palette != palette ||
      old.dyn != dyn ||
      old.levelDb != levelDb ||
      old.color != color;
}

/// Interactive transfer graph: horizontal drag edits threshold (both
/// kinds); comp vertical drag edits ratio. No sliders.
class U4DynGraph extends StatefulWidget {
  final ChannelDynVm dyn;
  final double? levelDb;
  final Color color;
  final ValueChanged<double> onThreshold;
  final ValueChanged<double>? onRatio;

  const U4DynGraph({
    super.key,
    required this.dyn,
    this.levelDb,
    required this.color,
    required this.onThreshold,
    this.onRatio,
  });

  @override
  State<U4DynGraph> createState() => _U4DynGraphState();
}

class _U4DynGraphState extends State<U4DynGraph> {
  final Map<int, Offset> _pointers = {};
  double _threshold = 0;
  double _ratio = 1;

  void _down(PointerDownEvent event) {
    if (_pointers.isEmpty) {
      _threshold = widget.dyn.thr;
      _ratio = widget.dyn.ratio;
    }
    _pointers[event.pointer] = event.localPosition;
  }

  void _move(PointerMoveEvent event, Size size) {
    final previous = _pointers[event.pointer];
    if (previous == null) return;
    _pointers[event.pointer] = event.localPosition;
    if (_pointers.length != 1 || size.isEmpty) return;
    final delta = event.localPosition - previous;
    if (delta.dx != 0) {
      _threshold = (_threshold + delta.dx / size.width * -widget.dyn.lo)
          .clamp(widget.dyn.lo, 0.0);
      widget.onThreshold(_threshold);
    }
    if (delta.dy != 0 &&
        widget.dyn.kind == DynKind.comp &&
        widget.onRatio != null) {
      _ratio = (_ratio - delta.dy / size.height * 19).clamp(1.0, 20.0);
      widget.onRatio!(_ratio);
    }
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(builder: (context, constraints) {
      final size =
          Size(constraints.maxWidth.isFinite ? constraints.maxWidth : 340, 170);
      return U4GraphTouchSurface(
        onDown: _down,
        onMove: (event) => _move(event, size),
        onUp: (event) => _pointers.remove(event.pointer),
        onCancel: (_) => _pointers.clear(),
        child: CustomPaint(
          size: size,
          painter: DynCurvePainter(
              palette: ConsolePalette.of(context),
              dyn: widget.dyn,
              levelDb: widget.levelDb,
              color: widget.color),
        ),
      );
    });
  }
}

/// GR meter bar: horizontal bar, 0..-24 dB scale.
class U4GrBar extends StatelessWidget {
  final double grDb; // <= 0
  final Color color;

  const U4GrBar({super.key, required this.grDb, required this.color});

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, c) {
        final w = c.maxWidth.isFinite ? c.maxWidth : 200.0;
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                Text('GR',
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 11,
                        color: ConsolePalette.of(context).text7)),
                Text('${grDb.toStringAsFixed(1)} dB',
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 11,
                        color: ConsolePalette.of(context).text1,
                        fontFeatures: [FontFeature.tabularFigures()])),
              ],
            ),
            const SizedBox(height: 4),
            CustomPaint(
              size: Size(w, 10),
              painter: _GrPainter(
                  palette: ConsolePalette.of(context),
                  frac: (-grDb / 24).clamp(0.0, 1.0),
                  color: color),
            ),
          ],
        );
      },
    );
  }
}

class _GrPainter extends CustomPainter {
  final ConsolePalette palette;
  final double frac;
  final Color color;
  _GrPainter({required this.palette, required this.frac, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(3)),
      Paint()..color = palette.control,
    );
    if (frac > 0) {
      canvas.drawRRect(
        RRect.fromRectAndRadius(
            Rect.fromLTWH(size.width - size.width * frac, 0, size.width * frac,
                size.height),
            const Radius.circular(3)),
        Paint()..color = color,
      );
    }
  }

  @override
  bool shouldRepaint(covariant _GrPainter old) =>
      old.palette != palette || old.frac != frac || old.color != color;
}

// ---------------------------------------------------------------------------
// Send row: level fader-strip + mute + tap cycle, no Slider
// ---------------------------------------------------------------------------

/// One send row: vertical mini-fader (drag), mute chip, tap cycler.
class U4SendRow extends StatelessWidget {
  final ChannelSendVm send;
  final ValueChanged<double> onLevel;
  final VoidCallback onMute;
  final VoidCallback? onTapCycle;

  const U4SendRow({
    super.key,
    required this.send,
    required this.onLevel,
    required this.onMute,
    this.onTapCycle,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).tileBg,
        border: Border.all(color: ConsolePalette.of(context).tileBorder),
        borderRadius: U4.rBtn,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('S${send.slot} · ${send.destName}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: ConsolePalette.of(context).text1)),
                const SizedBox(height: 6),
                _MiniLevel(
                  value: send.level,
                  onChanged: onLevel,
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          GestureDetector(
            onTap: onMute,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: send.muted
                    ? ConsolePalette.of(context).muteOnBg
                    : ConsolePalette.of(context).tileBg,
                border: Border.all(
                    color: send.muted
                        ? ConsolePalette.of(context).muteOnBd
                        : ConsolePalette.of(context).tileBorder),
                borderRadius: U4.rBtn,
              ),
              child: Text(send.muted ? 'MUTED' : 'open',
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: send.muted
                          ? ConsolePalette.of(context).onDanger
                          : ConsolePalette.of(context).text7)),
            ),
          ),
          const SizedBox(width: 6),
          GestureDetector(
            onTap: onTapCycle,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: ConsolePalette.of(context).panel,
                border:
                    Border.all(color: ConsolePalette.of(context).railBorder),
                borderRadius: U4.rBtn,
              ),
              child: Text(
                send.tap == null
                    ? 'tap ?'
                    : tapLabels[send.tap] ?? 'tap ${send.tap}',
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: ConsolePalette.of(context).cyan),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _MiniLevel extends StatelessWidget {
  final double value;
  final ValueChanged<double> onChanged;

  const _MiniLevel({required this.value, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onHorizontalDragStart: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      onHorizontalDragUpdate: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      onTapUp: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      child: SizedBox(
        height: 18,
        child: CustomPaint(
          size: const Size(double.infinity, 18),
          painter: _MiniLevelPainter(
              palette: ConsolePalette.of(context), value: value),
        ),
      ),
    );
  }
}

class _MiniLevelPainter extends CustomPainter {
  final ConsolePalette palette;
  final double value;
  _MiniLevelPainter({required this.palette, required this.value});

  @override
  void paint(Canvas canvas, Size size) {
    final bar = Rect.fromLTWH(0, 7, size.width, 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(bar, const Radius.circular(2)),
      Paint()..color = palette.control,
    );
    final fill = Rect.fromLTWH(0, 7, size.width * value.clamp(0.0, 1.0), 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(fill, const Radius.circular(2)),
      Paint()..color = palette.cyan,
    );
    final x = size.width * value.clamp(0.0, 1.0);
    canvas.drawCircle(Offset(x, 9), 7, Paint()..color = palette.text1);
    canvas.drawCircle(
        Offset(x, 9),
        7,
        Paint()
          ..color = palette.lineCtl
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5);
  }

  @override
  bool shouldRepaint(covariant _MiniLevelPainter old) =>
      old.palette != palette || old.value != value;
}

// ---------------------------------------------------------------------------
// Input level hbar (hbar meter variant)
// ---------------------------------------------------------------------------

/// Horizontal input-level bar fed by a dB ValueNotifier (no animation
/// widgets: repaints only via the notifier).
class U4LevelHbar extends StatelessWidget {
  final ValueNotifier<double> levelDb;

  const U4LevelHbar({super.key, required this.levelDb});

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<double>(
      valueListenable: levelDb,
      builder: (context, db, _) {
        final frac = ((db + 90) / 90).clamp(0.0, 1.0);
        return CustomPaint(
          size: const Size(double.infinity, 8),
          painter:
              _HbarPainter(palette: ConsolePalette.of(context), frac: frac),
        );
      },
    );
  }
}

class _HbarPainter extends CustomPainter {
  final ConsolePalette palette;
  final double frac;
  _HbarPainter({required this.palette, required this.frac});

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(3)),
      Paint()..color = palette.control,
    );
    final w = size.width * frac;
    if (w <= 0) {
      return;
    }
    final p = Paint()..color = palette.meterGreen;
    if (frac > 0.82) {
      p.color = palette.meterRed;
    } else if (frac > 0.66) {
      p.color = palette.meterAmber;
    }
    canvas.drawRRect(
      RRect.fromRectAndRadius(
          Rect.fromLTWH(0, 0, w, size.height), const Radius.circular(3)),
      p,
    );
  }

  @override
  bool shouldRepaint(covariant _HbarPainter old) =>
      old.palette != palette || old.frac != frac;
}
