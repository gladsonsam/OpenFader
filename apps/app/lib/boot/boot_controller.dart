// Staged boot runs after the launch screen paints, one named step at a time.
// The native engine load runs after "Preparing interface" has painted because it is the slow step.
library;

import 'package:flutter/foundation.dart';

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

  // Safe to call again after a failure; state resets and stages re-run from the top.
  Future<void> start() async {
    if (_running) return;
    _running = true;
    _done = false;
    _error = null;
    _step = 0;
    notifyListeners();
    try {
      // Let the launch screen paint before the slow work starts.
      await Future<void>.delayed(Duration.zero);

      _step = 1;
      notifyListeners();
      final ffi = await FfiEngine.create();
      _isRealEngine = ffi != null;
      _api = ffi ?? FakeSimEngine();

      _step = 2;
      notifyListeners();
      final id = await _api!.boot();

      _step = 3;
      notifyListeners();
      final store = AppStore(_api!, id);
      await store.attach(id);
      store.setOnline(true);
      // One upfront drain paints strips on arrival instead of flashing one empty frame first.
      await store.pollMeters();
      _store = store;

      _done = true;
    } catch (e) {
      _error = '$e';
    } finally {
      _running = false;
      // One terminal notify covers both outcomes because Retry must rebuild after _running clears.
      notifyListeners();
    }
  }
}
