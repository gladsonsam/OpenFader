part of 'channel.dart';

class _DynTab extends StatelessWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  final DynKind kind;
  final ValueChanged<ChannelFocus> onFocus;
  const _DynTab(
      {required this.store,
      required this.writes,
      required this.kind,
      required this.onFocus});
  @override
  Widget build(BuildContext context) {
    final d = kind == DynKind.gate ? store.gate : store.comp;
    final compact = MediaQuery.sizeOf(context).width < 700;
    final title = kind == DynKind.gate ? 'Gate' : 'Compressor';
    final knobs = _dynKnobs(kind, d).where((k) => k.prop != 'knee').toList();
    void apply(_KnobDef k, double v) {
      writes.queue('${kind.name}_${k.prop}', v);
      store.setDynParam(kind, k.prop, v);
    }

    void focus(_KnobDef k) => onFocus(ChannelFocus(
        title: title,
        label: k.label,
        text: () => _dynLiveText(store, kind, k.prop),
        min: k.min,
        max: k.max,
        apply: (v) => apply(k, v)));
    Widget control(_KnobDef k, {bool threshold = false}) => Container(
        padding: EdgeInsets.symmetric(
            horizontal: compact ? 12 : 16, vertical: compact ? 8 : 16),
        decoration: BoxDecoration(color: U4.tileBg, borderRadius: U4.rWell),
        child: _ProcessingSlider(
            label: k.label,
            text: k.text,
            value: k.value,
            min: k.min,
            max: k.max,
            compact: true,
            fontSize: compact
                ? 15
                : (MediaQuery.sizeOf(context).width / 2048 * 22)
                    .clamp(14.0, 22.0),
            color: kind == DynKind.gate
                ? AppDesign.processingBlue
                : AppDesign.processingPurple,
            solidFill: true,
            valueColor: threshold ? U4.cyan : U4.text1,
            enabled: store.supports('${kind.name}_${k.prop}'),
            onFocus: () => focus(k),
            onChanged: (v) => apply(k, v)));
    Widget secondary(_KnobDef k) => Container(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
        decoration: BoxDecoration(color: U4.tileBg, borderRadius: U4.rWell),
        child: Row(children: [
          Text(k.label, style: AppDesign.type(16, color: U4.text7)),
          const SizedBox(width: 16),
          Expanded(
              child: _ProcessingSlider(
                  label: k.label,
                  text: k.text,
                  value: k.value,
                  min: k.min,
                  max: k.max,
                  showLabel: false,
                  color: kind == DynKind.gate
                      ? AppDesign.processingBlue
                      : AppDesign.processingPurple,
                  solidFill: true,
                  enabled: store.supports('${kind.name}_${k.prop}'),
                  onFocus: () => focus(k),
                  onChanged: (v) => apply(k, v))),
          const SizedBox(width: 16),
          Text(k.text, style: AppDesign.type(16, color: U4.text1)),
        ]));
    final supplemental = Row(children: [
      Expanded(
          child: Container(
              padding: const EdgeInsets.all(12),
              decoration:
                  BoxDecoration(color: U4.tileBg, borderRadius: U4.rWell),
              child: _ProcessingSlider(
                  label: 'Temperature',
                  text: '—',
                  value: 0,
                  min: 0,
                  max: 100,
                  enabled: false,
                  compact: true,
                  color: AppDesign.processingPurple,
                  onChanged: (_) {}))),
      const SizedBox(width: 8),
      Expanded(
          child: Container(
              padding: const EdgeInsets.all(12),
              decoration:
                  BoxDecoration(color: U4.tileBg, borderRadius: U4.rWell),
              child: _ProcessingSlider(
                  label: 'Delay',
                  text: store.supports('delay_ms')
                      ? '${store.preamp.delayMs.toStringAsFixed(1)} ms'
                      : '—',
                  value: store.preamp.delayMs,
                  min: 0,
                  max: 100,
                  enabled: store.supports('delay_ms'),
                  compact: true,
                  color: AppDesign.processingPurple,
                  solidFill: true,
                  valueColor: U4.text1,
                  onFocus: () => onFocus(ChannelFocus(
                      title: 'Compressor',
                      label: 'Delay',
                      text: () =>
                          '${store.preamp.delayMs.toStringAsFixed(1)} ms',
                      min: 0,
                      max: 100,
                      apply: (v) => store.setPreamp('delay_ms', v))),
                  onChanged: (v) => store.setPreamp('delay_ms', v)))),
    ]);
    return LayoutBuilder(builder: (context, bounds) {
      final scale = bounds.maxWidth / 1292;
      return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        _ProcessorHeader(
            title: title,
            on: d.on,
            compact: compact,
            onToggle: () => store.setDynOn(kind, !d.on)),
        const SizedBox(height: 8),
        Row(children: [
          if (!compact)
            SizedBox(
                width: 420 * scale,
                child: _PresetsButton(store: store, compact: false))
          else if (kind == DynKind.comp && compact)
            SizedBox(
                width: 96, child: _PresetsButton(store: store, compact: true))
          else
            Expanded(child: _PresetsButton(store: store, compact: compact)),
          if (kind == DynKind.comp) ...[
            const SizedBox(width: 8),
            Expanded(
                child: Container(
                    padding: const EdgeInsets.all(4),
                    decoration: BoxDecoration(
                        color: AppDesign.chip, borderRadius: U4.rBtn),
                    child: Row(children: [
                      for (final soft in [true, false])
                        Expanded(
                            child: Semantics(
                                toggled: d.softKnee == soft,
                                child: _ProcessingButton(
                                    fontSize: compact ? 14 : 22,
                                    label: soft ? 'Soft Knee' : 'Hard Knee',
                                    active: d.softKnee == soft,
                                    onTap: store.supports('comp_knee') &&
                                            d.softKnee != null
                                        ? () => store.setCompSoftKnee(soft)
                                        : null)))
                    ]))),
          ] else if (!compact)
            const Spacer(),
        ]),
        const SizedBox(height: 8),
        Opacity(
            opacity: d.on ? 1 : .55,
            child: ValueListenableBuilder<double>(
                valueListenable: store.levelDb,
                builder: (context, db, _) => _DynamicsGraph(
                    dyn: d,
                    level: db,
                    reduction: store.grDb(kind),
                    height: compact ? 282 : 700 * scale,
                    compact: compact,
                    enabled: (prop) => store.supports('${kind.name}_$prop'),
                    onFocus: (prop) =>
                        focus(knobs.firstWhere((k) => k.prop == prop)),
                    onChanged: (prop, v) =>
                        apply(knobs.firstWhere((k) => k.prop == prop), v)))),
        SizedBox(height: compact ? 8 : 20 * scale),
        if (compact) ...[
          SizedBox(height: 80, child: control(knobs.first, threshold: true)),
          const SizedBox(height: 8),
          SizedBox(
              height: 216,
              child: Column(children: [
                for (var row = 0; row < 2; row++) ...[
                  if (row > 0) const SizedBox(height: 8),
                  Expanded(
                      child: Row(children: [
                    for (var col = 0; col < 2; col++) ...[
                      if (col > 0) const SizedBox(width: 8),
                      Expanded(child: control(knobs[1 + row * 2 + col])),
                    ]
                  ])),
                ]
              ])),
        ] else ...[
          Container(
              height: (200 * scale).clamp(128.0, 200.0),
              decoration:
                  BoxDecoration(color: U4.tileBg, borderRadius: U4.rWell),
              padding: const EdgeInsets.symmetric(vertical: 16),
              child: Row(children: [
                for (var i = 0;
                    i < (kind == DynKind.gate ? 3 : knobs.length);
                    i++) ...[
                  if (i > 0)
                    Container(width: 1, height: 100 * scale, color: U4.lineCtl),
                  Expanded(child: control(knobs[i], threshold: i == 0)),
                ]
              ])),
          if (kind == DynKind.gate) ...[
            SizedBox(height: 20 * scale),
            Row(children: [
              Expanded(child: secondary(knobs[3])),
              SizedBox(width: 20 * scale),
              Expanded(child: secondary(knobs[4])),
            ]),
          ],
        ],
        if (kind == DynKind.comp) ...[
          const SizedBox(height: 12),
          if (compact)
            Theme(
                data: Theme.of(context)
                    .copyWith(dividerColor: Colors.transparent),
                child: ExpansionTile(
                    title: Text('Temperature & Delay',
                        style: AppDesign.type(14, color: U4.text7)),
                    tilePadding: const EdgeInsets.symmetric(horizontal: 12),
                    childrenPadding: const EdgeInsets.all(8),
                    minTileHeight: 44,
                    children: [supplemental]))
          else
            supplemental,
        ],
      ]);
    });
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
  Offset node(ChannelDynVm d, String prop) => switch (prop) {
        'thr' =>
          Offset(x(d.thr), y(d.thr + (d.kind == DynKind.comp ? d.makeup : 0))),
        'ratio' => Offset(x(0), y(compOut(d, 0))),
        _ => Offset(x(-80), y(-80 + d.makeup)),
      };
  double meterX(int i) =>
      size.width *
      (compact
          ? [581 / 752, 629 / 752, 694 / 752][i]
          : [1090 / 1280, 1150 / 1280, 1220 / 1280][i]);
}

class _DynamicsGraph extends StatefulWidget {
  final ChannelDynVm dyn;
  final double level, height;
  final double? reduction;
  final bool compact;
  final bool Function(String) enabled;
  final ValueChanged<String> onFocus;
  final void Function(String, double) onChanged;
  const _DynamicsGraph(
      {required this.dyn,
      required this.level,
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
                color: AppDesign.panel,
                borderRadius: U4.rWell,
                border: Border.all(color: U4.linePanel)),
            child: Stack(clipBehavior: Clip.none, children: [
              Positioned.fill(
                  child: CustomPaint(
                      painter: _DynamicsPainter(widget.dyn, widget.level,
                          widget.reduction, widget.compact))),
              for (final prop in [
                'thr',
                if (widget.dyn.kind == DynKind.comp) ...['ratio', 'makeup']
              ])
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
                          _ => 'Gain'
                        },
                        value: switch (prop) {
                          'thr' => '${widget.dyn.thr} dB',
                          'ratio' => '${widget.dyn.ratio}:1',
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
                                    _ => widget.dyn.makeup,
                                  };
                                },
                                onMove: (event) {
                                  if (_pointer != event.pointer ||
                                      _drag == null) {
                                    return;
                                  }
                                  final delta = prop == 'thr'
                                      ? event.delta.dx / geom.width * 80
                                      : -event.delta.dy /
                                          geom.height *
                                          (prop == 'ratio' ? -19 : 80);
                                  final min = prop == 'thr'
                                      ? widget.dyn.lo
                                      : prop == 'ratio'
                                          ? 1.0
                                          : 0.0;
                                  final max = prop == 'thr'
                                      ? 0.0
                                      : prop == 'ratio'
                                          ? 20.0
                                          : 24.0;
                                  _drag = (_drag! + delta).clamp(min, max);
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
  final ChannelDynVm dyn;
  final double level;
  final double? reduction;
  final bool compact;
  const _DynamicsPainter(this.dyn, this.level, this.reduction, this.compact);
  @override
  void paint(Canvas canvas, Size size) {
    final g = _DynamicsGeometry(size, compact);
    void text(String value, Offset at, {Color color = U4.text7, double? font}) {
      final t = TextPainter(
          text: TextSpan(
              text: value,
              style: AppDesign.type(font ?? (compact ? 8 : 12), color: color)),
          textDirection: TextDirection.ltr)
        ..layout();
      t.paint(canvas, at - Offset(t.width / 2, t.height / 2));
    }

    for (var db = -80.0; db <= 0; db += 10) {
      canvas.drawLine(Offset(g.x(db), g.top), Offset(g.x(db), g.top + g.height),
          Paint()..color = U4.linePanel);
      canvas.drawLine(Offset(g.left, g.y(db)),
          Offset(g.left + g.width, g.y(db)), Paint()..color = U4.linePanel);
      text('${db.toInt()}${db == 0 ? ' dB' : ''}',
          Offset(g.x(db), size.height * .94));
      text(
          '${db.toInt()}', Offset(size.width * (compact ? .71 : .80), g.y(db)));
    }
    canvas.drawLine(
        Offset(g.x(-80), g.y(-80)),
        Offset(g.x(0), g.y(0)),
        Paint()
          ..color = AppDesign.panShape
          ..strokeWidth = 1.5);
    final curve = Path();
    if (dyn.kind == DynKind.gate) {
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
      ..lineTo(dyn.kind == DynKind.gate ? g.x(dyn.thr) : g.x(-80), g.y(-80))
      ..close();
    canvas.drawPath(
        fill, Paint()..color = AppDesign.meter.withValues(alpha: .22));
    canvas.drawPath(
        curve,
        Paint()
          ..color = U4.cyan
          ..strokeWidth = 2
          ..style = PaintingStyle.stroke);
    for (final prop in [
      'thr',
      if (dyn.kind == DynKind.comp) ...['makeup', 'ratio']
    ]) {
      final p = g.node(dyn, prop);
      final r = compact ? 21.0 : (size.width * 48 / 1280).clamp(22.0, 48.0);
      canvas.drawCircle(p, r, Paint()..color = AppDesign.chip);
      canvas.drawCircle(
          p,
          r,
          Paint()
            ..color = U4.cyan
            ..strokeWidth = 2
            ..style = PaintingStyle.stroke);
      text(switch (prop) { 'thr' => 'T', 'ratio' => 'R', _ => 'G' }, p,
          color: U4.text1, font: compact ? 17 : r * .8);
    }
    // No synthetic detector: missing GR also leaves derived OUT unavailable.
    final out = reduction == null
        ? null
        : level +
            reduction! +
            (dyn.kind == DynKind.comp && dyn.on ? dyn.makeup : 0);
    final values = [level, reduction, out];
    for (var i = 0; i < 3; i++) {
      final x = g.meterX(i);
      final w = size.width * (compact ? 26 / 752 : 24 / 1280);
      final rect = Rect.fromLTWH(x, g.top, w, g.height);
      canvas.drawRRect(RRect.fromRectAndRadius(rect, const Radius.circular(2)),
          Paint()..color = i == 1 ? AppDesign.over : AppDesign.greenBack);
      final value = values[i];
      if (value != null && value.isFinite) {
        final n = i == 1
            ? (-value / 20).clamp(0.0, 1.0)
            : ((value + 80) / 80).clamp(0.0, 1.0);
        canvas.drawRect(
            i == 1
                ? Rect.fromLTWH(x, g.top, w, g.height * n)
                : Rect.fromLTWH(x, g.top + g.height * (1 - n), w, g.height * n),
            Paint()..color = i == 1 ? AppDesign.comp : AppDesign.meter);
      }
      text(['IN', 'GR', 'OUT'][i], Offset(x + w / 2, size.height * .89));
      text(value == null || !value.isFinite ? '—' : value.toStringAsFixed(1),
          Offset(x + w / 2, size.height * .95),
          color: U4.text1);
    }
  }

  @override
  bool shouldRepaint(_DynamicsPainter old) =>
      old.dyn != dyn ||
      old.level != level ||
      old.reduction != reduction ||
      old.compact != compact;
}

class _EqPreviewPainter extends CustomPainter {
  final ChannelEqVm eq;
  const _EqPreviewPainter(this.eq);
  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRRect(
        RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(8)),
        Paint()..color = AppDesign.eqBg);
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
      ..lineTo(size.width, size.height)
      ..lineTo(0, size.height)
      ..close();
    canvas.drawPath(
        fill, Paint()..color = AppDesign.meter.withValues(alpha: .22));
    canvas.drawPath(
        curve,
        Paint()
          ..color = U4.text1
          ..strokeWidth = 1.6
          ..style = PaintingStyle.stroke);
  }

  @override
  bool shouldRepaint(_EqPreviewPainter old) => old.eq != eq;
}
