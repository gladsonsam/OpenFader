part of 'app_store.dart';

// Local processor preset storage, verified readback and ordered recalls.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStorePresetsContract {
  Map<String, dynamic>? _presetFamily(int channel);
  bool _presetEditingBlocked(int channel, String field);
  void _invalidatePresetOperations(String message);
  void _stopPresetRecalls(String message);
  Map<int, Set<String>> get _presetVerifiedFields;
  void _pollPresetRecalls(double now);
  bool _recordParameterAuthority(
      Map<String, dynamic> event, int channel, String field);
  void _confirmPresetValue(int channel, String field, Object value,
      {required bool verified});
  bool get _presetDisposed;
  int get _presetGeneration;
  ProcessorPresetLibrary get _processorPresets;
}

mixin _AppStorePresets on _AppStoreContext {
  // --- local user processor presets ---
  @override
  final ProcessorPresetLibrary _processorPresets = ProcessorPresetLibrary();
  // Display/optimistic state is intentionally separate from values that can
  // substantiate a stored preset or complete a recall. Legacy events without
  // provenance are unknown, never independent readback proof.
  @override
  final Map<int, Set<String>> _presetVerifiedFields = {};

  @override
  bool _recordParameterAuthority(
      Map<String, dynamic> event, int channel, String field) {
    final authority = event['authority'];
    final verified = event['type'] == 'param'
        ? authority == 'verified'
        : authority is Map && authority[field] == 'verified';
    if (field == 'color' || field == 'icon' || field == 'link') {
      final key = ps.pendingKey(channel, field);
      final edit = _appearanceEdits[key];
      final value = event['type'] == 'param'
          ? event['value']
          : (event['values'] as Map?)?[field];
      final provenance = event['type'] == 'param'
          ? authority
          : (authority is Map ? authority[field] : null);
      if (edit != null &&
          _nowMs() < edit.deadline &&
          (provenance == 'accepted' || provenance == 'verified') &&
          (edit.value == value ||
              (edit.value is num &&
                  value is num &&
                  ((edit.value as num) - value).abs() < .0001))) {
        _finishAppearanceEdit(
            key, _appearanceEditError(edit.target, edit.field));
      }
    }
    if (field == 'name') {
      final key = ps.pendingKey(channel, field);
      final value = event['type'] == 'param'
          ? event['value']
          : (event['values'] as Map?)?[field];
      final provenance = event['type'] == 'param'
          ? authority
          : (authority is Map ? authority[field] : null);
      if ((provenance == 'accepted' || provenance == 'verified') &&
          _nameEditValues[key] == value &&
          _nowMs() < (_nameEditDeadlines[key] ?? double.negativeInfinity)) {
        _finishNameEdit(key, null);
      }
    }
    final fields = _presetVerifiedFields.putIfAbsent(channel, () => {});
    if (verified) {
      fields.add(field);
    } else {
      fields.remove(field);
    }
    return verified;
  }

  final Map<(int, ProcessorPresetKind), _PresetRecall> _presetRecalls = {};
  final Expando<int> _presetTargets = Expando<int>();
  @override
  int _presetGeneration = 0;
  int _presetId = 0;
  @override
  bool _presetDisposed = false;

  @override
  Map<String, dynamic>? _presetFamily(int channel) {
    for (final family in engine.topology) {
      final first = family['first'];
      final count = family['count'];
      if (first is num &&
          count is num &&
          channel >= first &&
          channel < first + count) {
        return family;
      }
    }
    return null;
  }

  bool _presetFieldApplies(ProcessorPresetTarget target, String field) {
    final scopes = engine.parameterMetadata[field]?['scopes'];
    final family = _presetFamily(target.channel);
    final familyScopes = family?['params_scope'];
    final kinds = familyScopes is List ? familyScopes : [family?['kind']];
    return scopes is List && scopes.any(kinds.contains);
  }

  String? _presetCatalogueError(ProcessorPresetTarget target) {
    final family = _presetFamily(target.channel);
    final familyScopes = family?['params_scope'];
    final kinds = familyScopes is List ? familyScopes : [family?['kind']];
    if (kinds.isEmpty || kinds.any((kind) => kind is! String || kind.isEmpty)) {
      return 'The mixer has not provided this channel’s processor information.';
    }
    // Missing scope metadata must not silently remove a field from a save.
    // Properly scoped rows for other families (for example bands 5/6 on an
    // input) are excluded by _presetFields instead.
    for (final field in engine.supportedParams) {
      if (!processorPresetField(target.kind, field)) continue;
      final scopes = engine.parameterMetadata[field]?['scopes'];
      if (scopes is! List ||
          scopes.isEmpty ||
          scopes.any((scope) => scope is! String || scope.isEmpty)) {
        return 'The mixer has not provided complete ${target.kind.label} information.';
      }
    }
    return null;
  }

  List<String> _presetFields(ProcessorPresetTarget target) => [
        for (final field in engine.supportedParams)
          if (processorPresetField(target.kind, field) &&
              _presetFieldApplies(target, field))
            field,
      ];

  @override
  ProcessorPresetTarget? capturePresetTarget(ProcessorPresetKind kind) {
    if (kind.index > ProcessorPresetKind.compressor.index) return null;
    final family = _presetFamily(_channel)?['key'];
    if (family is! String || family.isEmpty) return null;
    for (final strip in engine.strips) {
      if (strip.ch != _channel) continue;
      final target = ProcessorPresetTarget(
          session: engine.session,
          channel: strip.ch,
          key: strip.key,
          driver: engine.driver,
          family: family,
          name: strip.name.isEmpty
              ? strip.key.replaceAll('/', '').toUpperCase()
              : strip.name,
          kind: kind);
      _presetTargets[target] = _presetGeneration;
      return presetTargetError(target) == null ? target : null;
    }
    return null;
  }

  @override
  String? presetTargetError(ProcessorPresetTarget target) {
    if (_presetDisposed ||
        _presetTargets[target] != _presetGeneration ||
        !engine.connected ||
        engine.session != target.session ||
        engine.driver != target.driver) {
      return 'Connection changed or lost. Reopen presets.';
    }
    if (!engine.capsReady ||
        _presetFamily(target.channel)?['key'] != target.family ||
        !engine.strips
            .any((s) => s.ch == target.channel && s.key == target.key)) {
      return 'The channel changed or is unavailable. Reopen presets.';
    }
    final catalogueError = _presetCatalogueError(target);
    if (catalogueError != null) return catalogueError;
    if (_presetFields(target).isEmpty) {
      return '${target.kind.label} presets are unavailable for this channel.';
    }
    return null;
  }

  String? _presetReadyError(ProcessorPresetTarget target) {
    final unavailable = presetTargetError(target);
    if (unavailable != null) return unavailable;
    if (_presetRecalls.containsKey((target.channel, target.kind))) {
      return 'Wait for the current ${target.kind.label} recall to finish.';
    }
    // Pending processor edits include bypass flags, even though they are never
    // stored. Reject before starting; held coalescer entries remain pending
    // until an echo or timeout and cannot race this recall.
    for (final key in _maps.pending.keys) {
      final edit = ps.splitKey(key);
      if (edit != null &&
          edit.ch == target.channel &&
          _presetProcessorField(target.kind, edit.id)) {
        return 'Wait for the current ${target.kind.label} changes to be confirmed.';
      }
    }
    return null;
  }

  bool _presetProcessorField(ProcessorPresetKind kind, String field) =>
      processorPresetField(kind, field) ||
      switch (kind) {
        ProcessorPresetKind.eq => field == 'eq_on' ||
            field == 'hpf_on' ||
            RegExp(r'^eq[1-8]_on$').hasMatch(field),
        ProcessorPresetKind.gate =>
          field == 'gate_on' || field == 'gate_filter_on',
        ProcessorPresetKind.compressor =>
          field == 'comp_on' || field == 'comp_filter_on',
        _ => false,
      };

  @override
  bool _presetEditingBlocked(int channel, String field) =>
      _presetRecalls.values.any((recall) =>
          recall.target.channel == channel &&
          _presetProcessorField(recall.target.kind, field));

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

  String? _presetValueError(
      ProcessorPresetTarget target, String field, Object? value,
      {bool requireVerified = false}) {
    if (!processorPresetField(target.kind, field) ||
        !engine.supportedParams.contains(field) ||
        !_presetFieldApplies(target, field)) {
      return 'The saved setting ${_presetSettingLabel(field)} is unavailable on this channel.';
    }
    if (!engine.valueKnown(target.channel, field) ||
        !ps.confirmedKnown(_maps, target.channel, field) ||
        (requireVerified &&
            !(_presetVerifiedFields[target.channel]?.contains(field) ??
                false))) {
      return 'Waiting for the mixer to provide ${_presetSettingLabel(field)}. Try again when it is ready.';
    }
    final metadata = engine.parameterMetadata[field];
    final kind = metadata?['kind'];
    if (kind == 'bool') {
      return value is bool
          ? null
          : 'The saved setting ${_presetSettingLabel(field)} has an invalid value.';
    }
    if ((kind != 'float' && kind != 'enum') ||
        value is! num ||
        !value.isFinite) {
      return 'The saved setting ${_presetSettingLabel(field)} has an invalid type.';
    }
    final min = metadata?['min'];
    final max = metadata?['max'];
    if (min is! num ||
        max is! num ||
        !min.isFinite ||
        !max.isFinite ||
        min > max ||
        value < min ||
        value > max ||
        ((kind == 'enum' ||
                field.endsWith('_type') ||
                field.endsWith('_mode')) &&
            value != value.roundToDouble())) {
      return 'The saved setting ${_presetSettingLabel(field)} is outside the mixer’s supported range.';
    }
    // DL32S exposes narrower raw binary domains than the canonical catalogue.
    // Preserve integers without assuming modern/vintage or soft/hard labels.
    if (target.driver == 'dl32s' &&
        const {'eq_mode', 'gate_mode', 'comp_mode', 'comp_knee'}
            .contains(field) &&
        value != 0 &&
        value != 1) {
      return 'The saved setting ${_presetSettingLabel(field)} must be 0 or 1 for this mixer.';
    }
    return null;
  }

  bool _presetCompatible(
          ProcessorPresetTarget target, ProcessorPreset preset) =>
      preset.driver == target.driver &&
      preset.family == target.family &&
      preset.kind == target.kind;

  @override
  Future<List<ProcessorPreset>> loadProcessorPresets(
      ProcessorPresetTarget target) async {
    final error = presetTargetError(target);
    if (error != null) throw StateError(error);
    final presets = await _processorPresets.load();
    final changed = presetTargetError(target);
    if (changed != null) throw StateError(changed);
    return List<ProcessorPreset>.unmodifiable(
        presets.where((preset) => _presetCompatible(target, preset)));
  }

  @override
  Future<String?> saveProcessorPreset(
      ProcessorPresetTarget target, String name) async {
    final error =
        _presetReadyError(target) ?? validateProcessorPresetName(name);
    if (error != null) return error;
    // Copy synchronously before the first await: neither I/O nor channel
    // navigation can substitute optimistic values or retarget this snapshot.
    final values = <String, Object>{};
    for (final field in _presetFields(target)) {
      final value = _maps.confirmed[target.channel]?[field];
      final invalid =
          _presetValueError(target, field, value, requireVerified: true);
      if (invalid != null) return invalid;
      values[field] = value!;
    }
    final now = DateTime.now();
    final preset = ProcessorPreset(
        id: '${now.microsecondsSinceEpoch}-${_presetId++}',
        name: name.trim(),
        driver: target.driver,
        family: target.family,
        kind: target.kind,
        values: values,
        createdAt: now,
        updatedAt: now);
    try {
      await _processorPresets.save(preset);
      final changed = presetTargetError(target);
      if (changed != null) return 'Preset saved on this device. $changed';
      notifyListeners();
      return null;
    } catch (error) {
      return 'Could not save preset: $error';
    }
  }

  Future<ProcessorPreset> _findProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    final presets = await loadProcessorPresets(target);
    for (final preset in presets) {
      if (preset.id == id) return preset;
    }
    throw StateError('This preset is no longer in the library.');
  }

  @override
  Future<String?> renameProcessorPreset(
      ProcessorPresetTarget target, String id, String name) async {
    try {
      await _findProcessorPreset(target, id);
      final error =
          presetTargetError(target) ?? validateProcessorPresetName(name);
      if (error != null) return error;
      await _processorPresets.rename(id, name);
      if (!_presetDisposed) notifyListeners();
      return null;
    } catch (error) {
      return 'Could not rename preset: $error';
    }
  }

  @override
  Future<String?> deleteProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    try {
      await _findProcessorPreset(target, id);
      final error = presetTargetError(target);
      if (error != null) return error;
      await _processorPresets.delete(id);
      if (!_presetDisposed) notifyListeners();
      return null;
    } catch (error) {
      return 'Could not delete preset: $error';
    }
  }

  @override
  void _stopPresetRecalls(String message) {
    cancelOutputPresetRecall();
    for (final recall in _presetRecalls.values) {
      recall.stop(message);
    }
  }

  @override
  void _invalidatePresetOperations(String message) {
    _presetGeneration++;
    _stopPresetRecalls(message);
  }

  Future<void> clearProcessorPresets() async {
    _invalidatePresetOperations(
        'The preset library was cleared. Reopen presets.');
    await _processorPresets.clear();
    if (!_presetDisposed) notifyListeners();
  }

  Future<int> processorPresetStorageBytes() async {
    final prefs = await SharedPreferences.getInstance();
    return utf8
        .encode(prefs.getString(kProcessorPresetStorageKey) ?? '')
        .length;
  }

  String? _presetRecallError(_PresetRecall recall) =>
      recall.error ??
      (recall.generation != _presetGeneration
          ? 'The preset operation was cancelled.'
          : presetTargetError(recall.target));

  @override
  void _pollPresetRecalls(double now) {
    for (final recall in _presetRecalls.values) {
      final error = _presetRecallError(recall);
      if (error != null) {
        recall.stop(error);
      } else if (recall.waiting case final waiting?) {
        if (!waiting.done.isCompleted && now >= waiting.deadline) {
          recall.stop(
              'The mixer did not confirm ${_presetSettingLabel(waiting.field)}. '
              'That setting may have changed; its result is uncertain.');
        }
      }
    }
  }

  @override
  void _confirmPresetValue(int channel, String field, Object value,
      {required bool verified}) {
    // A poll can start before expiry and return after it. Enforce the same
    // deadline at receipt, before any matching frame can complete the wait.
    _pollPresetRecalls(_nowMs());
    for (final recall in _presetRecalls.values) {
      final waiting = recall.waiting;
      if (recall.error != null ||
          recall.target.channel != channel ||
          waiting == null ||
          waiting.field != field ||
          !ps.valuesEqual(value, waiting.value)) {
        continue;
      }
      final error = _presetRecallError(recall);
      if (error != null) {
        recall.stop(error);
      } else {
        waiting.echoed = true;
        waiting.verified = verified;
        waiting.check();
      }
    }
  }

  int _presetFieldOrder(String field) {
    if (field.endsWith('_mode') || field == 'comp_auto') return 0;
    if (field.endsWith('_type') || field == 'hpf_slope') return 1;
    if (field.endsWith('_q')) return 3;
    return 2;
  }

  @override
  Future<ProcessorPresetResult> recallProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    final unavailable = _presetReadyError(target);
    if (unavailable != null) {
      return ProcessorPresetResult(error: unavailable, confirmed: 0, total: 0);
    }
    final recall = _PresetRecall(target, _presetGeneration);
    final slot = (target.channel, target.kind);
    _presetRecalls[slot] = recall;
    for (final field in engine.supportedParams) {
      if (_presetProcessorField(target.kind, field)) {
        _coalescer.cancel('param:${target.channel}:$field');
      }
    }
    notifyListeners();
    var confirmed = 0;
    var acceptedUnverified = 0;
    var total = 0;
    var attempted = 0;
    final skipped = <String>[];
    try {
      final preset = await Future.any<ProcessorPreset>([
        _findProcessorPreset(target, id),
        recall.stopped.future.then((error) => throw StateError(error)),
      ]);
      total = preset.values.length;
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
        final waiting =
            _PresetExpectation(field, value, _nowMs() + ps.confirmTimeoutMs);
        recall.waiting = waiting;
        attempted++;
        ps.noteOptimistic(_maps, target.channel, field, value, _nowMs());
        // Register expectation before dispatch, including synchronous throws.
        unawaited(Future<void>.sync(() =>
                _api.cmd(_clientId, Cmds.param(target.channel, field, value)))
            .then((_) {
          // Submission may finish after the deadline even when the matching
          // frame arrived earlier. Neither half can complete a late recall.
          _pollPresetRecalls(_nowMs());
          if (!identical(recall.waiting, waiting) || recall.error != null) {
            return;
          }
          final changed = _presetRecallError(recall);
          if (changed != null) {
            recall.stop(changed);
          } else {
            waiting.accepted = true;
            waiting.check();
          }
        }, onError: (Object error) {
          if (identical(recall.waiting, waiting)) {
            recall.stop(
                'Could not send ${_presetSettingLabel(field)}: $error. Its result is uncertain.');
          }
        }));
        error = await waiting.done.future;
        if (error != null) throw StateError(error);
        if (waiting.verified) {
          confirmed++;
        } else {
          acceptedUnverified++;
        }
        recall.waiting = null;
        error = _presetRecallError(recall);
        if (error != null) throw StateError(error);
      }
      return ProcessorPresetResult(
          error: acceptedUnverified == 0 && skipped.isEmpty
              ? null
              : [
                  if (acceptedUnverified > 0)
                    '$acceptedUnverified settings were accepted, but the mixer did not verify their values.',
                  if (skipped.isNotEmpty)
                    'Inactive Q settings were kept unchanged: ${skipped.map(_presetSettingLabel).join(', ')}. '
                        'Their saved values remain in the preset.',
                ].join(' '),
          confirmed: confirmed,
          total: total);
    } catch (error) {
      final notSent = total - attempted;
      return ProcessorPresetResult(
          error: '$error '
              '${acceptedUnverified > 0 ? '$acceptedUnverified settings were accepted without value verification. ' : ''}'
              '${notSent > 0 ? '$notSent settings were not sent. ' : ''}'
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
        notifyListeners();
      }
    }
  }
}

class _PresetRecall {
  _PresetRecall(this.target, this.generation);
  final ProcessorPresetTarget target;
  final int generation;
  final stopped = Completer<String>();
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
