// Real transport: embedded `openfader-engine` via the FRB bindings in
// `bridge/generated/` (see `bridge_stub.dart`). Same `EngineApi` surface
// as `FakeSimEngine`, so the UI is transport-agnostic.
//
// Construction never throws: call `FfiEngine.create()` — it returns an
// `FfiEngine` when the native bridge loads, else `null` and
// the caller falls back to `FakeSimEngine` (demo still works without FFI).
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

import 'bridge/generated/api.dart' as frb;
import 'bridge/generated/frb_generated.dart';
import 'engine.dart';

class FfiEngine implements EngineApi {
  FfiEngine._();

  /// Last FFI init failure (null when the last create() succeeded).
  /// Read this after `create()` returns null to show WHY the sim fallback
  /// won: missing native symbols vs init timeout matters on device.
  static Object? lastError;

  /// Init the FRB binding (linked iOS archive or Android shared library).
  /// Returns `null` when the native library is missing/broken.
  static Future<FfiEngine?> create() async {
    lastError = null;
    try {
      await OpenfaderBridge.init(
        // iOS links the Rust static archive into Runner. Android loads .so.
        externalLibrary: Platform.isIOS
            ? ExternalLibrary.process(iKnowHowToUseIt: true)
            : Platform.isMacOS
                ? ExternalLibrary.open(
                    '${File(Platform.resolvedExecutable).parent.parent.path}/Frameworks/libopenfader_flutter_bridge.dylib')
                : Platform.isLinux
                    ? ExternalLibrary.open(
                        '${File(Platform.resolvedExecutable).parent.path}/lib/libopenfader_flutter_bridge.so')
                    : Platform.isWindows
                        ? ExternalLibrary.open(
                            '${File(Platform.resolvedExecutable).parent.path}/openfader_flutter_bridge.dll')
                        : null,
      ).timeout(const Duration(seconds: 5));
      debugPrint('OpenFader: embedded native engine initialized');
      return FfiEngine._();
    } catch (e) {
      lastError = e;
      debugPrint('OpenFader: native engine initialization failed: $e');
      return null;
    }
  }

  @override
  Future<String> boot() async => frb.boot();

  @override
  Future<void> cmd(String clientId, String json) =>
      frb.cmd(clientId: clientId, json: json);

  @override
  Future<List<String>> poll(String clientId, int cap) =>
      frb.poll(clientId: clientId, cap: cap);

  /// Binary meter fan-out (`poll_binary` over FRB): length-prefixed
  /// `Event::to_binary` frames; throws on a stale native library missing
  /// the symbol (the caller falls back to JSON meters from [poll]).
  @override
  Future<Uint8List> pollBinary(String clientId, int cap) =>
      frb.pollBinary(clientId: clientId, cap: cap);

  @override
  Future<void> resync(String clientId) => frb.resync(clientId: clientId);
}
