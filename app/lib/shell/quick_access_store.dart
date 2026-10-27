import 'dart:async';

import 'package:shared_preferences/shared_preferences.dart';

import '../core/access_limits.dart';
import '../engine.dart';
import '../core/quick_access.dart';
import '../core/talkback_gesture.dart';
import 'fx_store.dart';
import 'store_adapter.dart';

class ShellQuickAccessStore extends ShellStoreAdapter
    implements QuickAccessStore {
  ShellQuickAccessStore(super.app, this.fx) {
    _talkback = TalkbackGestureController(
      session: () => engine.session,
      sessionActive: () =>
          app.onlineFlag && engine.connected && engine.capsReady,
      current: () => talkbackGestureControl,
      isCurrent: app.destinationControlIsCurrent,
      write: app.setDestinationControlWithReceipt,
      nowMilliseconds: () => _gestureClock.elapsedMilliseconds,
      changed: () {
        if (!isDisposed) notifyListeners();
      },
    );
    unawaited(_loadTalkbackPreference());
  }
  final Stopwatch _gestureClock = Stopwatch()..start();
  late final TalkbackGestureController _talkback;
  static const _talkbackModeKey = 'openfader.talkbackMode';
  TalkbackModePreference _talkbackMode = TalkbackModePreference.momentary;
  int _preferenceRevision = 0;

  Future<void> _loadTalkbackPreference() async {
    final revision = _preferenceRevision;
    final preferences = await SharedPreferences.getInstance();
    final value = preferences.getInt(_talkbackModeKey);
    if (isDisposed ||
        revision != _preferenceRevision ||
        value == null ||
        value < 0 ||
        value >= TalkbackModePreference.values.length) {
      return;
    }
    _talkback.cancel();
    _talkbackMode = TalkbackModePreference.values[value];
    notifyListeners();
  }

  @override
  int get controlAccessEpoch => app.controlAccessEpoch;

  @override
  TalkbackModePreference get talkbackModePreference => _talkbackMode;

  @override
  Future<void> setTalkbackModePreference(TalkbackModePreference mode) async {
    if (isDisposed) return;
    _talkback.cancel();
    ++_preferenceRevision;
    _talkbackMode = mode;
    notifyListeners();
    final preferences = await SharedPreferences.getInstance();
    if (!isDisposed && _talkbackMode == mode) {
      await preferences.setInt(_talkbackModeKey, mode.index);
    }
  }

  @override
  DestinationControlVm? get talkbackGestureControl => engine.destinationControls
      .where((row) => row.talkbackGesture)
      .firstOrNull;
  @override
  bool get canPressTalkback => !isDisposed && _talkback.canPress;
  @override
  bool get talkbackHeld => _talkback.held;
  @override
  bool get talkbackBusy => _talkback.busy;
  @override
  String? get talkbackGestureError => _talkback.error;
  @override
  void pressTalkback({int elapsedMilliseconds = 0}) =>
      _talkback.press(_talkbackMode, elapsedMilliseconds: elapsedMilliseconds);
  @override
  void releaseTalkback() => _talkback.release();
  @override
  void cancelTalkback() => _talkback.cancel();
  @override
  void tapTalkback() => _talkback.tap(_talkbackMode);

  /// Driven only by the existing shell loop. No page or gesture timer.
  void tickGestures() {
    if (!isDisposed) _talkback.poll();
  }

  @override
  void forwardChange() {
    if (!isDisposed) _talkback.poll();
    super.forwardChange();
  }

  @override
  void dispose() {
    _talkback.dispose();
    _gestureClock.stop();
    super.dispose();
  }

  final ShellFxStore fx;

  @override
  int get session => engine.session;

  @override
  List<DestinationControlVm> get fxControls => List.unmodifiable(
        fx.destinationControls.where(
          (control) =>
              // Only the provider's explicit FX mute and tap metadata. Send masters
              // and return strips remain separate; binary choices alone imply nothing.
              (control.id == 'mute' &&
                  !control.numeric &&
                  control.choices.length == 2) ||
              control.canTapTempo,
        ),
      );

  @override
  List<DestinationControlVm> get globalControls => List.unmodifiable(
        engine.destinationControls
            .where((control) => control.surface == 'global'),
      );

  @override
  bool isCurrent(DestinationControlVm control) =>
      !isDisposed &&
      (fxControls.contains(control) || globalControls.contains(control)) &&
      app.destinationControlIsCurrent(control);

  @override
  Future<String?> setControl(DestinationControlVm control, double value) =>
      isCurrent(control)
          ? app.setDestinationControl(control, value)
          : Future.value('Connection or setting changed. Reopen Quick Access.');

  @override
  Future<String?> tapControl(DestinationControlVm control) => isCurrent(control)
      ? fx.tapDestinationControl(control)
      : Future.value('Connection or setting changed. Reopen Quick Access.');

  @override
  String? get clearSoloLabel => switch (engine.clearSoloScope) {
        'inputs' => 'Clear input solos',
        'advertised' => 'Clear all solos',
        _ => null,
      };

  @override
  bool get canClearSolo =>
      !isDisposed &&
      clearSoloLabel != null &&
      app.onlineFlag &&
      engine.connected &&
      engine.capsReady &&
      engine.locks.isEmpty &&
      app.canClearSolos &&
      app.accessFamilyAllowed(AccessFamily.channelSolo) &&
      (engine.clearSoloScope == 'advertised'
          ? engine.destinationControls.where((r) => r.targetSolo).isNotEmpty &&
              engine.destinationControls.where((r) => r.targetSolo).every(
                    (r) =>
                        r.available &&
                        r.authority == 'verified' &&
                        r.members.isNotEmpty &&
                        r.members.every(app.accessTargetAllowed),
                  )
          : engine.strips.any((strip) => strip.key.startsWith('ch/')) &&
              engine.strips.where((strip) => strip.key.startsWith('ch/')).every(
                    (strip) =>
                        app.accessTargetAllowed(strip.key) &&
                        engine.valueKnown(strip.ch, 'solo'),
                  ));

  @override
  void clearSolo(int session) {
    if (session != engine.session || !canClearSolo) return;
    // The provider owns coverage and publishes only confirmed changes.
    // This is an immediate action, never an offline/coalesced edit.
    send(Cmds.clearSolo());
  }
}
