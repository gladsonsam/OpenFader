part of 'app_store.dart';

extension _InputProcessorGuard on _AppStorePresets {
  Future<Map<String, dynamic>> _inputProcessorStep(
    _PresetRecall recall, {
    Map<String, dynamic>? snapshot,
    String? field,
    Object? value,
  }) async {
    final target = recall.target;
    final changed = _presetRecallError(recall);
    if (changed != null) throw StateError(changed);
    final epoch = engine.syncEpoch;
    if (epoch == null) {
      throw StateError('Waiting for the mixer connection epoch.');
    }
    final waiting =
        _PresetExpectation(field ?? '', value ?? false, _nowMs() + 15000)
          ..requestId = 'input-processor-${target.session}-${_presetId++}'
          ..epoch = epoch;
    recall.waiting = waiting;
    unawaited(Future<void>.sync(() => _command(
        _clientId,
        jsonEncode({
          'cmd': 'input_processor',
          'request_id': waiting.requestId,
          'epoch': epoch,
          'ch': target.channel,
          'key': target.key,
          'processor': target.kind.name,
          'field': field,
          'value': value,
          'guard': snapshot,
        }))).catchError((Object error) {
      if (identical(recall.waiting, waiting)) {
        recall.stop(
            'Could not send processor command: $error. Result uncertain.');
      }
    }));
    final error = await waiting.done.future;
    if (error != null) throw StateError(error);
    final stale = _presetRecallError(recall);
    if (stale != null || engine.syncEpoch != epoch) {
      throw StateError(
          stale ?? 'Connection changed during processor readback.');
    }
    recall.waiting = null;
    return waiting.snapshot!;
  }

  void _validateInputSnapshot(
      ProcessorPresetTarget target, Map<String, dynamic> snapshot) {
    final rows = snapshot['rows'];
    final fields = _presetFields(target);
    if (snapshot['provider'] != target.driver ||
        snapshot['ch'] != target.channel ||
        snapshot['processor'] != target.kind.name ||
        snapshot['domain'] is! List ||
        rows is! List ||
        rows.any(
            (r) => r is! Map || r['id'] is! String || r['value'] == null) ||
        fields.any((f) => rows.where((r) => r['id'] == f).length != 1)) {
      throw StateError(
          'The provider did not capture a complete compatible processor block.');
    }
  }
}
