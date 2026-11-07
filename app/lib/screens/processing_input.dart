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

  Future<void> _editInputSource(BuildContext context) async {
    final snapshot = store.inputSource;
    if (snapshot == null || !snapshot.editable) return;
    final choice = await showDialog<ChannelInputSourceChoice>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (dialogContext) => ListenableBuilder(
            listenable: store,
            builder: (context, _) {
              final current = store.inputSource?.context == snapshot.context &&
                  store.inputSource?.editable == true;
              return AlertDialog(
                  title: const Text('Input patch'),
                  scrollable: true,
                  content: SizedBox(
                      width: 420,
                      child: Column(mainAxisSize: MainAxisSize.min, children: [
                        CompactChoicePicker<ChannelInputSourceChoice>(
                            choices: snapshot.choices,
                            label: (option) => option.label,
                            selected: (option) => option.selected,
                            onSelected: current
                                ? (option) {
                                    if (!option.selected) {
                                      Navigator.pop(dialogContext, option);
                                    }
                                  }
                                : null),
                        if (!current)
                          const Text(
                              'The source or connection changed. Reopen this control.'),
                      ])),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(dialogContext),
                        child: const Text('Cancel')),
                  ]);
            }));
    if (choice != null && context.mounted) {
      store.setInputSource(snapshot, choice);
    }
  }

  Future<void> _editInsert(BuildContext context) async {
    final snapshot = store.inputInsert;
    if (snapshot == null || !snapshot.editable) return;
    var sending = false;
    String? error;
    await showDialog<void>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (dialogContext) => StatefulBuilder(
            builder: (context, setLocalState) => ListenableBuilder(
                listenable: store,
                builder: (context, _) {
                  final current =
                      store.inputInsert?.context == snapshot.context &&
                          store.inputInsert?.editable == true;
                  return AlertDialog(
                      title: const Text('Insert'),
                      scrollable: true,
                      content: SizedBox(
                          width: 420,
                          child:
                              Column(mainAxisSize: MainAxisSize.min, children: [
                            CompactChoicePicker<DestinationChoiceVm>(
                                choices: snapshot.choices,
                                label: (option) => option.label,
                                selected: (option) =>
                                    option.value == snapshot.value,
                                onSelected: current && !sending
                                    ? (option) async {
                                        if (option.value == snapshot.value) {
                                          return;
                                        }
                                        setLocalState(() {
                                          sending = true;
                                          error = null;
                                        });
                                        final result =
                                            await store.setInputInsert(
                                                snapshot, option.value);
                                        if (!dialogContext.mounted) return;
                                        if (result == null) {
                                          Navigator.pop(dialogContext);
                                        } else {
                                          setLocalState(() {
                                            sending = false;
                                            error = result;
                                          });
                                        }
                                      }
                                    : null),
                            if (!current)
                              const Text(
                                  'The channel or connection changed. Reopen this control.'),
                            if (sending) const LinearProgressIndicator(),
                            if (error != null) ...[
                              const SizedBox(height: 8),
                              Text(error!),
                            ],
                          ])),
                      actions: [
                        TextButton(
                            onPressed: sending
                                ? null
                                : () => Navigator.pop(dialogContext),
                            child: const Text('Cancel')),
                      ]);
                })));
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 700;
    final palette = ConsolePalette.of(context);
    final src = store.source;
    String sourceName(String leaf, int? value) => value == null
        ? '-'
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
    final hasSource = store.inputSource != null ||
        store.paramApplies('source') ||
        store.paramApplies('source_a') ||
        store.paramApplies('usbreturn') ||
        store.paramApplies('provider:input_mode') ||
        store.paramApplies('provider:source_auto');
    final hasPreamp = store.paramApplies('gain') ||
        store.paramApplies('trim') ||
        store.paramApplies('polarity') ||
        store.paramApplies('phantom') ||
        store.paramApplies('provider:input_balance') ||
        store.paramApplies('provider:width');
    final hasSignalPath = store.hasDelay ||
        store.paramApplies('delay_ms') ||
        store.inputInserts.isNotEmpty ||
        store.paramApplies('insert_on') ||
        store.paramApplies('insert_sel') ||
        store.paramApplies('provider:insert_position');
    final hasAssignments = store.supports('automix_group') ||
        store.paramApplies('lr_assign') ||
        store.subgroupCount > 0 ||
        store.dcaCount > 0 ||
        store.muteGroupCount > 0 ||
        store.viewGroupCount > 0 ||
        store.automixSettings.isNotEmpty ||
        store.paramApplies('automix_weight') ||
        store.paramApplies('provider:automix_gain');

    final sourceControls =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      // The active provider source is independent of an editable patch in a
      // dormant input bank. Never use that patch as the active-source readout.
      if (store.paramApplies('source') || store.inputSource != null) ...[
        _InputReadout(label: 'Active source', value: src.srcName),
        const SizedBox(height: 8),
      ],
      if (store.inputSource != null)
        _ProcessingButton(
            label: 'Input patch',
            detail: store.inputSource!.editable
                ? store.inputSource!.label
                : store.inputSource!.unavailableReason ??
                    store.inputSource!.label,
            semanticsLabel: 'Choose input patch',
            onTap: store.inputSource!.editable
                ? () => _editInputSource(context)
                : null)
      else if (sources.isNotEmpty)
        Row(children: [
          for (var i = 0; i < sources.length; i++) ...[
            if (i > 0) const SizedBox(width: 8),
            Expanded(
                child: _ProcessingButton(
                    label: 'Source ${sources[i].label}',
                    detail: sources[i].name,
                    active: sources[i].selected,
                    onTap: sources[i].enabled
                        ? () =>
                            store.setSourceSelected(int.parse(sources[i].id))
                        : null)),
          ]
        ]),
      if (store.paramApplies('provider:input_mode')) ...[
        const SizedBox(height: 10),
        if (store.sourceMode?.editable == true)
          _ProcessingButton(
              label: 'Source mode',
              detail: store.sourceMode!.label,
              semanticsLabel: 'Edit shared source mode',
              onTap: () => _editSourceMode(context))
        else
          _InputReadout(
              label: 'Source mode',
              value: _choiceText(store, 'provider:input_mode', 'Loading…')),
      ],
      if (store.paramApplies('provider:source_auto')) ...[
        const SizedBox(height: 8),
        _InputToggleRow(
            label: 'Auto alternate source',
            value:
                store.paramBool('provider:source_auto') == true ? 'On' : 'Off',
            active: store.paramBool('provider:source_auto') == true,
            enabled: store.supports('provider:source_auto'),
            onTap: () => store.cmdParam('provider:source_auto',
                store.paramBool('provider:source_auto') != true)),
      ],
    ]);

    final preampControls =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (store.paramApplies('gain')) ...[
        _InputPreampControl(
            store: store, leaf: 'gain', label: 'Gain', onFocus: onFocus),
        const SizedBox(height: 8),
      ],
      if (store.paramApplies('trim')) ...[
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
        const SizedBox(height: 8),
      ] else if (store.paramApplies('polarity')) ...[
        _InputToggleRow(
            label: 'Polarity',
            value: pre.invert ? 'Inverted' : 'Normal',
            active: pre.invert,
            enabled: store.supports('polarity'),
            buttonLabel: 'Ø',
            onTap: store.toggleInvert),
        const SizedBox(height: 8),
      ],
      if (store.paramApplies('provider:input_balance')) ...[
        _InputPreampControl(
            store: store,
            leaf: 'provider:input_balance',
            label: 'Input balance',
            onFocus: onFocus),
        const SizedBox(height: 8),
      ],
      if (store.paramApplies('provider:width')) ...[
        _InputPreampControl(
            store: store,
            leaf: 'provider:width',
            label: 'Stereo width',
            section: 'Stereo',
            onFocus: onFocus),
        const SizedBox(height: 8),
      ],
      if (store.paramApplies('phantom'))
        Align(
            alignment: Alignment.centerLeft,
            child: SizedBox(
                width: 72,
                child: Tooltip(
                    message: store.preampPhantomAvailable == false
                        ? '48V unavailable for this source'
                        : pre.phantom
                            ? '48V on'
                            : '48V off',
                    child: _ProcessingButton(
                        label: '48V',
                        active: pre.phantom,
                        semanticsLabel: store.preampPhantomAvailable == false
                            ? '48V unavailable for this source'
                            : pre.phantom
                                ? 'Turn off 48V'
                                : 'Turn on 48V',
                        onTap: store.supports('phantom')
                            ? () => _confirmPhantom(context)
                            : null)))),
    ]);

    final signalPathControls =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (store.hasDelay) ...[
        _InputDelayControl(store: store, onFocus: onFocus),
        if (store.inputInserts.isNotEmpty ||
            store.paramApplies('insert_on') ||
            store.paramApplies('insert_sel') ||
            store.paramApplies('provider:insert_position'))
          const SizedBox(height: 12),
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
        const SizedBox(height: 12),
      ],
      if (store.inputInsert != null)
        _ProcessingButton(
            label: 'Insert',
            detail: store.inputInsert!.editable
                ? store.inputInsert!.label
                : store.inputInsert!.unavailableReason ??
                    store.inputInsert!.label,
            onTap:
                store.inputInsert!.editable ? () => _editInsert(context) : null)
      else if (store.inputInserts.isNotEmpty)
        for (var i = 0; i < store.inputInserts.length; i++) ...[
          if (i > 0) const SizedBox(height: 8),
          Builder(builder: (context) {
            final control = store.inputInserts[i];
            return _ProcessingButton(
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
            );
          }),
        ],
      // Provider-labelled simple insert controls supplement the richer surface.
      if (store.inputInserts.isEmpty && store.paramApplies('insert_on')) ...[
        _InputToggleRow(
            label: 'Insert',
            value: store.paramBool('insert_on') == true ? 'On' : 'Off',
            active: store.paramBool('insert_on') == true,
            enabled: store.supports('insert_on'),
            onTap: () => store.cmdParam(
                'insert_on', store.paramBool('insert_on') != true)),
        if (store.paramApplies('insert_sel') ||
            store.paramApplies('provider:insert_position'))
          const SizedBox(height: 8),
      ],
      if (store.inputInserts.isEmpty)
        for (final (leaf, label) in [
          ('insert_sel', 'Insert source'),
          ('provider:insert_position', 'Insert position'),
        ])
          if (store.paramApplies(leaf) &&
              parameterChoices(store.paramMetadata(leaf)).isNotEmpty) ...[
            _ParameterChoices(
                store: store,
                leaf: leaf,
                label: label,
                fallback: 'Loading…',
                compact: true,
                onChanged: (value) => store.cmdParam(leaf, value)),
            const SizedBox(height: 8),
          ],
    ]);

    final primary =
        Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (!wide) ...[
        Text('Input', style: AppDesign.type(20, weight: FontWeight.w600)),
        const SizedBox(height: 12),
      ],
      if (hasSource) ...[
        _InputSection(title: 'Source', child: sourceControls),
        const SizedBox(height: 12),
      ],
      if (hasPreamp) ...[
        _InputSection(title: 'Preamp', child: preampControls),
        const SizedBox(height: 12),
      ],
      if (hasSignalPath) ...[
        _InputSection(title: 'Signal path', child: signalPathControls),
        const SizedBox(height: 12),
      ],
      if (hasAssignments)
        _InputSection(
            title: 'Assignments & automix',
            child:
                _InputAssignments(store: store, wide: wide, onFocus: onFocus)),
    ]);

    final sends = _InputSection(
        title: 'Sends',
        child:
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          if (store.sends.isEmpty)
            Padding(
                padding: const EdgeInsets.symmetric(vertical: 8),
                child: Text('No sends available',
                    style: AppDesign.type(14, color: palette.text7))),
          if (store.supports('provider:send_tap')) ...[
            _ParameterChoices(
                store: store,
                leaf: 'provider:send_tap',
                label: 'Pre tap source',
                fallback: 'Loading…',
                compact: true,
                onChanged: (value) =>
                    store.cmdParam('provider:send_tap', value)),
            if (store.sends.isNotEmpty) const SizedBox(height: 12),
          ],
          for (var i = 0; i < store.sends.length; i++) ...[
            if (i > 0) const SizedBox(height: 10),
            _InputSendSurface(
                child: _ProcessingSend(
                    store: store,
                    send: store.sends[i],
                    wide: wide,
                    onFocus: onFocus)),
          ],
        ]));
    if (!wide) {
      return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [primary, const SizedBox(height: 12), sends]);
    }
    return Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Expanded(
          flex: 5,
          child: Padding(
              padding: const EdgeInsets.only(right: 8), child: primary)),
      Expanded(
          flex: 7,
          child:
              Padding(padding: const EdgeInsets.only(left: 8), child: sends)),
    ]);
  }
}

class _InputSection extends StatelessWidget {
  final String title;
  final Widget child;
  const _InputSection({required this.title, required this.child});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return DecoratedBox(
        decoration: BoxDecoration(
            color: palette.slot,
            border: Border.all(color: palette.slotBorder),
            borderRadius: U4.rBtn),
        child: Padding(
            padding: const EdgeInsets.fromLTRB(12, 11, 12, 12),
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(title,
                      style: AppDesign.type(16,
                          color: palette.text1, weight: FontWeight.w600)),
                  const SizedBox(height: 10),
                  child,
                ])));
  }
}

class _InputReadout extends StatelessWidget {
  final String label, value;
  const _InputReadout({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Container(
        constraints: const BoxConstraints(minHeight: 54),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
        decoration: BoxDecoration(
            color: palette.panel,
            border: Border.all(color: palette.linePanel),
            borderRadius: U4.rBtn),
        child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(label, style: AppDesign.type(12, color: palette.text7)),
              const SizedBox(height: 2),
              Text(value,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppDesign.type(16, color: palette.text1)),
            ]));
  }
}

class _InputToggleRow extends StatelessWidget {
  final String label, value, buttonLabel;
  final bool active, enabled;
  final VoidCallback onTap;
  const _InputToggleRow(
      {required this.label,
      required this.value,
      required this.active,
      required this.enabled,
      required this.onTap,
      this.buttonLabel = 'On'});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Row(children: [
      Expanded(
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
        Text(label, style: AppDesign.type(14, color: palette.text1)),
        const SizedBox(height: 2),
        Text(value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: AppDesign.type(12, color: palette.text7)),
      ])),
      const SizedBox(width: 10),
      SizedBox(
          width: 64,
          child: _ProcessingButton(
              label: buttonLabel,
              active: active,
              semanticsLabel: label,
              onTap: enabled ? onTap : null)),
    ]);
  }
}

class _InputSendSurface extends StatelessWidget {
  final Widget child;
  const _InputSendSurface({required this.child});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return DecoratedBox(
        decoration: BoxDecoration(
            color: palette.panel,
            border: Border.all(color: palette.linePanel),
            borderRadius: U4.rBtn),
        child: Padding(padding: const EdgeInsets.all(10), child: child));
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
          showValue: true,
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
        showValue: true,
        color: leaf == 'gain'
            ? ConsolePalette.of(context).mute
            : ConsolePalette.of(context).success,
        fillFromStart: true,
        solidFill: true,
        value: value ?? range?.min ?? 0,
        resetValue: range == null
            ? null
            : numericResetValue(range.min, range.max,
                nativeDefault: metadata?['default']),
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
