import 'dart:convert';

/// Per-strip vertical composition. The strip renders [order] top-to-bottom,
/// skipping elements absent from [visible]. The meter has no row of its own:
/// it is fused into the fader cell and shown when [visible] contains
/// [StripElement.meter].
enum StripElement {
  processing,
  pan,
  mute,
  fader,
  meter,
  solo,
  nameplate;

  String get key => name;

  static StripElement? fromKey(Object? key) {
    for (final el in StripElement.values) {
      if (el.name == key) return el;
    }
    return null;
  }

  String get label => switch (this) {
        StripElement.processing => 'Processing',
        StripElement.pan => 'Pan',
        StripElement.mute => 'Mute',
        StripElement.fader => 'Fader',
        StripElement.meter => 'Meter',
        StripElement.solo => 'Solo',
        StripElement.nameplate => 'Nameplate',
      };
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

  factory StripLayout.defaults() => const StripLayout(
        order: defaultOrder,
        visible: {
          StripElement.processing,
          StripElement.pan,
          StripElement.mute,
          StripElement.fader,
          StripElement.meter,
          StripElement.solo,
          StripElement.nameplate,
        },
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

  /// Ordered, visible elements with a row of their own. Meter is fused into
  /// the fader cell, so it never appears here even when visible.
  List<StripElement> get visibleOrder => [
        for (final el in order)
          if (el != StripElement.meter && visible.contains(el)) el,
      ];

  bool get showMeter =>
      visible.contains(StripElement.meter) && visible.contains(StripElement.fader);

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
    List<StripElement> parseList(Object? value, List<StripElement> fallback) {
      if (value is! List) return fallback;
      final parsed = <StripElement>[];
      for (final entry in value) {
        final el = StripElement.fromKey(entry);
        if (el != null && !parsed.contains(el)) parsed.add(el);
      }
      // Repair: any missing element (except meter) rejoins at the end so a
      // hand-edited or older payload can never drop a control silently.
      for (final el in StripElement.values) {
        if (el != StripElement.meter && !parsed.contains(el)) parsed.add(el);
      }
      return parsed;
    }

    final order = parseList(json['order'], defaultOrder);
    final visibleRaw = json['visible'];
    final Set<StripElement> visible;
    if (visibleRaw is! List) {
      visible = StripLayout.defaults().visible;
    } else {
      visible = {
        for (final entry in visibleRaw)
          if (StripElement.fromKey(entry) != null) StripElement.fromKey(entry)!,
      };
      // The fader is mandatory: a strip without it is unusable.
      visible.add(StripElement.fader);
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
  int get hashCode =>
      Object.hash(Object.hashAll(order), Object.hashAllUnordered(visible),
          faderWeight, compactProcessing);
}

bool _listEquals(List<StripElement> a, List<StripElement> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
