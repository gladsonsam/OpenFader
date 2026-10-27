import 'package:flutter/foundation.dart';

import 'destination_controls.dart';

/// Local preference values, matching the compiled QSettings enum.
/// This is not a mixer wire parameter or a claim of gesture support.
enum TalkbackModePreference { momentary, latch, auto }

/// Shell-injected, provider-neutral view of verified quick controls.
/// Descriptors remain opaque snapshots: implementations must recheck every write.
abstract class QuickAccessStore extends ChangeNotifier {
  int get session;
  List<DestinationControlVm> get fxControls;
  List<DestinationControlVm> get globalControls;
  TalkbackModePreference get talkbackModePreference;
  Future<void> setTalkbackModePreference(TalkbackModePreference mode);

  /// Null only when the provider advertises a cancellable held-control path.
  /// Ordinary enable writes must never be turned into queued hold gestures.
  String? get talkbackGestureUnavailable;
  bool isCurrent(DestinationControlVm control);
  Future<String?> setControl(DestinationControlVm control, double value);
  Future<String?> tapControl(DestinationControlVm control);

  /// Null means the provider has not advertised understood clear-solo coverage.
  String? get clearSoloLabel;
  bool get canClearSolo;
  void clearSolo(int session);
}
