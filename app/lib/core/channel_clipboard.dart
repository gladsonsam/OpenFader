import 'package:flutter/foundation.dart';

import 'channel_name.dart';
import 'processor_presets.dart';

/// Bounded channel DSP scope. This deliberately excludes identity, trim,
/// polarity, preamps/48V, sources, levels, pan, sends and assignments.
/// Enable flags belong to channel processing, but never to processor presets.
bool channelProcessingField(String field) =>
    const [
      ProcessorPresetKind.eq,
      ProcessorPresetKind.gate,
      ProcessorPresetKind.compressor,
    ].any((k) => processorPresetField(k, field)) ||
    const {
      'eq_on',
      'hpf_on',
      'gate_on',
      'gate_filter_on',
      'comp_on',
      'comp_filter_on',
    }.contains(field) ||
    RegExp(r'^eq[1-8]_on$').hasMatch(field);

const channelProcessingScope =
    'EQ / high-pass filter, gate and compressor settings, including enable flags, '
    'where advertised for this channel family. Inactive DL32S shelf Q stays unchanged.';
const channelProcessingExclusions =
    'Identity (name, icon, color), trim, polarity, mic preamp gain / 48V, source '
    'routing, stereo link, levels, pan, mute / solo, sends and assignments stay unchanged. '
    'Paste All is unavailable: the provider has no complete guarded channel contract.';

/// Captured once when the footer opens. Navigation never substitutes a channel.
class ChannelProcessingTarget {
  final ChannelNameTarget identity;
  final List<ProcessorPresetTarget> blocks;
  ChannelProcessingTarget(this.identity, List<ProcessorPresetTarget> blocks)
      : blocks = List.unmodifiable(blocks);
}

/// Opaque, store-owned proposal. Its destination block and clipboard are frozen
/// before confirmation. Reusing a proposal is rejected even after a failure.
class ChannelProcessingPaste {
  final String source, destination;
  final int settings;
  const ChannelProcessingPaste(this.source, this.destination, this.settings);
}

abstract class ChannelClipboardStore implements Listenable {
  ChannelProcessingTarget? captureChannelProcessingTarget(
    ChannelNameTarget target,
  );
  String? channelProcessingError(
    ChannelProcessingTarget target, {
    bool paste = false,
  });
  Future<String?> copyChannelProcessing(ChannelProcessingTarget target);
  Future<ChannelProcessingPaste> prepareChannelProcessingPaste(
    ChannelProcessingTarget target,
  );
  Future<ProcessorPresetResult> pasteChannelProcessing(
    ChannelProcessingPaste proposal,
  );
  void cancelChannelProcessing();
}
