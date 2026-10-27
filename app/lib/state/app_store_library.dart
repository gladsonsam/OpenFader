part of 'app_store.dart';

// Local show/scene library transactions, reset reservations and saved-address acknowledgements.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreLibraryContract {
  Map<String, _LibraryRequest> get _libraryRequests;
  Set<_LibraryRecall> get _libraryRecalls;
  int get _libraryGeneration;
  set _libraryGeneration(int value);
  void refreshLocalLibrary();
  void _interruptAllLibrary(String message);
  void _interruptForgetSavedMixers(String message);
  List<_LibraryQuery> get _libraryQueries;
  void _receiveLibraryResult(Map<String, dynamic> frame);
  void _checkLibraryQuery(_LibraryQuery query, double now);
  void _pollLibrary(double now);
  void _receiveForgetSavedMixersResult(Map<String, dynamic> frame);
}

mixin _AppStoreLibrary on _AppStoreContext {
  // Library control traffic shares the shell poll. No page timers/pollers.
  @override
  final Map<String, _LibraryRequest> _libraryRequests = {};
  @override
  final Set<_LibraryRecall> _libraryRecalls = {};
  @override
  final List<_LibraryQuery> _libraryQueries = [];
  final Map<String, _ForgetSavedMixersRequest> _forgetSavedMixersRequests = {};
  int _libraryId = 0;
  final String _libraryClientNonce =
      '${DateTime.now().microsecondsSinceEpoch}-${identityHashCode(Object())}';
  @override
  int _libraryGeneration = 0;
  bool _libraryResetting = false;
  bool _libraryDisposed = false;
  int get libraryGeneration => _libraryGeneration;
  bool get libraryResetting => _libraryResetting;
  bool get libraryPending =>
      _libraryRequests.isNotEmpty || _libraryRecalls.isNotEmpty;
  bool get libraryActive =>
      _libraryRequests.values.any((request) => !request.done.isCompleted) ||
      _libraryRecalls.any((request) => !request.done.isCompleted);
  bool get libraryWritable =>
      blockedAccess.isEmpty &&
      !_libraryDisposed &&
      !_libraryResetting &&
      !libraryPending &&
      engine.localLibrary?.status == LocalLibraryHealth.ready;

  Future<LocalLibraryStatus> queryLocalLibrary() {
    if (_libraryDisposed) {
      return Future.error(StateError('The engine was closed.'));
    }
    final query = _LibraryQuery(engine.session, _nowMs() + 10000);
    _libraryQueries.add(query);
    unawaited(
        Future<void>.sync(() => _command(_clientId, Cmds.localLibraryStatus()))
            .then((_) {
      _checkLibraryQuery(query, _nowMs());
    }, onError: (Object error) {
      _checkLibraryQuery(query, _nowMs());
      _libraryQueries.remove(query);
      if (!query.done.isCompleted) {
        query.done
            .completeError(StateError('Library status unavailable: $error'));
      }
    }));
    return query.done.future;
  }

  @override
  void refreshLocalLibrary() {
    unawaited(queryLocalLibrary().then((_) {}, onError: (Object _) {}));
  }

  Future<int> localLibraryStorageBytes() async {
    final status = await queryLocalLibrary();
    if (status.mode == LocalLibraryMode.memory) return 0;
    final bytes = status.bytes;
    if (bytes == null) {
      throw StateError('Local library storage size is unavailable.');
    }
    return bytes;
  }

  void _interruptLibrary(_LibraryRequest request, String message) {
    if (!request.done.isCompleted) {
      request.done.complete(LocalLibraryCompletion.interrupted(message));
    }
  }

  void cancelLocalLibraryOperations(Object owner, String message) {
    for (final request in _libraryRequests.values) {
      if (identical(request.owner, owner)) _interruptLibrary(request, message);
    }
    for (final request in _libraryRecalls) {
      if (identical(request.owner, owner) && !request.done.isCompleted) {
        request.done.complete(message);
      }
    }
    // Retain dispatched requests until their explicit result. A reset cannot
    // overtake an unresolved write merely because its page stopped waiting.
  }

  Future<String?> requestLocalLibraryRecall(String command, Object owner) {
    if (!libraryWritable || !engine.connected) {
      return Future.value(
          'Recall was not dispatched: library unavailable or busy.');
    }
    final request = _LibraryRecall(owner, engine.session, _nowMs() + 15000);
    _libraryRecalls.add(request);
    notifyListeners();
    unawaited(Future<void>.sync(() => _command(_clientId, command)).then((_) {
      _checkLibraryRecall(request, _nowMs());
      _libraryRecalls.remove(request);
      if (!request.done.isCompleted) request.done.complete(null);
      if (!_libraryDisposed) notifyListeners();
    }, onError: (Object error) {
      _checkLibraryRecall(request, _nowMs());
      // Transport failure cannot prove the legacy recall stopped. Keep the
      // reservation so reset cannot race a late current-marker update.
      if (!request.done.isCompleted) {
        request.done.complete('Recall transport failed: $error.');
      }
      if (!_libraryDisposed) notifyListeners();
    }));
    return request.done.future;
  }

  @override
  void _interruptAllLibrary(String message) {
    for (final request in _libraryRecalls) {
      if (!request.done.isCompleted) request.done.complete(message);
    }
    for (final request in _libraryRequests.values) {
      _interruptLibrary(request, message);
    }
    for (final query in _libraryQueries) {
      if (!query.done.isCompleted) {
        query.done.completeError(StateError(message));
      }
    }
    _libraryQueries.clear();
  }

  Future<LocalLibraryCompletion> mutateLocalLibrary(LocalLibraryAction action,
          {required int expectedRevision, Object? owner}) =>
      _submitLibraryMutation(action,
          expectedRevision: expectedRevision, owner: owner);

  Future<LocalLibraryCompletion> _submitLibraryMutation(
      LocalLibraryAction action,
      {required int expectedRevision,
      Object? owner,
      bool resetting = false}) {
    final status = engine.localLibrary;
    final reset = action.operation == 'reset';
    if (blockedAccess.isNotEmpty ||
        _libraryDisposed ||
        (_libraryResetting && !resetting) ||
        libraryPending ||
        status == null ||
        status.revision != expectedRevision ||
        (!reset && status.status != LocalLibraryHealth.ready)) {
      // No dispatch occurred, so this is a definite local refusal.
      return Future.value(const LocalLibraryCompletion.notDispatched(
          'Library action was not dispatched: refresh the library and review its current state.'));
    }
    final id = 'library-$_libraryClientNonce-${++_libraryId}';
    final request = _LibraryRequest(id, action, expectedRevision, status.mode,
        engine.session, owner, _nowMs() + 15000);
    _libraryRequests[id] = request;
    notifyListeners();
    // Invoke before returning: all mutations enter the engine's serialized
    // commit path in submission order, before reset can inspect the queue.
    unawaited(Future<void>.sync(() => _command(
            _clientId, Cmds.localLibraryMutate(id, expectedRevision, action)))
        .then((_) {
      _checkLibraryRequest(request, _nowMs());
    }, onError: (Object error) {
      _checkLibraryRequest(request, _nowMs());
      if (identical(_libraryRequests[id], request)) {
        _interruptLibrary(request, 'Library transport failed: $error.');
        if (!_libraryDisposed) notifyListeners();
      }
    }));
    return request.done.future;
  }

  @override
  void _receiveLibraryResult(Map<String, dynamic> frame) {
    final id = frame['request_id'];
    final request = id is String ? _libraryRequests[id] : null;
    // Never correlate by list, error, or operation alone.
    if (request == null) return;
    // The awaited poll may have crossed the deadline since its initial sweep.
    // A late authoritative result can release the reservation, never revive
    // an interrupted caller (including one from an earlier session).
    _checkLibraryRequest(request, _nowMs());
    final result = LocalLibraryResult.fromJson(frame);
    if (result == null ||
        result.operation != request.action.operation ||
        result.mode != request.mode ||
        result.revision < request.revision ||
        (result.status == LocalLibraryResultStatus.committed &&
            result.revision <= request.revision)) {
      _interruptLibrary(request,
          'The engine returned an invalid or mismatched library result.');
      return;
    }
    _checkLibraryRequest(request, _nowMs());
    _libraryRequests.remove(id);
    if (!request.done.isCompleted) {
      request.done.complete(LocalLibraryCompletion.result(result));
    }
    engine.invalidateLocalLibraryStatus(result.revision);
    if (engine.localLibrary == null) refreshLocalLibrary();
  }

  void _checkLibraryRecall(_LibraryRecall request, double now) {
    if (!request.done.isCompleted &&
        (_libraryDisposed ||
            request.session != engine.session ||
            now >= request.deadline)) {
      request.done
          .complete('Recall result unconfirmed; the mixer may have changed.');
    }
  }

  void _checkLibraryRequest(_LibraryRequest request, double now) {
    if (_libraryDisposed || request.session != engine.session) {
      _interruptLibrary(
          request, 'Connection changed while awaiting the library result.');
    } else if (now >= request.deadline) {
      _interruptLibrary(
          request, 'No correlated library result arrived before the deadline.');
    }
  }

  @override
  void _checkLibraryQuery(_LibraryQuery query, double now) {
    final message = _libraryDisposed || query.session != engine.session
        ? 'Connection changed while awaiting library status.'
        : now >= query.deadline
            ? 'Library status timed out.'
            : null;
    if (message == null) return;
    _libraryQueries.remove(query);
    if (!query.done.isCompleted) query.done.completeError(StateError(message));
  }

  @override
  void _pollLibrary(double now) {
    for (final request in _libraryRecalls) {
      _checkLibraryRecall(request, now);
    }
    for (final request in _libraryRequests.values) {
      _checkLibraryRequest(request, now);
    }
    for (final query in _libraryQueries.toList()) {
      _checkLibraryQuery(query, now);
    }
    for (final request in _forgetSavedMixersRequests.values.toList()) {
      _checkForgetSavedMixers(request, now);
    }
  }

  /// Clears only engine-owned remembered addresses. Submission is not proof:
  /// settings may clear Flutter recents only after the exact local ack arrives.
  Future<void> forgetSavedMixers() {
    if (_libraryDisposed) {
      return Future.error(StateError('The engine was closed.'));
    }
    final id = 'forget-saved-mixers-$_libraryClientNonce-${++_libraryId}';
    final request =
        _ForgetSavedMixersRequest(id, engine.session, _nowMs() + 15000);
    _forgetSavedMixersRequests[id] = request;
    unawaited(
        Future<void>.sync(() => _command(_clientId, Cmds.forgetSavedMixers(id)))
            .then((_) {
      _checkForgetSavedMixers(request, _nowMs());
    }, onError: (Object error) {
      _checkForgetSavedMixers(request, _nowMs());
      _failForgetSavedMixers(request,
          'Remembered-address clearing transport failed: $error. Addresses may have been cleared; the result is unconfirmed.');
    }));
    return request.done.future;
  }

  void _failForgetSavedMixers(
      _ForgetSavedMixersRequest request, String message) {
    _forgetSavedMixersRequests.remove(request.id);
    if (!request.done.isCompleted) {
      request.done.completeError(StateError(message));
    }
  }

  @override
  void _interruptForgetSavedMixers(String message) {
    for (final request in _forgetSavedMixersRequests.values.toList()) {
      _failForgetSavedMixers(request,
          '$message Remembered addresses may have been cleared; the result is unconfirmed.');
    }
  }

  void _checkForgetSavedMixers(_ForgetSavedMixersRequest request, double now) {
    if (_libraryDisposed || request.session != engine.session) {
      _failForgetSavedMixers(request,
          'Connection changed while clearing remembered addresses. Addresses may have been cleared; the result is unconfirmed.');
    } else if (now >= request.deadline) {
      _failForgetSavedMixers(request,
          'Remembered-address clearing timed out. Addresses may have been cleared; the result is unconfirmed.');
    }
  }

  @override
  void _receiveForgetSavedMixersResult(Map<String, dynamic> frame) {
    final id = frame['request_id'];
    final request = id is String ? _forgetSavedMixersRequests[id] : null;
    if (request == null) return;
    _checkForgetSavedMixers(request, _nowMs());
    if (request.done.isCompleted) return;
    final results = frame['results'];
    final error = frame['error'];
    if (frame['operation'] != 'forget_saved_mixers' ||
        results is! List ||
        results.isNotEmpty ||
        !frame.containsKey('error') ||
        (error != null && error is! String)) {
      _failForgetSavedMixers(request,
          'The engine returned an invalid remembered-address clearing result. Addresses may have been cleared; the result is unconfirmed.');
      return;
    }
    _checkForgetSavedMixers(request, _nowMs());
    if (request.done.isCompleted) return;
    if (error != null) {
      _failForgetSavedMixers(request, error as String);
      return;
    }
    _forgetSavedMixersRequests.remove(id);
    request.done.complete();
  }

  /// Blocks new writes and invalidates dialogs for the entire settings reset.
  /// Engine reset is first; no Flutter-owned data is removed on its failure.
  Future<void> resetLocalLibraryThen(
      Future<void> Function() clearAppData) async {
    if (_libraryResetting || _libraryDisposed) {
      throw StateError('A reset is unavailable or already running.');
    }
    _libraryResetting = true;
    final generation = ++_libraryGeneration;
    notifyListeners();
    var libraryCleared = false;
    try {
      final pending = _libraryRequests.values.toList();
      for (final request in pending) {
        final completion = await request.done.future;
        if (completion.result == null ||
            _libraryRequests.containsKey(request.id)) {
          throw StateError(
              'An earlier library write is still unconfirmed. Reset stopped before deleting app data.');
        }
      }
      for (final request in _libraryRecalls.toList()) {
        final error = await request.done.future;
        if (error != null || _libraryRecalls.contains(request)) {
          throw StateError(
              'An earlier recall is still unconfirmed. Reset stopped.');
        }
      }
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError('Engine attachment changed during reset.');
      }
      final status = await queryLocalLibrary();
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError('Engine attachment changed during reset.');
      }
      final completion = await _submitLibraryMutation(
          const LocalLibraryAction.reset(),
          expectedRevision: status.revision,
          resetting: true);
      if (!completion.committed) throw StateError(completion.message);
      libraryCleared = true;
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError(
            'Engine attachment changed after clearing the library.');
      }
      await clearAppData();
    } catch (error) {
      throw StateError(libraryCleared
          ? 'Partial reset: local scenes and shows were cleared, but remaining app data may be only partly removed: $error'
          : 'Reset stopped before clearing other app data: $error');
    } finally {
      _libraryResetting = false;
      _libraryGeneration++;
      if (!_libraryDisposed) notifyListeners();
    }
  }
}

class _LibraryRequest {
  _LibraryRequest(this.id, this.action, this.revision, this.mode, this.session,
      this.owner, this.deadline);
  final String id;
  final LocalLibraryAction action;
  final int revision;
  final LocalLibraryMode mode;
  final int session;
  final Object? owner;
  final double deadline;
  final done = Completer<LocalLibraryCompletion>();
}

class _LibraryRecall {
  _LibraryRecall(this.owner, this.session, this.deadline);
  final Object owner;
  final int session;
  final double deadline;
  final done = Completer<String?>();
}

class _LibraryQuery {
  _LibraryQuery(this.session, this.deadline);
  final int session;
  final double deadline;
  final done = Completer<LocalLibraryStatus>();
}

class _ForgetSavedMixersRequest {
  _ForgetSavedMixersRequest(this.id, this.session, this.deadline);
  final String id;
  final int session;
  final double deadline;
  final done = Completer<void>();
}
