import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import '../design.dart';

/// Live meter reusable in strips, bus pickers, and future meter bridges.
/// The signal repaints this boundary directly; it does not rebuild controls.
class MixerMeter extends StatefulWidget {
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
          child: CustomPaint(painter: _MeterPainter(_signal, widget.stereo))));
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
    final level = source.value.clamp(0.0, 1.0);
    if (level >= peak) {
      peak = level;
      _peakAt = now;
    } else if (now.difference(_peakAt).inMilliseconds > 1200) {
      peak =
          (peak - now.difference(_tick).inMilliseconds / 5000).clamp(level, 1);
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
  final _MeterSignal signal;
  final bool stereo;
  _MeterPainter(this.signal, this.stereo) : super(repaint: signal);
  @override
  void paint(Canvas c, Size s) {
    final clipHeight = 11.0;
    final top = clipHeight + 6;
    final h = (s.height - top).clamp(1.0, double.infinity);
    final count = stereo ? 2 : 1;
    final w = (s.width - (count - 1) * 3) / count;
    for (var i = 0; i < count; i++) {
      final x = i * (w + 3);
      mixerBox(
          c,
          Rect.fromLTWH(x, 0, w, clipHeight),
          signal.source.value >= .99
              ? MixerDesign.mute
              : const Color(0xff3d1006),
          radius: 3);
      mixerBox(c, Rect.fromLTWH(x, top, w, h * .2), const Color(0xff3a2600),
          radius: 3);
      mixerBox(c, Rect.fromLTWH(x, top + h * .21, w, h * .79),
          const Color(0xff1a2a00),
          radius: 2);
      final level = signal.source.value.clamp(0.0, 1.0);
      mixerBox(c, Rect.fromLTWH(x, top + (1 - level) * h, w, level * h),
          MixerDesign.meter,
          radius: 2);
      mixerBox(c, Rect.fromLTWH(x, top + (1 - signal.peak) * h, w, 3),
          MixerDesign.amber);
    }
  }

  @override
  bool shouldRepaint(_MeterPainter old) =>
      old.signal != signal || old.stereo != stereo;
}
