import 'package:flutter/material.dart';

/// Shared typography and shape. Runtime colors live in ConsolePalette;
/// widgets resolve them through the inherited Flutter theme.
abstract final class AppDesign {
  static const radius = BorderRadius.all(Radius.circular(10));
  static const font = 'Barlow';

  /// Larger touch targets (Appearance setting): chunkier fader caps and
  /// knobs. Read at build time; pages listen to prefs so it applies live.
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
