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
    final trim = store.paramNum('trim') ?? pre.trim;
    final source =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (!wide)
        Text('Input', style: AppDesign.type(18, weight: FontWeight.w600)),
      SizedBox(height: wide ? 16 : 12),
      Text('Source', style: AppDesign.type(wide ? 18 : 14, color: U4.text7)),
      const SizedBox(height: 8),
      if (sources.isEmpty)
        Container(
            constraints: const BoxConstraints(minHeight: 60),
            padding: const EdgeInsets.all(12),
            decoration:
                BoxDecoration(color: AppDesign.chip, borderRadius: U4.rBtn),
            child: Text(
                [store.source.srcName, store.source.srcGroup]
                    .where((s) => s.isNotEmpty)
                    .join(' · '),
                style: AppDesign.type(16, color: U4.text7)))
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
      _ProcessingSlider(
          label: 'Trim',
          compact: !wide,
          color: U4.cyan,
          value: trim,
          min: -18,
          max: 18,
          text: '${trim.toStringAsFixed(1)} dB',
          enabled: store.supports('trim'),
          onFocus: () => onFocus(ChannelFocus(
              title: 'Input',
              label: 'Trim',
              text: () =>
                  '${(store.paramNum('trim') ?? store.preamp.trim).toStringAsFixed(1)} dB',
              min: -18,
              max: 18,
              apply: (v) => store.setPreamp('trim', v))),
          onChanged: (v) => store.setPreamp('trim', v),
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
                style: AppDesign.type(14, color: U4.text7)))
      ]),
      SizedBox(height: wide ? 24 : 14),
      _InputAssignments(store: store, wide: wide),
    ]);
    final sends =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (!wide) ...[
        const SizedBox(height: 18),
        Text('Sends', style: AppDesign.type(14, color: U4.text7))
      ],
      if (store.sends.isEmpty)
        Padding(
            padding: const EdgeInsets.symmetric(vertical: 16),
            child: Text('No send slots on this desk.',
                style: AppDesign.type(14, color: U4.text7))),
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
      Container(width: 1, height: 760, color: U4.lineCtl),
      Expanded(
          flex: 806,
          child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 20),
              child: sends)),
    ]);
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
        for (var g = 1; g <= store.subgroupCount; g++)
          _ProcessingButton(
              label: 'Sub $g',
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
        for (var g = 1; g <= store.muteGroupCount; g++)
          _ProcessingButton(
              label: 'Mute $g',
              active: store.muteGroupAssigned(g) == true,
              onTap: store.muteGroupAssigned(g) == null
                  ? null
                  : () =>
                      store.cmdMuteGroupAssign(g, !store.muteGroupAssigned(g)!))
      ],
    };
    final selected = [
      if (store.mainAssigned == true) 'Main',
      for (var g = 1; g <= store.subgroupCount; g++)
        if (store.subgroupAssigned(g) == true) 'Sub $g',
      for (var g = 0; g < store.dcaCount; g++)
        if (store.dcaAssigned(g) == true) 'VCA ${g + 1}',
      for (var g = 1; g <= store.muteGroupCount; g++)
        if (store.muteGroupAssigned(g) == true) 'Mute $g'
    ].join(' / ');
    final columns = [
      for (final entry in groups.entries)
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(entry.key,
              textAlign: TextAlign.center,
              style: AppDesign.type(16, color: U4.text7)),
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
            backgroundColor: AppDesign.slot,
            collapsedBackgroundColor: AppDesign.slot,
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
                      style: AppDesign.type(12, color: U4.text7))),
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
            return _ProcessingSlider(
                label: send.destName,
                compact: !wide,
                value: level,
                min: 0,
                max: 1,
                text: '${fmtDb(level)} dB',
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
