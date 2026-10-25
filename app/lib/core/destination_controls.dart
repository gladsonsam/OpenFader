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
  final String target;
  final String targetLabel;
  final String? targetName;
  final int? sendSlot;
  final String id;
  final String label;
  final List<DestinationChoiceVm> choices;
  final double? value;
  final String? authority;

  const DestinationControlVm._(
      this.epoch,
      this.target,
      this.targetLabel,
      this.targetName,
      this.sendSlot,
      this.id,
      this.label,
      this.choices,
      this.value,
      this.authority);

  String get destinationName => targetName == null || targetName!.isEmpty
      ? targetLabel
      : '$targetLabel · $targetName';
  String? get valueLabel {
    for (final choice in choices) {
      if (choice.value == value) return choice.label;
    }
    return null;
  }

  bool get available => valueLabel != null && authority != null;

  static DestinationControlVm? fromJson(int epoch, Map data) {
    final target = data['target'];
    final targetLabel = data['target_label'];
    final id = data['id'];
    final label = data['label'];
    final rawChoices = data['choices'];
    if (data['scope'] != 'destination' ||
        target is! String ||
        target.isEmpty ||
        targetLabel is! String ||
        targetLabel.isEmpty ||
        id is! String ||
        id.isEmpty ||
        label is! String ||
        label.isEmpty ||
        rawChoices is! List) {
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
    if (choices.isEmpty) return null;
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
        target,
        targetLabel,
        data['target_name'] is String ? data['target_name'] as String : null,
        send is int && send > 0 ? send : null,
        id,
        label,
        List.unmodifiable(choices),
        value,
        authority);
  }

  @override
  bool operator ==(Object other) =>
      other is DestinationControlVm &&
      epoch == other.epoch &&
      target == other.target &&
      targetLabel == other.targetLabel &&
      targetName == other.targetName &&
      sendSlot == other.sendSlot &&
      id == other.id &&
      label == other.label &&
      value == other.value &&
      authority == other.authority &&
      choices.length == other.choices.length &&
      Iterable.generate(choices.length)
          .every((i) => choices[i] == other.choices[i]);
  @override
  int get hashCode => Object.hash(epoch, target, targetLabel, targetName,
      sendSlot, id, label, value, authority, Object.hashAll(choices));
}
