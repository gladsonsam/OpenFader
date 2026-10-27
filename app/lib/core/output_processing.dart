import 'package:flutter/foundation.dart';

import 'output_meters.dart';
import 'destination_controls.dart';
import 'output_presets.dart';
import 'processor_presets.dart';

/// Destination DSP is independent of input-strip indices and send-slot counts.
/// Providers supply the output inventory, processor surfaces and value domains.
abstract class OutputProcessingStore extends ChangeNotifier {
  int get controlAccessEpoch => 0;

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

  /// Applies every advertised GEQ gain, leaving all other fields unchanged.
  /// The captured block and output selection must still be current.
  Future<ProcessorPresetResult> applyOutputGeqDraft(
    OutputPresetTarget target,
    Map<String, double> gains,
  );
  OutputPresetTarget? captureOutputGeqDraftTarget();
  String? outputGeqDraftTargetError(OutputPresetTarget target);

  /// Stops before the next dispatch; an in-flight write can still complete.
  void cancelOutputPresetRecall();
  Map<String, String> get outputTargets;
  ValueListenable<OutputMeterVm> outputMeterFor(String target);
  List<DestinationControlVm> get outputProcessingControls;
  String? get outputProcessingTarget;
  void openOutputProcessing(String target);
  bool destinationControlIsCurrent(DestinationControlVm control);
  Future<String?> setDestinationControl(
    DestinationControlVm control,
    double value,
  );
}
