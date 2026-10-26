part of 'app_store.dart';

abstract interface class _AppStoreOutputPresetsContract {
  bool _outputPresetBusy(String target);
}

/// Output presets use the existing bounded local codec and storage. Input
/// parameter-plane recall is deliberately independent of destination DSP.
mixin _AppStoreOutputPresets on _AppStoreContext {
  final Expando<int> _outputPresetTargets = Expando<int>();
  ProcessorPreset? _outputClipboard;
  OutputPresetTarget? _outputRecall;
  bool _outputRecallCancelled = false;
  int _outputPresetSequence = 0;

  @override
  bool get hasOutputProcessorClipboard => _outputClipboard != null;
  @override
  bool _outputPresetBusy(String target) => _outputRecall?.target == target;

  @override
  OutputPresetTarget? captureOutputPresetTarget(String surface) {
    final target = outputProcessingTarget;
    final controls = outputProcessingControls
        .where((c) => c.target == target && c.surface == surface)
        .toList();
    if (target == null ||
        controls.isEmpty ||
        outputPresetKind(surface) == null) {
      return null;
    }
    final captured = OutputPresetTarget(
      engine.session,
      engine.driver,
      target,
      surface,
      controls,
    );
    _outputPresetTargets[captured] = _presetGeneration;
    return outputPresetTargetError(captured) == null ? captured : null;
  }

  String? _outputIdentityError(OutputPresetTarget target) {
    if (_presetDisposed ||
        !_online ||
        !engine.connected ||
        !engine.capsReady ||
        locked ||
        target.session != engine.session ||
        target.driver != engine.driver ||
        _outputPresetTargets[target] != _presetGeneration) {
      return 'Connection or permissions changed. Reopen output presets.';
    }
    return null;
  }

  String? _outputSnapshotError(
    OutputPresetTarget target,
    List<DestinationControlVm> snapshot,
  ) {
    final error = _outputIdentityError(target);
    if (error != null) return error;
    final live = engine.destinationControls
        .where((c) => c.target == target.target && c.surface == target.surface)
        .toList();
    if (snapshot.length != live.length ||
        snapshot.any((c) => c.revision == null || c.generation == null || !live.contains(c))) {
      return 'Output processor changed. Reopen presets; no further settings were sent.';
    }
    return null;
  }

  @override
  String? outputPresetTargetError(OutputPresetTarget target) {
    final error = _outputSnapshotError(target, target.controls);
    if (error != null) return error;
    if (_outputRecall != null || _destinationHasPending(target.target)) {
      return 'Wait for the current output changes to finish.';
    }
    final kind = target.kind;
    final fields = target.controls.where((c) => c.preset).toList();
    if (kind == null ||
        fields.isEmpty ||
        fields.any((c) => !processorPresetField(kind, c.id))) {
      return 'This output processor does not advertise a supported preset surface.';
    }
    if (fields.any((c) => !c.available)) {
      return 'Waiting for the mixer to provide all output processor settings.';
    }
    return null;
  }

  bool _outputCompatible(OutputPresetTarget target, ProcessorPreset preset) =>
      preset.driver == target.driver &&
      preset.family == target.surface &&
      preset.kind == target.kind;

  @override
  Future<List<ProcessorPreset>> loadOutputPresets(
    OutputPresetTarget target,
  ) async {
    final error = outputPresetTargetError(target);
    if (error != null) throw StateError(error);
    final presets = await _processorPresets.load();
    final changed = outputPresetTargetError(target);
    if (changed != null) throw StateError(changed);
    return List.unmodifiable(
      presets.where((p) => _outputCompatible(target, p)),
    );
  }

  ProcessorPreset _captureOutputProcessor(
    OutputPresetTarget target,
    String name,
  ) {
    final error =
        outputPresetTargetError(target) ?? validateProcessorPresetName(name);
    if (error != null) throw StateError(error);
    final fields = target.controls.where((c) => c.preset).toList();
    if (fields.any(
      (c) =>
          c.authority != 'verified' ||
          c.value == null ||
          !c.allowsValue(c.value!),
    )) {
      throw StateError(
        'Wait for verified mixer readback before saving or copying. Accepted values cannot substantiate a preset.',
      );
    }
    final now = DateTime.now();
    return ProcessorPreset(
      id: 'output-${now.microsecondsSinceEpoch}-${_outputPresetSequence++}',
      name: name.trim(),
      driver: target.driver,
      family: target.surface,
      kind: target.kind!,
      values: {for (final c in fields) c.id: c.value!},
      createdAt: now,
      updatedAt: now,
    );
  }

  @override
  Future<String?> saveOutputPreset(
    OutputPresetTarget target,
    String name,
  ) async {
    try {
      final preset = _captureOutputProcessor(target, name);
      await _processorPresets.save(preset);
      final changed = _outputIdentityError(target);
      return changed == null ? null : 'Preset saved on this device. $changed';
    } catch (error) {
      return 'Could not save preset: $error';
    }
  }

  Future<ProcessorPreset> _findOutputPreset(
    OutputPresetTarget target,
    String id,
  ) async {
    final presets = await loadOutputPresets(target);
    return presets.firstWhere(
      (p) => p.id == id,
      orElse: () =>
          throw StateError('This preset is no longer in the library.'),
    );
  }

  @override
  Future<String?> renameOutputPreset(
    OutputPresetTarget target,
    String id,
    String name,
  ) async {
    try {
      await _findOutputPreset(target, id);
      final error =
          outputPresetTargetError(target) ?? validateProcessorPresetName(name);
      if (error != null) return error;
      await _processorPresets.rename(id, name);
      return null;
    } catch (error) {
      return 'Could not rename preset: $error';
    }
  }

  @override
  Future<String?> deleteOutputPreset(
    OutputPresetTarget target,
    String id,
  ) async {
    try {
      await _findOutputPreset(target, id);
      final error = outputPresetTargetError(target);
      if (error != null) return error;
      await _processorPresets.delete(id);
      return null;
    } catch (error) {
      return 'Could not delete preset: $error';
    }
  }

  @override
  String? copyOutputProcessor(OutputPresetTarget target) {
    try {
      _outputClipboard = _captureOutputProcessor(target, 'Copied processor');
      notifyListeners();
      return null;
    } catch (error) {
      return 'Could not copy processor: $error';
    }
  }

  @override
  void cancelOutputPresetRecall() {
    _outputRecallCancelled = true;
  }

  @override
  Future<ProcessorPresetResult> recallOutputPreset(
    OutputPresetTarget target,
    String? id,
  ) async {
    var total = 0;
    var confirmed = 0;
    var sent = 0;
    var accepted = 0;
    var acquired = false;
    try {
      final ready = outputPresetTargetError(target);
      if (ready != null) throw StateError(ready);
      final preset =
          id == null ? _outputClipboard : await _findOutputPreset(target, id);
      final changed = outputPresetTargetError(target);
      if (changed != null) throw StateError(changed);
      if (preset == null || !_outputCompatible(target, preset)) {
        throw StateError('Copy a compatible output processor first.');
      }
      final fields = target.controls.where((c) => c.preset).toList();
      total = fields.length;
      if (preset.values.length != total ||
          fields.any(
            (c) =>
                preset.values[c.id] is! num ||
                !c.allowsValue((preset.values[c.id] as num).toDouble()),
          )) {
        throw StateError(
          'The saved processor does not match all advertised settings and domains on this output.',
        );
      }
      // The original block (including bypass states) is held through every
      // step. Only our just-written descriptor may advance; peer edits, ABA
      // revisions, reconnects and catalogue changes stop the next dispatch.
      final snapshot = List<DestinationControlVm>.of(target.controls);
      _outputRecall = target;
      acquired = true;
      _outputRecallCancelled = false;
      notifyListeners();
      for (final field in fields) {
        if (_outputRecallCancelled) throw StateError('Recall cancelled.');
        final stale = _outputSnapshotError(target, snapshot);
        if (stale != null) throw StateError(stale);
        final index = snapshot.indexWhere((c) => c.id == field.id);
        final control = snapshot[index];
        final value = (preset.values[field.id] as num).toDouble();
        if (control.value != value) {
          sent++;
          final error = await _sendDestinationControl(
            control,
            value,
            guard: snapshot,
          );
          if (error != null) throw StateError(error);
          accepted++;
          final identity = _outputIdentityError(target);
          if (identity != null) throw StateError(identity);
          final live = engine.destinationControls
              .where(
                (c) => c.target == target.target && c.surface == target.surface,
              )
              .toList();
          final result = live.where((c) => c.id == control.id).firstOrNull;
          if (result == null || result.value != value || !result.available) {
            throw StateError(
              'Command accepted without matching output state; result uncertain.',
            );
          }
          // Do not replace domains or any sibling from a fresh snapshot.
          // Backend guards recheck this block under the provider write lock.
          if (!_sameOutputDomain(control, result)) {
            throw StateError(
              'Output descriptor changed while writing; result uncertain.',
            );
          }
          snapshot[index] = result;
          final stale = _outputSnapshotError(target, snapshot);
          if (stale != null) throw StateError(stale);
        }
        if (snapshot[index].authority == 'verified') confirmed++;
      }
      if (_outputRecallCancelled) throw StateError('Recall cancelled.');
      return ProcessorPresetResult(
        error: confirmed == total
            ? null
            : '$accepted changes accepted; $confirmed of $total settings verified by mixer readback. Remaining values are unconfirmed.',
        confirmed: confirmed,
        total: total,
      );
    } catch (error) {
      return ProcessorPresetResult(
        error:
            '$error $sent writes attempted, $accepted accepted; $confirmed of $total settings verified. No changes were rolled back.',
        confirmed: confirmed,
        total: total,
      );
    } finally {
      if (acquired && identical(_outputRecall, target)) _outputRecall = null;
      if (!_presetDisposed) notifyListeners();
    }
  }

  bool _sameOutputDomain(DestinationControlVm a, DestinationControlVm b) {
    Map<String, dynamic> domain(DestinationControlVm control) =>
        control.descriptor
          ..remove('revision')
          ..remove('value')
          ..remove('authority');
    return a.epoch == b.epoch && jsonEncode(domain(a)) == jsonEncode(domain(b));
  }
}
