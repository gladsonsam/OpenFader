import 'package:flutter/foundation.dart';

import 'destination_controls.dart';

/// Destination DSP is independent of input-strip indices and send-slot counts.
/// Providers supply the output inventory, processor surfaces and value domains.
abstract class OutputProcessingStore extends ChangeNotifier {
  List<DestinationControlVm> get outputProcessingControls;
  String? get outputProcessingTarget;
  void openOutputProcessing(String target);
  bool destinationControlIsCurrent(DestinationControlVm control);
  Future<String?> setDestinationControl(
    DestinationControlVm control,
    double value,
  );
}
