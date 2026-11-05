part of 'channel.dart';

List<double>? _controlRange(ChannelStore store, String leaf) {
  if (parameterChoices(store.paramMetadata(leaf)).isNotEmpty) return null;
  if (!store.supports(leaf) || store.paramNum(leaf)?.isFinite != true) {
    return null;
  }
  return parameterRange(store.paramMetadata(leaf));
}

ChannelFocus _parameterFocus(ChannelStore store, String leaf, String title,
    String label, String Function() text, ValueChanged<double> update) {
  final epoch = store.connectionEpoch;
  final accessEpoch = store.controlAccessEpoch;
  final key = store.stripKey;
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

  final target = leaf == 'gain' || leaf == 'phantom' ? sourceContext() : null;
  bool current() =>
      store.connectionEpoch == epoch &&
      store.controlAccessEpoch == accessEpoch &&
      store.stripKey == key &&
      (target == null || sourceContext() == target);
  List<double>? range() => current() ? _controlRange(store, leaf) : null;

  return ChannelFocus(
      title: title,
      label: label,
      valueListenable: store,
      text: () => current() ? text() : 'Reopen control',
      entryText: () => current() ? store.paramNum(leaf)?.toString() ?? '' : '',
      min: 0,
      max: 1,
      resolveRange: range,
      apply: (v) {
        final bounds = range();
        if (bounds == null || !v.isFinite) return;
        update(v.clamp(bounds.first, bounds.last));
      });
}

ChannelFocus eqFreqFocus(ChannelStore store, int band) {
  final b = band.clamp(0, store.eq.bands.length);
  return _parameterFocus(
      store,
      b == 0 ? 'eq_hpf_f' : 'eq${b}_f',
      b == 0 ? 'EQ HPF' : 'EQ Band $b',
      'Frequency',
      () => _eqFreqText(store, b),
      (v) => store.setEqParam(b, 'f', v));
}

ChannelFocus _eqGainFocus(ChannelStore store, int band) =>
    _parameterFocus(store, 'eq${band}_g', 'EQ Band $band', 'Gain', () {
      final value = store.eq.bands.where((b) => b.band == band).firstOrNull?.g;
      return value == null ? 'Loading…' : '${value.toStringAsFixed(1)} dB';
    }, (v) => store.setEqParam(band, 'g', v));

ChannelFocus _eqQFocus(ChannelStore store, int band) => _parameterFocus(
    store,
    'eq${band}_q',
    'EQ Band $band',
    'Q',
    () =>
        store.eq.bands
            .where((b) => b.band == band)
            .firstOrNull
            ?.q
            .toStringAsFixed(2) ??
        'Loading…',
    (v) => store.setEqParam(band, 'q', v));

String _dynLiveText(ChannelStore store, DynKind kind, String prop) {
  final value = store.paramNum('${kind.name}_$prop');
  final label =
      parameterValueLabel(store.paramMetadata('${kind.name}_$prop'), value);
  if (label != null) return label;
  for (final choice
      in parameterChoices(store.paramMetadata('${kind.name}_$prop'))) {
    if (value != null && (choice.value - value).abs() < .0001) {
      return choice.label;
    }
  }
  final d = kind == DynKind.gate ? store.gate : store.comp;
  switch (prop) {
    case 'thr':
      return '${d.thr.toStringAsFixed(1)} dB';
    case 'range':
      return '${d.range.toStringAsFixed(0)} dB';
    case 'att':
      return '${d.att.toStringAsFixed(1)} ms';
    case 'hold':
      return '${d.hold.toStringAsFixed(0)} ms';
    case 'rel':
      return '${d.rel.toStringAsFixed(0)} ms';
    case 'ratio':
      return '${d.ratio.toStringAsFixed(1)}:1';
    case 'knee':
      return '${d.knee.toStringAsFixed(1)} dB';
    case 'makeup':
      return '${d.makeup.toStringAsFixed(1)} dB';
    default:
      return '';
  }
}

class _ProcessorHeader extends StatelessWidget {
  final String title;
  final bool on;
  final VoidCallback onToggle;
  final bool compact;

  const _ProcessorHeader(
      {required this.title,
      required this.on,
      required this.onToggle,
      required this.compact});

  @override
  Widget build(BuildContext context) {
    final scale = (MediaQuery.sizeOf(context).width / 1823).clamp(.6, 1.2);
    final button = Semantics(
      button: true,
      toggled: on,
      label:
          '$title ${on ? 'on' : 'bypassed'}, activate to ${on ? 'bypass' : 'enable'}',
      onTap: onToggle,
      child: GestureDetector(
        onTap: onToggle,
        behavior: HitTestBehavior.opaque,
        child: Container(
          height: compact ? 44 : (56 * scale).clamp(44, 68),
          padding: EdgeInsets.symmetric(horizontal: compact ? 12 : 16),
          decoration: BoxDecoration(
            color: on
                ? ConsolePalette.of(context).processingSelected
                : ConsolePalette.of(context).control,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(
                color: on
                    ? ConsolePalette.of(context).cyan.withValues(alpha: 0.27)
                    : ConsolePalette.of(context).lineCtl),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: compact ? 18 : 28 * scale,
                      fontWeight: FontWeight.w600,
                      color: on
                          ? ConsolePalette.of(context).cyan
                          : ConsolePalette.of(context).mutedText)),
              Icon(Icons.power_settings_new,
                  size: compact ? 22 : 24,
                  color: on
                      ? ConsolePalette.of(context).cyan
                      : ConsolePalette.of(context).mutedText),
            ],
          ),
        ),
      ),
    );
    if (compact) return button;
    return Align(
        alignment: Alignment.centerLeft,
        child: SizedBox(width: 260 * scale, child: button));
  }
}

/// Local user library for the explicitly selected processor.
class _PresetsButton extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  final ProcessorPresetKind kind;
  const _PresetsButton(
      {required this.store, required this.compact, required this.kind});

  @override
  Widget build(BuildContext context) {
    return TextButton(
      onPressed: () {
        final target = store.capturePresetTarget(kind);
        showProcessorPresetLibrary(context, store, kind, target);
      },
      style: TextButton.styleFrom(
          foregroundColor: ConsolePalette.of(context).text1,
          backgroundColor: ConsolePalette.of(context).control,
          minimumSize: const Size(64, 44),
          padding: EdgeInsets.symmetric(horizontal: compact ? 12 : 18),
          shape:
              RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
          textStyle: TextStyle(
              fontFamily: U4.font,
              fontSize: compact ? 14 : 22,
              fontWeight: FontWeight.w500)),
      child: const Row(children: [
        Expanded(child: Text('Presets')),
        Icon(Icons.arrow_drop_down, size: 20)
      ]),
    );
  }
}

class _KnobDef {
  final String label;
  final String prop;
  final double value;
  final double min;
  final double max;
  final String text;
  const _KnobDef(
      this.label, this.prop, this.value, this.min, this.max, this.text);
}

List<_KnobDef> _dynKnobs(DynKind kind, ChannelDynVm d) {
  _KnobDef knob(String label, String prop, double value, String text) {
    final range = d.ranges[prop];
    return _KnobDef(
        label, prop, value, range?.first ?? 0, range?.last ?? 1, text);
  }

  if (kind == DynKind.gate) {
    return [
      knob('Threshold', 'thr', d.thr, '${d.thr.toStringAsFixed(1)} dB'),
      knob('Range', 'range', d.range, '${d.range.toStringAsFixed(0)} dB'),
      knob('Attack', 'att', d.att, '${d.att.toStringAsFixed(1)} ms'),
      knob('Hold', 'hold', d.hold, '${d.hold.toStringAsFixed(0)} ms'),
      knob('Release', 'rel', d.rel, '${d.rel.toStringAsFixed(0)} ms'),
    ];
  }
  return [
    knob('Threshold', 'thr', d.thr, '${d.thr.toStringAsFixed(1)} dB'),
    knob('Ratio', 'ratio', d.ratio, '${d.ratio.toStringAsFixed(1)}:1'),
    knob('Attack', 'att', d.att, '${d.att.toStringAsFixed(1)} ms'),
    // Match Gate's Hold slot, leaving Release in the same final position.
    knob('Gain', 'makeup', d.makeup, '${d.makeup.toStringAsFixed(1)} dB'),
    knob('Release', 'rel', d.rel, '${d.rel.toStringAsFixed(0)} ms'),
  ];
}
