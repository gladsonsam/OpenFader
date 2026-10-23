// Staged app boot: the work `main()` used to do before the first frame
// (FFI init, engine boot, store attach) now runs *after* the launch screen
// paints, one named step at a time. The launch screen observes this
// controller and renders each step + the live status line, so startup
// never sits on a black/white flash with no feedback.
//
// Ordering matters for perceived performance: the native engine load is
// the slow step (up to the 5 s FRB timeout on failure), so it runs after
// "Preparing interface" has already painted.
library;

import 'package:flutter/foundation.dart';

import '../engine.dart';
import '../ffi_engine.dart';
import '../state/app_store.dart';
import 'fake_sim_engine.dart';

class BootController extends ChangeNotifier {
  BootController();

  /// Human-readable step labels, in run order. The launch screen renders
  /// these as a checklist with the live step highlighted.
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

  /// Index of the step currently running (or the failed step when [error]
  /// is set). Clamped to [steps] while running; equals `steps.length`
  /// once [done].
  int get step => _done ? steps.length : _step;

  /// `Loading native engine…` style status line for the launch screen.
  String get status {
    if (_error != null) return 'Boot failed';
    if (_done) return 'Ready';
    return '${steps[_step.clamp(0, steps.length - 1)]}…';
  }

  /// 0..1 progress for the launch-screen bar.
  double get progress => _done ? 1.0 : _step / steps.length;

  bool get running => _running;
  bool get done => _done;

  /// Non-null when a stage threw; the launch screen shows it with Retry.
  String? get error => _error;

  EngineApi? get api => _api;
  AppStore? get store => _store;

  /// Boot-time backend verdict (set once the engine-load step finishes).
  /// False = FAKE sim fallback; the shell banner keeps saying so loudly.
  bool get isRealEngine => _isRealEngine;

  /// Null until the engine-load step finishes; lets the launch screen
  /// show which backend won before the shell takes over.
  String? get engineLabel {
    if (_api == null) return null;
    return _isRealEngine ? 'embedded engine' : 'sim fallback';
  }

  /// Run all stages in order. Safe to call again after a failure (Retry):
  /// state resets and the stages re-run from the top.
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
      // One upfront drain so the mixer paints with strips on arrival
      // instead of one empty frame + a pop-in 80 ms later.
      await store.pollMeters();
      _store = store;

      _done = true;
    } catch (e) {
      _error = '$e';
    } finally {
      _running = false;
      // Single terminal notify for both outcomes: the error path must
      // rebuild observers *after* _running clears, or Retry stays
      // disabled; the success path wakes BootGate navigation here.
      notifyListeners();
    }
  }
}
