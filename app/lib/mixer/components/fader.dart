import '../../palette.dart';
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
  final bool compact;
  final bool showMeter;
  final ValueChanged<double> onPosition;
  final VoidCallback onSelect, onUnity;
  final VoidCallback? onTouch;
  const MixerFader(
      {super.key,
      required this.channel,
      this.compact = false,
      this.showMeter = true,
      required this.onPosition,
      required this.onSelect,
      this.onTouch,
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
        final geometry = _FaderGeometry(box.biggest, widget.compact);
        final position = _drag ?? MixerFaderScale.fromEngine(ch.fader);
        final y = geometry.top + (1 - position) * geometry.travel;
        // Enlarge only during a drag; selection keeps its border and glow.
        final scale = _drag != null ? (widget.compact ? 1.25 : 1.4) : 1.0;
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
                    color: ConsolePalette.of(context).well,
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(
                        color: ch.selected
                            ? ConsolePalette.of(context).selectedLine
                            : ConsolePalette.of(context).line,
                        width: ch.selected ? 1.5 : 1)),
                child: Stack(clipBehavior: Clip.none, children: [
                  Positioned.fill(
                      child: CustomPaint(
                          painter: _ScalePainter(ConsolePalette.of(context),
                              geometry, ch.number))),
                  Positioned(
                      left: box.maxWidth *
                          (widget.compact ? 22 / 96.2 : 45 / 178.2),
                      top: geometry.top - MixerMeter.scaleInset,
                      width: box.maxWidth *
                          (widget.compact ? 20 / 96.2 : 36 / 178.2),
                      bottom: geometry.bottom,
                      child: widget.showMeter
                          ? MixerMeter(level: ch.meter, stereo: ch.master)
                          : const SizedBox.shrink()),
                  Positioned(
                      left: geometry.x -
                          geometry.capWidth / 2 -
                          geometry.touchPad,
                      top: y - geometry.capHeight / 2 - geometry.touchPad,
                      width: geometry.capWidth + geometry.touchPad * 2,
                      height: geometry.capHeight + geometry.touchPad * 2,
                      child: Tooltip(
                          message: '${ch.levelLabel} · double-tap for unity',
                          triggerMode: TooltipTriggerMode.manual,
                          child: Listener(
                              onPointerDown: (_) {
                                widget.onTouch?.call();
                                setState(() => _held = true);
                              },
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
                                      padding:
                                          EdgeInsets.all(geometry.touchPad),
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
                                                  palette: ConsolePalette.of(
                                                      context),
                                                  held: _held,
                                                  glow: ch.selected)))))))),
                ])));
      });
}

class _FaderGeometry {
  final Size size;
  final bool compact;
  const _FaderGeometry(this.size, this.compact);
  double get top => size.height * (compact ? 38 / 534 : 65 / 647);
  double get bottom => size.height * (compact ? 56 / 534 : 55 / 647);
  double get travel => math.max(40, size.height - top - bottom);
  double get x => size.width * (compact ? 57 / 96.2 : 101 / 178.2);
  double get capWidth => math.max(
      34,
      size.width *
          (compact ? 54 / 96.2 : 84 / 178.2) *
          (!compact && MixerDesign.largeTargets ? 1.08 : 1));
  double get capHeight => capWidth;

  double get touchPad => MixerDesign.largeTargets ? 10 : 6;
}

class _ScalePainter extends CustomPainter {
  final ConsolePalette palette;
  final _FaderGeometry g;
  final String number;
  const _ScalePainter(this.palette, this.g, this.number);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Rect.fromLTWH(g.x - 3, g.top - 3, 6, g.travel + 6),
        palette.faderSlot,
        radius: 3, border: palette.faderSlotBorder);
    // Design number column (tablet+ only): 1-6 repeated in three banks.
    // The mock marks the third bank's "1" red on channel 1; replicate it.
    if (!g.compact) {
      final size = (s.width * .088).clamp(12.0, 18.0);
      for (var i = 0; i < 18; i++) {
        final y = g.top + (i + .5) / 18 * g.travel - size / 2;
        final red = number == '1' && i == 12;
        mixerText(c, '${i % 6 + 1}', Offset(s.width * .08, y), size,
            color: red ? palette.mute : palette.text10,
            weight: FontWeight.w600);
      }
    }
    for (final mark in MixerFaderScale.marks) {
      final y = g.top + (1 - MixerFaderScale.positionForDb(mark.$1)) * g.travel;
      final x = s.width * (g.compact ? 66 / 96.2 : 127 / 178.2);
      // Design prefixes: • labeled (/ – on phones), ○ unity, · minor.
      final label = mark.$2;
      final prefix = label == 'U'
          ? '○'
          : label.isEmpty
              ? '·'
              : s.width < 140
                  ? '–'
                  : '•';
      mixerText(c, '$prefix$label', Offset(x, y - 6),
          s.width * (g.compact ? 11 / 96.2 : 14 / 178.2),
          color: palette.scaleTick);
    }
  }

  @override
  bool shouldRepaint(_ScalePainter old) =>
      old.palette != palette || old.g.size != g.size || old.number != number;
}

class _CapPainter extends CustomPainter {
  final ConsolePalette palette;
  final bool held, glow;
  const _CapPainter(
      {required this.palette, required this.held, this.glow = false});
  @override
  void paint(Canvas c, Size s) {
    // Design cap: one rounded square with ribbed grip zones top + bottom
    // and the lime position line through the middle.
    final rect =
        RRect.fromRectAndRadius(Offset.zero & s, const Radius.circular(14));
    if (glow) {
      c.drawPath(
          Path()..addRRect(rect),
          Paint()
            ..color = palette.faderAccent.withValues(alpha: .4)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 5));
    }
    if (held) {
      c.drawPath(
          Path()..addRRect(rect),
          Paint()
            ..color = palette.faderAccent.withValues(alpha: .27)
            ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4));
    }
    c.drawShadow(Path()..addRRect(rect), palette.shadow, 3, false);
    mixerBox(c, Offset.zero & s, palette.cap,
        radius: 14, border: palette.capBorder);
    // Ribbed grips: repeating dark slits across the top and bottom zones.
    // All insets are proportional so the small phone cap keeps the same
    // look as the large tablet one (design: 14px inset on an 84px cap).
    final ix = s.width * .17;
    final ribTop = s.height * .14;
    final ribGap = s.height * .08;
    for (var zone = 0; zone < 2; zone++) {
      final top = zone == 0 ? ribTop : s.height - ribTop - 2 * ribGap - 3;
      for (var row = 0; row < 3; row++) {
        mixerBox(c, Rect.fromLTWH(ix, top + row * ribGap, s.width - 2 * ix, 2),
            palette.capGrip,
            radius: 1);
        mixerBox(
            c,
            Rect.fromLTWH(ix, top + row * ribGap + 2, s.width - 2 * ix, 1),
            palette.capGripLight,
            radius: 1);
      }
    }
    final lineH = (s.height * .06).clamp(3.0, 6.0);
    mixerBox(
        c,
        Rect.fromLTWH(ix, s.height / 2 - lineH / 2, s.width - 2 * ix, lineH),
        palette.faderAccent,
        radius: 3);
  }

  @override
  bool shouldRepaint(_CapPainter old) =>
      old.palette != palette || old.held != held || old.glow != glow;
}
