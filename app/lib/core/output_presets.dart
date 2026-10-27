import 'destination_controls.dart';
import 'processor_presets.dart';

/// Captured destination block. Never retarget an open dialog to new descriptors.
class OutputPresetTarget {
  final int session;
  final String driver;
  final String target;
  final String surface;
  final List<DestinationControlVm> controls;
  OutputPresetTarget(
    this.session,
    this.driver,
    this.target,
    this.surface,
    List<DestinationControlVm> controls,
  ) : controls = List.unmodifiable(controls);

  ProcessorPresetKind? get kind => outputPresetKind(surface);
  String get name => controls.first.destinationName;
}

ProcessorPresetKind? outputPresetKind(String surface) => switch (surface) {
      'output_peq' => ProcessorPresetKind.outputPeq,
      'output_geq' => ProcessorPresetKind.outputGeq,
      'output_comp' => ProcessorPresetKind.outputCompressor,
      'output_delay' => ProcessorPresetKind.outputDelay,
      _ => null,
    };
