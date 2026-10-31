import '../engine_protocol.dart';

/// An injected host connection. Credentials remain owned by the transport.
abstract interface class HostTransport implements EngineApi {
  Uri get endpoint;
  bool get supportsAdministration;
  Future<Map<String, dynamic>> recorderRequest(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)});

  /// Remote hosts reject this operation.
  Future<void> quitHost();
}
