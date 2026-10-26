// Shell navigation and composition. Responsibility-specific store adapters live
// beside this router and bind screens to AppStore through constructor injection.
// The shell owns the single shared 80 ms poll loop so every tab stays fresh.
// FX labels and engineering readouts come from provider caps.
// Stats frames are not yet parsed into EngineState.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import '../palette.dart';

import '../design.dart';
import '../engine.dart';
import '../ffi_engine.dart';
import '../mixer/components/meter.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../state/app_store.dart';
import '../theme.dart';
import '../u4law.dart' show panText;
import 'shell.dart';
import '../screens/channel.dart';
import '../screens/output_processing.dart';
import '../mixer/components/banks.dart';
import '../mixer/components/strip.dart';
import '../screens/channel_toolbar.dart';
import '../screens/connection.dart';
import '../screens/debug.dart';
import '../screens/fx.dart';
import '../screens/meters.dart';
import '../screens/start.dart';
import '../mixer/controller.dart';
import '../mixer/page.dart';
import '../screens/routing.dart';
import '../screens/scenes.dart';
import '../screens/settings.dart';

import 'connection_store.dart';
import 'routing_store.dart';
import 'scenes_store.dart';
import 'settings_store.dart';
import 'fx_store.dart';
import 'debug_store.dart';
import 'quick_access_store.dart';
import '../screens/quick_access.dart';

enum AppTab {
  mixer('Mixer'),
  meters('Meters'),
  channel('Channel'),
  scenes('Scenes'),
  fx('FX'),
  routing('Routing'),
  connect('Connection'),
  settings('Settings'),
  debug('Debug');

  const AppTab(this.label);
  final String label;
}

class AppShell extends StatefulWidget {
  final EngineApi api;
  final AppStore store;
  final AppTab initialTab;

  // True when main won the real engine; false is the loud FAKE sim fallback.

  final bool isRealEngine;

  const AppShell(
      {super.key,
      required this.api,
      required this.store,
      this.initialTab = AppTab.mixer,
      required this.isRealEngine});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  late AppTab _tab;

  // Where Settings Close returns.
  AppTab _prevTab = AppTab.mixer;
  Timer? _shellLoop;
  final _channelFocus = ValueNotifier<ChannelFocus?>(null);
  Object? _channelFocusTarget;

  void _syncChannelFocus() {
    final store = widget.store;
    final target = (store.stripKey, store.bank, store.engine.driver, store.tab);
    if (_channelFocusTarget == target) return;
    _channelFocusTarget = target;
    _channelFocus.value = null;
  }

  // Start screen is the front door; the shell stays alive underneath so returning keeps state.

  late bool _showStart;

  late final MixerController _mixer;
  late final ShellConnectionStore _conn;
  late final ShellRoutingStore _routing;
  late final ShellScenesStore _scenes;
  late final ShellSettingsStore _settings;
  late final ShellFxStore _fx;
  late final ShellDebugStore _debug;
  late final ShellQuickAccessStore _quickAccess;

  @override
  void initState() {
    super.initState();
    _tab = widget.initialTab;
    _syncChannelFocus();
    widget.store.addListener(_syncChannelFocus);
    _showStart = PrefsService.instance.startupDest != 'last';
    _mixer = MixerController(widget.store,
        seedReferenceDemo: widget.initialTab == AppTab.mixer);
    _conn = ShellConnectionStore(widget.store);
    _routing = ShellRoutingStore(widget.store);
    _scenes = ShellScenesStore(widget.store);
    _settings = ShellSettingsStore(widget.store);
    _fx = ShellFxStore(widget.store);
    _quickAccess = ShellQuickAccessStore(widget.store, _fx);
    _debug = ShellDebugStore(widget.store);
    _applyMeterTuning();
    PrefsService.instance.addListener(_onPrefs);
    // Single shared ticker: the only poll loop in the app because pages own no timer.

    _restartLoop();
    _tick();
    // Auto-reconnect stays off unless the preference enables it.

    final prefs = PrefsService.instance;
    if (prefs.reconnectLast && prefs.lastMixerAddr.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(_reconnectLast(prefs.lastMixerAddr));
      });
    }
  }

  Future<void> _reconnectLast(String addr) async {
    final run = _conn.connectionRun;
    final driver = await RecentMixers.savedDriverFor(addr);
    if (mounted && run == _conn.connectionRun) {
      _conn.probe(addr, mixerType: MixerType.fromDriver(driver));
    }
  }

  // Meter cadence and peak behavior follow the Appearance prefs.

  void _onPrefs() {
    if (!mounted) return;
    _applyMeterTuning();
    _restartLoop();
    setState(() {});
  }

  void _applyMeterTuning() {
    final prefs = PrefsService.instance;
    MeterTuning.peakHoldMs = prefs.peakHold * 1000;
    MeterTuning.holdClips = prefs.holdClips;
    AppDesign.largeTargets = prefs.largeTargets;
  }

  void _restartLoop() {
    _shellLoop?.cancel();
    final ms = (1000 / PrefsService.instance.meterRate).round().clamp(16, 250);
    _shellLoop = Timer.periodic(Duration(milliseconds: ms), (_) => _tick());
  }

  Future<void> _tick() async {
    try {
      await widget.store.pollMeters();
      _conn.tickRecovery();
      if (mounted && _tab == AppTab.debug) _debug.captureFrame();
    } catch (_) {
      // A transport hiccup must not crash the shell loop.
    }
  }

  @override
  void dispose() {
    _shellLoop?.cancel();
    PrefsService.instance.removeListener(_onPrefs);
    widget.store.removeListener(_syncChannelFocus);
    _mixer.dispose();
    _channelFocus.dispose();
    _conn.dispose();
    _routing.dispose();
    _scenes.dispose();
    _settings.dispose();
    _quickAccess.dispose();
    _fx.dispose();
    _debug.dispose();
    super.dispose();
  }

  void _go(AppTab t) {
    if (t != _tab) _channelFocus.value = null;
    if (t == AppTab.scenes) widget.store.refreshLocalLibrary();
    setState(() {
      if (t == AppTab.settings && _tab != AppTab.settings) {
        _prevTab = _tab;
      }
      _tab = t;
    });
  }

  SettingsSection? _settingsSection;

  void _openSettings(SettingsSection? section) {
    _settingsSection = section;
    _go(AppTab.settings);
  }

  ShellMode _modeOf(AppTab t) => switch (t) {
        AppTab.mixer || AppTab.meters => ShellMode.mixer,
        AppTab.channel => ShellMode.channel,
        AppTab.scenes => ShellMode.scenes,
        AppTab.fx => ShellMode.fx,
        AppTab.routing => ShellMode.routing,
        AppTab.connect => ShellMode.connect,
        AppTab.settings || AppTab.debug => ShellMode.settings,
      };

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    // Front door first: start screen replaces shell chrome until the user enters.

    if (_showStart && _tab != AppTab.settings) {
      return ListenableBuilder(
        listenable: store,
        builder: (context, _) => ConnectionsStartScreen(
          store: _conn,
          onSettings: () => _openSettings(null),
          onHelp: () => _openSettings(SettingsSection.about),
          onEnterMixer: () => setState(() => _showStart = false),
          onWorkOffline: () {
            _conn.connectSim();
            setState(() => _showStart = false);
          },
        ),
      );
    }
    // Structural listener only: hot-path writes update per-strip notifiers in place
    // so a fader drag rebuilds one strip instead of the whole shell.

    return ListenableBuilder(
      listenable: store,
      builder: (context, _) {
        final connected = store.engine.connected;
        final sim = store.engine.driver == 'sim';

        // SIM gets its own tag so it never reads as a desk; the backend verdict stays loud.

        final title = sim
            ? 'OpenFader · Demo desk'
            : '${store.engine.linkName.isEmpty ? 'OpenFader' : store.engine.linkName} · ${store.engine.linkState.isEmpty ? 'Not connected' : store.engine.linkState}';
        final isChan = _tab == AppTab.channel;
        final narrow = MediaQuery.sizeOf(context).width < 720;
        if (_tab == AppTab.mixer &&
            (connected || sim || store.engine.linkState == 'offline')) {
          return _withQuickAccess(_mixerPage());
        }
        // Settings and channel views carry their own top bars, so the shared header steps aside.

        final Widget header = isChan && store.outputProcessingTarget != null
            ? const SizedBox.shrink()
            : isChan
                ? ChannelToolbar(
                    key: ValueKey(
                        (store.stripKey, store.bank, store.engine.driver)),
                    controlFocus: _channelFocus,
                    store: store,
                    compact: MediaQuery.sizeOf(context).width < 600,
                    onHome: () => _go(AppTab.mixer),
                    onTool: _channelTool,
                    onLock: () =>
                        store.locked ? store.cmdUnlock() : store.cmdLock(),
                  )
                : _tab == AppTab.settings
                    ? const SizedBox.shrink()
                    : U4ShellHeader(
                        title: title,
                        pageTitle: _tab.label,
                        showBack: _tab != AppTab.mixer,
                        onBack: () => _go(AppTab.mixer),
                        mode: _modeOf(_tab),
                        connected: connected,
                        connDot: connected
                            ? (sim
                                ? ConsolePalette.of(context).text7
                                : ConsolePalette.of(context).meterGreen)
                            : ConsolePalette.of(context).meterAmber,
                        connAria: !connected
                            ? 'Disconnected'
                            : sim
                                ? 'Demo simulator (not a desk)'
                                : 'Connected to desk',
                        isChannel: isChan,
                        channelName: isChan ? store.displayName : '',
                        metersActive: _tab == AppTab.meters,
                        scenesActive: _tab == AppTab.scenes,
                        sceneNum: _sceneNum(store),
                        sceneName: _sceneName(store),
                        sceneAria:
                            '${_sceneNum(store).isEmpty ? '' : 'Scene ${_sceneNum(store)}, '}${_sceneName(store)}, open scene list',
                        compact: MediaQuery.sizeOf(context).width < 600,
                        onFx: () => _go(AppTab.fx),
                        onRouting: () => _go(AppTab.routing),
                        onConnect: () => _go(AppTab.connect),
                        onSettings: () => _go(AppTab.settings),
                        onMeters: () => _go(AppTab.meters),
                        onScene: () => _go(AppTab.scenes),
                        onPrevChannel: isChan && connected
                            ? () => store.openChannel((store.channelIndex - 1)
                                .clamp(0, store.engine.strips.length - 1))
                            : null,
                        onNextChannel: isChan && connected
                            ? () => store.openChannel((store.channelIndex + 1)
                                .clamp(0, store.engine.strips.length - 1))
                            : null,
                      );
        final page = _pageFor(_tab, store);
        final rail = _NavRail(
          active: _tab,
          onGo: _go,
          vertical: !narrow,
        );
        final ownsNavigation = isChan || _tab == AppTab.settings;
        final body = ownsNavigation
            ? page
            : narrow
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [Expanded(child: page), rail],
                  )
                : Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [Expanded(child: page), rail],
                  );
        return _withQuickAccess(U4Shell(
            header: header, bottomSafeArea: !(isChan && narrow), child: body));
      },
    );
  }

  Widget _withQuickAccess(Widget child) => Stack(children: [
        Positioned.fill(child: child),
        Positioned(
          right: 12,
          bottom: 12,
          child: SafeArea(
            child: FloatingActionButton.small(
              heroTag: 'quick-access',
              tooltip: 'Quick Access',
              backgroundColor: ConsolePalette.of(context).control,
              foregroundColor: ConsolePalette.of(context).cyan,
              onPressed: () => showModalBottomSheet<void>(
                context: context,
                isScrollControlled: true,
                useSafeArea: true,
                builder: (context) => ConstrainedBox(
                  constraints: BoxConstraints(
                    maxHeight: MediaQuery.sizeOf(context).height * 0.85,
                    maxWidth: 640,
                  ),
                  child: SafeArea(child: QuickAccessPanel(store: _quickAccess)),
                ),
              ),
              child: const Icon(Icons.bolt),
            ),
          ),
        ),
      ]);

  void _channelTool(ChannelTool t) {
    switch (t) {
      case ChannelTool.scenes:
        _go(AppTab.scenes);
      case ChannelTool.fx:
        _go(AppTab.fx);
      case ChannelTool.settings:
        _go(AppTab.settings);
      case ChannelTool.routing:
        _go(AppTab.routing);
      case ChannelTool.layout:
        showMixerLayout(
            context,
            _mixer,
            (d) => _go(switch (d) {
                  MixerDestination.meters => AppTab.meters,
                  MixerDestination.channel => AppTab.channel,
                  MixerDestination.scenes => AppTab.scenes,
                  MixerDestination.fx => AppTab.fx,
                  MixerDestination.routing => AppTab.routing,
                  MixerDestination.connect => AppTab.connect,
                  MixerDestination.settings => AppTab.settings,
                  MixerDestination.debug => AppTab.debug,
                }),
            locked: widget.store.locked,
            onLock: () => widget.store.locked
                ? widget.store.cmdUnlock()
                : widget.store.cmdLock());
    }
  }

  Widget _mixerPage() => MixerPage(
      controller: _mixer,
      onHome: () => setState(() => _showStart = true),
      onNavigate: (destination) => _go(switch (destination) {
            MixerDestination.meters => AppTab.meters,
            MixerDestination.channel => AppTab.channel,
            MixerDestination.scenes => AppTab.scenes,
            MixerDestination.fx => AppTab.fx,
            MixerDestination.routing => AppTab.routing,
            MixerDestination.connect => AppTab.connect,
            MixerDestination.settings => AppTab.settings,
            MixerDestination.debug => AppTab.debug,
          }));

  Widget _pageFor(AppTab t, AppStore store) {
    if (t == AppTab.scenes && store.blockedAccess.isNotEmpty) {
      return Center(child: Padding(
        padding: const EdgeInsets.all(24),
        child: Text('Scenes and shows are unavailable while local access limits are active. Bulk recalls can change restricted controls.',
          style: TextStyle(color: ConsolePalette.of(context).text1, fontSize: 18)),
      ));
    }
    if (!store.engine.connected &&
        store.engine.driver != 'sim' &&
        store.engine.linkState != 'offline' &&
        {AppTab.mixer, AppTab.channel, AppTab.fx, AppTab.routing}.contains(t)) {
      return Center(
          child: Column(mainAxisSize: MainAxisSize.min, children: [
        Text('Mixer disconnected',
            style: TextStyle(
                color: ConsolePalette.of(context).text1, fontSize: 18)),
        TextButton(
            onPressed: () => _go(AppTab.connect),
            child: const Text('Reconnect mixer')),
      ]));
    }
    return switch (t) {
      AppTab.mixer => _mixerPage(),
      AppTab.meters =>
        U4MetersPage(store: store, onClose: () => _go(AppTab.mixer)),
      AppTab.channel => store.outputProcessingTarget != null
          ? OutputProcessingPage(store: store, onClose: () => _go(AppTab.mixer))
          : ChannelPage(
              key: ValueKey((store.stripKey, store.bank, store.engine.driver)),
              stripKey: store.stripKey,
              store: store,
              mixerControls: _processingMixerControls(),
              controlFocus: _channelFocus),
      AppTab.scenes => U4ScenesPage(store: _scenes),
      AppTab.fx => U4FxPage(store: _fx),
      AppTab.routing => U4RoutingPage(store: _routing),
      AppTab.connect => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _EngineTagBanner(isReal: widget.isRealEngine),
            Expanded(
                child: U4ConnectionPage(
                    store: _conn, onOpenDebug: () => _go(AppTab.debug))),
          ],
        ),
      AppTab.settings => U4SettingsPage(
          initialSection: _settingsSection,
          store: _settings,
          onClose: () {
            _settingsSection = null;
            _go(_prevTab);
          },
          onOpenMixer: () {
            _showStart = false;
            _conn.connectSim();
            _go(AppTab.mixer);
          }),
      AppTab.debug => U4DebugPage(store: _debug),
    };
  }

  Widget _processingMixerControls() => LayoutBuilder(builder: (context, box) {
        final wide = MediaQuery.sizeOf(context).width >= 700;
        return ListenableBuilder(
            listenable: _mixer,
            builder: (context, _) {
              final selected = _mixer.selectedProcessing;
              Widget strip(MixerChannel ch) {
                final pan = ch.master || ch.channel == null
                    ? null
                    : _mixer.panNoteOf(ch.channel!);
                final panFocus = pan == null
                    ? null
                    : ChannelFocus(
                        title: 'Pan',
                        label: 'L/R',
                        text: () => panText(pan.value),
                        entryText: () => (pan.value * 100).toStringAsFixed(0),
                        valueListenable: pan,
                        min: -100,
                        max: 100,
                        apply: (v) => _mixer.setPan(ch, v / 100));
                return MixerStrip(
                    channel: ch,
                    controller: _mixer,
                    onControlFocus: (control) {
                      // Null restores the toolbar's live per-strip fader readout.
                      _channelFocus.value =
                          control == MixerReadoutControl.pan ? panFocus : null;
                    },
                    compact: !wide,
                    onOpen: () {});
              }

              if (!wide) {
                return selected == null
                    ? const SizedBox.shrink()
                    : strip(selected);
              }
              final railWidth = box.maxWidth * 93 / 285.2;
              return Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Expanded(
                        child: selected == null
                            ? const SizedBox.shrink()
                            : strip(selected)),
                    SizedBox(
                        width: railWidth,
                        child: MixerBanks(controller: _mixer, vertical: true)),
                  ]);
            });
      });

  String _sceneNum(AppStore s) {
    for (final e in s.engine.scenes) {
      if (e.current) return e.number.toString().padLeft(3, '0');
    }
    return '';
  }

  String _sceneName(AppStore s) {
    for (final e in s.engine.scenes) {
      if (e.current) return e.name.isEmpty ? 'Scene ${e.number}' : e.name;
    }
    return s.engine.scenes.isEmpty
        ? 'Scenes'
        : '${s.engine.scenes.length} ${s.engine.scenes.length == 1 ? 'scene' : 'scenes'}';
  }
}

class _NavRail extends StatelessWidget {
  final AppTab active;
  final ValueChanged<AppTab> onGo;
  final bool vertical;

  const _NavRail(
      {required this.active, required this.onGo, required this.vertical});

  @override
  Widget build(BuildContext context) {
    final items = [
      for (final t in AppTab.values)
        Padding(
          padding: EdgeInsets.only(
              right: vertical ? 0 : 4, bottom: vertical ? 4 : 0),
          child: TextButton(
            onPressed: () => onGo(t),
            style: TextButton.styleFrom(
                backgroundColor: t == active
                    ? ConsolePalette.of(context).control
                    : Colors.transparent,
                foregroundColor: t == active
                    ? ConsolePalette.of(context).cyan
                    : ConsolePalette.of(context).text7,
                minimumSize: const Size(44, 44),
                padding: EdgeInsets.symmetric(horizontal: vertical ? 10 : 14),
                shape: const RoundedRectangleBorder(borderRadius: U4.rBtn)),
            child: Text(t.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style:
                    const TextStyle(fontSize: 13, fontWeight: FontWeight.w600)),
          ),
        ),
    ];
    if (vertical) {
      return Container(
        width: 108,
        padding: const EdgeInsets.fromLTRB(8, 8, 8, 8),
        decoration: BoxDecoration(
            border: Border(
                left:
                    BorderSide(color: ConsolePalette.of(context).lineHeader))),
        child: SingleChildScrollView(
          child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch, children: items),
        ),
      );
    }
    return Container(
      padding: const EdgeInsets.fromLTRB(8, 6, 8, 6),
      decoration: BoxDecoration(
          border: Border(
              top: BorderSide(color: ConsolePalette.of(context).lineHeader))),
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(children: items),
      ),
    );
  }
}

// Backend banner: the boot-time verdict stays loud on the Link tab.

class _EngineTagBanner extends StatelessWidget {
  final bool isReal;

  const _EngineTagBanner({required this.isReal});

  @override
  Widget build(BuildContext context) {
    final err = isReal ? null : FfiEngine.lastError?.toString();
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: isReal
            ? ConsolePalette.of(context).successContainer
            : ConsolePalette.of(context).errorContainer,
        border: Border(
            bottom: BorderSide(
                color: isReal
                    ? ConsolePalette.of(context).meterGreen
                    : ConsolePalette.of(context).meterRed)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            isReal
                ? 'ENGINE: embedded openfader-engine (real)'
                : 'ENGINE: FAKE sim fallback — native lib missing, desk impossible',
            style: TextStyle(
                color: isReal
                    ? ConsolePalette.of(context).onSuccessContainer
                    : ConsolePalette.of(context).onErrorContainer,
                fontSize: 13,
                fontWeight: FontWeight.w700),
          ),
          if (err != null && err.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Text('FFI: $err',
                  style: TextStyle(
                      color: ConsolePalette.of(context).onErrorContainer,
                      fontSize: 11)),
            ),
        ],
      ),
    );
  }
}
