// BootGate drives staged boot behind the launch screen, then swaps in
// the full-port shell. Desk discovery lives in the Link tab.
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'boot/boot_controller.dart';
import 'boot/launch_screen.dart';
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
  runApp(const OpenfaderApp());
}

class OpenfaderApp extends StatelessWidget {
  const OpenfaderApp({super.key});

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
                  child: child!)),
          home: const BootGate(),
        );
      },
    );
  }
}

/// First-frame gate: paints [LaunchScreen], then swaps in [AppShell]
/// once the store is synced.
class BootGate extends StatefulWidget {
  const BootGate({super.key});

  @override
  State<BootGate> createState() => _BootGateState();
}

class _BootGateState extends State<BootGate> {
  late final BootController _boot;
  bool _navigated = false;

  @override
  void initState() {
    super.initState();
    _boot = BootController();
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
              api: api,
              store: store,
              initialTab: AppTab.mixer,
              isRealEngine: _boot.isRealEngine)));
    }
  }

  @override
  void dispose() {
    _boot.removeListener(_onBoot);
    _boot.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return LaunchScreen(
      controller: _boot,
      onRetry: () {
        _navigated = false;
        _boot.start();
      },
    );
  }
}
