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
import '../settings/prefs.dart';
import '../theme.dart';
import '../widgets/channel_widgets.dart';
import '../widgets/number_entry.dart';

/// Channel tabs (web CHANNEL_TAB_ORDER + groups/source/presets sections).
/// New order: Input → Gate → Comp → EQ. Sends lives inside Input;
/// groups/presets left the tab bar (presets is a per-processor dropdown).
enum ChannelTab { input, gate, comp, eq }

/// Minimal store contract the channel screen renders. Implemented by the
/// shell/owner over the live engine or a fixture sim. Meters arrive via
/// stable [ValueNotifier]s updated in place by [pollMeters]; DSP snapshots
/// ([eq]/[gate]/[comp]/[sends]/...) notify through [ChangeNotifier].
abstract class ChannelStore extends ChangeNotifier {
  String get stripKey;
  String get displayName;

  /// 1-based channel number for the toolbar readout.
  String get channelNumber;

  /// Live fader notifier for the toolbar readout + typed entry.
  ValueNotifier<double> faderNote();

  /// Move this channel's fader to an absolute dB value.
  void setFaderDb(double db);

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

/// One focused numeric control: the page's top readout renders [text]
/// live and tapping it opens typed entry that flows back through [apply].
class ChannelFocus {
  final String title;
  final String label;
  final String Function() text;
  final double min;
  final double max;
  final ValueChanged<double> apply;

  const ChannelFocus(
      {required this.title,
      required this.label,
      required this.text,
      required this.min,
      required this.max,
      required this.apply});
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
  final _swipePointers = <int>{};
  bool _swipeMultiTouch = false;
  double _swipeDistance = 0;
  ChannelTab? _swipeTab;

  /// Focused numeric control; the slim bar under the tab rail renders it
  /// live and tapping opens typed entry. Cleared on tab switch (EQ
  /// re-focuses the selected band's frequency).
  final _focus = ValueNotifier<ChannelFocus?>(null);

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
    _focus.dispose();
    super.dispose();
  }

  void _switchTab(ChannelStore store, ChannelTab tab) {
    // EQ opens focused on the selected band's frequency (the top readout's
    // default subject); other tabs clear the focus bar.
    _focus.value = tab == ChannelTab.eq
        ? eqFreqFocus(store, _writes!, store.eq.selectedBand)
        : null;
    store.setTab(tab);
  }

  Widget _swipeNavigation(ChannelStore store, Widget child) {
    return Listener(
      onPointerDown: (event) {
        if (_swipePointers.isEmpty) _swipeMultiTouch = false;
        _swipePointers.add(event.pointer);
        if (_swipePointers.length > 1) _swipeMultiTouch = true;
      },
      onPointerUp: (event) => _swipePointers.remove(event.pointer),
      onPointerCancel: (event) {
        _swipePointers.remove(event.pointer);
        _swipeMultiTouch = true;
      },
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        // Directional recognition leaves vertical scrolling to the scroll
        // view. Descendant graph/slider recognizers take precedence over
        // this fallback, so editing a control cannot also change tabs.
        onHorizontalDragStart: (_) {
          _swipeDistance = 0;
          _swipeTab = store.tab;
        },
        onHorizontalDragUpdate: (details) {
          _swipeDistance += details.delta.dx;
        },
        onHorizontalDragEnd: (details) {
          final start = _swipeTab;
          _swipeTab = null;
          if (_swipeMultiTouch || start == null || store.tab != start) return;
          final velocity = details.primaryVelocity ?? 0;
          if (_swipeDistance.abs() < 56 &&
              !(_swipeDistance.abs() >= 24 &&
                  velocity.abs() >= 450 &&
                  velocity.sign == _swipeDistance.sign)) {
            return;
          }
          final index = _tabOrder.indexOf(start);
          final next = index + (_swipeDistance < 0 ? 1 : -1);
          if (next >= 0 && next < _tabOrder.length) {
            _switchTab(store, _tabOrder[next]);
          }
        },
        onHorizontalDragCancel: () => _swipeTab = null,
        child: child,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable:
              Listenable.merge([widget.store, PrefsService.instance]),
          builder: (context, _) {
            final store = widget.store;
            // The toolbar lives in the shell header now; this page owns
            // the focus readout + processing navigation + content.
            final wide = MediaQuery.sizeOf(context).width >= 700;
            final content = SingleChildScrollView(
              padding: const EdgeInsets.all(8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  switch (store.tab) {
                    ChannelTab.input => _InputTab(
                        store: store,
                        writes: _writes!,
                        onFocus: (f) => _focus.value = f),
                    ChannelTab.eq => _EqTab(
                        store: store,
                        writes: _writes!,
                        onFocus: (f) => _focus.value = f),
                    ChannelTab.gate => _DynTab(
                        store: store,
                        writes: _writes!,
                        onFocus: (f) => _focus.value = f,
                        kind: DynKind.gate),
                    ChannelTab.comp => _DynTab(
                        store: store,
                        writes: _writes!,
                        onFocus: (f) => _focus.value = f,
                        kind: DynKind.comp),
                  },
                ],
              ),
            );
            final swipeContent = _swipeNavigation(store, content);
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _FocusBar(focus: _focus),
                Expanded(
                  child: wide
                      ? Row(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Padding(
                              padding: const EdgeInsets.fromLTRB(8, 0, 0, 8),
                              child: _SideMenu(
                                  store: store,
                                  tab: store.tab,
                                  onTab: (t) => _switchTab(store, t)),
                            ),
                            const SizedBox(width: 8),
                            Expanded(child: swipeContent),
                          ],
                        )
                      : swipeContent,
                ),
                if (!wide)
                  _BottomNav(
                      store: store,
                      tab: store.tab,
                      onTab: (t) => _switchTab(store, t)),
              ],
            );
          },
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Focus readout bar: slim live row under the tab rail. Tapping opens typed
// entry for the focused control.
// ---------------------------------------------------------------------------

class _FocusBar extends StatelessWidget {
  final ValueNotifier<ChannelFocus?> focus;
  const _FocusBar({required this.focus});

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<ChannelFocus?>(
      valueListenable: focus,
      builder: (context, f, _) {
        if (f == null) return const SizedBox.shrink();
        return GestureDetector(
          onTap: () async {
            final v = await showNumberEntry(
                context: context,
                title: '${f.title} · ${f.label}',
                initial: f.text(),
                min: f.min,
                max: f.max);
            if (v != null) f.apply(v);
          },
          behavior: HitTestBehavior.opaque,
          child: Container(
            padding:
                const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            decoration: const BoxDecoration(
              border:
                  Border(bottom: BorderSide(color: U4.lineHeader)),
            ),
            child: Row(children: [
              Expanded(
                  child: Text('${f.title} · ${f.label}',
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                          fontFamily: U4.font,
                          fontSize: 13,
                          color: U4.text7))),
              Text(f.text(),
                  maxLines: 1,
                  style: const TextStyle(
                      fontFamily: U4.font,
                      fontSize: 17,
                      fontWeight: FontWeight.w700,
                      color: U4.cyan,
                      fontFeatures: [FontFeature.tabularFigures()])),
              const SizedBox(width: 6),
              const Icon(Icons.edit_outlined,
                  size: 16, color: U4.text7),
            ]),
          ),
        );
      },
    );
  }
}

// ---------------------------------------------------------------------------
// Processing navigation: tablet side menu with live previews, phone flush
// bottom bar. Order is Input → Gate → Comp → EQ.
const _tabOrder = [
  ChannelTab.input,
  ChannelTab.gate,
  ChannelTab.comp,
  ChannelTab.eq,
];

const _tabLabels = {
  ChannelTab.input: 'Input',
  ChannelTab.gate: 'Gate',
  ChannelTab.comp: 'Comp',
  ChannelTab.eq: 'EQ',
};

bool _tabOff(ChannelStore store, ChannelTab tab) => switch (tab) {
      ChannelTab.input => false,
      ChannelTab.gate => !store.gate.on,
      ChannelTab.comp => !store.comp.on,
      ChannelTab.eq => !store.eq.on,
    };

class _SideMenu extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  final ValueChanged<ChannelTab> onTab;
  const _SideMenu(
      {required this.store, required this.tab, required this.onTab});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 178,
      padding: const EdgeInsets.all(6),
      decoration: const BoxDecoration(
        color: U4.panel,
        borderRadius: BorderRadius.all(Radius.circular(18)),
        border: Border.fromBorderSide(BorderSide(color: U4.lineHeader)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final t in _tabOrder)
            Expanded(
                child: _MenuRow(
                    store: store,
                    tab: t,
                    active: t == tab,
                    onTap: () => onTab(t))),
        ],
      ),
    );
  }
}

class _MenuRow extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  final bool active;
  final VoidCallback onTap;
  const _MenuRow(
      {required this.store,
      required this.tab,
      required this.active,
      required this.onTap});

  @override
  Widget build(BuildContext context) {
    final off = _tabOff(store, tab);
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Semantics(
        button: true,
        selected: active,
        label: '${_tabLabels[tab]} processing',
        onTap: onTap,
        child: GestureDetector(
          onTap: onTap,
          child: Container(
            padding: const EdgeInsets.fromLTRB(12, 12, 12, 10),
            decoration: BoxDecoration(
              color: active ? const Color(0xff262b14) : U4.tileBg,
              borderRadius: U4.rBtn,
              border: Border.all(
                  color: active
                      ? U4.cyan.withValues(alpha: 0.27)
                      : U4.tileBorder),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(children: [
                  Expanded(
                      child: Text(_tabLabels[tab]!,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontFamily: U4.font,
                              fontSize: 24,
                              fontWeight: FontWeight.w600,
                              color: active
                                  ? U4.cyan
                                  : off
                                      ? U4.text7
                                      : const Color(0xffc9c9c9)))),
                  if (off)
                    Text('OFF',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 1.1,
                            color: U4.text7)),
                ]),
                const SizedBox(height: 8),
                Expanded(
                    child: Opacity(
                        opacity: off ? 0.35 : 1,
                        child: _NavPreview(store: store, tab: tab))),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Live processing preview thumbnails (interaction disabled).
class _NavPreview extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  const _NavPreview({required this.store, required this.tab});

  @override
  Widget build(BuildContext context) {
    switch (tab) {
      case ChannelTab.input:
        final gain = (store.paramNum('gain') ?? store.preamp.gain)
            .clamp(0.0, 60.0)
            .toDouble();
        final phantom = store.preamp.phantom;
        return Container(
          padding: const EdgeInsets.all(6),
          decoration: BoxDecoration(
              color: U4.panel, borderRadius: BorderRadius.circular(8)),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Container(
                  height: 12,
                  decoration: BoxDecoration(
                      color: const Color(0xff252525),
                      borderRadius: BorderRadius.circular(3),
                      border: Border.all(
                          color: const Color(0xff474747), width: 1.5)),
                  child: FractionallySizedBox(
                      alignment: Alignment.centerLeft,
                      widthFactor: (gain / 60).clamp(0.05, 1.0),
                      child: Container(
                          decoration: BoxDecoration(
                              color: const Color(0xff4f5a16),
                              border: Border.all(color: U4.cyan))))),
              const SizedBox(height: 4),
              Text('48V',
                  textAlign: TextAlign.right,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      color: phantom ? const Color(0xffef5548) : U4.text10)),
            ],
          ),
        );
      case ChannelTab.gate:
      case ChannelTab.comp:
        final kind = tab == ChannelTab.gate ? DynKind.gate : DynKind.comp;
        final dyn = kind == DynKind.gate ? store.gate : store.comp;
        final color =
            kind == DynKind.gate ? U4.cyan : const Color(0xffa285d6);
        return ValueListenableBuilder<double>(
            valueListenable: store.levelDb,
            builder: (context, db, _) => IgnorePointer(
                child: U4DynGraph(
                    dyn: dyn,
                    levelDb: db,
                    color: color,
                    onThreshold: (_) {})));
      case ChannelTab.eq:
        final eq = store.eq;
        return ValueListenableBuilder<List<double>>(
            valueListenable: store.spectrum,
            builder: (context, bins, _) => IgnorePointer(
                child: Container(
                    padding: const EdgeInsets.all(4),
                    decoration: BoxDecoration(
                        color: const Color(0xff1f2314),
                        borderRadius: BorderRadius.circular(8)),
                    child: U4EqGraph(
                        eq: eq,
                        responseDb: eqResponseDb(eq),
                        rtaBins: bins,
                        rtaActive: false,
                        onSelectBand: (_) {},
                        onNode: (_, __, ___) {},
                        onQ: (_, __) {}))));
    }
  }
}

/// Phone flush bottom bar: four cells with mini previews.
class _BottomNav extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  final ValueChanged<ChannelTab> onTab;
  const _BottomNav(
      {required this.store, required this.tab, required this.onTab});

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 108,
      decoration: const BoxDecoration(
        color: U4.panel,
        border: Border(top: BorderSide(color: Color(0xff363636))),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (var i = 0; i < _tabOrder.length; i++) ...[
            Expanded(child: _BottomCell(
                store: store,
                tab: _tabOrder[i],
                active: _tabOrder[i] == tab,
                onTap: () => onTab(_tabOrder[i]))),
            if (i < _tabOrder.length - 1)
              Container(width: 1, color: const Color(0xff292929)),
          ],
        ],
      ),
    );
  }
}

class _BottomCell extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  final bool active;
  final VoidCallback onTap;
  const _BottomCell(
      {required this.store,
      required this.tab,
      required this.active,
      required this.onTap});

  @override
  Widget build(BuildContext context) {
    final off = _tabOff(store, tab);
    return Semantics(
      button: true,
      selected: active,
      label: '${_tabLabels[tab]} processing',
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        child: Container(
          color: active ? const Color(0xff262b14) : U4.tileBg,
          padding:
              const EdgeInsets.fromLTRB(7, 9, 7, 5),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              SizedBox(
                  height: 56,
                  child: Opacity(
                      opacity: off ? 0.35 : 1,
                      child: _NavPreview(store: store, tab: tab))),
              const SizedBox(height: 4),
              Text(_tabLabels[tab]!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: active
                          ? U4.cyan
                          : off
                              ? U4.text7
                              : const Color(0xffb5b5b5))),
            ],
          ),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Focus builders: live text reads the store at render time (closures over
// snapshots would go stale); apply mirrors the knob's write path.
// ---------------------------------------------------------------------------

String _eqFreqText(ChannelStore store, int band) {
  if (band == 0) return '${store.eq.hpfF.toStringAsFixed(0)} Hz';
  final bands = store.eq.bands;
  final b = bands.firstWhere((x) => x.band == band,
      orElse: () => bands.first);
  final f = b.f;
  return f >= 1000
      ? '${(f / 1000).toStringAsFixed(1)}k'
      : '${f.toStringAsFixed(0)} Hz';
}

/// Frequency focus for an EQ band (0 = HPF). Used when a band is pressed
/// on the graph/dots and when switching to the EQ tab.
ChannelFocus eqFreqFocus(
    ChannelStore store, ChannelWrites writes, int band) {
  final b = band.clamp(0, 4);
  if (b == 0) {
    return ChannelFocus(
        title: 'EQ HPF',
        label: 'Frequency',
        text: () => _eqFreqText(store, 0),
        min: 20,
        max: 1000,
        apply: (v) {
          writes.queue('eq_hpf_f', v);
          store.setEqParam(0, 'f', v);
        });
  }
  return ChannelFocus(
      title: 'EQ Band $b',
      label: 'Frequency',
      text: () => _eqFreqText(store, b),
      min: b == 1 ? 20 : 100,
      max: b == 4 ? 20000 : 16000,
      apply: (v) {
        writes.queue('eq${b}_f', v);
        store.setEqParam(b, 'f', v);
      });
}

ChannelFocus _eqGainFocus(
    ChannelStore store, ChannelWrites writes, int band) {
  return ChannelFocus(
      title: 'EQ Band $band',
      label: 'Gain',
      text: () {
        final bands = store.eq.bands;
        final b = bands.firstWhere((x) => x.band == band,
            orElse: () => bands.first);
        return '${b.g.toStringAsFixed(1)} dB';
      },
      min: -15,
      max: 15,
      apply: (v) {
        writes.queue('eq${band}_g', v);
        store.setEqParam(band, 'g', v);
      });
}

ChannelFocus _eqQFocus(
    ChannelStore store, ChannelWrites writes, int band) {
  return ChannelFocus(
      title: 'EQ Band $band',
      label: 'Q',
      text: () {
        final bands = store.eq.bands;
        final b = bands.firstWhere((x) => x.band == band,
            orElse: () => bands.first);
        return b.q.toStringAsFixed(2);
      },
      min: 0.3,
      max: 16,
      apply: (v) {
        writes.queue('eq${band}_q', v);
        store.setEqParam(band, 'q', v);
      });
}

String _dynLiveText(ChannelStore store, DynKind kind, String prop) {
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

// ---------------------------------------------------------------------------
// Input tab: preamp + source
// ---------------------------------------------------------------------------

class _InputTab extends StatelessWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  final ValueChanged<ChannelFocus> onFocus;
  const _InputTab(
      {required this.store, required this.writes, required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final pre = store.preamp;
    final src = store.source;
    final liveGain = store.paramNum('gain');
    final liveTrim = store.paramNum('trim');
    final left = Column(
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
                    onFocus: () => onFocus(ChannelFocus(
                        title: 'Preamp',
                        label: 'Gain',
                        text: () {
                          final live = store.paramNum('gain');
                          final v = live ?? store.preamp.gain;
                          return '${v.toStringAsFixed(1)} dB';
                        },
                        min: 0,
                        max: 60,
                        apply: (v) {
                          writes.queue('gain', v);
                          store.setPreamp('gain', v);
                        })),
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
                    onFocus: () => onFocus(ChannelFocus(
                        title: 'Preamp',
                        label: 'Trim',
                        text: () {
                          final live = store.paramNum('trim');
                          final v = live ?? store.preamp.trim;
                          return '${v.toStringAsFixed(1)} dB';
                        },
                        min: -18,
                        max: 18,
                        apply: (v) {
                          writes.queue('trim', v);
                          store.setPreamp('trim', v);
                        })),
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
                  // 48V is gray text when off, red when on — no box.
                  Expanded(
                    child: _TextSwitch(
                      label: pre.phantomArmed
                          ? (pre.phantom
                              ? 'Tap again: 48V off'
                              : 'Tap again: 48V on')
                          : (pre.phantom ? '48V On' : '48V Off'),
                      active: pre.phantom,
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
        const SizedBox(height: 8),
        _AssignBlock(store: store),
      ],
    );
    final sends = _SendsBlock(store: store);
    // Tablet: input panel beside the sends panel; phones stack them.
    return LayoutBuilder(builder: (context, box) {
      if (box.maxWidth >= 700) {
        return Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(flex: 538, child: left),
            const SizedBox(width: 12),
            Expanded(flex: 806, child: sends),
          ],
        );
      }
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [left, sends],
      );
    });
  }
}

/// Gray/red text switch (48V rule): no box, 44pt target.
class _TextSwitch extends StatelessWidget {
  final String label;
  final bool active;
  final bool enabled;
  final VoidCallback onTap;
  const _TextSwitch(
      {required this.label,
      required this.active,
      required this.enabled,
      required this.onTap});

  @override
  Widget build(BuildContext context) => Opacity(
        opacity: enabled ? 1 : 0.45,
        child: GestureDetector(
          onTap: enabled ? onTap : null,
          behavior: HitTestBehavior.opaque,
          child: SizedBox(
            height: 44,
            child: Center(
              child: Text(label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 14,
                      fontWeight: FontWeight.w700,
                      color: active
                          ? const Color(0xffef5548)
                          : const Color(0xff7a7a7a))),
            ),
          ),
        ),
      );
}

// ---------------------------------------------------------------------------
// Sends block (inside Input): one row per send slot.
// ---------------------------------------------------------------------------

class _SendsBlock extends StatelessWidget {
  final ChannelStore store;
  const _SendsBlock({required this.store});

  @override
  Widget build(BuildContext context) {
    final sends = store.sends;
    return U4Section(
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
                      final next = opts[(opts.indexOf(cur) + 1) % opts.length];
                      store.setSendTap(s.slot, next);
                    },
                  ),
                  const SizedBox(height: 6),
                ],
              ],
            ),
    );
  }
}

// ---------------------------------------------------------------------------
// Assignments block (inside Input): subgroup + mute-group assignment.
// ---------------------------------------------------------------------------

class _AssignBlock extends StatelessWidget {
  final ChannelStore store;
  const _AssignBlock({required this.store});

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

// ---------------------------------------------------------------------------
// Shared boxed processor header: title + power toggle. ON is the lime
// boxed style; bypassed is charcoal with gray text.
// ---------------------------------------------------------------------------

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
    final button = Semantics(
      button: true,
      label: '$title ${on ? 'on' : 'bypassed'}, activate to ${on ? 'bypass' : 'enable'}',
      onTap: onToggle,
      child: GestureDetector(
        onTap: onToggle,
        behavior: HitTestBehavior.opaque,
        child: Container(
          height: compact ? 44 : 56,
          padding: EdgeInsets.symmetric(horizontal: compact ? 12 : 16),
          decoration: BoxDecoration(
            color: on ? const Color(0xff262b14) : const Color(0xff343434),
            borderRadius: BorderRadius.circular(12),
            border: Border.all(
                color: on
                    ? U4.cyan.withValues(alpha: 0.27)
                    : const Color(0xff414141)),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: compact ? 20 : 28,
                      fontWeight: FontWeight.w600,
                      color: on ? U4.cyan : const Color(0xffaaa9a9))),
              Icon(Icons.power_settings_new,
                  size: compact ? 22 : 24,
                  color: on ? U4.cyan : const Color(0xffaaa9a9)),
            ],
          ),
        ),
      ),
    );
    if (compact) return button;
    return Align(
        alignment: Alignment.centerLeft,
        child: SizedBox(width: 260, child: button));
  }
}

/// Presets dropdown for the settings row (recall only; no new protocol).
class _PresetsButton extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  const _PresetsButton({required this.store, required this.compact});

  @override
  Widget build(BuildContext context) {
    return TextButton.icon(
      onPressed: () async {
        final box = context.findRenderObject() as RenderBox?;
        final overlay =
            Overlay.of(context).context.findRenderObject() as RenderBox?;
        final choice = await showMenu<String>(
          context: context,
          color: const Color(0xff262626),
          position: RelativeRect.fromRect(
              box == null
                  ? Rect.zero
                  : Offset.zero & box.size,
              Offset.zero & (overlay?.size ?? Size.zero)),
          items: [
            if (store.presets.isEmpty)
              const PopupMenuItem(
                  enabled: false, child: Text('No presets stored')),
            for (final p in store.presets)
              PopupMenuItem(
                  value: p,
                  child: Text(p,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: const TextStyle(
                          fontFamily: U4.font, color: U4.text1))),
          ],
        );
        if (choice != null) store.recallPreset(choice);
      },
      style: TextButton.styleFrom(
          foregroundColor: U4.text1,
          backgroundColor: const Color(0xff343434),
          minimumSize: Size(64, compact ? 44 : 56),
          padding:
              EdgeInsets.symmetric(horizontal: compact ? 12 : 18),
          shape: RoundedRectangleBorder(
              borderRadius: BorderRadius.circular(12)),
          textStyle: TextStyle(
              fontFamily: U4.font,
              fontSize: compact ? 14 : 22,
              fontWeight: FontWeight.w500)),
      icon: const Text('Presets'),
      label: const Icon(Icons.arrow_drop_down, size: 20),
    );
  }
}

/// Compressor knee segmented control: Soft (6) / Hard (0).
class _KneeSeg extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  const _KneeSeg({required this.store, required this.compact});

  @override
  Widget build(BuildContext context) {
    final knee = store.comp.knee;
    final soft = knee >= 3;
    Widget seg(String label, bool active, VoidCallback onTap) =>
        Expanded(
            child: GestureDetector(
                onTap: onTap,
                behavior: HitTestBehavior.opaque,
                child: Container(
                    height: compact ? 36 : 44,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                        color: active
                            ? const Color(0xff262b14)
                            : Colors.transparent,
                        borderRadius: BorderRadius.circular(9),
                        border: active
                            ? Border.all(
                                color: U4.cyan.withValues(alpha: 0.33))
                            : null),
                    child: Text(label,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: compact ? 14 : 22,
                            fontWeight: FontWeight.w500,
                            color: active
                                ? U4.cyan
                                : U4.text7)))));
    void setKnee(double v) => store.setDynParam(DynKind.comp, 'knee', v);
    return Expanded(
        child: Container(
            padding: const EdgeInsets.all(4),
            decoration: BoxDecoration(
                color: const Color(0xff262626),
                borderRadius: BorderRadius.circular(12)),
            child: Row(children: [
              seg('Soft', soft, () => setKnee(6)),
              const SizedBox(width: 4),
              seg('Hard', !soft, () => setKnee(0)),
            ])));
  }
}

// ---------------------------------------------------------------------------
// EQ tab: curve + RTA + HPF + band cards
// ---------------------------------------------------------------------------

class _EqTab extends StatefulWidget {
  final ChannelStore store;
  final ChannelWrites writes;
  final ValueChanged<ChannelFocus> onFocus;
  const _EqTab(
      {required this.store, required this.writes, required this.onFocus});

  @override
  State<_EqTab> createState() => _EqTabState();
}

class _EqTabState extends State<_EqTab> {
  /// Local display toggle for the RTA overlay (no desk state involved).
  bool _showRta = true;

  void _pick(int band) {
    widget.store.selectEqBand(band);
    widget.onFocus(eqFreqFocus(widget.store, widget.writes, band));
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final writes = widget.writes;
    final onFocus = widget.onFocus;
    final eq = store.eq;
    final response = eqResponseDb(eq);
    final compact = MediaQuery.sizeOf(context).width < 600;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ProcessorHeader(
            title: 'EQ',
            on: eq.on,
            compact: compact,
            onToggle: () => store.setEqOn(!eq.on)),
        const SizedBox(height: 8),
        Row(children: [
          Expanded(child: _PresetsButton(store: store, compact: compact)),
          const SizedBox(width: 8),
          Expanded(
              child: GestureDetector(
                  onTap: () => setState(() => _showRta = !_showRta),
                  behavior: HitTestBehavior.opaque,
                  child: Container(
                      height: compact ? 44 : 56,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: _showRta
                              ? const Color(0xff262b14)
                              : const Color(0xff343434),
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(
                              color: _showRta
                                  ? U4.cyan.withValues(alpha: 0.27)
                                  : const Color(0xff414141))),
                      child: Text('RTA',
                          style: TextStyle(
                              fontFamily: U4.font,
                              fontSize: compact ? 14 : 22,
                              fontWeight: FontWeight.w500,
                              color: _showRta
                                  ? U4.cyan
                                  : U4.text1))))),
        ]),
        const SizedBox(height: 8),
        Opacity(
          opacity: eq.on ? 1 : 0.55,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              ValueListenableBuilder<List<double>>(
                valueListenable: store.spectrum,
                builder: (context, bins, _) {
                  final active =
                      _showRta && bins.any((b) => b > 0.02);
                  return U4EqGraph(
                    eq: eq,
                    responseDb: response,
                    rtaBins: bins,
                    rtaActive: active,
                    onSelectBand: _pick,
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
              _BandDots(eq: eq, onPick: _pick),
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
                  onFocus: () =>
                      onFocus(eqFreqFocus(store, writes, 0)),
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
                  onFocus: () =>
                      onFocus(eqFreqFocus(store, writes, b.band)),
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
                  onFocus: () =>
                      onFocus(_eqGainFocus(store, writes, b.band)),
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
                  onFocus: () =>
                      onFocus(_eqQFocus(store, writes, b.band)),
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
          Semantics(
            button: true,
            selected: eq.selectedBand == b,
            label: b == 0 ? 'High-pass filter band' : 'EQ band $b',
            onTap: () => onPick(b),
            child: GestureDetector(
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
  final ValueChanged<ChannelFocus> onFocus;
  const _DynTab(
      {required this.store,
      required this.writes,
      required this.kind,
      required this.onFocus});

  @override
  Widget build(BuildContext context) {
    final dyn = kind == DynKind.gate ? store.gate : store.comp;
    final color =
        kind == DynKind.gate ? U4.cyan : const Color(0xffa285d6);
    final label = kind == DynKind.gate ? 'Gate' : 'Comp';
    final compact = MediaQuery.sizeOf(context).width < 600;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _ProcessorHeader(
            title: label,
            on: dyn.on,
            compact: compact,
            onToggle: () => store.setDynOn(kind, !dyn.on)),
        const SizedBox(height: 8),
        Row(children: [
          Expanded(child: _PresetsButton(store: store, compact: compact)),
          if (kind == DynKind.comp) ...[
            const SizedBox(width: 8),
            _KneeSeg(store: store, compact: compact),
          ],
        ]),
        const SizedBox(height: 8),
        Opacity(
          opacity: dyn.on ? 1 : 0.55,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
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
                  onFocus: () => onFocus(ChannelFocus(
                      title: label,
                      label: k.label,
                      text: () => _dynLiveText(store, kind, k.prop),
                      min: k.min,
                      max: k.max,
                      apply: (v) {
                        writes.queue('${kind.name}_${k.prop}', v);
                        store.setDynParam(kind, k.prop, v);
                      })),
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
// (Sends/groups/presets left the tab bar: sends + assignments live inside
// Input above; presets is a per-processor dropdown in the settings row.)
// ---------------------------------------------------------------------------

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
