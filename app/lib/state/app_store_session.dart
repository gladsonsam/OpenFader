part of 'app_store.dart';

// Connection lifetime, command transport and visible errors.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreSessionContract {
  bool get _online;
  set _online(bool value);
  bool get _demo;
  set _demo(bool value);
  String get _deskName;
  set _deskName(String value);
  String get _transportError;
  set _transportError(String value);
  set _transportErrorAt(DateTime? value);
  set _engineErrorAt(DateTime? value);
  int get _errorRevision;
  set _errorRevision(int value);
  void _noteTransportError(Object e);
  void _trackEngineError();
  void _clearSessionValues();
  void _send(String json);
}

mixin _AppStoreSession on _AppStoreContext {
  // --- connection (phase/message/host, reduced) ---
  @override
  bool _online = false;
  @override
  bool _demo = true;
  @override
  String _deskName = 'sim';
  String _message = 'Not connected';

  // --- visible transport errors (never silently swallowed) ---
  // Engine `error` frames already land in engine.error; poll/cmd transport
  // failures (socket/FFI hiccups) previously vanished in catch (_). They
  // now surface here so the Link tab can render them with a timestamp.
  @override
  String _transportError = '';
  @override
  DateTime? _transportErrorAt;
  String _lastEngineError = '';
  @override
  DateTime? _engineErrorAt;
  @override
  int _errorRevision = 0;

  // ------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------

  // Attach to a booted client id, then resync.

  Future<void> attach(String clientId) async {
    _invalidatePresetOperations('Connection changed. Reopen presets.');
    _interruptAllLibrary('Engine attachment changed.');
    _interruptForgetSavedMixers('Engine attachment changed.');
    _libraryRequests.clear();
    _libraryRecalls.clear();
    _libraryGeneration++;
    engine.resetLocalLibraryStatus();
    _clientId = clientId;
    _binaryMeters = true; // re-arm: a fresh attach may carry a new library
    try {
      await _api.resync(clientId);
    } catch (e) {
      // transports may not implement resync; anything else is visible.
      _noteTransportError(e);
    }
    refreshLocalLibrary();
    _invalidateDerived();
  }

  void setOnline(bool v) {
    if (!v) _stopPresetRecalls('Connection changed or lost. Reopen presets.');
    if (_online == v) return;
    _online = v;
    _message = v ? 'online' : 'Not connected';
    _invalidateDerived(); // groups renders an offline placeholder
    notifyListeners();
  }

  bool get onlineFlag => _online;
  String get statusMessage => _message;

  // Most recent poll or cmd transport failure, or '' when none.

  String get transportError => _transportError;
  DateTime? get transportErrorAt => _transportErrorAt;

  // Each received failure is distinct even when its message repeats.

  int get errorRevision => _errorRevision;

  // Transport failure wins because it explains silent probes, else the engine error frame.

  String get visibleError =>
      _transportError.isNotEmpty ? _transportError : engine.error;

  // Timestamp of the visible error, or null when healthy.

  DateTime? get visibleErrorAt =>
      _transportError.isNotEmpty ? _transportErrorAt : _engineErrorAt;

  @override
  void _noteTransportError(Object e) {
    _interruptDestinationEdits(
        'Connection error; the destination setting may have changed.');
    _interruptForgetSavedMixers(
        'A transport error interrupted address clearing: $e.');
    _interruptAllLibrary(
        'A transport error interrupted the library operation: $e.');
    _stopPresetRecalls('A connection error interrupted recall: $e. '
        'The last setting may have changed; its result is uncertain.');
    final s = '$e';
    _errorRevision++;
    _transportError = s;
    _transportErrorAt = DateTime.now();
    notifyListeners();
  }

  // Clear both error displays; the engine re-sets its own on the next error frame.

  void clearErrors() {
    _transportError = '';
    _transportErrorAt = null;
    engine.error = '';
    notifyListeners();
  }

  @override
  void _trackEngineError() {
    if (engine.error != _lastEngineError) {
      _lastEngineError = engine.error;
      _engineErrorAt = engine.error.isEmpty ? null : DateTime.now();
    }
  }

  @override
  void _send(String json) {
    unawaited(_api.cmd(_clientId, json).then((_) {}, onError: (e) {
      _noteTransportError(e);
    }));
  }

  // Raw Cmd JSON write for commands with no dedicated store method.

  void sendRaw(String json) {
    _prepareConnectionCommand(json);
    _send(json);
  }

  @override
  void _clearSessionValues() {
    _interruptDestinationEdits(
        'Connection changed; the destination setting may have changed.');
    _interruptAllLibrary(
        'Connection changed while waiting for the library result.');
    _interruptForgetSavedMixers('Connection changed during address clearing.');
    _presetVerifiedFields.clear();
    _invalidatePresetOperations('Connection changed or lost. Reopen presets.');
    _failNameEdits('Connection changed or lost. Reopen the name editor.');
    _coalescer.clear();
    _maps.confirmed.clear();
    _maps.pending.clear();
    _confirmedFader.clear();
    _confirmedMute.clear();
    _bank = 'ch1';
    _busId = 'main';
    _focusCh = null;
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    _invalidateDerived();
  }

  void _prepareConnectionCommand(String json) {
    final command = jsonDecode(json);
    if (command is! Map ||
        (command['cmd'] != 'connect' && command['cmd'] != 'disconnect')) {
      return;
    }
    engine.resetSession();
    _clearSessionValues();
    _syncControlNotes();
    notifyListeners();
  }

  Future<void> sendCommand(String json) async {
    _prepareConnectionCommand(json);
    await _api.cmd(_clientId, json);
    await pollMeters();
  }
}
