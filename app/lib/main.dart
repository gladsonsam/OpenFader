// BootGate drives staged boot behind the launch screen, then swaps in
// the full-port shell. Desk discovery lives in the Link tab.
import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'boot/boot_controller.dart';
import 'boot/browser_connection_gate.dart';
import 'engine_protocol.dart';
import 'boot/launch_screen.dart';
import 'boot/host_transport.dart';
import 'desktop/desktop_integration.dart';
import 'settings/prefs.dart';
import 'shell/app_router.dart';
import 'theme.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  try {
    await PrefsService.ensureInitialized();
  } catch (error) {
    debugPrint('Preferences unavailable; using defaults: $error');
  }
  final desktop = await DesktopIntegration.initialize();
  runApp(OpenfaderApp(desktop: desktop));
  if (desktop != null) {
    // Restore visibility after native window restoration and the first Flutter
    // frame, so a previously hidden or minimized window cannot strand startup.
    WidgetsBinding.instance.addPostFrameCallback((_) => desktop.show());
  }
}

class OpenfaderApp extends StatelessWidget {
  final DesktopIntegration? desktop;
  const OpenfaderApp({super.key, this.desktop});

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: PrefsService.instance,
      builder: (context, _) {
        final density = switch (PrefsService.instance.interfaceSize) {
          'compact' => VisualDensity.compact,
          'large' => VisualDensity.comfortable,
          _ => VisualDensity.standard,
        };
        return MaterialApp(
          navigatorKey: desktop?.navigatorKey,
          title: 'OpenFader',
          debugShowCheckedModeBanner: false,
          theme: u4Theme(
                  brightness: Brightness.light,
                  accent: PrefsService.instance.accent)
              .copyWith(visualDensity: density),
          darkTheme: u4Theme(
                  brightness: Brightness.dark,
                  accent: PrefsService.instance.accent)
              .copyWith(visualDensity: density),
          themeMode: switch (PrefsService.instance.appearance) {
            'light' => ThemeMode.light,
            'system' => ThemeMode.system,
            _ => ThemeMode.dark,
          },
          themeAnimationDuration: Duration.zero,
          builder: (context, child) => AnnotatedRegion<SystemUiOverlayStyle>(
              value: Theme.of(context).brightness == Brightness.light
                  ? SystemUiOverlayStyle.dark.copyWith(
                      systemNavigationBarColor:
                          Theme.of(context).scaffoldBackgroundColor)
                  : SystemUiOverlayStyle.light.copyWith(
                      systemNavigationBarColor:
                          Theme.of(context).scaffoldBackgroundColor),
              child: MediaQuery(
                  data: MediaQuery.of(context).copyWith(
                      textScaler:
                          TextScaler.linear(PrefsService.instance.textScale)),
                  child: CallbackShortcuts(
                      bindings: {
                        if (desktop != null)
                          const SingleActivator(LogicalKeyboardKey.keyQ,
                              control: true): () => desktop!.requestQuit(),
                      },
                      child: Listener(
                          onPointerDown: desktop?.pointerDown,
                          onPointerUp: desktop?.pointerEnded,
                          onPointerCancel: desktop?.pointerEnded,
                          child: child!)))),
          home: kIsWeb
              ? BrowserConnectionGate(connected: (host) => BootGate(api: host))
              : BootGate(desktop: desktop),
        );
      },
    );
  }
}

/// First-frame gate: paints [LaunchScreen], then swaps in [AppShell]
/// once the store is synced.
class BootGate extends StatefulWidget {
  final DesktopIntegration? desktop;
  final EngineApi? api;
  const BootGate({super.key, this.desktop, this.api});

  @override
  State<BootGate> createState() => _BootGateState();
}

class _BootGateState extends State<BootGate> {
  late final BootController _boot;
  bool _navigated = false;

  @override
  void initState() {
    super.initState();
    _boot = BootController(api: widget.api);
    widget.desktop?.attach(this, () async {
      if (_boot.running) {
        throw StateError(
            'OpenFader is still starting. Wait for startup to finish before quitting.');
      }
      final api = _boot.api;
      if (api is HostTransport) await api.quitHost();
      return true;
    });
    _boot.addListener(_onBoot);
    // Post-frame so the launch screen paints before the slow stages start.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _boot.start();
    });
  }

  void _onBoot() {
    if (_boot.done && !_navigated && mounted) {
      _navigated = true;
      final api = _boot.api;
      final store = _boot.store;
      if (api == null || store == null) {
        _navigated = false;
        return;
      }
      Navigator.of(context).pushReplacement(MaterialPageRoute(
          builder: (_) => AppShell(
              desktop: widget.desktop,
              api: api,
              store: store,
              initialTab: AppTab.mixer,
              isRealEngine: _boot.isRealEngine)));
    }
  }

  @override
  void dispose() {
    widget.desktop?.detach(this);
    _boot.removeListener(_onBoot);
    _boot.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Stack(children: [
      LaunchScreen(
        controller: _boot,
        onRetry: () {
          _navigated = false;
          _boot.start();
        },
      ),
      if (widget.desktop != null)
        Positioned(
            top: 8,
            right: 8,
            child: Material(
                type: MaterialType.transparency,
                child: SafeArea(
                    child: TextButton(
                        onPressed: () => widget.desktop!.requestQuit(),
                        child: const Text('Quit OpenFader')))))
    ]);
  }
}
