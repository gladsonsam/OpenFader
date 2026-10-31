import '../palette.dart';
import 'dart:math' as math;
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:vector_graphics/vector_graphics_compat.dart'
    show RenderingStrategy;
import '../u4law.dart';

import '../design.dart';

// Retain mixer call sites while every screen shares the same design tokens.
typedef MixerDesign = AppDesign;

abstract final class MixerStripMetrics {
  static const contentInset = 12.0;
}

/// Reference travel is a display coordinate. Engine values continue to use
/// the canonical law in u4law.dart; no protocol values depend on screen size.
abstract final class MixerFaderScale {
  static double db(double v) => v <= 0
      ? double.negativeInfinity
      : v >= .758
          ? (v - .758) / .0242
          : -math.pow((.758 - v) / .758, 1.45).toDouble() * 80;
  static double toEngine(double position) => dbToPos(db(position.clamp(0, 1)));
  static double fromEngine(double value) {
    final d = posToDb(value);
    if (!d.isFinite) return 0;
    return (d >= 0
            ? .758 + d * .0242
            : .758 - .758 * math.pow((-d / 80).clamp(0, 1), 1 / 1.45))
        .clamp(0, 1)
        .toDouble();
  }

  static const marks = [
    (10.0, '10'),
    (5.0, '5'),
    (0.0, 'U'),
    (-5.0, '5'),
    (-10.0, '10'),
    (-20.0, '20'),
    (-30.0, '30'),
    (-40.0, '40'),
    (-50.0, '50'),
    (-60.0, '60'),
    (-80.0, '∞')
  ];
  static double positionForDb(double d) => fromEngine(dbToPos(d));

  /// Meter events carry linear amplitude, not a fader position or fill ratio.
  /// Convert to dBFS before placing the signal against the shared dB ruler.
  static double positionForLevel(double level) {
    if (!level.isFinite || level <= 0) return 0;
    return positionForDb(20 * math.log(level.clamp(0.0, 1.0)) / math.ln10);
  }
}

class MixerArtwork extends StatelessWidget {
  final String name;
  final double width, height;
  final BoxFit fit;
  const MixerArtwork(this.name,
      {super.key,
      this.width = 28,
      this.height = 28,
      this.fit = BoxFit.contain});
  @override
  Widget build(BuildContext context) {
    final (w, h) = name.startsWith('instrument_')
        ? (120.0, 120.0)
        : name == 'connection'
            ? (60.0, 70.0)
            : (60.0, 60.0);
    return SizedBox(
        width: width,
        height: height,
        child: FittedBox(
            fit: fit,
            child: SvgPicture.asset(
                renderingStrategy: RenderingStrategy.raster,
                'assets/mixer/$name.svg',
                width: w,
                height: h,
                colorMapper: _ArtworkColors(ConsolePalette.of(context)),
                colorFilter: name == 'connection'
                    ? ColorFilter.mode(
                        ConsolePalette.of(context).mutedText, BlendMode.srcIn)
                    : null)));
  }
}

void mixerBox(Canvas c, Rect rect, Color color,
    {double radius = 0, Color? border, double stroke = 1}) {
  final rounded = RRect.fromRectAndRadius(rect, Radius.circular(radius));
  c.drawRRect(rounded, Paint()..color = color);
  if (border != null) {
    c.drawRRect(
        rounded.deflate(stroke / 2),
        Paint()
          ..color = border
          ..style = PaintingStyle.stroke
          ..strokeWidth = stroke);
  }
}

void mixerText(Canvas c, String text, Offset offset, double size,
    {required Color color,
    FontWeight weight = FontWeight.w400,
    double? width,
    TextAlign align = TextAlign.left}) {
  final p = TextPainter(
      text: TextSpan(
          text: text,
          style: MixerDesign.type(size, color: color, weight: weight)),
      textDirection: TextDirection.ltr,
      textAlign: align,
      maxLines: 1)
    ..layout(minWidth: width ?? 0, maxWidth: width ?? double.infinity);
  p.paint(c, offset);
}

TextStyle mixerType(BuildContext context, double size,
        {Color? color, FontWeight weight = FontWeight.w400}) =>
    MixerDesign.type(size,
        color: color ?? ConsolePalette.of(context).text, weight: weight);

/// Brand assets are loaded separately and never pass through this mapper.
class _ArtworkColors extends ColorMapper {
  final ConsolePalette palette;
  const _ArtworkColors(this.palette);
  @override
  Color substitute(
          String? id, String elementName, String attributeName, Color color) =>
      switch (color.toARGB32()) {
        0xff233306 => palette.eqBg,
        0xff969d88 => palette.panFill,
        0xff526c1b => palette.greenBack,
        0xffeef2e9 || 0xfff1f3e9 => palette.text,
        0xffb5b5b5 => palette.mutedText,
        0xff222222 => palette.background,
        _ => color,
      };
  @override
  bool operator ==(Object other) =>
      other is _ArtworkColors && other.palette == palette;
  @override
  int get hashCode => palette.hashCode;
}
