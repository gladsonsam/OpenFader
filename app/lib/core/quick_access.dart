import 'package:flutter/foundation.dart';

import 'destination_controls.dart';

/// Shell-injected, provider-neutral view of verified quick controls.
/// Descriptors remain opaque snapshots: implementations must recheck every write.
abstract class QuickAccessStore extends ChangeNotifier {
  int get session;
  List<DestinationControlVm> get fxControls;
  bool isCurrent(DestinationControlVm control);
  Future<String?> setControl(DestinationControlVm control, double value);
  Future<String?> tapControl(DestinationControlVm control);

  /// Null means the provider has not advertised understood clear-solo coverage.
  String? get clearSoloLabel;
  bool get canClearSolo;
  void clearSolo(int session);
}
