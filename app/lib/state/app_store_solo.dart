part of 'app_store.dart';

// Stable solo destinations supplement real topology strips. No synthetic ch IDs.
extension _SoloControls on _AppStoreContext {
  bool get _hasAnySolo => _hardSoloCount > 0;
  int get _hardSoloCount =>
      engine.solos
          .where((ch) =>
              _targetSolo(ch) == null && _inputSoloDescriptor(ch) == null)
          .length +
      _targetSoloRows.where((r) => r.value == 1).length;

  List<DestinationControlVm> get _targetSoloRows => engine.destinationControls
      .where((r) => r.surface == 'solo')
      .toList(growable: false);

  bool _inputSoloStrip(int ch) => engine.topology.any((f) =>
      f['kind'] == 'in' && ch >= (f['first'] as num).toInt() &&
      ch < (f['first'] as num).toInt() + (f['count'] as num).toInt());

  DestinationControlVm? _inputSoloDescriptor(int ch) {
    if (!_inputSoloStrip(ch)) return null;
    final key = engine.strips.where((s) => s.ch == ch).firstOrNull?.key;
    return _targetSoloRows
        .where((r) => r.target == key)
        .firstOrNull;
  }

  DestinationControlVm? _targetSolo(int ch) {
    var target = ch < 0
        ? masterProcessingTarget
        : engine.strips.where((s) => s.ch == ch).firstOrNull?.key;
    if (ch >= 0 && _inputSoloStrip(ch)) return null;
    var id = 'solo';
    if (target?.startsWith('ret/') == true) {
      target = target!.replaceFirst('ret/', 'return/');
    }
    if (target?.startsWith('mix/') == true) {
      target = target!.replaceFirst('mix/', 'bus/');
    }
    if (ch < 0 && _busId.startsWith('solo:')) {
      final parts = _busId.split(':');
      target = parts[1];
      id = parts[2];
    } else if (target?.startsWith('fxsnd/') == true) {
      target = target!.replaceFirst('fxsnd/', 'fx/');
      id = 'send_solo';
    } else if (target?.startsWith('fxret/') == true) {
      target = target!.replaceFirst('fxret/', 'fx/');
      id = 'return_solo';
    } else if (target?.startsWith('dca/') == true) {
      target = target!.replaceFirst('dca/', 'vca/');
    }
    return _targetSoloRows
        .where((r) => r.target == target && r.id == id)
        .firstOrNull;
  }

  bool _targetSoloReady(int ch) {
    final row = _targetSolo(ch);
    return row != null &&
        destinationControlIsCurrent(row) &&
        accessFamilyAllowed(AccessFamily.channelSolo) &&
        engine.locks.isEmpty;
  }

  bool _hardSoloValue(int ch) => _targetSolo(ch)?.value == 1;

  bool get _advertisedSoloClearReady =>
      engine.connected &&
      engine.capsReady &&
      _online &&
      !locked &&
      engine.locks.isEmpty &&
      _targetSoloRows.isNotEmpty &&
      _targetSoloRows.every(
        (r) =>
            r.authority == 'verified' &&
            r.available &&
            _accessDestinationAllowed(r),
      );

  List<MixerBusRow> get _soloBusRows => [
        for (final row in _targetSoloRows.where((r) =>
            !engine.strips.any((s) => _targetSolo(s.ch) == r ||
                (_inputSoloStrip(s.ch) && s.key == r.target)) &&
            !r.target.startsWith('bus/') &&
            !r.target.startsWith('subgroup/')))
          MixerBusRow(
              id: 'solo:${row.target}:${row.id}',
              name: row.targetLabel,
              sub: 'Solo',
              muted: false,
              active: _busId == 'solo:${row.target}:${row.id}',
              meter: _busMeters.putIfAbsent(
                  'solo:${row.target}:${row.id}', () => ValueNotifier(double.nan))),
      ];

  void _writeTargetSolo(int ch) {
    final row = _targetSolo(ch);
    if (row == null || !_targetSoloReady(ch)) return;
    // Guarded immediate command, never coalesced or replayed after reconnect.
    unawaited(
      setDestinationControl(row, row.value == 1 ? 0 : 1).then((error) {
        if (error != null) _noteTransportError(StateError(error));
      }),
    );
  }
}
