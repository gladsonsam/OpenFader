part of 'app_store.dart';

// Selected channel processing, assignments and sends.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreChannelContract {
  int get _channel;
  set _focusCh(int? value);
  bool get locked;
}

mixin _AppStoreChannel on _AppStoreContext {
  // --- channel page state ---
  @override
  int _channel = 0;
  ChannelTab _tab = ChannelTab.input;
  int _eqBand = 1;
  @override
  int? _focusCh;

  // ------------------------------------------------------------------
  // ChannelStore
  // ------------------------------------------------------------------
  @override
  List<DestinationControlVm> get rtaSettings => List.unmodifiable(
      engine.destinationControls.where((row) => row.surface == 'output_rta'));

  @override
  List<DestinationControlVm> get inputInserts => List.unmodifiable(engine
      .destinationControls
      .where((row) => row.surface == 'input_insert' && row.target == stripKey));

  @override
  ChannelInsertVm? get inputInsert {
    final controls = inputInserts;
    final enabled = controls
        .where((control) =>
            control.choices.length == 2 &&
            control.choices.first.label.toLowerCase() == 'off' &&
            control.choices.last.label.toLowerCase() == 'on')
        .firstOrNull;
    final source = controls
        .where((control) =>
            control.choices.length > 2 &&
            control.choices.first.label.toLowerCase() == 'off')
        .firstOrNull;
    if (enabled == null ||
        source == null ||
        enabled.value == null ||
        source.value == null) {
      return null;
    }
    final editable = destinationControlIsCurrent(enabled) &&
        destinationControlIsCurrent(source);
    return ChannelInsertVm(
        context: (
          connectionEpoch,
          controlAccessEpoch,
          enabled.revision,
          enabled.generation,
          enabled.value,
          source.revision,
          source.generation,
          source.value,
        ),
        label: enabled.value == enabled.choices.first.value
            ? enabled.choices.first.label
            : source.valueLabel ?? 'Unavailable',
        choices: source.choices,
        value: enabled.value == enabled.choices.first.value
            ? source.choices.first.value
            : source.value!,
        editable: editable,
        unavailableReason: editable ? null : 'Insert control unavailable');
  }

  @override
  Future<String?> setInputInsert(ChannelInsertVm snapshot, double value) async {
    var live = inputInsert;
    if (live == null ||
        !live.editable ||
        live.context != snapshot.context ||
        !live.choices.any((choice) => choice.value == value)) {
      return 'Insert changed; reopen the control.';
    }
    var controls = inputInserts;
    var enabled = controls
        .where((control) =>
            control.choices.length == 2 &&
            control.choices.first.label.toLowerCase() == 'off' &&
            control.choices.last.label.toLowerCase() == 'on')
        .first;
    var source = controls
        .where((control) =>
            control.choices.length > 2 &&
            control.choices.first.label.toLowerCase() == 'off')
        .first;
    final off = value == live.choices.first.value;
    if (off) return setDestinationControl(enabled, enabled.choices.first.value);
    if (source.value != value) {
      final error = await setDestinationControl(source, value);
      if (error != null) return error;
      controls = inputInserts;
      enabled =
          controls.where((control) => control.id == enabled.id).firstOrNull ??
              enabled;
      source =
          controls.where((control) => control.id == source.id).firstOrNull ??
              source;
      if (source.value != value || !destinationControlIsCurrent(enabled)) {
        return 'Insert source readback changed; reopen the control.';
      }
    }
    return enabled.value == enabled.choices.last.value
        ? null
        : setDestinationControl(enabled, enabled.choices.last.value);
  }

  @override
  List<DestinationControlVm> get automixSettings =>
      List.unmodifiable(engine.destinationControls
          .where((row) => row.surface == 'automix_settings'));

  // Current open channel index.

  int get channelIndex => _channel;

  // Select the open channel on shell navigation.

  void openChannel(int ch) {
    final wasOutput = outputProcessingTarget != null;
    _outputProcessingTarget = null;
    if (_channel == ch) {
      if (wasOutput) notifyListeners();
      return;
    }
    _channel = ch;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    cmdFocus();
    _invalidateDerived(); // sends are channel-scoped (sendN_on)
    notifyListeners();
  }

  @override
  String get stripKey =>
      engine.strips.where((s) => s.ch == _channel).firstOrNull?.key ??
      'ch/${(_channel + 1).toString().padLeft(2, '0')}';
  @override
  int get connectionEpoch => engine.session;

  @override
  String get displayName {
    if (stripKey == 'main') return 'Main LR';
    for (final s in engine.strips) {
      if (s.ch == _channel && s.name.isNotEmpty) return s.name;
    }
    return 'CH${(_channel + 1).toString().padLeft(2, '0')}';
  }

  @override
  String get channelNumber => stripKey == 'main' ? 'LR' : '${_channel + 1}';

  @override
  ValueNotifier<double> faderNote() => faderOf(_channel);

  @override
  void setFaderDb(double db) => setFader(_channel, dbToPos(db));

  @override
  List<double>? get faderEntryRange {
    // Simulation explicitly uses the UI law. Unknown native laws stay disabled.
    if (engine.driver == 'sim') return [-90, 10];
    final law = lawForDriver(engine.driver);
    if (law == null) return null;
    return [
      law.floorDb.clamp(-90, 10).toDouble(),
      law.topDb.clamp(-90, 10).toDouble()
    ];
  }

  @override
  double? paramNum(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, _providerLeaf(engine, leaf));
    return v is num ? v.toDouble() : null;
  }

  @override
  bool? paramBool(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, _providerLeaf(engine, leaf));
    return v is bool ? v : null;
  }

  @override
  bool get capabilitiesReady => engine.capsReady;

  @override
  Map<String, dynamic>? paramMetadata(String leaf) {
    final desk = _providerLeaf(engine, leaf);
    final metadata =
        parameterPresentation(engine.parameterMetadata[desk], paramNum);
    if (desk == _providerLeaf(engine, 'provider:delay_unit') &&
        metadata != null) {
      final choices = metadata['choices'];
      final policy = mixerProviderPolicy(engine.driver);
      if (choices is! List || !policy.supportsDelayCodec) {
        return {...metadata, 'choices': const [], 'available': false};
      }
      final supported = choices.where((choice) {
        if (choice is! Map || choice['value'] is! num) return false;
        final value = choice['value'] as num;
        return value.isFinite &&
            value == value.roundToDouble() &&
            policy.delayUnit(value.toInt()) != null;
      }).toList(growable: false);
      return {
        ...metadata,
        'choices': supported,
        if (supported.isEmpty) 'available': false
      };
    }
    if (desk == 'gain' &&
        mixerProviderPolicy(engine.driver).requiresRoutedPreamp &&
        metadata != null) {
      final range = preampGainRange;
      if (range == null) return {...metadata, 'available': false};
      return {...metadata, 'min': range.first, 'max': range.last};
    }
    return metadata;
  }

  @override
  bool paramApplies(String leaf) =>
      _stripParamApplies(_channel, _providerLeaf(engine, leaf));

  @override
  String? localAccessMessage(String leaf) =>
      _accessParamAllowed(_channel, _providerLeaf(engine, leaf))
          ? null
          : 'Limited on this device';

  @override
  bool supports(String leaf) {
    final desk = _providerLeaf(engine, leaf);
    if (!paramApplies(leaf)) return false;
    if (mixerProviderPolicy(engine.driver).requiresRoutedPreamp &&
        (desk == 'gain' || desk == 'phantom')) {
      if ((desk == 'gain' ? preampGainAvailable : preampPhantomAvailable) !=
          true) {
        return false;
      }
    }
    if (!_accessParamAllowed(_channel, desk) ||
        !engine.capsReady ||
        _presetEditingBlocked(_channel, desk)) {
      return false;
    }
    final presentation = paramMetadata(desk);
    if (presentation?['hidden'] == true ||
        presentation?['available'] == false) {
      return false;
    }
    final sendPan = RegExp(r'^send([1-9]\d*)_pan$').firstMatch(desk);
    if (sendPan != null &&
        !_sendDestinationIsStereoAux(int.parse(sendPan[1]!))) {
      return false;
    }
    final cutControl = RegExp(r'^eq([1-6])_(gain|q)$').firstMatch(desk);
    if (cutControl != null && _eqFixedCut(int.parse(cutControl[1]!))) {
      return false;
    }
    // Fixed-slope shelves retain the bell Q but cannot edit it, including
    // through pinch or a previously focused numeric readout.
    final eqQ = RegExp(r'^eq([1-6])_q$').firstMatch(desk);
    if (eqQ != null) {
      final band = int.parse(eqQ[1]!);
      final shape = EqFilterShape.fromValue(_eqPresentationType(band));
      if (!mixerProviderPolicy(engine.driver).eqQEditable(shape) ||
          paramMetadata('eq${band}_type')?['curve'] == 'fixed_shelf' &&
              (shape == EqFilterShape.lowShelf ||
                  shape == EqFilterShape.highShelf)) {
        return false;
      }
    }
    return engine.supportedParams.contains(desk) &&
        (mixerProviderPolicy(engine.driver).offlineValues ||
            (engine.connected && engine.valueKnown(_channel, desk)));
  }

  Map<String, Object> _preampExpect(int channel) => {
        for (final field in [
          'source_a',
          'source_b',
          'source_select',
          'source',
          'usbreturn',
          if (mixerProviderPolicy(engine.driver).preampStateLeaf != null)
            mixerProviderPolicy(engine.driver).preampStateLeaf!,
        ])
          if (engine.supportedParams.contains(field) &&
              ps.optimisticValue(_maps, channel, field) != null)
            field: ps.optimisticValue(_maps, channel, field)!,
      };

  @override
  void cmdParam(String leaf, Object value) {
    final desk = _providerLeaf(engine, leaf);
    if (desk == 'name') return; // Use the captured-target rename contract.
    if (!supports(desk)) return;
    if (value is num && !value.isFinite) return;
    final choices = parameterChoices(paramMetadata(desk));
    if (choices.isNotEmpty &&
        (value is! num ||
            !choices.any((c) => (c.value - value).abs() < .0001))) {
      return;
    }
    // DSP edits (including delayed ChannelWrites flushes) use the current
    // driver limits. Hydration remains untouched, even outside these limits.
    final isDspRange = desk == 'hpf_freq' ||
        mixerProviderPolicy(engine.driver).isProviderDspRange(desk) ||
        RegExp(r'^eq[1-8]_(freq|gain|q)$').hasMatch(desk) ||
        RegExp(r'^(gate|comp)_(thr|range|att|hold|rel|ratio|knee|makeup|filter_freq|mix)$')
            .hasMatch(desk);
    final range = isDspRange ? _editableRange(desk) : null;
    if (isDspRange && (range == null || value is! num)) return;
    var coerced = range == null
        ? value
        : (value as num).toDouble().clamp(range.first, range.last);
    coerced =
        mixerProviderPolicy(engine.driver).quantizeParameter(desk, coerced);
    // Capture at enqueue time: navigation must not retarget a queued edit.
    final ch = _channel;
    _maps.confirmed.putIfAbsent(ch, () => {});
    ps.noteOptimistic(_maps, ch, desk, coerced, _nowMs());
    final epoch = engine.session;
    final expected = desk == 'gain' || desk == 'phantom'
        ? _preampExpect(ch)
        : <String, Object>{};
    _coalescer.offer('param:$ch:$desk', () {
      if (engine.session != epoch) return;
      if (_maps.pending[ps.pendingKey(ch, desk)]?.value != coerced) return;
      if (expected.isNotEmpty &&
          (engine.session != epoch ||
              expected.entries.any(
                  (e) => ps.optimisticValue(_maps, ch, e.key) != e.value))) {
        if (_maps.pending[ps.pendingKey(ch, desk)]?.value == coerced) {
          _maps.pending.remove(ps.pendingKey(ch, desk));
        }
        return;
      }
      _send(expected.isEmpty
          ? Cmds.param(ch, desk, coerced)
          : Cmds.paramGuarded(ch, desk, coerced, expected));
    });
    _invalidateDerived(); // optimistic send reads feed sends
    if (_AppStorePoll._sendHotField.hasMatch(desk)) {
      _syncSendParamNotes();
    } else {
      notifyListeners();
    }
  }

  @override
  void cmdFocus() {
    _focusCh = _channel;
    // Processing readiness requires independent link readback for both
    // members, even while unlinked. Use provider identities (Return pairs
    // need not start at an even strip ID), and keep the open channel first.
    final pair =
        engine.stereoPairs.where((p) => p.contains(_channel)).firstOrNull;
    _send(Cmds.focus([
      _channel,
      if (pair != null) ...pair.where((ch) => ch != _channel),
    ]));
  }

  @override
  void cmdLock() => _send(Cmds.lock(_channel));

  @override
  void cmdUnlock() => _send(Cmds.unlock(_channel));

  @override
  void cmdSubgroupAssign(int g, bool on) {
    if (!engine.connected ||
        g < 0 ||
        g >= subgroupCount ||
        subgroupAssigned(g) == null) {
      return;
    }
    engine.subgroupAssign['$_channel:$g'] = on;
    _send(Cmds.subgroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void cmdMuteGroupAssign(int g, bool on) {
    if (!engine.connected ||
        g < 0 ||
        g >= muteGroupCount ||
        muteGroupAssigned(g) == null) {
      return;
    }
    engine.muteGroupAssign['$_channel:$g'] = on;
    _send(Cmds.muteGroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  // Providers may declare a fixed stereo destination; other desks need a live link value.

  bool _sendDestinationIsStereoAux(int slot) {
    if (slot < 1) return false;
    final destination = paramMetadata('send${slot}_level')?['destination'];
    if (destination is Map && destination['stereo'] == true) {
      return true;
    }
    for (final family in engine.topology) {
      if (family['kind'] != 'bus') continue;
      final first = family['first'];
      final count = family['count'];
      if (first is! num || count is! num || slot > count.toInt()) continue;
      final destination = first.toInt() + slot - 1;
      return engine.valueKnown(destination, 'link') &&
          engine.params[destination]?['link'] == true;
    }
    return false;
  }

  double? sendPan(int slot) {
    if (!supports('send${slot}_pan')) return null;
    final value = paramNum('send${slot}_pan');
    return value == null ? null : value.clamp(-100, 100) / 100;
  }

  @override
  void setSendPan(int slot, double pan) =>
      cmdParam('send${slot}_pan', pan.clamp(-1.0, 1.0) * 100);

  @override
  bool? get mainAssigned =>
      supports('lr_assign') ? paramBool('lr_assign') : null;
  @override
  void setMainAssigned(bool on) => cmdParam('lr_assign', on);
  @override
  int get dcaCount => _assignmentCount('dca');

  int _assignmentCount(String family) {
    final pattern = RegExp('^$family([1-9][0-9]*)_assign\$');
    return engine.supportedParams
        .map((id) => int.tryParse(pattern.firstMatch(id)?.group(1) ?? ''))
        .whereType<int>()
        .where((n) => paramApplies('$family${n}_assign'))
        .fold(0, math.max);
  }

  @override
  bool? dcaAssigned(int group) => supports('dca${group + 1}_assign')
      ? paramBool('dca${group + 1}_assign')
      : null;
  @override
  String dcaGroupLabel(int group) {
    for (final family in engine.topology) {
      if (family['kind'] == 'dca' && family['name_prefix'] is String) {
        return '${family['name_prefix']} ${group + 1}';
      }
    }
    final label =
        paramMetadata('dca${group + 1}_assign')?['label'] as String? ??
            'VCA ${group + 1}';
    final match =
        RegExp(r'^dca(\s*\d+)$', caseSensitive: false).firstMatch(label);
    return match == null ? label : 'DCA${match[1]}';
  }

  @override
  void setDcaAssigned(int group, bool on) =>
      cmdParam('dca${group + 1}_assign', on);

  @override
  int get viewGroupCount => _assignmentCount('view');
  @override
  bool? viewGroupAssigned(int group) => supports('view${group + 1}_assign')
      ? paramBool('view${group + 1}_assign')
      : null;
  @override
  String viewGroupLabel(int group) =>
      paramMetadata('view${group + 1}_assign')?['label'] as String? ??
      'View ${group + 1}';
  @override
  void setViewGroupAssigned(int group, bool on) =>
      cmdParam('view${group + 1}_assign', on);

  @override
  void setCompSoftKnee(bool soft) =>
      cmdParam('comp_knee', mixerProviderPolicy(engine.driver).compKnee(soft));

  List<double>? eqPreviewFor(int channel) {
    Object? value(String leaf) =>
        ps.optimisticValue(_maps, channel, _providerLeaf(engine, leaf));
    double? number(String leaf) {
      final v = value(leaf);
      return v is num && v.isFinite ? v.toDouble() : null;
    }

    bool applies(String leaf) =>
        _stripParamApplies(channel, _providerLeaf(engine, leaf));
    final on = value('eq_on');
    if (on is! bool) return null;
    if (!on) return eqResponseDb(const ChannelEqVm(on: false));
    final count = mixerProviderPolicy(engine.driver).eqBandCount(
        mixerProviderPolicy(engine.driver).eqBandCountLeaf == null
            ? null
            : number(mixerProviderPolicy(engine.driver).eqBandCountLeaf!),
        applies('eq6_gain'));
    final cutModel = mixerProviderPolicy(engine.driver).cutModel;
    final bands = <EqBandVm>[];
    for (var b = 1; b <= count; b++) {
      final bandOn = value('eq${b}_on');
      if (applies('eq${b}_on') && bandOn is! bool) return null;
      final native = number('eq${b}_type')?.toInt();
      final type =
          mixerProviderPolicy(engine.driver).eqPresentationType(native);
      final shape = EqFilterShape.fromValue(type);
      final f = number('eq${b}_freq');
      if (shape?.hasResponseModel(
                  cutModel: cutModel, bellModel: _eqBellModel) !=
              true ||
          f == null ||
          f <= 0) {
        return null;
      }
      final fixedCut =
          shape == EqFilterShape.lowCut || shape == EqFilterShape.highCut;
      final g = number('eq${b}_gain');
      final q = number('eq${b}_q');
      if (!fixedCut && (g == null || q == null || q <= 0)) return null;
      final metadata = parameterPresentation(
          engine.parameterMetadata['eq${b}_type'], number);
      bands.add(EqBandVm(
          band: b,
          name: '',
          on: applies('eq${b}_on') && bandOn is bool ? bandOn : true,
          f: f,
          g: g ?? 0,
          q: q ?? 1,
          type: type,
          cutModel: cutModel,
          bellModel: _eqBellModel,
          shelfModel: mixerProviderPolicy(engine.driver).shelfModel,
          shelfUsesQ: mixerProviderPolicy(engine.driver).shelfUsesQ &&
              metadata?['curve'] != 'fixed_shelf'));
    }
    var hpfOn = false;
    var hpfF = 80.0;
    if (mixerProviderPolicy(engine.driver).hpfInResponse && applies('hpf_on')) {
      final enabled = value('hpf_on');
      if (enabled is! bool) return null;
      hpfOn = enabled;
      if (hpfOn) {
        final f = number('hpf_freq');
        if (f == null || f <= 0) return null;
        hpfF = f;
      }
    }
    return eqResponseDb(ChannelEqVm(
        on: true,
        hpfOn: hpfOn,
        hpfF: hpfF,
        hpfInResponse: mixerProviderPolicy(engine.driver).hpfInResponse,
        bands: bands));
  }

  int? _eqPresentationType(int band) {
    final native = paramNum('eq${band}_type')?.toInt();
    return mixerProviderPolicy(engine.driver).eqPresentationType(native);
  }

  List<ParameterChoice> _eqTypeChoices(int band) {
    final choices = parameterChoices(paramMetadata('eq${band}_type'));
    if (choices.isNotEmpty &&
        mixerProviderPolicy(engine.driver).useNativeEqChoices) {
      return choices;
    }
    if (band == 1 || band == _eqBandCount) {
      return [
        (
          value: (band == 1 ? EqFilterShape.lowShelf : EqFilterShape.highShelf)
              .value
              .toDouble(),
          label: 'Shelf'
        ),
        (value: EqFilterShape.bell.value.toDouble(), label: 'Bell'),
      ];
    }
    return const [];
  }

  EqCutModel get _eqCutModel => mixerProviderPolicy(engine.driver).cutModel;

  EqBellModel get _eqBellModel => mixerProviderPolicy(engine.driver).bellModel;

  bool _eqFixedCut(int band) =>
      _eqCutModel == EqCutModel.xairButterworth96k &&
      const [0, 5].contains(_eqPresentationType(band));

  String? get _eqResponseNote {
    final notes = [
      if (paramApplies('provider:filter_model'))
        'EQ curve excludes input filters.',
      if (List.generate(_eqBandCount, (i) => _eqPresentationType(i + 1)).any(
          (type) =>
              EqFilterShape.fromValue(type)?.hasResponseModel(
                  cutModel: _eqCutModel, bellModel: _eqBellModel) !=
              true))
        'EQ curve excludes bands with unavailable response models.',
    ];
    return notes.isEmpty ? null : notes.join(' ');
  }

  List<double>? _editableRange(String leaf) {
    if (!supports(leaf) || paramNum(leaf)?.isFinite != true) return null;
    return parameterRange(paramMetadata(leaf));
  }

  int get _eqBandCount => mixerProviderPolicy(engine.driver).eqBandCount(
      mixerProviderPolicy(engine.driver).eqBandCountLeaf == null
          ? null
          : paramNum(mixerProviderPolicy(engine.driver).eqBandCountLeaf!),
      paramApplies('eq6_gain'));

  @override
  ChannelEqVm get eq => ChannelEqVm(
        on: paramBool('eq_on') ?? false,
        hpfF: paramNum('hpf_freq') ?? 80,
        hpfRange: _editableRange('hpf_freq'),
        hpfOn: paramBool('hpf_on') ?? false,
        hpfSlope:
            mixerProviderPolicy(engine.driver).hpfSlope(paramNum('hpf_slope')),
        hpfInResponse: mixerProviderPolicy(engine.driver).hpfInResponse,
        responseNote: _eqResponseNote,
        selectedBand: _eqBand,
        bands: [
          for (var b = 1; b <= _eqBandCount; b++)
            EqBandVm(
              band: b,
              name: _eqBandCount == 8
                  ? (b == 1
                      ? 'Low'
                      : b == 8
                          ? 'High'
                          : 'Mid ${b - 1}')
                  : _eqBandCount == 6
                      ? const [
                          'Low',
                          'Lo-Mid 1',
                          'Lo-Mid 2',
                          'Hi-Mid 1',
                          'Hi-Mid 2',
                          'High'
                        ][b - 1]
                      : const ['Low', 'Lo-Mid', 'Hi-Mid', 'High'][b - 1],
              f: paramNum('eq${b}_freq') ??
                  (_eqBandCount == 8
                      ? [
                          80.0,
                          160.0,
                          320.0,
                          640.0,
                          1250.0,
                          2500.0,
                          5000.0,
                          10000.0
                        ]
                      : _eqBandCount == 6
                          ? [80.0, 250.0, 600.0, 1500.0, 4000.0, 10000.0]
                          : [80.0, 400.0, 1200.0, 6000.0])[b - 1],
              g: paramNum('eq${b}_gain') ?? 0,
              q: paramNum('eq${b}_q') ?? 1,
              cutModel: _eqCutModel,
              bellModel: _eqBellModel,
              shelfModel: mixerProviderPolicy(engine.driver).shelfModel,
              shelfUsesQ: mixerProviderPolicy(engine.driver).shelfUsesQ &&
                  paramMetadata('eq${b}_type')?['curve'] != 'fixed_shelf',
              on: paramApplies('eq${b}_on')
                  ? paramBool('eq${b}_on') ?? true
                  : true,
              type: _eqPresentationType(b),
              typeControlValue:
                  mixerProviderPolicy(engine.driver).useNativeEqChoices
                      ? paramNum('eq${b}_type')?.toInt()
                      : _eqPresentationType(b),
              typeChoices: _eqTypeChoices(b),
              fRange: _editableRange('eq${b}_freq'),
              gRange: _eqFixedCut(b) ? null : _editableRange('eq${b}_gain'),
              qRange: _eqFixedCut(b) ? null : _editableRange('eq${b}_q'),
            ),
        ],
      );

  ChannelDynVm _dyn(DynKind kind) {
    final p = kind == DynKind.gate ? 'gate' : 'comp';
    return ChannelDynVm(
      kind: kind,
      responseCurve: (paramMetadata('${p}_knee')?['curve'] ??
          paramMetadata('${p}_mode')?['curve']) as String?,
      ranges: {
        for (final prop in [
          'thr',
          'range',
          'att',
          'hold',
          'rel',
          'ratio',
          'knee',
          'makeup',
          'filter_freq',
          'mix'
        ])
          if (_editableRange('${p}_$prop') case final range?) prop: range,
      },
      on: paramBool('${p}_on') ?? false,
      thr: paramNum('${p}_thr') ?? (kind == DynKind.gate ? -50 : -20),
      range: paramNum('${p}_range') ?? 40,
      att: paramNum('${p}_att') ?? (kind == DynKind.gate ? 0.5 : 10),
      hold: paramNum('${p}_hold') ?? 50,
      rel: paramNum('${p}_rel') ?? (kind == DynKind.gate ? 150 : 120),
      ratioIsLimit: paramNum('${p}_ratio') != null &&
          paramNum('${p}_ratio') ==
              paramMetadata('${p}_ratio')?['limiter_value'],
      ratio: paramNum('${p}_ratio') ?? 3,
      knee: paramNum('${p}_knee') ?? 3,
      makeup: paramNum('${p}_makeup') ?? 0,
      mode: paramNum('${p}_mode')?.toInt(),
      detector: paramNum('${p}_det')?.toInt(),
      envelope: paramNum('${p}_env')?.toInt(),
      keySource: paramNum('${p}_key_src')?.toInt(),
      filterOn: paramBool('${p}_filter_on'),
      filterType: paramNum('${p}_filter_type')?.toInt(),
      filterFreq: paramNum('${p}_filter_freq'),
      mix: paramNum('${p}_mix'),
      auto: paramBool('${p}_auto'),
      softKnee: kind == DynKind.comp && paramNum('comp_knee') != null
          ? mixerProviderPolicy(engine.driver)
              .isSoftKnee(paramNum('comp_knee')!)
          : null,
    );
  }

  @override
  ChannelDynVm get gate => _dyn(DynKind.gate);

  @override
  ChannelDynVm get comp => _dyn(DynKind.comp);

  List<int> get _sendSlots {
    final family = _presetFamily(_channel);
    final familyScopes = family?['params_scope'];
    final kinds = familyScopes is List ? familyScopes : [family?['kind']];
    final slots = <int>[];
    final sourceKey = stripKey;
    for (final id in engine.supportedParams) {
      final match = RegExp(r'^send([1-9][0-9]*)_level$').firstMatch(id);
      if (match == null) continue;
      final scopes = engine.parameterMetadata[id]?['scopes'];
      if (scopes is List && scopes.isNotEmpty && !scopes.any(kinds.contains)) {
        continue;
      }
      final destination = paramMetadata(id)?['destination'];
      if (destination is Map &&
          destination['allow_self'] == false &&
          destination['key'] == sourceKey) {
        continue;
      }
      slots.add(int.parse(match[1]!));
    }
    return slots..sort();
  }

  @override
  List<ChannelSendVm> get sends {
    final cached = _sendsCache;
    if (cached != null) return cached;
    final built = List<ChannelSendVm>.unmodifiable([
      for (final slot in _sendSlots)
        ChannelSendVm(
          destinationControls: List.unmodifiable(engine.destinationControls
              .where((control) => control.sendSlot == slot)),
          slot: slot,
          destName: _sendName(slot),
          level: dbToPos(paramNum('send${slot}_level') ?? -90),
          muted: supports('send${slot}_on')
              ? !(paramBool('send${slot}_on') ?? true)
              : paramBool(_sendMuteLeaf(slot)) ?? false,
          canMute: supports('send${slot}_on') || supports(_sendMuteLeaf(slot)),
          pan: sendPan(slot),
          tap: paramNum('send${slot}_tap')?.toInt(),
          tapOptions: supports('send${slot}_tap')
              ? (parameterChoices(paramMetadata('send${slot}_tap')).isEmpty
                  ? const [0, 1, 2, 3, 4, 5]
                  : parameterChoices(paramMetadata('send${slot}_tap'))
                      .map((c) => c.value.toInt())
                      .toList())
              : const [],
          grpon: paramBool('send${slot}_grpon'),
        ),
    ]);
    _sendsCache = built;
    return built;
  }

  String _sendMuteLeaf(int slot) {
    final auxCount = mixerProviderPolicy(engine.driver).legacyAuxCount;
    if (auxCount == 0) return '';
    return slot <= auxCount ? 'aux${slot}_mute' : 'fx${slot - auxCount}_mute';
  }

  String _sendName(int slot) {
    for (final control in engine.destinationControls) {
      final reported = control.targetName?.trim();
      if (control.sendSlot == slot && reported != null && reported.isNotEmpty) {
        return reported;
      }
    }
    final destination = paramMetadata('send${slot}_level')?['destination'];
    if (destination is Map && destination['key'] is String) {
      final key = destination['key'];
      for (final strip in engine.strips) {
        if (strip.key == key && strip.name.isNotEmpty) return strip.name;
      }
      if (destination['label'] is String) return destination['label'] as String;
    }

    final auxCount = mixerProviderPolicy(engine.driver).legacyAuxCount;
    if (auxCount == 0) return 'Send $slot';
    final kind = slot <= auxCount ? 'bus' : 'fxsnd';
    final index = slot <= auxCount ? slot - 1 : slot - auxCount - 1;
    for (final family in engine.topology) {
      if (family['key'] != kind || index >= (family['count'] as num).toInt()) {
        continue;
      }
      final ch = (family['first'] as num).toInt() + index;
      for (final strip in engine.strips) {
        if (strip.ch == ch) return strip.name;
      }
    }
    return slot <= auxCount ? 'Aux $slot' : 'FX ${slot - auxCount}';
  }

  @override
  int get subgroupCount =>
      engine.subgroupAssign.keys.any((key) => key.startsWith('$_channel:'))
          ? engine.subgroupCount
          : 0;

  @override
  int get muteGroupCount =>
      engine.muteGroupAssign.keys.any((key) => key.startsWith('$_channel:'))
          ? engine.muteGroupCount
          : 0;

  @override
  bool? subgroupAssigned(int g) =>
      accessTargetAllowed('subgroup/${(g + 1).toString().padLeft(2, '0')}')
          ? engine.subgroupAssign['$_channel:$g']
          : null;

  @override
  bool? muteGroupAssigned(int g) => engine.muteGroupAssign['$_channel:$g'];

  @override
  bool get locked => engine.locks.containsKey(_channel);

  @override
  bool get focused => _focusCh == _channel;

  @override
  ChannelTab get tab => _tab;

  @override
  void setTab(ChannelTab tab) {
    _tab = tab;
    notifyListeners();
  }

  @override
  void selectEqBand(int band) {
    _eqBand = band;
    notifyListeners();
  }

  @override
  void setEqParam(int band, String param, double value) =>
      cmdParam('eq${band}_$param', value);

  @override
  void setEqBandType(int band, int value) {
    if (band < 1 || band > _eqBandCount) return;
    final leaf = 'eq${band}_type';
    if (!supports(leaf) ||
        paramNum(leaf) == null ||
        !_eqTypeChoices(band).any((choice) => choice.value == value)) {
      return;
    }
    final policy = mixerProviderPolicy(engine.driver);
    if (policy.useNativeEqChoices) {
      cmdParam(leaf, value);
      return;
    }
    final shape = EqFilterShape.fromValue(value);
    if (shape != null) cmdParam(leaf, policy.eqNativeType(shape));
  }

  @override
  void setEqOn(bool on) => cmdParam('eq_on', on);

  @override
  void setEqBandOn(int band, bool on) {
    if (band == 0) {
      cmdParam('hpf_on', on);
      return;
    }
    if (band < 1 || band > _eqBandCount) return;
    final leaf = 'eq${band}_on';
    // Never synthesize bypass via gain or type: only a provider-advertised,
    // read-back switch can represent an individual band's actual state.
    if (supports(leaf) && paramBool(leaf) != null) cmdParam(leaf, on);
  }

  @override
  void setHpfSlope(int slope) {
    // The public VM uses dB/oct while the catalogue uses a provider-native
    // enum. Resolve through the connected provider's advertised choices so
    // an unverified slope never writes: X32 documents 0..2 = 12/18/24
    // (protocol reference v4.09-02 `/ch/[01..32]/preamp/hpslope`).
    if (!supports('hpf_slope')) return;
    final policy = mixerProviderPolicy(engine.driver);
    for (final choice in parameterChoices(paramMetadata('hpf_slope'))) {
      if (policy.hpfSlope(choice.value) == slope) {
        cmdParam('hpf_slope', choice.value);
        return;
      }
    }
  }

  @override
  void resetEq() {
    for (var b = 1; b <= _eqBandCount; b++) {
      cmdParam('eq${b}_g', 0.0);
    }
    notifyListeners();
  }

  @override
  void setDynParam(DynKind kind, String prop, double value) =>
      cmdParam('${kind.name}_$prop', value);

  @override
  void setDynOn(DynKind kind, bool on) => cmdParam('${kind.name}_on', on);

  @override
  void resetDyn(DynKind kind) {
    final defaults = kind == DynKind.gate ? dynResetGate : dynResetComp;
    for (final entry in defaults.entries) {
      if (kind == DynKind.comp && entry.key == 'knee') {
        setCompSoftKnee(true);
      } else {
        cmdParam('${kind.name}_${entry.key}', entry.value);
      }
    }
  }

  @override
  void setSendLevel(int slot, double value) {
    final db = posToDb(value.clamp(0.0, 1.0));
    final floor =
        (paramMetadata('send${slot}_level')?['min'] as num?)?.toDouble() ??
            -90.0;
    cmdParam('send${slot}_level', db.isFinite ? db : floor);
  }

  @override
  void setSendTap(int slot, int tap) => cmdParam('send${slot}_tap', tap);

  @override
  void setSendMute(int slot, bool muted) {
    if (supports('send${slot}_on')) {
      cmdParam('send${slot}_on', !muted);
    } else {
      cmdParam(_sendMuteLeaf(slot), muted);
    }
  }

  @override
  void togglePhantom() => cmdParam('phantom', !(paramBool('phantom') ?? false));

  @override
  void toggleInvert() =>
      cmdParam('polarity', !(paramBool('polarity') ?? false));

  @override
  void setPreamp(String leaf, double value) => cmdParam(leaf, value);
}
