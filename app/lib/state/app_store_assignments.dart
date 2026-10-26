part of 'app_store.dart';

// Captured-channel membership edits. No selection-dependent setters or timers.
abstract interface class _AppStoreAssignmentsContract {
  void _pollAssignmentEdit(double now);
  void _receiveAssignmentEvent(Map<String, dynamic> event);
  void _interruptAssignmentEdit(String error);
}

mixin _AppStoreAssignments on _AppStoreContext {
  _ChannelAssignmentEdit? _assignmentEdit;
  static final _membership = RegExp(r'^(dca|view)([1-9][0-9]*)_assign$');
  static final _group = RegExp(r'^(subgroup|mute_group):([0-9]+)$');

  String _assignmentLabel(String id) {
    if (id == 'lr_assign') return 'Main LR';
    final member = _membership.firstMatch(id)!;
    final family = member[1] == 'view' ? 'View' : 'Control group';
    final label = engine.parameterMetadata[id]?['label'] as String?;
    final identity = '$family ${member[2]}';
    return label == null || label == id ? identity : '$identity · $label';
  }

  bool _assignmentParamApplies(ChannelNameTarget target, String id) {
    final metadata = engine.parameterMetadata[id];
    final family = _presetFamily(target.channel);
    final tags = family?['params_scope'];
    final kinds = tags is List ? tags : [family?['kind']];
    final scopes = metadata?['scopes'];
    final presentation = parameterPresentation(metadata, (field) {
      final value = engine.params[target.channel]?[field];
      return value is num ? value.toDouble() : null;
    });
    return engine.supportedParams.contains(id) &&
        metadata?['kind'] == 'bool' &&
        scopes is List &&
        scopes.any(kinds.contains) &&
        presentation?['hidden'] != true &&
        presentation?['available'] != false;
  }

  @override
  List<ChannelAssignmentChoice> assignmentsForChannel(
    ChannelNameTarget target,
  ) {
    if (!engine.capsReady) return const [];
    final fields = engine.supportedParams
        .where((id) => id == 'lr_assign' || _membership.hasMatch(id))
        .toList()
      ..sort();
    return [
      for (final id in fields)
        if (_assignmentParamApplies(target, id) &&
            assignmentForChannel(target, id) != null)
          ChannelAssignmentChoice(
            id,
            _assignmentLabel(id),
          ),
      // Group capability counts alone do not imply this strip is assignable:
      // require its membership snapshot, including false, from the backend.
      for (var g = 0; g < engine.subgroupCount; g++)
        if (engine.subgroupAssign.containsKey('${target.channel}:$g'))
          ChannelAssignmentChoice('subgroup:$g', 'Subgroup ${g + 1}'),
      for (var g = 0; g < engine.muteGroupCount; g++)
        if (engine.muteGroupAssign.containsKey('${target.channel}:$g'))
          ChannelAssignmentChoice('mute_group:$g', 'Mute group ${g + 1}'),
    ];
  }

  @override
  bool? assignmentForChannel(ChannelNameTarget target, String id) {
    final group = _group.firstMatch(id);
    if (group != null) {
      final values = group[1] == 'subgroup'
          ? engine.subgroupAssign
          : engine.muteGroupAssign;
      return values['${target.channel}:${group[2]}'];
    }
    if (!engine.valueKnown(target.channel, id)) return null;
    final value = engine.params[target.channel]?[id];
    return value is bool ? value : null;
  }

  String? _assignmentUnavailable(ChannelNameTarget target, String id) {
    if (target.accessEpoch != controlAccessEpoch) {
      return 'Local access limits changed. Reopen Quick Assign.';
    }
    if (!engine.connected ||
        engine.session != target.session ||
        !engine.strips.any(
          (s) => s.ch == target.channel && s.key == target.key,
        )) {
      return 'Connection or channel changed. Reopen Quick Assign.';
    }
    if (!assignmentsForChannel(target).any((c) => c.id == id) ||
        assignmentForChannel(target, id) == null) {
      return 'This assignment is unavailable on this channel.';
    }
    final group = _group.firstMatch(id);
    if (group != null &&
        group[1] == 'subgroup' &&
        !accessTargetAllowed(
          'subgroup/${(int.parse(group[2]!) + 1).toString().padLeft(2, '0')}',
        )) {
      return 'This group is limited on this device.';
    }
    // Protect the sibling while linked or link state is unknown/pending.
    // Do not expand the write: the backend owns each provider's link policy.
    final partner = linkPartnerForChannel(target);
    final members = [
      target.channel,
      if (partner != null &&
          (linkedForChannel(target) != false ||
              engine.params[partner]?['link'] != false ||
              _presetVerifiedFields[target.channel]?.contains('link') != true ||
              _presetVerifiedFields[partner]?.contains('link') != true ||
              _maps.pending.containsKey(
                ps.pendingKey(target.channel, 'link'),
              ) ||
              _maps.pending.containsKey(ps.pendingKey(partner, 'link'))))
        partner,
    ];
    for (final ch in members) {
      if (!_accessParamAllowed(ch, id)) {
        return 'This assignment is limited on this device.';
      }
      if (engine.locks.containsKey(ch)) {
        return 'This channel or its stereo partner is locked.';
      }
      if (_maps.pending.containsKey(ps.pendingKey(ch, 'link')) ||
          _presetEditingBlocked(ch, id) ||
          _maps.pending.containsKey(ps.pendingKey(ch, id))) {
        return 'A change to this setting is already pending.';
      }
    }
    return null;
  }

  @override
  String? assignmentEditError(ChannelNameTarget target, String id) =>
      _assignmentUnavailable(target, id) ??
      (_assignmentEdit == null ? null : 'An assignment change is pending.');

  @override
  Future<ChannelAssignmentResult> assignChannel(
    ChannelNameTarget target,
    String id,
    bool value, {
    required bool expected,
  }) {
    final error = assignmentEditError(target, id);
    if (error != null) return Future.value(ChannelAssignmentResult(error));
    if (assignmentForChannel(target, id) != expected || value == expected) {
      return Future.value(const ChannelAssignmentResult(
          'Assignment changed. Review and try again.'));
    }
    final group = _group.firstMatch(id);
    final command = group == null
        ? {
            'cmd': 'param',
            'ch': target.channel,
            'key': target.key,
            'field': id,
            'value': value,
          }
        : {
            'cmd': '${group[1]}_assign',
            'ch': target.channel,
            'key': target.key,
            'group': int.parse(group[2]!),
            'member': value,
          };
    final edit = _ChannelAssignmentEdit(
      target,
      id,
      value,
      _nowMs() + ps.confirmTimeoutMs,
    );
    _assignmentEdit = edit;
    notifyListeners();
    // Listeners may navigate, reconnect or change local permissions during
    // notification. Revalidate before dispatch; an interrupted edit stays dead.
    if (!identical(_assignmentEdit, edit)) return edit.done.future;
    final dispatchError = _assignmentUnavailable(target, id) ??
        (assignmentForChannel(target, id) != expected
            ? 'Assignment changed. Review and try again.'
            : null);
    if (dispatchError != null) {
      _interruptAssignmentEdit(dispatchError);
      return edit.done.future;
    }
    // Immediate dispatch, with no coalescer or selection lookup after an await.
    unawaited(
      Future<void>.sync(() => _command(_clientId, jsonEncode(command))).then(
        (_) {
          if (!identical(_assignmentEdit, edit)) return;
          edit.sent = true;
          _pollAssignmentEdit(_nowMs());
          if (identical(_assignmentEdit, edit) && edit.observed) {
            _finishAssignmentEdit(null);
          }
        },
        onError: (Object error) {
          if (identical(_assignmentEdit, edit)) {
            _interruptAssignmentEdit(
              'Could not send assignment: $error. The result is uncertain.',
            );
          }
        },
      ),
    );
    return edit.done.future;
  }

  @override
  void _interruptAssignmentEdit(String error) => _finishAssignmentEdit(error);

  void _finishAssignmentEdit(String? error) {
    final edit = _assignmentEdit;
    _assignmentEdit = null;
    if (edit == null) return;
    edit.done.complete(ChannelAssignmentResult(error));
    notifyListeners();
  }

  @override
  void _pollAssignmentEdit(double now) {
    final edit = _assignmentEdit;
    if (edit == null) return;
    final error = _assignmentUnavailable(edit.target, edit.id);
    if (error != null || now >= edit.deadline) {
      _interruptAssignmentEdit(
        error ??
            'No matching assignment state arrived. The setting may have changed.',
      );
    }
  }

  @override
  void _receiveAssignmentEvent(Map<String, dynamic> event) {
    _pollAssignmentEdit(_nowMs());
    final edit = _assignmentEdit;
    if (edit == null) return;
    final ch = event['ch'];
    if (event['type'] == 'error' && (ch == null || ch == edit.target.channel)) {
      _interruptAssignmentEdit(
        'Backend reported an error: ${event['message'] ?? 'Unknown error'}. '
        'The assignment result is uncertain.',
      );
      return;
    }
    if (ch != edit.target.channel ||
        (event['key'] != null && event['key'] != edit.target.key)) {
      return;
    }
    final group = _group.firstMatch(edit.id);
    final matches = group == null
        ? event['type'] == 'param' &&
            event['field'] == edit.id &&
            (event['by'] == null || event['by'] == _clientId) &&
            event['value'] == edit.value &&
            (event['authority'] == 'accepted' ||
                event['authority'] == 'verified')
        : event['type'] == '${group[1]}_assign' &&
            event['group'] == int.parse(group[2]!) &&
            event['member'] == edit.value;
    if (matches) {
      // Events have no request ID: even own-writer verified state can be an
      // earlier ABA edit. Report observation only; let dispatch errors win.
      edit.observed = true;
      if (edit.sent) _finishAssignmentEdit(null);
    }
  }
}

class _ChannelAssignmentEdit {
  final ChannelNameTarget target;
  final String id;
  final bool value;
  final double deadline;
  bool sent = false, observed = false;
  final done = Completer<ChannelAssignmentResult>();
  _ChannelAssignmentEdit(this.target, this.id, this.value, this.deadline);
}
