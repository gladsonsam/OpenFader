// Staged boot runs after the launch screen paints, one named step at a time.
// The native engine load runs after "Preparing interface" has painted because it is the slow step.
library;

import 'dart:io';

import 'package:flutter/foundation.dart';

import 'desktop_host.dart';

import '../engine.dart';
import '../ffi_engine.dart';
import '../state/app_store.dart';
import 'fake_sim_engine.dart';

class BootController extends ChangeNotifier {
  BootController();

  // Step labels in run order; the launch screen renders them as a checklist.
  static const steps = <String>[
    'Preparing interface',
    'Loading native engine',
    'Starting mixer engine',
    'Syncing mixer state',
  ];

  int _step = 0;
  bool _running = false;
  bool _done = false;
  bool _disposed = false;
  String? _error;

  EngineApi? _api;
  AppStore? _store;
  bool _isRealEngine = false;

  // Index of the running step, or steps.length once done.
  int get step => _done ? steps.length : _step;

  String get status {
    if (_error != null) return 'Boot failed';
    if (_done) return 'Ready';
    return '${steps[_step.clamp(0, steps.length - 1)]}…';
  }

  double get progress => _done ? 1.0 : _step / steps.length;

  bool get running => _running;
  bool get done => _done;

  String? get error => _error;

  EngineApi? get api => _api;
  AppStore? get store => _store;

  // False means the FAKE sim fallback; the shell banner keeps saying so loudly.
  bool get isRealEngine => _isRealEngine;

  String? get engineLabel {
    if (_api == null) return null;
    return _isRealEngine ? 'embedded engine' : 'sim fallback';
  }

  /// Run all stages in order. Safe to call again after a failure (Retry):
  /// progress resets, retaining the loaded API and its successful boot client.
  Future<void> start() async {
    if (_running || _done || _disposed) return;
    _running = true;
    _done = false;
    _error = null;
    _step = 0;
    notifyListeners();
    AppStore? pendingStore;
    try {
      // Let the launch screen paint before the slow work starts.
      await Future<void>.delayed(Duration.zero);
      if (_disposed) return;

      _step = 1;
      notifyListeners();
      if (_api == null) {
        if (Platform.isMacOS || Platform.isWindows || Platform.isLinux) {
          _api = await DesktopHost.start();
          _isRealEngine = true;
        } else {
          final ffi = await FfiEngine.create();
          if (_disposed) return;
          _isRealEngine = ffi != null;
          _api = ffi ?? FakeSimEngine();
        }
      }
      final api = _api!;

      _step = 2;
      notifyListeners();
      final id = await api.boot();
      if (_disposed) return;

      _step = 3;
      notifyListeners();
      final store = AppStore(api, id);
      pendingStore = store;
      await store.attach(id);
      if (_disposed) return;
      // AppStore records transport failures instead of throwing so the shell's
      // live poll loop can recover. During boot they must keep Retry visible.
      if (store.transportError.isNotEmpty) {
        throw StateError('Could not sync mixer state: ${store.transportError}');
      }
      store.setOnline(true);
      // One upfront drain paints strips on arrival instead of flashing one empty frame first.
      await store.pollMeters();
      if (_disposed) return;
      if (store.transportError.isNotEmpty) {
        throw StateError('Could not sync mixer state: ${store.transportError}');
      }
      _store = store;
      pendingStore = null; // The shell owns the successfully synced store.

      _done = true;
    } catch (e) {
      if (!_disposed) _error = '$e';
    } finally {
      pendingStore?.dispose();
      _running = false;
      // One terminal notify covers both outcomes after _running clears so Retry rebuilds.
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    // An in-flight start disposes its pending store once the awaited work ends.
    // A successful store has already been handed to AppShell by BootGate.
    super.dispose();
  }
}
