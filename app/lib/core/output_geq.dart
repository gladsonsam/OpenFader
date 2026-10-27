import 'destination_controls.dart';
import 'output_presets.dart';

/// Canonical GEQ gain descriptors in provider-advertised order. The inventory,
/// band count, labels and domains belong to the provider, not a desk model.
List<DestinationControlVm> outputGeqBands(
  Iterable<DestinationControlVm> controls,
) =>
    List.unmodifiable(
      controls.where(
        (c) =>
            c.surface == 'output_geq' &&
            RegExp(r'^geq\d+_gain$').hasMatch(c.id),
      ),
    );

String? outputGeqDraftError(OutputPresetTarget target) {
  final bands = outputGeqBands(target.controls);
  if (target.surface != 'output_geq' ||
      bands.isEmpty ||
      bands.map((c) => c.id).toSet().length != bands.length ||
      bands.any((c) => !c.preset || !c.numeric || c.unit != 'dB')) {
    return 'This output does not advertise editable Graphic EQ gains.';
  }
  if (bands.any(
    (c) =>
        !c.available ||
        c.authority != 'verified' ||
        c.value == null ||
        !c.allowsValue(c.value!),
  )) {
    return 'Wait for verified mixer readback before editing Graphic EQ.';
  }
  return null;
}

/// Round to the advertised display precision, clamp to this band's domain and
/// retain wire precision so verified float32 readback can match the proposal.
double outputGeqDraftGain(DestinationControlVm band, double value) =>
    band.wireValue(
      double.parse(value.toStringAsFixed(band.decimals))
          .clamp(band.min!, band.max!),
    )!;
