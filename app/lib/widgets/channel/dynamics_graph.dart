import 'package:material_ui/material_ui.dart';

import '../../palette.dart';
import '../../core/channel_views.dart';
import 'graph_touch_surface.dart';

/// Curve math comes from dynModel.ts; this only paints.
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
    // The selected-control bar owns the numeric threshold readout.
    if (dyn.thr.isFinite) {
      final x = px(dyn.thr.clamp(lo, 0));
      canvas.drawLine(
          Offset(x, 0),
          Offset(x, size.height),
          Paint()
            ..color = color
            ..strokeWidth = 1.5);
    }
    // NaN denotes unavailable; a measured silent level parks at the floor.
    if (levelDb == null || !levelDb!.isNaN) {
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
  }

  @override
  bool shouldRepaint(covariant DynCurvePainter old) =>
      old.palette != palette ||
      old.dyn != dyn ||
      old.levelDb != levelDb ||
      old.color != color;
}

/// Vertical drag edits ratio on comp only; threshold edits work for both kinds.
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
