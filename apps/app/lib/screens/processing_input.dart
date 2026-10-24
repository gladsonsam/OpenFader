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
  final ChannelWrites writes;
  final ValueChanged<ChannelFocus> onFocus;
  const _InputTab(
      {required this.store, required this.writes, required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 700;
    final src = store.source;
    final sources = <ProcessingSource>[
      if (store.supports('source_select')) ...[
        ProcessingSource(
            id: '0',
            label: 'A',
            name: src.sourceA == null ? '—' : 'Source ${src.sourceA}',
            selected: src.selected == 0,
            enabled: src.selected != null),
        ProcessingSource(
            id: '1',
            label: 'B',
            name: src.sourceB == null ? '—' : 'Source ${src.sourceB}',
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
      _InputPreampControl(
          store: store, leaf: 'gain', label: 'Gain', onFocus: onFocus),
      const SizedBox(height: 7),
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
      Row(children: [
        SizedBox(
            width: 64,
            child: _TextSwitch(
                label: pre.phantomArmed ? '48V · tap again' : '48V',
                active: pre.phantom,
                enabled: store.supports('phantom'),
                onTap: store.togglePhantom)),
        Expanded(
            child: Text(
                pre.phantomArmed ? 'Confirm phantom power' : 'Phantom power',
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
            child: _ProcessingSend(store: store, send: send, wide: wide)),
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

/// A missing range never grants an invented editing surface.
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
    return _ProcessingSlider(
        label: label,
        compact: true,
        color: ConsolePalette.of(context).cyan,
        // Inert track coordinates only; missing metadata cannot enable edits.
        value: value ?? range?.min ?? 0,
        min: range?.min ?? 0,
        max: range?.max ?? 1,
        text: text,
        enabled: enabled,
        onFocus: enabled
            ? () => onFocus(ChannelFocus(
                title: 'Input',
                label: label,
                text: () {
                  final live = store.paramNum(leaf);
                  return live == null ? 'Loading…' : readout(live);
                },
                min: range.min,
                max: range.max,
                apply: (v) =>
                    store.setPreamp(leaf, v.clamp(range.min, range.max))))
            : null,
        onTypedEntry: enabled
            ? () async {
                final next = await showNumberEntry(
                    context: context,
                    title: unit.isEmpty ? label : '$label ($unit)',
                    initial: value.toStringAsFixed(1),
                    min: range.min,
                    max: range.max);
                if (next != null) store.setPreamp(leaf, next);
              }
            : null,
        onChanged: (v) => store.setPreamp(leaf, v),
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

class _ProcessingSend extends StatelessWidget {
  final ChannelStore store;
  final ChannelSendVm send;
  final bool wide;
  const _ProcessingSend(
      {required this.store, required this.send, required this.wide});

  @override
  Widget build(BuildContext context) {
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      ValueListenableBuilder<double?>(
          valueListenable: store.sendParamNote(send.slot, 'level'),
          builder: (context, db, _) {
            final level = dbToPos(db ?? -90);
            final range =
                _inputRange(store.paramMetadata('send${send.slot}_level'));
            // The normalized slider retains the shared fader law. Entry uses
            // engineering dB, bounded by both that law and advertised caps.
            final minDb = (range?.min ?? -90).clamp(-90.0, 10.0);
            final maxDb = (range?.max ?? 10).clamp(-90.0, 10.0);
            return _ProcessingSlider(
                label: send.destName,
                compact: !wide,
                value: level,
                min: 0,
                max: 1,
                onTypedEntry: () async {
                  final next = await showNumberEntry(
                      context: context,
                      title: '${send.destName} (dB)',
                      initial: db == null
                          ? ''
                          : db.clamp(minDb, maxDb).toStringAsFixed(1),
                      min: minDb,
                      max: maxDb,
                      allowNegInf: minDb == -90,
                      negInfValue: minDb);
                  if (next != null) {
                    store.setSendLevel(send.slot, dbToPos(next));
                  }
                },
                text: db == null ? 'Loading…' : '${fmtDb(level)} dB',
                enabled: db != null && store.supports('send${send.slot}_level'),
                onChanged: (v) => store.setSendLevel(send.slot, v),
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
              return Padding(
                  padding: EdgeInsets.only(top: wide ? 7 : 0, right: 50),
                  child: _ProcessingSlider(
                      label: '${send.destName} pan',
                      value: pan,
                      min: -1,
                      max: 1,
                      text: panText(pan),
                      pan: true,
                      compact: !wide,
                      showLabel: false,
                      onChanged: (v) => store.setSendPan(send.slot, v)));
            }),
    ]);
  }
}
