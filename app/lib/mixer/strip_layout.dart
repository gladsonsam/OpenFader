import 'dart:convert';

/// Per-strip vertical composition. The strip renders [order] top-to-bottom,
/// skipping elements absent from [visible]. The meter has no row of its own:
/// it is fused into the fader cell and shown when the layout includes it.
///
/// Labels, descriptions and the row rules live on the element itself so the
/// strip, the settings editor and persisted layouts all use one typed model.
/// Adding an element means extending this enum; every consumer follows.
enum StripElement {
  processing(
    label: 'Processing',
    description: 'Gain, EQ and dynamics preview',
  ),
  pan(label: 'Pan', description: 'Balance control'),
  mute(label: 'Mute', description: 'Mute button'),
  fader(label: 'Fader', description: 'Level control', mandatory: true),
  meter(
    label: 'Meter',
    description: 'Level meter inside the fader',
    fused: true,
  ),
  solo(label: 'Solo', description: 'Solo button'),
  nameplate(
    label: 'Nameplate',
    description: 'Channel number, icon and name',
    mandatory: true,
  );

  const StripElement({
    required this.label,
    required this.description,
    this.mandatory = false,
    this.fused = false,
  });

  /// Short action/value name for editors and readouts.
  final String label;

  /// One line of context for the settings editor. The strip supplies context
  /// for the control itself, so this never becomes a strip label.
  final String description;

  /// Cannot be hidden: a strip without this element is unusable.
  final bool mandatory;

  /// Has no row of its own and is drawn inside its host element.
  final bool fused;

  /// Whether [StripLayout.order] carries this element as a draggable row.
  bool get hasOwnRow => !fused;

  String get key => name;

  static StripElement? fromKey(Object? key) {
    for (final el in StripElement.values) {
      if (el.name == key) return el;
    }
    return null;
  }
}

class StripLayout {
  static const int version = 1;
  static const double defaultFaderWeight = 3.0;

  static const List<StripElement> defaultOrder = [
    StripElement.processing,
    StripElement.pan,
    StripElement.mute,
    StripElement.fader,
    StripElement.solo,
    StripElement.nameplate,
  ];

  final List<StripElement> order;
  final Set<StripElement> visible;
  final double faderWeight;
  final bool compactProcessing;

  const StripLayout({
    required this.order,
    required this.visible,
    this.faderWeight = defaultFaderWeight,
    this.compactProcessing = false,
  });

  factory StripLayout.defaults() => StripLayout(
        order: defaultOrder,
        visible: StripElement.values.toSet(),
      );

  /// Everything on: the full console strip.
  static StripLayout get full => StripLayout.defaults();

  /// Just the essentials for a small surface: fader, meter, mute, name.
  static StripLayout get essential => const StripLayout(
        order: defaultOrder,
        visible: {
          StripElement.fader,
          StripElement.meter,
          StripElement.mute,
          StripElement.nameplate,
        },
      );

  /// Monitor mix: levels first, no processing preview or pan.
  static StripLayout get monitor => const StripLayout(
        order: defaultOrder,
        faderWeight: 2.0,
        visible: {
          StripElement.fader,
          StripElement.meter,
          StripElement.mute,
          StripElement.solo,
          StripElement.nameplate,
        },
      );

  /// Phone-sized strip: compact processing preview, fader-led.
  static StripLayout get phone => const StripLayout(
        order: defaultOrder,
        faderWeight: 4.0,
        compactProcessing: true,
        visible: {
          StripElement.processing,
          StripElement.mute,
          StripElement.fader,
          StripElement.meter,
          StripElement.nameplate,
        },
      );

  /// Ordered rows with a place in the strip. The fused meter has no row, so
  /// it never appears here even when visible.
  List<StripElement> get rows => [
        for (final el in order)
          if (el.hasOwnRow) el,
      ];

  /// Ordered, visible elements with a row of their own.
  List<StripElement> get visibleOrder => [
        for (final el in order)
          if (el.hasOwnRow && shows(el)) el,
      ];

  /// Whether the element participates in the strip. Mandatory elements stay
  /// present even if a hand-edited payload omitted them.
  bool shows(StripElement element) =>
      element.mandatory || visible.contains(element);

  /// The meter is fused into the fader, so both must be present.
  bool get showMeter =>
      shows(StripElement.meter) && shows(StripElement.fader);

  /// All row elements exactly once, in [values] order first and canonical
  /// order for anything missing. Fused elements are not rows.
  static List<StripElement> normalizeOrder(Iterable<StripElement> values) {
    final parsed = <StripElement>[];
    for (final element in values) {
      if (element.hasOwnRow && !parsed.contains(element)) parsed.add(element);
    }
    for (final element in StripElement.values) {
      if (element.hasOwnRow && !parsed.contains(element)) parsed.add(element);
    }
    return parsed;
  }

  StripLayout withOrder(Iterable<StripElement> next) =>
      copyWith(order: normalizeOrder(next));

  /// Set visibility through the model, so mandatory rows can never be hidden.
  StripLayout withVisibility(StripElement element, bool isVisible) {
    if (element.mandatory && !isVisible) return this;
    final next = {...visible};
    if (isVisible) {
      next.add(element);
    } else {
      next.remove(element);
    }
    return copyWith(visible: next);
  }

  StripLayout copyWith({
    List<StripElement>? order,
    Set<StripElement>? visible,
    double? faderWeight,
    bool? compactProcessing,
  }) =>
      StripLayout(
        order: order ?? this.order,
        visible: visible ?? this.visible,
        faderWeight: faderWeight ?? this.faderWeight,
        compactProcessing: compactProcessing ?? this.compactProcessing,
      );

  Map<String, Object> toJson() => {
        'version': version,
        'order': [for (final el in order) el.key],
        'visible': [for (final el in visible) el.key],
        'faderWeight': faderWeight,
        'compactProcessing': compactProcessing,
      };

  String encode() => jsonEncode(toJson());

  static StripLayout? tryDecode(String? raw) {
    if (raw == null || raw.isEmpty) return null;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return null;
      return StripLayout.fromJson(decoded);
    } catch (_) {
      return null;
    }
  }

  factory StripLayout.fromJson(Map<dynamic, dynamic> json) {
    if ((json['version'] as num?)?.toInt() != version) {
      throw const FormatException('unsupported strip layout version');
    }
    List<StripElement> parseOrder(Object? value) {
      if (value is! List) return defaultOrder;
      final parsed = <StripElement>[];
      for (final entry in value) {
        final element = StripElement.fromKey(entry);
        if (element != null) parsed.add(element);
      }
      return normalizeOrder(parsed);
    }

    final order = parseOrder(json['order']);
    final visibleRaw = json['visible'];
    final Set<StripElement> visible;
    if (visibleRaw is! List) {
      visible = StripLayout.defaults().visible;
    } else {
      final parsed = <StripElement>{};
      for (final entry in visibleRaw) {
        final element = StripElement.fromKey(entry);
        if (element != null) parsed.add(element);
      }
      parsed.addAll(StripElement.values.where((element) => element.mandatory));
      visible = parsed;
    }
    final weight = (json['faderWeight'] as num?)?.toDouble() ?? defaultFaderWeight;
    return StripLayout(
      order: order,
      visible: visible,
      faderWeight: weight.clamp(1.0, 6.0),
      compactProcessing: (json['compactProcessing'] as bool?) ?? false,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is StripLayout &&
      _listEquals(order, other.order) &&
      visible.length == other.visible.length &&
      visible.containsAll(other.visible) &&
      faderWeight == other.faderWeight &&
      compactProcessing == other.compactProcessing;

  @override
  int get hashCode => Object.hash(Object.hashAll(order),
      Object.hashAllUnordered(visible), faderWeight, compactProcessing);
}

bool _listEquals(List<StripElement> a, List<StripElement> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
