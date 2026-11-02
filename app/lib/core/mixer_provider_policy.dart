import 'channel_views.dart';

/// Presentation and safety adaptations for provider-specific wire semantics.
/// Unknown providers never acquire an analyser or infer a master DSP target.
abstract class MixerProviderPolicy {
  const MixerProviderPolicy();
  bool get requiresRoutedPreamp => false;
  String? get preampStateLeaf => null;
  bool get meterSpectrumTarget => false;
  bool get channelSpectrumTarget => false;
  bool get masterChannelProcessing => false;
  bool get syntheticSpectrum => false;
  bool get offlineValues => false;
  int get legacyAuxCount => 0;
  EqCutModel get cutModel => EqCutModel.unavailable;
  EqBellModel get bellModel => EqBellModel.standard;
  EqShelfModel get shelfModel => EqShelfModel.standard;
  bool get shelfUsesQ => true;
  bool get hpfInResponse => true;
  bool get useNativeEqChoices => true;
  String parameterLeaf(String leaf) => leaf;
  String? get eqBandCountLeaf => null;
  bool isProviderDspRange(String leaf) => false;
  bool isInputRoutingField(String leaf) => false;
  bool isDspField(String leaf) => false;
  Object quantizeParameter(String leaf, Object value) => value;
  int? eqPresentationType(int? native) => native;
  int eqNativeType(EqFilterShape shape) => shape.value;
  int sourceSelection(int value) => value;
  int compKnee(bool soft) => soft ? 3 : 0;
  bool isSoftKnee(double value) => value > 0;
  int hpfSlope(double? native) => 12;
  int eqBandCount(double? reported, bool hasSix) => hasSix ? 6 : 4;
  bool skipRecalledField(String field, Map<String, dynamic> values) => false;
  bool validRawPreset(String field, double value) => true;
  bool eqQEditable(EqFilterShape? shape) => true;
}

class XairProviderPolicy extends MixerProviderPolicy {
  const XairProviderPolicy();
  @override
  bool get requiresRoutedPreamp => true;
  @override
  String parameterLeaf(String leaf) =>
      leaf == 'provider:automix_gain' ? 'xair:automix_gain' : leaf;
  @override
  String get preampStateLeaf => 'xair:preamp_state';
  @override
  bool get meterSpectrumTarget => true;
  @override
  bool get channelSpectrumTarget => true;
  @override
  bool get masterChannelProcessing => true;
  @override
  int get legacyAuxCount => 6;
  @override
  EqCutModel get cutModel => EqCutModel.xairButterworth96k;
  @override
  EqBellModel get bellModel => EqBellModel.xair96k;
  @override
  EqShelfModel get shelfModel => EqShelfModel.xair96k;
}

class Dl32sProviderPolicy extends MixerProviderPolicy {
  const Dl32sProviderPolicy();
  @override
  int get legacyAuxCount => 8;
  @override
  bool get shelfUsesQ => false;
  @override
  bool get useNativeEqChoices => false;
  @override
  int? eqPresentationType(int? native) => switch (native) {
        2 => EqFilterShape.bell.value,
        3 => EqFilterShape.lowShelf.value,
        4 => EqFilterShape.highShelf.value,
        _ => null,
      };
  @override
  int eqNativeType(EqFilterShape shape) =>
      shape == EqFilterShape.lowShelf ? 3 : shape.value;
  @override
  int sourceSelection(int value) => 1 - value;
  @override
  int compKnee(bool soft) => soft ? 0 : 1;
  @override
  bool isSoftKnee(double value) => value == 0;
  @override
  bool eqQEditable(EqFilterShape? shape) => shape == EqFilterShape.bell;
  @override
  bool skipRecalledField(String field, Map<String, dynamic> values) {
    final band = RegExp(r'^eq([1-4])_q$').firstMatch(field);
    return band != null && values['eq${band[1]}_type'] != 2;
  }

  @override
  bool validRawPreset(String field, double value) =>
      !const {'eq_mode', 'gate_mode', 'comp_mode', 'comp_knee'}
          .contains(field) ||
      value == 0 ||
      value == 1;
}

class WingProviderPolicy extends MixerProviderPolicy {
  const WingProviderPolicy();
  @override
  bool get meterSpectrumTarget => true;
  @override
  int get legacyAuxCount => 16;
  @override
  bool get hpfInResponse => false;
  @override
  String parameterLeaf(String leaf) => leaf.startsWith('provider:')
      ? 'wing:${leaf.substring('provider:'.length)}'
      : leaf;
  @override
  bool isInputRoutingField(String leaf) =>
      leaf == 'wing:source_auto' || leaf == 'wing:source_mode';
  @override
  bool isDspField(String leaf) =>
      RegExp(r'^wing:(eq_|filter_|hcf_)').hasMatch(leaf);
  @override
  String get eqBandCountLeaf => 'wing:eq_bands';
  @override
  bool isProviderDspRange(String leaf) =>
      leaf == 'wing:hcf_freq' ||
      leaf == 'wing:input_balance' ||
      leaf == 'wing:width' ||
      leaf.startsWith('wing:filter_') &&
          leaf != 'wing:filter_on' &&
          leaf != 'wing:filter_model';
  @override
  Object quantizeParameter(String leaf, Object value) {
    if (value is! num) return value;
    if (leaf == 'wing:width') return (value / 5).round() * 5.0;
    if (leaf == 'wing:input_balance') return (value * 10).round() / 10;
    return value;
  }

  @override
  int hpfSlope(double? native) =>
      native == null ? 12 : const [6, 12, 18, 24][native.toInt().clamp(0, 3)];
  @override
  int eqBandCount(double? reported, bool hasSix) =>
      (reported?.toInt() ?? (hasSix ? 6 : 4)).clamp(4, 8);
}

class SimProviderPolicy extends MixerProviderPolicy {
  const SimProviderPolicy();
  @override
  bool get meterSpectrumTarget => true;
  @override
  bool get syntheticSpectrum => true;
  @override
  bool get offlineValues => true;
  @override
  int get legacyAuxCount => 6;
}

class UnsupportedProviderPolicy extends MixerProviderPolicy {
  const UnsupportedProviderPolicy();
  @override
  bool get hpfInResponse => false;
  @override
  bool get shelfUsesQ => false;
}

MixerProviderPolicy mixerProviderPolicy(String driver) => switch (driver) {
      'xair' => const XairProviderPolicy(),
      'dl32s' => const Dl32sProviderPolicy(),
      'wing' => const WingProviderPolicy(),
      'sim' => const SimProviderPolicy(),
      _ => const UnsupportedProviderPolicy(),
    };
