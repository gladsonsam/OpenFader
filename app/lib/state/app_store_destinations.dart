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
      _accessDestinationAllowed(control) &&
      !_presetDisposed &&
      _online &&
      engine.connected &&
      engine.capsReady &&
      !locked &&
      control.available &&
      ((control.surface != 'global' &&
              control.surface != 'fx' &&
              control.surface != 'usb_interface' &&
              control.surface != 'safe_levels' &&
              control.surface != 'clock_rate' &&
              control.surface != 'midi_setup' &&
              control.surface != 'mute_behavior' &&
              !control.targetSolo &&
              !control.talkbackGesture) ||
          control.authority == 'verified') &&
      !_outputPresetBusy(control.target) &&
      engine.destinationControls.contains(control);

  @override
  Future<String?> setDestinationControl(
    DestinationControlVm control,
    double value,
  ) {
    if (!destinationControlIsCurrent(control)) {
      return Future.value(
          'Connection or setting changed. Reopen this control.');
    }
    // Shared global configuration uses an exact provider block guard too.
    // It is never preset material; the guard prevents peer/ABA changes while
    // an editor is open and waits for independently verified readback.
    return _sendDestinationControl(
      control,
      value,
      guard: control.surface == 'global' ||
              control.surface == 'fx' ||
              control.surface == 'routing' ||
              control.surface == 'input_insert' ||
              control.surface == 'automix_settings' ||
              control.surface == 'bus_link' ||
              control.surface == 'link_policy' ||
              control.surface == 'usb_interface' ||
              control.surface == 'safe_levels' ||
              control.surface == 'clock_rate' ||
              control.surface == 'midi_setup' ||
              control.surface == 'mute_behavior' ||
              control.surface.startsWith('output_') ||
              control.targetSolo ||
              control.talkbackGesture
          ? engine.destinationControls
              .where(
                (row) =>
                    row.target == control.target &&
                    row.surface == control.surface,
              )
              .toList(growable: false)
          : null,
    );
  }

  Future<DestinationEditResult> setDestinationControlWithReceipt(
      DestinationControlVm control, double value) async {
    if (!destinationControlIsCurrent(control) || !control.talkbackGesture) {
      return const DestinationEditResult(
          'Talkback connection or access changed.');
    }
    DestinationControlVm? applied;
    final error = await _sendDestinationControl(control, value,
        guard: engine.destinationControls
            .where((row) =>
                row.target == control.target && row.surface == control.surface)
            .toList(growable: false),
        onApplied: (row) => applied = row);
    return DestinationEditResult(
        error ??
            (applied == null
                ? 'Talkback write has no independent ownership receipt; result uncertain.'
                : null),
        applied);
  }

  @override
  bool _destinationHasPending(String target) =>
      _destinationEdits.values.any((edit) => edit.target.target == target);

  @override
  Future<String?> _sendDestinationControl(
    DestinationControlVm control,
    double value, {
    List<DestinationControlVm>? guard,
    void Function(DestinationControlVm)? onApplied,
  }) {
    if (!_accessDestinationAllowed(control)) {
      return Future.value('This control is limited on this device.');
    }
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
    final wireValue = control.wireValue(value);
    if (wireValue == null) {
      return Future.value('This setting is unavailable.');
    }
    if (wireValue == control.value) return Future.value(null);
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
      recalledValue: guard == null ? null : wireValue,
      onApplied: onApplied,
    );
    _destinationEdits[id] = edit;
    unawaited(
      _command(
        _clientId,
        Cmds.destinationParam(id, control, wireValue, guard: guard),
      ).then(
        (_) {},
        onError: (Object error) {
          if (guard != null) {
            _finishDestinationEdit(
              id,
              'Could not send output setting: $error. Its result is uncertain.',
            );
          }
          if (!_presetDisposed) _noteTransportError(error);
        },
      ),
    );
    return edit.done.future;
  }

  void _finishDestinationEdit(String id, String? error) {
    final edit = _destinationEdits.remove(id);
    if (edit != null && !edit.done.isCompleted) {
      if (error == null && edit.applied != null) {
        edit.onApplied?.call(edit.applied!);
      }
      edit.done.complete(error);
    }
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
      if (edit.onApplied != null &&
          edit.accepted &&
          edit.applied != null &&
          engine.destinationControls.any((row) =>
              row.epoch == edit.target.epoch &&
              row.target == edit.target.target &&
              row.id == edit.target.id &&
              row.generation == edit.applied!.generation &&
              row.authority == 'verified' &&
              row.revision != null &&
              row.revision! > edit.applied!.revision!)) {
        _finishDestinationEdit(entry.key,
            'Talkback changed after readback; ownership not acquired. Current mixer state preserved.');
        continue;
      }
      if (now < edit.deadline &&
          edit.accepted &&
          edit.recalledValue != null &&
          engine.destinationControls.any(
            (control) =>
                control.epoch == edit.target.epoch &&
                control.target == edit.target.target &&
                control.id == edit.target.id &&
                control.revision != edit.target.revision &&
                control.value == edit.recalledValue &&
                control.authority == 'verified' &&
                (edit.onApplied == null || control == edit.applied),
          )) {
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
    final rawApplied = frame['applied'];
    if (rawApplied is Map) {
      final row = DestinationControlVm.fromJson(edit.target.epoch, rawApplied);
      if (row != null &&
          row.target == edit.target.target &&
          row.id == edit.target.id &&
          row.generation == edit.target.generation &&
          row.revision != null &&
          row.revision != edit.target.revision &&
          row.talkbackGesture == edit.target.talkbackGesture &&
          row.authority == 'verified' &&
          row.value == edit.recalledValue) {
        edit.applied = row;
      }
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
  _DestinationEdit(this.target, this.deadline,
      {this.recalledValue, this.onApplied});
  final void Function(DestinationControlVm)? onApplied;
  DestinationControlVm? applied;
  final DestinationControlVm target;
  final double deadline;
  final double? recalledValue;
  bool accepted = false;
  final done = Completer<String?>();
}
