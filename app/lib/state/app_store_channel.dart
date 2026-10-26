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
  String get stripKey => 'ch/${(_channel + 1).toString().padLeft(2, '0')}';
  @override
  int get connectionEpoch => engine.session;

  @override
  String get displayName {
    for (final s in engine.strips) {
      if (s.ch == _channel && s.name.isNotEmpty) return s.name;
    }
    return 'CH${(_channel + 1).toString().padLeft(2, '0')}';
  }

  @override
  String get channelNumber => '${_channel + 1}';

  @override
  ValueNotifier<double> faderNote() => faderOf(_channel);

  @override
  void setFaderDb(double db) => setFader(_channel, dbToPos(db));

  @override
  double? paramNum(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, AppStore.deskLeaf(leaf));
    return v is num ? v.toDouble() : null;
  }

  @override
  bool? paramBool(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, AppStore.deskLeaf(leaf));
    return v is bool ? v : null;
  }

  @override
  bool get capabilitiesReady => engine.capsReady;

  @override
  Map<String, dynamic>? paramMetadata(String leaf) => parameterPresentation(
      engine.parameterMetadata[AppStore.deskLeaf(leaf)], paramNum);

  @override
  bool supports(String leaf) {
    final desk = AppStore.deskLeaf(leaf);
    if (!engine.capsReady || _presetEditingBlocked(_channel, desk)) {
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
    // manufacturer mixer software's modern DL32S EQ disables Q while a shelf is selected
    // (reference guide p101). Keep the stored value, but block every edit
    // path, including pinch and a previously focused numeric readout.
    final eqQ = RegExp(r'^eq([1-4])_q$').firstMatch(desk);
    if (engine.driver == 'dl32s' &&
        eqQ != null &&
        EqFilterShape.fromValue(_eqPresentationType(int.parse(eqQ[1]!))) !=
            EqFilterShape.bell) {
      return false;
    }
    return engine.supportedParams.contains(desk) &&
        (engine.driver == 'sim' ||
            (engine.connected && engine.valueKnown(_channel, desk)));
  }

  Map<String, Object> _preampExpect(int channel) => {
        for (final field in [
          'source_a',
          'source_b',
          'source_select',
          'source',
          'usbreturn'
        ])
          if (engine.supportedParams.contains(field) &&
              ps.optimisticValue(_maps, channel, field) != null)
            field: ps.optimisticValue(_maps, channel, field)!,
      };

  @override
  void cmdParam(String leaf, Object value) {
    final desk = AppStore.deskLeaf(leaf);
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
        RegExp(r'^eq[1-6]_(freq|gain|q)$').hasMatch(desk) ||
        RegExp(r'^(gate|comp)_(thr|range|att|hold|rel|ratio|knee|makeup|filter_freq|mix)$')
            .hasMatch(desk);
    final range = isDspRange ? _editableRange(desk) : null;
    if (isDspRange && (range == null || value is! num)) return;
    final coerced = range == null
        ? value
        : (value as num).toDouble().clamp(range.first, range.last);
    // Capture at enqueue time: navigation must not retarget a queued edit.
    final ch = _channel;
    _maps.confirmed.putIfAbsent(ch, () => {});
    ps.noteOptimistic(_maps, ch, desk, coerced, _nowMs());
    final epoch = engine.session;
    final expected = desk == 'gain' || desk == 'phantom'
        ? _preampExpect(ch)
        : <String, Object>{};
    _coalescer.offer('param:$ch:$desk', () {
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
    _send(Cmds.focus([_channel]));
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
      srcName: raw is String ? raw : '—',
      sourceA: paramNum('source_a')?.toInt(),
      sourceB: paramNum('source_b')?.toInt(),
      // DL32S wire selector is 1=A / 0=B, verified against manufacturer mixer software.
      // The view model uses the displayed A/B index, 0=A / 1=B.
      selected: selected == null
          ? null
          : engine.driver == 'dl32s'
              ? 1 - selected
              : selected,
      usbReturn: paramNum('usbreturn')?.toInt(),
      levelDb: _levelDb.value,
    );
  }

  // A send-pan leaf alone never proves stereo; only a reported link on the destination does.

  bool _sendDestinationIsStereoAux(int slot) {
    if (slot < 1) return false;
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

  // A/B selects the active slot; the Routing page edits each slot separately.

  @override
  void setSourceSelected(int selected) {
    if (selected == 0 || selected == 1) {
      cmdParam(
          'source_select', engine.driver == 'dl32s' ? 1 - selected : selected);
    }
  }

  @override
  void setSendPan(int slot, double pan) =>
      cmdParam('send${slot}_pan', pan.clamp(-1.0, 1.0) * 100);

  @override
  bool? get mainAssigned => paramBool('lr_assign');
  @override
  void setMainAssigned(bool on) => cmdParam('lr_assign', on);
  @override
  int get dcaCount => _assignmentCount('dca');

  int _assignmentCount(String family) {
    final pattern = RegExp('^$family([1-9][0-9]*)_assign\$');
    return engine.supportedParams
        .map((id) => int.tryParse(pattern.firstMatch(id)?.group(1) ?? ''))
        .whereType<int>()
        .fold(0, math.max);
  }

  @override
  bool? dcaAssigned(int group) => paramBool('dca${group + 1}_assign');
  @override
  String dcaGroupLabel(int group) =>
      paramMetadata('dca${group + 1}_assign')?['label'] as String? ??
      'VCA ${group + 1}';
  @override
  void setDcaAssigned(int group, bool on) =>
      cmdParam('dca${group + 1}_assign', on);

  @override
  int get viewGroupCount => _assignmentCount('view');
  @override
  bool? viewGroupAssigned(int group) => paramBool('view${group + 1}_assign');
  @override
  String viewGroupLabel(int group) =>
      paramMetadata('view${group + 1}_assign')?['label'] as String? ??
      'View ${group + 1}';
  @override
  void setViewGroupAssigned(int group, bool on) =>
      cmdParam('view${group + 1}_assign', on);

  @override
  void setCompSoftKnee(bool soft) => cmdParam(
      'comp_knee', engine.driver == 'dl32s' ? (soft ? 0 : 1) : (soft ? 3 : 0));

  int? _eqPresentationType(int band) {
    final native = paramNum('eq${band}_type')?.toInt();
    if (engine.driver != 'dl32s') return native;
    // Only live-verified DL32S shapes. Preserve raw values in the engine.
    return switch (native) {
      2 => EqFilterShape.bell.value,
      3 => EqFilterShape.lowShelf.value,
      4 => EqFilterShape.highShelf.value,
      _ => null,
    };
  }

  List<double>? _editableRange(String leaf) {
    if (!supports(leaf) || paramNum(leaf)?.isFinite != true) return null;
    return parameterRange(paramMetadata(leaf));
  }

  @override
  ChannelEqVm get eq => ChannelEqVm(
        on: paramBool('eq_on') ?? false,
        hpfF: paramNum('hpf_freq') ?? 80,
        hpfRange: _editableRange('hpf_freq'),
        hpfOn: paramBool('hpf_on') ?? false,
        hpfSlope: 12,
        selectedBand: _eqBand,
        bands: [
          for (var b = 1; b <= 4; b++)
            EqBandVm(
              band: b,
              name: const ['Low', 'Lo-Mid', 'Hi-Mid', 'High'][b - 1],
              f: paramNum('eq${b}_freq') ??
                  [80.0, 400.0, 1200.0, 6000.0][b - 1],
              g: paramNum('eq${b}_gain') ?? 0,
              q: paramNum('eq${b}_q') ?? 1,
              shelfUsesQ: engine.driver != 'dl32s',
              on: true,
              type: _eqPresentationType(b),
              fRange: _editableRange('eq${b}_freq'),
              gRange: _editableRange('eq${b}_gain'),
              qRange: _editableRange('eq${b}_q'),
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
          ? (engine.driver == 'dl32s'
              ? paramNum('comp_knee') == 0
              : paramNum('comp_knee')! > 0)
          : null,
    );
  }

  @override
  ChannelDynVm get gate => _dyn(DynKind.gate);

  @override
  ChannelDynVm get comp => _dyn(DynKind.comp);

  @override
  List<ChannelSendVm> get sends {
    final cached = _sendsCache;
    if (cached != null) return cached;
    final built = List<ChannelSendVm>.unmodifiable([
      for (var slot = 1; slot <= 16; slot++)
        if (engine.supportedParams.contains('send${slot}_level'))
          ChannelSendVm(
            destinationControls: List.unmodifiable(engine.destinationControls
                .where((control) => control.sendSlot == slot)),
            slot: slot,
            destName: _sendName(slot),
            level: dbToPos(paramNum('send${slot}_level') ?? -90),
            muted: supports('send${slot}_on')
                ? !(paramBool('send${slot}_on') ?? true)
                : paramBool(_sendMuteLeaf(slot)) ?? false,
            canMute:
                supports('send${slot}_on') || supports(_sendMuteLeaf(slot)),
            pan: sendPan(slot),
            tap: paramNum('send${slot}_tap')?.toInt(),
            tapOptions: supports('send${slot}_tap')
                ? const [0, 1, 2, 3, 4, 5]
                : const [],
            grpon: paramBool('send${slot}_grpon'),
          ),
    ]);
    _sendsCache = built;
    return built;
  }

  String _sendMuteLeaf(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
    return slot <= auxCount ? 'aux${slot}_mute' : 'fx${slot - auxCount}_mute';
  }

  String _sendName(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
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
  int get subgroupCount => engine.subgroupCount;

  @override
  int get muteGroupCount => engine.muteGroupCount;

  @override
  bool? subgroupAssigned(int g) => engine.subgroupAssign['$_channel:$g'];

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
  void setEqBandType(int band, EqFilterShape shape) {
    if (band != 1 && band != 4) return;
    if (shape != EqFilterShape.bell &&
        shape !=
            (band == 1 ? EqFilterShape.lowShelf : EqFilterShape.highShelf)) {
      return;
    }
    final leaf = 'eq${band}_type';
    if (!supports(leaf) ||
        EqFilterShape.fromValue(_eqPresentationType(band)) == null) {
      return;
    }
    final native = engine.driver == 'dl32s' && shape == EqFilterShape.lowShelf
        ? 3
        : shape.value;
    cmdParam(leaf, native);
  }

  @override
  void setEqOn(bool on) => cmdParam('eq_on', on);

  @override
  void setEqBandOn(int band, bool on) {
    if (band == 0) {
      cmdParam('hpf_on', on);
      return;
    }
    // Neither hardware driver has a per-band bypass leaf.
  }

  @override
  void setHpfSlope(int slope) {
    // The public VM uses dB/oct, while the catalogue uses a native enum.
    // Neither supported hardware driver advertises a verified slope control.
  }

  @override
  void resetEq() {
    for (var b = 1; b <= 4; b++) {
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
    cmdParam('send${slot}_level', db.isFinite ? db : -90.0);
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
