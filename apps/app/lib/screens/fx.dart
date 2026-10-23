// Area D: FX screen. 4-slot FX rack: slot cards with live meter, type grid,
// on/bypass toggle, param drag bars, tap tempo on Delay.
//
// Wiring: everything flows through [FxStore] (constructor injection —
// Area A implements it over the engine via cmdFxSetType/Param/Bypass/On;
// this file only renders ui4 tokens, see lib/theme.dart). No Material
// Card/AppBar/Switch/Slider; knobs are drag bars (GestureDetector).
import 'dart:async';

import 'package:flutter/material.dart';

import '../theme.dart';
import '../palette.dart';

/// Purple FX accent (design knobItem colour for the FX screen).

/// One param row in the selected slot (stable param key + range + value).
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

/// One rack slot card.
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

/// Minimal store contract the FX screen renders. Implemented by the
/// shell/owner over the live engine or a fixture sim. Meters arrive via
/// stable [ValueNotifier]s updated in place by [pollMeters].
abstract class FxStore extends ChangeNotifier {
  List<FxSlotVm> get slots;
  int get selected;
  List<String> get typeOptions;
  List<FxKnobVm> get knobs;
  bool get tapVisible;
  String get tapLabel;
  ValueNotifier<double> meterOf(int slot);

  Future<void> pollMeters();

  void selectSlot(int slot);
  void setType(String type); // cmdFxSetType
  void setParam(String id, double value); // cmdFxParam
  void setOn(bool on); // cmdFxOn
  void setBypass(bool bypass); // cmdFxBypass
  void tap(); // tap tempo (Delay)
}

class U4FxPage extends StatefulWidget {
  final FxStore store;

  const U4FxPage({super.key, required this.store});

  @override
  State<U4FxPage> createState() => _U4FxPageState();
}

class _U4FxPageState extends State<U4FxPage> {
  // Freshness comes from the single shell-owned 80 ms loop
  // (AppShell -> AppStore.pollMeters); this page owns no timer and only
  // listens to already-updating notifiers.
  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            final sel = store.slots.isEmpty
                ? null
                : store.slots[store.selected.clamp(0, store.slots.length - 1)];
            return SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text('FX',
                      style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.6,
                          color: ConsolePalette.of(context).text1)),
                  const SizedBox(height: 8),
                  GridView.builder(
                    shrinkWrap: true,
                    physics: const NeverScrollableScrollPhysics(),
                    gridDelegate:
                        const SliverGridDelegateWithMaxCrossAxisExtent(
                      maxCrossAxisExtent: 320,
                      mainAxisSpacing: 6,
                      crossAxisSpacing: 6,
                      childAspectRatio: 2.1,
                    ),
                    itemCount: store.slots.length,
                    itemBuilder: (context, i) {
                      final s = store.slots[i];
                      final on = i == store.selected;
                      final stateOn = s.on && !s.bypass;
                      return RepaintBoundary(
                        child: GestureDetector(
                          onTap: () => store.selectSlot(i),
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 10, vertical: 8),
                            decoration: BoxDecoration(
                              color: on
                                  ? ConsolePalette.of(context).selected
                                  : ConsolePalette.of(context).panel,
                              border: Border.all(
                                color: on
                                    ? ConsolePalette.of(context).cyan
                                    : ConsolePalette.of(context).tileBorder,
                                width: on ? 2 : 1,
                              ),
                              borderRadius: U4.rBtn,
                            ),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              mainAxisAlignment: MainAxisAlignment.center,
                              children: [
                                Row(
                                  children: [
                                    Expanded(
                                      child: Text(s.name,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                              fontSize: 11,
                                              color: ConsolePalette.of(context)
                                                  .text7)),
                                    ),
                                    Text(
                                      stateOn ? 'On' : 'Bypassed',
                                      style: TextStyle(
                                        fontSize: 11,
                                        fontWeight: FontWeight.w700,
                                        color: stateOn
                                            ? ConsolePalette.of(context)
                                                .meterGreen
                                            : ConsolePalette.of(context)
                                                .meterAmber,
                                      ),
                                    ),
                                  ],
                                ),
                                Text(s.type,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                        fontSize: 16,
                                        fontWeight: FontWeight.w700,
                                        color:
                                            ConsolePalette.of(context).text1)),
                                const SizedBox(height: 4),
                                ValueListenableBuilder<double>(
                                  valueListenable: store.meterOf(s.slot),
                                  builder: (context, lv, _) {
                                    return Container(
                                      height: 4,
                                      decoration: BoxDecoration(
                                        color:
                                            ConsolePalette.of(context).control,
                                        borderRadius: BorderRadius.all(
                                            Radius.circular(2)),
                                      ),
                                      child: FractionallySizedBox(
                                        alignment: Alignment.centerLeft,
                                        widthFactor: (stateOn ? lv : 0.0)
                                            .clamp(0.0, 1.0),
                                        child: Container(
                                          decoration: BoxDecoration(
                                            color:
                                                ConsolePalette.of(context).cyan,
                                            borderRadius: BorderRadius.all(
                                                Radius.circular(2)),
                                          ),
                                        ),
                                      ),
                                    );
                                  },
                                ),
                                if (s.ret.isNotEmpty)
                                  Text(s.ret,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                          fontSize: 11,
                                          color: ConsolePalette.of(context)
                                              .text7)),
                              ],
                            ),
                          ),
                        ),
                      );
                    },
                  ),
                  if (sel != null) ...[
                    const SizedBox(height: 10),
                    _DetailPanel(store: store, slot: sel),
                  ],
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _DetailPanel extends StatelessWidget {
  final FxStore store;
  final FxSlotVm slot;

  const _DetailPanel({required this.store, required this.slot});

  @override
  Widget build(BuildContext context) {
    final stateOn = slot.on && !slot.bypass;
    return Container(
      padding: const EdgeInsets.all(14),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(color: ConsolePalette.of(context).linePanel),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text('FX ${slot.slot} · ${slot.type}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontSize: 18,
                  fontWeight: FontWeight.w600,
                  color: ConsolePalette.of(context).text1)),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: GestureDetector(
                  onTap: () => store.setOn(!slot.on),
                  child: Container(
                    padding: const EdgeInsets.symmetric(vertical: 12),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: slot.on
                          ? ConsolePalette.of(context).cyan
                          : ConsolePalette.of(context).warningContainer,
                      border: Border.all(
                          color: slot.on
                              ? ConsolePalette.of(context).cyan
                              : ConsolePalette.of(context).meterAmber),
                      borderRadius: U4.rBtn,
                    ),
                    child: Text(slot.on ? 'On' : 'Off',
                        textAlign: TextAlign.center,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: slot.on
                                ? ConsolePalette.of(context).onAccent
                                : ConsolePalette.of(context)
                                    .onWarningContainer)),
                  ),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: GestureDetector(
                  onTap: () => store.setBypass(!slot.bypass),
                  child: Container(
                    padding: const EdgeInsets.symmetric(vertical: 12),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: slot.bypass
                          ? ConsolePalette.of(context).solo
                          : ConsolePalette.of(context).well,
                      border: Border.all(
                          color: slot.bypass
                              ? ConsolePalette.of(context).solo
                              : ConsolePalette.of(context).lineCtl),
                      borderRadius: U4.rBtn,
                    ),
                    child: Text(slot.bypass ? 'Bypassed' : 'Bypass',
                        textAlign: TextAlign.center,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: slot.bypass
                                ? ConsolePalette.of(context).onSolo
                                : ConsolePalette.of(context).text1)),
                  ),
                ),
              ),
              if (store.tapVisible) ...[
                const SizedBox(width: 6),
                Expanded(
                  child: GestureDetector(
                    onTap: store.tap,
                    child: Container(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: ConsolePalette.of(context).well,
                        border: Border.all(
                            color: ConsolePalette.of(context).lineCtl),
                        borderRadius: U4.rBtn,
                      ),
                      child: Text(store.tapLabel,
                          textAlign: TextAlign.center,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.w700,
                              color: ConsolePalette.of(context).text1)),
                    ),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final t in store.typeOptions)
                GestureDetector(
                  onTap: () => store.setType(t),
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 12, vertical: 10),
                    decoration: BoxDecoration(
                      color: t == slot.type
                          ? ConsolePalette.of(context).cyan
                          : ConsolePalette.of(context).well,
                      borderRadius: U4.rBtn,
                    ),
                    child: Text(t,
                        textAlign: TextAlign.center,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w600,
                          color: t == slot.type
                              ? ConsolePalette.of(context).onAccent
                              : ConsolePalette.of(context).text4,
                        )),
                  ),
                ),
            ],
          ),
          const SizedBox(height: 12),
          for (final k in store.knobs)
            _ParamBar(
              knob: k,
              enabled: stateOn,
              onChange: (v) => store.setParam(k.id, v),
            ),
        ],
      ),
    );
  }
}

/// One param drag bar (GestureDetector horizontal drag — no Material
/// Slider). Disabled (dimmed, ignore) while the slot is bypassed/off.
class _ParamBar extends StatelessWidget {
  final FxKnobVm knob;
  final bool enabled;
  final ValueChanged<double> onChange;

  const _ParamBar({
    required this.knob,
    required this.enabled,
    required this.onChange,
  });

  @override
  Widget build(BuildContext context) {
    final span = (knob.max - knob.min).abs() < 1e-9 ? 1.0 : knob.max - knob.min;
    final frac = ((knob.value - knob.min) / span).clamp(0.0, 1.0);
    return Opacity(
      opacity: enabled ? 1.0 : 0.45,
      child: Padding(
        padding: const EdgeInsets.only(bottom: 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(knob.label,
                      style: TextStyle(
                          fontSize: 12,
                          color: ConsolePalette.of(context).text4)),
                ),
                Text(knob.text,
                    style: TextStyle(
                        fontSize: 13, color: ConsolePalette.of(context).text1)),
              ],
            ),
            const SizedBox(height: 4),
            GestureDetector(
              behavior: HitTestBehavior.opaque,
              onHorizontalDragUpdate: enabled
                  ? (d) {
                      final box = context.findRenderObject() as RenderBox?;
                      if (box == null) return;
                      final w = box.size.width;
                      if (w <= 0) return;
                      final dx = d.localPosition.dx.clamp(0.0, w);
                      onChange(knob.min + span * (dx / w));
                    }
                  : null,
              onTapDown: enabled
                  ? (d) {
                      final box = context.findRenderObject() as RenderBox?;
                      if (box == null) return;
                      final w = box.size.width;
                      if (w <= 0) return;
                      final dx = d.localPosition.dx.clamp(0.0, w);
                      onChange(knob.min + span * (dx / w));
                    }
                  : null,
              child: Container(
                height: 28,
                alignment: Alignment.center,
                child: Container(
                  height: 6,
                  decoration: BoxDecoration(
                    color: ConsolePalette.of(context).control,
                    borderRadius: BorderRadius.all(Radius.circular(10)),
                  ),
                  child: FractionallySizedBox(
                    alignment: Alignment.centerLeft,
                    widthFactor: frac,
                    child: Container(
                      decoration: BoxDecoration(
                        color: ConsolePalette.of(context).cyan,
                        borderRadius: BorderRadius.all(Radius.circular(10)),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
