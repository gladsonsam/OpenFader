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
import '../widgets/mobile_reference_width.dart';
import '../palette.dart';
import '../design.dart';
import '../u4law.dart';

import '../widgets/channel_widgets.dart';
import '../widgets/number_entry.dart';

part 'processing_input.dart';
part 'processing_eq.dart';
part 'processing_workspace.dart';
part 'processing_dynamics.dart';
part 'processing_mobile.dart';

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

  /// Catalogue reads do not imply that a live value is hydrated/editable.
  bool get capabilitiesReady;
  Map<String, dynamic>? paramMetadata(String leaf);

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
  void setSourceSelected(int selected);
  void setSendPan(int slot, double pan);

  /// Stable per-channel/send feed for `level` (dB) or `pan` (-100..100).
  /// Null means no value has arrived. The store owns the notifier.
  ValueNotifier<double?> sendParamNote(int slot, String param);
  bool? get mainAssigned;
  void setMainAssigned(bool on);
  int get dcaCount;
  bool? dcaAssigned(int group);
  void setDcaAssigned(int group, bool on);
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
  void setCompSoftKnee(bool soft);
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

  /// Shell-owned mixer widgets preserve the actual strip/bank state. The
  /// processing page only places them; it never creates a second mixer store.
  final Widget? mixerControls;
  final ValueNotifier<ChannelFocus?>? controlFocus;

  const ChannelPage(
      {super.key,
      required this.stripKey,
      required this.store,
      this.mixerControls,
      this.controlFocus});

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
  late final _focus = widget.controlFocus ?? ValueNotifier<ChannelFocus?>(null);

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
    if (widget.controlFocus == null) _focus.dispose();
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
      resizeToAvoidBottomInset: false,
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        bottom: MediaQuery.sizeOf(context).width >= 700,
        child: ListenableBuilder(
          listenable: Listenable.merge([widget.store, PrefsService.instance]),
          builder: (context, _) {
            final store = widget.store;
            // The toolbar lives in the shell header now; this page owns
            // the focus readout + processing navigation + content.
            final wide = MediaQuery.sizeOf(context).width >= 700;
            final tabContent = switch (store.tab) {
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
            };
            final content = SingleChildScrollView(
              key: PageStorageKey('processing-${store.tab.name}'),
              child: wide
                  ? Padding(
                      padding: const EdgeInsets.all(12), child: tabContent)
                  : MobileReferenceWidth(
                      referenceWidth: 362,
                      child: Padding(
                          padding: const EdgeInsets.fromLTRB(8, 12, 8, 20),
                          child: tabContent)),
            );
            return LayoutBuilder(builder: (context, bounds) {
              // HTML phone: 362 / 466 processing, 104 / 466 selected strip.
              // Tablet: selected-channel strip and mix/bank rail stay persistent.
              final stripWidth = wide
                  ? bounds.maxWidth * (192.2 + 93) / 1823
                  : bounds.maxWidth * 104 / 466;
              return Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (wide)
                      Padding(
                          padding: const EdgeInsets.fromLTRB(8, 12, 8, 12),
                          child: SizedBox(
                              width: bounds.maxWidth * 178 / 1823,
                              child: _SideMenu(
                                  store: store,
                                  tab: store.tab,
                                  onTab: (t) => _switchTab(store, t)))),
                    Expanded(
                        child: Column(children: [
                      Expanded(child: _swipeNavigation(store, content)),
                      if (!wide)
                        _BottomNav(
                            store: store,
                            tab: store.tab,
                            onTab: (t) => _switchTab(store, t)),
                    ])),
                    SizedBox(width: stripWidth, child: widget.mixerControls),
                  ]);
            });
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
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        borderRadius: BorderRadius.all(Radius.circular(18)),
        border: Border.fromBorderSide(
            BorderSide(color: ConsolePalette.of(context).lineHeader)),
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
              color: active
                  ? ConsolePalette.of(context).selectionFill
                  : ConsolePalette.of(context).tileBg,
              borderRadius: U4.rBtn,
              border: Border.all(
                  color: active
                      ? ConsolePalette.of(context).cyan.withValues(alpha: 0.27)
                      : ConsolePalette.of(context).tileBorder),
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
                                  ? ConsolePalette.of(context).cyan
                                  : off
                                      ? ConsolePalette.of(context).text7
                                      : ConsolePalette.of(context).text4))),
                  if (off)
                    Text('OFF',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 1.1,
                            color: ConsolePalette.of(context).text7)),
                ]),
                const SizedBox(height: 8),
                Expanded(
                    child: Opacity(
                        opacity: off && !ConsolePalette.of(context).isLight
                            ? 0.35
                            : 1,
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
              color: ConsolePalette.of(context).panel,
              borderRadius: BorderRadius.circular(8)),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Container(
                  height: 12,
                  decoration: BoxDecoration(
                      color: ConsolePalette.of(context).chip,
                      borderRadius: BorderRadius.circular(3),
                      border: Border.all(
                          color: ConsolePalette.of(context).lineCtl,
                          width: 1.5)),
                  child: FractionallySizedBox(
                      alignment: Alignment.centerLeft,
                      widthFactor: (gain / 60).clamp(0.05, 1.0),
                      child: Container(
                          decoration: BoxDecoration(
                              color: ConsolePalette.of(context).selectionFill,
                              border: Border.all(
                                  color: ConsolePalette.of(context).cyan))))),
              const SizedBox(height: 4),
              Text('48V',
                  textAlign: TextAlign.right,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      color: phantom
                          ? ConsolePalette.of(context).phantom
                          : ConsolePalette.of(context).text10)),
            ],
          ),
        );
      case ChannelTab.gate:
      case ChannelTab.comp:
        final kind = tab == ChannelTab.gate ? DynKind.gate : DynKind.comp;
        final dyn = kind == DynKind.gate ? store.gate : store.comp;
        final color = kind == DynKind.gate
            ? ConsolePalette.of(context).cyan
            : ConsolePalette.of(context).processingPurple;
        return ValueListenableBuilder<double>(
            valueListenable: store.levelDb,
            builder: (context, db, _) => IgnorePointer(
                child: U4DynGraph(
                    dyn: dyn, levelDb: db, color: color, onThreshold: (_) {})));
      case ChannelTab.eq:
        return SizedBox.expand(
            child: CustomPaint(
                painter:
                    _EqPreviewPainter(store.eq, ConsolePalette.of(context))));
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
    return MobileReferenceWidth(
        referenceWidth: 362,
        child: Container(
          height: 112,
          decoration: BoxDecoration(
            color: ConsolePalette.of(context).panel,
            border:
                Border(top: BorderSide(color: ConsolePalette.of(context).line)),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (var i = 0; i < _tabOrder.length; i++) ...[
                Expanded(
                    child: _BottomCell(
                        store: store,
                        tab: _tabOrder[i],
                        active: _tabOrder[i] == tab,
                        onTap: () => onTab(_tabOrder[i]))),
                if (i < _tabOrder.length - 1)
                  Container(width: 1, color: ConsolePalette.of(context).line),
              ],
            ],
          ),
        ));
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
          color: active
              ? ConsolePalette.of(context).processingSelected
              : ConsolePalette.of(context).navigationTile,
          padding: const EdgeInsets.fromLTRB(7, 9, 7, 5),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(_tabLabels[tab]!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 15,
                      fontWeight: FontWeight.w600,
                      color: active
                          ? ConsolePalette.of(context).cyan
                          : off
                              ? ConsolePalette.of(context).text7
                              : ConsolePalette.of(context).mutedText)),
              const SizedBox(height: 7),
              Expanded(
                  child: Opacity(
                      opacity:
                          off && !ConsolePalette.of(context).isLight ? 0.35 : 1,
                      child: _PhoneNavPreview(
                          store: store, tab: tab, active: active))),
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
  final b = bands.firstWhere((x) => x.band == band, orElse: () => bands.first);
  final f = b.f;
  return f >= 1000
      ? '${(f / 1000).toStringAsFixed(1)}k'
      : '${f.toStringAsFixed(0)} Hz';
}

/// Frequency focus for an EQ band (0 = HPF). Used when a band is pressed
/// on the graph/dots and when switching to the EQ tab.
ChannelFocus eqFreqFocus(ChannelStore store, ChannelWrites writes, int band) {
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

ChannelFocus _eqGainFocus(ChannelStore store, ChannelWrites writes, int band) {
  return ChannelFocus(
      title: 'EQ Band $band',
      label: 'Gain',
      text: () {
        final bands = store.eq.bands;
        final b =
            bands.firstWhere((x) => x.band == band, orElse: () => bands.first);
        return '${b.g.toStringAsFixed(1)} dB';
      },
      min: -15,
      max: 15,
      apply: (v) {
        writes.queue('eq${band}_g', v);
        store.setEqParam(band, 'g', v);
      });
}

ChannelFocus _eqQFocus(ChannelStore store, ChannelWrites writes, int band) {
  return ChannelFocus(
      title: 'EQ Band $band',
      label: 'Q',
      text: () {
        final bands = store.eq.bands;
        final b =
            bands.firstWhere((x) => x.band == band, orElse: () => bands.first);
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
                          ? ConsolePalette.of(context).phantom
                          : ConsolePalette.of(context).text10)),
            ),
          ),
        ),
      );
}

// ---------------------------------------------------------------------------
// Sends block (inside Input): one row per send slot.
// ---------------------------------------------------------------------------

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

/// Presets dropdown for the settings row (recall only; no new protocol).
class _PresetsButton extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  const _PresetsButton({required this.store, required this.compact});

  @override
  Widget build(BuildContext context) {
    return TextButton(
      onPressed: () async {
        final box = context.findRenderObject() as RenderBox?;
        final overlay =
            Overlay.of(context).context.findRenderObject() as RenderBox?;
        if (box == null || overlay == null) return;
        final choice = await showMenu<String>(
          context: context,
          color: ConsolePalette.of(context).chip,
          position: RelativeRect.fromRect(
              Rect.fromPoints(
                  box.localToGlobal(Offset.zero, ancestor: overlay),
                  box.localToGlobal(box.size.bottomRight(Offset.zero),
                      ancestor: overlay)),
              Offset.zero & overlay.size),
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
                      style: TextStyle(
                          fontFamily: U4.font,
                          color: ConsolePalette.of(context).text1))),
          ],
        );
        if (choice != null) store.recallPreset(choice);
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
  final double dflt;
  const _KnobDef(this.label, this.prop, this.value, this.min, this.max,
      this.text, this.dflt);
}

List<_KnobDef> _dynKnobs(DynKind kind, ChannelDynVm d) {
  if (kind == DynKind.gate) {
    return [
      _KnobDef('Threshold', 'thr', d.thr, -80, 0,
          '${d.thr.toStringAsFixed(1)} dB', -50),
      _KnobDef('Range', 'range', d.range, 0, 60,
          '${d.range.toStringAsFixed(0)} dB', 40),
      _KnobDef('Attack', 'att', d.att, 0.1, 100,
          '${d.att.toStringAsFixed(1)} ms', 0.5),
      _KnobDef('Hold', 'hold', d.hold, 0, 500,
          '${d.hold.toStringAsFixed(0)} ms', 50),
      _KnobDef('Release', 'rel', d.rel, 10, 1000,
          '${d.rel.toStringAsFixed(0)} ms', 150),
    ];
  }
  return [
    _KnobDef('Threshold', 'thr', d.thr, -60, 0,
        '${d.thr.toStringAsFixed(1)} dB', -20),
    _KnobDef(
        'Ratio', 'ratio', d.ratio, 1, 20, '${d.ratio.toStringAsFixed(1)}:1', 3),
    _KnobDef(
        'Knee', 'knee', d.knee, 0, 6, '${d.knee.toStringAsFixed(1)} dB', 3),
    _KnobDef(
        'Attack', 'att', d.att, 0.1, 100, '${d.att.toStringAsFixed(1)} ms', 10),
    _KnobDef('Release', 'rel', d.rel, 10, 1000,
        '${d.rel.toStringAsFixed(0)} ms', 120),
    _KnobDef('Gain', 'makeup', d.makeup, 0, 24,
        '${d.makeup.toStringAsFixed(1)} dB', 0),
  ];
}

// ---------------------------------------------------------------------------
// (Sends/groups/presets left the tab bar: sends + assignments live inside
// Input above; presets is a per-processor dropdown in the settings row.)
// ---------------------------------------------------------------------------
