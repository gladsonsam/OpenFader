// Write coalescer: leading-edge send plus latest-wins coalescing.
//
// A fader drag fires dozens of writes per second; the engine accepts limited
// per-control throughput. The first write of a burst goes out at once and
// writes inside the window collapse to the latest value, flushed by one
// timer per key.
import 'dart:async';

/// Minimum interval between sends for one control (SPEC 2a).
const double minIntervalMs = 1000 / 60;

/// Injectable clock and timers (mirrors the TS CoalescerClock).
abstract class CoalescerClock {
  double nowMs();
  Object? setTimer(void Function() fn, int ms);
  void clearTimer(Object? t);
}

class TimerCoalescerClock implements CoalescerClock {
  final Stopwatch _sw = Stopwatch()..start();

  @override
  double nowMs() => _sw.elapsedMicroseconds / 1000.0;

  @override
  Object? setTimer(void Function() fn, int ms) =>
      Timer(Duration(milliseconds: ms), fn);

  @override
  void clearTimer(Object? t) => (t as Timer?)?.cancel();
}

enum CoalesceResult { sent, coalesced }

class WriteCoalescer {
  WriteCoalescer(this._clock);

  final CoalescerClock _clock;
  final Map<String, double> _lastSentAt = {};
  final Map<String, void Function()> _queued = {};
  final Map<String, Object?> _timers = {};

  /// Offer one write. Runs [send] at once on the leading edge, else holds
  /// it and sends only the latest per key when the window lapses.
  CoalesceResult offer(String key, void Function() send) {
    final now = _clock.nowMs();
    final last = _lastSentAt[key];
    if (last == null || now - last >= minIntervalMs) {
      _cancelLocked(key);
      _lastSentAt[key] = now;
      send();
      return CoalesceResult.sent;
    }
    _queued[key] = send;
    if (!_timers.containsKey(key)) {
      final wait = (minIntervalMs - (now - last)).clamp(0, minIntervalMs);
      _timers[key] = _clock.setTimer(() {
        _timers.remove(key);
        final fn = _queued.remove(key);
        if (fn != null) {
          _lastSentAt[key] = _clock.nowMs();
          fn();
        }
      }, wait.round());
    }
    return CoalesceResult.coalesced;
  }

  void flush(String key) {
    final send = _queued.remove(key);
    _cancelLocked(key);
    send?.call();
  }

  void cancel(String key) => _cancelLocked(key);

  void clear() {
    for (final t in _timers.values) {
      _clock.clearTimer(t);
    }
    _timers.clear();
    _queued.clear();
    _lastSentAt.clear();
  }

  void _cancelLocked(String key) {
    final t = _timers.remove(key);
    if (t != null) _clock.clearTimer(t);
    _queued.remove(key);
    _lastSentAt.remove(key);
  }
}
