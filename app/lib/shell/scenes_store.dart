import 'dart:async';
// Shell adapter: scenes responsibility. The shell owns polling.
import '../engine.dart';
import '../screens/scenes.dart';
import 'store_adapter.dart';
import 'native_library_store.dart';

class ShellScenesStore extends ShellStoreAdapter with ShellNativeLibrary implements ScenesStore {
  ShellScenesStore(super.app) {
    app.addListener(_settle);
  }

  String? _show;
  bool _newShow = false;
  bool _busy = false;
  String? _status;
  int _runId = 0;
  Object? _pendingContext;

  Object get _context => (engine.session, _show, _newShow);
  bool get _canRun => !isDisposed && !_busy && !nativeBusy && online && app.libraryWritable;

  @override
  Object get actionContext => (
        _context,
        app.libraryGeneration,
        show,
        engine.localLibrary?.revision,
        engine.scenes,
        engine.shows
      );

  @override
  bool validateActionContext(Object captured) {
    if (isDisposed) return false;
    if (!_canRun || captured != actionContext) {
      _status = 'Scene action cancelled: the connection, selection, or '
          'library changed. Review the target and try again.';
      notifyListeners();
      return false;
    }
    return true;
  }

  @override
  bool get busy => _busy || nativeBusy || app.libraryResetting || app.libraryActive;

  @override
  String get libraryDescription {
    final library = engine.localLibrary;
    if (library == null) {
      return 'Local library durability is not yet proven. '
          'It is not a native mixer backup.';
    }
    final storage = library.mode == LocalLibraryMode.device
        ? library.status == LocalLibraryHealth.ready
            ? 'Local scenes and shows are stored on this device.'
            : 'Local library uses device storage; current contents are not confirmed.'
        : 'Local library is in memory only and lasts until the engine restarts.';
    final health = switch (library.status) {
      LocalLibraryHealth.ready => '',
      LocalLibraryHealth.error =>
        ' Library unavailable: ${library.error ?? 'load failed'}. Writes are blocked until an explicit reset.',
      LocalLibraryHealth.uncertain =>
        ' Library state is uncertain: ${library.error ?? 'commit could not be confirmed'}. Changes may have taken effect; writes are blocked until an explicit reset.',
    };
    return '$storage$health Numbered snapshots include writable strip '
        'catalogue parameters, basic FX rack settings and input/output patches. '
        'Provider-specific shared, global and FX controls outside this set '
        'are not captured. Named show captures include strip names, faders, '
        'mutes and main level. '
        'Local snapshot export/import is not available here. '
        'It is not a native mixer backup.';
  }

  @override
  String? get status {
    // Global errors remain visible as separate context, never evidence for
    // the outcome of a particular library request.
    final error = app.visibleError;
    if (error.isEmpty) return _status;
    return _status == null ? error : 'Engine/transport: $error\n$_status';
  }

  void _finish(String message) {
    _runId++;
    _busy = false;
    _pendingContext = null;
    _status = message;
    if (!isDisposed) notifyListeners();
  }

  void _settle() {
    if (isDisposed || !_busy) return;
    if (!engine.connected || _pendingContext != _context) {
      app.cancelLocalLibraryOperations(this,
          'Connection or selection changed while waiting for the library result.');
      _finish('Connection or selection changed. The scene operation result '
          'is unconfirmed; it may already have taken effect.');
    }
  }

  Future<void> _mutate(LocalLibraryAction action, String pending) async {
    if (!_canRun) return;
    final revision = engine.localLibrary!.revision;
    final run = ++_runId;
    _busy = true;
    _pendingContext = _context;
    _status = pending;
    notifyListeners();
    final result = await app.mutateLocalLibrary(action,
        expectedRevision: revision, owner: this);
    if (!isDisposed && run == _runId) _finish(result.message);
  }

  Future<void> _recall(String command, String pending, String requested) async {
    if (!_canRun) return;
    final run = ++_runId;
    _busy = true;
    _pendingContext = _context;
    _status = pending;
    notifyListeners();
    try {
      final error = await app.requestLocalLibraryRecall(command, this);
      if (!isDisposed && run == _runId) {
        _finish(error == null
            ? requested
            : '$error The mixer may have changed; its state is unverified.');
      }
    } catch (error) {
      if (!isDisposed && run == _runId) {
        _finish(
            'Recall result unconfirmed; the mixer may have changed: $error');
      }
    }
  }

  @override
  void dispose() {
    _runId++;
    app.cancelLocalLibraryOperations(this, 'The scenes screen was closed.');
    app.removeListener(_settle);
    super.dispose();
  }

  @override
  List<SceneEntryVm> get entries => [
        for (final s in engine.scenes)
          SceneEntryVm(number: s.number, name: s.name, current: s.current),
      ];

  @override
  NativeSnapshotsVm get nativeSnapshots => engine.nativeSnapshots;

  @override
  List<String> get shows => engine.shows.keys.toList();

  @override
  List<String> get showScenes => engine.shows[show] ?? const [];

  @override
  String? get show => _newShow
      ? null
      : engine.shows.containsKey(_show)
          ? _show
          : shows.firstOrNull;

  @override
  bool get online => engine.connected && app.libraryWritable;

  // The 001..999 range is the OpenFader engine library, not native mixer capacity.

  int? _freeSlot() {
    final used = {for (final s in engine.scenes) s.number};
    for (var n = 1; n <= 999; n++) {
      if (!used.contains(n)) return n;
    }
    return null;
  }

  @override
  void save(String name) {
    if (!_canRun || name.trim().isEmpty) return;
    final slot = _freeSlot();
    if (slot == null) {
      _status = 'OpenFader snapshot library full (001–999). Delete a snapshot '
          'before saving; nothing was overwritten.';
      notifyListeners();
      return;
    }
    name = name.trim();
    unawaited(_mutate(LocalLibraryAction.save(slot, name),
        'Saving snapshot $slot… Awaiting a correlated engine commit result.'));
  }

  @override
  void recall(int number) {
    if (!engine.scenes.any((s) => s.number == number)) return;
    unawaited(_recall(
        Cmds.sceneRecall(number),
        'Requesting snapshot $number recall…',
        'Snapshot $number recall requested. Completion and mixer value '
            'verification are unavailable here.'));
  }

  @override
  void rename(int number, String name) {
    name = name.trim();
    if (name.isEmpty || !engine.scenes.any((s) => s.number == number)) return;
    unawaited(_mutate(LocalLibraryAction.rename(number, name),
        'Renaming snapshot $number… Awaiting a correlated engine commit result.'));
  }

  @override
  void remove(int number) {
    if (!engine.scenes.any((s) => s.number == number)) return;
    unawaited(_mutate(LocalLibraryAction.delete(number),
        'Deleting snapshot $number… Awaiting a correlated engine commit result.'));
  }

  @override
  void clearAll() {
    if (engine.scenes.isEmpty) return;
    unawaited(_mutate(const LocalLibraryAction.clearNumbered(),
        'Clearing snapshots… Awaiting a correlated engine commit result.'));
  }

  @override
  void selectShow(String show) {
    if (isDisposed || _busy) return;
    _newShow = false;
    _show = show;
    send(Cmds.listShows());
    notifyListeners();
  }

  @override
  void beginShow() {
    if (isDisposed || _busy) return;
    _newShow = true;
    notifyListeners();
  }

  @override
  bool captureWillOverwrite(String name) =>
      engine.shows[show ?? name]?.contains(name) ?? false;

  @override
  void captureShow(String name) {
    name = name.trim();
    if (!_canRun || name.isEmpty) return;
    final selected = show ?? name;
    _newShow = false;
    _show = selected;
    unawaited(_mutate(LocalLibraryAction.capture(selected, name),
        'Capturing “$name” in “$selected”… Awaiting a correlated engine commit result.'));
  }

  @override
  void recallShow(String name) {
    final selected = show;
    if (selected == null ||
        !(engine.shows[selected]?.contains(name) ?? false)) {
      return;
    }
    unawaited(_recall(
        Cmds.recallScene(selected, name),
        'Requesting recall of “$selected” / “$name”…',
        'Recall requested for “$selected” / “$name”. '
            'Completion and mixer value verification are unavailable here.'));
  }

  @override
  void deleteShow(String name) {
    final selected = show;
    if (selected == null ||
        !(engine.shows[selected]?.contains(name) ?? false)) {
      return;
    }
    unawaited(_mutate(LocalLibraryAction.deleteNamed(selected, name),
        'Deleting “$selected” / “$name”… Awaiting a correlated engine commit result.'));
  }
}
