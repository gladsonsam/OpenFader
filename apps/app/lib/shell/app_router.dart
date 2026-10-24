// Area A wire-up: app shell + router + D-screen store implementations.
//
// After boot (sim or FFI), main.dart creates one [AppStore] and pushes
// [AppShell]. The shell owns:
// - a shared 80 ms poll loop ([AppStore.pollMeters]) so every tab stays
//   fresh from this single ticker. The A/B/C pages own no loop of their
//   own; AppStore serializes mailbox drains and the _polling guard makes
//   any stray overlap a no-op.
// - bottom (narrow) / side (wide >= 720) rail across Mixer, Meters,
//   Channel + the six Area D screens (read-only imports — this file never
//   edits them);
// - the header ([U4ShellHeader]) with tile jumps into the D tabs.
//
// The six `_*Store` classes below implement the Area D store contracts
// over [AppStore.engine] (reads) + [AppStore.sendRaw] (writes). They
// forward AppStore notifications so tab switches rebuild. Honest gaps
// (documented, not faked):
// - settings stats: `stats` frames are not parsed into EngineState; the
//   request is sent and the Debug tab shows raw state instead.
// - FX knobs render the slot `params` map generically (0..1) until
//   caps.fx param tables are parsed; tap-tempo has no engine cmd.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import '../palette.dart';
import 'package:share_plus/share_plus.dart';

import '../core/wifi_net.dart';
import '../design.dart';
import '../engine.dart';
import '../ffi_engine.dart';
import '../mixer/components/meter.dart';
import '../settings/mixes.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../state/app_store.dart';
import '../theme.dart';
import 'shell.dart';
import '../screens/channel.dart';
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

/// Shell tabs in rail order.
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

/// Post-boot shell: header + routed page + bottom/side rail.
class AppShell extends StatefulWidget {
  final EngineApi api;
  final AppStore store;
  final AppTab initialTab;

  /// True when main() won the real embedded engine; false = loud FAKE
  /// sim fallback (native .so missing or FFI init failed). Never silent.
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

  /// Where Close on the Settings page returns (the tab that opened it).
  AppTab _prevTab = AppTab.mixer;
  Timer? _shellLoop;
  final _channelFocus = ValueNotifier<ChannelFocus?>(null);

  /// The Connections start screen is the app's front door: shown first
  /// after boot (unless the startup preference says Last mixer),
  /// re-opened from the mixer brand button. The shell (header, rail,
  /// poll loop) stays alive underneath so returning keeps state.
  late bool _showStart;

  late final MixerController _mixer;
  late final _ConnStore _conn;
  late final _RoutingStore _routing;
  late final _ScenesStore _scenes;
  late final _SettingsStore _settings;
  late final _FxStore _fx;
  late final _DebugStore _debug;

  @override
  void initState() {
    super.initState();
    _tab = widget.initialTab;
    _showStart = PrefsService.instance.startupDest != 'last';
    _mixer = MixerController(widget.store,
        seedReferenceDemo: widget.initialTab == AppTab.mixer);
    _conn = _ConnStore(widget.store);
    _routing = _RoutingStore(widget.store);
    _scenes = _ScenesStore(widget.store);
    _settings = _SettingsStore(widget.store);
    _fx = _FxStore(widget.store);
    _debug = _DebugStore(widget.store);
    _applyMeterTuning();
    PrefsService.instance.addListener(_onPrefs);
    // Single shared freshness ticker: the ONLY poll loop in the app.
    // Every tab (mixer/meters/channel/FX/...) renders from the notifiers
    // this drain updates in place; pages own no timer of their own.
    _restartLoop();
    _tick();
    // Optional auto-reconnect to the last desk (off by default).
    final prefs = PrefsService.instance;
    if (prefs.reconnectLast && prefs.lastMixerAddr.isNotEmpty) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(_reconnectLast(prefs.lastMixerAddr));
      });
    }
  }

  Future<void> _reconnectLast(String addr) async {
    final driver = await RecentMixers.savedDriverFor(addr);
    if (mounted) _conn.probe(addr, mixerType: MixerType.fromDriver(driver));
  }

  /// Meter cadence + peak/clip behavior follow the Appearance prefs.
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
      if (mounted && _tab == AppTab.debug) _debug.captureFrame();
    } catch (_) {
      // Never crash the shell loop on a transport hiccup.
    }
  }

  @override
  void dispose() {
    _shellLoop?.cancel();
    PrefsService.instance.removeListener(_onPrefs);
    _mixer.dispose();
    _channelFocus.dispose();
    _conn.dispose();
    _routing.dispose();
    _scenes.dispose();
    _settings.dispose();
    _fx.dispose();
    _debug.dispose();
    super.dispose();
  }

  void _go(AppTab t) => setState(() {
        if (t == AppTab.settings && _tab != AppTab.settings) {
          _prevTab = _tab;
        }
        _tab = t;
      });

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
    // Front door first: the Connections start screen replaces all shell
    // chrome until the user enters the mixer (connect, open, offline).
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
    // Structural listener only: fader/mute/pan hot-path writes update
    // per-strip ValueNotifiers in place (AppStore.faderOf/muteOf/panNoteOf)
    // and never call notifyListeners(), so a fader drag rebuilds one
    // MixerStrip instead of this whole shell + every strip. This builder
    // fires for connection / strips add-remove / bank / bus / solo-banner
    // changes.
    return ListenableBuilder(
      listenable: store,
      builder: (context, _) {
        final connected = store.engine.connected;
        final sim = store.engine.driver == 'sim';

        // Never let "Connected · sim" read as a desk: SIM gets its own tag.
        // Plus the boot-time backend verdict: real embedded engine vs the
        // FAKE sim fallback. Loud on every path — never silent.
        final title = sim
            ? 'OpenFader · Demo desk'
            : '${store.engine.linkName.isEmpty ? 'OpenFader' : store.engine.linkName} · ${store.engine.linkState.isEmpty ? 'Not connected' : store.engine.linkState}';
        final isChan = _tab == AppTab.channel;
        final narrow = MediaQuery.sizeOf(context).width < 720;
        if (_tab == AppTab.mixer &&
            (connected || sim || store.engine.linkState == 'offline')) {
          return _mixerPage();
        }
        // Settings and channel views carry their own approved top bars,
        // so the shared shell header steps aside for them.
        final Widget header = isChan
            ? ChannelToolbar(
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
        return U4Shell(
            header: header, bottomSafeArea: !(isChan && narrow), child: body);
      },
    );
  }

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
      AppTab.channel => ChannelPage(
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
              Widget strip(MixerChannel ch) => MixerStrip(
                  channel: ch,
                  controller: _mixer,
                  compact: !wide,
                  onOpen: () {});
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

/// Bottom (narrow) / side (wide) tab rail. Plain text buttons, ui4 tokens.
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

// ---------------------------------------------------------------------------
// Engine backend banner: the boot-time verdict, loud on every tab that
// matters (AppShell header carries it in the title; the Link tab gets
// this full-width strip too). Green = real embedded engine, red = FAKE
// sim fallback with the FFI failure reason inline.
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Shared base: forwards AppStore notifications, exposes send + engine.
// ---------------------------------------------------------------------------

abstract class _StoreBase extends ChangeNotifier {
  _StoreBase(this.app) {
    app.addListener(_forward);
  }

  final AppStore app;
  bool _dead = false;

  EngineState get engine => app.engine;
  void send(String json) => app.sendRaw(json);

  void _forward() {
    if (!_dead) notifyListeners();
  }

  @override
  void dispose() {
    _dead = true;
    app.removeListener(_forward);
    super.dispose();
  }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

class _ConnStore extends _StoreBase implements ConnectionStore {
  _ConnStore(super.app) {
    _lastError = app.visibleError;
    app.addListener(_settle);
  }

  bool _scanning = false;
  String _lastScan = 'never';
  Timer? _scanTimer;

  // Probe retry state: the desk unicast can be lost (wrong interface,
  // mobile data, ARP miss), so one shot is not enough.
  String _lastProbe = '';
  MixerType _lastMixerType = MixerType.automatic;
  String _connectionProgress = '';
  int _connectionRun = 0;
  int _probeAttempt = 0;
  bool _probing = false;

  // Settle tracking: scanning ends when the engine answers (devices list
  // changes or an error frame/transport failure lands), not on a fixed
  // timer. The timer below is only a fallback so the spinner cannot stick.
  String _lastError = '';
  String? _deviceRequest;
  String? _operationError;
  final Map<String, String> _checks = {};
  final Map<String, String> _canonicalHosts = {};

  @override
  void dispose() {
    _connectionRun++;
    app.removeListener(_settle);
    _scanTimer?.cancel();
    super.dispose();
  }

  void _settle() {
    final complete = engine.deviceOperationComplete;
    if (_deviceRequest != null && complete?['request_id'] == _deviceRequest) {
      _deviceRequest = null;
      _scanning = false;
      _scanTimer?.cancel();
      _operationError = complete?['error'] as String?;
      for (final result in complete?['results'] as List? ?? const []) {
        if (result is Map && result['addr'] is String) {
          final host = (result['addr'] as String).trim().toLowerCase();
          _checks[host] = '${result['status']}';
          final device = result['device'];
          if (device is Map && device['ip'] is String) {
            _canonicalHosts[host] = (device['ip'] as String).toLowerCase();
          }
        }
      }
      _lastScan = '${complete?['operation']} ${_fmtTime(DateTime.now())}';
      WifiNet.scanStop();
      notifyListeners();
    }
    final err = app.visibleError;
    if (_scanning && err.isNotEmpty && err != _lastError) {
      _scanning = false;
      _deviceRequest = null;
      _scanTimer?.cancel();
      WifiNet.scanStop();
      notifyListeners();
    }
    _lastError = err;
  }

  static String _fmtTime(DateTime t) => t.toIso8601String().substring(11, 19);

  bool get _isSim => engine.driver == 'sim';

  @override
  String get title {
    if (_probing) return 'Connecting · ${_lastMixerType.label}';
    if (!isConnected) {
      return _isSim
          ? 'Not connected · sim (demo — not a desk)'
          : 'Not connected · ${engine.driver}';
    }
    if (_isSim) return 'Demo desk · SIM — not a real desk';
    final link = engine.linkState.isEmpty ? '' : ' · ${engine.linkState}';
    final name = engine.linkName.isEmpty ? '' : ' · ${engine.linkName}';
    return 'Connected · desk ${engine.driver}$link$name';
  }

  @override
  String get sub {
    if (_probing) return _connectionProgress;
    if (!isConnected) {
      return _isSim
          ? 'Simulator only — no desk hardware. Find a mixer or add one manually.'
          : 'driver=${engine.driver} — scan/probe the desk Wi-Fi.';
    }
    if (_isSim) {
      return 'SIMULATOR — ${engine.strips.length} strips · no desk hardware · '
          'link=${engine.linkState.isEmpty ? '—' : engine.linkState}';
    }
    return '${engine.strips.length} strips · '
        'link=${engine.linkState.isEmpty ? '—' : engine.linkState} '
        '${engine.linkName} ${engine.linkIp}';
  }

  @override
  bool get isConnected => engine.connected;

  @override
  bool get scanning => _scanning;

  @override
  String get lastScan => _lastScan;

  /// Exact engine/transport failure + timestamp, persistent until cleared
  /// by a good poll or the next scan/probe. Never hides the string.
  @override
  String? get scanError {
    if (_operationError != null) return _operationError;
    final e = app.visibleError;
    if (e.isEmpty) return null;
    final at = app.visibleErrorAt;
    final ts = at == null ? '' : ' (${_fmtTime(at)})';
    return '$e$ts';
  }

  @override
  String? get errorAt {
    final at = app.visibleErrorAt;
    return at == null ? null : _fmtTime(at);
  }

  @override
  String? get lastProbe => _lastProbe.isEmpty ? null : _lastProbe;

  @override
  int get probeAttempt => _probeAttempt;

  @override
  MixerType get lastMixerType => _lastMixerType;

  @override
  String get connectionProgress => _connectionProgress;

  @override
  bool get probing => _probing;

  @override
  List<ConnDevice> get devices => [
        for (final d in engine.devices)
          ConnDevice(
            id: d.id,
            name: d.name,
            mixerType: '${d.id} ${d.model}'.toLowerCase().contains('dl32s')
                ? MixerType.dl32s
                : MixerType.automatic,
            sub: [d.model, d.ip, d.firmware]
                .where((s) => s.isNotEmpty)
                .join(' · '),
            state: d.state == 'connected' && !engine.connected
                ? 'available'
                : d.state,
            connected: engine.connected &&
                engine.driver != 'sim' &&
                d.ip == engine.linkIp,
          ),
      ];

  @override
  String? get emptyText =>
      engine.devices.isEmpty ? 'No desks seen yet — Scan first.' : null;

  @override
  String hostOf(String id) {
    for (final d in engine.devices) {
      if (d.id == id) return d.ip;
    }
    return '';
  }

  @override
  String canonicalHost(String host) =>
      _canonicalHosts[host.trim().toLowerCase()] ?? host.trim().toLowerCase();

  @override
  String get connectedHost =>
      engine.driver == 'sim' || !engine.connected ? '' : engine.linkIp;

  @override
  String get linkInfo {
    final simTag = _isSim ? 'SIM (not a desk)' : 'desk';
    return '$simTag driver=${engine.driver} '
        'link=${engine.linkState.isEmpty ? '—' : engine.linkState} '
        '${engine.linkName}';
  }

  @override
  String get deskInfo => [
        engine.model,
        engine.firmware.isEmpty ? '' : 'FW ${engine.firmware}',
        engine.sampleRate == 0 ? '' : '${engine.sampleRate ~/ 1000} kHz',
        '${engine.strips.length} strips'
      ].where((s) => s.isNotEmpty).join(' · ');

  @override
  int get clientCount => engine.clientCount;

  @override
  List<String> get writers => engine.writers;

  @override
  Future<bool> ensurePermissions() => WifiNet.ensureScanPermissions();

  @override
  void scan() => _findDevices();

  @override
  void refreshKnown(List<String> hosts) => _findDevices(hosts: hosts);

  @override
  String? checkStatusOf(String host) => _checks[host.trim().toLowerCase()];

  void _findDevices({List<String>? hosts}) {
    if (_scanning || _probing) return;
    _scanning = true;
    _operationError = null;
    _lastError = app.visibleError;
    app.clearErrors();
    final request = 'devices-${DateTime.now().microsecondsSinceEpoch}';
    _deviceRequest = request;
    notifyListeners();
    WifiNet.ensureScanPermissions().then((granted) {
      if (_dead || _deviceRequest != request) return;
      if (!granted) {
        _scanning = false;
        _deviceRequest = null;
        _operationError = 'Local network permission is required.';
        notifyListeners();
        return;
      }
      WifiNet.scanStart();
      send(hosts == null
          ? Cmds.discoverDevices(request)
          : Cmds.refreshDevices(request, hosts));
    });
    _scanTimer?.cancel();
    _scanTimer = Timer(const Duration(seconds: 10), () {
      if (_scanning && _deviceRequest == request) {
        _scanning = false;
        _deviceRequest = null;
        _operationError = 'The mixer check did not complete. Try again.';
        WifiNet.scanStop();
        notifyListeners();
      }
    });
  }

  @override
  void probe(String addr, {MixerType mixerType = MixerType.automatic}) {
    // Ignore repeated submissions without changing the pending target/type.
    if (_probing || addr.trim().isEmpty) return;
    _lastProbe = addr.trim();
    _lastMixerType = mixerType;
    unawaited(_probeLoop(_lastProbe));
  }

  /// Explicit retries keep the original driver. DL32S connects once per user
  /// attempt: replaying connect while it hydrates restarts the handshake.
  @override
  Future<void> retryProbe() => _probeLoop(_lastProbe);

  Future<void> _probeLoop(String addr) async {
    if (addr.isEmpty || _probing || _dead) return;
    final run = ++_connectionRun;
    final direct = _lastMixerType == MixerType.dl32s;
    bool cancelled() => _dead || run != _connectionRun;
    bool connected() =>
        engine.connected &&
        engine.linkIp == addr &&
        (!direct || engine.driver == 'dl32s');
    _probing = true;
    _operationError = null;
    _probeAttempt = 0;
    _connectionProgress = 'Preparing connection to $_lastProbe…';
    app.clearErrors();
    notifyListeners();
    try {
      if (direct) {
        await WifiNet.unbindWifi();
      } else {
        if (!await WifiNet.ensureScanPermissions()) {
          _operationError = 'Local network permission is required.';
          return;
        }
        await WifiNet.bindWifi();
      }
      // Direct TCP connections keep system routing, including VPN routes.
      if (cancelled()) return;
      if (InternetAddress.tryParse(addr) == null) {
        try {
          final resolved = await InternetAddress.lookup(addr)
              .timeout(const Duration(seconds: 5));
          if (resolved.isEmpty) throw StateError('No address found');
          final target = resolved
              .firstWhere((ip) => ip.type == InternetAddressType.IPv4,
                  orElse: () => resolved.first)
              .address;
          _canonicalHosts[addr.toLowerCase()] = target;
          addr = target;
        } catch (error) {
          _operationError = 'Could not resolve $_lastProbe: $error';
          return;
        }
      }
      if (cancelled()) return;
      final attempts = direct ? 1 : 3;
      for (var i = 0; i < attempts; i++) {
        _probeAttempt = i + 1;
        _connectionProgress = direct
            ? 'Connecting to $_lastProbe · waiting for mixer state (up to 90 seconds)…'
            : 'Checking $_lastProbe · attempt $_probeAttempt/3…';
        notifyListeners();
        send(direct ? Cmds.connect('dl32s', addr) : Cmds.probe(addr));
        // Remote DL32S hydration can take many round trips. Never restart a
        // live handshake automatically; only the user's Retry sends again.
        final deadline = DateTime.now().add(Duration(seconds: direct ? 90 : 3));
        while (!cancelled() && DateTime.now().isBefore(deadline)) {
          if (connected() || app.visibleError.isNotEmpty) break;
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        if (cancelled() || connected() || app.visibleError.isNotEmpty) break;
      }
      if (!cancelled() && !connected() && app.visibleError.isEmpty) {
        _operationError = direct
            ? 'Still waiting for Mackie DL32S at $_lastProbe after 90 seconds. '
                'The connection may still complete; Retry starts a new connection.'
            : 'No response from $_lastProbe after $_probeAttempt attempts.';
      }
    } catch (error) {
      if (!cancelled()) _operationError = 'Connection failed: $error';
    } finally {
      if (!cancelled()) {
        _probing = false;
        _connectionProgress = '';
        notifyListeners();
      }
    }
  }

  void _cancelConnection() {
    _connectionRun++;
    _probing = false;
    _connectionProgress = '';
  }

  @override
  void connectDevice(String id) {
    if (_probing) return;
    WifiNet.bindWifi();
    send(Cmds.connectDevice(id));
  }

  @override
  void connectSim() {
    _cancelConnection();
    send(Cmds.connectSim());
  }

  @override
  void disconnect() {
    _cancelConnection();
    send(Cmds.disconnect());
  }

  @override
  void requestInfo() => send(Cmds.info());

  @override
  void clearError() {
    _operationError = null;
    app.clearErrors();
    notifyListeners();
  }
}

// ---------------------------------------------------------------------------
// Routing
// ---------------------------------------------------------------------------

class _RoutingStore extends _StoreBase implements RoutingStore {
  _RoutingStore(super.app);

  List<Map<String, dynamic>> get _groups => [
        for (final g in engine.routingCaps['groups'] as List? ?? const [])
          if (g is Map<String, dynamic>) g,
      ];
  String _group = 'Lcl';
  int? _sel;

  @override
  List<RouteStrip> get strips => [
        for (final s in engine.strips)
          RouteStrip(
            strip: s.ch,
            id: 'in${s.ch + 1}',
            name: s.name.isEmpty ? 'CH${s.ch + 1}' : s.name,
            src: _srcOf(s.ch),
          ),
      ];

  String _srcOf(int ch) {
    for (final p in engine.routing) {
      final strip = (p['strip'] as num?)?.toInt();
      if (strip == ch) {
        final src = p['source'] ?? p['src'];
        if (src is String && src.isNotEmpty) return src;
      }
    }
    return '';
  }

  @override
  List<String> get groups => [for (final g in _groups) g['key'] as String];

  @override
  String get group =>
      groups.contains(_group) ? _group : groups.firstOrNull ?? '';

  @override
  int? get selectedStrip => _sel;

  @override
  String get selectedName {
    if (_sel == null) return 'No strip selected';
    for (final s in engine.strips) {
      if (s.ch == _sel) return s.name.isEmpty ? 'CH${s.ch + 1}' : s.name;
    }
    return 'CH${_sel! + 1}';
  }

  @override
  String get selectedSrc {
    if (_sel == null) return '';
    return _srcOf(_sel!);
  }

  @override
  List<RouteSrcTile> get srcTiles => [
        for (var i = 1;
            i <=
                ((_groups.where((g) => g['key'] == group).firstOrNull?['count']
                            as num?)
                        ?.toInt() ??
                    0);
            i++)
          RouteSrcTile(
            label: '$group $i',
            group: group,
            index: i,
            usedBy: _usedBy('$group $i'),
            current: selectedSrc == '$group $i',
          ),
      ];

  String _usedBy(String label) {
    for (final s in engine.strips) {
      if (_srcOf(s.ch) == label) {
        return s.name.isEmpty ? 'CH${s.ch + 1}' : s.name;
      }
    }
    return '';
  }

  // Honest until caps.routing is parsed into EngineState: the desk has
  // answered caps, so patch commands are at least addressable.
  @override
  bool get routingSupported => engine.routingCaps['supported'] == true;

  @override
  void selectStrip(int strip) {
    _sel = strip;
    notifyListeners();
  }

  @override
  void selectGroup(String group) {
    _group = group;
    notifyListeners();
  }

  @override
  void patch(RouteSrcTile tile) {
    if (_sel == null) return;
    send(Cmds.patchSet(_sel!, tile.group, tile.index));
  }

  @override
  void clearPatch(int strip) => send(Cmds.patchClear(strip));
}

// ---------------------------------------------------------------------------
// Scenes
// ---------------------------------------------------------------------------

class _ScenesStore extends _StoreBase implements ScenesStore {
  _ScenesStore(super.app) {
    app.addListener(_settle);
  }

  String? _show;
  bool _newShow = false;
  bool _busy = false;
  bool _submitted = false;
  String? _status;
  int _runId = 0;
  Object? _pendingContext;
  Object? _errorBefore;
  Object? _libraryBefore;
  Object Function()? _library;
  bool Function()? _confirmed;
  String? _done;

  // Libraries belong to OpenFader, not the desk's native show storage.
  // EngineState replaces these collections for each scenes/shows event.
  // Requiring a fresh collection prevents submission or an old matching
  // library entry from being treated as a completed mutation.
  Object get _context => (engine.session, _show, _newShow);
  Object get _errorContext =>
      (app.errorRevision, app.visibleError, app.visibleErrorAt, engine.error);
  bool get _canRun => !_dead && !_busy && online;

  @override
  Object get actionContext => (_context, show, engine.scenes, engine.shows);

  @override
  bool validateActionContext(Object captured) {
    if (_dead) return false;
    if (!_canRun || captured != actionContext) {
      _status = 'Scene action cancelled: the connection, selection, or '
          'library changed. Review the target and try again.';
      notifyListeners();
      return false;
    }
    return true;
  }

  @override
  bool get busy => _busy;

  @override
  String? get status {
    final error = app.visibleError;
    if (error.isEmpty) return _status;
    if (_status == null || _status!.contains(error)) return _status ?? error;
    return '$error\n$_status';
  }

  void _finish(String message) {
    _runId++;
    _busy = false;
    _pendingContext = null;
    _library = null;
    _libraryBefore = null;
    _confirmed = null;
    _done = null;
    _status = message;
    if (!_dead) notifyListeners();
  }

  void _settle() {
    if (_dead || !_busy) return;
    if (!online || _pendingContext != _context) {
      _finish('Connection or selection changed. The scene operation result '
          'is unconfirmed; it may already have taken effect.');
    } else if (app.visibleError.isNotEmpty && _errorBefore != _errorContext) {
      _finish('Scene operation result unconfirmed: ${app.visibleError}');
    } else if (_submitted &&
        _library != null &&
        !identical(_libraryBefore, _library!()) &&
        _confirmed!()) {
      _finish(_done!);
    }
  }

  Future<void> _run(String command, String pending, String done,
      {Object Function()? library, bool Function()? confirmed}) async {
    if (!_canRun) return;
    final run = ++_runId;
    _busy = true;
    _submitted = false;
    _pendingContext = _context;
    _errorBefore = _errorContext;
    _library = library;
    _libraryBefore = library?.call();
    _confirmed = confirmed;
    _done = done;
    _status = pending;
    // Keep existing engine/transport errors visible, including during a retry.
    notifyListeners();
    try {
      if (_dead || run != _runId) return;
      _settle();
      if (_dead || run != _runId) return;
      await app.sendCommand(command);
      if (_dead || run != _runId) return;
      _submitted = true;
      _settle();
      if (_dead || run != _runId) return;
      // Named recall has no correlated completion event in EngineState.
      // Its receipt only permits "requested", never a mixer-value claim.
      if (library == null) _finish(done);
      // No per-poll deadline hook is exposed to this store. A missing library
      // update stays pending on the existing structural listener; no timer
      // or extra poller, and no success inferred from command submission.
    } catch (error) {
      if (!_dead && run == _runId) {
        _finish('Scene command failed; result unconfirmed: $error');
      }
    }
  }

  @override
  void dispose() {
    _runId++;
    app.removeListener(_settle);
    super.dispose();
  }

  @override
  List<SceneEntryVm> get entries => [
        for (final s in engine.scenes)
          SceneEntryVm(number: s.number, name: s.name, current: s.current),
      ];

  @override
  NativeSnapshotsVm get nativeSnapshots => engine.nativeSnapshots;

  @override
  List<String> get shows => engine.shows.keys.toList();

  @override
  List<String> get showScenes => engine.shows[show] ?? const [];

  @override
  String? get show => _newShow
      ? null
      : engine.shows.containsKey(_show)
          ? _show
          : shows.firstOrNull;

  @override
  bool get online => engine.connected;

  // OpenFader's engine library explicitly supports 001..999. This domain
  // does not describe manufacturer mixer software's native show/snapshot capacity.
  int? _freeSlot() {
    final used = {for (final s in engine.scenes) s.number};
    for (var n = 1; n <= 999; n++) {
      if (!used.contains(n)) return n;
    }
    return null;
  }

  @override
  void save(String name) {
    if (!_canRun || name.trim().isEmpty) return;
    final slot = _freeSlot();
    if (slot == null) {
      _status = 'OpenFader snapshot library full (001–999). Delete a snapshot '
          'before saving; nothing was overwritten.';
      notifyListeners();
      return;
    }
    name = name.trim();
    unawaited(_run(
        Cmds.sceneSave(slot, name),
        'Saving snapshot $slot… Awaiting engine library confirmation.',
        'Snapshot $slot is present in the OpenFader engine library.',
        library: () => engine.scenes,
        confirmed: () =>
            engine.scenes.any((s) => s.number == slot && s.name == name)));
  }

  @override
  void recall(int number) {
    if (!engine.scenes.any((s) => s.number == number)) return;
    unawaited(_run(
        Cmds.sceneRecall(number),
        'Snapshot $number recall requested… Awaiting engine library marker.',
        'Engine library marked snapshot $number current. Recall accepted; '
            'mixer value verification is unavailable here.',
        library: () => engine.scenes,
        confirmed: () =>
            engine.scenes.any((s) => s.number == number && s.current)));
  }

  @override
  void rename(int number, String name) {
    name = name.trim();
    if (name.isEmpty || !engine.scenes.any((s) => s.number == number)) return;
    unawaited(_run(
        Cmds.sceneRename(number, name),
        'Renaming snapshot $number… Awaiting engine library confirmation.',
        'Snapshot $number name confirmed in the OpenFader engine library.',
        library: () => engine.scenes,
        confirmed: () =>
            engine.scenes.any((s) => s.number == number && s.name == name)));
  }

  @override
  void remove(int number) {
    if (!engine.scenes.any((s) => s.number == number)) return;
    unawaited(_run(
        Cmds.sceneDelete(number),
        'Deleting snapshot $number… Awaiting engine library confirmation.',
        'Snapshot $number removed from the OpenFader engine library.',
        library: () => engine.scenes,
        confirmed: () => !engine.scenes.any((s) => s.number == number)));
  }

  @override
  void clearAll() {
    if (engine.scenes.isEmpty) return;
    unawaited(_run(
        Cmds.sceneClear(),
        'Clearing snapshots… Awaiting engine library confirmation.',
        'OpenFader numbered snapshot library is empty.',
        library: () => engine.scenes,
        confirmed: () => engine.scenes.isEmpty));
  }

  @override
  void selectShow(String show) {
    if (_dead || _busy) return;
    _newShow = false;
    _show = show;
    send(Cmds.listShows());
    notifyListeners();
  }

  @override
  void beginShow() {
    if (_dead || _busy) return;
    _newShow = true;
    notifyListeners();
  }

  @override
  bool captureWillOverwrite(String name) =>
      engine.shows[show ?? name]?.contains(name) ?? false;

  @override
  void captureShow(String name) {
    name = name.trim();
    if (!_canRun || name.isEmpty) return;
    final selected = show ?? name;
    final overwrite = captureWillOverwrite(name);
    _newShow = false;
    _show = selected;
    unawaited(_run(
        Cmds.captureScene(selected, name),
        'Capturing “$name” in “$selected”… Awaiting engine library confirmation.',
        overwrite
            ? 'Engine library refreshed for “$selected” / “$name”. '
                'Replacement contents are not verified here.'
            : 'Scene “$name” is listed in OpenFader show “$selected”.',
        library: () => engine.shows,
        confirmed: () => engine.shows[selected]?.contains(name) ?? false));
  }

  @override
  void recallShow(String name) {
    final selected = show;
    if (selected == null ||
        !(engine.shows[selected]?.contains(name) ?? false)) {
      return;
    }
    unawaited(_run(
        Cmds.recallScene(selected, name),
        'Requesting recall of “$selected” / “$name”…',
        'Recall requested for “$selected” / “$name”. '
            'Completion and mixer value verification are unavailable here.'));
  }

  @override
  void deleteShow(String name) {
    final selected = show;
    if (selected == null ||
        !(engine.shows[selected]?.contains(name) ?? false)) {
      return;
    }
    unawaited(_run(
        Cmds.deleteScene(selected, name),
        'Deleting “$selected” / “$name”… Awaiting engine library confirmation.',
        'Scene “$name” removed from OpenFader show “$selected”.',
        library: () => engine.shows,
        confirmed: () => !(engine.shows[selected]?.contains(name) ?? false)));
  }
}

// ---------------------------------------------------------------------------
// Settings: prefs + recents + local mixes over platform services.
// ---------------------------------------------------------------------------

class _SettingsStore extends _StoreBase implements SettingsStore {
  _SettingsStore(super.app) {
    prefs.addListener(_forward);
    recents.addListener(_forward);
    mixes.addListener(_forward);
    recents.load();
    mixes.refresh();
  }

  @override
  void dispose() {
    prefs.removeListener(_forward);
    recents.removeListener(_forward);
    mixes.removeListener(_forward);
    super.dispose();
  }

  @override
  PrefsService get prefs => PrefsService.instance;

  final RecentMixers _recents = RecentMixers.instance;

  @override
  RecentMixers get recents {
    RecentMixers.ensureLoaded();
    return _recents;
  }

  final LocalMixStore _mixes = LocalMixStore();

  @override
  LocalMixStore get mixes => _mixes;

  @override
  Future<int> appStorageBytes() async =>
      await _mixes.storageBytes() + await app.processorPresetStorageBytes();

  @override
  Future<String?> importMix() async {
    final err = await _mixes.importMix();
    if (err == null) {
      await _snapshotBackup();
      notifyListeners();
    }
    return err;
  }

  @override
  Future<void> exportMix(LocalMix mix) => _mixes.exportMix(mix);

  @override
  Future<void> deleteMix(LocalMix mix) async {
    await _mixes.deleteMix(mix);
    await _snapshotBackup();
    notifyListeners();
  }

  @override
  Future<String> importBackup() async {
    final res = await _mixes.importBackup();
    if (res != 'cancelled') {
      await _snapshotBackup();
      notifyListeners();
    }
    return res;
  }

  @override
  Future<void> exportBackup() => _mixes.exportBackup();

  Future<void> _snapshotBackup() async {
    if (!prefs.autoBackup) return;
    try {
      await _mixes.snapshotBackup();
      await prefs.setBackupCreatedAt(DateTime.now().toIso8601String());
    } catch (error) {
      throw StateError(
          "The mix change was saved, but its automatic backup failed: $error");
    }
  }

  @override
  Future<void> clearCache() async {
    await _mixes.clearCache();
    notifyListeners();
  }

  @override
  Future<void> forgetMixers() async {
    await _recents.clear();
    notifyListeners();
  }

  @override
  Future<void> resetPrefs() async {
    await prefs.resetToDefaults();
    notifyListeners();
  }

  @override
  Future<void> deleteMixes() async {
    await _mixes.deleteAll();
    notifyListeners();
  }

  @override
  Future<void> fullReset() async {
    await app.clearProcessorPresets();
    await _mixes.deleteAll();
    await _mixes.clearCache();
    await _recents.clear();
    await prefs.clearAll();
    notifyListeners();
  }

  @override
  Future<void> exportDiagnostics() async {
    final payload = {
      'app': 'OpenFader',
      'generatedAt': DateTime.now().toIso8601String(),
      'preferences': {
        'autoFind': prefs.autoFind,
        'reconnectLast': prefs.reconnectLast,
        'rememberRecents': prefs.rememberRecents,
        'startupDest': prefs.startupDest,
        'interfaceSize': prefs.interfaceSize,
        'keepAwake': prefs.keepAwake,
        'meterRate': prefs.meterRate,
        'peakHold': prefs.peakHold,
        'holdClips': prefs.holdClips,
        'largeTargets': prefs.largeTargets,
        'autoBackup': prefs.autoBackup,
      },
      'localMixCount': _mixes.mixes.length,
      'savedMixerCount': _recents.items.length,
    };
    final tmp = Directory.systemTemp;
    final file = File('${tmp.path}/open-fader-diagnostics.json');
    await file.writeAsString(jsonEncode(payload));
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    final size = view.physicalSize / view.devicePixelRatio;
    final result = await SharePlus.instance.share(ShareParams(
        files: [XFile(file.path)],
        subject: 'open-fader-diagnostics.json',
        sharePositionOrigin:
            Rect.fromLTWH(size.width / 2, size.height / 2, 1, 1)));
    if (result.status == ShareResultStatus.dismissed) {
      throw MixExportCancelled();
    }
    if (result.status != ShareResultStatus.success) {
      throw StateError(
          'The platform could not confirm the diagnostics export.');
    }
  }
}

// ---------------------------------------------------------------------------
// FX (port of web fxModel FX_TYPES + generic param knobs)
// ---------------------------------------------------------------------------

class _FxStore extends _StoreBase implements FxStore {
  _FxStore(super.app) {
    // FX meters ride the single shell-owned poll loop: every AppStore drain
    // re-syncs the slot notifiers in place, so the FX page needs no timer.
    app.addListener(_syncFxMeters);
  }

  int _sel = 0;
  final Map<int, ValueNotifier<double>> _meters = {};

  @override
  void dispose() {
    app.removeListener(_syncFxMeters);
    for (final m in _meters.values) {
      m.dispose();
    }
    super.dispose();
  }

  List<FxSlot> get _raw => engine.fx;

  @override
  List<FxSlotVm> get slots => [
        for (final s in _raw)
          FxSlotVm(
            slot: s.slot,
            name: 'FX ${s.slot}',
            type: s.type.isEmpty ? '—' : s.type,
            on: s.on,
            bypass: s.bypass,
            ret: s.returnLabel,
          ),
      ];

  @override
  int get selected => _sel.clamp(0, _raw.isEmpty ? 0 : _raw.length - 1);

  @override
  List<String> get typeOptions => [
        for (final type in engine.fxTypes)
          (type['id'] ?? type['label']) as String
      ];

  FxSlot? get _cur =>
      _raw.isEmpty ? null : _raw[selected.clamp(0, _raw.length - 1)];

  @override
  List<FxKnobVm> get knobs {
    final c = _cur;
    if (c == null) return const [];
    final ps = c.params.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));
    return [
      for (final e in ps)
        FxKnobVm(
          id: e.key,
          label: e.key,
          min: 0,
          max: 1,
          value: e.value.clamp(0.0, 1.0),
          text: '${(e.value.clamp(0.0, 1.0) * 100).toStringAsFixed(0)}%',
        ),
    ];
  }

  @override
  bool get tapVisible => (_cur?.type ?? '').toLowerCase().contains('delay');

  @override
  String get tapLabel => 'TAP';

  @override
  ValueNotifier<double> meterOf(int slot) {
    final n = _meters.putIfAbsent(slot, () => ValueNotifier(0));
    final i = _raw.indexWhere((s) => s.slot == slot);
    if (i >= 0 && i < engine.meters.length) {
      n.value = _raw[i].level.clamp(0.0, 1.0);
    }
    return n;
  }

  @override
  Future<void> pollMeters() async {
    await app.pollMeters();
    _syncFxMeters();
  }

  void _syncFxMeters() {
    if (_dead) return;
    for (final e in _meters.entries) {
      final i = _raw.indexWhere((s) => s.slot == e.key);
      if (i >= 0 && i < engine.meters.length) {
        e.value.value = _raw[i].level.clamp(0.0, 1.0);
      }
    }
  }

  @override
  void selectSlot(int slot) {
    final i = _raw.indexWhere((s) => s.slot == slot);
    _sel = i < 0 ? slot : i;
    notifyListeners();
  }

  @override
  void setType(String type) {
    final c = _cur;
    if (c == null) return;
    send(Cmds.fxSetType(c.slot, type));
  }

  @override
  void setParam(String id, double value) {
    final c = _cur;
    if (c == null) return;
    send(Cmds.fxParam(c.slot, id, value));
  }

  @override
  void setOn(bool on) {
    final c = _cur;
    if (c == null) return;
    send(Cmds.fxOn(c.slot, on));
  }

  @override
  void setBypass(bool bypass) {
    final c = _cur;
    if (c == null) return;
    send(Cmds.fxBypass(c.slot, bypass));
  }

  // No tap-tempo cmd in the engine protocol; the TAP tile is shown for
  // Delay types (web parity) but stays a no-op until the protocol grows one.
  @override
  void tap() {}
}

// ---------------------------------------------------------------------------
// Debug
// ---------------------------------------------------------------------------

class _DebugStore extends _StoreBase implements DebugStore {
  _DebugStore(super.app);

  int _tick = 0;

  @override
  int get tick => _tick;

  @override
  List<double> get spark => List.of(engine.meters);

  @override
  List<DebugSection> get sections => [
        DebugSection(title: 'link', lines: [
          'client=${engine.clientId}',
          'driver=${engine.driver}',
          'state=${engine.linkState} name=${engine.linkName}',
          'strips=${engine.strips.length} meters=${engine.meters.length}',
        ]),
        DebugSection(title: 'strips', lines: [
          for (final s in engine.strips.take(8))
            'ch${s.ch} ${s.key} "${s.name}" f=${s.fader.toStringAsFixed(3)} m=${s.muted}',
          if (engine.strips.length > 8) '… +${engine.strips.length - 8} more',
        ]),
        DebugSection(title: 'caps', lines: [
          'ready=${engine.capsReady} params=${engine.supportedParams.length}',
          ...engine.supportedParams.take(12),
        ]),
        DebugSection(title: 'live', lines: [
          'main_fader=${engine.mainFader} main_muted=${engine.mainMuted}',
          'solos=${engine.solos} locks=${engine.locks}',
          'scenes=${engine.scenes.length} fx=${engine.fx.length} routing=${engine.routing.length}',
          'gr=${engine.grGate.length}/${engine.grComp.length} spectrum=${engine.spectrum.length}',
        ]),
        if (engine.error.isNotEmpty)
          DebugSection(title: 'error', lines: [engine.error]),
      ];

  @override
  Future<void> refresh() async {
    captureFrame();
  }

  /// Observe the shell's completed drain; debug must not own another poller.
  void captureFrame() {
    _tick++;
    notifyListeners();
  }

  @override
  void sendHello() => send(Cmds.hello());

  @override
  void sendStats() => send(Cmds.stats());

  @override
  void nudgeFader() {
    var f = 0.75;
    for (final s in engine.strips) {
      if (s.ch == 0) f = s.fader;
    }
    send(Cmds.fader(0, (f + 0.05).clamp(0.0, 1.0)));
  }

  @override
  void toggleMute() {
    var m = false;
    for (final s in engine.strips) {
      if (s.ch == 0) m = s.muted;
    }
    send(Cmds.mute(0, !m));
  }

  @override
  void scan() => send(Cmds.scan());

  @override
  void requestInfo() => send(Cmds.info());
}
