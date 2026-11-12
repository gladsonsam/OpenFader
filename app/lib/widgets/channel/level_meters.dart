import 'package:material_ui/material_ui.dart';

import '../../palette.dart';
import '../../theme.dart';

/// Full-scale at -24 dB.
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

/// Repaints only via the notifier; no animation widgets.
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
