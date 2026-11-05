part of 'app_store.dart';

// Input source topology, routed preamp context and input delay views.
mixin _AppStoreInput on _AppStoreContext {
  List _routingList(Object? value) => value is List ? value : const [];

  Map? get _inputRouteDestination {
    for (final plane in _routingList(engine.routingCaps['planes'])) {
      if (plane is! Map) continue;
      for (final destination in _routingList(plane['destinations'])) {
        if (destination is Map &&
            destination['key'] == stripKey &&
            destination['strip'] == _channel &&
            destination['output'] != true) {
          return destination;
        }
      }
    }
    return null;
  }

  Map? _inputRoute(Map destination) {
    final route = engine.routing
        .where((route) =>
            route['destination'] == destination['id'] &&
            route['key'] == destination['key'] &&
            route['strip'] == destination['strip'] &&
            (route['target'] ?? '') == destination['target'])
        .firstOrNull;
    if (route == null ||
        route['group'] is! String ||
        route['index'] is! int ||
        route['label'] is! String ||
        route['authority'] != 'provider_display') {
      return null;
    }
    final group = route['group'] as String;
    final index = route['index'] as int;
    final allowed = _routingList(destination['source_groups'])
        .whereType<Map>()
        .any((descriptor) =>
            descriptor['key'] == group &&
            descriptor['count'] is int &&
            index > 0 &&
            index <= descriptor['count']);
    final parts = (route['label'] as String).split(' ');
    return allowed &&
            parts.length == 2 &&
            parts.first == group &&
            int.tryParse(parts.last) == index
        ? route
        : null;
  }

  String _inputSourceLabel(Map group, int index) {
    final options = _routingList(group['options']);
    if (index > 0 && index <= options.length && options[index - 1] is String) {
      return options[index - 1] as String;
    }
    final label = group['label'] as String;
    return group['count'] == 1 ? label : '$label $index';
  }

  @override
  ChannelInputSourceVm? get inputSource {
    final destination = _inputRouteDestination;
    if (destination == null) return null;
    final route = _inputRoute(destination);
    final groups = [
      for (final group in _routingList(destination['source_groups']))
        if (group is Map &&
            group['key'] is String &&
            group['label'] is String &&
            group['count'] is int &&
            group['count'] > 0 &&
            group['count'] <= 255)
          group,
    ];
    final routeGroup = route == null
        ? null
        : groups.where((group) => group['key'] == route['group']).firstOrNull;
    final pair =
        engine.stereoPairs.where((pair) => pair.contains(_channel)).firstOrNull;
    final members = pair == null ||
            pair.every((member) => engine.params[member]?['link'] == false)
        ? [_channel]
        : pair;
    final reason = !engine.connected
        ? 'Not connected'
        : engine.routingCaps['supported'] != true
            ? 'Input routing is not supported by this provider'
            : route == null
                ? 'Waiting for verified source readback'
                : routeGroup == null
                    ? 'No source choices are advertised'
                    : !accessFamilyAllowed(AccessFamily.inputRouting) ||
                            members.any((member) {
                              final key = engine.strips
                                  .where((strip) => strip.ch == member)
                                  .firstOrNull
                                  ?.key;
                              return key == null ||
                                  !accessTargetAllowed(key) ||
                                  engine.locks.containsKey(member);
                            })
                        ? 'Limited on this device'
                        : null;
    final context = (
      engine.session,
      controlAccessEpoch,
      engine.driver,
      engine.routingCaps['epoch'],
      destination['id'],
      destination['key'],
      destination['strip'],
      destination['target'],
      route?['group'],
      route?['index'],
      route?['label'],
      route?['guard'],
    );
    return ChannelInputSourceVm(
        context: context,
        label: route == null
            ? source.srcName
            : routeGroup == null
                ? source.srcName
                : _inputSourceLabel(routeGroup, route['index'] as int),
        choices: List.unmodifiable([
          for (final group in groups)
            for (var index = 1; index <= (group['count'] as int); index++)
              ChannelInputSourceChoice(
                  group: group['key'] as String,
                  index: index,
                  label: _inputSourceLabel(group, index),
                  selected: route?['group'] == group['key'] &&
                      route?['index'] == index),
        ]),
        editable: reason == null,
        unavailableReason: reason);
  }

  @override
  void setInputSource(
      ChannelInputSourceVm snapshot, ChannelInputSourceChoice choice) {
    final current = inputSource;
    final destination = _inputRouteDestination;
    final route = destination == null ? null : _inputRoute(destination);
    if (current == null ||
        !current.editable ||
        current.context != snapshot.context ||
        destination == null ||
        route == null ||
        choice.selected ||
        !current.choices.any((candidate) =>
            candidate.group == choice.group &&
            candidate.index == choice.index)) {
      return;
    }
    _send(jsonEncode({
      'cmd': 'route_patch',
      'driver': engine.driver,
      'epoch': engine.routingCaps['epoch'],
      'destination': destination['id'],
      'expected': route['label'],
      if (route['guard'] != null) 'guard': route['guard'],
      'source': {'group': choice.group, 'index': choice.index},
    }));
  }

  Map<String, dynamic>? _activePreampStateFor(int channel) {
    final leaf = mixerProviderPolicy(engine.driver).preampStateLeaf;
    if (leaf == null) return null;
    final active = ps.optimisticValue(_maps, channel, 'source');
    final raw = ps.optimisticValue(_maps, channel, leaf);
    if (!engine.valueKnown(channel, 'source') ||
        !engine.valueKnown(channel, leaf) ||
        active is! String ||
        raw is! String) {
      return null;
    }
    try {
      final state = jsonDecode(raw);
      if (state is! Map<String, dynamic> ||
          state['source'] != active ||
          state['gain_available'] is! bool ||
          state['phantom_available'] is! bool) {
        return null;
      }
      if (state['gain_available'] == true &&
          (state['gain_min'] is! num ||
              state['gain_max'] is! num ||
              (state['gain_min'] as num) >= (state['gain_max'] as num))) {
        return null;
      }
      return state;
    } catch (_) {
      return null;
    }
  }

  Map<String, dynamic>? get _activePreampState =>
      _activePreampStateFor(_channel);

  bool? preampPhantomAvailableFor(int channel) =>
      _activePreampStateFor(channel)?['phantom_available'] as bool?;

  List<double>? preampGainRangeFor(int channel) {
    final state = _activePreampStateFor(channel);
    if (state == null || state['gain_available'] != true) return null;
    return [
      (state['gain_min'] as num).toDouble(),
      (state['gain_max'] as num).toDouble()
    ];
  }

  @override
  bool? get preampGainAvailable =>
      _activePreampState?['gain_available'] as bool?;
  @override
  bool? get preampPhantomAvailable =>
      _activePreampState?['phantom_available'] as bool?;
  @override
  List<double>? get preampGainRange {
    return preampGainRangeFor(_channel);
  }

  @override
  ChannelSourceModeVm? get sourceMode {
    final raw = ps.optimisticValue(
        _maps, _channel, _providerLeaf(engine, 'provider:source_mode'));
    final decoded = mixerProviderPolicy(engine.driver).decodeSourceMode(raw);
    if (decoded == null ||
        source.srcName != '${decoded.group} ${decoded.port}') {
      return null;
    }
    var label = '${decoded.group} ${decoded.port}';
    final groups = engine.routingCaps['groups'];
    if (groups is List) {
      for (final group in groups) {
        if (group is Map &&
            group['key'] == decoded.group &&
            group['label'] is String) {
          label = '${group['label']} ${decoded.port}';
          break;
        }
      }
    }
    final choices = mixerProviderPolicy(engine.driver).sourceModeChoices;
    if (!choices.any((choice) => choice.value == decoded.mode)) return null;
    final editable = choices.isNotEmpty &&
        supports('provider:source_mode') &&
        paramBool('provider:source_mode_writable') == true &&
        supports('provider:source_mode_writable');
    return ChannelSourceModeVm(
        context: (
          engine.session,
          controlAccessEpoch,
          _channel,
          source.sourceA,
          source.sourceB,
          source.selected,
          raw,
          editable
        ),
        target: decoded.target,
        sourceLabel: label,
        mode: decoded.mode,
        choices: choices,
        editable: editable);
  }

  @override
  void setSourceMode(ChannelSourceModeVm snapshot, int mode) {
    final current = sourceMode;
    if (current == null ||
        !current.editable ||
        current.context != snapshot.context ||
        current.mode == mode ||
        !current.choices.any((choice) => choice.value == mode)) {
      return;
    }
    final encoded =
        mixerProviderPolicy(engine.driver).encodeSourceMode(snapshot, mode);
    if (encoded != null) cmdParam('provider:source_mode', encoded);
  }

  @override
  bool get hasDelay {
    if (!mixerProviderPolicy(engine.driver).supportsDelayCodec) return false;
    final scopes =
        engine.parameterMetadata[_providerLeaf(engine, 'provider:delay_state')]
            ?['scopes'];
    final family = _presetFamily(_channel);
    final tags = family?['params_scope'];
    final kinds = tags is List ? tags : [family?['kind']];
    return engine.supportedParams
            .contains(_providerLeaf(engine, 'provider:delay_state')) &&
        scopes is List &&
        scopes.any(kinds.contains);
  }

  @override
  ChannelDelayVm? get delay {
    final raw = ps.optimisticValue(
        _maps, _channel, _providerLeaf(engine, 'provider:delay_state'));
    final report = mixerProviderPolicy(engine.driver)
        .decodeDelay(raw, paramNum('provider:delay_unit'));
    if (report == null) return null;
    final unit = mixerProviderPolicy(engine.driver).delayUnit(report.unit);
    if (unit == null) return null;
    return ChannelDelayVm(
        context: (
          engine.session,
          _channel,
          report.unit,
          report.min,
          report.max,
          report.steps
        ),
        unit: report.unit,
        unitLabel: unit.label,
        precision: unit.precision,
        steps: report.steps,
        value: report.value.toDouble(),
        min: report.min.toDouble(),
        max: report.max.toDouble(),
        on: report.on,
        editable: report.editable && supports('provider:delay_value'));
  }

  @override
  void setDelay(ChannelDelayVm snapshot, double value) {
    final live = delay;
    if (live == null ||
        !live.editable ||
        live.context != snapshot.context ||
        !value.isFinite ||
        value < live.min ||
        value > live.max) {
      return;
    }
    final encoded = mixerProviderPolicy(engine.driver).encodeDelay(live, value);
    if (encoded != null) cmdParam('provider:delay_value', encoded);
  }

  @override
  bool get canPatchInputSource {
    final planes = engine.routingCaps['planes'];
    if (planes is! List) return false;
    for (final plane in planes) {
      if (plane is! Map) continue;
      final destinations = plane['destinations'];
      if (destinations is! List) continue;
      for (final destination in destinations) {
        if (destination is Map && destination['key'] == stripKey) {
          return true;
        }
      }
    }
    return false;
  }

  @override
  ChannelPreampVm get preamp => ChannelPreampVm(
        hasInput: supports('gain'),
        gain: paramNum('gain') ?? 0,
        trim: paramNum('trim') ?? 0,
        phantom: paramBool('phantom') ?? false,
        invert: paramBool('polarity') ?? false,
        delayMs: paramNum('delay_ms') ?? 0,
        delayOn: paramBool('delay_on') ?? false,
      );

  @override
  ChannelSourceVm get source {
    final raw = ps.optimisticValue(_maps, _channel, 'source');
    final selected = paramNum('source_select')?.toInt();
    return ChannelSourceVm(
      // Some providers expose a source field without a descriptive value.
      // Do not infer a physical route from the strip number or seed defaults.
      srcName:
          raw is String && raw.trim().isNotEmpty ? raw : 'Source unavailable',
      sourceA: paramNum('source_a')?.toInt(),
      sourceB: paramNum('source_b')?.toInt(),
      // The provider converts its selector into the displayed A/B index.
      selected: selected == null
          ? null
          : mixerProviderPolicy(engine.driver).sourceSelection(selected),
      usbReturn: paramNum('usbreturn')?.toInt(),
      levelDb: _levelDb.value,
    );
  }

  // A/B selects the active slot; the Routing page edits each slot separately.

  @override
  void setSourceSelected(int selected) {
    if (selected == 0 || selected == 1) {
      cmdParam('source_select',
          mixerProviderPolicy(engine.driver).sourceSelection(selected));
    }
  }
}
