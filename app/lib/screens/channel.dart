// Channel screen: preamp, EQ, dynamics, sends, groups, and source tabs.
// The owner implements the store over the engine. Freshness comes from the
// single shell-owned 80 ms loop; this page owns no timer.

import '../core/channel_name.dart';
import '../core/destination_controls.dart';
import '../core/processor_presets.dart';
import 'dart:async';

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';

import '../core/channel_views.dart';
import '../core/audio_spectrum_peaks.dart';
import '../engine.dart' show AudioSpectrumFrame;
import '../settings/prefs.dart';
import '../theme.dart';
import '../widgets/mobile_reference_width.dart';
import '../palette.dart';
import '../design.dart';
import '../u4law.dart';

import '../widgets/channel_widgets.dart';
import '../widgets/processor_preset_library.dart';
import '../widgets/destination_control_editor.dart';

part 'processing_input.dart';
part 'channel_controls.dart';
part 'processing_assignments.dart';
part 'processing_eq.dart';
part 'processing_workspace.dart';
part 'processing_dynamics.dart';
part 'processing_mobile.dart';

// Channel tabs: sends lives inside Input; presets is a per-processor dropdown.
enum ChannelTab { input, gate, comp, eq }

// Implemented by the shell over the engine or a fixture sim.
abstract class ChannelStore extends ChangeNotifier
    implements ChannelNameStore, ProcessorPresetStore {
  String get stripKey;
  int get connectionEpoch;
  // A removed local limit must not revive a previously captured editor.
  int get controlAccessEpoch => 0;
  String get displayName;

  String get channelNumber;

  /// Live fader notifier for the toolbar readout + typed entry.
  ValueNotifier<double> faderNote();

  /// Move this channel's fader to an absolute dB value.
  void setFaderDb(double db);

  // --- param plane (echo-confirmed reads, caps gating) ---
  double? paramNum(String leaf);
  bool? paramBool(String leaf);
  bool supports(String leaf);
  String? localAccessMessage(String leaf) => null;

  // Catalogue reads do not imply a hydrated or editable live value.
  bool get capabilitiesReady;
  Map<String, dynamic>? paramMetadata(String leaf);
  bool paramApplies(String leaf) => paramMetadata(leaf) != null;

  // --- commands ---
  void cmdParam(String leaf, Object value);
  void cmdFocus();
  void cmdLock();
  void cmdUnlock();
  void cmdSubgroupAssign(int g, bool on);
  void cmdMuteGroupAssign(int g, bool on);

  // --- live feeds (80 ms loop target) ---
  ValueNotifier<double> get levelDb;
  ValueNotifier<List<double>> get spectrum;
  ValueListenable<AudioSpectrumFrame?> get audioSpectrum =>
      const AlwaysStoppedAnimation<AudioSpectrumFrame?>(null);

  ValueListenable<AudioSpectrumPeaks> get audioSpectrumPeaks =>
      const AlwaysStoppedAnimation<AudioSpectrumPeaks>(
          AudioSpectrumPeaks.empty());

  void resetAudioSpectrumPeaks() {}
  bool get hasDynamicsMeters => false;
  ValueListenable<ChannelDynamicsMetersVm?> get dynamicsMeters =>
      const AlwaysStoppedAnimation<ChannelDynamicsMetersVm?>(null);
  double? grDb(DynKind kind);
  Future<void> pollMeters();

  // --- snapshots ---
  ChannelPreampVm get preamp;
  bool get hasDelay => false;
  ChannelDelayVm? get delay => null;
  void setDelay(ChannelDelayVm snapshot, double value) {}
  ChannelSourceVm get source;
  ChannelSourceModeVm? get sourceMode => null;
  void setSourceMode(ChannelSourceModeVm snapshot, int mode) {}
  void setSourceSelected(int selected);
  void setSendPan(int slot, double pan);

  // Stable per-channel feed for `level` or `pan`. Null means no value arrived.
  ValueNotifier<double?> sendParamNote(int slot, String param);
  bool? get mainAssigned;
  void setMainAssigned(bool on);
  int get dcaCount;
  bool? dcaAssigned(int group);
  String dcaGroupLabel(int group) => 'VCA ${group + 1}';
  void setDcaAssigned(int group, bool on);
  int get viewGroupCount => 0;
  bool? viewGroupAssigned(int group) => null;
  String viewGroupLabel(int group) => 'View ${group + 1}';
  void setViewGroupAssigned(int group, bool on) {}

  ChannelEqVm get eq;
  ChannelDynVm get gate;
  ChannelDynVm get comp;
  List<ChannelSendVm> get sends;
  bool destinationControlIsCurrent(DestinationControlVm control) => false;
  Future<String?> setDestinationControl(
          DestinationControlVm control, double value) async =>
      'Destination control unavailable.';
  int get subgroupCount;
  int get muteGroupCount;
  bool? subgroupAssigned(int g);
  bool? muteGroupAssigned(int g);
  bool get locked;
  bool get focused;

  // --- tab + selection state ---
  ChannelTab get tab;
  void setTab(ChannelTab tab);
  void selectEqBand(int band);

  // --- edits ---
  void setEqParam(int band, String param, double value);

  void setEqBandType(int band, EqFilterShape shape);
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
}

// One focused numeric control: tapping the top readout opens typed entry.
class ChannelFocus {
  final String title;
  final String label;
  final String Function() text;
  final String Function()? entryText;
  final Listenable? valueListenable;
  final double _min;
  final double _max;
  final List<double>? Function()? resolveRange;
  bool get enabled => resolveRange == null || resolveRange!() != null;
  double get min => resolveRange?.call()?.first ?? _min;
  double get max => resolveRange?.call()?.last ?? _max;
  final ValueChanged<double> apply;

  const ChannelFocus(
      {required this.title,
      required this.label,
      required this.text,
      this.entryText,
      this.valueListenable,
      required double min,
      required double max,
      this.resolveRange,
      required this.apply})
      : _min = min,
        _max = max;
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
  final _swipePointers = <int>{};
  bool _swipeMultiTouch = false;
  double _swipeDistance = 0;
  ChannelTab? _swipeTab;

  late final _focus = widget.controlFocus ?? ValueNotifier<ChannelFocus?>(null);

  @override
  void initState() {
    super.initState();
    widget.store.cmdFocus();
  }

  @override
  void dispose() {
    if (widget.controlFocus == null) _focus.dispose();
    super.dispose();
  }

  void _switchTab(ChannelStore store, ChannelTab tab) {
    store.setTab(tab);
    _focus.value =
        tab == ChannelTab.eq ? eqFreqFocus(store, store.eq.selectedBand) : null;
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
        // Descendant graph and slider recognizers win over this fallback
        // so editing a control cannot also change tabs.
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
          final tabs = _availableTabs(store);
          final index = tabs.indexOf(start);
          final next = index + (_swipeDistance < 0 ? 1 : -1);
          if (next >= 0 && next < tabs.length) {
            _switchTab(store, tabs[next]);
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
        child: ListenableBuilder(
          listenable: Listenable.merge([widget.store, PrefsService.instance]),
          builder: (context, _) {
            final store = widget.store;
            final wide = MediaQuery.sizeOf(context).width >= 700;
            final selectedTab = _availableTabs(store).contains(store.tab)
                ? store.tab
                : ChannelTab.input;
            final tabContent = switch (selectedTab) {
              ChannelTab.input =>
                _InputTab(store: store, onFocus: (f) => _focus.value = f),
              ChannelTab.eq =>
                _EqTab(store: store, onFocus: (f) => _focus.value = f),
              ChannelTab.gate => _DynTab(
                  store: store,
                  onFocus: (f) => _focus.value = f,
                  kind: DynKind.gate),
              ChannelTab.comp => _DynTab(
                  store: store,
                  onFocus: (f) => _focus.value = f,
                  kind: DynKind.comp),
            };
            final content = SingleChildScrollView(
              key: PageStorageKey('processing-${selectedTab.name}'),
              child: KeyedSubtree(
                  key: ValueKey((store.stripKey, selectedTab)),
                  child: wide
                      ? Padding(
                          padding: const EdgeInsets.all(12), child: tabContent)
                      : MobileReferenceWidth(
                          referenceWidth: 362,
                          child: Padding(
                              padding: const EdgeInsets.fromLTRB(8, 12, 8, 20),
                              child: tabContent))),
            );
            return LayoutBuilder(builder: (context, bounds) {
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
                                  tab: selectedTab,
                                  onTab: (t) => _switchTab(store, t)))),
                    Expanded(
                        child: Column(children: [
                      Expanded(child: _swipeNavigation(store, content)),
                      if (!wide)
                        _BottomNav(
                            store: store,
                            tab: selectedTab,
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

List<ChannelTab> _availableTabs(ChannelStore store) => [
      for (final tab in _tabOrder)
        if (switch (tab) {
          ChannelTab.input => true,
          ChannelTab.gate => store.paramApplies('gate_on'),
          ChannelTab.comp => store.paramApplies('comp_on'),
          ChannelTab.eq => store.paramApplies('eq_on'),
        })
          tab,
    ];

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
          for (final t in _availableTabs(store))
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
              for (var i = 0; i < _availableTabs(store).length; i++) ...[
                Expanded(
                    child: _BottomCell(
                        store: store,
                        tab: _availableTabs(store)[i],
                        active: _availableTabs(store)[i] == tab,
                        onTap: () => onTab(_availableTabs(store)[i]))),
                if (i < _availableTabs(store).length - 1)
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

// Live text reads the store at render time because closures over snapshots go stale.
String _eqFreqText(ChannelStore store, int band) {
  if (band == 0) return '${store.eq.hpfF.toStringAsFixed(0)} Hz';
  final bands = store.eq.bands;
  final b = bands.firstWhere((x) => x.band == band, orElse: () => bands.first);
  final f = b.f;
  return f >= 1000
      ? '${(f / 1000).toStringAsFixed(1)}k'
      : '${f.toStringAsFixed(0)} Hz';
}
