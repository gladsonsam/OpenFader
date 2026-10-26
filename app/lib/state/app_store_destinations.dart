part of 'app_store.dart';

// Capability-addressed destination edits and result deadlines.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreDestinationsContract {
  void _interruptDestinationEdits(String error);
  bool _destinationHasPending(String target);
  Future<String?> _sendDestinationControl(
    DestinationControlVm control,
    double value, {
    List<DestinationControlVm>? guard,
  });
  void _pollDestinationEdits(double now);
  void _receiveDestinationResult(Map<String, dynamic> frame);
}

mixin _AppStoreDestinations on _AppStoreContext {
  int _destinationRequestSequence = 0;
  final Map<String, _DestinationEdit> _destinationEdits = {};

  @override
  bool destinationControlIsCurrent(DestinationControlVm control) =>
      !_presetDisposed &&
      _online &&
      engine.connected &&
      engine.capsReady &&
      !locked &&
      control.available &&
      !_outputPresetBusy(control.target) &&
      engine.destinationControls.contains(control);

  @override
  Future<String?> setDestinationControl(
    DestinationControlVm control,
    double value,
  ) {
    return _sendDestinationControl(control, value);
  }

  @override
  bool _destinationHasPending(String target) =>
      _destinationEdits.values.any((edit) => edit.target.target == target);

  @override
  Future<String?> _sendDestinationControl(
    DestinationControlVm control,
    double value, {
    List<DestinationControlVm>? guard,
  }) {
    if (guard == null
        ? !destinationControlIsCurrent(control)
        : (!_online ||
            !engine.connected ||
            locked ||
            !engine.destinationControls.contains(control))) {
      return Future.value(
        'Connection or setting changed. Reopen this control.',
      );
    }
    if (!control.allowsValue(value)) {
      return Future.value('This setting is unavailable.');
    }
    if (value == control.value) return Future.value(null);
    if (_destinationEdits.values.any(
      (edit) =>
          edit.target.target == control.target && edit.target.id == control.id,
    )) {
      return Future.value('A change to this destination is already pending.');
    }
    final id = 'destination-${engine.session}-${++_destinationRequestSequence}';
    final edit = _DestinationEdit(
      control,
      _nowMs() + 15000,
      recalledValue: guard == null ? null : value,
    );
    _destinationEdits[id] = edit;
    unawaited(
      _api
          .cmd(
        _clientId,
        Cmds.destinationParam(id, control, value, guard: guard),
      )
          .then(
        (_) {},
        onError: (Object error) {
          if (guard != null) {
            _finishDestinationEdit(id,
                'Could not send output setting: $error. Its result is uncertain.');
          }
          if (!_presetDisposed) _noteTransportError(error);
        },
      ),
    );
    return edit.done.future;
  }

  void _finishDestinationEdit(String id, String? error) {
    final edit = _destinationEdits.remove(id);
    if (edit != null && !edit.done.isCompleted) edit.done.complete(error);
  }

  @override
  void _interruptDestinationEdits(String error) {
    for (final id in _destinationEdits.keys.toList()) {
      _finishDestinationEdit(id, error);
    }
  }

  @override
  void _pollDestinationEdits(double now) {
    if (!_online || !engine.connected) {
      _interruptDestinationEdits(
        'Connection lost; the destination setting may have changed.',
      );
    }
    for (final entry in _destinationEdits.entries.toList()) {
      final edit = entry.value;
      if (now < edit.deadline &&
          edit.accepted &&
          edit.recalledValue != null &&
          engine.destinationControls.any((control) =>
              control.epoch == edit.target.epoch &&
              control.target == edit.target.target &&
              control.id == edit.target.id &&
              control.revision != edit.target.revision &&
              control.value == edit.recalledValue &&
              control.authority == 'verified')) {
        _finishDestinationEdit(entry.key, null);
        continue;
      }
      if (now >= entry.value.deadline) {
        _finishDestinationEdit(
          entry.key,
          'Command result or verified state did not arrive. The destination setting may have changed.',
        );
      }
    }
  }

  @override
  void _receiveDestinationResult(Map<String, dynamic> frame) {
    _pollDestinationEdits(_nowMs());
    final id = frame['request_id'];
    final edit = _destinationEdits[id];
    if (id is! String || edit == null || frame['epoch'] != edit.target.epoch) {
      return;
    }
    final message = frame['message'];
    if (frame['accepted'] == true &&
        message is! String &&
        edit.recalledValue != null) {
      // The engine unicasts its result BEFORE broadcasting destination state.
      // Hold guarded recalls until the shell poll has consumed both frames.
      edit.accepted = true;
      _pollDestinationEdits(_nowMs());
      return;
    }
    _finishDestinationEdit(
      id,
      message is String
          ? message
          : frame['accepted'] == true
              ? null
              : 'The mixer rejected this change.',
    );
  }
}

class _DestinationEdit {
  _DestinationEdit(this.target, this.deadline, {this.recalledValue});
  final DestinationControlVm target;
  final double deadline;
  final double? recalledValue;
  bool accepted = false;
  final done = Completer<String?>();
}
