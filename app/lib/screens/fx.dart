// FX rack: slot cards, type choices and rotary parameter controls.
// Screens stay constructor-injected so Area A can implement the store.
import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../palette.dart';
import '../core/destination_controls.dart';
import '../widgets/destination_control_editor.dart';

class FxKnobVm {
  final String id;
  final String label;
  final double min;
  final double max;
  final double value;
  final String text;

  const FxKnobVm({
    required this.id,
    required this.label,
    required this.min,
    required this.max,
    required this.value,
    required this.text,
  });
}

class FxSlotVm {
  final int slot;
  final String name; // `FX 1`..
  final String type; // display name / desk code
  final bool on;
  final bool bypass;
  final String ret; // return label, e.g. `FxRet 1`

  const FxSlotVm({
    required this.slot,
    required this.name,
    required this.type,
    this.on = true,
    this.bypass = false,
    this.ret = '',
  });
}

// Implemented by the shell over the engine or a fixture sim.
abstract class FxStore extends ChangeNotifier {
  bool get stateControlsSupported => false;
  List<DestinationControlVm> get destinationControls => const [];
  bool destinationControlIsCurrent(DestinationControlVm control) => false;
  Future<String?> setDestinationControl(
          DestinationControlVm control, double value) =>
      Future.value('This FX control is unavailable.');
  Future<String?> tapDestinationControl(DestinationControlVm control) =>
      Future.value('Tap tempo is unavailable.');
  List<FxSlotVm> get slots;
  int get selected;
  List<String> get typeOptions;
  String typeLabel(String type);
  List<FxKnobVm> get knobs;
  bool get tapVisible;
  String get tapLabel;
  ValueNotifier<double> meterOf(int slot);

  Future<void> pollMeters();

  void selectSlot(int slot);
  void setType(String type);
  void setParam(String id, double value);
  void setOn(bool on);
  void setBypass(bool bypass);
  void tap();
}

class U4FxPage extends StatefulWidget {
  final FxStore store;
  const U4FxPage({super.key, required this.store});
  @override
  State<U4FxPage> createState() => _U4FxPageState();
}

class _FxTarget {
  final String key;
  final FxSlotVm? slot;
  final List<DestinationControlVm> controls;
  const _FxTarget(this.key, {this.slot, this.controls = const []});
}

class _U4FxPageState extends State<U4FxPage> {
  String? _selection;

  @override
  Widget build(BuildContext context) => Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
          child: ListenableBuilder(
              listenable: widget.store,
              builder: (context, _) {
                final store = widget.store;
                final groups = <String, List<DestinationControlVm>>{};
                for (final control in store.destinationControls) {
                  groups.putIfAbsent(control.target, () => []).add(control);
                }
                final targets = [
                  for (final entry in groups.entries)
                    _FxTarget('destination:${entry.key}',
                        controls: entry.value),
                  for (final slot in store.slots)
                    _FxTarget('slot:${slot.slot}', slot: slot),
                ];
                final fallback = groups.isNotEmpty || store.slots.isEmpty
                    ? targets.firstOrNull
                    : targets[store.selected.clamp(0, targets.length - 1)];
                final selected =
                    targets.where((t) => t.key == _selection).firstOrNull ??
                        fallback;
                return LayoutBuilder(builder: (context, bounds) {
                  final palette = ConsolePalette.of(context);
                  final width = math.max(0.0, bounds.maxWidth - 24);
                  final cardWidth = targets.isEmpty
                      ? 180.0
                      : ((width - 6 * (targets.length - 1)) / targets.length)
                          .clamp(148.0, 240.0);
                  final detailHeight = math.max(330.0, bounds.maxHeight - 158);
                  return SingleChildScrollView(
                      padding: const EdgeInsets.fromLTRB(12, 10, 12, 16),
                      child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Text('FX',
                                style: TextStyle(
                                    fontSize: 15,
                                    fontWeight: FontWeight.w700,
                                    letterSpacing: .6,
                                    color: palette.text1)),
                            const SizedBox(height: 8),
                            if (targets.isEmpty)
                              Text(
                                  'No FX controls are available for this mixer.',
                                  style: TextStyle(color: palette.text4)),
                            if (targets.isNotEmpty)
                              SizedBox(
                                  height: 112,
                                  child: ListView.separated(
                                      scrollDirection: Axis.horizontal,
                                      itemCount: targets.length,
                                      separatorBuilder: (context, index) =>
                                          const SizedBox(width: 6),
                                      itemBuilder: (context, index) {
                                        final target = targets[index];
                                        return SizedBox(
                                            width: cardWidth,
                                            child: _FxCard(
                                                store: store,
                                                target: target,
                                                selected: target == selected,
                                                onSelect: () {
                                                  setState(() =>
                                                      _selection = target.key);
                                                  if (target.slot
                                                      case final slot?) {
                                                    store.selectSlot(slot.slot);
                                                  }
                                                }));
                                      })),
                            if (selected != null) ...[
                              const SizedBox(height: 8),
                              if (selected.slot case final slot?)
                                _LegacyFxDetail(
                                    store: store,
                                    slot: slot,
                                    minHeight: detailHeight)
                              else
                                _NativeFxDetail(
                                    store: store,
                                    controls: selected.controls,
                                    minHeight: detailHeight),
                            ],
                          ]));
                });
              })));
}

class _FxCard extends StatelessWidget {
  final FxStore store;
  final _FxTarget target;
  final bool selected;
  final VoidCallback onSelect;
  const _FxCard(
      {required this.store,
      required this.target,
      required this.selected,
      required this.onSelect});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final slot = target.slot;
    final control = target.controls.firstOrNull;
    final title = slot?.name ?? control!.targetLabel;
    final type = slot == null
        ? control!.targetName ?? control.targetLabel
        : store.typeLabel(slot.type);
    final status = slot != null
        ? !store.stateControlsSupported
            ? '—'
            : slot.bypass
                ? 'Bypassed'
                : slot.on
                    ? 'On'
                    : 'Off'
        : target.controls.any((c) => !c.available)
            ? 'Unavailable'
            : target.controls.any((c) => c.authority == 'accepted')
                ? 'Unconfirmed'
                : 'Live';
    return Semantics(
        button: true,
        selected: selected,
        label: '$title, $type, $status',
        child: InkWell(
            onTap: onSelect,
            child: Container(
                padding: const EdgeInsets.all(10),
                decoration: BoxDecoration(
                    color: selected ? palette.selected : palette.panel,
                    border: Border.all(
                        color: selected
                            ? palette.processingPurple
                            : palette.tileBorder,
                        width: selected ? 2 : 1),
                    borderRadius: BorderRadius.circular(2)),
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(children: [
                        Expanded(
                            child: Text(title,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                    fontSize: 11, color: palette.text7))),
                        Text(status,
                            style: TextStyle(
                                fontSize: 10,
                                color: status == 'On' || status == 'Live'
                                    ? palette.meterGreen
                                    : palette.meterAmber)),
                      ]),
                      const SizedBox(height: 5),
                      Expanded(
                          child: Align(
                              alignment: Alignment.centerLeft,
                              child: Text(type,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                      fontSize: 16,
                                      fontWeight: FontWeight.w700,
                                      color: palette.text1)))),
                      if (slot != null) ...[
                        Text(slot.ret,
                            maxLines: 1,
                            style:
                                TextStyle(fontSize: 10, color: palette.text7)),
                        const SizedBox(height: 4),
                        ValueListenableBuilder<double>(
                            valueListenable: store.meterOf(slot.slot),
                            builder: (context, value, _) =>
                                LinearProgressIndicator(
                                    value: value.clamp(0, 1),
                                    minHeight: 3,
                                    color: palette.meterGreen,
                                    backgroundColor: palette.control)),
                      ] else
                        Text(control!.valueLabel ?? 'Unavailable',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style:
                                TextStyle(fontSize: 11, color: palette.text7)),
                    ]))));
  }
}

BoxDecoration _detailDecoration(ConsolePalette palette) => BoxDecoration(
    color: palette.panel,
    border: Border.all(color: palette.linePanel),
    borderRadius: BorderRadius.circular(2));

class _NativeFxDetail extends StatelessWidget {
  final FxStore store;
  final List<DestinationControlVm> controls;
  final double minHeight;
  const _NativeFxDetail(
      {required this.store, required this.controls, required this.minHeight});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final tapControls =
        controls.where((control) => control.canTapTempo).toList();
    return Container(
        padding: const EdgeInsets.all(14),
        decoration: _detailDecoration(palette),
        child:
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Text(controls.first.destinationName,
              style: TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                  color: palette.text1)),
          const SizedBox(height: 12),
          _KnobLayout(
              minHeight:
                  math.max(0.0, minHeight - 68 - 64 * tapControls.length),
              children: [
                for (final control in controls)
                  _FxRotary(
                      key: ValueKey('${control.target}/${control.id}'),
                      label: control.label,
                      text: '${control.valueLabel ?? 'Unavailable'}'
                          '${control.authority == 'accepted' ? ' (unconfirmed)' : ''}',
                      fraction: control.numeric && control.value != null
                          ? ((control.value! - control.min!) /
                                  (control.max! - control.min!))
                              .clamp(0, 1)
                          : 0,
                      enabled: store.destinationControlIsCurrent(control),
                      onTap: () => showDialog<void>(
                          context: context,
                          barrierDismissible: false,
                          builder: (_) => DestinationControlEditor(
                              changes: store,
                              isCurrent: store.destinationControlIsCurrent,
                              apply: store.setDestinationControl,
                              control: control))),
              ]),
          for (final control in tapControls)
            Padding(
                padding: const EdgeInsets.only(top: 12),
                child: Align(
                    child: Tooltip(
                        message: 'Tap tempo for ${control.destinationName}',
                        child: _FxToggle(
                            label: tapControls.length == 1
                                ? 'Tap tempo'
                                : 'Tap ${control.label}',
                            active: false,
                            onTap: store.destinationControlIsCurrent(control)
                                ? () async {
                                    final error = await store
                                        .tapDestinationControl(control);
                                    if (context.mounted && error != null) {
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                              SnackBar(content: Text(error)));
                                    }
                                  }
                                : null)))),
        ]));
  }
}

class _LegacyFxDetail extends StatelessWidget {
  final FxStore store;
  final FxSlotVm slot;
  final double minHeight;
  const _LegacyFxDetail(
      {required this.store, required this.slot, required this.minHeight});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final knobs = store.knobs;
    return Container(
        padding: const EdgeInsets.all(14),
        decoration: _detailDecoration(palette),
        child:
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          Row(children: [
            Expanded(
                child: Text('FX ${slot.slot} · ${store.typeLabel(slot.type)}',
                    style: TextStyle(
                        fontSize: 18,
                        fontWeight: FontWeight.w600,
                        color: palette.text1))),
            if (store.stateControlsSupported) ...[
              _FxToggle(
                  label: slot.on ? 'On' : 'Off',
                  active: slot.on,
                  activeColor: palette.meterGreen,
                  onTap: () => store.setOn(!slot.on)),
              const SizedBox(width: 6),
              _FxToggle(
                  label: slot.bypass ? 'Bypassed' : 'Bypass',
                  active: slot.bypass,
                  activeColor: palette.meterAmber,
                  onTap: () => store.setBypass(!slot.bypass)),
            ],
          ]),
          const SizedBox(height: 12),
          LayoutBuilder(builder: (context, bounds) {
            final types = store.typeOptions;
            final columns = math.max(
                1, math.min(types.length, (bounds.maxWidth / 140).floor()));
            return Wrap(spacing: 6, runSpacing: 6, children: [
              for (final type in types)
                SizedBox(
                    width: (bounds.maxWidth - 6 * (columns - 1)) / columns,
                    child: _FxToggle(
                        label: store.typeLabel(type),
                        active: type == slot.type,
                        onTap: () => store.setType(type))),
            ]);
          }),
          const SizedBox(height: 12),
          _KnobLayout(minHeight: minHeight - 130, children: [
            for (final knob in knobs)
              _FxRotary(
                  key: ValueKey('${slot.slot}/${knob.id}'),
                  label: knob.label,
                  text: knob.text,
                  fraction: knob.max > knob.min
                      ? ((knob.value - knob.min) / (knob.max - knob.min))
                          .clamp(0, 1)
                      : 0,
                  enabled: true,
                  onChange: (fraction) => store.setParam(
                      knob.id, knob.min + fraction * (knob.max - knob.min))),
          ]),
          if (store.tapVisible)
            Align(
                child: _FxToggle(
                    label: store.tapLabel, active: false, onTap: store.tap)),
        ]));
  }
}

class _FxToggle extends StatelessWidget {
  final String label;
  final bool active;
  final Color? activeColor;
  final VoidCallback? onTap;
  const _FxToggle(
      {required this.label,
      required this.active,
      required this.onTap,
      this.activeColor});
  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return TextButton(
        onPressed: onTap,
        style: TextButton.styleFrom(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
            backgroundColor:
                active ? activeColor ?? palette.processingPurple : palette.well,
            foregroundColor: active ? palette.onAccent : palette.text4,
            shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(2),
                side: BorderSide(color: palette.lineCtl))),
        child: Text(label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600)));
  }
}

class _KnobLayout extends StatelessWidget {
  final double minHeight;
  final List<Widget> children;
  const _KnobLayout({required this.minHeight, required this.children});
  @override
  Widget build(BuildContext context) =>
      LayoutBuilder(builder: (context, bounds) {
        final columns = math.max(
            1, math.min(children.length, (bounds.maxWidth / 150).floor()));
        final rows = (children.length / columns).ceil();
        final height = math.max(minHeight, rows * 170.0);
        return SizedBox(
            height: height,
            child: Center(
                child: Wrap(
                    alignment: WrapAlignment.spaceEvenly,
                    runSpacing: 12,
                    children: [
                  for (final child in children)
                    SizedBox(
                        width: bounds.maxWidth / columns,
                        height: 158,
                        child: child),
                ])));
      });
}

class _FxRotary extends StatefulWidget {
  final String label;
  final String text;
  final double fraction;
  final bool enabled;
  final ValueChanged<double>? onChange;
  final VoidCallback? onTap;
  const _FxRotary(
      {super.key,
      required this.label,
      required this.text,
      required this.fraction,
      required this.enabled,
      this.onChange,
      this.onTap});
  @override
  State<_FxRotary> createState() => _FxRotaryState();
}

class _FxRotaryState extends State<_FxRotary> {
  double? _drag;
  void _step(double delta) =>
      widget.onChange?.call((widget.fraction + delta).clamp(0, 1));
  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final adjustable = widget.enabled && widget.onChange != null;
    return Semantics(
        button: widget.enabled && widget.onTap != null,
        slider: adjustable,
        enabled: widget.enabled,
        label: widget.label,
        value: widget.text,
        onIncrease: adjustable ? () => _step(.01) : null,
        onDecrease: adjustable ? () => _step(-.01) : null,
        child: Opacity(
            opacity: widget.enabled ? 1 : .45,
            child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: widget.enabled ? widget.onTap : null,
                onVerticalDragStart:
                    adjustable ? (_) => _drag = widget.fraction : null,
                onVerticalDragUpdate: adjustable
                    ? (details) {
                        _drag = ((_drag ?? widget.fraction) -
                                details.delta.dy / 160)
                            .clamp(0, 1);
                        widget.onChange!(_drag!);
                      }
                    : null,
                onVerticalDragEnd: adjustable ? (_) => _drag = null : null,
                onVerticalDragCancel: adjustable ? () => _drag = null : null,
                child: Column(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      CustomPaint(
                          size: const Size(88, 88),
                          painter: _FxKnobPainter(widget.fraction, palette)),
                      const SizedBox(height: 8),
                      Text(widget.label,
                          textAlign: TextAlign.center,
                          style: TextStyle(fontSize: 12, color: palette.text7)),
                      const SizedBox(height: 4),
                      Text(widget.text,
                          textAlign: TextAlign.center,
                          style: TextStyle(fontSize: 13, color: palette.text1)),
                    ]))));
  }
}

class _FxKnobPainter extends CustomPainter {
  final double fraction;
  final ConsolePalette palette;
  const _FxKnobPainter(this.fraction, this.palette);
  @override
  void paint(Canvas canvas, Size size) {
    final center = size.center(Offset.zero);
    final radius = size.shortestSide * .39;
    final rect = Rect.fromCircle(center: center, radius: radius);
    final arc = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 5
      ..strokeCap = StrokeCap.round
      ..color = palette.lineCtl;
    const start = math.pi * .75;
    const sweep = math.pi * 1.5;
    canvas.drawArc(rect, start, sweep, false, arc);
    arc.color = palette.processingPurple;
    canvas.drawArc(rect, start, sweep * fraction, false, arc);
    canvas.drawCircle(center, radius - 9, Paint()..color = palette.well);
    canvas.drawCircle(
        center,
        radius - 9,
        Paint()
          ..color = palette.control
          ..style = PaintingStyle.stroke
          ..strokeWidth = 3);
    final angle = start + sweep * fraction;
    final needle = Offset(math.cos(angle), math.sin(angle));
    canvas.drawLine(
        center + needle * 7,
        center + needle * (radius - 14),
        Paint()
          ..color = palette.text4
          ..strokeWidth = 2.5
          ..strokeCap = StrokeCap.round);
  }

  @override
  bool shouldRepaint(_FxKnobPainter oldDelegate) =>
      oldDelegate.fraction != fraction || oldDelegate.palette != palette;
}
