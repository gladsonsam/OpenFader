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
  double get left => x(phone ? 5 : 8);
  double get right => x(phone ? 3 : 6);
  double get rackY => y(phone ? 6 : 24);
  double get rackH => y(phone ? 62 : 202);
  double get panY => y(phone ? 76 : 235);
  double get panH => y(phone ? 38 : 58);
  double get muteY => y(phone ? 124 : 300);
  double get muteH => y(phone ? 40 : 57);
  double get faderY => y(phone ? 174 : 372);
  double get faderH => y(phone ? 534 : 647);
  double get soloY => y(phone ? 720 : 1034);
  double get soloH => y(phone ? 36 : 57);
  double get scribbleY => y(phone ? 760 : 1097);
  double get scribbleH => y(phone ? 86 : 122);
  double get nameY => y(phone ? 846 : 1208);
  double get nameH => y(phone ? 36 : 51);
  double get nameText => x(phone ? 17 : 26);
  double target(double visual) => math.max(44, visual);
  static double stripWidth(double availableWidth, bool phone) => phone
      ? availableWidth * 104.2 / 466
      : (availableWidth - railWidth(availableWidth)) / 9;
  static double railWidth(double availableWidth) => availableWidth * 93 / 1823;
}
