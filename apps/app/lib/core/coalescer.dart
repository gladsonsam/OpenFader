// W2a write coalescer: SPEC 2a leading-edge send + latest-wins coalescing.
//
// A fader drag fires dozens of writes per second; the engine accepts <= 60/s
// per control. First write of a burst goes out immediately (local-first
// latency); writes inside the 1/60 s window collapse to the latest value,
// flushed by one timer per key. Timer + clock are injected for tests.
import 'dart:async';

/// SPEC 2a minimum interval between sends for one control.
const double minIntervalMs = 1000 / 60;

/// Injectable clock + timers (mirrors the TS `CoalescerClock`).
abstract class CoalescerClock {
  double nowMs();
  Object? setTimer(void Function() fn, int ms);
  void clearTimer(Object? t);
}

/// Default clock: Stopwatch + dart:async Timer.
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

/// Result of [WriteCoalescer.offer].
enum CoalesceResult { sent, coalesced }

class WriteCoalescer {
  WriteCoalescer(this._clock);

  final CoalescerClock _clock;
  final Map<String, double> _lastSentAt = {};
  final Map<String, void Function()> _queued = {};
  final Map<String, Object?> _timers = {};

  /// Offer one write. [send] runs immediately on the leading edge, else the
  /// write is held and only the latest per key is sent when the window
  /// lapses. Returns [CoalesceResult.sent] or [CoalesceResult.coalesced].
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

  /// Send the held write for [key] now, if any.
  void flush(String key) {
    final send = _queued.remove(key);
    _cancelLocked(key);
    send?.call();
  }

  /// Drop the held write (and its send timestamp) for [key].
  void cancel(String key) => _cancelLocked(key);

  /// Drop all held writes (e.g. on disconnect).
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
