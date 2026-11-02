part of 'app_store.dart';

// Local processor preset storage, verified readback and ordered recalls.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStorePresetsContract {
  Map<String, dynamic>? _presetFamily(int channel);
  void _receiveInputProcessorResult(Map<String, dynamic> frame);
  bool _presetEditingBlocked(int channel, String field);
  void _invalidatePresetOperations(String message);
  void _stopPresetRecalls(String message);
  Map<int, Set<String>> get _presetVerifiedFields;
  void _pollPresetRecalls(double now);
  void _observeInputPresetCatalogue();
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
    _observeInputPresetEvent(event, channel, field);
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
  final Expando<int> _presetDomains = Expando<int>();
  String? _inputCatalogue;
  int _inputCatalogueRevision = 0;
  _ChannelProcessingCopy? _channelClipboard;
  _PresetRecall? _channelRecall;
  final Expando<_ChannelProcessingPlan> _channelPlans =
      Expando<_ChannelProcessingPlan>();

  @override
  ChannelProcessingTarget? captureChannelProcessingTarget(
          ChannelNameTarget target) =>
      _captureChannelProcessingTarget(target);
  @override
  String? channelProcessingError(ChannelProcessingTarget target,
          {bool paste = false}) =>
      _channelProcessingError(target, paste: paste);
  @override
  Future<String?> copyChannelProcessing(ChannelProcessingTarget target) =>
      _copyChannelProcessing(target);
  @override
  Future<ChannelProcessingPaste> prepareChannelProcessingPaste(
          ChannelProcessingTarget target) =>
      _prepareChannelProcessingPaste(target);
  @override
  Future<ProcessorPresetResult> pasteChannelProcessing(
          ChannelProcessingPaste proposal) =>
      _pasteChannelProcessing(proposal);
  @override
  void cancelChannelProcessing() => _cancelChannelProcessing();

  ProcessorPreset? _inputClipboard;
  String? _inputClipboardDomain;
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
  void _observeInputPresetCatalogue() {
    final catalogue = jsonEncode([
      engine.topology,
      engine.stereoPairs,
      engine.supportedParams.toList()..sort(),
      engine.parameterMetadata
    ]);
    if (_inputCatalogue != catalogue) {
      _inputCatalogue = catalogue;
      _inputCatalogueRevision++;
      for (final recall in _presetRecalls.values) {
        recall.stop('Processor capabilities changed. Reopen presets.');
      }
    }
  }

  @override
  ProcessorPresetTarget? capturePresetTarget(ProcessorPresetKind kind) {
    return captureChannelPresetTarget(kind, _channel);
  }

  @override
  ProcessorPresetTarget? captureChannelPresetTarget(
    ProcessorPresetKind kind,
    int channel, {
    int? session,
    String? key,
    int? accessEpoch,
  }) {
    if ((session != null && session != engine.session) ||
        (accessEpoch != null && accessEpoch != controlAccessEpoch) ||
        (key != null &&
            !engine.strips.any((s) => s.ch == channel && s.key == key))) {
      return null;
    }
    if (kind.index > ProcessorPresetKind.compressor.index) return null;
    final family = _presetFamily(channel)?['key'];
    if (family is! String || family.isEmpty) return null;
    for (final strip in engine.strips) {
      if (strip.ch != channel) continue;
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
      _observeInputPresetCatalogue();
      _presetDomains[target] = _inputCatalogueRevision;
      // A structurally valid target keeps the library reachable while guard
      // and value hydration report their specific readiness errors inside it.
      return target;
    }
    return null;
  }

  @override
  String? presetTargetError(ProcessorPresetTarget target) {
    if (_presetFields(target)
        .any((field) => !_accessParamAllowed(target.channel, field))) {
      return 'This processor is limited on this device.';
    }
    if (locked ||
        _presetDisposed ||
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
    _observeInputPresetCatalogue();
    if (_presetDomains[target] != _inputCatalogueRevision) {
      return 'Processor capabilities changed. Reopen presets.';
    }
    if (!engine.inputProcessorGuard) {
      return 'Guarded input processor recall and paste are unavailable for this provider.';
    }
    if (engine.syncEpoch == null) {
      return 'Waiting for mixer connection state. Reopen presets after the connection finishes.';
    }
    final catalogueError = _presetCatalogueError(target);
    if (catalogueError != null) return catalogueError;
    if (_presetFields(target).isEmpty) {
      return '${target.kind.label} presets are unavailable for this channel.';
    }
    final protection = _inputPresetProtection(target);
    if (protection != null) return protection;
    for (final field in _presetFields(target)) {
      if (!engine.valueKnown(target.channel, field) ||
          !ps.confirmedKnown(_maps, target.channel, field) ||
          _presetVerifiedFields[target.channel]?.contains(field) != true) {
        return 'Waiting for verified mixer readback of ${_presetSettingLabel(field)}. '
            'Open channel processing, then reopen presets.';
      }
    }
    return null;
  }

  String? _presetReadyError(ProcessorPresetTarget target) {
    final unavailable = presetTargetError(target);
    if (unavailable != null) return unavailable;
    if (_presetRecalls.values.any((r) =>
        r.target.channel == target.channel &&
        (r.processing || r.target.kind == target.kind))) {
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
      _presetRecalls.values.any(
        (recall) =>
            recall.target.channel == channel &&
            (field == 'link' || recall.contains(field)),
      );

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
    if (!mixerProviderPolicy(target.driver)
        .validRawPreset(field, value.toDouble())) {
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
    final ProcessorPreset preset;
    try {
      preset = _captureInputProcessor(target, name);
    } catch (error) {
      return '$error';
    }
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

  @override
  String? processorPasteError(ProcessorPresetTarget target) =>
      _inputClipboardReadyError(target) ??
      (_inputClipboard == null || !_presetCompatible(target, _inputClipboard!)
          ? 'Copy a compatible input processor first.'
          : _inputClipboardDomain != _inputFieldDomain(target)
              ? 'The copied processor has different field domains. Copy it again.'
              : null);

  @override
  String? copyInputProcessor(ProcessorPresetTarget target) {
    try {
      final error = _inputClipboardReadyError(target);
      if (error != null) return error;
      final snapshot = _captureInputProcessor(target, 'Copied processor');
      _inputClipboardDomain = _inputFieldDomain(target);
      _inputClipboard = snapshot;
      notifyListeners();
      return null;
    } catch (error) {
      return 'Could not copy processor: $error';
    }
  }

  @override
  Future<ProcessorPresetResult> pasteInputProcessor(
    ProcessorPresetTarget target,
  ) {
    final error = processorPasteError(target);
    if (error != null) {
      return Future.value(
        ProcessorPresetResult(error: error, confirmed: 0, total: 0),
      );
    }
    return _recallInputProcessor(target, clipboard: _inputClipboard);
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
    // Parameter echoes never complete a processor command. Only the matching
    // provider result, carrying its independently verified block, can do so.
    _pollPresetRecalls(_nowMs());
  }

  @override
  void _receiveInputProcessorResult(Map<String, dynamic> frame) {
    _pollPresetRecalls(_nowMs());
    for (final recall in _presetRecalls.values) {
      final waiting = recall.waiting;
      if (waiting == null ||
          waiting.requestId != frame['request_id'] ||
          waiting.epoch != frame['epoch'] ||
          recall.error != null ||
          waiting.done.isCompleted) {
        continue;
      }
      final snapshot = frame['snapshot'];
      final message = frame['message'];
      if (message is String || snapshot is! Map<String, dynamic>) {
        recall.stop(message is String
            ? message
            : 'No verified processor result arrived; result uncertain.');
      } else {
        try {
          _validateInputSnapshot(recall.target, snapshot,
              processing: recall.processing);
        } catch (error) {
          recall.stop('$error');
          continue;
        }
        // Keep our own observed values aligned with the correlated result.
        // Regular parameter frames still supply the screen's read model.
        for (final row in snapshot['rows'] as List) {
          recall.observed[row['id'] as String] = row['value'] as Object;
        }
        waiting.snapshot = Map<String, dynamic>.from(snapshot);
        waiting.finish(null);
      }
    }
  }

  void _notifyPresetRecall() => notifyListeners();

  @override
  Future<ProcessorPresetResult> recallProcessorPreset(
          ProcessorPresetTarget target, String id) =>
      _recallInputProcessor(target, id: id);
}
