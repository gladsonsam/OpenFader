// Channel screen: per-channel preamp / EQ / dynamics / inserts / sends /
// groups / source / presets with the tab rail (input/eq/gate/comp/sends)
//
// Wiring: everything flows through [ChannelStore] (constructor injection —
// no dependency on lib/core/store, lib/shell, lib/state or Area A). The
// owner implements it over the engine (cmdParam/cmdFocus/cmdLock/
// cmdUnlock/cmdSubgroupAssign/cmdMuteGroupAssign); this file only renders
// ui4 tokens (see lib/theme.dart) with CustomPainter DSP controls.
// Portrait-first: horizontal tab rail on top, single vertical scroll of
// section cards. Freshness comes from the single shell-owned 80 ms loop
// (AppShell -> AppStore.pollMeters); this page owns no timer and renders
// levels + spectrum + GR in place via notifiers.

import 'dart:async';

import 'package:flutter/material.dart';

import '../core/channel_views.dart';
import '../theme.dart';
import '../widgets/channel_widgets.dart';

/// Channel tabs (web CHANNEL_TAB_ORDER + groups/source/presets sections).
enum ChannelTab { input, eq, gate, comp, sends, groups, presets }

/// Minimal store contract the channel screen renders. Implemented by the
/// shell/owner over the live engine or a fixture sim. Meters arrive via
/// stable [ValueNotifier]s updated in place by [pollMeters]; DSP snapshots
/// ([eq]/[gate]/[comp]/[sends]/...) notify through [ChangeNotifier].
abstract class ChannelStore extends ChangeNotifier {
  String get stripKey;
  String get displayName;

  // --- param plane (echo-confirmed reads, caps gating) ---
  double? paramNum(String leaf);
  bool? paramBool(String leaf);
  bool supports(String leaf);

  // --- commands ---
  void cmdParam(String leaf, Object value);
  void cmdFocus();
  void cmdLock();
  void cmdUnlock();
  void cmdSubgroupAssign(int g, bool on);
  void cmdMuteGroupAssign(int g, bool on);

  // --- live feeds (80 ms loop target) ---
  ValueNotifier<double> get levelDb;
  ValueNotifier<List<double>> get spectrum; // RTA_BINS magnitudes 0..1
  double? grDb(DynKind kind);
  Future<void> pollMeters();

  // --- snapshots ---
  ChannelPreampVm get preamp;
  ChannelSourceVm get source;
  ChannelEqVm get eq;
  ChannelDynVm get gate;
  ChannelDynVm get comp;
  List<ChannelSendVm> get sends;
  int get subgroupCount;
  int get muteGroupCount;
  bool? subgroupAssigned(int g);
  bool? muteGroupAssigned(int g);
  List<String> get presets;
  bool get locked;
  bool get focused;

  // --- tab + selection state owned here until the Area A merge ---
  ChannelTab get tab;
  void setTab(ChannelTab tab);
  void selectEqBand(int band);

  // --- edits (owner forwards through the 60 Hz coalescer) ---
  void setEqParam(int band, String param, double value);
  void setEqOn(bool on);
  void setEqBandOn(int band, bool on);
  void setHpfSlope(int slope);
  void resetEq();
  void setDynParam(DynKind kind, String prop, double value);
  void setDynOn(DynKind kind, bool on);
  void resetDyn(DynKind kind);
  void setSendLevel(int slot, double value);
  void setSendTap(int slot, int tap);
  void setSendMute(int slot, bool muted);
  void togglePhantom();
  void toggleInvert();
  void setPreamp(String leaf, double value);
  void recallPreset(String name);
  void storePreset(String name);
}

class ChannelPage extends StatefulWidget {
  final String stripKey;
  final ChannelStore store;

  const ChannelPage({super.key, required this.stripKey, required this.store});

  @override
  State<ChannelPage> createState() => _ChannelPageState();
}

class _ChannelPageState extends State<ChannelPage> {
  ChannelWrites? _writes;

  @override
  void initState() {
    super.initState();
    _writes = ChannelWrites(widget.store.cmdParam);
    // Keep the engine's persistent detail lane on this channel
    // (web: w.setOpenChannel(bridgeStrip(w, id))).
    widget.store.cmdFocus();
  }

  @override
  void dispose() {
    _writes?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _TabRail(
                  tab: store.tab,
                  onTab: store.setTab,
                  locked: store.locked,
                  focused: store.focused,
                  onLock: () =>
                      store.locked ? store.cmdUnlock() : store.cmdLock(),
                ),
                Expanded(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.all(8),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        switch (store.tab) {
                          ChannelTab.input =>
                            _InputTab(store: store, writes: _writes!),
                          ChannelTab.eq =>
                            _EqTab(store: store, writes: _writes!),
                          ChannelTab.gate => _DynTab(
                              store: store,
                              writes: _writes!,
                              kind: DynKind.gate),
                          ChannelTab.comp => _DynTab(
                              store: store,
                              writes: _writes!,
                              kind: DynKind.comp),
                          ChannelTab.sends => _SendsTab(store: store),
                          ChannelTab.groups => _GroupsTab(store: store),
                          ChannelTab.presets => _PresetsTab(store: store),
                        },
                      ],
                    ),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Header + tab rail
// ---------------------------------------------------------------------------

class _TabRail extends StatelessWidget {
  final ChannelTab tab;
  final ValueChanged<ChannelTab> onTab;
  final bool locked;
  final bool focused;
  final VoidCallback onLock;

  const _TabRail({
    required this.tab,
    required this.onTab,
    required this.locked,
    required this.focused,
    required this.onLock,
  });

  @override
  Widget build(BuildContext context) {
    const tabs = ChannelTab.values;
    const labels = {
      ChannelTab.input: 'Input',
      ChannelTab.eq: 'EQ',
      ChannelTab.gate: 'Gate',
      ChannelTab.comp: 'Comp',
      ChannelTab.sends: 'Sends',
      ChannelTab.groups: 'Groups',
      ChannelTab.presets: 'Presets',
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: const BoxDecoration(
        border: Border(bottom: BorderSide(color: U4.lineHeader)),
      ),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: [
            for (final t in tabs)
              GestureDetector(
                onTap: () => onTab(t),
                child: Container(
                  margin: const EdgeInsets.only(right: 5),
                  padding:
                      const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                  decoration: BoxDecoration(
                    color: t == tab ? U4.selected : U4.bankOff,
                    borderRadius: U4.rBtn,
                    border:
                        Border.all(color: t == tab ? U4.cyan : U4.railBorder),
                  ),
                  child: Text(labels[t]!,
                      textAlign: TextAlign.center,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                          color: t == tab ? U4.text1 : U4.text7)),
                ),
              ),
            GestureDetector(
              onTap: onLock,
              child: Container(
                margin: const EdgeInsets.only(left: 4),
                padding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(
                  color: locked ? U4.muteOffBg : U4.bankOff,
                  borderRadius: U4.rBtn,
                  border:
                      Border.all(color: locked ? U4.muteOffBd : U4.railBorder),
                ),
                child: Text(locked ? 'UNLOCK' : 'LOCK',
                    textAlign: TextAlign.center,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: locked ? U4.muteOffTx : U4.text7)),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Input tab: preamp + source
// ---------------------------------------------------------------------------

class _InputTab extends StatelessWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  const _InputTab({required this.store, required this.writes});

  @override
  Widget build(BuildContext context) {
    final pre = store.preamp;
    final src = store.source;
    final liveGain = store.paramNum('gain');
    final liveTrim = store.paramNum('trim');
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        U4Section(
          title: 'Preamp',
          child: Column(
            children: [
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceAround,
                children: [
                  U4Knob(
                    label: 'Gain',
                    value: liveGain ?? pre.gain,
                    min: 0,
                    max: 60,
                    text: '${(liveGain ?? pre.gain).toStringAsFixed(1)} dB',
                    defaultValue: 30,
                    enabled: store.supports('gain'),
                    onInput: (v) {
                      writes.queue('gain', v);
                      store.setPreamp('gain', v);
                    },
                  ),
                  U4Knob(
                    label: 'Trim',
                    value: liveTrim ?? pre.trim,
                    min: -18,
                    max: 18,
                    text: '${(liveTrim ?? pre.trim).toStringAsFixed(1)} dB',
                    defaultValue: 0,
                    enabled: store.supports('trim'),
                    onInput: (v) {
                      writes.queue('trim', v);
                      store.setPreamp('trim', v);
                    },
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  Expanded(
                    child: U4Chip(
                      label: pre.phantomArmed
                          ? (pre.phantom
                              ? 'Tap again: 48V off'
                              : 'Tap again: 48V on')
                          : (pre.phantom ? '48V On' : '48V Off'),
                      on: pre.phantom,
                      accent: U4.meterAmber,
                      enabled: store.supports('phantom'),
                      onTap: store.togglePhantom,
                    ),
                  ),
                  const SizedBox(width: 6),
                  Expanded(
                    child: U4Chip(
                      label: pre.invert ? 'Ø Inverted' : 'Ø Normal',
                      on: pre.invert,
                      enabled: store.supports('invert'),
                      onTap: store.toggleInvert,
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 8),
        U4Section(
          title: 'Source',
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text('${src.srcName}  ${src.srcGroup}',
                  style: const TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.w600,
                      color: U4.text1)),
              const SizedBox(height: 8),
              Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  const Text('Input level',
                      style: TextStyle(fontSize: 12, color: U4.text7)),
                  ValueListenableBuilder<double>(
                    valueListenable: store.levelDb,
                    builder: (context, db, _) => Text(
                        db <= -89 ? '-∞ dB' : '${db.toStringAsFixed(1)} dB',
                        style: const TextStyle(
                            fontSize: 12,
                            color: U4.text1,
                            fontFeatures: [FontFeature.tabularFigures()])),
                  ),
                ],
              ),
              const SizedBox(height: 6),
              U4LevelHbar(levelDb: store.levelDb),
            ],
          ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// EQ tab: curve + RTA + HPF + band cards
// ---------------------------------------------------------------------------

class _EqTab extends StatelessWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  const _EqTab({required this.store, required this.writes});

  @override
  Widget build(BuildContext context) {
    final eq = store.eq;
    final response = eqResponseDb(eq);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        U4Section(
          title: 'EQ curve',
          trailing: U4Chip(
            label: eq.on ? 'EQ In' : 'EQ Out',
            on: eq.on,
            onTap: () => store.setEqOn(!eq.on),
          ),
          child: Column(
            children: [
              ValueListenableBuilder<List<double>>(
                valueListenable: store.spectrum,
                builder: (context, bins, _) {
                  final active = bins.any((b) => b > 0.02);
                  return U4EqGraph(
                    eq: eq,
                    responseDb: response,
                    rtaBins: bins,
                    rtaActive: active,
                    onSelectBand: store.selectEqBand,
                    onNode: (band, f, g) {
                      if (band == 0) {
                        writes.queue('eq_hpf_f', f.clamp(20, 1000));
                        store.setEqParam(0, 'f', f.clamp(20, 1000));
                      } else {
                        writes.queue('eq${band}_f', f);
                        writes.queue('eq${band}_g', g);
                        store.setEqParam(band, 'f', f);
                        store.setEqParam(band, 'g', g);
                      }
                    },
                    onQ: (band, q) {
                      writes.queue('eq${band}_q', q);
                      store.setEqParam(band, 'q', q);
                    },
                  );
                },
              ),
              const SizedBox(height: 6),
              _BandDots(eq: eq, onPick: store.selectEqBand),
            ],
          ),
        ),
        const SizedBox(height: 8),
        U4Section(
          title: 'HPF',
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              U4Chip(
                  label: '12',
                  on: eq.hpfSlope == 12,
                  onTap: () => store.setHpfSlope(12)),
              const SizedBox(width: 6),
              U4Chip(
                  label: '24',
                  on: eq.hpfSlope == 24,
                  onTap: () => store.setHpfSlope(24)),
            ],
          ),
          child: Row(
            children: [
              Expanded(
                child: U4Chip(
                  label: eq.hpfOn
                      ? 'HPF ${eq.hpfF.toStringAsFixed(0)} Hz'
                      : 'HPF Off',
                  on: eq.hpfOn,
                  onTap: () => store.setEqBandOn(0, !eq.hpfOn),
                ),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: U4Knob(
                  label: 'Freq',
                  value: eq.hpfF,
                  min: 20,
                  max: 1000,
                  text: '${eq.hpfF.toStringAsFixed(0)} Hz',
                  defaultValue: 80,
                  onInput: (v) {
                    writes.queue('eq_hpf_f', v);
                    store.setEqParam(0, 'f', v);
                  },
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 8),
        for (final b in eq.bands) ...[
          U4Section(
            title: '${b.name} · band ${b.band}',
            trailing: U4Chip(
              label: b.on ? 'In' : 'Out',
              on: b.on,
              onTap: () => store.setEqBandOn(b.band, !b.on),
            ),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceAround,
              children: [
                U4Knob(
                  label: 'Freq',
                  value: b.f,
                  min: b.band == 1 ? 20 : 100,
                  max: b.band == 4 ? 20000 : 16000,
                  text: b.f >= 1000
                      ? '${(b.f / 1000).toStringAsFixed(1)}k'
                      : '${b.f.toStringAsFixed(0)} Hz',
                  defaultValue: const [
                    0.0,
                    100.0,
                    400.0,
                    2500.0,
                    8000.0
                  ][b.band],
                  color: Color(EqGraphGeom
                      .bandColors[b.band % EqGraphGeom.bandColors.length]),
                  onInput: (v) {
                    writes.queue('eq${b.band}_f', v);
                    store.setEqParam(b.band, 'f', v);
                  },
                ),
                U4Knob(
                  label: 'Gain',
                  value: b.g,
                  min: -15,
                  max: 15,
                  text: '${b.g.toStringAsFixed(1)} dB',
                  defaultValue: 0,
                  color: Color(EqGraphGeom
                      .bandColors[b.band % EqGraphGeom.bandColors.length]),
                  onInput: (v) {
                    writes.queue('eq${b.band}_g', v);
                    store.setEqParam(b.band, 'g', v);
                  },
                ),
                U4Knob(
                  label: 'Q',
                  value: b.q,
                  min: 0.3,
                  max: 16,
                  text: b.q.toStringAsFixed(2),
                  defaultValue: 1.4,
                  color: Color(EqGraphGeom
                      .bandColors[b.band % EqGraphGeom.bandColors.length]),
                  onInput: (v) {
                    writes.queue('eq${b.band}_q', v);
                    store.setEqParam(b.band, 'q', v);
                  },
                ),
              ],
            ),
          ),
          const SizedBox(height: 8),
        ],
        GestureDetector(
          onTap: store.resetEq,
          child: Container(
            padding: const EdgeInsets.symmetric(vertical: 14),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: U4.tileBg,
              border: Border.all(color: U4.tileBorder),
              borderRadius: U4.rBtn,
            ),
            child: const Text('Reset EQ',
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: U4.text1)),
          ),
        ),
      ],
    );
  }
}

class _BandDots extends StatelessWidget {
  final ChannelEqVm eq;
  final ValueChanged<int> onPick;
  const _BandDots({required this.eq, required this.onPick});

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        for (var b = 0; b <= eq.bands.length; b++)
          GestureDetector(
            onTap: () => onPick(b),
            child: Container(
              width: 36,
              height: 36,
              alignment: Alignment.center,
              margin: const EdgeInsets.symmetric(horizontal: 2),
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                border: Border.all(
                    color: eq.selectedBand == b
                        ? Colors.white
                        : Colors.transparent,
                    width: 2),
              ),
              child: Container(
                width: 18,
                height: 18,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: Color(EqGraphGeom
                      .bandColors[b % EqGraphGeom.bandColors.length]),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Gate / comp tab: transfer curve + GR + knobs
// ---------------------------------------------------------------------------

class _DynTab extends StatelessWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  final DynKind kind;
  const _DynTab(
      {required this.store, required this.writes, required this.kind});

  @override
  Widget build(BuildContext context) {
    final dyn = kind == DynKind.gate ? store.gate : store.comp;
    final color = kind == DynKind.gate ? U4.cyan : U4.meterAmber;
    final label = kind == DynKind.gate ? 'Gate' : 'Comp';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        U4Section(
          title: '$label transfer',
          trailing: U4Chip(
            label: dyn.on ? '$label In' : '$label Out',
            on: dyn.on,
            accent: color,
            onTap: () => store.setDynOn(kind, !dyn.on),
          ),
          child: Column(
            children: [
              ValueListenableBuilder<double>(
                valueListenable: store.levelDb,
                builder: (context, db, _) => U4DynGraph(
                  dyn: dyn,
                  levelDb: db,
                  color: color,
                  onThreshold: (v) {
                    writes.queue('${kind.name}_thr', v);
                    store.setDynParam(kind, 'thr', v);
                  },
                  onRatio: kind == DynKind.comp
                      ? (v) {
                          writes.queue('comp_ratio', v);
                          store.setDynParam(kind, 'ratio', v);
                        }
                      : null,
                ),
              ),
              const SizedBox(height: 8),
              ValueListenableBuilder<double>(
                valueListenable: store.levelDb,
                builder: (context, db, _) {
                  final gr = store.grDb(kind) ?? dynReductionDb(dyn, db);
                  return U4GrBar(grDb: gr, color: color);
                },
              ),
            ],
          ),
        ),
        const SizedBox(height: 8),
        U4Section(
          title: '$label knobs',
          child: Wrap(
            alignment: WrapAlignment.spaceAround,
            spacing: 8,
            runSpacing: 12,
            children: [
              for (final k in _dynKnobs(kind, dyn))
                U4Knob(
                  label: k.label,
                  value: k.value,
                  min: k.min,
                  max: k.max,
                  text: k.text,
                  defaultValue: k.dflt,
                  color: color,
                  onInput: (v) {
                    writes.queue('${kind.name}_${k.prop}', v);
                    store.setDynParam(kind, k.prop, v);
                  },
                ),
            ],
          ),
        ),
        const SizedBox(height: 8),
        GestureDetector(
          onTap: () => store.resetDyn(kind),
          child: Container(
            padding: const EdgeInsets.symmetric(vertical: 14),
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: U4.tileBg,
              border: Border.all(color: U4.tileBorder),
              borderRadius: U4.rBtn,
            ),
            child: Text('Reset $label',
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: U4.text1)),
          ),
        ),
      ],
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
  final double dflt;
  const _KnobDef(this.label, this.prop, this.value, this.min, this.max,
      this.text, this.dflt);
}

List<_KnobDef> _dynKnobs(DynKind kind, ChannelDynVm d) {
  if (kind == DynKind.gate) {
    return [
      _KnobDef(
          'Thr', 'thr', d.thr, -80, 0, '${d.thr.toStringAsFixed(1)} dB', -50),
      _KnobDef('Range', 'range', d.range, 0, 60,
          '${d.range.toStringAsFixed(0)} dB', 40),
      _KnobDef(
          'Att', 'att', d.att, 0.1, 100, '${d.att.toStringAsFixed(1)} ms', 0.5),
      _KnobDef('Hold', 'hold', d.hold, 0, 500,
          '${d.hold.toStringAsFixed(0)} ms', 50),
      _KnobDef(
          'Rel', 'rel', d.rel, 10, 1000, '${d.rel.toStringAsFixed(0)} ms', 150),
    ];
  }
  return [
    _KnobDef(
        'Thr', 'thr', d.thr, -60, 0, '${d.thr.toStringAsFixed(1)} dB', -20),
    _KnobDef(
        'Ratio', 'ratio', d.ratio, 1, 20, '${d.ratio.toStringAsFixed(1)}:1', 3),
    _KnobDef(
        'Knee', 'knee', d.knee, 0, 6, '${d.knee.toStringAsFixed(1)} dB', 3),
    _KnobDef(
        'Att', 'att', d.att, 0.1, 100, '${d.att.toStringAsFixed(1)} ms', 10),
    _KnobDef(
        'Rel', 'rel', d.rel, 10, 1000, '${d.rel.toStringAsFixed(0)} ms', 120),
    _KnobDef('Makeup', 'makeup', d.makeup, 0, 24,
        '${d.makeup.toStringAsFixed(1)} dB', 0),
  ];
}

// ---------------------------------------------------------------------------
// Sends tab
// ---------------------------------------------------------------------------

class _SendsTab extends StatelessWidget {
  final ChannelStore store;
  const _SendsTab({required this.store});

  @override
  Widget build(BuildContext context) {
    final sends = store.sends;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        U4Section(
          title: 'Sends · ${sends.length}',
          child: sends.isEmpty
              ? const Text('No send slots on this desk.',
                  style: TextStyle(fontSize: 13, color: U4.text7))
              : Column(
                  children: [
                    for (final s in sends) ...[
                      U4SendRow(
                        send: s,
                        onLevel: (v) => store.setSendLevel(s.slot, v),
                        onMute: () => store.setSendMute(s.slot, !s.muted),
                        onTapCycle: () {
                          final opts = s.tapOptions;
                          if (opts.isEmpty) return;
                          final cur = s.tap ?? opts.first;
                          final next =
                              opts[(opts.indexOf(cur) + 1) % opts.length];
                          store.setSendTap(s.slot, next);
                        },
                      ),
                      const SizedBox(height: 6),
                    ],
                  ],
                ),
        ),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Groups tab: subgroup + mute-group assignment
// ---------------------------------------------------------------------------

class _GroupsTab extends StatelessWidget {
  final ChannelStore store;
  const _GroupsTab({required this.store});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        U4Section(
          title: 'Subgroups',
          child: store.subgroupCount == 0
              ? const Text('No subgroups on this desk.',
                  style: TextStyle(fontSize: 13, color: U4.text7))
              : Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    for (var g = 1; g <= store.subgroupCount; g++)
                      _AssignChip(
                        label: 'Sub $g',
                        assigned: store.subgroupAssigned(g),
                        onTap: () {
                          final cur = store.subgroupAssigned(g) ?? false;
                          store.cmdSubgroupAssign(g, !cur);
                        },
                      ),
                  ],
                ),
        ),
        const SizedBox(height: 8),
        U4Section(
          title: 'Mute groups',
          child: store.muteGroupCount == 0
              ? const Text('No mute groups on this desk.',
                  style: TextStyle(fontSize: 13, color: U4.text7))
              : Wrap(
                  spacing: 6,
                  runSpacing: 6,
                  children: [
                    for (var g = 1; g <= store.muteGroupCount; g++)
                      _AssignChip(
                        label: 'MG $g',
                        assigned: store.muteGroupAssigned(g),
                        onTap: () {
                          final cur = store.muteGroupAssigned(g) ?? false;
                          store.cmdMuteGroupAssign(g, !cur);
                        },
                      ),
                  ],
                ),
        ),
      ],
    );
  }
}

class _AssignChip extends StatelessWidget {
  final String label;
  final bool? assigned; // null = unknown until echo lands
  final VoidCallback onTap;
  const _AssignChip(
      {required this.label, required this.assigned, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final on = assigned ?? false;
    return GestureDetector(
      onTap: onTap,
      child: Opacity(
        opacity: assigned == null ? 0.55 : 1,
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          decoration: BoxDecoration(
            color: on ? U4.cyan.withValues(alpha: 0.22) : U4.tileBg,
            border: Border.all(color: on ? U4.cyan : U4.tileBorder),
            borderRadius: U4.rBtn,
          ),
          child: Text(label,
              textAlign: TextAlign.center,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w700,
                  color: on ? U4.text1 : U4.text7)),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Presets tab
// ---------------------------------------------------------------------------

class _PresetsTab extends StatelessWidget {
  final ChannelStore store;
  const _PresetsTab({required this.store});

  @override
  Widget build(BuildContext context) {
    return U4Section(
      title: 'Strip presets',
      child: store.presets.isEmpty
          ? const Text('No presets stored.',
              style: TextStyle(fontSize: 13, color: U4.text7))
          : Column(
              children: [
                for (final p in store.presets) ...[
                  GestureDetector(
                    onTap: () => store.recallPreset(p),
                    child: Container(
                      width: double.infinity,
                      padding: const EdgeInsets.symmetric(
                          horizontal: 12, vertical: 14),
                      decoration: BoxDecoration(
                        color: U4.tileBg,
                        border: Border.all(color: U4.tileBorder),
                        borderRadius: U4.rBtn,
                      ),
                      child: Text(p,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style:
                              const TextStyle(fontSize: 14, color: U4.text1)),
                    ),
                  ),
                  const SizedBox(height: 6),
                ],
              ],
            ),
    );
  }
}
