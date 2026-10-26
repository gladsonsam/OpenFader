part of 'app_store.dart';

class _PresetRecall {
  _PresetRecall(this.target, this.generation);
  final ProcessorPresetTarget target;
  final int generation;
  final stopped = Completer<String>();
  final Map<String, Object> observed = {};
  _PresetExpectation? waiting;
  String? error;
  void stop(String message) {
    error ??= message;
    if (!stopped.isCompleted) stopped.complete(error!);
    waiting?.finish(error);
  }
}

class _PresetExpectation {
  _PresetExpectation(this.field, this.value, this.deadline);
  final String field;
  final Object value;
  final double deadline;
  final done = Completer<String?>();
  bool accepted = false;
  bool echoed = false;
  bool verified = false;
  void finish(String? error) {
    if (!done.isCompleted) done.complete(error);
  }

  void check() {
    if (accepted && echoed) finish(null);
  }
}

// Input snapshots retain provider-specific primitive values and use the input
// recall plane. Output descriptor guards and clipboard remain independent.
extension _InputPresetGuards on _AppStorePresets {
  ProcessorPreset _captureInputProcessor(
    ProcessorPresetTarget target,
    String name,
  ) {
    final error =
        _presetReadyError(target) ?? validateProcessorPresetName(name);
    if (error != null) throw StateError(error);
    // Copy synchronously before the first await: neither I/O nor channel
    // navigation can substitute optimistic values or retarget this snapshot.
    final values = <String, Object>{};
    for (final field in _presetFields(target)) {
      final value = _maps.confirmed[target.channel]?[field];
      final invalid =
          _presetValueError(target, field, value, requireVerified: true);
      if (invalid != null) throw StateError(invalid);
      values[field] = value!;
    }
    final now = DateTime.now();
    return ProcessorPreset(
        id: '${now.microsecondsSinceEpoch}-${_presetId++}',
        name: name.trim(),
        driver: target.driver,
        family: target.family,
        kind: target.kind,
        values: values,
        createdAt: now,
        updatedAt: now);
  }

  String _inputFieldDomain(ProcessorPresetTarget target) => jsonEncode({
    for (final f in (_presetFields(target)..sort())) f: engine.parameterMetadata[f],
  });

  String? _inputClipboardReadyError(ProcessorPresetTarget target) {
    final error = _presetReadyError(target);
    if (error != null) return error;
    if (_presetFamily(target.channel)?['kind'] != 'in') {
      return 'Input processor copy and paste are unavailable on this channel.';
    }
    for (final field in _presetFields(target)) {
      final invalid = _presetValueError(target, field,
          _maps.confirmed[target.channel]?[field], requireVerified: true);
      if (invalid != null) return invalid;
    }
    return null;
  }

  String? _inputPresetProtection(ProcessorPresetTarget target) {
    final pair =
        engine.stereoPairs.where((p) => p.contains(target.channel)).firstOrNull;
    for (final ch in pair ?? [target.channel]) {
      if (engine.locks.containsKey(ch)) {
        return 'This channel or its stereo partner is locked.';
      }
      if (_maps.pending.containsKey(ps.pendingKey(ch, 'link'))) {
        return 'Wait for stereo link changes to be confirmed.';
      }
      if (_presetFields(target).any((f) => !_accessParamAllowed(ch, f))) {
        return 'This processor or its stereo partner is limited on this device.';
      }
      // Shared processing cannot be safely snapshotted through the legacy
      // parameter plane. Do not infer an unlinked state from missing evidence.
      if (pair != null &&
          (engine.params[ch]?['link'] != false ||
              _presetVerifiedFields[ch]?.contains('link') != true)) {
        return 'Input presets require verified unlinked channels. Reopen after mixer readback.';
      }
    }
    return null;
  }

  void _observeInputPresetEvent(
    Map<String, dynamic> event,
    int channel,
    String field,
  ) {
    final value = event['type'] == 'param'
        ? event['value']
        : (event['values'] as Map?)?[field];
    for (final recall in _presetRecalls.values) {
      if (recall.target.channel != channel ||
          !(field == 'link' ||
              _presetProcessorField(recall.target.kind, field))) {
        continue;
      }
      final previous = recall.observed[field];
      if (previous != null && !ps.valuesEqual(previous, value)) {
        final waiting = recall.waiting;
        if (waiting == null ||
            waiting.field != field ||
            !ps.valuesEqual(waiting.value, value)) {
          recall.stop(
            'Processor changed during recall. No further settings were sent; result uncertain.',
          );
        }
      }
      if (value != null) recall.observed[field] = value;
    }
  }
}

extension _InputPresetLabels on _AppStoreContext {
  String _presetSettingLabel(String field) {
    final band = RegExp(r'^eq([1-8])_(freq|gain|q|type)$').firstMatch(field);
    if (band != null) {
      final parameter = switch (band[2]) {
        'freq' => 'frequency',
        'gain' => 'gain',
        'q' => 'Q',
        _ => 'shape',
      };
      return 'EQ band ${band[1]} $parameter';
    }
    if (field == 'eq_mode') return 'EQ mode';
    if (field == 'hpf_freq') return 'high-pass frequency';
    if (field == 'hpf_slope') return 'high-pass slope';
    final dynamics = RegExp(r'^(gate|comp)_(.+)$').firstMatch(field);
    if (dynamics != null) {
      final processor = dynamics[1] == 'gate' ? 'gate' : 'compressor';
      final setting = switch (dynamics[2]) {
        'thr' => 'threshold',
        'att' => 'attack',
        'rel' => 'release',
        'det' => 'detector',
        'env' => 'envelope',
        'key_src' => 'key source',
        'filter_type' => 'filter shape',
        'filter_freq' => 'filter frequency',
        'auto' => 'automatic timing',
        _ => dynamics[2]!,
      };
      return '$processor $setting';
    }
    final label = engine.parameterMetadata[field]?['label'];
    return label is String && label.isNotEmpty
        ? label
        : field.replaceAll('_', ' ');
  }
}
