import 'package:flutter/foundation.dart';

/// Captured identity; selection changes must never retarget an open editor.
class ChannelNameTarget {
  final int session, channel;
  final String key;

  /// Actual parameter value; null means it was not available at capture.
  final String? name;
  const ChannelNameTarget(this.session, this.channel, this.key, this.name);
}

class ChannelColorChoice {
  final double value;
  final String label;
  final int? rgb;
  const ChannelColorChoice(this.value, this.label, this.rgb);
}

class ChannelIconChoice {
  final double value;
  final String label, symbol;
  const ChannelIconChoice(this.value, this.label, this.symbol);
}

List<ChannelIconChoice> channelIconChoices(Map<String, dynamic>? metadata) {
  final presentations = metadata?['presentations'];
  final rows = metadata?['symbols'] ??
      (presentations is List
          ? presentations
              .whereType<Map>()
              .map((p) => p['symbols'])
              .whereType<List>()
              .firstOrNull
          : null);
  if (rows is! List) return const [];
  return [
    for (final row in rows)
      if (row is Map &&
          row['value'] is num &&
          (row['value'] as num).isFinite &&
          row['label'] is String &&
          row['symbol'] is String &&
          (row['symbol'] as String).isNotEmpty)
        ChannelIconChoice((row['value'] as num).toDouble(),
            row['label'] as String, row['symbol'] as String),
  ];
}

List<ChannelColorChoice> channelColorChoices(Map<String, dynamic>? metadata) {
  final presentations = metadata?['presentations'];
  final rows = metadata?['swatches'] ??
      (presentations is List
          ? presentations
              .whereType<Map>()
              .map((p) => p['swatches'])
              .whereType<List>()
              .firstOrNull
          : null);
  if (rows is! List) return const [];
  return [
    for (final row in rows)
      if (row is Map &&
          row['value'] is num &&
          (row['value'] as num).isFinite &&
          row['label'] is String &&
          (row['rgb'] == null ||
              (row['rgb'] is int &&
                  (row['rgb'] as int) >= 0 &&
                  (row['rgb'] as int) <= 0xffffff)))
        ChannelColorChoice((row['value'] as num).toDouble(),
            row['label'] as String, row['rgb'] as int?),
  ];
}

abstract class ChannelNameStore implements Listenable {
  ChannelNameTarget? captureNameTarget([int? channel]);
  String? nameEditError(ChannelNameTarget target);

  /// Null after a matching accepted/verified receipt. Readback authority stays separate.
  Future<String?> renameChannel(ChannelNameTarget target, String name);
  bool nameCanBeEmpty(ChannelNameTarget target);
  List<ChannelColorChoice> colorsForChannel(ChannelNameTarget target);
  double? colorForChannel(ChannelNameTarget target);
  String? colorEditError(ChannelNameTarget target);

  /// Null after the backend accepts the write; this does not claim readback.
  Future<String?> colorChannel(ChannelNameTarget target, double value);
  List<ChannelIconChoice> iconsForChannel(ChannelNameTarget target);
  double? iconForChannel(ChannelNameTarget target);
  String? iconEditError(ChannelNameTarget target);
  Future<String?> iconChannel(ChannelNameTarget target, double value);
  int? linkPartnerForChannel(ChannelNameTarget target);
  bool? linkedForChannel(ChannelNameTarget target);
  String? linkEditError(ChannelNameTarget target);
  Future<String?> linkChannel(ChannelNameTarget target, bool linked);
}
