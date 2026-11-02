part of 'app_store.dart';

class _ChannelProcessingCopy {
  final String driver, family, domain, nativeDomain, name;
  final ChannelNameTarget source;
  final int epoch;
  final Map<String, Object> values;
  _ChannelProcessingCopy(
    this.driver,
    this.family,
    this.domain,
    this.nativeDomain,
    this.name,
    this.source,
    this.epoch,
    Map<String, Object> values,
  ) : values = Map.unmodifiable(values);
}

class _ChannelProcessingPlan {
  final ChannelProcessingTarget target;
  final _ChannelProcessingCopy copy;
  final Map<String, dynamic> snapshot;
  _ChannelProcessingPlan(this.target, this.copy, this.snapshot);
}

/// Composes the existing strict provider lane into one complete DSP block.
/// No file library or per-processor clipboard state is involved.
extension _ChannelClipboard on _AppStorePresets {
  List<String> _channelProcessingFields(ProcessorPresetTarget target) => [
        for (final field in engine.supportedParams)
          if (channelProcessingField(field) &&
              _presetFieldApplies(target, field))
            field,
      ];

  String _channelProcessingDomain(ChannelProcessingTarget target) =>
      jsonEncode({
        for (final f in (_channelProcessingFields(target.blocks.first)..sort()))
          f: engine.parameterMetadata[f],
      });

  ChannelProcessingTarget? _captureChannelProcessingTarget(
    ChannelNameTarget target,
  ) {
    if (!const {'in', 'ret', 'fxret', 'return'}
        .contains(_presetFamily(target.channel)?['kind'])) {
      return null;
    }
    final blocks = <ProcessorPresetTarget>[];
    for (final kind in [
      ProcessorPresetKind.eq,
      ProcessorPresetKind.gate,
      ProcessorPresetKind.compressor,
    ]) {
      final block = captureChannelPresetTarget(
        kind,
        target.channel,
        session: target.session,
        key: target.key,
        accessEpoch: target.accessEpoch,
      );
      if (block != null && _presetFields(block).isNotEmpty) blocks.add(block);
    }
    if (blocks.isEmpty) return null;
    return ChannelProcessingTarget(target, blocks);
  }

  String? _channelProcessingError(
    ChannelProcessingTarget target, {
    bool paste = false,
  }) {
    if (!engine.channelProcessingGuard) {
      return 'This provider does not advertise guarded channel DSP copy / paste. Use its supported processor libraries.';
    }
    if (target.identity.accessEpoch != controlAccessEpoch) {
      return 'Access changed. Reopen the channel popup.';
    }
    if (_channelRecall != null ||
        _presetRecalls.values.any(
          (r) => r.target.channel == target.identity.channel,
        )) {
      return 'Wait for channel processing to finish.';
    }
    return _channelProcessingReady(target, paste: paste);
  }

  String? _channelProcessingReady(
    ChannelProcessingTarget target, {
    bool paste = false,
  }) {
    if (!engine.channelProcessingGuard ||
        target.identity.accessEpoch != controlAccessEpoch) {
      return 'Channel processing support or access changed. Reopen the popup.';
    }
    for (final block in target.blocks) {
      final error = presetTargetError(block);
      if (error != null) return error;
    }
    final anchor = target.blocks.first;
    final fields = _channelProcessingFields(anchor);
    if (fields.isEmpty) return 'No channel processing fields were advertised.';
    final pair =
        engine.stereoPairs.where((p) => p.contains(anchor.channel)).firstOrNull;
    for (final field in fields) {
      for (final ch in pair ?? [anchor.channel]) {
        if (!_accessParamAllowed(ch, field)) {
          return 'Channel processing or its partner is limited on this device.';
        }
      }
      if (_maps.pending.containsKey(ps.pendingKey(anchor.channel, field))) {
        return 'Wait for all channel processing changes to be confirmed.';
      }
      if (_presetVerifiedFields[anchor.channel]?.contains(field) != true ||
          !ps.confirmedKnown(_maps, anchor.channel, field)) {
        return 'Waiting for verified ${_presetSettingLabel(field)} readback. Open channel processing, then reopen this popup.';
      }
    }
    if (paste) {
      final copy = _channelClipboard;
      if (copy == null) return 'Copy channel processing first.';
      if (copy.driver != anchor.driver ||
          copy.family != anchor.family ||
          copy.domain != _channelProcessingDomain(target)) {
        return 'Copy and destination must have the same provider, channel family and complete field domains.';
      }
    }
    return null;
  }

  _PresetRecall _beginChannelProcessing(ChannelProcessingTarget target) {
    final error = _channelProcessingError(target);
    if (error != null) throw StateError(error);
    final recall = _PresetRecall(
      target.blocks.first,
      _presetGeneration,
      processing: true,
    );
    _channelRecall = recall;
    _presetRecalls[(recall.target.channel, recall.target.kind)] = recall;
    for (final field in _channelProcessingFields(recall.target)) {
      _coalescer.cancel('param:${recall.target.channel}:$field');
    }
    _notifyPresetRecall();
    return recall;
  }

  void _endChannelProcessing(_PresetRecall recall) {
    final slot = (recall.target.channel, recall.target.kind);
    if (identical(_presetRecalls[slot], recall)) _presetRecalls.remove(slot);
    if (identical(_channelRecall, recall)) _channelRecall = null;
    if (!_presetDisposed) _notifyPresetRecall();
  }

  Future<String?> _copyChannelProcessing(ChannelProcessingTarget target) async {
    _PresetRecall? recall;
    try {
      recall = _beginChannelProcessing(target);
      final snapshot = await _inputProcessorStep(recall);
      final error = _channelProcessingReady(target);
      if (error != null) throw StateError(error);
      _validateInputSnapshot(recall.target, snapshot, processing: true);
      final values = <String, Object>{
        for (final row in snapshot['rows'] as List)
          row['id'] as String: row['value'] as Object,
      };
      _validateChannelProcessingValues(target, values);
      _channelClipboard = _ChannelProcessingCopy(
        recall.target.driver,
        recall.target.family,
        _channelProcessingDomain(target),
        jsonEncode(snapshot['domain']),
        recall.target.name,
        target.identity,
        engine.syncEpoch!,
        values,
      );
      return null;
    } catch (error) {
      return 'Channel processing was not copied: $error';
    } finally {
      if (recall != null) _endChannelProcessing(recall);
    }
  }

  void _validateChannelProcessingValues(
    ChannelProcessingTarget target,
    Map<String, Object> values,
  ) {
    final fields = _channelProcessingFields(target.blocks.first);
    if (values.length != fields.length ||
        fields.any((f) => !values.containsKey(f))) {
      throw StateError(
        'The clipboard must contain the entire advertised channel DSP block.',
      );
    }
    for (final entry in values.entries) {
      final block = target.blocks
          .where((b) => processorPresetField(b.kind, entry.key))
          .firstOrNull;
      if (block != null) {
        final error = _presetValueError(block, entry.key, entry.value);
        if (error != null) throw StateError(error);
      } else if (!channelProcessingField(entry.key) ||
          engine.parameterMetadata[entry.key]?['kind'] != 'bool' ||
          entry.value is! bool) {
        throw StateError(
          'Unsupported channel processing setting ${entry.key}.',
        );
      }
    }
  }

  Future<ChannelProcessingPaste> _prepareChannelProcessingPaste(
    ChannelProcessingTarget target,
  ) async {
    final error = _channelProcessingError(target, paste: true);
    if (error != null) throw StateError(error);
    final copy = _channelClipboard!;
    _validateChannelProcessingValues(target, copy.values);
    final recall = _beginChannelProcessing(target);
    try {
      final snapshot = await _inputProcessorStep(recall);
      _validateInputSnapshot(recall.target, snapshot, processing: true);
      if (copy.nativeDomain != jsonEncode(snapshot['domain'])) {
        throw StateError('Source and destination native DSP domains differ.');
      }
      final stale = _channelProcessingReady(target);
      if (stale != null) throw StateError(stale);
      final proposal = ChannelProcessingPaste(
        '${copy.name} (${copy.source.key})',
        '${recall.target.name} (${recall.target.key})',
        copy.values.length,
      );
      _channelPlans[proposal] = _ChannelProcessingPlan(
        target,
        copy,
        jsonDecode(jsonEncode(snapshot)) as Map<String, dynamic>,
      );
      return proposal;
    } finally {
      _endChannelProcessing(recall);
    }
  }

  void _cancelChannelProcessing() {
    _channelRecall?.stop(
      'Channel processing cancelled. An in-flight setting may have changed; result uncertain. No further settings will be sent.',
    );
  }

  Future<ProcessorPresetResult> _pasteChannelProcessing(
    ChannelProcessingPaste proposal,
  ) async {
    final plan = _channelPlans[proposal];
    _channelPlans[proposal] = null;
    var confirmed = 0, attempted = 0;
    final total = proposal.settings;
    _PresetRecall? recall;
    final skipped = <String>[];
    try {
      if (plan == null) {
        throw StateError(
            'This proposal expired or was already used. Review a fresh proposal.');
      }
      final target = plan.target;
      final copy = plan.copy;
      final ready = _channelProcessingError(target);
      if (ready != null) throw StateError(ready);
      if (copy.driver != engine.driver ||
          copy.family != target.blocks.first.family ||
          copy.domain != _channelProcessingDomain(target)) {
        throw StateError('Channel DSP domains changed. Copy again.');
      }
      _validateChannelProcessingValues(target, copy.values);
      recall = _beginChannelProcessing(target);
      // Validate the exact destination captured BEFORE confirmation, including
      // every sibling/bypass revision and both link words, before any write.
      var snapshot = await _inputProcessorStep(recall, snapshot: plan.snapshot);
      final fields = copy.values.keys.toList()
        ..sort((a, b) {
          final order = (a.endsWith('_on') ? 4 : _presetFieldOrder(a))
              .compareTo(b.endsWith('_on') ? 4 : _presetFieldOrder(b));
          return order == 0 ? a.compareTo(b) : order;
        });
      for (final field in fields) {
        final error =
            _channelProcessingReady(target) ?? _presetRecallError(recall);
        if (error != null) throw StateError(error);
        if (mixerProviderPolicy(copy.driver)
            .skipRecalledField(field, copy.values)) {
          skipped.add(field);
          continue;
        }
        final row = (snapshot['rows'] as List).firstWhere(
          (r) => r['id'] == field,
        );
        final value = copy.values[field]!;
        // These are captured provider values, not optimistic slider echoes.
        // The UI echo tolerance can hide distinct native DSP values and must
        // never turn a required write into a falsely verified unchanged row.
        final unchanged = row['value'] == value;
        if (!unchanged) attempted++;
        snapshot = await _inputProcessorStep(
          recall,
          snapshot: snapshot,
          field: unchanged ? null : field,
          value: unchanged ? null : value,
        );
        _validateInputSnapshot(recall.target, snapshot, processing: true);
        confirmed++;
      }
      final finalError =
          _channelProcessingReady(target) ?? _presetRecallError(recall);
      if (finalError != null) throw StateError(finalError);
      return ProcessorPresetResult(
        error: skipped.isEmpty
            ? null
            : 'Inactive shelf Q kept unchanged: ${skipped.join(', ')}. $confirmed of $total settings verified.',
        confirmed: confirmed,
        total: total,
      );
    } catch (error) {
      return ProcessorPresetResult(
        error:
            '$error $attempted writes attempted; $confirmed of $total settings verified. '
            'Remaining settings are unverified. No changes were rolled back; no reconnect replay.',
        confirmed: confirmed,
        total: total,
      );
    } finally {
      if (recall != null) _endChannelProcessing(recall);
    }
  }
}
