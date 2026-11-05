part of 'channel.dart';

class _DynTab extends StatelessWidget {
  final ChannelStore store;
  final DynKind kind;
  final ValueChanged<ChannelFocus> onFocus;
  const _DynTab(
      {required this.store, required this.kind, required this.onFocus});
  @override
  Widget build(BuildContext context) {
    final d = kind == DynKind.gate ? store.gate : store.comp;
    final compact =
        MediaQuery.sizeOf(context).width < AppDesign.phoneBreakpoint;
    final title = kind == DynKind.gate ? 'Gate' : 'Compressor';
    final presetKind = kind == DynKind.gate
        ? ProcessorPresetKind.gate
        : ProcessorPresetKind.compressor;
    final knobs = _dynKnobs(kind, d);
    final knobMetadata = <String, Map<String, Object?>?>{
      for (final knob in knobs)
        knob.prop: store.paramMetadata('${kind.name}_${knob.prop}'),
    };
    final visibleKnobs = [
      for (final knob in knobs)
        if (store.supports('${kind.name}_${knob.prop}') &&
            knobMetadata[knob.prop]?['hidden'] != true)
          knob,
    ];
    final visibleKnobProps = <String>{
      for (final knob in visibleKnobs) knob.prop,
    };
    final knobByProp = <String, _KnobDef>{
      for (final knob in visibleKnobs) knob.prop: knob,
    };
    void focus(_KnobDef k) => onFocus(_parameterFocus(
        store,
        '${kind.name}_${k.prop}',
        title,
        k.label,
        () => _dynLiveText(store, kind, k.prop),
        (v) => store.setDynParam(kind, k.prop, v)));
    void apply(_KnobDef k, double v) {
      store.setDynParam(kind, k.prop, v);
      focus(k);
    }

    String controlText(_KnobDef k) =>
        parameterValueLabel(
            store.paramMetadata('${kind.name}_${k.prop}'), k.value) ??
        k.text;
    Widget choiceControl(_KnobDef k) => _ParameterChoices(
        store: store,
        leaf: '${kind.name}_${k.prop}',
        label: k.label,
        fallback: controlText(k),
        compact: compact,
        onChanged: (v) => apply(k, v));
    Widget control(_KnobDef k, {bool threshold = false}) {
      return parameterChoices(knobMetadata[k.prop]).isNotEmpty
          ? choiceControl(k)
          : Container(
              padding: EdgeInsets.symmetric(
                  horizontal: compact ? 12 : 16, vertical: compact ? 8 : 16),
              decoration: BoxDecoration(
                  color: ConsolePalette.of(context).processingPanel,
                  borderRadius: U4.rWell),
              child: _ProcessingSlider(
                  label: k.label,
                  text: controlText(k),
                  value: k.value,
                  min: k.min,
                  max: k.max,
                  compact: true,
                  fontSize: compact
                      ? 15
                      : (MediaQuery.sizeOf(context).width / 2048 * 22)
                          .clamp(14.0, 22.0),
                  color: kind == DynKind.gate
                      ? ConsolePalette.of(context).processingBlue
                      : ConsolePalette.of(context).processingPurple,
                  solidFill: true,
                  valueColor: threshold
                      ? ConsolePalette.of(context).cyan
                      : ConsolePalette.of(context).text1,
                  enabled: d.ranges[k.prop] != null,
                  onFocus: () => focus(k),
                  onChanged: (v) => apply(k, v)));
    }

    return LayoutBuilder(builder: (context, bounds) {
      final scale = bounds.maxWidth / 1292;
      final optionFontSize = compact ? 14.0 : 22.0;
      final textScaler = MediaQuery.textScalerOf(context);
      // The knee selector has inner padding. Both processors reserve its full
      // height so neither Presets nor the graph shifts when it is present.
      final optionsHeight = math.max(
              AppDesign.controlHeight,
              textScaler.scale(optionFontSize) * 1.15 +
                  AppDesign.spaceLg +
                  AppDesign.spaceXs) +
          2 * AppDesign.spaceXs;
      final presetsWidth = compact
          ? math.min(bounds.maxWidth / 2,
              96 * textScaler.scale(optionFontSize) / optionFontSize)
          : 420 * scale;
      return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _ProcessorHeader(
            title: title,
            on: d.on,
            compact: compact,
            onToggle: () => store.setDynOn(kind, !d.on)),
        const SizedBox(height: AppDesign.spaceSm),
        SizedBox(
          height: optionsHeight,
          child: Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
            SizedBox(
                width: presetsWidth,
                child: _PresetsButton(
                    store: store, compact: compact, kind: presetKind)),
            const SizedBox(width: AppDesign.spaceSm),
            Expanded(
              child: kind == DynKind.comp &&
                      store.paramMetadata('comp_knee')?['hidden'] != true
                  ? parameterChoices(store.paramMetadata('comp_knee'))
                          .isNotEmpty
                      ? _ParameterChoices(
                          store: store,
                          leaf: 'comp_knee',
                          label: 'Knee',
                          fallback: 'Loading…',
                          compact: compact,
                          onChanged: (value) =>
                              store.setDynParam(DynKind.comp, 'knee', value))
                      : Container(
                          padding: const EdgeInsets.all(AppDesign.spaceXs),
                          decoration: BoxDecoration(
                              color: ConsolePalette.of(context).chip,
                              borderRadius: U4.rBtn),
                          child: Row(children: [
                            for (final soft in [true, false])
                              Expanded(
                                  child: Semantics(
                                      toggled: d.softKnee == soft,
                                      child: _ProcessingButton(
                                          fontSize: optionFontSize,
                                          label:
                                              soft ? 'Soft Knee' : 'Hard Knee',
                                          active: d.softKnee == soft,
                                          onTap: store.supports('comp_knee') &&
                                                  d.softKnee != null
                                              ? () =>
                                                  store.setCompSoftKnee(soft)
                                              : null)))
                          ]))
                  : const SizedBox.shrink(),
            ),
          ]),
        ),
        const SizedBox(height: AppDesign.spaceSm),
        Opacity(
            opacity: d.on || ConsolePalette.of(context).isLight ? 1 : .55,
            child: AnimatedBuilder(
                animation:
                    Listenable.merge([store.levelDb, store.dynamicsMeters]),
                builder: (context, _) {
                  final meters = store.dynamicsMeters.value;
                  final gate = kind == DynKind.gate;
                  return _DynamicsGraph(
                      dyn: d,
                      measured: store.hasDynamicsMeters,
                      level: store.hasDynamicsMeters
                          ? (gate ? meters?.gateIn : meters?.compIn)
                          : store.levelDb.value,
                      output: gate ? meters?.gateOut : meters?.compOut,
                      reduction: store.hasDynamicsMeters
                          ? (meters == null
                              ? null
                              : -(gate
                                  ? meters.gateReduction
                                  : meters.compReduction))
                          : store.grDb(kind),
                      height: compact ? 282 : 700 * scale,
                      compact: compact,
                      enabled: (prop) =>
                          d.ranges[prop] != null &&
                          visibleKnobProps.contains(prop),
                      onFocus: (prop) {
                        final knob = knobByProp[prop];
                        if (knob != null) focus(knob);
                      },
                      onChanged: (prop, v) {
                        final knob = knobByProp[prop];
                        if (knob != null) apply(knob, v);
                      });
                })),
        const SizedBox(height: AppDesign.spaceSm),
        if (compact && visibleKnobs.isNotEmpty) ...[
          if (knobByProp['thr'] case final threshold?) ...[
            SizedBox(height: 80, child: control(threshold, threshold: true)),
            if (visibleKnobs.length > 1) const SizedBox(height: 8),
          ],
          if (visibleKnobs.any((knob) => knob.prop != 'thr'))
            LayoutBuilder(builder: (context, controlsBounds) {
              const gap = 8.0;
              final width = (controlsBounds.maxWidth - gap) / 2;
              return Wrap(spacing: gap, runSpacing: gap, children: [
                for (final knob in visibleKnobs)
                  if (knob.prop != 'thr')
                    SizedBox(width: width, height: 104, child: control(knob)),
              ]);
            }),
        ] else if (!compact && visibleKnobs.isNotEmpty) ...[
          Container(
              height: (200 * scale).clamp(128.0, 200.0),
              decoration: BoxDecoration(
                  color: ConsolePalette.of(context).processingPanel,
                  borderRadius: U4.rWell),
              padding: const EdgeInsets.symmetric(vertical: 16),
              child: Row(children: [
                for (var i = 0; i < visibleKnobs.length; i++) ...[
                  if (i > 0)
                    Container(
                        width: 1,
                        height: 100 * scale,
                        color: ConsolePalette.of(context).lineCtl),
                  Expanded(
                      child: control(visibleKnobs[i],
                          threshold: visibleKnobs[i].prop == 'thr')),
                ]
              ])),
        ],
        _AdvancedDynamics(
            store: store, kind: kind, compact: compact, onFocus: onFocus),
      ]);
    });
  }
}

/// Controls are offered only when the active provider describes their values.
class _AdvancedDynamics extends StatelessWidget {
  final ChannelStore store;
  final DynKind kind;
  final bool compact;
  final ValueChanged<ChannelFocus> onFocus;
  const _AdvancedDynamics(
      {required this.store,
      required this.kind,
      required this.compact,
      required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final prefix = kind.name;
    final palette = ConsolePalette.of(context);
    final enums = <(String, String)>[
      ('mode', 'Mode'),
      if (kind == DynKind.comp) ...[
        ('det', 'Detector'),
        ('env', 'Envelope'),
      ],
      ('key_src', 'Key source'),
      ('filter_type', 'Key filter type'),
    ];
    final numbers = <(String, String, String)>[
      if (kind == DynKind.comp) ...[
        ('hold', 'Hold', 'ms'),
        ('mix', 'Mix', '%'),
      ],
      ('filter_freq', 'Key filter frequency', 'Hz'),
    ];
    final toggles = <(String, String)>[
      if (kind == DynKind.comp) ('auto', 'Auto'),
      ('filter_on', 'Key filter'),
    ];
    final controls = <Widget>[];
    for (final (prop, label) in enums) {
      final leaf = '${prefix}_$prop';
      if (store.supports(leaf) &&
          parameterChoices(store.paramMetadata(leaf)).isNotEmpty) {
        controls.add(_ParameterChoices(
            store: store,
            leaf: leaf,
            label: label,
            fallback: '—',
            compact: compact,
            onChanged: (value) => store.cmdParam(leaf, value)));
      }
    }
    for (final (prop, label) in toggles) {
      final leaf = '${prefix}_$prop';
      if (!store.supports(leaf)) continue;
      final value = store.paramBool(leaf);
      controls.add(Container(
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
              color: palette.processingPanel, borderRadius: U4.rWell),
          child: Row(children: [
            Expanded(
                child: Text(label,
                    style: AppDesign.type(14, color: palette.text1))),
            Switch(
                value: value ?? false,
                onChanged:
                    value == null ? null : (v) => store.cmdParam(leaf, v)),
          ])));
    }
    for (final (prop, label, unit) in numbers) {
      final leaf = '${prefix}_$prop';
      final range = _controlRange(store, leaf);
      if (range == null) continue;
      final value = store.paramNum(leaf)!;
      String readout() {
        final live = store.paramNum(leaf);
        return live == null
            ? 'Loading…'
            : '${live.toStringAsFixed(live < 10 ? 1 : 0)} $unit';
      }

      void focus() => onFocus(_parameterFocus(
          store,
          leaf,
          kind == DynKind.gate ? 'Gate' : 'Compressor',
          label,
          readout,
          (v) => store.cmdParam(leaf, v)));
      controls.add(Container(
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
              color: palette.processingPanel, borderRadius: U4.rWell),
          child: _ProcessingSlider(
              label: label,
              text: '${value.toStringAsFixed(value < 10 ? 1 : 0)} $unit',
              value: value,
              min: range.first,
              max: range.last,
              compact: true,
              color: kind == DynKind.gate
                  ? palette.processingBlue
                  : palette.processingPurple,
              onFocus: focus,
              onChanged: (v) {
                store.cmdParam(leaf, v);
                focus();
              })));
    }
    if (controls.isEmpty) return const SizedBox.shrink();
    return Padding(
        padding: const EdgeInsets.only(top: 12),
        child: ExpansionTile(
            key: PageStorageKey('advanced-dynamics-${store.stripKey}'),
            title: Text('Advanced',
                style: AppDesign.type(14, color: palette.text7)),
            tilePadding: const EdgeInsets.symmetric(horizontal: 12),
            childrenPadding: const EdgeInsets.all(8),
            children: [
              LayoutBuilder(builder: (context, bounds) {
                final columns = compact
                    ? 1
                    : bounds.maxWidth >= 900
                        ? 3
                        : 2;
                const gap = 8.0;
                final width = (bounds.maxWidth - gap * (columns - 1)) / columns;
                return Wrap(spacing: gap, runSpacing: gap, children: [
                  for (final control in controls)
                    SizedBox(width: width, child: control),
                ]);
              }),
            ]));
  }
}

String _choiceText(ChannelStore store, String leaf, String fallback) {
  final value = store.paramNum(leaf);
  for (final choice in parameterChoices(store.paramMetadata(leaf))) {
    if (value != null && (value - choice.value).abs() < .0001) {
      return choice.label;
    }
  }
  return fallback;
}

/// Shared choice control: labels and values come entirely from the provider.
class _ParameterChoices extends StatelessWidget {
  final ChannelStore store;
  final String leaf, label, fallback;
  final bool compact;
  final ValueChanged<double> onChanged;
  const _ParameterChoices(
      {required this.store,
      required this.leaf,
      required this.label,
      required this.fallback,
      required this.compact,
      required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final choices = parameterChoices(store.paramMetadata(leaf));
    final enabled = store.supports(leaf);
    final stripKey = store.stripKey;
    final epoch = store.connectionEpoch;
    return Semantics(
        label: label,
        value: _choiceText(store, leaf, fallback),
        button: true,
        enabled: enabled,
        child: PopupMenuButton<double>(
          popUpAnimationStyle: AnimationStyle.noAnimation,
          enabled: enabled,
          tooltip: label,
          color: palette.processingPanel,
          onSelected: (value) {
            // A menu may outlive a model change or a channel switch.
            if (store.stripKey == stripKey &&
                store.connectionEpoch == epoch &&
                store.supports(leaf) &&
                parameterChoices(store.paramMetadata(leaf))
                    .any((c) => (c.value - value).abs() < .0001)) {
              onChanged(value);
            }
          },
          itemBuilder: (context) => [
            for (final choice in choices)
              PopupMenuItem(
                  value: choice.value,
                  child: Text(choice.label,
                      style: AppDesign.type(16, color: palette.text1))),
          ],
          child: Container(
              constraints: BoxConstraints(minHeight: !compact ? 64 : 44),
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                  color: palette.processingPanel, borderRadius: U4.rWell),
              child: Row(children: [
                Expanded(
                    child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                      Text(label,
                          style: AppDesign.type(compact ? 12 : 14,
                              color: palette.text7)),
                      Text(_choiceText(store, leaf, fallback),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AppDesign.type(compact ? 15 : 18,
                              color: palette.text1)),
                    ])),
                Icon(Icons.arrow_drop_down, color: palette.text7),
              ])),
        ));
  }
}

class _DynamicsGeometry {
  final Size size;
  final bool compact;
  const _DynamicsGeometry(this.size, this.compact);
  double get left => size.width * (compact ? 64 / 752 : 70 / 1280);
  double get top => size.height * (compact ? 60 / 570 : 50 / 620);
  double get width => size.width * (compact ? 408 / 752 : 900 / 1280);
  double get height => size.height * (compact ? 408 / 570 : 440 / 620);
  double x(double db) => left + (db + 80) / 80 * width;
  double y(double db) => top - db.clamp(-80, 0) / 80 * height;
  double get handleRadius =>
      compact ? 21.0 : (size.width * 48 / 1280).clamp(22.0, 48.0);
  Offset anchor(ChannelDynVm d, String prop) => switch (prop) {
        'thr' =>
          Offset(x(d.thr), y(d.thr + (d.kind == DynKind.comp ? d.makeup : 0))),
        'ratio' => Offset(x(0), y(compOut(d, 0))),
        'range' => Offset(x(d.thr), y(d.thr - d.range)),
        _ => Offset(x(-80), y(-80 + d.makeup)),
      };
  // Keep nearby controls independently readable and touchable. Drags use deltas,
  // so moving the handle away from its curve point does not change its value.
  Offset node(ChannelDynVm d, String prop) {
    final point = anchor(d, prop);
    if (d.kind != DynKind.comp || prop != 'thr') return point;
    final spacing = math.max(52.0, handleRadius * 2 + 8);
    var x = point.dx;
    for (final other in ['ratio', 'makeup']) {
      final fixed = anchor(d, other);
      final dy = point.dy - fixed.dy;
      if (dy.abs() >= spacing) continue;
      final dx = math.sqrt(spacing * spacing - dy * dy);
      if ((x - fixed.dx).abs() < dx) {
        x = fixed.dx + (other == 'ratio' ? -dx : dx);
      }
    }
    return Offset(x, point.dy);
  }

  List<String> handles(ChannelDynVm d) => [
        if (d.kind == DynKind.gate &&
            d.responseCurve == 'gate_attenuation' &&
            d.ranges['range'] != null &&
            d.thr - d.range >= -80 &&
            d.range / 80 * height >= 44)
          'range',
        'thr',
        if (d.kind == DynKind.comp) ...['makeup', 'ratio'],
      ];
  double meterX(int i) =>
      size.width *
      (compact
          ? [581 / 752, 629 / 752, 694 / 752][i]
          : [1090 / 1280, 1150 / 1280, 1220 / 1280][i]);
}

class _DynamicsGraph extends StatefulWidget {
  final ChannelDynVm dyn;
  final double height;
  final double? level, output, reduction;
  final bool measured;
  final bool compact;
  final bool Function(String) enabled;
  final ValueChanged<String> onFocus;
  final void Function(String, double) onChanged;
  const _DynamicsGraph(
      {required this.dyn,
      required this.level,
      this.output,
      this.measured = false,
      required this.reduction,
      required this.height,
      required this.compact,
      required this.enabled,
      required this.onFocus,
      required this.onChanged});
  @override
  State<_DynamicsGraph> createState() => _DynamicsGraphState();
}

class _DynamicsGraphState extends State<_DynamicsGraph> {
  double? _drag;
  int? _pointer;
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final size = Size(box.maxWidth, widget.height);
        final geom = _DynamicsGeometry(size, widget.compact);
        return Container(
            height: widget.height,
            decoration: BoxDecoration(
                color: ConsolePalette.of(context).panel,
                borderRadius: U4.rWell,
                border:
                    Border.all(color: ConsolePalette.of(context).linePanel)),
            child: Stack(clipBehavior: Clip.none, children: [
              Positioned.fill(
                  child: CustomPaint(
                      painter: _DynamicsPainter(
                          widget.dyn,
                          widget.level,
                          widget.reduction,
                          widget.output,
                          widget.measured,
                          widget.compact,
                          ConsolePalette.of(context)))),
              for (final prop in geom.handles(widget.dyn))
                Positioned(
                    left: (geom.node(widget.dyn, prop).dx - 22)
                        .clamp(0, size.width - 44),
                    top: (geom.node(widget.dyn, prop).dy - 22)
                        .clamp(0, size.height - 44),
                    width: 44,
                    height: 44,
                    child: Semantics(
                        slider: true,
                        label: switch (prop) {
                          'thr' => 'Threshold',
                          'ratio' => 'Ratio',
                          'range' => 'Range',
                          _ => 'Gain'
                        },
                        value: switch (prop) {
                          'thr' => '${widget.dyn.thr} dB',
                          'ratio' => widget.dyn.ratioIsLimit
                              ? '∞:1'
                              : '${widget.dyn.ratio}:1',
                          'range' => '${widget.dyn.range} dB',
                          _ => '${widget.dyn.makeup} dB'
                        },
                        enabled: widget.enabled(prop),
                        child: !widget.enabled(prop)
                            ? const SizedBox.expand()
                            : U4GraphTouchSurface(
                                onDown: (event) {
                                  if (_pointer != null) {
                                    _pointer = null;
                                    _drag = null;
                                    return;
                                  }
                                  _pointer = event.pointer;
                                  widget.onFocus(prop);
                                  _drag = switch (prop) {
                                    'thr' => widget.dyn.thr,
                                    'ratio' => widget.dyn.ratio,
                                    'range' => widget.dyn.range,
                                    _ => widget.dyn.makeup,
                                  };
                                },
                                onMove: (event) {
                                  if (_pointer != event.pointer ||
                                      _drag == null) {
                                    return;
                                  }
                                  final range = widget.dyn.ranges[prop];
                                  if (range == null) return;
                                  final delta = prop == 'thr'
                                      ? event.delta.dx / geom.width * 80
                                      : prop == 'range'
                                          ? event.delta.dy / geom.height * 80
                                          : -event.delta.dy /
                                              geom.height *
                                              (prop == 'ratio'
                                                  ? -(range.last - range.first)
                                                  : 80);
                                  _drag = (_drag! + delta)
                                      .clamp(range.first, range.last);
                                  widget.onChanged(prop, _drag!);
                                },
                                onUp: (event) {
                                  if (_pointer == event.pointer) {
                                    _pointer = null;
                                    _drag = null;
                                  }
                                },
                                onCancel: (_) {
                                  _pointer = null;
                                  _drag = null;
                                },
                                child: const SizedBox.expand()))),
            ]));
      });
}

class _DynamicsPainter extends CustomPainter {
  final ConsolePalette palette;
  final ChannelDynVm dyn;
  final double? level, reduction, output;
  final bool measured, compact;
  const _DynamicsPainter(this.dyn, this.level, this.reduction, this.output,
      this.measured, this.compact, this.palette);
  @override
  void paint(Canvas canvas, Size size) {
    final g = _DynamicsGeometry(size, compact);
    void text(String value, Offset at, {Color? color, double? font}) {
      final t = TextPainter(
          text: TextSpan(
              text: value,
              style: AppDesign.type(font ?? (compact ? 10 : 14),
                  color: color ?? palette.text7, weight: FontWeight.w500)),
          textDirection: TextDirection.ltr)
        ..layout();
      t.paint(canvas, at - Offset(t.width / 2, t.height / 2));
    }

    for (var db = -80.0; db <= 0; db += 10) {
      canvas.drawLine(Offset(g.x(db), g.top), Offset(g.x(db), g.top + g.height),
          Paint()..color = palette.linePanel);
      canvas.drawLine(
          Offset(g.left, g.y(db)),
          Offset(g.left + g.width, g.y(db)),
          Paint()..color = palette.linePanel);
      if (!compact || db % 20 == 0) {
        text('${db.toInt()}${db == 0 ? ' dB' : ''}',
            Offset(g.x(db), size.height * .94));
      }
      text(
          '${db.toInt()}', Offset(size.width * (compact ? .71 : .80), g.y(db)));
    }
    canvas.drawLine(
        Offset(g.x(-80), g.y(-80)),
        Offset(g.x(0), g.y(0)),
        Paint()
          ..color = palette.panShape
          ..strokeWidth = 1.5);
    final curve = Path();
    if (dyn.kind == DynKind.gate && dyn.responseCurve == 'gate_attenuation') {
      final points = dynCurve(dyn);
      curve.moveTo(g.x(points.first.$1), g.y(points.first.$2));
      for (final point in points.skip(1)) {
        curve.lineTo(g.x(point.$1), g.y(point.$2));
      }
    } else if (dyn.kind == DynKind.gate) {
      curve.moveTo(g.x(dyn.thr), g.y((dyn.thr - dyn.range).clamp(-80, 0)));
      curve.lineTo(g.x(dyn.thr), g.y(dyn.thr));
      curve.lineTo(g.x(0), g.y(0));
    } else {
      for (var i = 0; i <= 160; i++) {
        final x = -80 + i / 2;
        if (i == 0) {
          curve.moveTo(g.x(x), g.y(compOut(dyn, x)));
        } else {
          curve.lineTo(g.x(x), g.y(compOut(dyn, x)));
        }
      }
    }
    final fill = Path.from(curve)
      ..lineTo(g.x(0), g.y(-80))
      ..lineTo(
          dyn.kind == DynKind.gate && dyn.responseCurve != 'gate_attenuation'
              ? g.x(dyn.thr)
              : g.x(-80),
          g.y(-80))
      ..close();
    canvas.drawPath(fill, Paint()..color = palette.dynamicsFill);
    canvas.drawPath(
        curve,
        Paint()
          ..color = palette.cyan
          ..strokeWidth = 2
          ..style = PaintingStyle.stroke);
    for (final prop in g.handles(dyn)) {
      final p = g.node(dyn, prop);
      final r = g.handleRadius;
      final anchor = g.anchor(dyn, prop);
      if ((anchor - p).distance > 1) {
        canvas.drawLine(
            anchor,
            p,
            Paint()
              ..color = palette.scaleTick
              ..strokeWidth = 1);
      }
      canvas.drawCircle(p, r, Paint()..color = palette.chip);
      canvas.drawCircle(
          p,
          r,
          Paint()
            ..color = palette.cyan
            ..strokeWidth = 2
            ..style = PaintingStyle.stroke);
      text(switch (prop) { 'thr' => 'T', 'ratio' || 'range' => 'R', _ => 'G' },
          p,
          color: palette.text1, font: compact ? 17 : r * .8);
    }
    // No synthetic detector: missing GR also leaves derived OUT unavailable.
    final out = measured
        ? output
        : (reduction == null || level == null
            ? null
            : level! +
                reduction! +
                (dyn.kind == DynKind.comp && dyn.on ? dyn.makeup : 0));
    final values = [level, reduction, out];
    for (var i = 0; i < 3; i++) {
      final x = g.meterX(i);
      final w = size.width * (compact ? 26 / 752 : 24 / 1280);
      final rect = Rect.fromLTWH(x, g.top, w, g.height);
      canvas.drawRRect(
          RRect.fromRectAndRadius(rect, const Radius.circular(2)),
          Paint()
            ..color = i == 1 ? palette.meterClipBack : palette.meterGreenBack);
      final value = values[i];
      if (value != null && value.isFinite) {
        final n = i == 1
            ? (-value / 20).clamp(0.0, 1.0)
            : ((value + 80) / 80).clamp(0.0, 1.0);
        canvas.drawRect(
            i == 1
                ? Rect.fromLTWH(x, g.top, w, g.height * n)
                : Rect.fromLTWH(x, g.top + g.height * (1 - n), w, g.height * n),
            Paint()..color = i == 1 ? palette.meterReduction : palette.meter);
      }
      text(['IN', 'GR', 'OUT'][i], Offset(x + w / 2, size.height * .89));
      text(
          value == null || !value.isFinite
              ? '—'
              : value.toStringAsFixed(compact ? 0 : 1),
          Offset(x + w / 2, size.height * .95),
          color: palette.text1);
    }
  }

  @override
  bool shouldRepaint(_DynamicsPainter old) =>
      old.palette != palette ||
      old.dyn != dyn ||
      old.level != level ||
      old.output != output ||
      old.measured != measured ||
      old.reduction != reduction ||
      old.compact != compact;
}

class _EqPreviewPainter extends CustomPainter {
  final ConsolePalette palette;
  final ChannelEqVm eq;
  const _EqPreviewPainter(this.eq, this.palette);
  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
        RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(8)),
        Paint()..color = palette.eqBg);
    final zeroY = size.height / 2;
    canvas.drawLine(Offset(0, zeroY), Offset(size.width, zeroY),
        Paint()..color = palette.text7.withValues(alpha: .35));
    final response = eqResponseDb(eq);
    if (response.isEmpty) return;
    final curve = Path();
    for (var i = 0; i < response.length; i++) {
      final p = Offset(i / (response.length - 1) * size.width,
          size.height * (.5 - response[i].clamp(-15, 15) / 30));
      if (i == 0) {
        curve.moveTo(p.dx, p.dy);
      } else {
        curve.lineTo(p.dx, p.dy);
      }
    }
    final fill = Path.from(curve)
      ..lineTo(size.width, zeroY)
      ..lineTo(0, zeroY)
      ..close();
    canvas.drawPath(
        fill, Paint()..color = palette.eqPreviewFill.withValues(alpha: .85));
    canvas.drawPath(
        curve,
        Paint()
          ..color = palette.eqPreviewCurve
          ..strokeWidth = 1.6
          ..style = PaintingStyle.stroke);
  }

  @override
  bool shouldRepaint(_EqPreviewPainter old) =>
      old.palette != palette || old.eq != eq;
}
