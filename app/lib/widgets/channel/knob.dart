import 'package:flutter/material.dart';

import '../../palette.dart';
import '../../core/control_reset.dart';
import '../../theme.dart';
import 'dart:math' as math;
import '../../design.dart';
import '../../core/channel_views.dart';

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
    final reset = numericResetValue(widget.min, widget.max,
        nativeDefault: widget.defaultValue);
    return Semantics(
      button: true,
      label: '${widget.label} knob, ${widget.text}',
      hint: widget.enabled && reset != null
          ? 'Double-tap to reset to $reset'
          : null,
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
        onDoubleTap: widget.enabled && reset != null
            ? () {
                widget.onFocus?.call();
                widget.onInput(reset);
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
