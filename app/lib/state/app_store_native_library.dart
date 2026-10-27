part of 'app_store.dart';

abstract interface class _AppStoreNativeLibraryContract {
  void _pollNativeLibrary(double now);
  void _receiveNativeLibrary(Map<String, dynamic> frame);
  void cancelNativeLibrary(Object owner);
  void _stopNativeLibrary();
}

class _NativeRequest {
  final Object owner;
  final Object session;
  final String driver;
  final int epoch;
  final int accessEpoch;
  final String op;
  final double deadline;
  final Completer<Map<String, dynamic>> done = Completer();
  _NativeRequest(
    this.owner,
    this.session,
    this.driver,
    this.epoch,
    this.accessEpoch,
    this.op,
    this.deadline,
  );
}

mixin _AppStoreNativeLibrary on _AppStoreContext {
  final _nativeRequests = <String, _NativeRequest>{};
  int _nativeSequence = 0;

  Future<Map<String, dynamic>> requestNativeLibrary(
    Object owner,
    Map<String, dynamic> action,
    NativeLibraryVm? expected,
  ) async {
    final epoch = engine.syncEpoch;
    if (!engine.connected || epoch == null) {
      return {
        'status': 'rejected',
        'message': 'Connect before inspecting a provider library.',
      };
    }
    final id =
        'native-${DateTime.now().microsecondsSinceEpoch}-${++_nativeSequence}';
    final request = _NativeRequest(
      owner,
      engine.session,
      engine.driver,
      epoch,
      controlAccessEpoch,
      action['op'] as String,
      _nowMs() + 20000,
    );
    _nativeRequests[id] = request;
    // One send, no coalescer, offline queue, retry or optimistic inventory.
    unawaited(
      _command(
        _clientId,
        jsonEncode({
          'cmd': 'native_library',
          'request_id': id,
          'driver': request.driver,
          'epoch': epoch,
          'action': action,
          'expected': expected?.guard,
        }),
      ).catchError((Object error) {
        _finishNative(id, {
          'status': 'uncertain',
          'message':
              'Transport/access interrupted: $error. No replay; inspect again.',
        });
      }),
    );
    return request.done.future;
  }

  void _finishNative(String id, Map<String, dynamic> result) {
    final request = _nativeRequests.remove(id);
    if (request != null && !request.done.isCompleted) {
      request.done.complete(result);
    }
  }

  @override
  void _receiveNativeLibrary(Map<String, dynamic> frame) {
    final id = frame['request_id'];
    final request = _nativeRequests[id];
    if (request == null ||
        frame['driver'] != request.driver ||
        frame['epoch'] != request.epoch ||
        frame['operation'] != request.op ||
        controlAccessEpoch != request.accessEpoch ||
        engine.session != request.session) {
      return;
    }
    final result = frame['result'];
    if (result is! Map<String, dynamic> ||
        !const [
          'confirmed',
          'uncertain',
          'rejected',
        ].contains(result['status']) ||
        result['message'] is! String) {
      return;
    }
    _finishNative(id as String, result);
  }

  @override
  void cancelNativeLibrary(Object owner) {
    for (final entry in _nativeRequests.entries.toList()) {
      if (!identical(entry.value.owner, owner)) continue;
      // Cancellation must reach the engine even while access persistence is
      // saving. It revokes an admitted request; it never authorizes a write.
      unawaited(
        _api
            .cmd(
              _clientId,
              jsonEncode({
                'cmd': 'native_library_cancel',
                'request_id': entry.key,
              }),
            )
            .catchError((Object error) {
              _noteTransportError(error);
            }),
      );
      _finishNative(entry.key, {
        'status': 'uncertain',
        'message': 'Operation interrupted. It may already have taken effect; inspect again. No replay.',
      });
    }
  }

  @override
  void _stopNativeLibrary() {
    for (final owner in _nativeRequests.values.map((r) => r.owner).toSet()) {
      cancelNativeLibrary(owner);
    }
  }

  @override
  void _pollNativeLibrary(double now) {
    for (final entry in _nativeRequests.entries.toList()) {
      if (now >= entry.value.deadline ||
          engine.session != entry.value.session ||
          controlAccessEpoch != entry.value.accessEpoch ||
          !engine.connected ||
          blockedAccess.isNotEmpty &&
              !const [
                'inspect',
                'refresh',
                'read_show',
                'export_file',
                'inspect_file',
              ].contains(entry.value.op)) {
        cancelNativeLibrary(entry.value.owner);
      }
    }
  }
}
