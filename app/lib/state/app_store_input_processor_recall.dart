part of 'app_store.dart';

extension _InputProcessorRecall on _AppStorePresets {
  int _presetFieldOrder(String field) {
    if (field.endsWith('_mode') || field == 'comp_auto') return 0;
    if (field.endsWith('_type') || field == 'hpf_slope') return 1;
    if (field.endsWith('_q')) return 3;
    return 2;
  }

  Future<ProcessorPresetResult> _recallInputProcessor(
    ProcessorPresetTarget target, {
    String? id,
    ProcessorPreset? clipboard,
  }) async {
    final unavailable = _presetReadyError(target);
    if (unavailable != null) {
      return ProcessorPresetResult(error: unavailable, confirmed: 0, total: 0);
    }
    final recall = _PresetRecall(target, _presetGeneration);
    recall.observed.addAll({
      for (final entry
          in (_maps.confirmed[target.channel] ?? <String, Object>{}).entries)
        if (entry.key == 'link' ||
            _presetProcessorField(target.kind, entry.key))
          entry.key: entry.value,
    });
    final slot = (target.channel, target.kind);
    _presetRecalls[slot] = recall;
    for (final field in engine.supportedParams) {
      if (_presetProcessorField(target.kind, field)) {
        _coalescer.cancel('param:${target.channel}:$field');
      }
    }
    _notifyPresetRecall();
    var confirmed = 0;
    var total = 0;
    var attempted = 0;
    final skipped = <String>[];
    try {
      var snapshot = await _inputProcessorStep(recall);
      _validateInputSnapshot(target, snapshot);
      final preset = await Future.any<ProcessorPreset>([
        clipboard != null
            ? Future.value(clipboard)
            : _findProcessorPreset(target, id!),
        recall.stopped.future.then((error) => throw StateError(error)),
      ]);
      total = preset.values.length;
      if (!_presetCompatible(target, preset)) {
        throw StateError('Copy a compatible input processor first.');
      }
      var error = _presetRecallError(recall);
      if (error != null) throw StateError(error);
      final fields = _presetFields(target);
      if (fields.any((field) => !preset.values.containsKey(field))) {
        throw StateError(
            'This preset does not contain every setting required by this processor.');
      }
      // Validate EVERY saved field before issuing a single command. Saved
      // values are never silently clamped or dropped when capabilities differ.
      for (final entry in preset.values.entries) {
        error = _presetValueError(target, entry.key, entry.value);
        if (error != null) throw StateError(error);
      }
      for (final key in _maps.pending.keys) {
        final edit = ps.splitKey(key);
        if (edit != null &&
            edit.ch == target.channel &&
            _presetProcessorField(target.kind, edit.id)) {
          throw StateError(
              'Wait for the current processor changes to be confirmed.');
        }
      }
      final ordered = preset.values.keys.toList()
        ..sort((a, b) {
          final order = _presetFieldOrder(a).compareTo(_presetFieldOrder(b));
          return order == 0 ? a.compareTo(b) : order;
        });
      for (final field in ordered) {
        error = _presetRecallError(recall);
        if (error != null) throw StateError(error);
        error = _presetValueError(target, field, preset.values[field]);
        if (error != null) throw StateError(error);
        final band = RegExp(r'^eq([1-4])_q$').firstMatch(field);
        if (target.driver == 'dl32s' &&
            band != null &&
            preset.values['eq${band[1]}_type'] != 2) {
          // DL32S Q is inactive outside bell mode. Keep the saved Q in the
          // library and explicitly report it as not recalled.
          skipped.add(field);
          continue;
        }
        // Cancel any held callback defensively; readiness already rejected
        // unconfirmed edits before taking this processor's write lock.
        _coalescer.cancel('param:${target.channel}:$field');
        final value = preset.values[field]!;
        final row = (snapshot['rows'] as List).firstWhere((r) => r['id'] == field);
        if (ps.valuesEqual(row['value'], value)) {
          // A no-op is verified by the captured provider state. Revalidate
          // its whole block without writing an unchanged desk setting.
          snapshot = await _inputProcessorStep(recall, snapshot: snapshot);
        } else {
          attempted++;
          snapshot = await _inputProcessorStep(recall,
              snapshot: snapshot, field: field, value: value);
        }
        _validateInputSnapshot(target, snapshot);
        confirmed++;
        error = _presetRecallError(recall);
        if (error != null) throw StateError(error);
      }
      return ProcessorPresetResult(
          error: skipped.isEmpty
              ? null
              : [
                  if (skipped.isNotEmpty)
                    'Inactive Q settings were kept unchanged: ${skipped.map(_presetSettingLabel).join(', ')}. '
                        'Their saved values remain in the preset.',
                ].join(' '),
          confirmed: confirmed,
          total: total);
    } catch (error) {
      final notVerified = total - confirmed;
      return ProcessorPresetResult(
          error: '$error '
              '$attempted writes attempted; $confirmed of $total settings verified. '
              '${notVerified > 0 ? '$notVerified settings remain unverified. ' : ''}'
              'No changes were rolled back.',
          confirmed: confirmed,
          total: total);
    } finally {
      final waiting = recall.waiting;
      if (waiting != null) {
        _maps.pending.remove(ps.pendingKey(target.channel, waiting.field));
      }
      if (identical(_presetRecalls[slot], recall)) _presetRecalls.remove(slot);
      if (!_presetDisposed) {
        _invalidateDerived();
        _notifyPresetRecall();
      }
    }
  }
}
