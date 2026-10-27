import 'dart:async';

import 'destination_controls.dart';

/// Local preference, never a mixer mode parameter.
enum TalkbackModePreference { momentary, latch, auto }

/// Store-owned gesture state. No timers, transport queue, or unrelated restores.
/// https://mackie.com/img/file_resources/Master%20Fader_V5_1_RG.pdf p188:
/// Latch toggles; Momentary follows hold;
/// Auto uses a short tap as latch and a hold as momentary. The guide gives no
/// threshold. OpenFader deliberately uses 500ms, not claimed as native timing.
class TalkbackGestureController {
  static const autoHoldMilliseconds = 500;
  final int Function() session;
  final bool Function() sessionActive;
  final DestinationControlVm? Function() current;
  final bool Function(DestinationControlVm) isCurrent;
  final Future<DestinationEditResult> Function(DestinationControlVm, double)
      write;
  final int Function() nowMilliseconds;
  final void Function() changed;

  TalkbackGestureController(
      {required this.session,
      required this.sessionActive,
      required this.current,
      required this.isCurrent,
      required this.write,
      required this.nowMilliseconds,
      required this.changed});

  _TalkbackPress? _press;
  _TalkbackPress? _activation;
  DestinationControlVm? _owned;
  int? _ownedSession;
  bool _busy = false;
  bool _disposed = false;
  String? _error;
  bool get held => _press != null;
  bool get busy => _busy;
  String? get error => _error;
  bool get ownsActivation => _owned != null;
  bool get canPress {
    final row = current();
    return !_disposed &&
        sessionActive() &&
        !_busy &&
        !held &&
        row != null &&
        row.talkbackGesture &&
        row.revision != null &&
        row.generation != null &&
        row.authority == 'verified' &&
        (row.value == 0 || row.value == 1) &&
        row.allowsValue(0) &&
        row.allowsValue(1) &&
        isCurrent(row);
  }

  void _notify() {
    if (!_disposed) changed();
  }

  bool _matches(DestinationControlVm row) =>
      sessionActive() &&
      row.authority == 'verified' &&
      current() == row &&
      isCurrent(row);

  /// Called by the existing shell ticker and before each gesture. An observed
  /// peer transition (including ABA) forfeits ownership. Unreceived transitions
  /// and same-value peer writes cannot be attributed by this protocol.
  void poll() {
    final press = _press;
    final owned = _owned;
    final activation = _activation;
    if ((!sessionActive() ||
            (press != null && press.session != session()) ||
            (activation != null && activation.session != session()) ||
            (owned != null && _ownedSession != session())) &&
        (press != null ||
            owned != null ||
            (activation != null && !activation.abandoned))) {
      if (press != null) {
        press.cancelled = true;
        press.stop = true;
        press.abandoned = true;
      }
      if (activation != null) activation.abandoned = true;
      _press = null;
      _owned = null;
      _error =
          'Talkback connection changed. OFF cannot be guaranteed; no reconnect write will be replayed.';
      _notify();
    } else if (!_busy &&
        owned != null &&
        (_ownedSession != session() || !_matches(owned))) {
      _owned = null;
      _press?.cancelled = true;
      _press = null;
      _error =
          'Talkback changed or became unavailable. Ownership released; the current mixer state was preserved.';
      _notify();
    }
  }

  void press(TalkbackModePreference mode) {
    poll();
    if (!canPress) return;
    final row = current()!;
    _error = null;
    final press = _TalkbackPress(session(), nowMilliseconds(), mode, row);
    _press = press;
    // Latch commits on a completed tap, so a cancelled tap never toggles.
    // Active peer talkback is not acquired just by touching the button.
    if (mode != TalkbackModePreference.latch && row.value == 0) {
      unawaited(_activate(press));
    }
    _notify();
  }

  void release() {
    final press = _press;
    if (press == null) return;
    _press = null;
    if (press.session != session() || !sessionActive()) {
      press.abandoned = true;
      press.cancelled = true;
      press.stop = true;
      _owned = null;
      _error =
          'Talkback disconnected during the gesture; result uncertain. No reconnect replay.';
      _notify();
      return;
    }
    final shortTap = nowMilliseconds() - press.started < autoHoldMilliseconds;
    if (press.mode == TalkbackModePreference.latch ||
        (press.mode == TalkbackModePreference.auto &&
            shortTap &&
            press.initial.value == 1)) {
      // Explicit tap-to-OFF may stop the captured active value, including a
      // peer activation. Implicit release/cancel only stops our own receipt.
      if (!_busy && _matches(press.initial)) {
        if (press.initial.value == 0) {
          unawaited(_activate(press));
        } else {
          unawaited(_disable(press.initial, press.session));
        }
      } else {
        _error = 'Talkback changed before the tap completed; no toggle sent.';
      }
    } else {
      press.stop = press.mode == TalkbackModePreference.momentary || !shortTap;
      if (press.stop && !_busy) unawaited(_stopOwned(press.session));
      // If ON is pending, _activate consumes this captured stop intent after
      // the exact independently verified receipt, even after pointer release.
    }
    _notify();
  }

  void tap(TalkbackModePreference mode) {
    if (!canPress) return;
    press(mode);
    release();
  }

  /// Pointer cancel, route disposal, focus/app interruption, or mode change.
  /// A cancelled Auto short tap must not become a latched activation.
  void cancel() {
    final press = _press;
    if (press == null) return;
    _press = null;
    press.cancelled = true;
    press.stop = true;
    if (!_busy && press.mode != TalkbackModePreference.latch) {
      unawaited(_stopOwned(press.session));
    }
    _notify();
  }

  Future<DestinationEditResult> _send(
      DestinationControlVm row, double value) async {
    try {
      return await write(row, value);
    } catch (failure) {
      return DestinationEditResult(
          'Talkback transport failure: $failure. Result uncertain.');
    }
  }

  Future<void> _activate(_TalkbackPress press) async {
    if (_busy || press.session != session() || !_matches(press.initial)) return;
    _busy = true;
    _activation = press;
    _notify();
    final result = await _send(press.initial, 1);
    if (identical(_activation, press)) _activation = null;
    _busy = false;
    final receipt = result.applied;
    if (press.abandoned || press.session != session() || !sessionActive()) {
      if (identical(_press, press)) _press = null;
      _owned = null;
      _error =
          'Talkback connection changed during ON. OFF cannot be guaranteed; no reconnect replay.';
    } else if (result.error != null ||
        receipt == null ||
        receipt.target != press.initial.target ||
        receipt.id != press.initial.id ||
        receipt.epoch != press.initial.epoch ||
        receipt.generation != press.initial.generation ||
        receipt.revision == null ||
        receipt.revision == press.initial.revision ||
        receipt.value != 1 ||
        !_matches(receipt)) {
      if (identical(_press, press)) _press = null;
      _owned = null;
      _error = result.error ??
          'Talkback ON ownership could not be independently confirmed. Current mixer state preserved; result uncertain.';
    } else {
      _owned = receipt;
      _ownedSession = press.session;
      if (press.stop || press.cancelled) await _stopOwned(press.session);
    }
    _notify();
  }

  Future<void> _stopOwned(int capturedSession) async {
    final owned = _owned;
    if (owned == null) return; // Never stop somebody else's held activation.
    if (capturedSession != session() ||
        _ownedSession != capturedSession ||
        !_matches(owned)) {
      _owned = null;
      _error =
          'Talkback ownership or connection changed. No automatic OFF sent; result uncertain.';
      _notify();
      return;
    }
    await _disable(owned, capturedSession);
  }

  Future<void> _disable(
      DestinationControlVm captured, int capturedSession) async {
    if (_busy) return;
    if (capturedSession != session() || !_matches(captured)) {
      _owned = null;
      _error = 'Talkback changed before OFF; no write sent.';
      _notify();
      return;
    }
    _busy = true;
    _owned = null; // A failed/uncertain OFF cannot retain enable ownership.
    _notify();
    final result = await _send(captured, 0);
    _busy = false;
    final receipt = result.applied;
    if (capturedSession != session() ||
        result.error != null ||
        receipt == null ||
        receipt.target != captured.target ||
        receipt.id != captured.id ||
        receipt.epoch != captured.epoch ||
        receipt.generation != captured.generation ||
        receipt.revision == null ||
        receipt.revision == captured.revision ||
        receipt.value != 0 ||
        !_matches(receipt)) {
      _error = result.error ??
          'Talkback OFF was not independently confirmed; result uncertain. No reconnect replay.';
    }
    _notify();
  }

  void dispose() {
    cancel();
    _disposed = true;
    // A pending ON may still finish: its captured cancel intent can only
    // clean up its exact receipt on the same session. It can never re-enable.
  }
}

class _TalkbackPress {
  final int session;
  final int started;
  final TalkbackModePreference mode;
  final DestinationControlVm initial;
  bool abandoned = false;
  bool cancelled = false;
  bool stop = false;
  _TalkbackPress(this.session, this.started, this.mode, this.initial);
}
