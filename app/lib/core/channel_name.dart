import 'package:flutter/foundation.dart';

/// Captured identity; selection changes must never retarget an open editor.
class ChannelNameTarget {
  final int session, channel;
  final String key, name;
  const ChannelNameTarget(this.session, this.channel, this.key, this.name);
}

class ChannelColorChoice {
  final double value;
  final String label;
  final int? rgb;
  const ChannelColorChoice(this.value, this.label, this.rgb);
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

  /// Null only after a matching backend echo. Errors remain visible in editor.
  Future<String?> renameChannel(ChannelNameTarget target, String name);
  List<ChannelColorChoice> colorsForChannel(ChannelNameTarget target);
  double? colorForChannel(ChannelNameTarget target);
  String? colorEditError(ChannelNameTarget target);

  /// Null after the backend accepts the write; this does not claim readback.
  Future<String?> colorChannel(ChannelNameTarget target, double value);
}
