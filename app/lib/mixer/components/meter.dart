import '../../palette.dart';

import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';

import '../design.dart';

/// Live meter tunable behavior, driven by the Appearance prefs (applied
/// by the shell; read at signal-update time, no rebuild needed).
abstract final class MeterTuning {
  /// Peak-hold time in ms (Peak hold setting, 1/3/5 s).
  static int peakHoldMs = 1000;

  /// When true the clip LED follows the held peak; otherwise live level.
  static bool holdClips = true;
}

/// Live meter reusable in strips, bus pickers, and future meter bridges.
/// The signal repaints this boundary directly; it does not rebuild controls.
class MixerMeter extends StatefulWidget {
  /// Align the signal's scale origin with the adjacent fader ruler.
  static const scaleInset = 17.0;
  final ValueListenable<double> level;
  final bool stereo;
  const MixerMeter({super.key, required this.level, this.stereo = false});
  @override
  State<MixerMeter> createState() => _MixerMeterState();
}

class _MixerMeterState extends State<MixerMeter> {
  late _MeterSignal _signal;
  @override
  void initState() {
    super.initState();
    _signal = _MeterSignal(widget.level);
  }

  @override
  void didUpdateWidget(MixerMeter old) {
    super.didUpdateWidget(old);
    if (old.level != widget.level) {
      _signal.dispose();
      _signal = _MeterSignal(widget.level);
    }
  }

  @override
  void dispose() {
    _signal.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: RepaintBoundary(
      child: CustomPaint(
        painter: _MeterPainter(
          ConsolePalette.of(context),
          _signal,
          widget.stereo,
        ),
      ),
    ),
  );
}

class _MeterSignal extends ChangeNotifier {
  final ValueListenable<double> source;
  double peak = 0;
  DateTime _peakAt = DateTime.now();
  DateTime _tick = DateTime.now();
  _MeterSignal(this.source) {
    source.addListener(_update);
    _update();
  }
  void _update() {
    final now = DateTime.now();
    if (!source.value.isFinite) {
      peak = 0;
      _tick = now;
      notifyListeners();
      return;
    }
    final level = source.value.clamp(0.0, 1.0);
    if (level >= peak) {
      peak = level;
      _peakAt = now;
    } else if (now.difference(_peakAt).inMilliseconds >
        MeterTuning.peakHoldMs) {
      peak = (peak - now.difference(_tick).inMilliseconds / 5000).clamp(
        level,
        1,
      );
    }
    _tick = now;
    notifyListeners();
  }

  @override
  void dispose() {
    source.removeListener(_update);
    super.dispose();
  }
}

class _MeterPainter extends CustomPainter {
  static final _unity = MixerFaderScale.positionForDb(0);
  static final _warning = MixerFaderScale.positionForDb(-12);
  final ConsolePalette palette;
  final _MeterSignal signal;
  final bool stereo;
  _MeterPainter(this.palette, this.signal, this.stereo)
    : super(repaint: signal);
  @override
  void paint(Canvas c, Size s) {
    if (!signal.source.value.isFinite) {
      final center = s.center(Offset.zero);
      c.drawLine(
        center.translate(-3, 0),
        center.translate(3, 0),
        Paint()
          ..color = palette.scaleTick
          ..strokeWidth = 2,
      );
      return;
    }
    final clipHeight = 11.0;
    const top = MixerMeter.scaleInset;
    final h = (s.height - top).clamp(1.0, double.infinity);
    final count = stereo ? 2 : 1;
    final w = (s.width - (count - 1) * 3) / count;
    final clipped = MeterTuning.holdClips
        ? signal.peak >= .99
        : signal.source.value >= .99;
    final level = MixerFaderScale.positionForLevel(signal.source.value);
    final peak = MixerFaderScale.positionForLevel(signal.peak);
    for (var i = 0; i < count; i++) {
      final x = i * (w + 3);
      mixerBox(
        c,
        Rect.fromLTWH(x, 0, w, clipHeight),
        clipped ? palette.meterRed : palette.meterClipBack,
        radius: 3,
      );
      mixerBox(
        c,
        Rect.fromLTWH(x, top + (1 - _unity) * h, w, (_unity - _warning) * h),
        palette.meterAmberBack,
        radius: 3,
      );
      mixerBox(
        c,
        Rect.fromLTWH(x, top + (1 - _warning) * h, w, _warning * h),
        palette.meterGreenBack,
        radius: 2,
      );
      if (level > 0) {
        mixerBox(
          c,
          Rect.fromLTWH(x, top + (1 - level) * h, w, level * h),
          palette.meter,
          radius: 2,
        );
      }
      if (peak > 0) {
        mixerBox(
          c,
          Rect.fromLTWH(x, top + (1 - peak) * h, w, 3),
          palette.meterAmber,
        );
      }
    }
  }

  @override
  bool shouldRepaint(_MeterPainter old) =>
      old.palette != palette || old.signal != signal || old.stereo != stereo;
}
