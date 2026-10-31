// FX rack: slot cards, type choices and rotary parameter controls.
// Screens stay constructor-injected so Area A can implement the store.
import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../palette.dart';
import '../core/selected_control_readout.dart';
import '../core/destination_controls.dart';
import '../widgets/destination_control_editor.dart';
import '../widgets/number_entry.dart';

class FxKnobVm {
  final String id;
  final String label;
  final String? entryTitle;
  final double min;
  final double max;
  final double value;
  final String text;
  final double entryMin;
  final double entryMax;
  final double entryValue;
  final double Function(double)? normalize;
  final List<String> choices;
  final double increment;

  const FxKnobVm({
    required this.id,
    required this.label,
    this.entryTitle,
    required this.min,
    required this.max,
    required this.value,
    required this.text,
    this.entryMin = 0,
    this.entryMax = 1,
    this.entryValue = 0,
    this.normalize,
    this.choices = const [],
    this.increment = 0.01,
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
  int get controlAccessEpoch => 0;
  Object? get controlSession => null;
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
  String? get tapTarget;
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
  final ValueChanged<SelectedControlReadoutData?> onReadout;
  const U4FxPage({super.key, required this.store, required this.onReadout});
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
  String? _focusedId;
  String? _focusedTarget;
  Object? _focusContext;

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
                final focusContext = (
                  store.controlSession,
                  store.controlAccessEpoch,
                  selected?.key,
                  selected?.slot?.type ??
                      selected?.controls.firstOrNull?.targetName
                );
                if (_focusContext != focusContext) {
                  _focusContext = focusContext;
                  _focusedTarget = null;
                  _focusedId = null;
                }
                final readout = selected == null
                    ? null
                    : _selectedReadout(context, selected);
                final readoutSession = store.controlSession;
                WidgetsBinding.instance.addPostFrameCallback((_) {
                  if (mounted && readoutSession == store.controlSession) {
                    widget.onReadout(readout);
                  }
                });
                return LayoutBuilder(builder: (context, bounds) {
                  final palette = ConsolePalette.of(context);
                  final width = math.max(0.0, bounds.maxWidth - 24);
                  final cardWidth = targets.isEmpty
                      ? 180.0
                      : ((width - 6 * (targets.length - 1)) / targets.length)
                          .clamp(148.0, 240.0);
                  final detailHeight = math.max(330.0, bounds.maxHeight - 158);
                  return Column(children: [
                    Expanded(
                        child: SingleChildScrollView(
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
                                            separatorBuilder:
                                                (context, index) =>
                                                    const SizedBox(width: 6),
                                            itemBuilder: (context, index) {
                                              final target = targets[index];
                                              return SizedBox(
                                                  width: cardWidth,
                                                  child: _FxCard(
                                                      store: store,
                                                      target: target,
                                                      selected:
                                                          target == selected,
                                                      onSelect: () {
                                                        setState(() =>
                                                            _selection =
                                                                target.key);
                                                        _focusedId = null;
                                                        _focusedTarget = null;
                                                        if (target.slot
                                                            case final slot?) {
                                                          store.selectSlot(
                                                              slot.slot);
                                                        }
                                                      }));
                                            })),
                                  if (selected != null) ...[
                                    const SizedBox(height: 8),
                                    if (selected.slot case final slot?)
                                      _LegacyFxDetail(
                                          store: store,
                                          slot: slot,
                                          minHeight: detailHeight,
                                          onFocus: (id) => setState(() {
                                                _focusedTarget = selected.key;
                                                _focusedId = id;
                                              }))
                                    else
                                      _NativeFxDetail(
                                          store: store,
                                          controls: selected.controls,
                                          minHeight: detailHeight,
                                          onFocus: (id) => setState(() {
                                                _focusedTarget = selected.key;
                                                _focusedId = id;
                                              })),
                                  ],
                                ]))),
                  ]);
                });
              })));

  SelectedControlReadoutData _selectedReadout(
      BuildContext context, _FxTarget target) {
    final store = widget.store;
    final native = _focusedTarget == target.key
        ? target.controls.where((c) => c.id == _focusedId).firstOrNull
        : null;
    final knob = _focusedTarget == target.key && target.slot != null
        ? store.knobs.where((k) => k.id == _focusedId).firstOrNull
        : null;
    return SelectedControlReadoutData(
      target: target.slot == null
          ? target.controls.first.targetName ??
              target.controls.first.targetLabel
          : '${target.slot!.name} · ${store.typeLabel(target.slot!.type)}',
      label: native?.label ?? knob?.label ?? 'Select a control',
      value: native?.valueLabel ?? knob?.text ?? '—',
      unconfirmed:
          native?.valueLabel != null && native?.authority == 'accepted',
      onEdit: native != null && store.destinationControlIsCurrent(native)
          ? () async {
              if (!mounted ||
                  _focusedTarget != target.key ||
                  _focusedId != native.id ||
                  !store.destinationControlIsCurrent(native)) {
                return;
              }
              await showDialog<void>(
                  animationStyle: AnimationStyle.noAnimation,
                  context: context,
                  barrierDismissible: false,
                  builder: (_) => DestinationControlEditor(
                      accessEpoch: () => store.controlAccessEpoch,
                      changes: store,
                      isCurrent: store.destinationControlIsCurrent,
                      apply: store.setDestinationControl,
                      control: native));
            }
          : knob == null
              ? null
              : () async {
                  if (!mounted ||
                      _focusedTarget != target.key ||
                      _focusedId != knob.id ||
                      store.selected >= store.slots.length ||
                      store.slots[store.selected].slot != target.slot?.slot ||
                      store.slots[store.selected].type != target.slot?.type) {
                    return;
                  }
                  final epoch = store.controlAccessEpoch;
                  final session = store.controlSession;
                  final slot = target.slot!;
                  final double? value;
                  if (knob.choices.isNotEmpty) {
                    value = await showDialog<double>(
                        animationStyle: AnimationStyle.noAnimation,
                        context: context,
                        builder: (context) =>
                            SimpleDialog(title: Text(knob.label), children: [
                              for (var i = 0; i < knob.choices.length; i++)
                                SimpleDialogOption(
                                    onPressed: () =>
                                        Navigator.pop(context, i.toDouble()),
                                    child: Text(knob.choices[i]))
                            ]));
                  } else {
                    value = await showNumberEntry(
                        context: context,
                        title: knob.entryTitle ?? knob.label,
                        initial: knob.entryValue.toString(),
                        min: knob.entryMin,
                        max: knob.entryMax);
                  }
                  if (!mounted ||
                      value == null ||
                      epoch != store.controlAccessEpoch ||
                      session != store.controlSession ||
                      !value.isFinite ||
                      value < knob.entryMin ||
                      value > knob.entryMax ||
                      _focusedTarget != target.key ||
                      _focusedId != knob.id ||
                      store.selected >= store.slots.length ||
                      store.slots[store.selected].slot != slot.slot ||
                      store.slots[store.selected].type != slot.type ||
                      !store.knobs.any((k) =>
                          k.id == knob.id &&
                          k.min == knob.min &&
                          k.max == knob.max &&
                          k.entryMin == knob.entryMin &&
                          k.entryMax == knob.entryMax)) {
                    return;
                  }
                  store.setParam(knob.id, knob.normalize?.call(value) ?? value);
                },
    );
  }
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
  final ValueChanged<String> onFocus;
  const _NativeFxDetail(
      {required this.store,
      required this.controls,
      required this.minHeight,
      required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final selector =
        controls.where((control) => control.id == 'algorithm').firstOrNull;
    final tapControls =
        controls.where((control) => control.canTapTempo).toList();
    final binaryControls = controls
        .where((control) => !control.numeric && control.choices.length == 2)
        .toList();
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
          if (selector != null) ...[
            OutlinedButton.icon(
              icon: const Icon(Icons.expand_more),
              label: Text(
                '${selector.label}: ${selector.valueLabel ?? 'Waiting for mixer'}',
              ),
              onPressed: store.destinationControlIsCurrent(selector)
                  ? () => showDialog<void>(
                        animationStyle: AnimationStyle.noAnimation,
                        context: context,
                        barrierDismissible: false,
                        builder: (_) => DestinationControlEditor(
                          accessEpoch: () => store.controlAccessEpoch,
                          changes: store,
                          isCurrent: store.destinationControlIsCurrent,
                          apply: store.setDestinationControl,
                          control: selector,
                        ),
                      )
                  : null,
            ),
            const SizedBox(height: 8),
            Text(
              'FX preset library is unavailable for this mixer.',
              style: TextStyle(fontSize: 12, color: palette.text4),
            ),
            const SizedBox(height: 12),
          ],
          if (binaryControls.isNotEmpty)
            Padding(
                padding: const EdgeInsets.only(bottom: 12),
                child: Wrap(spacing: 12, runSpacing: 12, children: [
                  for (final control in binaryControls)
                    Tooltip(
                        message: control.description,
                        child: _FxToggle(
                            label:
                                '${control.label}: ${control.valueLabel ?? 'Unavailable'}'
                                '${control.authority == 'accepted' ? ' (unconfirmed)' : ''}',
                            active: control.value == control.choices.last.value,
                            onTap: store.destinationControlIsCurrent(control)
                                ? () async {
                                    final next = control.value ==
                                            control.choices.first.value
                                        ? control.choices.last.value
                                        : control.choices.first.value;
                                    final error = await store
                                        .setDestinationControl(control, next);
                                    if (context.mounted && error != null) {
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                              SnackBar(content: Text(error)));
                                    }
                                  }
                                : null)),
                ])),
          _KnobLayout(
              minHeight: math.max(
                  0.0,
                  minHeight -
                      68 -
                      64 * tapControls.length -
                      (binaryControls.isEmpty ? 0 : 56)),
              children: [
                for (final control in controls)
                  if (control != selector && !binaryControls.contains(control))
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
                        onTap: () => onFocus(control.id)),
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
  final ValueChanged<String> onFocus;
  const _LegacyFxDetail(
      {required this.store,
      required this.slot,
      required this.minHeight,
      required this.onFocus});

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
          if (store.typeOptions.isNotEmpty)
            DropdownButton<String>(
              isExpanded: true,
              value: store.typeOptions.contains(slot.type) ? slot.type : null,
              hint: Text(store.typeLabel(slot.type)),
              items: [
                for (final type in store.typeOptions)
                  DropdownMenuItem(
                      value: type, child: Text(store.typeLabel(type))),
              ],
              onChanged: (type) {
                if (type != null && type != slot.type) store.setType(type);
              },
            ),
          const SizedBox(height: 12),
          _KnobLayout(minHeight: minHeight - 130, children: [
            for (final knob in knobs)
              _FxRotary(
                  key: ValueKey('${slot.slot}/${knob.id}'),
                  label: knob.label,
                  text: knob.text,
                  increment: knob.increment,
                  fraction: knob.max > knob.min
                      ? ((knob.value - knob.min) / (knob.max - knob.min))
                          .clamp(0, 1)
                      : 0,
                  enabled: true,
                  onTap: () => onFocus(knob.id),
                  onChange: (fraction) {
                    onFocus(knob.id);
                    store.setParam(
                        knob.id, knob.min + fraction * (knob.max - knob.min));
                  }),
          ]),
          if (store.tapVisible)
            Align(
                child: _FxToggle(
                    label: store.tapLabel,
                    active: false,
                    onTap: () {
                      final id = store.tapTarget;
                      if (id != null) onFocus(id);
                      store.tap();
                    })),
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
  final double increment;
  final bool enabled;
  final ValueChanged<double>? onChange;
  final VoidCallback? onTap;
  const _FxRotary(
      {super.key,
      required this.label,
      required this.text,
      required this.fraction,
      this.increment = 0.01,
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
        onIncrease: adjustable ? () => _step(widget.increment) : null,
        onDecrease: adjustable ? () => _step(-widget.increment) : null,
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
