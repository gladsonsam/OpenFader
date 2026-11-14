// Shell navigation and composition. Responsibility-specific store adapters live
// beside this router and bind screens to AppStore through constructor injection.
// The shell owns the single shared 80 ms poll loop so every tab stays fresh.
// FX labels and engineering readouts come from provider caps.
// Stats frames are not yet parsed into EngineState.
library;

import '../boot/host_transport.dart';
import '../boot/browser_session.dart';
import '../boot/remote_host.dart';
import '../desktop/desktop_integration.dart';
import '../ui/overlays/console_dialogs.dart';

import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';
import '../palette.dart';
import '../mixer/geometry.dart';

import '../design.dart';
import '../ui/motion.dart';
import '../engine.dart';
import '../ffi_engine.dart';
import '../mixer/components/meter.dart';
import '../mixer/components/toolbar.dart';
import '../core/selected_control_readout.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../state/app_store.dart';
import '../u4law.dart' show panText;
import 'shell.dart';
import 'marketing_fixture.dart';
import 'workspace_home.dart';
import '../screens/channel.dart';
import '../screens/output_processing.dart';
import '../mixer/components/banks.dart';
import '../mixer/components/strip.dart';
import '../screens/channel_toolbar.dart';
import '../screens/connection.dart';
import '../screens/debug.dart';
import '../screens/console_controls.dart';
import '../screens/fx.dart';
import '../screens/meters.dart';
import '../screens/start.dart';
import '../mixer/controller.dart';
import '../mixer/page.dart';
import '../screens/routing.dart';
import '../screens/recorder.dart';
import 'recorder_store.dart';
import '../screens/scenes.dart';
import '../screens/settings.dart';

import 'connection_store.dart';
import 'routing_store.dart';
import 'scenes_store.dart';
import 'settings_store.dart';
import 'fx_store.dart';
import 'debug_store.dart';
import 'console_controls_store.dart';

enum AppTab {
  home('Tools'),
  mixer('Mixer'),
  recorder('Recorder'),
  meters('Meters'),
  channel('Channel'),
  scenes('Scenes'),
  fx('FX'),
  routing('Routing'),
  connect('Connection'),
  settings('Settings'),
  debug('Diagnostics'),
  consoleControls('Console controls');

  const AppTab(this.label);
  final String label;
}

class AppShell extends StatefulWidget {
  final DesktopIntegration? desktop;
  final EngineApi api;
  final AppStore store;
  final AppTab initialTab;

  // True when main won the real engine; false is the loud FAKE sim fallback.

  final bool isRealEngine;

  const AppShell(
      {super.key,
      this.desktop,
      required this.api,
      required this.store,
      this.initialTab = AppTab.mixer,
      required this.isRealEngine});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> with WidgetsBindingObserver {
  late AppTab _tab;

  // Soft-close resume: host stashed while backgrounded, verdict after return.
  Timer? _resumeVerdict;
  int _resumeGen = 0;
  String? _resumeHost;
  double _resumeWallMs = 0;

  // Where Settings Close returns.
  AppTab _prevTab = AppTab.mixer;
  Timer? _shellLoop;
  final _channelFocus = ValueNotifier<ChannelFocus?>(null);
  final _fxReadout = ValueNotifier<SelectedControlReadoutData?>(null);
  Object? _fxReadoutSession;
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
  bool _canReturnFromStart = false;

  late final MixerController _mixer;
  late final ShellConnectionStore _conn;
  late final ShellRoutingStore _routing;
  late final ShellScenesStore _scenes;
  late final ShellSettingsStore _settings;
  late final ShellFxStore _fx;
  late final ShellDebugStore _debug;
  late final ShellConsoleControlsStore _consoleControls;
  late final ShellRecorderStore _recorder;
  bool _desktopQuitting = false;

  @override
  void initState() {
    super.initState();
    final host = widget.api;
    _recorder = ShellRecorderStore(
        host: host is HostTransport ? host : null,
        names: () => {
              for (final strip in widget.store.assignmentStrips)
                if (strip.ch >= 0 && strip.name.isNotEmpty)
                  strip.ch: strip.name
            });
    widget.desktop?.attach(this, _prepareDesktopQuit);
    widget.store.addListener(_updateDesktopStatus);
    _recorder.addListener(_updateDesktopStatus);
    _updateDesktopStatus();
    WidgetsBinding.instance.addObserver(this);
    _tab = widget.initialTab;
    _syncChannelFocus();
    widget.store.addListener(_syncChannelFocus);
    _showStart = widget.api is! RemoteHost &&
        PrefsService.instance.startupDest != 'last';
    _mixer = MixerController(widget.store);
    _conn = ShellConnectionStore(widget.store);
    _routing = ShellRoutingStore(widget.store);
    _scenes = ShellScenesStore(widget.store);
    _settings = ShellSettingsStore(widget.store);
    _fx = ShellFxStore(widget.store);
    _debug = ShellDebugStore(widget.store);
    _consoleControls = ShellConsoleControlsStore(widget.store);
    _applyMeterTuning();
    PrefsService.instance.addListener(_onPrefs);
    // Single shared ticker: the only poll loop in the app because pages own no timer.

    _restartLoop();
    _tick();
    // Auto-reconnect stays off unless the preference enables it.

    final prefs = PrefsService.instance;
    if (widget.api is! RemoteHost &&
        prefs.reconnectLast &&
        prefs.reconnectArmed &&
        prefs.lastMixerAddr.isNotEmpty) {
      // A settled cold-start handshake skips the front door: the user asked
      // to keep mixing, not to tap Connect again after every relaunch.
      _coldReconnectAddr = prefs.lastMixerAddr;
      widget.store.addListener(_watchColdReconnect);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        unawaited(_reconnectLast(prefs.lastMixerAddr));
      });
    }
  }

  void _updateDesktopStatus() {
    final engine = widget.store.engine;
    final recorder = _recorder.state['transport'];
    widget.desktop?.updateStatus([
      engine.connected ? 'Connected · ${engine.driver}' : 'Disconnected',
      if (recorder == 'recording') 'Recording',
      if (recorder == 'playing') 'Playing',
    ].join(' · '));
  }

  Future<bool> _prepareDesktopQuit() async {
    try {
      final api = widget.api;
      if (api is HostTransport && api.supportsAdministration) {
        if (_recorder.pending) {
          throw StateError(
              'Wait for the recorder operation to finish before quitting.');
        }
        // Read the host directly: the window may have been hidden for hours,
        // or a paired client may have started recording since the last frame.
        var state = await api.recorderRequest('state');
        if (state['error'] != null) throw StateError('${state['error']}');
        if (state['soundcheck_pending'] == true ||
            (state['input_mode'] == 'recovery_required' &&
                state['manual_routing'] != true)) {
          throw StateError(
              'Return the recorder to live mode and resolve pending routing recovery before quitting.');
        }
        if (state['manual_routing'] == true) {
          if (!mounted) return false;
          final stop = await showConsoleConfirmation(context,
              title: 'Stop manual soundcheck?',
              message:
                  'Playback will stop. You will then need to restore the mixer’s live input routing before quitting.',
              confirmLabel: 'Stop output',
              danger: true);
          if (!stop || !mounted) return false;
          await _recorder.command('stop_manual_output');
          if (_recorder.error != null) throw StateError(_recorder.error!);
          state = await api.recorderRequest('state');
          if (state['transport'] != 'stopped') {
            throw StateError('Playback output could not be confirmed stopped.');
          }
          if (!mounted) return false;
          final restored = await showConsoleConfirmation(context,
              title: 'Restore live inputs',
              message:
                  'Playback is stopped. Restore physical live routing on the mixer, then confirm. OpenFader does not change mixer routing in manual mode.',
              confirmLabel: 'Live routing restored');
          if (!restored || !mounted) return false;
          await _recorder
              .command('return_manual_live', {'routing_restored': true});
          if (_recorder.error != null) throw StateError(_recorder.error!);
          state = await api.recorderRequest('state');
          if (state['manual_routing'] == true) {
            throw StateError('Manual routing recovery remains active.');
          }
        }
        final transport = state['transport'];
        final needsStop = transport == 'recording' ||
            transport == 'playing' ||
            transport == 'paused';
        final soundcheck = state['input_mode'] == 'soundcheck';
        if (needsStop || soundcheck) {
          if (!mounted) return false;
          final confirmed = await showConsoleConfirmation(context,
              title: transport == 'recording'
                  ? 'Stop recording and quit?'
                  : 'Stop recorder and quit?',
              message: transport == 'recording'
                  ? 'The recording will be stopped and the take finalized before OpenFader quits.'
                  : 'Playback will stop and the mixer will return to live inputs before OpenFader quits.',
              confirmLabel: 'Stop and quit',
              danger: true);
          if (!confirmed || !mounted) return false;
          await _recorder.command('stop');
          if (_recorder.error != null) throw StateError(_recorder.error!);
          state = await api.recorderRequest('state');
          if (state['input_mode'] == 'soundcheck') {
            await _recorder.command('return_live');
            if (_recorder.error != null) throw StateError(_recorder.error!);
          }
        }
        // Stop UI polling before requesting shutdown: the desktop transport
        // otherwise reconnects automatically when its WebSocket closes.
        _desktopQuitting = true;
        _shellLoop?.cancel();
        _resumeVerdict?.cancel();
        if (!await _recorder.quitHost()) {
          throw StateError(_recorder.error ??
              'The recorder host could not confirm shutdown.');
        }
      } else {
        _desktopQuitting = true;
        _shellLoop?.cancel();
        _resumeVerdict?.cancel();
        // A remote host belongs to another computer and must remain running.
        if (api is RemoteHost) {
          await api.disconnect();
        } else {
          await widget.store.sendCommand(Cmds.disconnect());
        }
      }
      return true;
    } catch (error) {
      _desktopQuitting = false;
      _restartLoop();
      if (mounted) {
        await showDialog<void>(
            context: context,
            builder: (context) => ConsoleAlertDialog(
                  title: const Text('OpenFader is still running'),
                  content: Text('$error'),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(context),
                        child: const Text('OK'))
                  ],
                ));
      }
      return false;
    }
  }

  String? _coldReconnectAddr;
  bool _coldReconnectEntered = false;

  void _watchColdReconnect() {
    final addr = _coldReconnectAddr;
    if (addr == null || _coldReconnectEntered || !mounted) return;
    final engine = widget.store.engine;
    if (!engine.connected || engine.driver == 'sim' || engine.linkIp.isEmpty) {
      return;
    }
    if (_conn.canonicalHost(engine.linkIp) == _conn.canonicalHost(addr)) {
      _coldReconnectEntered = true;
      _coldReconnectAddr = null;
      widget.store.removeListener(_watchColdReconnect);
      setState(() => _showStart = false);
    }
  }

  Future<void> _reconnectLast(String addr) async {
    final run = _conn.connectionRun;
    final driver = await RecentMixers.savedDriverFor(addr);
    if (mounted && run == _conn.connectionRun) {
      if (driver == MixerType.server.driver) {
        _coldReconnectAddr = null;
        widget.store.removeListener(_watchColdReconnect);
        await _connectServer(addr);
      } else {
        _conn.probe(addr, mixerType: MixerType.fromDriver(driver));
      }
    }
  }

  // Soft-close resume: iOS suspends the process, killing sockets and desk
  // subscriptions. Returning is an explicit intent to keep mixing, so a
  // previously live hardware session re-establishes itself without asking.
  // Healthy sessions are untouched (traffic since resume proves it); only a
  // stale or dropped link re-probes. Explicit disconnects never retry.
  // The verdict wait is short on purpose: a dead link should redial in
  // under a second, and the traffic check below still guards healthy ones.
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (widget.desktop != null || widget.api is RemoteHost) return;
    if (state == AppLifecycleState.paused) {
      _resumeVerdict?.cancel();
      final engine = widget.store.engine;
      if (engine.connected &&
          engine.driver != 'sim' &&
          engine.linkIp.isNotEmpty) {
        _resumeHost = engine.linkIp;
        _resumeWallMs = DateTime.now().microsecondsSinceEpoch / 1000.0;
      } else {
        _resumeHost = null;
      }
      return;
    }
    if (state != AppLifecycleState.resumed) return;
    final host = _resumeHost;
    _resumeHost = null;
    if (host == null || host.isEmpty || !mounted) return;
    if (widget.store.engine.driver == 'sim') return;
    // Cheap liveness probe through the normal command path first.
    _conn.requestInfo();
    final gen = ++_resumeGen;
    final resumeWall = _resumeWallMs;
    _resumeVerdict?.cancel();
    _resumeVerdict = Timer(const Duration(milliseconds: 800),
        () => _verdictResume(gen, host, resumeWall));
  }

  Future<void> _verdictResume(int gen, String host, double resumeWall) async {
    if (!mounted || gen != _resumeGen) return;
    final store = widget.store;
    if (_conn.probing || store.engine.linkState == 'reconnecting') return;
    // An explicit disconnect between suspend and verdict wins over retry.
    if (store.engine.linkState == 'disconnected') return;
    // Traffic since resume proves the session survived suspension untouched.
    if (store.engine.connected && store.lastTrafficMs >= resumeWall) return;
    final driver = await RecentMixers.savedDriverFor(host);
    if (!mounted || gen != _resumeGen || _conn.probing) return;
    _conn.probe(host, mixerType: MixerType.fromDriver(driver));
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
    if (_desktopQuitting) return;
    final ms = (1000 / PrefsService.instance.meterRate).round().clamp(16, 250);
    _shellLoop = Timer.periodic(Duration(milliseconds: ms), (_) => _tick());
  }

  Future<void> _tick() async {
    if (_desktopQuitting) return;
    try {
      await widget.store.pollMeters();
      _fx.syncMeters();
      _conn.tickRecovery();
      if (widget.api is HostTransport) _recorder.tick();
      if (mounted && _tab == AppTab.debug) _debug.captureFrame();
      if (mounted && _tab == AppTab.consoleControls) _consoleControls.poll();
    } catch (_) {
      // A transport hiccup must not crash the shell loop.
    }
  }

  @override
  void dispose() {
    widget.desktop?.detach(this);
    widget.store.removeListener(_updateDesktopStatus);
    _recorder.removeListener(_updateDesktopStatus);
    WidgetsBinding.instance.removeObserver(this);
    widget.store.removeListener(_watchColdReconnect);
    _resumeVerdict?.cancel();
    _shellLoop?.cancel();
    PrefsService.instance.removeListener(_onPrefs);
    widget.store.removeListener(_syncChannelFocus);
    _mixer.dispose();
    _channelFocus.dispose();
    _fxReadout.dispose();
    _conn.dispose();
    _routing.dispose();
    _scenes.dispose();
    _settings.dispose();
    _fx.dispose();
    _debug.dispose();
    _consoleControls.dispose();
    _recorder.dispose();
    super.dispose();
  }

  bool _serverConnecting = false;
  String? _serverConnectionError;

  Future<void> _connectServer(String address) async {
    if (_serverConnecting) return;
    setState(() {
      _serverConnecting = true;
      _serverConnectionError = null;
    });
    RemoteHost? host;
    AppStore? remoteStore;
    try {
      final endpoint = RemoteHost.parseAddress(address);
      final prefs = await SharedPreferences.getInstance();
      final server = await RemoteHost.inspect(address,
          trustedFingerprint: prefs.getString(RemoteHost.pinKey(endpoint)));
      if (!mounted) return;
      if (server.identityChanged) {
        final trust = await showDialog<bool>(
            context: context,
            builder: (context) => ConsoleAlertDialog(
                  title: const Text('Server identity changed'),
                  content: SelectableText(
                      'The server at ${server.endpoint.host} has a different TLS identity. Verify that you are connecting to the intended server before continuing.\n\nPrevious: ${server.previousFingerprint}\n\nCurrent: ${server.fingerprint}'),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(context, false),
                        child: const Text('Cancel')),
                    TextButton(
                        onPressed: () => Navigator.pop(context, true),
                        child: const Text('Trust this server')),
                  ],
                ));
        if (trust != true || !mounted) return;
      }
      host = await _authenticateServer(server);
      if (host == null) return;
      if (!await prefs.setString(
          RemoteHost.pinKey(endpoint), server.fingerprint)) {
        throw StateError('Could not save server identity');
      }
      final id = await host.boot();
      remoteStore = AppStore(host, id);
      await remoteStore.attach(id);
      if (remoteStore.transportError.isNotEmpty) {
        throw StateError(remoteStore.transportError);
      }
      remoteStore.setOnline(true);
      await remoteStore.pollMeters();
      if (remoteStore.transportError.isNotEmpty) {
        throw StateError(remoteStore.transportError);
      }
      if (!mounted) return;
      final prefsService = PrefsService.instance;
      if (prefsService.rememberRecents) {
        await RecentMixers.instance.remember(
            'OpenFader server', endpoint.toString(),
            driver: MixerType.server.driver);
      }
      await RecentMixers.rememberLastConnection(
          endpoint.toString(), MixerType.server.driver);
      await prefsService.setLastMixerAddr(endpoint.toString());
      await prefsService.setReconnectArmed(true);
      if (!mounted) return;
      _shellLoop?.cancel();
      await Navigator.of(context).push(MaterialPageRoute<void>(
          builder: (_) =>
              AppShell(api: host!, store: remoteStore!, isRealEngine: true)));
    } catch (error) {
      if (mounted) {
        setState(() =>
            _serverConnectionError = 'Could not connect to server: $error');
      }
    } finally {
      remoteStore?.dispose();
      await host?.disconnect();
      _serverConnecting = false;
      if (mounted) _restartLoop();
    }
  }

  Future<RemoteHost?> _authenticateServer(RemoteServer server) async {
    if (!server.passwordRequired) {
      return RemoteHost.connect(server,
          trustIdentityChange: server.identityChanged);
    }
    final password = TextEditingController();
    var pending = false;
    String? error;
    try {
      return await showDialog<RemoteHost>(
          context: context,
          barrierDismissible: false,
          builder: (dialogContext) =>
              StatefulBuilder(builder: (context, update) {
                Future<void> connect() async {
                  if (pending) return;
                  update(() {
                    pending = true;
                    error = null;
                  });
                  try {
                    final host = await RemoteHost.connect(server,
                        password: password.text,
                        trustIdentityChange: server.identityChanged);
                    if (dialogContext.mounted) {
                      Navigator.pop(dialogContext, host);
                    } else {
                      await host.disconnect();
                    }
                  } catch (failure) {
                    if (dialogContext.mounted) {
                      update(() {
                        error = failure.toString();
                        pending = false;
                      });
                    }
                  }
                }

                return PopScope(
                    canPop: !pending,
                    child: ConsoleAlertDialog(
                      title: const Text('Server password'),
                      content:
                          Column(mainAxisSize: MainAxisSize.min, children: [
                        Text('Enter the password for ${server.endpoint.host}.'),
                        TextField(
                            controller: password,
                            autofocus: true,
                            obscureText: true,
                            enabled: !pending,
                            enableSuggestions: false,
                            autocorrect: false,
                            textInputAction: TextInputAction.go,
                            decoration: InputDecoration(
                                labelText: 'Password', errorText: error),
                            onSubmitted: (_) => connect()),
                      ]),
                      actions: [
                        TextButton(
                            onPressed: pending
                                ? null
                                : () => Navigator.pop(dialogContext),
                            child: const Text('Cancel')),
                        TextButton(
                            onPressed: pending ? null : connect,
                            child: Text(pending ? 'Connecting…' : 'Connect')),
                      ],
                    ));
              }));
    } finally {
      password.dispose();
    }
  }

  void _go(AppTab t) {
    if (t != _tab) {
      _channelFocus.value = null;
      _fxReadout.value = null;
      _fxReadoutSession = null;
    }
    if (t == AppTab.scenes) widget.store.refreshLocalLibrary();
    setState(() {
      if (t == AppTab.settings && _tab != AppTab.settings) {
        _prevTab = _tab;
      }
      _tab = t;
      if (t != AppTab.settings) _showStart = false;
    });
  }

  void _openConnectionsHome({bool canReturn = true}) {
    _channelFocus.value = null;
    _fxReadout.value = null;
    _fxReadoutSession = null;
    setState(() {
      _tab = AppTab.mixer;
      _canReturnFromStart = canReturn;
      _showStart = true;
    });
  }

  Future<void> _leaveServer() async {
    if (widget.api case final RemoteHost remote) {
      if (kIsWeb) {
        try {
          await remote.disconnect();
        } catch (_) {
          // Reload still provides sign-in when the server is unreachable.
        }
        reloadBrowserSession();
      } else if (mounted) {
        Navigator.of(context).pop();
      }
    }
  }

  SettingsSection? _settingsSection;

  void _openSettings(SettingsSection? section) {
    _settingsSection = section;
    _go(AppTab.settings);
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    // Front door first: start screen replaces shell chrome until the user enters.

    if (_showStart && _tab != AppTab.settings) {
      return ListenableBuilder(
        listenable: store,
        builder: (context, _) => ConnectionsStartScreen(
          store: _conn,
          serverError: _serverConnectionError,
          onConnectServer: widget.api is RemoteHost ? null : _connectServer,
          onSettings: () => _openSettings(null),
          onHelp: () => _openSettings(SettingsSection.about),
          onEnterMixer: () => setState(() => _showStart = false),
          onBackToMixer: _canReturnFromStart
              ? () => setState(() => _showStart = false)
              : null,
          onWorkOffline: () {
            _conn.connectSim();
            if (const bool.fromEnvironment('OPENFADER_MARKETING_FIXTURE')) {
              unawaited(stageMarketingFixture(store));
            }
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
        final narrow =
            MediaQuery.sizeOf(context).width < AppDesign.phoneBreakpoint;
        return U4Shell(
          header: Column(mainAxisSize: MainAxisSize.min, children: [
            if (_tab != AppTab.recorder) _workspaceToolbar(),
            if (widget.api is HostTransport && _tab != AppTab.recorder)
              _recorderBanner(),
            if (_tab != AppTab.recorder)
              Container(
                  height: narrow ? 6 : 8,
                  color: _mixer.masterColor(ConsolePalette.of(context))),
          ]),
          bottomSafeArea:
              _tab != AppTab.mixer && !(narrow && _tab == AppTab.channel),
          child: ConsolePageReveal(token: _tab, child: _pageFor(_tab, store)),
        );
      },
    );
  }

  Widget _recorderBanner() => ListenableBuilder(
      listenable: _recorder,
      builder: (context, _) {
        final state = _recorder.state;
        final mode = '${state['input_mode']}';
        final recording = state['transport'] == 'recording';
        final recovery = mode.contains('recovery');
        if (!recording && mode != 'soundcheck' && !recovery) {
          return const SizedBox.shrink();
        }
        return Row(children: [
          const SizedBox(width: 12),
          Icon(recording ? Icons.fiber_manual_record : Icons.play_arrow,
              size: 16,
              color: recording ? ConsolePalette.of(context).meterRed : ConsolePalette.of(context).lime),
          const SizedBox(width: 8),
          Expanded(child: Text(recording ? 'Recording' : recovery ? 'Routing recovery' : 'Playback')),
          if (!recording)
            IconButton(
                tooltip: 'Return to live inputs',
                onPressed: _recorder.pending ? null : state['manual_routing'] == true || recovery
                    ? () => _go(AppTab.recorder)
                    : () => _recorder.command('return_live'),
                icon: const Icon(Icons.input)),
          IconButton(
              tooltip: 'Open recorder',
              onPressed: () => _go(AppTab.recorder),
              icon: const Icon(Icons.audio_file_outlined)),
        ]);
      });

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

  void _navigateMixer(MixerDestination destination) =>
      _go(switch (destination) {
        MixerDestination.meters => AppTab.meters,
        MixerDestination.channel => AppTab.channel,
        MixerDestination.scenes => AppTab.scenes,
        MixerDestination.fx => AppTab.fx,
        MixerDestination.routing => AppTab.routing,
        MixerDestination.connect => AppTab.connect,
        MixerDestination.settings => AppTab.settings,
        MixerDestination.debug => AppTab.debug,
      });

  Widget _workspaceToolbar() => ListenableBuilder(
      listenable: Listenable.merge([_mixer, _fx, _fxReadout, _channelFocus]),
      builder: (context, _) {
        final compact = MediaQuery.sizeOf(context).width <
            AppDesign.compactToolbarBreakpoint;
        final store = widget.store;
        Widget? readout;
        if (_tab == AppTab.fx) {
          final data =
              _fxReadoutSession == _fx.controlSession ? _fxReadout.value : null;
          readout = SelectedControlReadout(
              data: data ??
                  const SelectedControlReadoutData(
                      target: 'FX', label: 'Select a control', value: '-'),
              compact: compact,
              connectionName: _mixer.demo ? '' : _mixer.connectionDisplayName,
              onConnection: () => _go(AppTab.connect));
        } else if (_tab == AppTab.channel &&
            store.outputProcessingTarget == null) {
          readout = ChannelToolbar(
              key: ValueKey((store.stripKey, store.bank, store.engine.driver)),
              store: store,
              compact: compact,
              embedded: true,
              controlFocus: _channelFocus,
              onHome: () => _go(AppTab.mixer),
              onTool: _channelTool,
              onLock: () => store.locked ? store.cmdUnlock() : store.cmdLock(),
              trailing: _mixer.demo
                  ? null
                  : TextButton(
                      style: TextButton.styleFrom(
                          backgroundColor: Colors.transparent,
                          alignment: Alignment.centerRight,
                          padding: EdgeInsets.zero),
                      onPressed: () => _go(AppTab.connect),
                      child: Text(_mixer.connectionDisplayName,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontSize: compact ? 11 : 13,
                              color: ConsolePalette.of(context).mutedText))));
        }
        return MixerToolbar(
            controller: _mixer,
            compact: compact,
            pageTitle: _tab == AppTab.mixer
                ? null
                : _tab == AppTab.channel && store.outputProcessingTarget != null
                    ? 'Output processing'
                    : _tab.label,
            onNavigate: _navigateMixer,
            onHome: () =>
                _go(_tab == AppTab.mixer ? AppTab.home : AppTab.mixer),
            homeLabel: _tab == AppTab.mixer ? 'Home' : 'Mixer',
            onOpenTools: () => _go(AppTab.home),
            readoutContent: readout);
      });

  Widget _mixerPage() => MixerPage(
      controller: _mixer,
      embedded: true,
      onHome: () => _go(AppTab.mixer),
      onNavigate: _navigateMixer);

  Widget _pageFor(AppTab t, AppStore store) =>
      KeyedSubtree(key: ValueKey(t), child: _pageContent(t, store));

  Widget _pageContent(AppTab t, AppStore store) {
    if (t == AppTab.scenes && store.blockedAccess.isNotEmpty) {
      return Center(
          child: Padding(
        padding: const EdgeInsets.all(24),
        child: Text(
            'Scenes and shows are unavailable while local access limits are active. Bulk recalls can change restricted controls.',
            style: TextStyle(
                color: ConsolePalette.of(context).text1, fontSize: 18)),
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
      AppTab.home => WorkspaceHome(
          serverError: widget.api is RemoteHost ? store.transportError : '',
          onReloadServer:
              widget.api is RemoteHost && kIsWeb ? reloadBrowserSession : null,
          onLeaveServer: widget.api is RemoteHost ? _leaveServer : null,
          leaveServerLabel: kIsWeb ? 'Sign out' : 'Disconnect server',
          onQuit: widget.desktop?.requestQuit,
          onMixer: () => _go(AppTab.mixer),
          onConnections: () => _openConnectionsHome(),
          onConsoleControls: () => _go(AppTab.consoleControls),
          onRecorder:
              (widget.api is HostTransport) ? () => _go(AppTab.recorder) : null,
          onNavigate: _navigateMixer,
          onLayout: (anchor) => showMixerLayout(anchor, _mixer, _navigateMixer,
              locked: widget.store.locked,
              onLock: () => widget.store.locked
                  ? widget.store.cmdUnlock()
                  : widget.store.cmdLock())),
      AppTab.mixer => _mixerPage(),
      AppTab.recorder => RecorderPage(
          store: _recorder,
          nativeStore: _scenes,
          onMixer: () => _go(AppTab.mixer),
          onFx: () => _go(AppTab.fx),
          onSettings: () => _go(AppTab.settings),
          onTools: () => _go(AppTab.home),
          mixerName: _mixer.demo ? '' : _mixer.connectionDisplayName),
      AppTab.meters =>
        U4MetersPage(store: store, onClose: () => _go(AppTab.mixer)),
      AppTab.channel => store.outputProcessingTarget != null
          ? OutputProcessingPage(
              store: store, onClose: () => _go(AppTab.mixer), embedded: true)
          : ChannelPage(
              key: ValueKey(
                  (store.bank, store.engine.driver, store.connectionEpoch)),
              stripKey: store.stripKey,
              store: store,
              mixerControls: _processingMixerControls(),
              controlFocus: _channelFocus,
              onChannelStep: _mixer.moveProcessingSelection),
      AppTab.scenes => U4ScenesPage(store: _scenes),
      AppTab.fx => U4FxPage(
          store: _fx,
          onReadout: (readout) {
            if (!mounted || _tab != AppTab.fx) return;
            _fxReadoutSession = _fx.controlSession;
            _fxReadout.value = readout;
          }),
      AppTab.routing => U4RoutingPage(store: _routing),
      AppTab.connect => Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            if (!widget.isRealEngine) const _EngineTagBanner(isReal: false),
            Expanded(
                child: U4ConnectionPage(
                    store: _conn, onOpenDebug: () => _go(AppTab.debug))),
          ],
        ),
      AppTab.settings => U4SettingsPage(
          initialSection: _settingsSection,
          embedded: true,
          store: _settings,
          onClose: () {
            _settingsSection = null;
            _go(_prevTab);
          },
          onHome: () {
            _settingsSection = null;
            _openConnectionsHome(canReturn: !_showStart);
          },
          onOpenMixer: () {
            _showStart = false;
            _conn.connectSim();
            _go(AppTab.mixer);
          }),
      AppTab.debug => U4DebugPage(store: _debug),
      AppTab.consoleControls => ConsoleControlsPage(store: _consoleControls),
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
                    identityChannel:
                        ch.master ? _mixer.masterIdentityChannel : null,
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
              final screen = MediaQuery.sizeOf(context).width -
                  MediaQuery.paddingOf(context).horizontal;
              final railWidth = MixerGeometry.railWidth(screen);
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
                : 'ENGINE: FAKE sim fallback - native lib missing, desk impossible',
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
