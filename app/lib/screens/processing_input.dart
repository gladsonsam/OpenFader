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
    final accessEpoch = store.controlAccessEpoch;
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
        store.controlAccessEpoch == accessEpoch &&
        store.stripKey == key &&
        sourceContext() == target &&
        store.supports('phantom') &&
        store.paramBool('phantom') == current;
    final confirmed = await showDialog<bool>(
        animationStyle: AnimationStyle.noAnimation,
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
        animationStyle: AnimationStyle.noAnimation,
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
                            for (final choice in snapshot.choices)
                              Padding(
                                  padding: const EdgeInsets.only(bottom: 7),
                                  child: _ProcessingButton(
                                      label: choice.label,
                                      active: selected == choice.value,
                                      onTap: current()
                                          ? () => setLocalState(
                                              () => selected = choice.value)
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
      if (store.paramApplies('source') ||
          store.paramApplies('source_a') ||
          store.paramApplies('usbreturn')) ...[
        Text('Source',
            style: AppDesign.type(wide ? 18 : 14,
                color: ConsolePalette.of(context).text7)),
        const SizedBox(height: 8),
        if (sources.isEmpty)
          Container(
              constraints: const BoxConstraints(minHeight: 60),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                  color: ConsolePalette.of(context).chip,
                  borderRadius: U4.rBtn),
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
      ],
      if (store.paramApplies('source') && store.canPatchInputSource)
        Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text('Input patching lives in Routing.',
                style: AppDesign.type(14,
                    color: ConsolePalette.of(context).text7))),
      if (store.paramApplies('provider:input_mode')) ...[
        const SizedBox(height: 12),
        if (store.sourceMode?.editable == true)
          _ProcessingButton(
              label: 'Source mode',
              detail: store.sourceMode!.label,
              semanticsLabel: 'Edit shared source mode',
              onTap: () => _editSourceMode(context))
        else
          Text(
              'Input mode · ${_choiceText(store, 'provider:input_mode', 'Loading…')}',
              style:
                  AppDesign.type(14, color: ConsolePalette.of(context).text7)),
      ],
      if (store.paramApplies('provider:source_auto')) ...[
        const SizedBox(height: 7),
        _ProcessingButton(
            label: 'Auto alternate source',
            active: store.paramBool('provider:source_auto') == true,
            onTap: store.supports('provider:source_auto')
                ? () => store.cmdParam('provider:source_auto',
                    store.paramBool('provider:source_auto') != true)
                : null),
      ],
      SizedBox(height: wide ? 88 : 14),
      if (store.paramApplies('gain')) ...[
        _InputPreampControl(
            store: store, leaf: 'gain', label: 'Gain', onFocus: onFocus),
        const SizedBox(height: 7),
      ],
      if (store.paramApplies('trim'))
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
      if (store.paramApplies('polarity') && !store.paramApplies('trim'))
        _ProcessingButton(
            label: 'Ø Invert polarity',
            active: pre.invert,
            semanticsLabel: 'Invert polarity',
            onTap: store.supports('polarity') ? store.toggleInvert : null),
      if (store.paramApplies('provider:input_balance')) ...[
        const SizedBox(height: 7),
        _InputPreampControl(
            store: store,
            leaf: 'provider:input_balance',
            label: 'Input balance',
            onFocus: onFocus),
      ],
      if (store.paramApplies('provider:width')) ...[
        const SizedBox(height: 7),
        _InputPreampControl(
            store: store,
            leaf: 'provider:width',
            label: 'Stereo width',
            section: 'Stereo',
            onFocus: onFocus),
      ],
      const SizedBox(height: 7),
      if (store.paramApplies('phantom'))
        Row(children: [
          SizedBox(
              width: 64,
              child: _TextSwitch(
                  label: '48V',
                  active: pre.phantom,
                  enabled: store.supports('phantom'),
                  onTap: () => _confirmPhantom(context))),
          Expanded(
              child: Text(
                  store.preampPhantomAvailable == false
                      ? 'Phantom unavailable for this source'
                      : 'Phantom power',
                  style: AppDesign.type(14,
                      color: ConsolePalette.of(context).text7)))
        ]),
      SizedBox(height: wide ? 24 : 14),
      if (store.hasDelay) ...[
        _InputDelayControl(store: store, onFocus: onFocus),
        SizedBox(height: wide ? 24 : 14),
      ],
      // Native delay leaves without a structured unit codec (X32
      // `delay/time` 0.3..500 ms, protocol reference v4.09-02): generic direct
      // control where the provider range proves valid, never a codec.
      if (!store.hasDelay && store.paramApplies('delay_ms')) ...[
        _InputPreampControl(
            store: store,
            leaf: 'delay_ms',
            label: 'Delay',
            onFocus: onFocus,
            trailing: _ProcessingButton(
                label: 'On',
                active: store.paramBool('delay_on') == true,
                semanticsLabel: 'Delay enabled',
                onTap: store.supports('delay_on')
                    ? () => store.cmdParam(
                        'delay_on', store.paramBool('delay_on') != true)
                    : null)),
        SizedBox(height: wide ? 24 : 14),
      ],
      if (store.inputInserts.isNotEmpty) ...[
        Text('Insert',
            style: AppDesign.type(14, color: ConsolePalette.of(context).text7)),
        const SizedBox(height: 8),
        for (final control in store.inputInserts)
          Padding(
            padding: const EdgeInsets.only(bottom: 7),
            child: _ProcessingButton(
              label: control.label,
              detail: control.valueLabel,
              onTap: control.available &&
                      store.destinationControlIsCurrent(control)
                  ? () => showDialog<void>(
                      animationStyle: AnimationStyle.noAnimation,
                      context: context,
                      builder: (_) => DestinationControlEditor(
                          changes: store,
                          accessEpoch: () => store.controlAccessEpoch,
                          isCurrent: store.destinationControlIsCurrent,
                          apply: store.setDestinationControl,
                          control: control))
                  : null,
            ),
          ),
        const SizedBox(height: 12),
      ],
      // Provider-labelled simple insert controls supplement the toggle
      // when no richer insert surface is advertised.
      if (store.inputInserts.isEmpty && store.paramApplies('insert_on'))
        Padding(
          padding: const EdgeInsets.only(bottom: 12),
          child: _ProcessingButton(
              label: 'Insert',
              detail: store.paramBool('insert_on') == true ? 'On' : 'Off',
              active: store.paramBool('insert_on') == true,
              semanticsLabel: 'Insert enabled',
              onTap: store.supports('insert_on')
                  ? () => store.cmdParam(
                      'insert_on', store.paramBool('insert_on') != true)
                  : null),
        ),
      if (store.inputInserts.isEmpty)
        for (final (leaf, label) in [
          ('insert_sel', 'Insert source'),
          ('provider:insert_position', 'Insert position'),
        ])
          if (store.paramApplies(leaf) &&
              parameterChoices(store.paramMetadata(leaf)).isNotEmpty)
            Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: _ParameterChoices(
                    store: store,
                    leaf: leaf,
                    label: label,
                    fallback: 'Loading…',
                    compact: true,
                    onChanged: (value) => store.cmdParam(leaf, value))),
      _InputAssignments(store: store, wide: wide, onFocus: onFocus),
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
      if (store.supports('provider:send_tap'))
        Padding(
            padding: const EdgeInsets.only(top: 12),
            child: _ParameterChoices(
                store: store,
                leaf: 'provider:send_tap',
                label: 'Pre tap source',
                fallback: 'Loading…',
                compact: true,
                onChanged: (value) =>
                    store.cmdParam('provider:send_tap', value))),
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
    final accessEpoch = store.controlAccessEpoch;
    final key = store.stripKey;
    bool current() =>
        snapshot != null &&
        store.connectionEpoch == epoch &&
        store.controlAccessEpoch == accessEpoch &&
        store.stripKey == key &&
        store.delay?.context == snapshot.context;
    String readout(ChannelDelayVm value) =>
        '${value.value.toStringAsFixed(value.precision)} ${value.unitLabel}';
    void apply(double value) {
      if (current()) store.setDelay(snapshot!, value);
    }

    void focus() {
      if (snapshot == null || !snapshot.editable || !current()) return;
      onFocus(ChannelFocus(
          title: 'Delay',
          label: snapshot.unitLabel,
          valueListenable: store,
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
          leaf: 'provider:delay_unit',
          label: 'Delay unit',
          fallback: 'Loading…',
          compact: true,
          onChanged: (value) => store.cmdParam('provider:delay_unit', value)),
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
    final text = store.localAccessMessage(leaf) ??
        (!store.capabilitiesReady
            ? 'Loading…'
            : leaf == 'gain' && store.preampGainAvailable == false
                ? 'Unavailable for this source'
                : metadata == null
                    ? 'Not supported'
                    : value == null || !store.supports(leaf)
                        ? 'Loading…'
                        : range == null
                            ? '${readout(value)} · range unavailable'
                            : readout(value));
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
