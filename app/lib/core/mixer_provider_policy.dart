import 'dart:convert';

import 'channel_views.dart';

typedef ProviderSourceMode = ({
  String target,
  String group,
  String port,
  int mode
});
typedef ProviderDelayUnit = ({String label, int precision});
typedef ProviderDelay = ({
  int unit,
  int steps,
  num value,
  num min,
  num max,
  bool on,
  bool editable
});

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
  List<ChannelModeChoice> get sourceModeChoices => const [];
  bool get supportsDelayCodec => false;
  ProviderDelayUnit? delayUnit(int unit) => null;
  ProviderSourceMode? decodeSourceMode(Object? raw) => null;
  Object? encodeSourceMode(ChannelSourceModeVm snapshot, int mode) => null;
  ProviderDelay? decodeDelay(Object? raw, double? currentUnit) => null;
  Object? encodeDelay(ChannelDelayVm snapshot, double value) => null;
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

class X32ProviderPolicy extends MixerProviderPolicy {
  const X32ProviderPolicy();
  @override
  bool get masterChannelProcessing => true;
  @override
  String get eqBandCountLeaf => 'x32:eq_bands';
  @override
  bool get requiresRoutedPreamp => true;
  @override
  String get preampStateLeaf => 'x32:preamp_state';
  @override
  String parameterLeaf(String leaf) => switch (leaf) {
        'provider:automix_gain' => 'x32:automix_gain',
        'provider:insert_position' => 'x32:insert_pos',
        _ => leaf,
      };
  // Documented native EQ enum (X32.c Xeqty1): 0 LCut, 1 LShv, 2 PEQ, 3 VEQ,
  // 4 HShv, 5 HCut — identical to the canonical presentation values.
  // Out-of-range natives stay unrepresented; no X-Air DSP response model is
  // assumed, so cuts/VEQ render as unavailable until a verified model lands.
  @override
  int? eqPresentationType(int? native) =>
      native != null && native >= 0 && native <= 5 ? native : null;
  @override
  int eqNativeType(EqFilterShape shape) => shape.value;
  // Documented HPF slopes (X32.c Xhslop): native 0..2 = 12/18/24 dB/oct.
  @override
  int hpfSlope(double? native) => switch (native?.toInt()) {
        0 => 12,
        1 => 18,
        2 => 24,
        _ => 12,
      };
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
  static const _sourceCodes = ['M', 'ST', 'M/S'];
  static const _delayCodes = ['M', 'FT', 'MS', 'SMP'];
  @override
  bool get supportsDelayCodec => true;
  @override
  List<ChannelModeChoice> get sourceModeChoices => const [
        (value: 0, label: 'Mono'),
        (value: 1, label: 'Stereo'),
        (value: 2, label: 'Mid/side'),
      ];
  @override
  ProviderDelayUnit? delayUnit(int unit) => switch (unit) {
        0 => (label: 'm', precision: 2),
        1 => (label: 'ft', precision: 2),
        2 => (label: 'ms', precision: 2),
        3 => (label: 'samples', precision: 0),
        _ => null,
      };
  @override
  ProviderSourceMode? decodeSourceMode(Object? raw) {
    if (raw is! String) return null;
    final parts = raw.split(':');
    if (parts.length != 2) return null;
    final mode = _sourceCodes.indexOf(parts[1]);
    final port = parts[0].split('/');
    if (mode < 0 || port.length != 2 || int.tryParse(port[1]) == null) {
      return null;
    }
    return (target: parts[0], group: port[0], port: port[1], mode: mode);
  }

  @override
  Object? encodeSourceMode(ChannelSourceModeVm snapshot, int mode) {
    if (mode < 0 ||
        mode >= _sourceCodes.length ||
        snapshot.mode < 0 ||
        snapshot.mode >= _sourceCodes.length) {
      return null;
    }
    return '${snapshot.target}:${_sourceCodes[snapshot.mode]}>${_sourceCodes[mode]}';
  }

  @override
  ProviderDelay? decodeDelay(Object? raw, double? currentUnit) {
    if (raw is! String) return null;
    try {
      final data = jsonDecode(raw);
      if (data is! Map) return null;
      final unit = data['unit'],
          value = data['value'],
          min = data['min'],
          max = data['max'],
          steps = data['steps'];
      if (unit is! int ||
          unit < 0 ||
          unit >= _delayCodes.length ||
          value is! num ||
          min is! num ||
          max is! num ||
          steps is! int ||
          steps < 0 ||
          !value.isFinite ||
          !min.isFinite ||
          !max.isFinite ||
          min >= max ||
          value < min ||
          value > max ||
          data['on'] is! bool ||
          data['editable'] is! bool ||
          currentUnit != unit) {
        return null;
      }
      return (
        unit: unit,
        value: value,
        min: min,
        max: max,
        steps: steps,
        on: data['on'] as bool,
        editable: data['editable'] as bool
      );
    } catch (_) {
      return null;
    }
  }

  @override
  Object? encodeDelay(ChannelDelayVm snapshot, double value) {
    if (snapshot.unit < 0 ||
        snapshot.unit >= _delayCodes.length ||
        !value.isFinite ||
        value < snapshot.min ||
        value > snapshot.max) {
      return null;
    }
    final step = snapshot.steps > 1
        ? (snapshot.max - snapshot.min) / (snapshot.steps - 1)
        : 0.0;
    final snapped = step > 0
        ? (snapshot.min + ((value - snapshot.min) / step).round() * step)
            .clamp(snapshot.min, snapshot.max)
        : value;
    return '${_delayCodes[snapshot.unit]}:$snapped';
  }

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
      'x32' => const X32ProviderPolicy(),
      'dl32s' => const Dl32sProviderPolicy(),
      'wing' => const WingProviderPolicy(),
      'sim' => const SimProviderPolicy(),
      _ => const UnsupportedProviderPolicy(),
    };
