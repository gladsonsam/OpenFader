// Channel screen: preamp, EQ, dynamics, sends, groups, and source tabs.
// The owner implements the store over the engine. Freshness comes from the
// single shell-owned 80 ms loop; this page owns no timer.

import '../core/channel_name.dart';
import '../core/destination_controls.dart';
import '../core/processor_presets.dart';
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart' show ValueListenable;
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';

import '../core/channel_views.dart';
import '../core/audio_spectrum_peaks.dart';
import '../engine.dart' show AudioSpectrumFrame;
import '../settings/prefs.dart';
import '../mixer/geometry.dart';
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
  void setChannelSpectrumActive(bool active, {Object? owner}) {}
  bool get hasDynamicsMeters => false;
  ValueListenable<ChannelDynamicsMetersVm?> get dynamicsMeters =>
      const AlwaysStoppedAnimation<ChannelDynamicsMetersVm?>(null);
  ValueListenable<double?> get automixGain =>
      const AlwaysStoppedAnimation<double?>(null);
  double? grDb(DynKind kind);
  Future<void> pollMeters();

  // --- snapshots ---
  ChannelPreampVm get preamp;

  /// Null means unknown or not a source-dependent provider.
  bool? get preampGainAvailable => null;
  bool? get preampPhantomAvailable => null;
  List<double>? get preampGainRange => null;
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

  List<DestinationControlVm> get rtaSettings => const [];
  List<DestinationControlVm> get inputInserts => const [];
  List<DestinationControlVm> get automixSettings => const [];
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

  /// Positive steps select the next channel in the shell's bank/layer order.
  final ValueChanged<int>? onChannelStep;

  const ChannelPage(
      {super.key,
      required this.stripKey,
      required this.store,
      this.mixerControls,
      this.controlFocus,
      this.onChannelStep});

  @override
  State<ChannelPage> createState() => _ChannelPageState();
}

class _ChannelPageState extends State<ChannelPage> {
  final _swipePointers = <int>{};
  bool _swipeMultiTouch = false;
  double _swipeDistance = 0;
  ChannelTab? _swipeTab;
  String? _swipeStripKey;
  int? _swipeEpoch;
  VelocityTracker? _swipeVelocity;
  double _edgeSwipeDistance = 0;
  bool _scrollStartedAtTop = false, _scrollStartedAtBottom = false;

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

  ChannelTab _navigationTab(ChannelStore store) =>
      _availableTabs(store).contains(store.tab) ? store.tab : ChannelTab.input;

  void _finishSwipe(
      ChannelStore store, Axis axis, double distance, double velocity) {
    final start = _swipeTab;
    _swipeTab = null;
    _edgeSwipeDistance = 0;
    if (!mounted ||
        _swipeMultiTouch ||
        start == null ||
        _navigationTab(store) != start ||
        store.stripKey != _swipeStripKey ||
        store.connectionEpoch != _swipeEpoch) {
      return;
    }
    if (distance.abs() < 56 &&
        !(distance.abs() >= 24 &&
            velocity.abs() >= 450 &&
            velocity.sign == distance.sign)) {
      return;
    }
    if (axis == Axis.horizontal) {
      widget.onChannelStep?.call(distance < 0 ? 1 : -1);
      return;
    }
    final tabs = _availableTabs(store);
    final index = tabs.indexOf(start);
    if (index < 0) return;
    // The processor rail is ordered top-to-bottom: up selects the previous
    // processor, down the next. Boundaries stop rather than wrapping.
    final next = index + (distance < 0 ? -1 : 1);
    if (next >= 0 && next < tabs.length) _switchTab(store, tabs[next]);
  }

  bool _trackScrollEdge(ScrollNotification notification) {
    if (notification.depth != 0 || notification.metrics.axis != Axis.vertical) {
      return false;
    }
    if (notification is ScrollStartNotification &&
        notification.dragDetails != null) {
      _scrollStartedAtTop = notification.metrics.extentBefore <= .5;
      _scrollStartedAtBottom = notification.metrics.extentAfter <= .5;
      _edgeSwipeDistance = 0;
    }
    final details = switch (notification) {
      OverscrollNotification() => notification.dragDetails,
      ScrollUpdateNotification() => notification.dragDetails,
      _ => null,
    };
    if (details == null || _swipeMultiTouch || _swipeTab == null) return false;
    final delta = details.delta.dy;
    // Only a fresh outward swipe that starts at a boundary can navigate.
    // Scrolling to a boundary in the middle of a drag never changes pages.
    final outward = (_scrollStartedAtTop &&
            notification.metrics.extentBefore <= .5 &&
            delta > 0) ||
        (_scrollStartedAtBottom &&
            notification.metrics.extentAfter <= .5 &&
            delta < 0);
    _edgeSwipeDistance = outward ? _edgeSwipeDistance + delta : 0;
    return false;
  }

  void _startSwipePointer(ChannelStore store, PointerEvent event) {
    if (_swipePointers.isEmpty) {
      _swipeMultiTouch = false;
      _swipeTab = _navigationTab(store);
      _swipeStripKey = store.stripKey;
      _swipeEpoch = store.connectionEpoch;
      _edgeSwipeDistance = 0;
      _scrollStartedAtTop = false;
      _scrollStartedAtBottom = false;
      _swipeVelocity = VelocityTracker.withKind(event.kind)
        ..addPosition(event.timeStamp, event.position);
    }
    _swipePointers.add(event.pointer);
    if (_swipePointers.length > 1) _swipeMultiTouch = true;
  }

  void _endSwipePointer(ChannelStore store, PointerEvent event,
      {bool trackPosition = true}) {
    if (trackPosition) {
      _swipeVelocity?.addPosition(event.timeStamp, event.position);
    }
    final distance = _edgeSwipeDistance;
    final velocity = _swipeVelocity?.getVelocity().pixelsPerSecond.dy ?? 0;
    _swipePointers.remove(event.pointer);
    if (distance != 0) {
      // Let the scroll recognizer finish before replacing its page.
      scheduleMicrotask(
          () => _finishSwipe(store, Axis.vertical, distance, velocity));
    }
  }

  Widget _swipeNavigation(ChannelStore store, Widget child) {
    return Listener(
      onPointerDown: (event) => _startSwipePointer(store, event),
      onPointerMove: (event) =>
          _swipeVelocity?.addPosition(event.timeStamp, event.position),
      onPointerUp: (event) => _endSwipePointer(store, event),
      onPointerPanZoomStart: (event) => _startSwipePointer(store, event),
      onPointerPanZoomUpdate: (event) => _swipeVelocity?.addPosition(
          event.timeStamp, event.position + event.pan),
      onPointerPanZoomEnd: (event) =>
          _endSwipePointer(store, event, trackPosition: false),
      onPointerCancel: (event) {
        _swipePointers.remove(event.pointer);
        _swipeMultiTouch = true;
        _swipeTab = null;
      },
      child: NotificationListener<ScrollNotification>(
        onNotification: _trackScrollEdge,
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          // Graph/slider recognizers retain their edits. Scroll views consume
          // vertical drags while they have content in that direction.
          onHorizontalDragStart: (_) => _swipeDistance = 0,
          onHorizontalDragUpdate: (details) =>
              _swipeDistance += details.delta.dx,
          onHorizontalDragEnd: (details) => _finishSwipe(store, Axis.horizontal,
              _swipeDistance, details.primaryVelocity ?? 0),
          onVerticalDragStart: (_) => _swipeDistance = 0,
          onVerticalDragUpdate: (details) => _swipeDistance += details.delta.dy,
          onVerticalDragEnd: (details) => _finishSwipe(store, Axis.vertical,
              _swipeDistance, details.primaryVelocity ?? 0),
          child: child,
        ),
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
              // Gate and Compressor share geometry and scroll position.
              key: PageStorageKey(selectedTab == ChannelTab.gate ||
                      selectedTab == ChannelTab.comp
                  ? 'processing-dynamics'
                  : 'processing-${selectedTab.name}'),
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
                  ? MixerGeometry.stripWidth(bounds.maxWidth, false) +
                      MixerGeometry.railWidth(bounds.maxWidth)
                  : bounds.maxWidth * 104 / 466;
              return Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Expanded(
                        child: _swipeNavigation(
                            store,
                            Row(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  if (wide)
                                    Padding(
                                        padding: const EdgeInsets.fromLTRB(
                                            8, 12, 8, 12),
                                        child: SizedBox(
                                            width: bounds.maxWidth * 178 / 1823,
                                            child: _SideMenu(
                                                store: store,
                                                tab: selectedTab,
                                                onTab: (t) =>
                                                    _switchTab(store, t)))),
                                  Expanded(
                                      child: Column(children: [
                                    Expanded(child: content),
                                    if (!wide)
                                      _BottomNav(
                                          store: store,
                                          tab: selectedTab,
                                          onTab: (t) => _switchTab(store, t)),
                                  ])),
                                ]))),
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
      child: LayoutBuilder(builder: (context, constraints) {
        final tabs = _availableTabs(store);
        final height =
            (constraints.maxHeight / tabs.length).clamp(110.0, 190.0);
        return SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final t in tabs)
                SizedBox(
                  height: height,
                  child: _MenuRow(
                      store: store,
                      tab: t,
                      active: t == tab,
                      onTap: () => onTab(t)),
                ),
            ],
          ),
        );
      }),
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
    final status = switch (tab) {
      ChannelTab.input => null,
      ChannelTab.gate => store.paramBool('gate_on'),
      ChannelTab.comp => store.paramBool('comp_on'),
      ChannelTab.eq => store.paramBool('eq_on'),
    };
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
                Text(_tabLabels[tab]!,
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
                                : ConsolePalette.of(context).text4)),
                if (status != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 2),
                    child: Text(status ? 'On' : 'Off',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: 13,
                            fontWeight: FontWeight.w600,
                            color: ConsolePalette.of(context).text7)),
                  ),
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
