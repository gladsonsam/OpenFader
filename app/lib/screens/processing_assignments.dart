part of 'channel.dart';

class _InputAssignments extends StatelessWidget {
  final ChannelStore store;
  final bool wide;
  final ValueChanged<ChannelFocus> onFocus;
  const _InputAssignments(
      {required this.store, required this.wide, required this.onFocus});

  String _summaryLabel(String family, String label) =>
      label.startsWith('$family ') ? label : '$family: $label';

  @override
  Widget build(BuildContext context) {
    final assignmentFont = wide && store.viewGroupCount > 0 ? 14.0 : 16.0;
    final controlGroup = store.dcaCount > 0 &&
            store.dcaGroupLabel(0).toUpperCase().startsWith('DCA')
        ? 'DCA'
        : 'VCA';
    final groups = <String, List<Widget>>{
      if (store.supports('automix_group'))
        'Automix': [
          for (var group = 0; group < 3; group++)
            _ProcessingButton(
                fontSize: assignmentFont,
                label: const ['Off', 'X', 'Y'][group],
                active: store.paramNum('automix_group') == group,
                onTap: store.paramNum('automix_group') == null ||
                        store.localAccessMessage('automix_group') != null
                    ? null
                    : () => store.cmdParam('automix_group', group.toDouble())),
        ],
      if (store.paramApplies('lr_assign'))
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
      if (store.subgroupCount > 0)
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
      if (store.dcaCount > 0)
        controlGroup: [
          for (var g = 0; g < store.dcaCount; g++)
            _ProcessingButton(
                fontSize: assignmentFont,
                label: store.dcaGroupLabel(g),
                active: store.dcaAssigned(g) == true,
                onTap: store.dcaAssigned(g) == null
                    ? null
                    : () => store.setDcaAssigned(g, !store.dcaAssigned(g)!))
        ],
      if (store.muteGroupCount > 0)
        'Mute': [
          for (var g = 0; g < store.muteGroupCount; g++)
            _ProcessingButton(
                fontSize: assignmentFont,
                label: 'Mute ${g + 1}',
                active: store.muteGroupAssigned(g) == true,
                onTap: store.muteGroupAssigned(g) == null
                    ? null
                    : () => store.cmdMuteGroupAssign(
                        g, !store.muteGroupAssigned(g)!))
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
          _summaryLabel(controlGroup, store.dcaGroupLabel(g)),
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
    final Widget assignments;
    if (wide) {
      assignments =
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        for (var i = 0; i < columns.length; i++) ...[
          if (i > 0) const SizedBox(width: 8),
          Expanded(child: columns[i])
        ]
      ]);
    } else {
      assignments = Theme(
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
              collapsedShape:
                  const RoundedRectangleBorder(borderRadius: U4.rBtn),
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
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      assignments,
      if (store.automixSettings.isNotEmpty) ...[
        const SizedBox(height: 12),
        Text('Automix settings',
            style: AppDesign.type(14, color: ConsolePalette.of(context).text7)),
        const SizedBox(height: 8),
        LayoutBuilder(builder: (context, box) {
          final count = box.maxWidth >= 640 ? 4 : 2;
          final width = (box.maxWidth - 8 * (count - 1)) / count;
          return Wrap(spacing: 8, runSpacing: 8, children: [
            for (final control in store.automixSettings)
              SizedBox(
                  width: width,
                  child: _ProcessingButton(
                      fontSize: 14,
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
                          : null)),
          ]);
        }),
      ],
      if (store.paramApplies('automix_weight')) ...[
        const SizedBox(height: 12),
        _AutomixWeight(store: store, onFocus: onFocus),
      ],
      if (store.paramApplies('provider:automix_gain')) ...[
        const SizedBox(height: 12),
        _AutomixGainMeter(store: store, onFocus: onFocus),
      ],
    ]);
  }
}

class _AutomixWeight extends StatelessWidget {
  final ChannelStore store;
  final ValueChanged<ChannelFocus> onFocus;
  const _AutomixWeight({required this.store, required this.onFocus});

  @override
  Widget build(BuildContext context) {
    const leaf = 'automix_weight';
    final display =
        ParameterNumericDisplay.fromMetadata(store.paramMetadata(leaf));
    final value = store.paramNum(leaf);
    final bounds = _controlRange(store, leaf);
    final epoch = store.connectionEpoch;
    final accessEpoch = store.controlAccessEpoch;
    final key = store.stripKey;
    bool current() =>
        store.connectionEpoch == epoch &&
        store.controlAccessEpoch == accessEpoch &&
        store.stripKey == key &&
        store.localAccessMessage(leaf) == null;
    List<double>? range() {
      final native = current() ? _controlRange(store, leaf) : null;
      return native == null
          ? null
          : [
              display?.toDisplay(native.first) ?? native.first,
              display?.toDisplay(native.last) ?? native.last,
            ];
    }

    String text() {
      final live = store.paramNum(leaf);
      return store.localAccessMessage(leaf) ??
          (live == null
              ? 'Loading…'
              : display?.label(live) ?? live.toStringAsFixed(2));
    }

    void apply(double v) {
      final native = current() ? _controlRange(store, leaf) : null;
      if (native == null || !v.isFinite) return;
      store.cmdParam(
          leaf, (display?.toNative(v) ?? v).clamp(native.first, native.last));
    }

    void focus() => onFocus(ChannelFocus(
          title: 'Automix',
          label: 'Weight',
          valueListenable: store,
          text: () => current() ? text() : 'Reopen control',
          entryText: () {
            final live = current() ? store.paramNum(leaf) : null;
            return live == null
                ? ''
                : (display?.toDisplay(live) ?? live)
                    .toStringAsFixed(display == null ? 2 : 1);
          },
          min: 0,
          max: 1,
          resolveRange: range,
          apply: apply,
        ));
    return _ProcessingSlider(
      label: 'Automix weight',
      compact: true,
      value: display?.toDisplay(value ?? bounds?.first ?? 0) ?? value ?? 0,
      min: display?.min ?? bounds?.first ?? 0,
      max: display?.max ?? bounds?.last ?? 1,
      text: text(),
      enabled: bounds != null && current(),
      onFocus: focus,
      onChanged: apply,
    );
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
    final accessEpoch = store.controlAccessEpoch;
    final stripKey = store.stripKey;
    bool current() =>
        store.connectionEpoch == epoch &&
        store.controlAccessEpoch == accessEpoch &&
        store.stripKey == stripKey;
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
                        animationStyle: AnimationStyle.noAnimation,
                        context: context,
                        barrierDismissible: false,
                        builder: (_) => DestinationControlEditor(
                            accessEpoch: () => store.controlAccessEpoch,
                            changes: store,
                            isCurrent: store.destinationControlIsCurrent,
                            apply: store.setDestinationControl,
                            control: control))
                    : null)),
    ]);
  }
}

class _AutomixGainMeter extends StatelessWidget {
  final ChannelStore store;
  final ValueChanged<ChannelFocus> onFocus;
  const _AutomixGainMeter({required this.store, required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final key = store.stripKey;
    final epoch = store.connectionEpoch;
    bool active() {
      final group = store.paramNum('automix_group');
      final id = group == 1
          ? 'enable_x'
          : group == 2
              ? 'enable_y'
              : null;
      return id != null &&
          store.automixSettings
                  .where((control) => control.id == id)
                  .firstOrNull
                  ?.value ==
              1;
    }

    String readout() {
      if (store.stripKey != key || store.connectionEpoch != epoch) {
        return 'Reopen control';
      }
      if (store.paramNum('automix_group') == 0) return 'Off';
      final group = store.paramNum('automix_group');
      if (group != 1 && group != 2) return 'Unavailable';
      final enabled = store.automixSettings
          .where(
              (control) => control.id == (group == 1 ? 'enable_x' : 'enable_y'))
          .firstOrNull
          ?.value;
      if (enabled == 0) return 'Group disabled';
      final value = store.automixGain.value;
      if (!active() || value == null) return 'Unavailable';
      final rounded = value.abs() < .05 ? 0.0 : value;
      return '${rounded > 0 ? '+' : ''}${rounded.toStringAsFixed(1)} dB';
    }

    void focus() => onFocus(ChannelFocus(
          title: 'Automix',
          label: 'Gain',
          text: readout,
          valueListenable: Listenable.merge([store, store.automixGain]),
          min: 0,
          max: 0,
          resolveRange: () => null,
          apply: (_) {},
        ));
    return ValueListenableBuilder<double?>(
      valueListenable: store.automixGain,
      builder: (context, value, _) => Semantics(
        label: 'Automix gain',
        value: readout(),
        button: true,
        excludeSemantics: true,
        onTap: focus,
        child: InkWell(
            onTap: focus,
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Row(children: [
                Text('Automix gain',
                    style: AppDesign.type(14, color: palette.text7)),
                const SizedBox(width: 12),
                Expanded(
                    child: !active() || value == null
                        ? Text(readout(),
                            textAlign: TextAlign.right,
                            style: AppDesign.type(12, color: palette.text7))
                        : LinearProgressIndicator(
                            minHeight: 10,
                            value: math
                                .pow(10, value / 20)
                                .toDouble()
                                .clamp(0.0, 1.0),
                            backgroundColor: palette.well,
                            color: palette.lime,
                          )),
              ]),
            )),
      ),
    );
  }
}
