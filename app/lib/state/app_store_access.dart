part of 'app_store.dart';

abstract interface class _AppStoreAccessContract implements AccessLimitsStore {
  String _processingTargetForAccessBus(String id);
  bool accessFamilyAllowed(AccessFamily family);
  bool accessTargetAllowed(String target);
  bool _accessParamAllowed(int ch, String field);
  bool _accessDestinationAllowed(DestinationControlVm control);
  Future<void> _command(String clientId, String json);
}

/// Local policy is intersected with existing readiness/locks, never substituted
/// for them. This module owns all classifications and the final send boundary.
mixin _AppStoreAccess on _AppStoreContext {
  Set<String> _blockedAccess = const {};
  int _controlAccessEpoch = 0;
  @override
  int get controlAccessEpoch => _controlAccessEpoch;

  // Existing provider input fields. Trim, polarity, balance, width and
  // sidechain sources remain independent processing controls.
  static const _inputRoutingFields = {
    'source',
    'source_a',
    'source_b',
    'source_select',
    'usbreturn',
    'wing:source_auto',
    'wing:source_mode',
  };
  static const _micPreampFields = {'gain', 'phantom'};
  @override
  Set<String> get blockedAccess => _blockedAccess;

  @override
  bool accessFamilyAllowed(AccessFamily family) =>
      !_blockedAccess.contains(family.key);

  static String _targetKey(String target) {
    final parts = target.trim().split('/');
    if (parts.length != 2) return target.trim();
    final index = int.tryParse(parts[1].trim());
    if (index == null || index < 1) return target.trim();
    final family = switch (parts[0].trim()) {
      'mix' => 'bus',
      'dca' => 'vca',
      // Strip topology calls FX send masters fxsnd; send metadata and global
      // controls call the same destination fx. Returns use fxret, separately.
      'fxsnd' => 'fx',
      final family => family,
    };
    if (family == 'main' && index == 1) return 'main';
    return '$family/${index.toString().padLeft(2, '0')}';
  }

  bool _targetBlocked(String target) => _blockedAccess.any(
        (key) =>
            key.startsWith('target:') && _targetKey(key.substring(7)) == target,
      );

  String? _channelTarget(int ch) {
    final kind = _presetFamily(ch)?['kind'];
    if (!const [
      'bus',
      'main',
      'dca',
      'mtx',
      'subgroup',
      'fxsnd',
    ].contains(kind)) {
      return null; // FX return strips are deliberately independent of FX globals.
    }
    return engine.strips.where((s) => s.ch == ch).firstOrNull?.key;
  }

  @override
  bool accessTargetAllowed(String target) {
    if (_blockedAccess.isEmpty) return true;
    target = _targetKey(target);
    if (_targetBlocked(target)) return false;
    // Stereo output pairs must not provide a second entrance to a limited mix.
    final strip =
        engine.strips.where((s) => _targetKey(s.key) == target).firstOrNull;
    if (strip != null) {
      final pair =
          engine.stereoPairs.where((p) => p.contains(strip.ch)).firstOrNull;
      if (pair != null &&
          pair.any((ch) => engine.params[ch]?['link'] == true)) {
        for (final ch in pair) {
          final key = engine.strips.where((s) => s.ch == ch).firstOrNull?.key;
          if (key != null && _targetBlocked(_targetKey(key))) {
            return false;
          }
        }
      }
    }
    return true;
  }

  static final _sendField = RegExp(r'^send([1-9]\d*)_(.+)$');
  static final _dspField = RegExp(
    r'^(eq\d*_|gate_|comp_|hpf_|lpf_|geq|peq|delay_|wing:(eq_|filter_|hcf_))',
  );
  static final _presetDspFields = {
    for (final kind in ProcessorPresetKind.values)
      ...processorPresetFields(kind),
  };
  static final _legacySendField = RegExp(r'^(aux|fx)([1-9]\d*)_(level|mute)$');

  bool _isDsp(String field) =>
      _dspField.hasMatch(field) || _presetDspFields.contains(field);

  String? _sendTarget(int slot) {
    final metadata = engine.parameterMetadata['send${slot}_level'];
    final presentation = parameterPresentation(metadata, (field) => null);
    final destination = presentation?['destination'];
    if (destination is Map && destination['key'] is String) {
      return destination['key'] as String;
    }
    return engine.destinationControls
        .where((c) => c.sendSlot == slot)
        .firstOrNull
        ?.target;
  }

  @override
  bool _accessParamAllowed(int ch, String field) {
    if (_blockedAccess.isEmpty) return true;
    if (_inputRoutingFields.contains(field) &&
        !accessFamilyAllowed(AccessFamily.inputRouting)) {
      return false;
    }
    if (_micPreampFields.contains(field) &&
        !accessFamilyAllowed(AccessFamily.micPreamps)) {
      return false;
    }
    if (field == 'link') {
      final pair = engine.stereoPairs.where((p) => p.contains(ch)).firstOrNull;
      // Linking copies processing, identity, sends and levels to the peer and
      // resets pan. Check both members even while the pair is still unlinked.
      final copiedFields = {
        ...engine.supportedParams,
        'name',
        'mute',
        'pan',
        'fader',
      }..removeAll(['link', 'solo']);
      if ((pair ?? [ch]).any(
        (member) =>
            copiedFields.any((field) => !_accessParamAllowed(member, field)),
      )) {
        return false;
      }
    }
    final target = _channelTarget(ch);
    if (target != null && !accessTargetAllowed(target)) return false;
    if (_presetFamily(ch)?['kind'] == 'fxsnd' &&
        !accessFamilyAllowed(AccessFamily.fx)) {
      return false;
    }
    if (field == 'mute' &&
        target == null &&
        !accessFamilyAllowed(AccessFamily.channelMute)) {
      return false;
    }
    if (field == 'solo') {
      // Raw legacy solo commands must respect the same canonical scope and
      // aliases as target buttons (notably VCA/DCA and FX returns).
      final solo = _targetSolo(ch) ?? _inputSoloDescriptor(ch);
      if (solo != null && !_accessDestinationAllowed(solo)) return false;
    }
    if (field == 'solo' && !accessFamilyAllowed(AccessFamily.channelSolo)) {
      return false;
    }
    if (const ['name', 'color', 'icon'].contains(field) &&
        !accessFamilyAllowed(AccessFamily.channelId)) {
      return false;
    }
    if (field == 'lr_assign' && !accessTargetAllowed('main')) return false;
    final dca = RegExp(r'^dca([1-9]\d*)_assign$').firstMatch(field);
    if (dca != null && !accessTargetAllowed('dca/${dca[1]!.padLeft(2, '0')}')) {
      return false;
    }
    final legacy = _legacySendField.firstMatch(field);
    if (legacy != null) {
      final destination =
          '${legacy[1] == 'aux' ? 'bus' : 'fx'}/${legacy[2]!.padLeft(2, '0')}';
      if (!accessTargetAllowed(destination) ||
          legacy[1] == 'fx' && !accessFamilyAllowed(AccessFamily.fx)) {
        return false;
      }
    }
    final send = _sendField.firstMatch(field);
    if (send != null) {
      final destination = _sendTarget(int.parse(send[1]!));
      if (destination != null && !accessTargetAllowed(destination)) {
        return false;
      }
      if (destination?.startsWith('fx/') == true &&
          !accessFamilyAllowed(AccessFamily.fx)) {
        return false;
      }
    }
    if ((field == 'pan' || send?[2] == 'pan') &&
        !accessFamilyAllowed(AccessFamily.pan)) {
      return false;
    }
    if (_isDsp(field) &&
        !accessFamilyAllowed(
          target == null ? AccessFamily.inputDsp : AccessFamily.masterDsp,
        )) {
      return false;
    }
    return true;
  }

  @override
  bool _accessDestinationAllowed(DestinationControlVm control) {
    if (control.targetSolo) {
      return control.soloScopeAllowed(
        accessFamilyAllowed(AccessFamily.channelSolo),
        accessTargetAllowed,
      );
    }
    if (control.surface == 'bus_link_state' ||
        control.surface == 'link_policy_state') {
      return false;
    }
    if (control.surface == 'link_policy' ||
        control.surface == 'usb_interface' ||
        control.surface == 'safe_levels' ||
        control.surface == 'clock_rate' ||
        control.surface == 'midi_setup') {
      return blockedAccess.isEmpty &&
          control.members.isNotEmpty &&
          AccessFamily.values.every(accessFamilyAllowed) &&
          control.members.every(accessTargetAllowed);
    }
    if (control.surface == 'bus_link') {
      return accessFamilyAllowed(AccessFamily.masterDsp) &&
          accessFamilyAllowed(AccessFamily.channelId) &&
          accessFamilyAllowed(AccessFamily.pan) &&
          control.members.length == 2 &&
          control.members.every(accessTargetAllowed);
    }
    if (control.surface == 'automix_settings') {
      return accessFamilyAllowed(AccessFamily.masterDsp);
    }
    if (control.surface == 'input_insert') {
      return accessFamilyAllowed(AccessFamily.inputDsp) &&
          control.members.every(accessTargetAllowed);
    }
    // Provider descriptor identifies main_return separately from FX globals.
    if (control.surface == 'fx' && control.id == 'main_return') {
      return accessTargetAllowed('main');
    }
    return accessTargetAllowed(control.target) &&
        (control.surface != 'routing' ||
            accessFamilyAllowed(AccessFamily.outputRouting)) &&
        (!control.surface.startsWith('output_') ||
            accessFamilyAllowed(AccessFamily.masterDsp)) &&
        (control.surface != 'fx' || accessFamilyAllowed(AccessFamily.fx));
  }

  @override
  List<AccessLimitOption> get accessOptions {
    final fields = engine.supportedParams;
    final routeDestinations = [
      for (final plane in (engine.routingCaps['planes'] as List? ?? const []))
        if (plane is Map)
          for (final destination
              in (plane['destinations'] as List? ?? const []))
            if (destination is Map) destination,
    ];
    final targets = <String, String>{
      for (final d in routeDestinations)
        if (d['output'] == true && d['key'] is String && d['label'] is String)
          d['key'] as String: d['label'] as String,
      for (final control in engine.destinationControls)
        _targetKey(control.target): control.targetLabel,
      if (engine.mainFader != null || engine.mainMuted != null)
        'main': 'Main LR',
      for (final bus in {...engine.busFaders.keys, ...engine.busMuted.keys})
        'bus/${(bus + 1).toString().padLeft(2, '0')}': 'Bus ${bus + 1}',
      for (var group = 0; group < engine.subgroupCount; group++)
        'subgroup/${(group + 1).toString().padLeft(2, '0')}':
            'Subgroup ${group + 1}',
      for (final strip in engine.strips)
        if (_channelTarget(strip.ch) != null)
          _targetKey(strip.key): strip.name.isEmpty ? strip.key : strip.name,
      for (final field in fields)
        if (RegExp(r'^(aux|fx)([1-9]\d*)_level$').firstMatch(field)
            case final legacy?)
          '${legacy[1] == 'aux' ? 'bus' : 'fx'}/${legacy[2]!.padLeft(2, '0')}':
              '${legacy[1] == 'aux' ? 'Aux' : 'FX'} ${legacy[2]}',
      for (final field in fields)
        if (_sendField.firstMatch(field) case final match?)
          if (_sendTarget(int.parse(match[1]!)) case final target?)
            _targetKey(target): parameterPresentation(
                  engine.parameterMetadata['send${match[1]}_level'],
                  (field) => null,
                )?['destination']?['label'] as String? ??
                target,
    };
    final available = <AccessFamily>{
      if (routeDestinations.any((d) => d['output'] == true))
        AccessFamily.outputRouting,
      if (engine.routingCaps['supported'] == true ||
          fields.any((f) => f != 'source' && _inputRoutingFields.contains(f)))
        AccessFamily.inputRouting,
      if (fields.any(_micPreampFields.contains)) AccessFamily.micPreamps,
      if (fields.any(_isDsp)) AccessFamily.inputDsp,
      if (engine.destinationControls.any(
            (c) => c.surface.startsWith('output_'),
          ) ||
          engine.strips.any((s) => _channelTarget(s.ch) != null) &&
              fields.any(_isDsp))
        AccessFamily.masterDsp,
      if (fields.contains('mute')) AccessFamily.channelMute,
      if (engine.destinationControls.any((r) => r.targetSolo) ||
          engine.solos.isNotEmpty ||
          engine.knownValues.keys.any((ch) => engine.valueKnown(ch, 'solo')))
        AccessFamily.channelSolo,
      if (fields.any((f) => f == 'pan' || f.endsWith('_pan'))) AccessFamily.pan,
      if (fields.any((f) => const ['name', 'color', 'icon'].contains(f)))
        AccessFamily.channelId,
      if (engine.fx.isNotEmpty ||
          engine.destinationControls.any((c) => c.surface == 'fx'))
        AccessFamily.fx,
    };
    final options = <AccessLimitOption>[
      for (final family in AccessFamily.values)
        if (!const {
              AccessFamily.inputRouting,
              AccessFamily.micPreamps,
            }.contains(family) ||
            available.contains(family) ||
            _blockedAccess.contains(family.key))
          AccessLimitOption(
            family.key,
            family.label,
            family.coverage,
            available: available.contains(family),
          ),
      for (final entry in targets.entries)
        AccessLimitOption(
          'target:${entry.key}',
          entry.value,
          'Controls, sends and master selection',
        ),
    ];
    // Keep restrictions editable even when another mixer lacks these identities.
    for (final key in _blockedAccess) {
      if (!options.any((o) => o.key == key)) {
        options.add(
          AccessLimitOption(
            key,
            key.replaceFirst('target:', ''),
            'Saved restriction; not advertised by the current mixer',
            available: false,
          ),
        );
      }
    }
    return List.unmodifiable(options);
  }

  @override
  String _processingTargetForAccessBus(String id) => id.startsWith('solo:')
      ? id.split(':')[1]
      : id == 'main'
          ? 'main'
          : id.startsWith('subgroup')
              ? 'subgroup/${((int.tryParse(id.substring(8)) ?? 0) + 1).toString().padLeft(2, '0')}'
              : 'bus/${((int.tryParse(id.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';

  @override
  Future<void> setAccessBlocked(String key, bool blocked) async {
    if (!accessOptions.any((o) => o.key == key && (o.available || !blocked))) {
      throw StateError('This limit is not available on the current mixer.');
    }
    final next = {...blockedAccess};
    if (blocked) {
      next.add(key);
    } else {
      next.remove(key);
    }
    await PrefsService.instance.setBlockedAccess(next);
  }

  @override
  bool get accessLocked => PrefsService.instance.accessPolicy.locked;
  @override
  bool get accessHasPasscode => PrefsService.instance.accessPolicy.hasPasscode;
  @override
  Future<void> setAccessPasscode(String passcode, String confirmation) =>
      PrefsService.instance.setAccessPasscode(passcode, confirmation);
  @override
  Future<void> lockAccess() => PrefsService.instance.lockAccess();
  @override
  Future<void> unlockAccess(String passcode) =>
      PrefsService.instance.unlockAccess(passcode);
  @override
  Future<void> changeAccessPasscode(
    String current,
    String next,
    String confirmation,
  ) =>
      PrefsService.instance.changeAccessPasscode(current, next, confirmation);
  @override
  Future<void> removeAccessPasscode(String current) =>
      PrefsService.instance.removeAccessPasscode(current);
  @override
  Future<void> setAllAccessBlocked(bool blocked) async {
    final keys = accessOptions
        .where((o) => o.available)
        .map((o) => o.key.startsWith('target:')
            ? 'target:${_targetKey(o.key.substring(7))}'
            : o.key)
        .toSet();
    final next = {...blockedAccess};
    if (blocked) {
      next.addAll(keys);
    } else {
      next.removeWhere((key) => keys.contains(key.startsWith('target:')
          ? 'target:${_targetKey(key.substring(7))}'
          : key));
    }
    await PrefsService.instance.setBlockedAccess(next);
  }

  int _accessRevision = -1;
  void _accessChanged() {
    final prefs = PrefsService.instance;
    final next = prefs.blockedAccess;
    if (setEquals(next, _blockedAccess) &&
        _accessRevision == prefs.accessRevision) {
      return;
    }
    _accessRevision = prefs.accessRevision;
    _blockedAccess = next;
    _controlAccessEpoch++;
    _stopNativeLibrary();
    // Queued writes and captured preset editors must not outlive permission.
    _coalescer.clear();
    _maps.pending.clear();
    _invalidatePresetOperations('Local access limits changed. Reopen presets.');
    _libraryGeneration++;
    _interruptAllLibrary('Local access limits changed. Reopen the library.');
    _interruptDestinationEdits(
      'Local access limits changed. Reopen this control.',
    );
    _interruptAssignmentEdit(
      'Local access limits changed. Reopen Quick Assign.',
    );
    _failNameEdits('Local access limits changed. Reopen the name editor.');
    _invalidateDerived();
    _syncControlNotes();
    notifyListeners();
  }

  bool _accessCommandAllowed(Map command) {
    final cmd = command['cmd'];
    final key = command['key'];
    final resolved = key is String
        ? engine.strips
            .where((s) => _targetKey(s.key) == _targetKey(key))
            .firstOrNull
            ?.ch
        : null;
    final ch = resolved ?? command['ch'];
    if (cmd == 'input_processor') {
      final kind = switch (command['processor']) {
        'eq' => ProcessorPresetKind.eq,
        'gate' => ProcessorPresetKind.gate,
        'compressor' => ProcessorPresetKind.compressor,
        _ => null,
      };
      final processing = command['processor'] == 'channel_processing';
      if (ch is! int ||
          (!processing && kind == null) ||
          _presetFamily(ch) == null) {
        return false;
      }
      final fields = engine.supportedParams
          .where((field) =>
              (processing
                  ? channelProcessingField(field)
                  : processorPresetField(kind!, field)) &&
              _stripParamApplies(ch, field))
          .toList();
      final field = command['field'];
      if (fields.isEmpty ||
          (field != null && (field is! String || !fields.contains(field)))) {
        return false;
      }
      // Raw commands share the same complete-block and partner access gate
      // as preset recall, including captures and unchanged-value checks.
      final pair = engine.stereoPairs.where((p) => p.contains(ch)).firstOrNull;
      return (pair ?? [ch]).every(
        (member) => fields.every((field) => _accessParamAllowed(member, field)),
      );
    }
    if (const [
          'param',
          'param_guarded',
          'fader',
          'mute',
          'solo',
        ].contains(cmd) &&
        ch is int) {
      return _accessParamAllowed(
        ch,
        cmd == 'param' || cmd == 'param_guarded'
            ? command['field'] as String
            : cmd as String,
      );
    }
    if (cmd == 'patch_set' || cmd == 'patch_clear' || cmd == 'route_patch') {
      final destinations = [
        for (final plane in (engine.routingCaps['planes'] as List? ?? const []))
          if (plane is Map)
            for (final d in (plane['destinations'] as List? ?? const []))
              if (d is Map) d,
      ];
      return routingCommandAccessAllowed(
        command,
        destinations,
        familyAllowed: accessFamilyAllowed,
        targetAllowed: accessTargetAllowed,
        membersFor: (strip) {
          final pair =
              engine.stereoPairs.where((p) => p.contains(strip)).firstOrNull;
          return pair == null ||
                  pair.every((ch) => engine.params[ch]?['link'] == false)
              ? [strip]
              : pair;
        },
        memberKey: (member) =>
            engine.strips.where((s) => s.ch == member).firstOrNull?.key,
      );
    }
    if (cmd == 'clear_solo') {
      return canClearSolos;
    }
    if (cmd == 'destination_param') {
      final control = engine.destinationControls
          .where(
            (c) => c.target == command['target'] && c.id == command['control'],
          )
          .firstOrNull;
      return control != null && _accessDestinationAllowed(control);
    }
    if (cmd is String && cmd.startsWith('fx_')) {
      return accessFamilyAllowed(AccessFamily.fx) &&
          accessTargetAllowed(
            'fx/${command['slot'].toString().padLeft(2, '0')}',
          );
    }
    if (const [
      'set_main_fader',
      'main_fader',
      'main_mute',
      'bus_fader',
      'bus_mute',
      'subgroup_fader',
      'subgroup_mute',
      'subgroup_assign',
    ].contains(cmd)) {
      final target = cmd == 'set_main_fader' ||
              cmd.toString().startsWith('main_')
          ? 'main'
          : cmd.toString().startsWith('bus_')
              ? 'bus/${((command['bus'] as int) + 1).toString().padLeft(2, '0')}'
              : 'subgroup/${((command['group'] as int) + 1).toString().padLeft(2, '0')}';
      return accessTargetAllowed(target);
    }
    if (cmd == 'native_library' && _blockedAccess.isNotEmpty) {
      final action = command['action'];
      if (action is! Map ||
          !const ['inspect', 'refresh', 'read_show'].contains(action['op'])) {
        return false;
      }
    }
    // Bulk state changes cannot be safely split around restricted fields.
    if (_blockedAccess.isNotEmpty &&
        const [
          'recall_scene',
          'scene_recall',
          'offline_push',
          'file_upload',
          'local_library_mutate',
        ].contains(cmd)) {
      return false;
    }
    return true;
  }

  @override
  Future<void> _command(String clientId, String json) async {
    if (!PrefsService.instance.loaded) {
      throw StateError(
          'Local access policy is unavailable. Reload preferences first.');
    }
    if (PrefsService.instance.accessSaving) {
      throw StateError('Access policy is changing. Reopen this control.');
    }
    if (_blockedAccess.isEmpty) return _api.cmd(clientId, json);
    final command = jsonDecode(json);
    if (command is Map && !_accessCommandAllowed(command)) {
      throw StateError('Blocked by this device’s local access limits.');
    }
    await _api.cmd(clientId, json);
  }
}
