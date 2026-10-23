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

import 'package:flutter/material.dart';

import '../core/channel_views.dart';
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
        color: U4.panel,
        border: Border.all(color: U4.linePanel),
        borderRadius: U4.rBtn,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(title,
                    style: const TextStyle(
                        fontFamily: U4.font, fontSize: 12, color: U4.text7)),
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

/// Touch knob: vertical drag edits (spec math via [onDrag]), double-tap
/// resets to [defaultValue]. Paints arc + pointer + value text.
class U4Knob extends StatefulWidget {
  final String label;
  final double value;
  final double min;
  final double max;
  final String text;
  final double defaultValue;
  final Color color;
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
    this.color = U4.meterGreen,
    this.fine = false,
    this.enabled = true,
    this.onFocus,
  });

  @override
  State<U4Knob> createState() => _U4KnobState();
}

class _U4KnobState extends State<U4Knob> {
  double? _startN;
  double? _startValue;

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
              _startValue = widget.value;
            }
          : null,
      onVerticalDragUpdate: widget.enabled
          ? (d) {
              final n0 = _startN ?? toN(widget.min, widget.max, widget.value);
              final v = specFromDrag(widget.min, widget.max, n0,
                  -d.localPosition.dy + 60, widget.fine);
              widget.onInput(v);
            }
          : null,
      onDoubleTap: widget.enabled
          ? () {
              widget.onFocus?.call();
              widget.onInput(widget.defaultValue);
            }
          : null,
      child: Opacity(
        opacity: widget.enabled ? 1 : 0.45,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            CustomPaint(
              size: const Size(68, 68),
              painter: _KnobPainter(
                n: toN(widget.min, widget.max, widget.value),
                color: widget.color,
              ),
            ),
            const SizedBox(height: 4),
            Text(widget.label,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                    fontFamily: U4.font, fontSize: 11, color: U4.text7)),
            Text(widget.text,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                    fontFamily: U4.font,
                    fontSize: 12,
                    color: U4.text1,
                    fontFeatures: [FontFeature.tabularFigures()])),
            if (_startValue != null)
              const SizedBox.shrink()
            else
              const SizedBox.shrink(),
          ],
        ),
      ),
      ),
    );
  }
}

class _KnobPainter extends CustomPainter {
  final double n;
  final Color color;

  _KnobPainter({required this.n, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final c = size.center(Offset.zero);
    final r = size.width / 2 - 4;
    const sweep0 = math.pi * 0.75;
    const sweep1 = math.pi * 2.25;
    final bg = Paint()
      ..color = U4.railBorder
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
    canvas.drawCircle(c, 3, Paint()..color = U4.text1);
  }

  @override
  bool shouldRepaint(covariant _KnobPainter old) =>
      old.n != n || old.color != color;
}

// ---------------------------------------------------------------------------
// EQ curve + RTA overlay
// ---------------------------------------------------------------------------

/// EQ response painter: grid + summed curve + band nodes + live RTA area
/// overlay (rtaOverlay.ts: peak-hold handled by the owner feeding [rtaBins]).
class EqCurvePainter extends CustomPainter {
  final List<double> responseDb; // EqGraphGeom.n + 1 samples
  final ChannelEqVm eq;
  final List<double>? rtaBins; // 0..1 magnitudes, RTA_BINS wide
  final bool rtaActive;

  EqCurvePainter({
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
  static const _dbTicks = <double>[12, 6, 0, -6, -12];

  @override
  void paint(Canvas canvas, Size size) {
    final sx = size.width / EqGraphGeom.w;
    final sy = size.height / EqGraphGeom.h;
    // Grid.
    final grid = Paint()
      ..color = U4.linePanel
      ..strokeWidth = 1;
    for (final f in _freqTicks) {
      final x = EqGraphGeom.xOf(f) * sx;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
    }
    for (final db in _dbTicks) {
      final y = EqGraphGeom.yOf(db) * sy;
      final p = Paint()
        ..color = db == 0 ? U4.lineCtl : U4.linePanel
        ..strokeWidth = db == 0 ? 1.5 : 1;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), p);
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
      canvas.drawPath(path, Paint()..color = const Color(0x33a3ed19));
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
      canvas.drawPath(fill, Paint()..color = const Color(0x22969d88));
      canvas.drawPath(
        curve,
        Paint()
          ..color = U4.cyan
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    // Nodes.
    for (var i = 0; i < eq.bands.length; i++) {
      final b = eq.bands[i];
      final x = EqGraphGeom.xOf(b.f.clamp(20, 20000)) * sx;
      final y = EqGraphGeom.yOf(b.g.clamp(-15, 15)) * sy;
      final col = Color(
          EqGraphGeom.bandColors[(i + 1) % EqGraphGeom.bandColors.length]);
      canvas.drawCircle(Offset(x, y), 9,
          Paint()..color = col.withValues(alpha: b.on ? 1 : 0.35));
      canvas.drawCircle(
          Offset(x, y),
          9,
          Paint()
            ..color = eq.selectedBand == b.band ? Colors.white : U4.well
            ..style = PaintingStyle.stroke
            ..strokeWidth = eq.selectedBand == b.band ? 2.5 : 1.5);
    }
    // HPF node on the 0 dB line.
    {
      final x = EqGraphGeom.xOf(eq.hpfF.clamp(20, 1000)) * sx;
      final y = EqGraphGeom.yOf(0) * sy;
      final col = Color(EqGraphGeom.bandColors[0]);
      canvas.drawCircle(Offset(x, y), 8,
          Paint()..color = col.withValues(alpha: eq.hpfOn ? 1 : 0.35));
      if (eq.selectedBand == 0) {
        canvas.drawCircle(
            Offset(x, y),
            8,
            Paint()
              ..color = Colors.white
              ..style = PaintingStyle.stroke
              ..strokeWidth = 2.5);
      }
    }
  }

  @override
  bool shouldRepaint(covariant EqCurvePainter old) =>
      old.responseDb != responseDb ||
      old.eq != eq ||
      old.rtaBins != rtaBins ||
      old.rtaActive != rtaActive;
}

/// Interactive EQ graph: node drag (freq/gain) + double-tap band select.
/// Pinch-Q is exposed via [onQ]; HPF drags edit frequency only.
class U4EqGraph extends StatelessWidget {
  final ChannelEqVm eq;
  final List<double> responseDb;
  final List<double>? rtaBins;
  final bool rtaActive;
  final ValueChanged<int> onSelectBand;
  final void Function(int band, double f, double g) onNode;
  final void Function(int band, double q) onQ;

  const U4EqGraph({
    super.key,
    required this.eq,
    required this.responseDb,
    this.rtaBins,
    this.rtaActive = false,
    required this.onSelectBand,
    required this.onNode,
    required this.onQ,
  });

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final w = constraints.maxWidth.isFinite ? constraints.maxWidth : 340.0;
        const h = 190.0;
        return GestureDetector(
          behavior: HitTestBehavior.opaque,
          onDoubleTap: null,
          child: _EqGraphGesture(
            width: w,
            height: h,
            eq: eq,
            onSelectBand: onSelectBand,
            onNode: onNode,
            child: CustomPaint(
              size: Size(w, h),
              painter: EqCurvePainter(
                responseDb: responseDb,
                eq: eq,
                rtaBins: rtaBins,
                rtaActive: rtaActive,
              ),
            ),
          ),
        );
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
  final Widget child;

  const _EqGraphGesture({
    required this.width,
    required this.height,
    required this.eq,
    required this.onSelectBand,
    required this.onNode,
    required this.child,
  });

  @override
  State<_EqGraphGesture> createState() => _EqGraphGestureState();
}

class _EqGraphGestureState extends State<_EqGraphGesture> {
  int? _dragBand;

  int _hitBand(Offset local) {
    var best = -1;
    var bestD = 30.0;
    for (final b in widget.eq.bands) {
      final x =
          toN(0, 1000, EqGraphGeom.xOf(b.f.clamp(20, 20000))) * widget.width;
      final y = toN(-15, 15, b.g.clamp(-15, 15)) * widget.height;
      final d = (Offset(x, y) - local).distance;
      if (d < bestD) {
        bestD = d;
        best = b.band;
      }
    }
    if (best < 0) {
      final x = toN(0, 1000, EqGraphGeom.xOf(widget.eq.hpfF.clamp(20, 1000))) *
          widget.width;
      if ((Offset(x, widget.height / 2) - local).distance < 30) best = 0;
    }
    return best;
  }

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onPanStart: (d) {
        final band = _hitBand(d.localPosition);
        if (band >= 0) {
          _dragBand = band;
          widget.onSelectBand(band);
        }
      },
      onPanUpdate: (d) {
        final band = _dragBand;
        if (band == null) return;
        final ux = (d.localPosition.dx / widget.width).clamp(0.0, 1.0);
        final uy = (d.localPosition.dy / widget.height).clamp(0.0, 1.0);
        final pos = eqNodeFromPos(ux, uy);
        if (band == 0) {
          widget.onNode(band, pos.f.clamp(20, 1000), 0);
        } else {
          widget.onNode(band, pos.f, pos.g);
        }
      },
      onPanEnd: (_) => _dragBand = null,
      child: widget.child,
    );
  }
}

// ---------------------------------------------------------------------------
// Dynamics transfer curve + GR bar
// ---------------------------------------------------------------------------

/// Transfer-curve painter (dynModel.ts: lo..0 axes, gold curve, threshold
/// line + tag, operating dot, unity diagonal).
class DynCurvePainter extends CustomPainter {
  final ChannelDynVm dyn;
  final double? levelDb; // null when frozen: dot parks at lo
  final Color color;

  DynCurvePainter({required this.dyn, this.levelDb, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    final lo = dyn.lo;
    double px(double v) => (v - lo) / -lo * size.width;
    double py(double v) => size.height - (v - lo) / -lo * size.height;

    // Grid every 10 dB.
    final grid = Paint()
      ..color = U4.linePanel
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
          ..color = U4.lineCtl
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
          ..color = U4.cyan
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
            style: const TextStyle(
                fontFamily: U4.font, fontSize: 10, color: U4.text1)),
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
          ..color = Colors.white
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5);
  }

  @override
  bool shouldRepaint(covariant DynCurvePainter old) =>
      old.dyn != dyn || old.levelDb != levelDb || old.color != color;
}

/// Interactive transfer graph: horizontal drag edits threshold (both
/// kinds); comp vertical drag edits ratio. No sliders.
class U4DynGraph extends StatelessWidget {
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
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onPanUpdate: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null) return;
        final size = box.size;
        final lo = dyn.lo;
        final thr =
            lo + (d.localPosition.dx / size.width).clamp(0.0, 1.0) * -lo;
        onThreshold(thr);
        if (dyn.kind == DynKind.comp && onRatio != null) {
          final up = 1 - (d.localPosition.dy / size.height).clamp(0.0, 1.0);
          onRatio!(1 + up * 19); // 1..20
        }
      },
      child: CustomPaint(
        size: const Size(double.infinity, 170),
        painter: DynCurvePainter(dyn: dyn, levelDb: levelDb, color: color),
      ),
    );
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
                const Text('GR',
                    style: TextStyle(
                        fontFamily: U4.font, fontSize: 11, color: U4.text7)),
                Text('${grDb.toStringAsFixed(1)} dB',
                    style: const TextStyle(
                        fontFamily: U4.font,
                        fontSize: 11,
                        color: U4.text1,
                        fontFeatures: [FontFeature.tabularFigures()])),
              ],
            ),
            const SizedBox(height: 4),
            CustomPaint(
              size: Size(w, 10),
              painter:
                  _GrPainter(frac: (-grDb / 24).clamp(0.0, 1.0), color: color),
            ),
          ],
        );
      },
    );
  }
}

class _GrPainter extends CustomPainter {
  final double frac;
  final Color color;
  _GrPainter({required this.frac, required this.color});

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(3)),
      Paint()..color = U4.control,
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
      old.frac != frac || old.color != color;
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
        color: U4.tileBg,
        border: Border.all(color: U4.tileBorder),
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
                    style: const TextStyle(
                        fontFamily: U4.font,
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: U4.text1)),
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
                color: send.muted ? U4.muteOnBg : U4.tileBg,
                border:
                    Border.all(color: send.muted ? U4.muteOnBd : U4.tileBorder),
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
                      color: send.muted ? Colors.white : U4.text7)),
            ),
          ),
          const SizedBox(width: 6),
          GestureDetector(
            onTap: onTapCycle,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: U4.panel,
                border: Border.all(color: U4.railBorder),
                borderRadius: U4.rBtn,
              ),
              child: Text(
                send.tap == null
                    ? 'tap ?'
                    : tapLabels[send.tap] ?? 'tap ${send.tap}',
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                    fontFamily: U4.font,
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: U4.cyan),
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
      onPanUpdate: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      onTapDown: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      child: SizedBox(
        height: 18,
        child: CustomPaint(
          size: const Size(double.infinity, 18),
          painter: _MiniLevelPainter(value: value),
        ),
      ),
    );
  }
}

class _MiniLevelPainter extends CustomPainter {
  final double value;
  _MiniLevelPainter({required this.value});

  @override
  void paint(Canvas canvas, Size size) {
    final bar = Rect.fromLTWH(0, 7, size.width, 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(bar, const Radius.circular(2)),
      Paint()..color = U4.control,
    );
    final fill = Rect.fromLTWH(0, 7, size.width * value.clamp(0.0, 1.0), 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(fill, const Radius.circular(2)),
      Paint()..color = U4.cyan,
    );
    final x = size.width * value.clamp(0.0, 1.0);
    canvas.drawCircle(Offset(x, 9), 7, Paint()..color = U4.text1);
    canvas.drawCircle(
        Offset(x, 9),
        7,
        Paint()
          ..color = U4.lineCtl
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5);
  }

  @override
  bool shouldRepaint(covariant _MiniLevelPainter old) => old.value != value;
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
          painter: _HbarPainter(frac: frac),
        );
      },
    );
  }
}

class _HbarPainter extends CustomPainter {
  final double frac;
  _HbarPainter({required this.frac});

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
      RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(3)),
      Paint()..color = U4.control,
    );
    final w = size.width * frac;
    if (w <= 0) {
      return;
    }
    final p = Paint()..color = U4.meterGreen;
    if (frac > 0.82) {
      p.color = U4.meterRed;
    } else if (frac > 0.66) {
      p.color = U4.meterAmber;
    }
    canvas.drawRRect(
      RRect.fromRectAndRadius(
          Rect.fromLTWH(0, 0, w, size.height), const Radius.circular(3)),
      p,
    );
  }

  @override
  bool shouldRepaint(covariant _HbarPainter old) => old.frac != frac;
}
