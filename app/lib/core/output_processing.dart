import 'package:flutter/foundation.dart';

import 'destination_controls.dart';
import 'output_presets.dart';
import 'processor_presets.dart';

/// Destination DSP is independent of input-strip indices and send-slot counts.
/// Providers supply the output inventory, processor surfaces and value domains.
abstract class OutputProcessingStore extends ChangeNotifier {
  /// Frozen output block, independent of input-channel preset targets.
  OutputPresetTarget? captureOutputPresetTarget(String surface);
  String? outputPresetTargetError(OutputPresetTarget target);
  Future<List<ProcessorPreset>> loadOutputPresets(OutputPresetTarget target);
  Future<String?> saveOutputPreset(OutputPresetTarget target, String name);
  Future<String?> renameOutputPreset(
    OutputPresetTarget target,
    String id,
    String name,
  );
  Future<String?> deleteOutputPreset(OutputPresetTarget target, String id);
  String? copyOutputProcessor(OutputPresetTarget target);
  bool get hasOutputProcessorClipboard;
  /// Null id pastes the immutable in-memory processor clipboard.
  /// Acceptance without independent mixer readback is reported as unconfirmed.
  Future<ProcessorPresetResult> recallOutputPreset(
    OutputPresetTarget target,
    String? id,
  );
  /// Stops before the next dispatch; an in-flight write can still complete.
  void cancelOutputPresetRecall();
  List<DestinationControlVm> get outputProcessingControls;
  String? get outputProcessingTarget;
  void openOutputProcessing(String target);
  bool destinationControlIsCurrent(DestinationControlVm control);
  Future<String?> setDestinationControl(
    DestinationControlVm control,
    double value,
  );
}
