import 'engine_protocol.dart';

abstract class FfiEngine implements EngineApi {
  static Object? lastError;
  static Future<FfiEngine?> create() async => null;
}
