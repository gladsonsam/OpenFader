import 'host_transport.dart';

/// Local desktop administration is unavailable to browser clients.
abstract class DesktopHost implements HostTransport {
  String get instanceId;
  static Future<DesktopHost> start() async =>
      throw UnsupportedError('The browser connects to its hosting server.');
  Future<Map<String, dynamic>> remoteAdminRequest(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)});
}
