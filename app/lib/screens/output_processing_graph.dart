import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../core/destination_controls.dart';
import '../palette.dart';

/// Plots reported engineering values. PEQ shows band positions rather than
/// inventing a response curve from opaque provider filter-shape enums.
class OutputProcessingGraph extends StatelessWidget {
  final List<DestinationControlVm> controls;
  final String surface;
  const OutputProcessingGraph({
    super.key,
    required this.controls,
    required this.surface,
  });
  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          surface == 'output_peq'
              ? 'Band positions · frequency / gain'
              : surface == 'output_geq'
                  ? 'Graphic EQ · band gain'
                  : 'Transfer reference · input / output',
          style: TextStyle(color: palette.mutedText),
        ),
        const SizedBox(height: 8),
        SizedBox(
          height: 180,
          child: CustomPaint(
            painter: _GraphPainter(controls, surface, palette),
          ),
        ),
      ],
    );
  }
}

class _GraphPainter extends CustomPainter {
  final List<DestinationControlVm> controls;
  final String surface;
  final ConsolePalette palette;
  _GraphPainter(this.controls, this.surface, this.palette);
  double? value(String id) =>
      controls.where((c) => c.id == id).firstOrNull?.value;
  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    canvas.drawRect(rect, Paint()..color = palette.well);
    final grid = Paint()
      ..color = palette.line
      ..strokeWidth = 1;
    for (var i = 1; i < 6; i++) {
      canvas.drawLine(
        Offset(size.width * i / 6, 0),
        Offset(size.width * i / 6, size.height),
        grid,
      );
      canvas.drawLine(
        Offset(0, size.height * i / 6),
        Offset(size.width, size.height * i / 6),
        grid,
      );
    }
    final ink = Paint()
      ..color = palette.lime
      ..strokeWidth = 2
      ..style = PaintingStyle.stroke;
    double gainY(double gain) => size.height * (0.5 - gain.clamp(-18, 18) / 36);
    if (surface == 'output_peq') {
      for (var band = 1; band <= 4; band++) {
        final frequency = value('eq${band}_freq');
        final gain = value('eq${band}_gain');
        if (frequency == null || gain == null || frequency <= 0) continue;
        final point = Offset(
          size.width * (math.log(frequency / 20) / math.log(1000)).clamp(0, 1),
          gainY(gain),
        );
        canvas.drawCircle(point, 5, ink);
        final text = TextPainter(
          text: TextSpan(
            text: '$band',
            style: TextStyle(color: palette.text, fontSize: 12),
          ),
          textDirection: TextDirection.ltr,
        )..layout();
        text.paint(canvas, point + const Offset(7, -8));
      }
    } else if (surface == 'output_geq') {
      final bands = controls
          .where((c) => RegExp(r'^geq\d+_gain$').hasMatch(c.id))
          .toList();
      final width = size.width / math.max(1, bands.length);
      for (var i = 0; i < bands.length; i++) {
        final gain = bands[i].value;
        if (gain == null) continue;
        canvas.drawLine(
          Offset(width * (i + .5), gainY(0)),
          Offset(width * (i + .5), gainY(gain)),
          ink..strokeWidth = math.max(2, width * .6),
        );
      }
    } else {
      final threshold = value('comp_thr');
      final ratio = value('comp_ratio');
      final makeup = value('comp_makeup');
      if (threshold == null || ratio == null || makeup == null || ratio < 1) {
        return;
      }
      final path = Path();
      for (var i = 0; i <= 160; i++) {
        final input = -80 + i / 2;
        final output = (input <= threshold
                ? input
                : threshold +
                    (ratio >= 101 ? 0 : (input - threshold) / ratio)) +
            makeup;
        final point = Offset(
          size.width * i / 160,
          size.height * (1 - ((output + 80) / 100).clamp(0, 1)),
        );
        if (i == 0) {
          path.moveTo(point.dx, point.dy);
        } else {
          path.lineTo(point.dx, point.dy);
        }
      }
      canvas.drawPath(path, ink);
    }
  }

  @override
  bool shouldRepaint(covariant _GraphPainter oldDelegate) =>
      oldDelegate.palette != palette ||
      oldDelegate.surface != surface ||
      !identical(oldDelegate.controls, controls);
}
