// Pan bar (strip variant, h30 base): two triangles meeting at centre, centre-out fill
// showing direction + amount, white position marker, centre detent notch.
// Touch builds use the 44px minimum from the ui4 touch rules; double-tap
// resets to centre. Horizontal intent locks the pan drag, vertical intent
// aborts so the surrounding scroll keeps working.
import 'package:flutter/material.dart';
import '../palette.dart';

import '../theme.dart';
import '../u4law.dart';

class U4PanBar extends StatefulWidget {
  final double? value; // -1..1, 0 = centre; null = empty placeholder well
  final double height;
  final ValueChanged<double>? onChanged;

  const U4PanBar(
      {super.key, required this.value, this.height = 30, this.onChanged});

  @override
  State<U4PanBar> createState() => _U4PanBarState();
}

class _U4PanBarState extends State<U4PanBar> {
  Offset? _down;
  double _start = 0;
  bool _active = false;
  double _width = 1;

  @override
  Widget build(BuildContext context) {
    final v = widget.value;
    final enabled = widget.onChanged != null;
    if (v == null) {
      return Container(
        height: widget.height,
        decoration: BoxDecoration(
          border: Border.all(color: ConsolePalette.of(context).railBorder),
          borderRadius: U4.rBtn,
        ),
      );
    }
    final val = v.clamp(-1.0, 1.0);
    return LayoutBuilder(
      builder: (context, constraints) {
        _width = constraints.maxWidth > 0 ? constraints.maxWidth : 1;
        return GestureDetector(
          behavior: HitTestBehavior.opaque,
          onHorizontalDragDown: (d) {
            if (!enabled) return;
            _down = d.localPosition;
            _start = val;
            _active = false;
          },
          onHorizontalDragUpdate: (d) {
            if (!enabled || _down == null) return;
            final dx = d.localPosition.dx - _down!.dx;
            final dy = d.localPosition.dy - _down!.dy;
            if (!_active) {
              final ax = dx.abs(), ay = dy.abs();
              // Clearly vertical -> let the page scroll, never touch pan.
              if (ay > 12 && ay > ax) {
                _down = null;
                return;
              }
              if (ax <= 10 || ax <= ay) return;
              _active = true;
            }
            widget.onChanged?.call(panFromDrag(_start, dx, _width, false));
          },
          onHorizontalDragEnd: (_) => _down = null,
          onHorizontalDragCancel: () => _down = null,
          onDoubleTap: enabled ? () => widget.onChanged?.call(0) : null,
          child: Container(
            height: widget.height,
            decoration: BoxDecoration(
              color: ConsolePalette.of(context).well,
              border: Border.all(color: ConsolePalette.of(context).railBorder),
              borderRadius: U4.rBtn,
            ),
            child: CustomPaint(
              painter:
                  _PanPainter(palette: ConsolePalette.of(context), value: val),
            ),
          ),
        );
      },
    );
  }
}

class _PanPainter extends CustomPainter {
  final ConsolePalette palette;
  final double value;

  _PanPainter({required this.palette, required this.value});

  @override
  void paint(Canvas canvas, Size size) {
    const padH = 6.0, padV = 5.0;
    final w = size.width - padH * 2;
    final h = size.height - padV * 2;
    if (w <= 0 || h <= 0) return;
    canvas.save();
    canvas.translate(padH, padV);

    // Twin-triangle backdrop (notch colour at 40%).
    final notch = Paint()..color = palette.panNotch.withValues(alpha: 0.4);
    final left = Path()
      ..moveTo(0, 1)
      ..lineTo(w / 2, h / 2)
      ..lineTo(0, h - 1)
      ..close();
    final right = Path()
      ..moveTo(w, 1)
      ..lineTo(w / 2, h / 2)
      ..lineTo(w, h - 1)
      ..close();
    canvas.drawPath(left, notch);
    canvas.drawPath(right, notch);

    // Centre-out fill from 50% to the marker.
    final fillL = 50 + (value < 0 ? value : 0) * 50;
    final fillW = value.abs() * 50;
    if (fillW > 0.5) {
      canvas.save();
      canvas.clipRect(Rect.fromLTWH(w * fillL / 100, 0, w * fillW / 100, h));
      canvas.drawPath(left, Paint()..color = palette.panFill);
      canvas.drawPath(right, Paint()..color = palette.panFill);
      canvas.restore();
    }

    // Centre detent notch (glows when centred).
    final centred = value.abs() < 0.04;
    canvas.drawRect(
      Rect.fromLTWH(
          w / 2 - (centred ? 1 : 0.5), h * 0.3, centred ? 2 : 1, h * 0.4),
      Paint()
        ..color = centred ? palette.text1 : palette.panNotch
        ..maskFilter =
            centred ? const MaskFilter.blur(BlurStyle.normal, 2) : null,
    );

    // White position marker (panMarkerLeft: 50 + pan * 44, percent).
    final mx = w * panMarkerLeft(value) / 100;
    canvas.drawRRect(
      RRect.fromRectAndRadius(
          Rect.fromLTWH(mx - 1.5, 4, 3, h - 8), const Radius.circular(1)),
      Paint()
        ..color = palette.panMarker
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 1),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(_PanPainter old) =>
      old.palette != palette || old.value != value;
}
