import 'package:flutter/material.dart';

/// Shared typography and shape. Runtime colors live in ConsolePalette.
abstract final class AppDesign {
  static const radius = BorderRadius.all(Radius.circular(10));
  static const font = 'Barlow';

  /// Larger touch targets (Appearance setting). Read at build time so
  /// pages listening to prefs apply it live.
  static bool largeTargets = true;
  static TextStyle type(double size,
          {Color? color, FontWeight weight = FontWeight.w400}) =>
      TextStyle(
          fontFamily: font,
          height: 1.15,
          fontSize: size,
          color: color,
          fontWeight: weight);
}
