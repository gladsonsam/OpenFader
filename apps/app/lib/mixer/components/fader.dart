import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../controller.dart';
import '../../u4law.dart';
import '../design.dart';
import 'meter.dart';

/// Cap-only vertical gestures coexist with horizontal channel scrolling.
/// Travel is in local logical points, with a stable start value per gesture.
class MixerFader extends StatefulWidget {
  final MixerChannel channel;
  final ValueChanged<double> onPosition;
  final VoidCallback onSelect, onUnity;
  const MixerFader(
      {super.key,
      required this.channel,
      required this.onPosition,
      required this.onSelect,
      required this.onUnity});
  @override
  State<MixerFader> createState() => _MixerFaderState();
}

class _MixerFaderState extends State<MixerFader> {
  double? _drag;
  bool _held = false;

  void _release() => setState(() {
        _held = false;
        _drag = null;
      });
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final ch = widget.channel;
        final geometry = _FaderGeometry(box.biggest);
        final position = _drag ?? MixerFaderScale.fromEngine(ch.fader);
        final y = geometry.top + (1 - position) * geometry.travel;
        return Semantics(
            label: '${ch.name} level',
            value: ch.levelLabel,
            increasedValue:
                '${fmtDb(MixerFaderScale.toEngine(position + .01))} dB',
            decreasedValue:
                '${fmtDb(MixerFaderScale.toEngine(position - .01))} dB',
            slider: true,
            onIncrease: () => widget.onPosition(position + .01),
            onDecrease: () => widget.onPosition(position - .01),
            child: DecoratedBox(
                decoration: BoxDecoration(
                    color: MixerDesign.well,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(
                        color: ch.selected
                            ? MixerDesign.selectedLine
                            : MixerDesign.line,
                        width: ch.selected ? 1.5 : 1)),
                child: Stack(clipBehavior: Clip.none, children: [
                  Positioned.fill(
                      child: CustomPaint(painter: _ScalePainter(geometry))),
                  Positioned(
                      left: box.maxWidth * .24,
                      top: geometry.top - 16,
                      width: (box.maxWidth * .21).clamp(16, 30),
                      bottom: geometry.bottom,
                      child: MixerMeter(level: ch.meter, stereo: ch.master)),
                  Positioned(
                      left: geometry.x - geometry.capWidth / 2 - 6,
                      top: y - geometry.capHeight / 2 - 6,
                      width: geometry.capWidth + 12,
                      height: geometry.capHeight + 12,
                      child: Tooltip(
                          message: '${ch.levelLabel} · double-tap for unity',
                          triggerMode: TooltipTriggerMode.manual,
                          child: Listener(
                              onPointerDown: (_) =>
                                  setState(() => _held = true),
                              onPointerUp: (_) => _release(),
                              onPointerCancel: (_) => _release(),
                              child: GestureDetector(
                                  behavior: HitTestBehavior.opaque,
                                  onTapCancel: () {
                                    if (_drag == null) _release();
                                  },
                                  onTap: widget.onSelect,
                                  onDoubleTap: widget.onUnity,
                                  onVerticalDragStart: (_) {
                                    setState(() {
                                      _held = true;
                                      _drag = position;
                                    });
                                    widget.onSelect();
                                  },
                                  onVerticalDragUpdate: (d) {
                                    final next = ((_drag ?? position) -
                                            d.delta.dy / geometry.travel)
                                        .clamp(0.0, 1.0);
                                    setState(() => _drag = next);
                                    widget.onPosition(next);
                                  },
                                  onVerticalDragEnd: (_) => _release(),
                                  onVerticalDragCancel: _release,
                                  child: Padding(
                                      padding: const EdgeInsets.all(6),
                                      child: AnimatedScale(
                                          scale: _held ? 1.28 : 1,
                                          duration:
                                              MediaQuery.disableAnimationsOf(
                                                      context)
                                                  ? Duration.zero
                                                  : const Duration(
                                                      milliseconds: 110),
                                          curve: Curves.easeOutCubic,
                                          child: CustomPaint(
                                              painter:
                                                  _CapPainter(_held)))))))),
                ])));
      });
}

class _FaderGeometry {
  final Size size;
  const _FaderGeometry(this.size);
  // Reserve room for the held cap without changing travel during a gesture.
  double get top => capHeight * 1.28 / 2 + 5;
  double get bottom => top;
  double get travel => math.max(40, size.height - top - bottom);
  double get x => size.width * .59;
  double get capWidth => (size.width * .39).clamp(34, 54);
  double get capHeight => capWidth * 1.32;
}

class _ScalePainter extends CustomPainter {
  final _FaderGeometry g;
  const _ScalePainter(this.g);
  @override
  void paint(Canvas c, Size s) {
    mixerText(c, 'LR', const Offset(8, 10), 10,
        color: MixerDesign.mutedText, weight: FontWeight.w600);
    mixerBox(c, Rect.fromLTWH(g.x - 3, g.top - 3, 6, g.travel + 6),
        const Color(0xff161616),
        radius: 3, border: const Color(0xff2a2a2a));
    for (final mark in MixerFaderScale.marks) {
      final y = g.top + (1 - MixerFaderScale.positionForDb(mark.$1)) * g.travel;
      final x = s.width * .77;
      c.drawLine(Offset(x - 4, y), Offset(x - 2, y),
          Paint()..color = MixerDesign.mutedText);
      mixerText(c, mark.$2, Offset(x, y - 6), 9,
          color: const Color(0xffdddddd));
    }
  }

  @override
  bool shouldRepaint(_ScalePainter old) => old.g.size != g.size;
}

class _CapPainter extends CustomPainter {
  final bool held;
  const _CapPainter(this.held);
  @override
  void paint(Canvas c, Size s) {
    final gap = 3.0;
    final h = (s.height - gap) / 2;
    final path = Path()
      ..addRRect(RRect.fromRectAndRadius(
          Rect.fromLTWH(0, 0, s.width, h), const Radius.circular(6)))
      ..addRRect(RRect.fromRectAndRadius(
          Rect.fromLTWH(0, h + gap, s.width, h), const Radius.circular(6)));
    if (held) {
      c.drawPath(
          path,
          Paint()
            ..color = const Color(0x45effb00)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4));
    }
    c.drawShadow(path, Colors.black.withValues(alpha: .6), 3, false);
    for (var n = 0; n < 2; n++) {
      final y = n * (h + gap);
      mixerBox(c, Rect.fromLTWH(0, y, s.width, h),
          Color(n == 0 ? 0xffdcdcd8 : 0xffc8c8c4),
          radius: 6);
      c.drawLine(Offset(6, y + .5), Offset(s.width - 6, y + .5),
          Paint()..color = const Color(0xbfffffff));
      for (var grip = 0; grip < 2; grip++) {
        mixerBox(
            c,
            Rect.fromLTWH(s.width * .18, y + h * (n == 0 ? .5 : .25) + grip * 6,
                s.width * .64, 2),
            const Color(0xff8c8c88),
            radius: 1);
      }
    }
  }

  @override
  bool shouldRepaint(_CapPainter old) => old.held != held;
}
