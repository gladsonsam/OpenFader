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
  final String leaf, label;
  final ValueChanged<ChannelFocus> onFocus;
  final Widget? trailing;
  const _InputPreampControl(
      {required this.store,
      required this.leaf,
      required this.label,
      required this.onFocus,
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
      onFocus(_parameterFocus(store, leaf, 'Input', label, () {
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

  @override
  Widget build(BuildContext context) {
    final groups = <String, List<Widget>>{
      'Main': [
        _ProcessingButton(
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
              label: 'VCA ${g + 1}',
              active: store.dcaAssigned(g) == true,
              onTap: store.dcaAssigned(g) == null
                  ? null
                  : () => store.setDcaAssigned(g, !store.dcaAssigned(g)!))
      ],
      'Mute': [
        for (var g = 0; g < store.muteGroupCount; g++)
          _ProcessingButton(
              label: 'Mute ${g + 1}',
              active: store.muteGroupAssigned(g) == true,
              onTap: store.muteGroupAssigned(g) == null
                  ? null
                  : () =>
                      store.cmdMuteGroupAssign(g, !store.muteGroupAssigned(g)!))
      ],
    };
    final selected = [
      if (store.mainAssigned == true) 'Main',
      for (var g = 0; g < store.subgroupCount; g++)
        if (store.subgroupAssigned(g) == true) 'Sub ${g + 1}',
      for (var g = 0; g < store.dcaCount; g++)
        if (store.dcaAssigned(g) == true) 'VCA ${g + 1}',
      for (var g = 0; g < store.muteGroupCount; g++)
        if (store.muteGroupAssigned(g) == true) 'Mute ${g + 1}'
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
              Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
                for (var i = 0; i < columns.length; i++) ...[
                  if (i > 0) const SizedBox(width: 6),
                  Expanded(child: columns[i])
                ]
              ])
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
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      ValueListenableBuilder<double?>(
          valueListenable: store.sendParamNote(send.slot, 'level'),
          builder: (context, db, _) {
            final level = dbToPos(db ?? -90);
            final note = store.sendParamNote(send.slot, 'level');
            final range = _sendControlRange(store, send.slot, 'level', -90, 10);
            // The normalized slider retains the shared fader law. Entry uses
            // engineering dB, bounded by both that law and advertised caps.
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
                      : '${fmtDb(dbToPos(live))} dB';
                },
                entryText: () {
                  final live = store.sendParamNote(send.slot, 'level').value;
                  return live == null ? 'Loading…' : live.toStringAsFixed(1);
                },
                min: minDb,
                max: maxDb,
                resolveRange: () =>
                    _sendControlRange(store, send.slot, 'level', -90, 10),
                apply: (v) {
                  final live =
                      _sendControlRange(store, send.slot, 'level', -90, 10);
                  if (live == null || !v.isFinite) return;
                  store.setSendLevel(
                      send.slot, dbToPos(v.clamp(live.first, live.last)));
                }));

            return _ProcessingSlider(
                label: send.destName,
                compact: !wide,
                showValue: false,
                value: level,
                min: dbToPos(minDb.toDouble()),
                max: dbToPos(maxDb.toDouble()),
                onFocus: focusSend,
                text: db == null ? 'Loading…' : '${fmtDb(level)} dB',
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
    ]);
  }
}
