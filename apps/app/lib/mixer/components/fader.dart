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
        // Design: selected cap grows (1.4x tablet) with a lime glow.
        final scale = ch.selected ? 1.4 : _held ? 1.15 : 1.0;
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
                      child: CustomPaint(painter: _ScalePainter(geometry, ch.number))),
                  Positioned(
                      left: box.maxWidth * .24,
                      top: geometry.top - MixerMeter.scaleInset,
                      width: (box.maxWidth * .21).clamp(16, 30),
                      bottom: geometry.bottom,
                      child: MixerMeter(level: ch.meter, stereo: ch.master)),
                  Positioned(
                      left: geometry.x - geometry.capWidth / 2 - geometry.touchPad,
                      top: y - geometry.capHeight / 2 - geometry.touchPad,
                      width: geometry.capWidth + geometry.touchPad * 2,
                      height: geometry.capHeight + geometry.touchPad * 2,
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
                                      padding: EdgeInsets.all(
                                          geometry.touchPad),
                                      child: AnimatedScale(
                                          scale: scale,
                                          duration:
                                              MediaQuery.disableAnimationsOf(
                                                      context)
                                                  ? Duration.zero
                                                  : const Duration(
                                                      milliseconds: 110),
                                          curve: Curves.easeOutCubic,
                                          child: CustomPaint(
                                              painter: _CapPainter(
                                                  held: _held,
                                                  glow: ch.selected)))))))),
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
  // Design caps are chunky squares: ~38% of strip width on tablet, ~50%
  // on phones (54px on a 104px strip). Meter width ≈ strip minus padding.
  // Larger touch targets (Appearance) grows caps a step further.
  double get capWidth => (size.width *
          (MixerDesign.largeTargets ? .52 : .48))
      .clamp(34, MixerDesign.largeTargets ? 110 : 96);
  double get capHeight => capWidth;

  /// Touch padding around the cap (also grows with larger targets).
  double get touchPad => MixerDesign.largeTargets ? 10 : 6;
}

class _ScalePainter extends CustomPainter {
  final _FaderGeometry g;
  final String number;
  const _ScalePainter(this.g, this.number);
  @override
  void paint(Canvas c, Size s) {
    mixerText(c, 'LR', const Offset(8, 10), 10,
        color: MixerDesign.mutedText, weight: FontWeight.w600);
    mixerBox(c, Rect.fromLTWH(g.x - 3, g.top - 3, 6, g.travel + 6),
        const Color(0xff161616),
        radius: 3, border: const Color(0xff2a2a2a));
    // Design number column (tablet+ only): 1-6 repeated in three banks.
    // The mock marks the third bank's "1" red on channel 1; replicate it.
    if (s.width >= 140) {
      final size = (s.width * .088).clamp(12.0, 18.0);
      for (var i = 0; i < 18; i++) {
        final y = g.top + (i + .5) / 18 * g.travel - size / 2;
        final red = number == '1' && i == 12;
        mixerText(c, '${i % 6 + 1}', Offset(s.width * .08, y), size,
            color: red ? const Color(0xffed0800) : const Color(0xff646464),
            weight: FontWeight.w600);
      }
    }
    for (final mark in MixerFaderScale.marks) {
      final y = g.top + (1 - MixerFaderScale.positionForDb(mark.$1)) * g.travel;
      final x = s.width * .77;
      // Design prefixes: • labeled (/ – on phones), ○ unity, · minor.
      final label = mark.$2;
      final prefix = label == 'U'
          ? '○'
          : label.isEmpty
              ? '·'
              : s.width < 140
                  ? '–'
                  : '•';
      mixerText(c, '$prefix$label', Offset(x, y - 6), 9,
          color: const Color(0xffdddddd));
    }
  }

  @override
  bool shouldRepaint(_ScalePainter old) =>
      old.g.size != g.size || old.number != number;
}

class _CapPainter extends CustomPainter {
  final bool held, glow;
  const _CapPainter({required this.held, this.glow = false});
  @override
  void paint(Canvas c, Size s) {
    // Design cap: one rounded square with ribbed grip zones top + bottom
    // and the lime position line through the middle.
    final rect = RRect.fromRectAndRadius(
        Offset.zero & s, const Radius.circular(14));
    if (glow) {
      c.drawPath(
          Path()..addRRect(rect),
          Paint()
            ..color = MixerDesign.lime.withValues(alpha: .4)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 5));
    }
    if (held) {
      c.drawPath(
          Path()..addRRect(rect),
          Paint()
            ..color = const Color(0x45effb00)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4));
    }
    c.drawShadow(Path()..addRRect(rect), Colors.black.withValues(alpha: .6), 3,
        false);
    mixerBox(c, Offset.zero & s, MixerDesign.cap,
        radius: 14, border: const Color(0xff555555));
    c.drawLine(const Offset(6, .5), Offset(s.width - 6, .5),
        Paint()..color = const Color(0xbfffffff));
    // Ribbed grips: repeating dark slits across the top and bottom zones.
    // All insets are proportional so the small phone cap keeps the same
    // look as the large tablet one (design: 14px inset on an 84px cap).
    final ix = s.width * .17;
    final ribTop = s.height * .14;
    final ribGap = s.height * .08;
    for (var zone = 0; zone < 2; zone++) {
      final top = zone == 0 ? ribTop : s.height - ribTop - 2 * ribGap - 3;
      for (var row = 0; row < 3; row++) {
        mixerBox(
            c,
            Rect.fromLTWH(ix, top + row * ribGap, s.width - 2 * ix, 2),
            const Color(0xff151515),
            radius: 1);
        mixerBox(
            c,
            Rect.fromLTWH(ix, top + row * ribGap + 2, s.width - 2 * ix, 1),
            const Color(0xff585858),
            radius: 1);
      }
    }
    // Lime position line through the middle.
    final lineH = (s.height * .06).clamp(3.0, 6.0);
    mixerBox(
        c,
        Rect.fromLTWH(ix, s.height / 2 - lineH / 2, s.width - 2 * ix, lineH),
        MixerDesign.lime,
        radius: 3);
  }

  @override
  bool shouldRepaint(_CapPainter old) =>
      old.held != held || old.glow != glow;
}
