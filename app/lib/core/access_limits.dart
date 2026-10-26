/// Device-local UI limits. These supplement engine policy; they are not remote
/// authentication. Keys describe control families/targets, never driver names.
enum AccessFamily {
  inputDsp(
    'Input DSP',
    'Input EQ, filters, gate and compressor edits and recalls',
  ),
  masterDsp('Master DSP', 'Output processor controls and recalls'),
  channelMute(
    'Channel mute',
    'Input/return strip mute controls (send and master mutes remain independent)',
  ),
  channelSolo('Channel solo', 'Strip solo and clear solo'),
  pan('Pan', 'Strip pan and send pan'),
  channelId('Channel ID', 'Channel names, colors and icons'),
  fx(
    'FX globals',
    'FX sends and global parameters; returns remain independent',
  );

  const AccessFamily(this.label, this.coverage);
  final String label;
  final String coverage;
  String get key => 'family:$name';
}

class AccessLimitOption {
  const AccessLimitOption(
    this.key,
    this.label,
    this.coverage, {
    this.available = true,
  });
  final String key;
  final String label;
  final String coverage;
  final bool available;
}

abstract interface class AccessLimitsStore {
  Set<String> get blockedAccess;
  List<AccessLimitOption> get accessOptions;
  Future<void> setAccessBlocked(String key, bool blocked);
}
