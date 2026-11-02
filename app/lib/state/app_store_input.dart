part of 'app_store.dart';

// Input source topology, routed preamp context and input delay views.
mixin _AppStoreInput on _AppStoreContext {
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
    if (raw is! String) return null;
    final parts = raw.split(':');
    if (parts.length != 2) return null;
    final mode = const ['M', 'ST', 'M/S'].indexOf(parts[1]);
    final target = parts[0];
    final port = target.split('/');
    if (mode < 0 ||
        port.length != 2 ||
        int.tryParse(port[1]) == null ||
        source.srcName != '${port[0]} ${port[1]}') {
      return null;
    }
    var label = '${port[0]} ${port[1]}';
    final groups = engine.routingCaps['groups'];
    if (groups is List) {
      for (final group in groups) {
        if (group is Map &&
            group['key'] == port[0] &&
            group['label'] is String) {
          label = '${group['label']} ${port[1]}';
          break;
        }
      }
    }
    final editable = supports('provider:source_mode') &&
        paramBool('provider:source_mode_writable') == true &&
        supports('provider:source_mode_writable');
    return ChannelSourceModeVm(context: (
      engine.session,
      controlAccessEpoch,
      _channel,
      source.sourceA,
      source.sourceB,
      source.selected,
      raw,
      editable
    ), target: target, sourceLabel: label, mode: mode, editable: editable);
  }

  @override
  void setSourceMode(ChannelSourceModeVm snapshot, int mode) {
    final current = sourceMode;
    if (mode < 0 ||
        mode > 2 ||
        current == null ||
        !current.editable ||
        current.context != snapshot.context ||
        current.mode == mode) {
      return;
    }
    final code = const ['M', 'ST', 'M/S'][mode];
    cmdParam(
        'provider:source_mode', '${snapshot.target}:${snapshot.code}>$code');
  }

  @override
  bool get hasDelay {
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
    if (raw is! String) return null;
    try {
      final data = jsonDecode(raw);
      if (data is! Map) return null;
      final unit = data['unit'],
          value = data['value'],
          min = data['min'],
          max = data['max'],
          steps = data['steps'];
      if (unit is! int ||
          unit < 0 ||
          unit > 3 ||
          value is! num ||
          min is! num ||
          max is! num ||
          steps is! int ||
          steps < 0 ||
          !value.isFinite ||
          !min.isFinite ||
          !max.isFinite ||
          min >= max ||
          value < min ||
          value > max ||
          data['on'] is! bool ||
          data['editable'] is! bool ||
          paramNum('provider:delay_unit') != unit) {
        return null;
      }
      return ChannelDelayVm(
          context: (engine.session, _channel, unit, min, max, steps),
          unit: unit,
          steps: steps,
          value: value.toDouble(),
          min: min.toDouble(),
          max: max.toDouble(),
          on: data['on'] as bool,
          editable:
              data['editable'] == true && supports('provider:delay_value'));
    } catch (_) {
      return null;
    }
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
    final step =
        live.steps > 1 ? (live.max - live.min) / (live.steps - 1) : 0.0;
    final snapped = step > 0
        ? (live.min + ((value - live.min) / step).round() * step)
            .clamp(live.min, live.max)
        : value;
    cmdParam('provider:delay_value', '${snapshot.code}:$snapped');
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
      // DL32S wire selector is 1=A / 0=B, verified against the desk.
      // The view model uses the displayed A/B index, 0=A / 1=B.
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
