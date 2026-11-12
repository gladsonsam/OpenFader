import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:dbus/dbus.dart';
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:tray_manager/tray_manager.dart' as tray;
import 'package:window_manager/window_manager.dart';

import '../ui/overlays/console_dialogs.dart';

/// Process-owned window/tray integration. Hiding never disposes the shell or
/// changes its connection, recorder, or polling cadence.
class DesktopIntegration {
  DesktopIntegration._();

  static bool get supported =>
      Platform.isWindows || Platform.isMacOS || Platform.isLinux;
  static const _native = MethodChannel('openfader/desktop');
  final navigatorKey = GlobalKey<NavigatorState>();
  final _pointers = <int>{};
  SharedPreferences? _prefs;
  Future<bool> Function()? _prepareQuit;
  Object? _owner;
  Timer? _boundsDebounce;
  Window? _window;
  ListenerId? _windowListener;
  tray.TrayIcon? _trayIcon;
  tray.Menu? _trayMenu;
  tray.MenuItem? _statusItem;
  bool _windowReady = false, _trayReady = false, _hidden = false;
  bool _quitting = false, _hiding = false, _restoring = false;
  String _status = 'Starting…';
  Future<void> _menuUpdate = Future.value();
  DBusClient? _bus;
  final _subscriptions = <StreamSubscription<dynamic>>[];
  bool _linuxTrayHost = false;

  static Future<DesktopIntegration?> initialize() async {
    if (!supported) return null;
    final desktop = DesktopIntegration._();
    _native.setMethodCallHandler((call) async {
      if (call.method == 'requestQuit') {
        await desktop.requestQuit();
      }
    });
    try {
      final window = WindowManager.instance.getCurrent();
      if (window == null) throw StateError('Native window unavailable');
      desktop._window = window;
      desktop._windowListener = window.addListener(desktop._onWindowEvent);
      desktop._windowReady = true;
      desktop._prefs = await SharedPreferences.getInstance();
      await desktop._restoreBounds();
    } catch (error) {
      debugPrint('Desktop window integration unavailable: $error');
    }
    if (desktop._windowReady) {
      try {
        final icon =
            tray.TrayIcon.createWithIdentifier('com.openfader.openfader');
        if (icon == null) throw StateError('Native tray unavailable');
        desktop._trayIcon = icon;
        final image = tray.ImageAsset.fromAsset(Platform.isWindows
            ? 'assets/brand/open-fader-tray.ico'
            : 'assets/brand/open-fader-logo-32.png');
        if (image == null) throw StateError('Tray image unavailable');
        icon.icon = image;
        icon.setTooltip('OpenFader');
        icon.addListener((event) {
          if (event is tray.TrayIconClickedEvent) {
            Platform.isMacOS
                ? desktop._openTrayMenu()
                : unawaited(desktop.show());
          }
        });
        icon.setContextMenuTrigger(tray.ContextMenuTrigger.rightClicked);
        desktop._trayReady = true;
        await desktop._updateMenu();
        if (!desktop._trayReady) throw StateError('Tray menu unavailable');
        if (!icon.setVisible(true)) {
          throw StateError('Could not show tray icon');
        }
        if (Platform.isLinux) await desktop._watchLinuxTray();
      } catch (error) {
        desktop._disposeTray();
        debugPrint('Tray unavailable; Hide to tray will minimize: $error');
      }
    }
    return desktop;
  }

  void attach(Object owner, Future<bool> Function() prepareQuit) {
    _owner = owner;
    _prepareQuit = prepareQuit;
  }

  void detach(Object owner) {
    if (!identical(_owner, owner)) return;
    _owner = null;
    _prepareQuit = null;
  }

  void updateStatus(String status) {
    if (_status == status) return;
    _status = status;
    unawaited(_updateMenu());
  }

  Future<void> _updateMenu() {
    return _menuUpdate = _menuUpdate.then((_) async {
      if (!_trayReady) return;
      final statusItem = _statusItem;
      if (statusItem != null) {
        statusItem.label = _status;
        return;
      }
      final menu = tray.Menu.create();
      if (menu == null) throw StateError('Native tray menu unavailable');
      _trayMenu = menu;
      void add(String label, Future<void> Function()? action) {
        final item = tray.MenuItem.createWithLabelAndType(
            label, tray.MenuItemType.normal);
        if (item == null) throw StateError('Native tray item unavailable');
        item.isEnabled = action != null;
        if (action != null) {
          item.addListener((event) {
            if (event is tray.MenuItemClickedEvent) unawaited(action());
          });
        } else {
          _statusItem = item;
        }
        menu.addItem(item);
      }

      add('Show OpenFader', show);
      add('Hide to tray', _hide);
      add(_status, null);
      menu.addSeparator();
      add('Quit OpenFader', requestQuit);
      _trayIcon!.setContextMenu(menu);
    }).catchError((Object error) {
      _trayReady = false;
      debugPrint('Tray menu unavailable: $error');
      if (_hidden) unawaited(show());
    });
  }

  void _disposeTray() {
    _trayIcon?.dispose();
    _trayIcon = null;
    _trayMenu?.dispose();
    _trayMenu = null;
    _statusItem = null;
    _trayReady = false;
  }

  // Linux can register a tray icon even when no panel hosts icons.
  // Watch the actual host, and restore a hidden window if its panel vanishes.
  Future<void> _watchLinuxTray() async {
    try {
      final bus = _bus = DBusClient.session();
      final watcher = DBusRemoteObject(bus,
          name: 'org.kde.StatusNotifierWatcher',
          path: DBusObjectPath('/StatusNotifierWatcher'));
      Future<void> refresh() async {
        var available = false;
        try {
          final value = await watcher
              .getProperty('org.kde.StatusNotifierWatcher',
                  'IsStatusNotifierHostRegistered')
              .timeout(const Duration(seconds: 2));
          available = value.asBoolean();
        } catch (_) {
          // No panel, no bus, or a disappearing watcher: keep the app visible.
        }
        _linuxTrayHost = available;
        if (!available && _hidden) await show();
      }

      void changed(dynamic _) => unawaited(refresh());
      _subscriptions.add(watcher.propertiesChanged
          .listen(changed, onError: (Object _) => changed(null)));
      _subscriptions.add(DBusSignalStream(bus,
              sender: 'org.kde.StatusNotifierWatcher',
              path: DBusObjectPath('/StatusNotifierWatcher'),
              interface: 'org.kde.StatusNotifierWatcher')
          .listen(changed, onError: (Object _) => changed(null)));
      _subscriptions.add(bus.nameOwnerChanged.listen((event) {
        if (event.name == 'org.kde.StatusNotifierWatcher') changed(null);
      }, onError: (Object _) => changed(null)));
      await refresh();
    } catch (error) {
      _linuxTrayHost = false;
      debugPrint('Could not detect a Linux tray host: $error');
    }
  }

  void pointerDown(PointerDownEvent event) => _pointers.add(event.pointer);
  void pointerEnded(PointerEvent event) => _pointers.remove(event.pointer);

  void _cancelPointers() {
    for (final pointer in _pointers.toList()) {
      GestureBinding.instance.cancelPointer(pointer);
    }
    _pointers.clear();
  }

  Future<void> show() async {
    if (!_windowReady || _restoring) return;
    _restoring = true;
    try {
      _window!.show();
      if (_window!.isMinimized) _window!.restore();
      _window!.focus();
      _hidden = false;
    } catch (error) {
      debugPrint('Could not restore OpenFader: $error');
    } finally {
      _restoring = false;
    }
  }

  Future<void> _hide() async {
    if (_hiding || _quitting) return;
    _hiding = true;
    try {
      _cancelPointers();
      await _saveBounds();
      if (_trayReady && (!Platform.isLinux || _linuxTrayHost)) {
        _window!.hide();
        _hidden = true;
        if (!_trayReady || (Platform.isLinux && !_linuxTrayHost)) {
          await show();
        }
      } else {
        _window!.minimize();
      }
    } catch (error) {
      debugPrint('Could not hide OpenFader: $error');
      await show();
    } finally {
      _hiding = false;
    }
  }

  Future<void> requestQuit() async {
    if (_quitting) return;
    _quitting = true;
    var shutdownPrepared = false;
    try {
      await show();
      _cancelPointers();
      final prepare = _prepareQuit;
      if (prepare == null || !await prepare()) return;
      shutdownPrepared = true;
      await _saveBounds();
      try {
        for (final subscription in _subscriptions) {
          await subscription.cancel();
        }
        await _bus?.close().timeout(const Duration(seconds: 2));
        _disposeTray();
      } catch (error) {
        // Cosmetic tray cleanup must not strand the app after its engine has
        // already shut down successfully.
        debugPrint('Could not remove the desktop tray: $error');
      } finally {
        _subscriptions.clear();
        _bus = null;
        _trayReady = false;
      }
      // Native termination is enabled only after the shell has acknowledged
      // host shutdown. Cmd+Q and the macOS application menu use the same path.
      if (Platform.isMacOS) {
        await _native.invokeMethod<void>('terminate');
      } else if (Platform.isLinux) {
        // The embedded engine has already disconnected. Exit the process
        // without closing GTK's implicit Flutter view from a method callback
        // (Flutter 3.47 rejects removing that view during window teardown).
        exit(0);
      } else {
        final window = _window!;
        final listener = _windowListener;
        if (listener != null) window.removeListener(listener);
        _windowListener = null;
        if (!window.close()) exit(0);
      }
    } catch (error) {
      if (shutdownPrepared) {
        // The engine is already safely stopped. A failed native window API
        // must not leave a frozen shell or restart the host behind the user.
        debugPrint('Native window termination failed; exiting: $error');
        exit(0);
      }
      final context = navigatorKey.currentContext;
      if (context != null && context.mounted) {
        await showDialog<void>(
            context: context,
            builder: (context) => ConsoleAlertDialog(
                  title: const Text('Could not quit OpenFader'),
                  content: Text('$error'),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(context),
                        child: const Text('OK')),
                  ],
                ));
      }
    } finally {
      _quitting = false;
    }
  }

  Future<void> _restoreBounds() async {
    final displays = await screenRetriever.getAllDisplays();
    if (displays.isEmpty) return;
    final areas = displays
        .map((d) =>
            (d.visiblePosition ?? Offset.zero) & (d.visibleSize ?? d.size))
        .where((r) => r.width > 0 && r.height > 0)
        .toList();
    if (areas.isEmpty) return;
    final saved = _prefs?.getStringList('desktop.window.bounds');
    final values = saved?.map(double.tryParse).toList();
    Rect? previous;
    if (values != null &&
        values.length == 4 &&
        values.every((v) => v != null && v.isFinite) &&
        values[2]! > 0 &&
        values[3]! > 0) {
      previous = Rect.fromLTWH(values[0]!, values[1]!, values[2]!, values[3]!);
    }
    final area = areas.firstWhere(
        (r) => previous != null && r.contains(previous.topLeft),
        orElse: () => areas.first);
    final minSize = Size(math.min(720, area.width), math.min(560, area.height));
    final width = (previous?.width ?? 1280).clamp(minSize.width, area.width);
    final height = (previous?.height ?? 800).clamp(minSize.height, area.height);
    final x = (previous?.left ?? area.center.dx - width / 2)
        .clamp(area.left, area.right - width);
    final y = (previous?.top ?? area.center.dy - height / 2)
        .clamp(area.top, area.bottom - height);
    _window!.minimumSize = minSize.toNative();
    _window!.bounds = Rect.fromLTWH(x, y, width, height).toNative();
    if (_prefs?.getBool('desktop.window.maximized') == true) {
      _window!.maximize();
    }
  }

  void _scheduleBoundsSave() {
    _boundsDebounce?.cancel();
    _boundsDebounce = Timer(const Duration(milliseconds: 400), _saveBounds);
  }

  Future<void> _saveBounds() async {
    _boundsDebounce?.cancel();
    final prefs = _prefs;
    if (!_windowReady || prefs == null) return;
    try {
      if (_window!.isMinimized || _window!.isFullScreen) {
        return;
      }
      final maximized = _window!.isMaximized;
      await prefs.setBool('desktop.window.maximized', maximized);
      if (maximized) return;
      final rect = _window!.bounds.toRect();
      await prefs.setStringList(
          'desktop.window.bounds',
          [rect.left, rect.top, rect.width, rect.height]
              .map((v) => '$v')
              .toList());
    } catch (error) {
      debugPrint('Could not save desktop window: $error');
    }
  }

  void _onWindowEvent(WindowEvent event) {
    switch (event) {
      case WindowCloseRequestedEvent():
        // Title-bar close and window-manager shortcuts share the guarded path.
        event.request.cancel();
        unawaited(Platform.isLinux ? requestQuit() : _hide());
      case WindowBlurredEvent():
        _cancelPointers();
      case WindowFocusedEvent():
        _hidden = false;
      case WindowMovedEvent() ||
            WindowResizedEvent() ||
            WindowMaximizedEvent() ||
            WindowRestoredEvent():
        _scheduleBoundsSave();
      default:
        break;
    }
  }

  void _openTrayMenu() {
    // Linux's panel opens the exported tray menu itself.
    if (Platform.isLinux || !_trayReady || _quitting) return;
    if (!_trayIcon!.openContextMenu()) {
      debugPrint('Could not open the tray menu');
    }
  }
}
