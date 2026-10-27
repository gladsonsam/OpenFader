import 'dart:convert';

/// Custom mixer layers (P3): named channel sets with their own density.
///
/// The built-in [MixerLayer.all] layer follows bank paging exactly as before;
/// user layers carry an explicit [channelKeys] order and their own
/// [stripsPerPage]. The master strip is never part of a layer — it stays
/// fixed on the mixer page regardless of the active layer.
typedef LayerChannelOption = ({String key, String name});

class MixerLayer {
  static const int version = 1;

  /// Reserved id of the built-in layer. Never persisted in the layers list.
  static const String allLayerId = 'all';

  static const int minStripsPerPage = 4;
  static const int maxStripsPerPage = 12;
  static const int defaultStripsPerPage = 8;

  /// Densities offered by the layer editor stepper.
  static const List<int> stripsPerPageOptions = [4, 6, 8, 12];

  final String id;
  final String name;
  final List<String> channelKeys;
  final int stripsPerPage;

  const MixerLayer({
    required this.id,
    required this.name,
    required this.channelKeys,
    this.stripsPerPage = defaultStripsPerPage,
  });

  /// The built-in layer: follows bank paging as today.
  factory MixerLayer.all() => const MixerLayer(
        id: allLayerId,
        name: 'All',
        channelKeys: [],
      );

  bool get isAll => id == allLayerId;

  /// Effective density, clamped to the supported range so hand-edited or
  /// older payloads can never break layout.
  int get effectiveStripsPerPage =>
      stripsPerPage.clamp(minStripsPerPage, maxStripsPerPage);

  static String newId() =>
      'layer_${DateTime.now().microsecondsSinceEpoch}';

  MixerLayer copyWith({
    String? id,
    String? name,
    List<String>? channelKeys,
    int? stripsPerPage,
  }) =>
      MixerLayer(
        id: id ?? this.id,
        name: name ?? this.name,
        channelKeys: channelKeys ?? this.channelKeys,
        stripsPerPage: stripsPerPage ?? this.stripsPerPage,
      );

  Map<String, Object> toJson() => {
        'version': version,
        'id': id,
        'name': name,
        'channelKeys': channelKeys,
        'stripsPerPage': stripsPerPage,
      };

  factory MixerLayer.fromJson(Map<dynamic, dynamic> json) {
    if ((json['version'] as num?)?.toInt() != version) {
      throw const FormatException('unsupported mixer layer version');
    }
    final id = json['id'];
    if (id is! String || id.isEmpty || id == allLayerId) {
      throw const FormatException('invalid mixer layer id');
    }
    final rawName = json['name'];
    final name = rawName is String && rawName.trim().isNotEmpty
        ? rawName.trim()
        : 'Layer';
    // Unknown channel keys are kept here and ignored at resolve time, so a
    // layer survives desk reconfigurations without losing its order.
    final seen = <String>{};
    final keys = <String>[];
    final rawKeys = json['channelKeys'];
    if (rawKeys is List) {
      for (final entry in rawKeys) {
        if (entry is String && entry.isNotEmpty && seen.add(entry)) {
          keys.add(entry);
        }
      }
    }
    final rawDensity = json['stripsPerPage'];
    final density = rawDensity is num
        ? rawDensity.toInt().clamp(minStripsPerPage, maxStripsPerPage)
        : defaultStripsPerPage;
    return MixerLayer(
      id: id,
      name: name,
      channelKeys: List.unmodifiable(keys),
      stripsPerPage: density,
    );
  }

  static String encodeList(List<MixerLayer> layers) =>
      jsonEncode([for (final layer in layers) layer.toJson()]);

  /// Decodes a persisted layers list. Returns null when the payload is
  /// missing or corrupt (caller falls back to defaults); skips individual
  /// invalid entries so one bad layer never nukes the rest.
  static List<MixerLayer>? tryDecodeList(String? raw) {
    if (raw == null || raw.isEmpty) return null;
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! List) return null;
      final layers = <MixerLayer>[];
      final seenIds = <String>{};
      for (final entry in decoded) {
        if (entry is! Map) continue;
        try {
          final layer = MixerLayer.fromJson(entry);
          if (seenIds.add(layer.id)) layers.add(layer);
        } on FormatException {
          continue;
        }
      }
      return List.unmodifiable(layers);
    } catch (_) {
      return null;
    }
  }

  @override
  bool operator ==(Object other) =>
      other is MixerLayer &&
      id == other.id &&
      name == other.name &&
      _listEquals(channelKeys, other.channelKeys) &&
      stripsPerPage == other.stripsPerPage;

  @override
  int get hashCode => Object.hash(
      id, name, Object.hashAll(channelKeys), stripsPerPage);
}

/// Layers list plus the selected layer id, as read from prefs. An unknown
/// [activeLayerId] falls back to the built-in All layer.
class MixerLayersState {
  final List<MixerLayer> layers;
  final String activeLayerId;

  const MixerLayersState({
    required this.layers,
    required this.activeLayerId,
  });

  MixerLayer get activeLayer {
    for (final layer in layers) {
      if (layer.id == activeLayerId) return layer;
    }
    return MixerLayer.all();
  }

  bool get isCustomLayerActive => !activeLayer.isAll;
}

bool _listEquals(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}
