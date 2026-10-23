import 'package:flutter/material.dart';
import '../../u4law.dart';
import '../design.dart';

class MixerPan extends StatefulWidget {
  final String name;
  final double value;
  final ValueChanged<double>? onChanged;
  const MixerPan(
      {super.key, required this.name, required this.value, this.onChanged});
  @override
  State<MixerPan> createState() => _MixerPanState();
}

class _MixerPanState extends State<MixerPan> {
  double? _drag;
  double _snap(double value) => value.abs() < .03 ? 0 : value.clamp(-1, 1);
  void _release() => setState(() => _drag = null);
  void _send(double value) => widget.onChanged?.call(_snap(value));
  @override
  Widget build(BuildContext context) => LayoutBuilder(
      builder: (context, box) => Semantics(
          label: '${widget.name} pan',
          value: panText(widget.value),
          increasedValue: panText((widget.value + .05).clamp(-1.0, 1.0)),
          decreasedValue: panText((widget.value - .05).clamp(-1.0, 1.0)),
          slider: true,
          enabled: widget.onChanged != null,
          onIncrease:
              widget.onChanged == null ? null : () => _send(widget.value + .05),
          onDecrease:
              widget.onChanged == null ? null : () => _send(widget.value - .05),
          child: Tooltip(
              message: '${panText(widget.value)} · double-tap to centre',
              triggerMode: TooltipTriggerMode.manual,
              child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onHorizontalDragStart: widget.onChanged == null
                      ? null
                      : (_) => setState(() => _drag = widget.value),
                  onHorizontalDragUpdate: widget.onChanged == null
                      ? null
                      : (d) {
                          final next = ((_drag ?? widget.value) +
                                  d.delta.dx / ((box.maxWidth - 24) * .36))
                              .clamp(-1.0, 1.0);
                          setState(() => _drag = next);
                          _send(next);
                        },
                  onHorizontalDragEnd: (_) => _release(),
                  onHorizontalDragCancel: _release,
                  onDoubleTap: widget.onChanged == null ? null : () => _send(0),
                  child: SizedBox(
                      height: 44,
                      width: double.infinity,
                      child: CustomPaint(
                          painter: _PanPainter(
                              _drag == null ? widget.value : _snap(_drag!),
                              _drag != null)))))));
}

class _PanPainter extends CustomPainter {
  final double value;
  final bool held;
  const _PanPainter(this.value, this.held);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, MixerDesign.panel,
        radius: 12, border: held ? MixerDesign.selectedLine : null);
    final left = 12.0, right = s.width - 12, center = s.width / 2;
    final bow = Path()
      ..moveTo(left, 8)
      ..quadraticBezierTo(center, 26, right, 8)
      ..lineTo(right, 36)
      ..quadraticBezierTo(center, 18, left, 36)
      ..close();
    c.drawPath(bow, Paint()..color = const Color(0xff4a4a4a));
    final x = center + value * (s.width - 24) * .36;
    if (value.abs() > .001) {
      c.save();
      c.clipPath(bow);
      mixerBox(
          c,
          Rect.fromLTRB(
              x < center ? x : center, 8, x > center ? x : center, 36),
          const Color(0xffcfcfcf));
      c.restore();
    }
    mixerBox(c, Rect.fromLTWH(x - 10, 5, 20, 34), const Color(0xffd8d8d4),
        radius: 6);
    for (var n = 0; n < 3; n++) {
      mixerBox(
          c, Rect.fromLTWH(x - 5 + n * 4, 11, 1, 22), const Color(0xff8e8e8a),
          radius: 1);
    }
  }

  @override
  bool shouldRepaint(_PanPainter old) => old.value != value || old.held != held;
}
