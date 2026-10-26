part of 'channel.dart';

class ProcessingSource {
  final String id, label, name;
  final bool selected, enabled;
  const ProcessingSource(
      {required this.id,
      required this.label,
      required this.name,
      required this.selected,
      this.enabled = true});
}

class _InputTab extends StatelessWidget {
  final ChannelStore store;
  final ValueChanged<ChannelFocus> onFocus;
  const _InputTab({required this.store, required this.onFocus});

  Future<void> _confirmPhantom(BuildContext context) async {
    final current = store.paramBool('phantom');
    if (!store.supports('phantom') || current == null) return;
    final epoch = store.connectionEpoch;
    final key = store.stripKey;
    final name = store.displayName;
    Object sourceContext() {
      final source = store.source;
      return (
        source.sourceA,
        source.sourceB,
        source.selected,
        source.usbReturn,
        source.srcName,
        source.srcGroup
      );
    }

    final target = sourceContext();
    bool targetCurrent() =>
        store.connectionEpoch == epoch &&
        store.stripKey == key &&
        sourceContext() == target &&
        store.supports('phantom') &&
        store.paramBool('phantom') == current;
    final confirmed = await showDialog<bool>(
        context: context,
        builder: (dialogContext) => ListenableBuilder(
            listenable: store,
            builder: (context, _) {
              final palette = ConsolePalette.of(context);
              return AlertDialog(
                  backgroundColor: palette.panel,
                  title: Text(current ? 'Turn off 48V?' : 'Turn on 48V?',
                      style: AppDesign.type(18, color: palette.text1)),
                  content: Text(
                      targetCurrent()
                          ? '${current ? 'Disable' : 'Enable'} phantom power for $name?'
                          : 'The channel, source, or connection changed. Reopen this confirmation.',
                      style: AppDesign.type(16, color: palette.text7)),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(dialogContext, false),
                        child: const Text('Cancel')),
                    TextButton(
                        onPressed: targetCurrent()
                            ? () => Navigator.pop(dialogContext, true)
                            : null,
                        child: Text(current ? 'Turn off' : 'Turn on')),
                  ]);
            }));
    if (confirmed == true && context.mounted && targetCurrent()) {
      store.togglePhantom();
    }
  }

  Future<void> _editSourceMode(BuildContext context) async {
    final snapshot = store.sourceMode;
    if (snapshot == null || !snapshot.editable) return;
    var selected = snapshot.mode;
    bool current() =>
        store.sourceMode?.context == snapshot.context &&
        store.sourceMode?.editable == true;
    final result = await showDialog<int>(
        context: context,
        builder: (dialogContext) => StatefulBuilder(
            builder: (context, setLocalState) => ListenableBuilder(
                listenable: store,
                builder: (context, _) {
                  final palette = ConsolePalette.of(context);
                  return AlertDialog(
                      backgroundColor: palette.panel,
                      title: Text('Source mode',
                          style: AppDesign.type(18, color: palette.text1)),
                      content: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Text(
                                current()
                                    ? '${snapshot.sourceLabel} is shared. This changes its mode for every channel using that source.'
                                    : 'The source, mode, or connection changed. Reopen this control.',
                                style:
                                    AppDesign.type(16, color: palette.text7)),
                            const SizedBox(height: 12),
                            for (var mode = 0; mode < 3; mode++)
                              Padding(
                                  padding: const EdgeInsets.only(bottom: 7),
                                  child: _ProcessingButton(
                                      label: const [
                                        'Mono',
                                        'Stereo',
                                        'Mid/side'
                                      ][mode],
                                      active: selected == mode,
                                      onTap: current()
                                          ? () => setLocalState(
                                              () => selected = mode)
                                          : null)),
                          ]),
                      actions: [
                        TextButton(
                            onPressed: () => Navigator.pop(dialogContext),
                            child: const Text('Cancel')),
                        TextButton(
                            onPressed: current() && selected != snapshot.mode
                                ? () => Navigator.pop(dialogContext, selected)
                                : null,
                            child: const Text('Apply')),
                      ]);
                })));
    if (result != null && context.mounted && current()) {
      store.setSourceMode(snapshot, result);
    }
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 700;
    final src = store.source;
    String sourceName(String leaf, int? value) => value == null
        ? '—'
        : parameterValueLabel(store.paramMetadata(leaf), value.toDouble()) ??
            'Source $value';
    final sources = <ProcessingSource>[
      if (store.supports('source_select')) ...[
        ProcessingSource(
            id: '0',
            label: 'A',
            name: sourceName('source_a', src.sourceA),
            selected: src.selected == 0,
            enabled: src.selected != null),
        ProcessingSource(
            id: '1',
            label: 'B',
            name: sourceName('source_b', src.sourceB),
            selected: src.selected == 1,
            enabled: src.selected != null),
      ],
    ];
    final pre = store.preamp;
    final source =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (!wide)
        Text('Input', style: AppDesign.type(18, weight: FontWeight.w600)),
      SizedBox(height: wide ? 16 : 12),
      Text('Source',
          style: AppDesign.type(wide ? 18 : 14,
              color: ConsolePalette.of(context).text7)),
      const SizedBox(height: 8),
      if (sources.isEmpty)
        Container(
            constraints: const BoxConstraints(minHeight: 60),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
                color: ConsolePalette.of(context).chip, borderRadius: U4.rBtn),
            child: Text(
                [store.source.srcName, store.source.srcGroup]
                    .where((s) => s.isNotEmpty)
                    .join(' · '),
                style: AppDesign.type(16,
                    color: ConsolePalette.of(context).text7)))
      else if (wide)
        Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Expanded(
              child: Column(children: [
            for (final s in sources)
              Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: _ProcessingButton(
                      label: s.label,
                      active: s.selected,
                      onTap: s.enabled
                          ? () => store.setSourceSelected(int.parse(s.id))
                          : null))
          ])),
          const SizedBox(width: 12),
          Expanded(
              child: Column(children: [
            for (final s in sources)
              SizedBox(
                  height: 50,
                  child: Align(
                      alignment: Alignment.centerLeft,
                      child: Text(s.name, style: AppDesign.type(18))))
          ])),
        ])
      else
        Row(children: [
          for (var i = 0; i < sources.length; i++) ...[
            if (i > 0) const SizedBox(width: 6),
            Expanded(
                child: _ProcessingButton(
                    label: sources[i].label,
                    detail: sources[i].name,
                    active: sources[i].selected,
                    onTap: sources[i].enabled
                        ? () =>
                            store.setSourceSelected(int.parse(sources[i].id))
                        : null)),
          ]
        ]),
      if (store.paramApplies('wing:input_mode')) ...[
        const SizedBox(height: 12),
        if (store.sourceMode?.editable == true)
          _ProcessingButton(
              label: 'Source mode',
              detail: store.sourceMode!.label,
              semanticsLabel: 'Edit shared source mode',
              onTap: () => _editSourceMode(context))
        else
          Text(
              'Input mode · ${_choiceText(store, 'wing:input_mode', 'Loading…')}',
              style:
                  AppDesign.type(14, color: ConsolePalette.of(context).text7)),
      ],
      if (store.paramApplies('wing:source_auto')) ...[
        const SizedBox(height: 7),
        _ProcessingButton(
            label: 'Auto alternate source',
            active: store.paramBool('wing:source_auto') == true,
            onTap: store.supports('wing:source_auto')
                ? () => store.cmdParam('wing:source_auto',
                    store.paramBool('wing:source_auto') != true)
                : null),
      ],
      SizedBox(height: wide ? 88 : 14),
      if (store.paramMetadata('gain')?['hidden'] != true) ...[
        _InputPreampControl(
            store: store, leaf: 'gain', label: 'Gain', onFocus: onFocus),
        const SizedBox(height: 7),
      ],
      _InputPreampControl(
          store: store,
          leaf: 'trim',
          label: 'Trim',
          onFocus: onFocus,
          trailing: _ProcessingButton(
              label: 'Ø',
              active: pre.invert,
              semanticsLabel: 'Invert polarity',
              onTap: store.supports('polarity') ? store.toggleInvert : null)),
      if (store.paramApplies('wing:input_balance')) ...[
        const SizedBox(height: 7),
        _InputPreampControl(
            store: store,
            leaf: 'wing:input_balance',
            label: 'Input balance',
            onFocus: onFocus),
      ],
      if (store.paramApplies('wing:width')) ...[
        const SizedBox(height: 7),
        _InputPreampControl(
            store: store,
            leaf: 'wing:width',
            label: 'Stereo width',
            section: 'Stereo',
            onFocus: onFocus),
      ],
      const SizedBox(height: 7),
      if (store.paramMetadata('phantom')?['hidden'] != true)
        Row(children: [
          SizedBox(
              width: 64,
              child: _TextSwitch(
                  label: '48V',
                  active: pre.phantom,
                  enabled: store.supports('phantom'),
                  onTap: () => _confirmPhantom(context))),
          Expanded(
              child: Text('Phantom power',
                  style: AppDesign.type(14,
                      color: ConsolePalette.of(context).text7)))
        ]),
      SizedBox(height: wide ? 24 : 14),
      if (store.hasDelay) ...[
        _InputDelayControl(store: store, onFocus: onFocus),
        SizedBox(height: wide ? 24 : 14),
      ],
      _InputAssignments(store: store, wide: wide),
    ]);
    final sends =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (!wide) ...[
        const SizedBox(height: 18),
        Text('Sends',
            style: AppDesign.type(14, color: ConsolePalette.of(context).text7))
      ],
      if (store.sends.isEmpty)
        Padding(
            padding: const EdgeInsets.symmetric(vertical: 16),
            child: Text('No send slots on this desk.',
                style: AppDesign.type(14,
                    color: ConsolePalette.of(context).text7))),
      if (store.supports('wing:send_tap'))
        Padding(
            padding: const EdgeInsets.only(top: 12),
            child: _ParameterChoices(
                store: store,
                leaf: 'wing:send_tap',
                label: 'Pre tap source',
                fallback: 'Loading…',
                compact: true,
                onChanged: (value) => store.cmdParam('wing:send_tap', value))),
      for (final send in store.sends)
        Padding(
            padding:
                EdgeInsets.only(top: wide ? 24 : 14, bottom: wide ? 16 : 0),
            child: _ProcessingSend(
                store: store, send: send, wide: wide, onFocus: onFocus)),
    ]);
    if (!wide) {
      return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [source, sends]);
    }
    return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Expanded(
          flex: 538,
          child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              child: source)),
      Container(
          width: 1, height: 760, color: ConsolePalette.of(context).lineCtl),
      Expanded(
          flex: 806,
          child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: sends)),
    ]);
  }
}

class _InputDelayControl extends StatelessWidget {
  final ChannelStore store;
  final ValueChanged<ChannelFocus> onFocus;
  const _InputDelayControl({required this.store, required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final snapshot = store.delay;
    final epoch = store.connectionEpoch;
    final key = store.stripKey;
    bool current() =>
        snapshot != null &&
        store.connectionEpoch == epoch &&
        store.stripKey == key &&
        store.delay?.context == snapshot.context;
    String readout(ChannelDelayVm value) =>
        '${value.value.toStringAsFixed(value.unit == 3 ? 0 : 2)} ${value.unitLabel}';
    void apply(double value) {
      if (current()) store.setDelay(snapshot!, value);
    }

    void focus() {
      if (snapshot == null || !snapshot.editable || !current()) return;
      onFocus(ChannelFocus(
          title: 'Delay',
          label: snapshot.unitLabel,
          text: () =>
              current() ? readout(store.delay!) : 'Reopen delay control',
          entryText: () => current() ? store.delay!.value.toString() : '',
          min: snapshot.min,
          max: snapshot.max,
          resolveRange: () => current() && store.delay?.editable == true
              ? [snapshot.min, snapshot.max]
              : null,
          apply: apply));
    }

    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      _ProcessingSlider(
          label: 'Delay',
          compact: true,
          fillFromStart: true,
          solidFill: true,
          color: ConsolePalette.of(context).success,
          value: snapshot?.value ?? 0,
          min: snapshot?.min ?? 0,
          max: snapshot?.max ?? 1,
          text: snapshot == null ? 'Loading…' : readout(snapshot),
          enabled: snapshot?.editable == true,
          onFocus: focus,
          onChanged: (value) {
            apply(value);
            focus();
          },
          trailing: _ProcessingButton(
              label: 'On',
              active: store.paramBool('delay_on') == true,
              semanticsLabel: 'Delay enabled',
              onTap: store.supports('delay_on')
                  ? () => store.cmdParam(
                      'delay_on', store.paramBool('delay_on') != true)
                  : null)),
      const SizedBox(height: 7),
      _ParameterChoices(
          store: store,
          leaf: 'wing:delay_unit',
          label: 'Delay unit',
          fallback: 'Loading…',
          compact: true,
          onChanged: (value) => store.cmdParam('wing:delay_unit', value)),
    ]);
  }
}

// A missing range never enables an editing surface.
({double min, double max})? _inputRange(Map<String, dynamic>? metadata) {
  final min = metadata?['min'], max = metadata?['max'];
  if (min is! num ||
      max is! num ||
      !min.isFinite ||
      !max.isFinite ||
      min >= max) {
    return null;
  }
  return (min: min.toDouble(), max: max.toDouble());
}

class _InputPreampControl extends StatelessWidget {
  final ChannelStore store;
  final String leaf, label, section;
  final ValueChanged<ChannelFocus> onFocus;
  final Widget? trailing;
  const _InputPreampControl(
      {required this.store,
      required this.leaf,
      required this.label,
      required this.onFocus,
      this.section = 'Input',
      this.trailing});

  @override
  Widget build(BuildContext context) {
    final metadata = store.paramMetadata(leaf);
    final range = _inputRange(metadata);
    final value = store.paramNum(leaf);
    final unit = metadata?['unit'] as String? ?? '';
    String readout(double v) =>
        '${v.toStringAsFixed(1)}${unit.isEmpty ? '' : ' $unit'}';
    final enabled = store.supports(leaf) && value != null && range != null;
    final text = !store.capabilitiesReady
        ? 'Loading…'
        : metadata == null
            ? 'Not supported'
            : value == null || !store.supports(leaf)
                ? 'Loading…'
                : range == null
                    ? '${readout(value)} · range unavailable'
                    : readout(value);
    void focusPreamp() {
      if (!enabled) return;
      onFocus(_parameterFocus(store, leaf, section, label, () {
        final live = store.paramNum(leaf);
        return live == null ? 'Loading…' : readout(live);
      }, (v) => store.setPreamp(leaf, v)));
    }

    return _ProcessingSlider(
        label: label,
        compact: true,
        color: leaf == 'gain'
            ? ConsolePalette.of(context).mute
            : ConsolePalette.of(context).success,
        fillFromStart: true,
        solidFill: true,
        value: value ?? range?.min ?? 0,
        min: range?.min ?? 0,
        max: range?.max ?? 1,
        text: text,
        enabled: enabled,
        onFocus: enabled ? focusPreamp : null,
        onChanged: (v) {
          store.setPreamp(leaf, v);
          focusPreamp();
        },
        trailing: trailing);
  }
}

class _InputAssignments extends StatelessWidget {
  final ChannelStore store;
  final bool wide;
  const _InputAssignments({required this.store, required this.wide});

  String _summaryLabel(String family, String label) =>
      label.startsWith('$family ') ? label : '$family: $label';

  @override
  Widget build(BuildContext context) {
    final assignmentFont = wide && store.viewGroupCount > 0 ? 14.0 : 16.0;
    final groups = <String, List<Widget>>{
      'Main': [
        _ProcessingButton(
            fontSize: assignmentFont,
            label: 'Main',
            detail: wide ? 'LR' : null,
            active: store.mainAssigned == true,
            onTap: store.mainAssigned == null
                ? null
                : () => store.setMainAssigned(!store.mainAssigned!))
      ],
      'Sub': [
        for (var g = 0; g < store.subgroupCount; g++)
          _ProcessingButton(
              fontSize: assignmentFont,
              label: 'Sub ${g + 1}',
              active: store.subgroupAssigned(g) == true,
              onTap: store.subgroupAssigned(g) == null
                  ? null
                  : () =>
                      store.cmdSubgroupAssign(g, !store.subgroupAssigned(g)!))
      ],
      'VCA': [
        for (var g = 0; g < store.dcaCount; g++)
          _ProcessingButton(
              fontSize: assignmentFont,
              label: store.dcaGroupLabel(g),
              active: store.dcaAssigned(g) == true,
              onTap: store.dcaAssigned(g) == null
                  ? null
                  : () => store.setDcaAssigned(g, !store.dcaAssigned(g)!))
      ],
      'Mute': [
        for (var g = 0; g < store.muteGroupCount; g++)
          _ProcessingButton(
              fontSize: assignmentFont,
              label: 'Mute ${g + 1}',
              active: store.muteGroupAssigned(g) == true,
              onTap: store.muteGroupAssigned(g) == null
                  ? null
                  : () =>
                      store.cmdMuteGroupAssign(g, !store.muteGroupAssigned(g)!))
      ],
      if (store.viewGroupCount > 0)
        'View': [
          for (var g = 0; g < store.viewGroupCount; g++)
            _ProcessingButton(
                fontSize: assignmentFont,
                label: store.viewGroupLabel(g),
                active: store.viewGroupAssigned(g) == true,
                onTap: store.viewGroupAssigned(g) == null
                    ? null
                    : () => store.setViewGroupAssigned(
                        g, !store.viewGroupAssigned(g)!)),
        ],
    };
    final selected = [
      if (store.mainAssigned == true) 'Main',
      for (var g = 0; g < store.subgroupCount; g++)
        if (store.subgroupAssigned(g) == true) 'Sub ${g + 1}',
      for (var g = 0; g < store.dcaCount; g++)
        if (store.dcaAssigned(g) == true)
          _summaryLabel('VCA', store.dcaGroupLabel(g)),
      for (var g = 0; g < store.muteGroupCount; g++)
        if (store.muteGroupAssigned(g) == true) 'Mute ${g + 1}',
      for (var g = 0; g < store.viewGroupCount; g++)
        if (store.viewGroupAssigned(g) == true)
          _summaryLabel('View', store.viewGroupLabel(g))
    ].join(' / ');
    final columns = [
      for (final entry in groups.entries)
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(entry.key,
              textAlign: TextAlign.center,
              style:
                  AppDesign.type(16, color: ConsolePalette.of(context).text7)),
          const SizedBox(height: 8),
          for (final button in entry.value)
            Padding(padding: const EdgeInsets.only(bottom: 8), child: button),
        ])
    ];
    if (wide) {
      return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        for (var i = 0; i < columns.length; i++) ...[
          if (i > 0) const SizedBox(width: 8),
          Expanded(child: columns[i])
        ]
      ]);
    }
    return Theme(
        data: Theme.of(context).copyWith(dividerColor: Colors.transparent),
        child: ExpansionTile(
            // Keep the bool expansion state separate from the enclosing
            // processing scroll view's double scroll offset in PageStorage.
            key: PageStorageKey('input-assignments-${store.stripKey}'),
            minTileHeight: 44,
            dense: true,
            tilePadding: const EdgeInsets.symmetric(horizontal: 10),
            childrenPadding: const EdgeInsets.all(10),
            backgroundColor: ConsolePalette.of(context).slot,
            collapsedBackgroundColor: ConsolePalette.of(context).slot,
            shape: const RoundedRectangleBorder(borderRadius: U4.rBtn),
            collapsedShape: const RoundedRectangleBorder(borderRadius: U4.rBtn),
            title: Row(children: [
              Text('Assignments',
                  style: AppDesign.type(16, weight: FontWeight.w600)),
              const SizedBox(width: 8),
              Expanded(
                  child: Text(selected,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      textAlign: TextAlign.right,
                      style: AppDesign.type(12,
                          color: ConsolePalette.of(context).text7))),
            ]),
            children: [
              LayoutBuilder(builder: (context, box) {
                final count = (box.maxWidth / 88).floor().clamp(1, 3);
                final width = (box.maxWidth - 6 * (count - 1)) / count;
                return Wrap(spacing: 6, runSpacing: 12, children: [
                  for (final column in columns)
                    SizedBox(width: width, child: column),
                ]);
              })
            ]));
  }
}

List<double>? _sendControlRange(
    ChannelStore store, int slot, String param, double min, double max) {
  final range = _controlRange(store, 'send${slot}_$param');
  if (range == null || store.sendParamNote(slot, param).value == null) {
    return null;
  }
  final low = range.first.clamp(min, max);
  final high = range.last.clamp(min, max);
  return low < high ? [low, high] : null;
}

List<double>? _sendLevelRange(ChannelStore store, int slot) {
  final range = _controlRange(store, 'send${slot}_level');
  return store.sendParamNote(slot, 'level').value == null ? null : range;
}

class _ProcessingSend extends StatelessWidget {
  final ChannelStore store;
  final ChannelSendVm send;
  final bool wide;
  final ValueChanged<ChannelFocus> onFocus;
  const _ProcessingSend(
      {required this.store,
      required this.send,
      required this.wide,
      required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final epoch = store.connectionEpoch;
    final stripKey = store.stripKey;
    bool current() =>
        store.connectionEpoch == epoch && store.stripKey == stripKey;
    String levelText(double value, double floor) => value <= floor
        ? '-∞ dB'
        : '${value > 0 ? '+' : ''}${value.toStringAsFixed(1)} dB';
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      ValueListenableBuilder<double?>(
          valueListenable: store.sendParamNote(send.slot, 'level'),
          builder: (context, db, _) {
            final level = dbToPos(db ?? -90);
            final note = store.sendParamNote(send.slot, 'level');
            final range = _sendLevelRange(store, send.slot);
            // The normalized slider retains the shared fader law. Entry uses
            // native engineering dB, including values below the visual floor.
            final minDb = range?.first ?? -90;
            final maxDb = range?.last ?? 10;
            void focusSend() => onFocus(ChannelFocus(
                title: 'Sends',
                label: send.destName,
                valueListenable: note,
                text: () {
                  final live = store.sendParamNote(send.slot, 'level').value;
                  return live == null
                      ? 'Loading…'
                      : levelText(live, minDb.toDouble());
                },
                entryText: () {
                  final live = store.sendParamNote(send.slot, 'level').value;
                  return live == null ? 'Loading…' : live.toStringAsFixed(1);
                },
                min: minDb,
                max: maxDb,
                resolveRange: () => _sendLevelRange(store, send.slot),
                apply: (v) {
                  final live = _sendLevelRange(store, send.slot);
                  if (!current() || live == null || !v.isFinite) return;
                  store.cmdParam(
                      'send${send.slot}_level', v.clamp(live.first, live.last));
                }));

            return _ProcessingSlider(
                label: send.destName,
                compact: !wide,
                showValue: false,
                value: level,
                min: dbToPos(minDb.toDouble()),
                max: dbToPos(maxDb.toDouble()),
                onFocus: focusSend,
                text: db == null ? 'Loading…' : levelText(db, minDb.toDouble()),
                enabled: range != null,
                onChanged: (v) {
                  store.setSendLevel(send.slot, v);
                  focusSend();
                },
                trailing: _ProcessingButton(
                    label: 'M',
                    active: send.muted,
                    semanticsLabel: '${send.destName} mute',
                    onTap: send.canMute
                        ? () => store.setSendMute(send.slot, !send.muted)
                        : null));
          }),
      if (store.supports('send${send.slot}_pan'))
        ValueListenableBuilder<double?>(
            valueListenable: store.sendParamNote(send.slot, 'pan'),
            builder: (context, value, _) {
              if (value == null) return const SizedBox.shrink();
              final pan = value.clamp(-100, 100) / 100;
              final note = store.sendParamNote(send.slot, 'pan');
              final range =
                  _sendControlRange(store, send.slot, 'pan', -100, 100);
              final minPan = range?.first ?? -100;
              final maxPan = range?.last ?? 100;
              void focusPan() => onFocus(ChannelFocus(
                  title: 'Sends',
                  label: '${send.destName} pan (−100 L / +100 R)',
                  valueListenable: note,
                  text: () {
                    final live = store.sendParamNote(send.slot, 'pan').value;
                    return live == null ? 'Loading…' : panText(live / 100);
                  },
                  entryText: () {
                    final live = store.sendParamNote(send.slot, 'pan').value;
                    return live == null ? 'Loading…' : live.toStringAsFixed(0);
                  },
                  min: minPan,
                  max: maxPan,
                  resolveRange: () =>
                      _sendControlRange(store, send.slot, 'pan', -100, 100),
                  apply: (v) {
                    final live =
                        _sendControlRange(store, send.slot, 'pan', -100, 100);
                    if (live == null || !v.isFinite) return;
                    store.setSendPan(
                        send.slot, v.clamp(live.first, live.last) / 100);
                  }));

              return Padding(
                  padding: EdgeInsets.only(top: wide ? 7 : 0, right: 50),
                  child: _ProcessingSlider(
                      label: '${send.destName} pan',
                      value: pan,
                      enabled: range != null,
                      min: minPan / 100,
                      max: maxPan / 100,
                      text: panText(pan),
                      pan: true,
                      compact: !wide,
                      showLabel: false,
                      showValue: false,
                      onFocus: focusPan,
                      onChanged: (v) {
                        store.setSendPan(send.slot, v);
                        focusPan();
                      }));
            }),
      if (send.tapOptions.isNotEmpty &&
          parameterChoices(store.paramMetadata('send${send.slot}_tap'))
              .isNotEmpty)
        Padding(
            padding: const EdgeInsets.only(top: 6),
            child: _ParameterChoices(
                store: store,
                leaf: 'send${send.slot}_tap',
                label: '${send.destName} position',
                fallback: tapLabels[send.tap] ?? 'Loading…',
                compact: true,
                onChanged: (value) =>
                    store.setSendTap(send.slot, value.toInt()))),
      if (store.supports('send${send.slot}_pre'))
        Padding(
            padding: const EdgeInsets.only(top: 6),
            child: _ProcessingButton(
                label: store.paramBool('send${send.slot}_pre') == true
                    ? 'Pre-fader'
                    : 'Post-fader',
                semanticsLabel: '${send.destName} send position',
                onTap: () {
                  if (!current() || !store.supports('send${send.slot}_pre')) {
                    return;
                  }
                  final value = store.paramBool('send${send.slot}_pre');
                  if (value != null) {
                    store.cmdParam('send${send.slot}_pre', !value);
                  }
                })),
      for (final control in send.destinationControls)
        Padding(
            padding: const EdgeInsets.only(top: 6),
            child: _ProcessingButton(
                label:
                    '${control.label}: ${control.valueLabel ?? 'Unavailable'}'
                    '${control.authority == 'accepted' ? ' (unconfirmed)' : ''}',
                detail: 'All inputs feeding ${control.destinationName}',
                semanticsLabel: '${control.destinationName} ${control.label}, '
                    '${control.valueLabel ?? 'unavailable'}, shared by all inputs',
                fontSize: 12,
                onTap: store.destinationControlIsCurrent(control)
                    ? () => showDialog<void>(
                        context: context,
                        barrierDismissible: false,
                        builder: (_) => DestinationControlEditor(
                            changes: store,
                            isCurrent: store.destinationControlIsCurrent,
                            apply: store.setDestinationControl,
                            control: control))
                    : null)),
    ]);
  }
}
