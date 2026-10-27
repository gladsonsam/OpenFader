import '../core/native_library.dart';
import 'store_adapter.dart';

mixin ShellNativeLibrary on ShellStoreAdapter {
  NativeLibraryVm? _nativeLibrary;
  String? _nativeStatus;
  bool _nativeBusy = false;
  int _nativeRun = 0;
  Object? _nativeSession;

  NativeLibraryVm? get nativeLibrary =>
      _nativeSession == engine.session ? _nativeLibrary : null;
  String? get nativeStatus => _nativeStatus;
  bool get nativeBusy => _nativeBusy;
  bool get nativeWritable => engine.connected && app.blockedAccess.isEmpty;
  Object get nativeContext =>
      (engine.session, nativeLibrary, app.blockedAccess);

  Future<void> nativeAction(
    Map<String, dynamic> action,
    Object captured,
  ) async {
    if (isDisposed || _nativeBusy || !engine.connected) return;
    if (app.libraryActive || app.libraryResetting) {
      _nativeStatus = 'Wait for the local library operation to finish.';
      notifyListeners();
      return;
    }
    if (nativeContext != captured) {
      _nativeStatus =
          'Provider session, inventory or access changed. Inspect again.';
      notifyListeners();
      return;
    }
    final run = ++_nativeRun;
    final session = engine.session;
    _nativeBusy = true;
    _nativeStatus = 'Waiting for a correlated provider result…';
    notifyListeners();
    final result = await app.requestNativeLibrary(this, action, nativeLibrary);
    if (isDisposed || run != _nativeRun) return;
    _nativeBusy = false;
    _nativeStatus = '${result['status']}: ${result['message']}';
    // Discard stale receipts. An uncertain operation invalidates the dialog
    // guard instead of encouraging a blind repeat or optimistic slot update.
    _nativeLibrary =
        session == engine.session && result['status'] == 'confirmed'
        ? NativeLibraryVm.parse(result['inventory'])
        : null;
    _nativeSession = session;
    notifyListeners();
  }

  void cancelNativeAction() {
    _nativeRun++;
    app.cancelNativeLibrary(this);
    _nativeBusy = false;
    _nativeLibrary = null;
    _nativeStatus =
        'Cancelled waiting. The mixer may already have changed. Inspect again.';
    if (!isDisposed) notifyListeners();
  }

  @override
  void dispose() {
    _nativeRun++;
    app.cancelNativeLibrary(this);
    super.dispose();
  }
}
