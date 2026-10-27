import 'dart:math' as math;

/// Density helpers for the mixer strip row. Strips are flow-laid-out
/// ([StripLayout.visibleOrder] in a Column); absolute rack/pan/mute/fader
/// positions no longer exist.
class MixerGeometry {
  const MixerGeometry._();

  /// Minimum touch size for strip controls.
  static double target(double visual) => math.max(44, visual);

  static double stripWidth(double availableWidth, bool phone) => phone
      ? availableWidth * 104.2 / 466
      : (availableWidth - railWidth(availableWidth)) / 9;
  static double railWidth(double availableWidth) => availableWidth * 93 / 1823;
}
