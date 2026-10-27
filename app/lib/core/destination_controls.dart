import 'dart:typed_data';
import 'dart:convert';

/// A provider-owned control shared by all sends feeding one destination.
/// IDs and choices stay opaque to screens; values retain provider units.
class DestinationChoiceVm {
  final double value;
  final String label;
  const DestinationChoiceVm(this.value, this.label);

  @override
  bool operator ==(Object other) =>
      other is DestinationChoiceVm &&
      value == other.value &&
      label == other.label;
  @override
  int get hashCode => Object.hash(value, label);
}

class DestinationControlVm {
  final int epoch;
  final int? revision;
  final int? generation;
  final bool preset;
  final String _descriptorJson;
  Map<String, dynamic> get descriptor =>
      jsonDecode(_descriptorJson) as Map<String, dynamic>;
  final String target;
  final String targetLabel;
  final String? targetName;
  final int? sendSlot;
  final String surface;
  final String description;
  final String id;
  final String label;
  final bool tapTempo;

  /// Explicit provider opt-in; IDs/binary choices alone imply no gesture.
  final bool talkbackGesture;
  final List<DestinationChoiceVm> choices;
  final double? min;
  final double? max;
  final String unit;
  final int decimals;
  final double? value;
  final String? authority;

  const DestinationControlVm._(
      this.epoch,
      this.revision,
      this.generation,
      this.preset,
      this._descriptorJson,
      this.target,
      this.targetLabel,
      this.targetName,
      this.sendSlot,
      this.surface,
      this.description,
      this.id,
      this.label,
      this.tapTempo,
      this.talkbackGesture,
      this.choices,
      this.min,
      this.max,
      this.unit,
      this.decimals,
      this.value,
      this.authority);

  bool get targetSolo => surface == 'solo';
  List<String> get members => List.unmodifiable(
      (descriptor['members'] as List? ?? const []).whereType<String>());

  /// Native VCA and FX-return identities also respect existing strip aliases.
  bool soloScopeAllowed(bool familyAllowed, bool Function(String) targetAllowed) =>
      targetSolo && familyAllowed && members.isNotEmpty &&
      members.every((target) => targetAllowed(target) &&
          (!target.startsWith('return/') ||
              targetAllowed(target.replaceFirst('return/', 'ret/'))) &&
          (!target.startsWith('vca/') ||
              targetAllowed(target.replaceFirst('vca/', 'dca/'))) &&
          (id != 'return_solo' ||
              targetAllowed(target.replaceFirst('fx/', 'fxret/'))));

  String get destinationName => targetName == null || targetName!.isEmpty
      ? targetLabel
      : '$targetLabel · $targetName';
  String? get valueLabel {
    for (final choice in choices) {
      if (choice.value == value) return choice.label;
    }
    if (value != null && numeric && allowsValue(value!)) {
      return '${value!.toStringAsFixed(decimals)}${unit.isEmpty ? '' : ' $unit'}';
    }
    return null;
  }

  bool get numeric => min != null && max != null;
  bool get canTapTempo =>
      tapTempo &&
      numeric &&
      const ['ms', 's', 'bpm'].contains(unit.toLowerCase());

  double? valueForTapPeriod(int microseconds) {
    if (!canTapTempo || microseconds <= 0) return null;
    final value = switch (unit.toLowerCase()) {
      'ms' => microseconds / 1000,
      's' => microseconds / 1000000,
      'bpm' => 60000000 / microseconds,
      _ => double.nan,
    };
// Native FX tap intervals pause above 1.5 times the maximum, then clamp.
    if (surface == 'fx') {
      if (value > max! * 1.5) return null;
      return wireValue(value.clamp(min!, max!));
    }
    return wireValue(value);
  }

  /// Exact value sent to the engine and expected back in its f32 receipt.
  /// Validate enum identity before rounding: a nearby number is not a choice.
  /// Numeric bounds are provider f32 values, so their displayed decimal
  /// endpoints (e.g. 115.850 m) remain enterable at the same wire precision.
  double? wireValue(double candidate) {
    if (!candidate.isFinite) return null;
    if (choices.isNotEmpty) {
      for (final choice in choices) {
        if (choice.value == candidate) return choice.value;
      }
      return null;
    }
    if (!numeric) return null;
    final wireValue = Float32List.fromList([candidate]).first;
    return wireValue.isFinite && wireValue >= min! && wireValue <= max!
        ? wireValue
        : null;
  }

  bool allowsValue(double candidate) => wireValue(candidate) != null;

  bool get available => valueLabel != null && authority != null;

  static DestinationControlVm? fromJson(int epoch, Map data) {
    final target = data['target'];
    final targetLabel = data['target_label'];
    final id = data['id'];
    final label = data['label'];
    final rawChoices = data['choices'];
    final tapTempo = data['tap_tempo'];
    if (data['scope'] != 'destination' ||
        target is! String ||
        target.isEmpty ||
        targetLabel is! String ||
        targetLabel.isEmpty ||
        id is! String ||
        id.isEmpty ||
        label is! String ||
        label.isEmpty ||
        rawChoices is! List ||
        (tapTempo != null && tapTempo is! bool)) {
      return null;
    }
    final choices = <DestinationChoiceVm>[];
    for (final raw in rawChoices) {
      if (raw is! Map) return null;
      final value = raw['value'];
      final name = raw['label'];
      if (value is! num ||
          !value.isFinite ||
          name is! String ||
          name.isEmpty ||
          choices.any((choice) => choice.value == value)) {
        return null;
      }
      choices.add(DestinationChoiceVm(value.toDouble(), name));
    }
    final rawRange = data['range'];
    double? min;
    double? max;
    var unit = '';
    var decimals = 0;
    if (rawRange != null) {
      if (rawRange is! Map) return null;
      final low = rawRange['min'];
      final high = rawRange['max'];
      final digits = rawRange['decimals'];
      if (low is! num ||
          high is! num ||
          !low.isFinite ||
          !high.isFinite ||
          low >= high ||
          rawRange['unit'] is! String ||
          digits is! int ||
          digits < 0 ||
          digits > 6 ||
          choices.isNotEmpty) {
        return null;
      }
      min = low.toDouble();
      max = high.toDouble();
      unit = rawRange['unit'] as String;
      decimals = digits;
    }
    if (choices.isEmpty && min == null) return null;
    final rawValue = data['value'];
    final rawAuthority = data['authority'];
    final authority = rawAuthority == 'verified' || rawAuthority == 'accepted'
        ? rawAuthority as String
        : null;
    final value = rawValue is num && rawValue.isFinite && authority != null
        ? rawValue.toDouble()
        : null;
    final send = data['send_slot'];
    return DestinationControlVm._(
        epoch,
        data['revision'] is int ? data['revision'] as int : null,
        data['generation'] is int ? data['generation'] as int : null,
        data['preset'] == true,
        jsonEncode(data),
        target,
        targetLabel,
        data['target_name'] is String ? data['target_name'] as String : null,
        send is int && send > 0 ? send : null,
        data['surface'] is String ? data['surface'] as String : 'send',
        data['description'] is String
            ? data['description'] as String
            : 'Applies to every input feeding $targetLabel. Changing this can alter that mix.',
        id,
        label,
        tapTempo == true,
        data['gesture'] == 'talkback',
        List.unmodifiable(choices),
        min,
        max,
        unit,
        decimals,
        value,
        authority);
  }

  @override
  bool operator ==(Object other) =>
      other is DestinationControlVm &&
      epoch == other.epoch &&
      revision == other.revision &&
      generation == other.generation &&
      preset == other.preset &&
      target == other.target &&
      targetLabel == other.targetLabel &&
      targetName == other.targetName &&
      sendSlot == other.sendSlot &&
      surface == other.surface &&
      description == other.description &&
      id == other.id &&
      label == other.label &&
      tapTempo == other.tapTempo &&
      talkbackGesture == other.talkbackGesture &&
      value == other.value &&
      authority == other.authority &&
      min == other.min &&
      max == other.max &&
      unit == other.unit &&
      decimals == other.decimals &&
      choices.length == other.choices.length &&
      Iterable.generate(choices.length)
          .every((i) => choices[i] == other.choices[i]);
  @override
  int get hashCode => Object.hash(
      epoch,
      revision,
      generation,
      preset,
      target,
      targetLabel,
      targetName,
      sendSlot,
      surface,
      description,
      id,
      label,
      Object.hash(tapTempo, talkbackGesture),
      value,
      authority,
      min,
      max,
      unit,
      decimals,
      Object.hashAll(choices));
}

/// Receipt comes from a provider-held write lane after independent readback.
/// A later matching cache value cannot be substituted to infer ownership.
class DestinationEditResult {
  final String? error;
  final DestinationControlVm? applied;
  const DestinationEditResult(this.error, [this.applied]);
}
