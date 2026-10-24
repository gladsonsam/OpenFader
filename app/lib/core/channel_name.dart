import 'package:flutter/foundation.dart';

/// Captured identity; selection changes must never retarget an open editor.
class ChannelNameTarget {
  final int session, channel;
  final String key, name;
  const ChannelNameTarget(this.session, this.channel, this.key, this.name);
}

abstract class ChannelNameStore implements Listenable {
  ChannelNameTarget? captureNameTarget([int? channel]);
  String? nameEditError(ChannelNameTarget target);

  /// Null only after a matching backend echo. Errors remain visible in editor.
  Future<String?> renameChannel(ChannelNameTarget target, String name);
}
