// Real transport: embedded `openfader-engine` via the FRB bindings in
// `bridge/generated/`. Same `EngineApi` surface as the sim engine.
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:path_provider/path_provider.dart';

import 'bridge/generated/api.dart' as frb;
import 'bridge/generated/frb_generated.dart';
import 'engine.dart';

class FfiEngine implements EngineApi {
  FfiEngine._();

  // Keep the underlying init even if a caller times out: it may still finish.
  // Only an actual init failure permits a new bridge initialization attempt.
  static Future<void>? _nativeInitialization;

  String? _clientId;
  Future<String>? _bootInFlight;

  /// Last FFI init failure, so the caller can report why sim fallback won.
  static Object? lastError;

  /// Binds the FRB bridge (linked iOS archive or Android shared library).
  /// Returns `null` when the native bridge cannot initialize. Durable engine
  /// storage is opened separately by [boot], whose failures must propagate.
  static Future<FfiEngine?> create() async {
    lastError = null;
    try {
      await (_nativeInitialization ??= _initializeNative().onError(
        (Object error, StackTrace stackTrace) {
          _nativeInitialization = null;
          Error.throwWithStackTrace(error, stackTrace);
        },
      ))
          .timeout(const Duration(seconds: 5));
      return FfiEngine._();
    } catch (e) {
      lastError = e;
      debugPrint('OpenFader: native engine initialization failed: $e');
      return null;
    }
  }

  static Future<void> _initializeNative() async {
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
    );
    debugPrint('OpenFader: embedded native engine initialized');
  }

  @override
  Future<String> boot() {
    final id = _clientId;
    if (id != null) return Future<String>.value(id);
    return _bootInFlight ??= _bootNative().whenComplete(() {
      // Failed loads can be retried; a successful client survives sync retries.
      _bootInFlight = null;
    });
  }

  Future<String> _bootNative() async {
    final Directory support;
    try {
      support = await getApplicationSupportDirectory();
    } catch (e) {
      throw FileSystemException(
          'Could not locate OpenFader application support storage. '
          'Check app storage access and retry. $e');
    }
    final directory =
        Directory('${support.path}${Platform.pathSeparator}openfader_engine');
    try {
      await directory.create(recursive: true);
    } catch (e) {
      throw FileSystemException(
          'Could not create the OpenFader engine data directory. '
          'Check available storage and folder permissions, then retry. $e',
          directory.path);
    }
    // Native boot validates and loads the durable library before publishing
    // its singleton. Never convert a storage/load failure into a fake engine.
    final id = await frb.boot(dataDir: directory.path);
    _clientId = id;
    return id;
  }

  @override
  Future<void> cmd(String clientId, String json) =>
      frb.cmd(clientId: clientId, json: json);

  @override
  Future<List<String>> poll(String clientId, int cap) =>
      frb.poll(clientId: clientId, cap: cap);

  /// Binary meter fan-out over FRB. Throws on a stale native library
  /// missing the symbol, so the caller falls back to JSON meters.
  @override
  Future<Uint8List> pollBinary(String clientId, int cap) =>
      frb.pollBinary(clientId: clientId, cap: cap);

  @override
  Future<void> resync(String clientId) => frb.resync(clientId: clientId);
}
