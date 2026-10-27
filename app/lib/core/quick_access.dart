import 'package:flutter/foundation.dart';

import 'destination_controls.dart';

import 'talkback_gesture.dart' show TalkbackModePreference;

export 'talkback_gesture.dart' show TalkbackModePreference;

/// Shell-injected, provider-neutral view of verified quick controls.
/// Descriptors remain opaque snapshots: implementations must recheck every write.
abstract class QuickAccessStore extends ChangeNotifier {
  int get session;
  List<DestinationControlVm> get fxControls;
  List<DestinationControlVm> get globalControls;
  TalkbackModePreference get talkbackModePreference;
  Future<void> setTalkbackModePreference(TalkbackModePreference mode);

  DestinationControlVm? get talkbackGestureControl;
  bool get canPressTalkback;
  bool get talkbackHeld;
  bool get talkbackBusy;
  String? get talkbackGestureError;
  void pressTalkback();
  void releaseTalkback();
  void cancelTalkback();
  void tapTalkback();
  bool isCurrent(DestinationControlVm control);
  Future<String?> setControl(DestinationControlVm control, double value);
  Future<String?> tapControl(DestinationControlVm control);

  /// Null means the provider has not advertised understood clear-solo coverage.
  String? get clearSoloLabel;
  bool get canClearSolo;
  void clearSolo(int session);
}
