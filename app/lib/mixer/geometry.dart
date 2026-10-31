import 'dart:math' as math;

/// HTML coordinates before the tablet export's 1.09714756 transform.
/// SafeArea removes only the authored system row (46 phone / 35 tablet).
/// The toolbar owns its geometry; strips map into the remaining height.
class MixerGeometry {
  final bool phone;
  final double width, height;
  const MixerGeometry(
      {required this.phone, required this.width, required this.height});
  double get authoredHeight => phone ? 882 : 1259;
  double get sy => height / authoredHeight;
  double get sx => width / (phone ? 104.2 : 192.2);
  double y(double value) => value * sy;
  double x(double value) => value * sx;
  double get left => x(phone ? 4 : 7);
  double get right => left;
  // Reclaim the tablet top gap for fader/meter travel.
  double get rackY => y(6);
  double get rackH => y(phone ? 62 : 188);
  double get panY => y(phone ? 76 : 209);
  double get panH => y(phone ? 38 : 58);
  double get muteY => y(phone ? 124 : 274);
  double get muteH => y(phone ? 40 : 57);
  double get faderY => y(phone ? 174 : 346);
  double get faderH => y(phone ? 534 : 673);
  double get soloY => y(phone ? 720 : 1034);
  double get soloH => y(phone ? 36 : 57);
  double get scribbleY => y(phone ? 760 : 1097);
  double get scribbleH => y(phone ? 86 : 122);
  double get nameY => y(phone ? 846 : 1208);
  double get nameH => y(phone ? 36 : 51);
  double get nameText => x(phone ? 17 : 26);
  double target(double visual) => math.max(44, visual);
  static double stripWidth(double availableWidth, bool phone, [int count = 8]) => phone
      ? availableWidth * 104.2 / 466 * 8 / count.clamp(4, 12)
      : math.min(120, (availableWidth - railWidth(availableWidth)) / (count.clamp(4, 12) + 1));
  static double railWidth(double availableWidth) =>
      math.max(88, availableWidth * 93 / 1823);
}
