// Connection intent only. Driven by the shell poll, never a meter/page timer.
import 'dart:async';

import 'package:flutter/foundation.dart';

typedef RecoveryTarget = ({String driver, String host});

/// A bounded recovery episode for an established hardware session. No mixer
/// commands or edits are retained. A successful session arms the next episode;
/// explicit disconnect/offline/target selection cancels the current one.
class ConnectionRecovery extends ChangeNotifier {
  ConnectionRecovery(this._connect);

  final Future<bool> Function(RecoveryTarget target) _connect;
  final Stopwatch _clock = Stopwatch()..start();
  static const _delays = [2, 5, 10];
  RecoveryTarget? _target;
  Duration? _due;
  Duration? _deadline;
  int _run = 0;
  int _attempt = 0;
  bool _running = false;
  bool _exhausted = false;
  bool _disposed = false;

  bool get active => _due != null || _running;
  String get status => _exhausted
      ? 'Automatic reconnect stopped after $_attempt attempts. Use Retry or connect again.'
      : active
          ? 'Reconnecting to ${_target?.host} · attempt ${_attempt + (_running ? 0 : 1)} of ${_delays.length}…'
          : '';

  void cancel() {
    _run++;
    _target = null;
    _due = null;
    _deadline = null;
    _running = false;
    _exhausted = false;
    _attempt = 0;
    if (!_disposed) notifyListeners();
  }

  void tick({
    required bool enabled,
    required bool busy,
    required bool connected,
    required String state,
    required String driver,
    required String host,
  }) {
    if (_disposed) return;
    if (!enabled ||
        driver == 'sim' ||
        state == 'offline' ||
        state == 'disconnected') {
      if (_target != null || active || _exhausted) cancel();
      return;
    }
    if (connected && !busy && !_running && host.isNotEmpty) {
      final target = (driver: driver, host: host);
      if (_target != target || _attempt != 0 || _exhausted || _due != null) {
        cancel();
        _target = target;
      }
      return;
    }
    if (busy || _target == null || _exhausted) return;
    final now = _clock.elapsed;
    if (_running) {
      if (_deadline != null && now >= _deadline!) {
        // Completion is uncertain: never overlap another handshake. A late
        // successful session may still rearm recovery through a later tick.
        _run++;
        _running = false;
        _deadline = null;
        _exhausted = true;
        notifyListeners();
      }
      return;
    }
    if (state != 'reconnecting' && _attempt == 0) return;
    if (_due == null) {
      _due = now + Duration(seconds: _delays[_attempt]);
      notifyListeners();
    } else if (now >= _due!) {
      _due = null;
      _running = true;
      _attempt++;
      _deadline = now + const Duration(seconds: 90);
      final run = ++_run;
      final target = _target!;
      notifyListeners();
      unawaited(_attemptConnection(run, target));
    }
  }

  Future<void> _attemptConnection(int run, RecoveryTarget target) async {
    var success = false;
    try {
      success = await _connect(target);
    } catch (_) {
      // The connection store retains the concrete engine/network error.
    }
    if (_disposed || run != _run) return;
    _running = false;
    _deadline = null;
    if (success) {
      _attempt = 0;
    } else if (_attempt >= _delays.length) {
      _exhausted = true;
    } else {
      _due = _clock.elapsed + Duration(seconds: _delays[_attempt]);
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    cancel();
    super.dispose();
  }
}
