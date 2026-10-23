// Signal meter (strip variant): 6px column, gradient bands (green 0-66% / amber 66-84% /
// red 84-100%), dark cover above the fill line, 2px peak-hold line and a
// square clip light above the column. Single CustomPainter inside a
// RepaintBoundary; levels arrive as 0..1 fractions from the engine poll.
import 'package:flutter/material.dart';
import '../palette.dart';

/// Peak/clip hold times mirror the meter-sim constants
/// (PEAK_HOLD_MS / CLIP_HOLD_MS = 1.5 s).
const _hold = Duration(milliseconds: 1500);

class U4Meter extends StatefulWidget {
  /// 0..1 level fraction; repaint-driven via this listenable.
  final ValueNotifier<double> level;

  const U4Meter({super.key, required this.level});

  @override
  State<U4Meter> createState() => _U4MeterState();
}

class _U4MeterState extends State<U4Meter> {
  double _peak = 0;
  DateTime _peakHoldUntil = DateTime.fromMillisecondsSinceEpoch(0);
  DateTime _clipHoldUntil = DateTime.fromMillisecondsSinceEpoch(0);
  bool _clip = false;

  @override
  void initState() {
    super.initState();
    widget.level.addListener(_onTick);
  }

  @override
  void didUpdateWidget(U4Meter old) {
    super.didUpdateWidget(old);
    if (old.level != widget.level) {
      old.level.removeListener(_onTick);
      widget.level.addListener(_onTick);
    }
  }

  @override
  void dispose() {
    widget.level.removeListener(_onTick);
    super.dispose();
  }

  void _onTick() {
    final now = DateTime.now();
    final lv = widget.level.value.clamp(0.0, 1.0);
    var changed = false;
    if (lv >= _peak || now.isAfter(_peakHoldUntil)) {
      // Re-peak instantly; after the hold, fall toward the live level.
      final next =
          lv >= _peak ? lv : (_peak + (lv - _peak) * 0.35).clamp(0.0, 1.0);
      if ((next - _peak).abs() > 0.0005 || lv >= _peak) {
        _peak = next;
        changed = true;
      }
      if (lv >= _peak) _peakHoldUntil = now.add(_hold);
    }
    final clipNow = lv >= 0.985;
    if (clipNow) {
      _clipHoldUntil = now.add(_hold);
      if (!_clip) {
        _clip = true;
        changed = true;
      }
    } else if (_clip && now.isAfter(_clipHoldUntil)) {
      _clip = false;
      changed = true;
    }
    if (changed) setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    // Strip variant box: 14px wide (6px column + clip square above).
    return RepaintBoundary(
      child: CustomPaint(
        painter: _MeterPainter(
          palette: ConsolePalette.of(context),
          level: widget.level,
          peak: _peak,
          clip: _clip,
        ),
        // Repaint on every level tick; peak/clip ride setState above.
        isComplex: true,
      ),
    );
  }
}

class _MeterPainter extends CustomPainter {
  final ConsolePalette palette;
  final ValueNotifier<double> level;
  final double peak;
  final bool clip;

  _MeterPainter(
      {required this.palette,
      required this.level,
      required this.peak,
      required this.clip})
      : super(repaint: level);

  @override
  void paint(Canvas canvas, Size size) {
    const mw = 6.0;
    const meterTop = mw + 3;
    final h = (size.height - meterTop).clamp(0.0, double.infinity);
    final lv = level.value.clamp(0.0, 1.0);

    // Gradient bands measured from the bottom (design: 66/84 splits).
    final yGreen = meterTop + h * (1 - 0.66);
    final yAmber = meterTop + h * (1 - 0.84);
    canvas.drawRect(Rect.fromLTWH(0, yGreen, mw, meterTop + h - yGreen),
        _fill(palette.meterGreen));
    canvas.drawRect(Rect.fromLTWH(0, yAmber, mw, yGreen - yAmber),
        _fill(palette.meterAmber));
    canvas.drawRect(Rect.fromLTWH(0, meterTop, mw, yAmber - meterTop),
        _fill(palette.meterRed));

    // Dark cover above the fill line.
    final fillY = meterTop + h * (1 - lv);
    if (fillY > meterTop) {
      canvas.drawRect(Rect.fromLTWH(0, meterTop, mw, fillY - meterTop),
          _fill(palette.meterCover));
    }
    // Peak-hold line (2px), hidden below -80 dB (design); fraction ~0 here.
    if (peak > 0.005) {
      final py = (meterTop + h * (1 - peak)).clamp(meterTop, meterTop + h - 2);
      canvas.drawRect(Rect.fromLTWH(0, py, mw, 2), _fill(palette.peakLine));
    }
    // Clip square above the column (design: square clip light).
    canvas.drawRect(
      Rect.fromLTWH(0, 0, mw, mw),
      _fill(clip ? palette.clipOn : palette.clipOff),
    );
  }

  static Paint _fill(Color c) => Paint()..color = c;

  @override
  bool shouldRepaint(_MeterPainter old) =>
      old.palette != palette ||
      old.level != level ||
      old.peak != peak ||
      old.clip != clip;
}
