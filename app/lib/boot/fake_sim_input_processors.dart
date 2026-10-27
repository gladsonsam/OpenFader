part of 'fake_sim_engine.dart';

extension _FakeSimInputProcessors on FakeSimEngine {
  void _inputProcessor(Map<String, dynamic> c) {
    Map<String, dynamic>? result;
    String? error;
    try {
      if (c['request_id'] is! String ||
          (c['request_id'] as String).isEmpty ||
          (c['request_id'] as String).length > 128) {
        throw StateError('Invalid processor request ID');
      }
      if ((c['field'] == null) != (c['value'] == null) ||
          (c['guard'] != null && jsonEncode(c['guard']).length > 65536)) {
        throw StateError('Incomplete or oversized processor request');
      }
      final ch = c['ch'];
      final processor = c['processor'];
      final kind = switch (processor) {
        'eq' => ProcessorPresetKind.eq,
        'gate' => ProcessorPresetKind.gate,
        'compressor' => ProcessorPresetKind.compressor,
        _ => throw StateError('Unknown input processor'),
      };
      if (ch is! int ||
          ch < 0 ||
          ch >= _params.length ||
          c['epoch'] != _processorEpoch ||
          c['key'] != _strips[ch].key) {
        throw StateError('Input connection or channel changed');
      }
      bool member(String id) =>
          processorPresetField(kind, id) ||
          switch (kind) {
            ProcessorPresetKind.eq => id == 'eq_on' ||
                id == 'hpf_on' ||
                RegExp(r'^eq[1-4]_on$').hasMatch(id),
            ProcessorPresetKind.gate =>
              id == 'gate_on' || id == 'gate_filter_on',
            _ => id == 'comp_on' || id == 'comp_filter_on',
          };
      Map<String, dynamic> capture() {
        final left = ch - ch % 2;
        for (final i
            in _params[ch].containsKey('link') ? [left, left + 1] : <int>[]) {
          if (_params[i]['link'] != false) {
            throw StateError(
                'Input presets require verified unlinked channels');
          }
        }
        return {
          'provider': 'sim',
          'ch': ch,
          'processor': processor,
          'generation': _processorEpoch,
          'revision': _processorRevision,
          'domain': [
            for (final id in _params[ch].keys.where(member))
              {
                'id': id,
                'scopes': [FakeSimEngine._kind(ch)],
                'kind': _params[ch][id] is bool
                    ? 'bool'
                    : id.endsWith('_type')
                        ? 'enum'
                        : 'float',
                'range': canonicalSimDspRange(id) ??
                    (id.endsWith('_type') ? [0, 5] : null),
              }
          ],
          'rows': [
            for (final entry in _params[ch].entries.where((e) => member(e.key)))
              {'id': entry.key, 'value': entry.value}
          ],
          'links': [
            for (final i
                in _params[ch].containsKey('link') ? [left, left + 1] : <int>[])
              {'ch': i, 'value': false}
          ]
        };
      }

      final before = capture();
      if (c['guard'] != null && jsonEncode(c['guard']) != jsonEncode(before)) {
        throw StateError('Input processor changed; reopen presets');
      }
      final field = c['field'];
      if (field != null) {
        final value = c['value'];
        if (c['guard'] == null ||
            field is! String ||
            !processorPresetField(kind, field) ||
            !_params[ch].containsKey(field) ||
            !((_params[ch][field] is bool && value is bool) ||
                (_params[ch][field] is num &&
                    value is num &&
                    value.isFinite))) {
          throw StateError('Input setting cannot be recalled');
        }
        final range = canonicalSimDspRange(field) ??
            (field.endsWith('_type') ? [0, 5] : null);
        if (value is num &&
            (range == null ||
                value < range.first ||
                value > range.last ||
                (field.endsWith('_type') && value != value.roundToDouble()))) {
          throw StateError('Input value is outside the provider domain');
        }
        if (_params[ch][field] != value) _processorRevision++;
        _params[ch][field] = value as Object;
        _mailbox.add(jsonEncode({
          'type': 'param',
          'ch': ch,
          'field': field,
          'value': value,
          'authority': 'verified'
        }));
      }
      result = capture();
    } catch (e) {
      error = '$e';
    }
    _mailbox.add(jsonEncode({
      'type': 'input_processor_result',
      'request_id': c['request_id'],
      'epoch': c['epoch'],
      'snapshot': result,
      'message': error
    }));
  }
}
