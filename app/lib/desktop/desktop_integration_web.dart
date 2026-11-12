import 'package:material_ui/material_ui.dart';

/// Browser windows remain owned by the browser, never the desktop host.
class DesktopIntegration {
  static bool get supported => false;
  static Future<DesktopIntegration?> initialize() async => null;
  final navigatorKey = GlobalKey<NavigatorState>();
  void attach(Object owner, Future<bool> Function() prepareQuit) {}
  void detach(Object owner) {}
  void updateStatus(String status) {}
  void pointerDown(PointerDownEvent event) {}
  void pointerEnded(PointerEvent event) {}
  Future<void> show() async {}
  Future<void> requestQuit() async {}
}
