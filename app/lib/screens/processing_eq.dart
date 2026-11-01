part of 'channel.dart';

class _EqTab extends StatefulWidget {
  final ChannelStore store;
  final ValueChanged<ChannelFocus> onFocus;
  const _EqTab({required this.store, required this.onFocus});
  @override
  State<_EqTab> createState() => _EqTabState();
}

class _EqTabState extends State<_EqTab> {
  bool _showRta = true;
  int? _spectrumSession;

  void _followSpectrumSession() {
    final session = widget.store.connectionEpoch;
    if (_spectrumSession == session) return;
    _spectrumSession = session;
    widget.store.setChannelSpectrumActive(true, owner: this);
  }

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_followSpectrumSession);
    _followSpectrumSession();
  }

  @override
  void dispose() {
    widget.store.removeListener(_followSpectrumSession);
    widget.store.setChannelSpectrumActive(false, owner: this);
    super.dispose();
  }

  void _pick(int band) {
    widget.store.selectEqBand(band);
    widget.onFocus(eqFreqFocus(widget.store, band));
  }

  Widget _slider(EqBandVm band, String prop, String label) {
    final range = switch (prop) {
      'g' => band.gRange,
      'f' => band.fRange,
      _ => band.qRange,
    };
    final value = switch (prop) { 'g' => band.g, 'f' => band.f, _ => band.q };
    final min = range?.first ?? 0;
    final max = range?.last ?? 1;
    ChannelFocus focus() => switch (prop) {
          'g' => _eqGainFocus(widget.store, band.band),
          'f' => eqFreqFocus(widget.store, band.band),
          _ => _eqQFocus(widget.store, band.band),
        };
    return Padding(
        padding: const EdgeInsets.only(top: 6),
        child: _ProcessingSlider(
            label: label,
            text: focus().text(),
            value: value,
            min: min,
            max: max,
            compact: true,
            inlineLabel: MediaQuery.sizeOf(context).width < 700,
            color: ConsolePalette.of(context).bandColor(band.band),
            enabled: range != null,
            onFocus: () {
              _pick(band.band);
              widget.onFocus(focus());
            },
            onChanged: (v) {
              widget.store.setEqParam(band.band, prop, v);
              widget.onFocus(focus());
            }));
  }

  Widget _hpf() {
    final store = widget.store;
    final eq = store.eq;
    return Padding(
        padding: const EdgeInsets.only(top: 10),
        child: Row(children: [
          SizedBox(
              width: 84,
              child: _ProcessingButton(
                  label: 'HPF',
                  active: eq.hpfOn,
                  onTap: store.supports('hpf_on')
                      ? () => store.setEqBandOn(0, !eq.hpfOn)
                      : null)),
          const SizedBox(width: 8),
          Expanded(
              child: _ProcessingSlider(
                  label: 'HPF frequency',
                  text: '${eq.hpfF.toStringAsFixed(0)} Hz',
                  value: eq.hpfF,
                  min: eq.hpfRange?.first ?? 0,
                  max: eq.hpfRange?.last ?? 1,
                  showLabel: false,
                  color: ConsolePalette.of(context).bandColor(0),
                  enabled: eq.hpfRange != null,
                  onFocus: () => widget.onFocus(eqFreqFocus(store, 0)),
                  onChanged: (v) {
                    store.setEqParam(0, 'f', v);
                    widget.onFocus(eqFreqFocus(store, 0));
                  })),
        ]));
  }

  Widget _shapeChoices(EqBandVm band, {bool compact = false}) {
    final enabled =
        band.type != null && widget.store.supports('eq${band.band}_type');
    if (band.typeChoices.length > 2) {
      return DropdownButton<EqFilterShape>(
        isExpanded: true,
        value: EqFilterShape.fromValue(band.type),
        hint: const Text('Shape'),
        items: [
          for (final choice in band.typeChoices)
            if (EqFilterShape.fromValue(choice.value.toInt()) case final shape?)
              DropdownMenuItem(value: shape, child: Text(choice.label)),
        ],
        onChanged: enabled
            ? (shape) {
                if (shape != null) widget.store.setEqBandType(band.band, shape);
              }
            : null,
      );
    }
    return Wrap(spacing: 2, runSpacing: 2, children: [
      for (final choice in band.typeChoices)
        if (EqFilterShape.fromValue(choice.value.toInt()) case final shape?)
          _ProcessingButton(
            label: choice.label,
            semanticsLabel: 'Band ${band.band} ${choice.label}',
            fontSize: compact ? 12 : 14,
            active: band.type == shape.value,
            onTap: enabled
                ? () => widget.store.setEqBandType(band.band, shape)
                : null,
          ),
    ]);
  }

  Widget _panel(EqBandVm band, double height) => Container(
      height: height,
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
          color: ConsolePalette.of(context).tileBg,
          borderRadius: U4.rWell,
          border: Border.all(color: ConsolePalette.of(context).linePanel)),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _ProcessingButton(
            label: 'Band ${band.band}',
            active: band.on,
            onTap: widget.store.supports('eq${band.band}_on')
                ? () => widget.store.setEqBandOn(band.band, !band.on)
                : null),
        const Spacer(),
        if (band.typeChoices.isNotEmpty) _shapeChoices(band),
        _slider(band, 'g', 'G'),
        _slider(band, 'f', 'F'),
        _slider(band, 'q', 'Q'),
      ]));

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final eq = store.eq;
    final wide = MediaQuery.sizeOf(context).width >= 700;
    final selected =
        eq.bands.where((b) => b.band == eq.selectedBand).firstOrNull ??
            eq.bands.firstOrNull;
    final showMobileShapes =
        selected != null && selected.typeChoices.isNotEmpty;
    return LayoutBuilder(builder: (context, bounds) {
      final scale = bounds.maxWidth / 1443;
      return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _ProcessorHeader(
            title: 'EQ',
            on: eq.on,
            compact: !wide,
            onToggle: () => store.setEqOn(!eq.on)),
        const SizedBox(height: 12),
        Row(children: [
          if (wide)
            SizedBox(
                width: 460 * scale,
                child: _PresetsButton(
                    store: store, compact: false, kind: ProcessorPresetKind.eq))
          else
            Expanded(
                child: _PresetsButton(
                    store: store, compact: true, kind: ProcessorPresetKind.eq)),
          ListenableBuilder(
              listenable:
                  Listenable.merge([store.spectrum, store.audioSpectrum]),
              builder: (context, _) {
                final available = store.audioSpectrum.value != null ||
                    store.spectrum.value.isNotEmpty;
                if (!available) return const SizedBox.shrink();
                return Padding(
                  padding: const EdgeInsets.only(left: 8),
                  child: SizedBox(
                    width: wide ? 248 * scale : null,
                    child: Semantics(
                      toggled: _showRta,
                      child: _ProcessingButton(
                        label: store.audioSpectrum.value?.isShared == true
                            ? 'RTA · shared'
                            : 'RTA',
                        active: _showRta,
                        onTap: () {
                          store.resetAudioSpectrumPeaks();
                          setState(() => _showRta = !_showRta);
                        },
                      ),
                    ),
                  ),
                );
              }),
          if (wide) const Spacer(),
        ]),
        if (_showRta && store.rtaSettings.isNotEmpty)
          _RtaSettings(store: store),
        const SizedBox(height: 8),
        if (eq.responseNote != null)
          Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text(eq.responseNote!,
                  style: AppDesign.type(14,
                      color: ConsolePalette.of(context).text7))),
        Opacity(
            opacity: eq.on || ConsolePalette.of(context).isLight ? 1 : .55,
            child: ListenableBuilder(
                listenable: Listenable.merge([
                  store.spectrum,
                  store.audioSpectrum,
                  store.audioSpectrumPeaks,
                ]),
                builder: (context, _) {
                  final bins = store.spectrum.value;
                  final frame = store.audioSpectrum.value;
                  return Stack(children: [
                    U4EqGraph(
                      height: wide ? 665 * scale : bounds.maxWidth * 344 / 346,
                      eq: eq,
                      responseDb: eqResponseDb(eq),
                      rtaBins: bins,
                      audioSpectrum: frame,
                      audioSpectrumPeaks: store.audioSpectrumPeaks.value,
                      rtaActive: _showRta && (frame != null || bins.isNotEmpty),
                      onSelectBand: _pick,
                      onNode: (band, f, g) {
                        if (band == 0) {
                          store.setEqParam(0, 'f', f);
                        } else {
                          store.setEqParam(band, 'f', f);
                          store.setEqParam(band, 'g', g);
                        }
                        widget.onFocus(eqFreqFocus(store, band));
                      },
                      onQ: (band, q) {
                        store.setEqParam(band, 'q', q);
                        widget.onFocus(_eqQFocus(store, band));
                      },
                    ),
                    if (frame != null)
                      Positioned(
                          top: 4,
                          left: 40,
                          right: 14,
                          child: IgnorePointer(
                              child: Align(
                                  alignment: Alignment.topLeft,
                                  child: Container(
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: 6, vertical: 3),
                                      decoration: BoxDecoration(
                                          color:
                                              ConsolePalette.of(context).chip,
                                          borderRadius: U4.rBtn),
                                      child: Text(frame.label,
                                          style: AppDesign.type(14,
                                              color: ConsolePalette.of(context)
                                                  .cyan,
                                              weight: FontWeight.w600))))))
                  ]);
                })),
        const SizedBox(height: 8),
        if (wide)
          Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
            for (var i = 0; i < eq.bands.length; i++) ...[
              if (i > 0) const SizedBox(width: 10),
              Expanded(
                  child: _panel(eq.bands[i], (500 * scale).clamp(360, 500))),
            ],
          ])
        else ...[
          Row(children: [
            Expanded(
                child: Container(
                    height: 44,
                    decoration: BoxDecoration(
                        color: ConsolePalette.of(context).chip,
                        borderRadius: U4.rBtn),
                    padding: const EdgeInsets.all(3),
                    child: Row(children: [
                      for (final band in eq.bands)
                        Expanded(
                            child: Semantics(
                                button: true,
                                selected: eq.selectedBand == band.band,
                                label: 'EQ Band ${band.band}',
                                child: InkWell(
                                    onTap: () => _pick(band.band),
                                    child: Container(
                                        alignment: Alignment.center,
                                        decoration: BoxDecoration(
                                            color: eq.selectedBand == band.band
                                                ? ConsolePalette.of(context)
                                                    .processingSelected
                                                : null,
                                            borderRadius: U4.rBtn),
                                        child: Text(
                                            showMobileShapes
                                                ? '${band.band}'
                                                : 'Band ${band.band}',
                                            style: AppDesign.type(15,
                                                color: eq.selectedBand == band.band
                                                    ? ConsolePalette.of(context)
                                                        .cyan
                                                    : ConsolePalette.of(context)
                                                        .text7,
                                                weight: FontWeight.w600)))))),
                    ]))),
            if (showMobileShapes) ...[
              const SizedBox(width: 8),
              SizedBox(
                  width: 128, child: _shapeChoices(selected, compact: true)),
            ],
          ]),
          if (selected != null) ...[
            _slider(selected, 'g', 'G'),
            _slider(selected, 'f', 'F'),
            _slider(selected, 'q', 'Q'),
          ],
        ],
        if (store.paramApplies('hpf_on')) _hpf(),
        if (store.paramApplies('hpf_slope') &&
            parameterChoices(store.paramMetadata('hpf_slope')).isNotEmpty)
          Padding(
              padding: const EdgeInsets.only(top: 7),
              child: _ParameterChoices(
                  store: store,
                  leaf: 'hpf_slope',
                  label: 'Low cut slope',
                  fallback: 'Loading…',
                  compact: true,
                  onChanged: (value) => store.cmdParam('hpf_slope', value))),
        if (store.paramApplies('wing:hcf_on')) ...[
          const SizedBox(height: 14),
          _InputPreampControl(
              store: store,
              leaf: 'wing:hcf_freq',
              label: 'High cut',
              section: 'Input filters',
              onFocus: widget.onFocus,
              trailing: _ProcessingButton(
                  label: 'On',
                  active: store.paramBool('wing:hcf_on') == true,
                  semanticsLabel: 'High cut enabled',
                  onTap: store.supports('wing:hcf_on')
                      ? () => store.cmdParam(
                          'wing:hcf_on', store.paramBool('wing:hcf_on') != true)
                      : null)),
          const SizedBox(height: 7),
          _ParameterChoices(
              store: store,
              leaf: 'wing:hcf_slope',
              label: 'High cut slope',
              fallback: 'Loading…',
              compact: true,
              onChanged: (value) => store.cmdParam('wing:hcf_slope', value)),
        ],
        if (store.paramApplies('wing:filter_model')) ...[
          const SizedBox(height: 14),
          Row(children: [
            Expanded(
                child: _ParameterChoices(
                    store: store,
                    leaf: 'wing:filter_model',
                    label: 'Tool filter',
                    fallback: 'Loading…',
                    compact: true,
                    onChanged: (value) =>
                        store.cmdParam('wing:filter_model', value))),
            const SizedBox(width: 8),
            _ProcessingButton(
                label: 'On',
                active: store.paramBool('wing:filter_on') == true,
                semanticsLabel: 'Tool filter enabled',
                onTap: store.supports('wing:filter_on')
                    ? () => store.cmdParam('wing:filter_on',
                        store.paramBool('wing:filter_on') != true)
                    : null),
          ]),
          for (final control in const [
            ('wing:filter_tilt', 'Tilt'),
            ('wing:filter_low', 'Low contour'),
            ('wing:filter_high', 'High process'),
            ('wing:filter_ap1_freq', 'Frequency'),
            ('wing:filter_ap2_freq', 'Frequency'),
            ('wing:filter_ap2_q', 'Q'),
          ])
            if (store.paramMetadata(control.$1)?['hidden'] != true)
              Padding(
                  padding: const EdgeInsets.only(top: 7),
                  child: _InputPreampControl(
                      store: store,
                      leaf: control.$1,
                      label: control.$2,
                      section: 'Tool filter',
                      onFocus: widget.onFocus)),
        ],
      ]);
    });
  }
}

class _RtaSettings extends StatelessWidget {
  final ChannelStore store;
  const _RtaSettings({required this.store});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Wrap(spacing: 8, runSpacing: 8, children: [
      for (final row in store.rtaSettings)
        if (row.choices.isNotEmpty)
          PopupMenuButton<double>(
            popUpAnimationStyle: AnimationStyle.noAnimation,
            enabled: store.destinationControlIsCurrent(row),
            tooltip: 'Shared RTA ${row.label}',
            color: palette.processingPanel,
            onSelected: (value) {
              if (store.destinationControlIsCurrent(row) &&
                  row.allowsValue(value)) {
                store.setDestinationControl(row, value);
              }
            },
            itemBuilder: (_) => [
              for (final choice in row.choices)
                PopupMenuItem(value: choice.value, child: Text(choice.label)),
            ],
            child: Chip(label: Text('${row.label}: ${row.valueLabel ?? '—'}')),
          )
        else if (row.numeric)
          ActionChip(
            label: Text('${row.label}: ${row.valueLabel ?? '—'}'),
            onPressed: !store.destinationControlIsCurrent(row)
                ? null
                : () async {
                    var value = row.value!;
                    final selected = await showDialog<double>(
                      animationStyle: AnimationStyle.noAnimation,
                      context: context,
                      builder: (dialogContext) => StatefulBuilder(
                          builder: (dialogContext, setState) => AlertDialog(
                                title: Text('Shared RTA ${row.label}'),
                                content: Slider(
                                  value: value.clamp(row.min!, row.max!),
                                  min: row.min!,
                                  max: row.max!,
                                  label: value.toStringAsFixed(row.decimals),
                                  onChanged: (next) =>
                                      setState(() => value = next),
                                ),
                                actions: [
                                  TextButton(
                                      onPressed: () =>
                                          Navigator.pop(dialogContext),
                                      child: const Text('Cancel')),
                                  TextButton(
                                      onPressed: () =>
                                          Navigator.pop(dialogContext, value),
                                      child: const Text('Apply')),
                                ],
                              )),
                    );
                    if (selected != null &&
                        store.destinationControlIsCurrent(row) &&
                        row.allowsValue(selected)) {
                      await store.setDestinationControl(row, selected);
                    }
                  },
          ),
    ]);
  }
}
