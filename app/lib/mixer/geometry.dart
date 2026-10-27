import 'dart:math' as math;

/// Density helpers for the mixer strip row. Strips are flow-laid-out
/// ([StripLayout.visibleOrder] in a Column); absolute rack/pan/mute/fader
/// positions no longer exist.
class MixerGeometry {
  const MixerGeometry._();

  /// Minimum touch size for strip controls.
  static double target(double visual) => math.max(44, visual);

  /// Strip width for [count] strips per page (plus the fixed master strip on
  /// desktop). Defaults to 8 to preserve today's layout; custom layers pass
  /// their own density.
  static double stripWidth(double availableWidth, bool phone,
          [int count = 8]) =>
      phone
          ? availableWidth * 104.2 / 466 * 8 / count.clamp(4, 12)
          : (availableWidth - railWidth(availableWidth)) /
              (count.clamp(4, 12) + 1);
  static double railWidth(double availableWidth) => availableWidth * 93 / 1823;
}
