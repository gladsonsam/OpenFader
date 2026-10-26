import '../core/access_limits.dart';
import '../engine.dart';
import '../core/quick_access.dart';
import 'fx_store.dart';
import 'store_adapter.dart';

class ShellQuickAccessStore extends ShellStoreAdapter
    implements QuickAccessStore {
  ShellQuickAccessStore(super.app, this.fx);
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
  bool isCurrent(DestinationControlVm control) =>
      !isDisposed &&
      fxControls.contains(control) &&
      fx.destinationControlIsCurrent(control);

  @override
  Future<String?> setControl(DestinationControlVm control, double value) =>
      isCurrent(control)
          ? fx.setDestinationControl(control, value)
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
