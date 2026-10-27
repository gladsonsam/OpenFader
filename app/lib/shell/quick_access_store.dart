import 'dart:async';

import 'package:shared_preferences/shared_preferences.dart';

import '../core/access_limits.dart';
import '../engine.dart';
import '../core/quick_access.dart';
import 'fx_store.dart';
import 'store_adapter.dart';

class ShellQuickAccessStore extends ShellStoreAdapter
    implements QuickAccessStore {
  ShellQuickAccessStore(super.app, this.fx) {
    unawaited(_loadTalkbackPreference());
  }
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
    _talkbackMode = TalkbackModePreference.values[value];
    notifyListeners();
  }

  @override
  TalkbackModePreference get talkbackModePreference => _talkbackMode;

  @override
  Future<void> setTalkbackModePreference(TalkbackModePreference mode) async {
    if (isDisposed) return;
    ++_preferenceRevision;
    _talkbackMode = mode;
    notifyListeners();
    final preferences = await SharedPreferences.getInstance();
    if (!isDisposed && _talkbackMode == mode) {
      await preferences.setInt(_talkbackModeKey, mode.index);
    }
  }

  @override
  String? get talkbackGestureUnavailable =>
      'Momentary and Auto gestures are unavailable for this provider. '
      'The saved mode is local only. Enable (latch) is an explicit mixer command.';
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
      engine.strips.any((strip) => strip.key.startsWith('ch/')) &&
      engine.strips.where((strip) => strip.key.startsWith('ch/')).every(
            (strip) =>
                app.accessTargetAllowed(strip.key) &&
                engine.valueKnown(strip.ch, 'solo'),
          );

  @override
  void clearSolo(int session) {
    if (session != engine.session || !canClearSolo) return;
    // Keep other solo families intact; the provider echoes only its coverage.
    // This is an immediate action, never an offline/coalesced edit.
    send(Cmds.clearSolo());
  }
}
