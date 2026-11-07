// Connections start screen: the app front door with a compact recent-mixers box.
// Rows come from recents plus live engine devices. Entering the mixer happens
// only through an explicit user action or a user-initiated connect success.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../ui/console.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import 'connection.dart';

class ConnectionsStartScreen extends StatefulWidget {
  final ConnectionStore store;
  final VoidCallback onSettings;
  final VoidCallback onHelp;

  /// Row connected / already-connected tapped, or Work offline chosen.
  final VoidCallback onEnterMixer;

  /// Work offline chosen: the owner connects the demo desk, then enters.
  final VoidCallback onWorkOffline;
  final VoidCallback? onConnectComputer;

  const ConnectionsStartScreen(
      {super.key,
      required this.store,
      required this.onSettings,
      required this.onHelp,
      required this.onEnterMixer,
      required this.onWorkOffline,
      this.onConnectComputer});

  @override
  State<ConnectionsStartScreen> createState() => _ConnectionsStartState();
}

class _Row {
  final String key;
  final String name;
  final String addr;
  final bool discovered;
  final bool connected;
  final bool reachable;
  final MixerType mixerType;
  const _Row(
      {required this.key,
      required this.name,
      required this.addr,
      required this.discovered,
      required this.connected,
      required this.reachable,
      this.mixerType = MixerType.automatic});
}

class _ConnectionsStartState extends State<ConnectionsStartScreen> {
  RecentMixers get _recentsBox => RecentMixers.instance;
  PrefsService get _prefs => PrefsService.instance;

  String? _pending;
  String? _pendingName;
  MixerType _pendingType = MixerType.automatic;
  bool _wasProbing = false;
  String? _lastError;
  bool _scannedOnce = false;

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_onStore);
    RecentMixers.ensureLoaded();
    PrefsService.ensureInitialized();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _scannedOnce) return;
      _scannedOnce = true;
      if (_prefs.autoFind) widget.store.scan();
    });
  }

  @override
  void dispose() {
    widget.store.removeListener(_onStore);
    super.dispose();
  }

  ConnectionStore get _store => widget.store;

  void _toast(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
        backgroundColor: ConsolePalette.of(context).chip,
        content: Text(msg,
            textAlign: TextAlign.center,
            style: AppDesign.type(15,
                weight: FontWeight.w500,
                color: ConsolePalette.of(context).text)),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3)));
  }

  List<_Row> _rows() {
    final devices = _store.devices;
    final byHost = <String, ConnDevice>{};
    for (final d in devices) {
      final host = _store.hostOf(d.id);
      if (host.isNotEmpty) byHost[_store.canonicalHost(host)] = d;
    }
    final seen = <String>{};
    final rows = <_Row>[];
    for (final r in _recentsBox.items) {
      seen.add(_store.canonicalHost(r.addr));
      final d = byHost[_store.canonicalHost(r.addr)];
      rows.add(_Row(
          key: 'recent:${r.addr}',
          name: r.name.isEmpty ? r.addr : r.name,
          addr: r.addr,
          mixerType: r.driver == 'auto' && d != null
              ? d.mixerType
              : MixerType.fromDriver(r.driver),
          discovered: false,
          connected: d?.connected ??
              (_store.isConnected &&
                  _store.connectedHost == _store.canonicalHost(r.addr)),
          reachable: _store.checkStatusOf(r.addr) == 'available' ||
              (_store.checkStatusOf(r.addr) == null && d != null)));
    }
    for (final d in devices) {
      final host = _store.hostOf(d.id);
      if (host.isEmpty || seen.contains(_store.canonicalHost(host))) continue;
      seen.add(_store.canonicalHost(host));
      rows.add(_Row(
          key: 'dev:${d.id}',
          name: d.name.isEmpty ? host : d.name,
          addr: host,
          mixerType: d.mixerType,
          discovered: true,
          connected: d.connected,
          reachable: true));
    }
    return rows;
  }

  void _onStore() {
    if (!mounted) return;
    final err = _store.scanError;
    if (err == null || err.isEmpty) _lastError = null;
    if (err != null && err.isNotEmpty && err != _lastError) {
      _lastError = err;
      if (_pending != null) {
        final target = _pendingName ?? _pending!;
        _pending = null;
        _pendingName = null;
        _toast('Could not connect to $target: $err');
      } else {
        _toast(err);
      }
    }
    final probing = _store.probing;
    // A settled probe with no target hit and no engine error means the host stayed silent.
    if (_wasProbing && !probing && _pending != null) {
      final addr = _pending!;
      final name = _pendingName ?? addr;
      final hit = _store.connectedHost == _store.canonicalHost(addr) ||
          (addr.startsWith('id:') &&
              _store.devices.any((d) => d.connected && 'id:${d.id}' == addr));
      if (!hit && (_store.scanError?.isEmpty ?? true)) {
        _pending = null;
        _pendingName = null;
        _toast('No response from $name');
      }
    }
    _wasProbing = probing;
    if (_pending != null && !probing) {
      final addr = _pending!;
      final name = _pendingName ?? addr;
      final hit = _store.connectedHost == _store.canonicalHost(addr) ||
          (addr.startsWith('id:') &&
              _store.devices.any((d) => d.connected && 'id:${d.id}' == addr));
      if (hit) {
        _pending = null;
        _pendingName = null;
        unawaited(_persistAndEnter(name, addr, _pendingType));
        return;
      }
    }
    setState(() {});
  }

  Future<void> _persistAndEnter(
      String name, String addr, MixerType mixerType) async {
    try {
      await _persistConnection(name, addr, mixerType);
      if (mounted) widget.onEnterMixer();
    } catch (error) {
      _toast(
          'Connected, but this mixer could not be saved: $error. Tap Open mixer to continue.');
    }
  }

  Future<void> _persistConnection(
      String name, String addr, MixerType mixerType) async {
    if (addr.isEmpty || addr.startsWith('id:')) return;
    if (PrefsService.instance.rememberRecents) {
      await RecentMixers.instance
          .remember(name, addr, driver: mixerType.driver);
    }
  }

  void _connectRow(_Row row) {
    if (_store.probing) return;
    _pendingType = row.mixerType;
    if (row.connected) {
      widget.onEnterMixer();
      return;
    }
    final host = row.addr;
    if (host.isEmpty) {
      // Devices without an IP connect by id and enter when the link reports it.
      _pending = 'id:${row.key}';
      _pendingName = row.name;
      _store.connectDevice(_deviceIdFor(row));
      _toast('Connecting to ${row.name}…');
      return;
    }
    _pending = host;
    _pendingName = row.name;
    _store.probe(host, mixerType: row.mixerType);
  }

  String _deviceIdFor(_Row row) {
    for (final d in _store.devices) {
      if (_store.hostOf(d.id) == row.addr && row.addr.isNotEmpty) return d.id;
    }
    return '';
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).background,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: Listenable.merge([
            widget.store,
            RecentMixers.instance,
            PrefsService.instance,
          ]),
          builder: (context, _) => Stack(
            children: [
              Column(children: [
                Expanded(
                    child: Center(
                        child: SingleChildScrollView(
                  padding: EdgeInsets.fromLTRB(wide ? 32 : 24, wide ? 100 : 82,
                      wide ? 32 : 24, wide ? 40 : 20),
                  child: Transform.translate(
                      offset: Offset(0, wide ? -40 : -10),
                      child: Column(mainAxisSize: MainAxisSize.min, children: [
                        _Brand(wide: wide),
                        SizedBox(height: wide ? 56 : 36),
                        _Box(
                          wide: wide,
                          title: _rows().any((r) => r.discovered)
                              ? 'Mixers'
                              : 'Recent mixers',
                          scanned: _store.lastScan != 'never',
                          onRefresh: _store.scanning || _store.probing
                              ? null
                              : () => _store.refreshKnown(_recentsBox.items
                                  .map((r) => r.addr)
                                  .toList()),
                          refreshing: _store.scanning,
                          error: (_store.scanError?.isEmpty ?? true)
                              ? null
                              : _store.scanError,
                          rows: _rows(),
                          pending: _pending,
                          progress:
                              _store.probing ? _store.connectionProgress : null,
                          onRetry: !_store.probing && _store.lastProbe != null
                              ? _retryConnection
                              : null,
                          onConnect: _connectRow,
                          onDiscover:
                              _store.scanning ? null : () => _store.scan(),
                          onAdd: _showManualAdd,
                        ),
                      ])),
                ))),
                SizedBox(
                    height: widget.onConnectComputer == null
                        ? (wide ? 112 : 100)
                        : 150,
                    child: Center(
                        child:
                            Column(mainAxisSize: MainAxisSize.min, children: [
                      if (widget.onConnectComputer != null)
                        TextButton(
                            onPressed: widget.onConnectComputer,
                            child: const Text('Connect to computer')),
                      _WorkOffline(onTap: widget.onWorkOffline),
                    ]))),
              ]),
              Positioned(
                top: wide ? 32 : 18,
                right: wide ? 40 : 18,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    _CornerIcon(
                        tooltip: 'Connection help',
                        onTap: widget.onHelp,
                        child: const _HelpGlyph()),
                    SizedBox(width: wide ? 12 : 6),
                    _CornerIcon(
                        tooltip: 'Settings',
                        onTap: widget.onSettings,
                        child: const _GearGlyph()),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  void _retryConnection() {
    if (_store.probing || _store.lastProbe == null) return;
    _pending = _store.lastProbe;
    _pendingName = _store.lastProbe;
    _pendingType = _store.lastMixerType;
    unawaited(_store.retryProbe());
  }

  void _showManualAdd() {
    if (_store.probing) return;
    showDialog<void>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => _ManualAddDialog(
        onSubmit: (addr, name, mixerType) {
          if (_store.probing) return;
          _pendingType = mixerType;
          _pending = addr;
          _pendingName = name.isEmpty ? addr : name;
          _store.probe(addr, mixerType: mixerType);
        },
      ),
    );
  }
}

class _Brand extends StatelessWidget {
  final bool wide;
  const _Brand({required this.wide});

  @override
  Widget build(BuildContext context) => Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
              width: wide ? 76 : 50,
              height: wide ? 76 : 50,
              child: SvgPicture.asset('assets/brand/open-fader-logo.svg',
                  semanticsLabel: 'OpenFader logo')),
          SizedBox(width: wide ? 24 : 16),
          Text('OpenFader',
              style: AppDesign.type(wide ? 48 : 34,
                  weight: FontWeight.w600,
                  color: ConsolePalette.of(context).text)),
        ],
      );
}

class _Box extends StatelessWidget {
  final bool wide;
  final String title;
  final bool scanned;
  final VoidCallback? onRefresh;
  final bool refreshing;
  final String? error;
  final List<_Row> rows;
  final String? pending;
  final String? progress;
  final VoidCallback? onRetry;
  final ValueChanged<_Row> onConnect;
  final VoidCallback? onDiscover;
  final VoidCallback onAdd;

  const _Box(
      {required this.wide,
      required this.title,
      required this.scanned,
      required this.onRefresh,
      required this.refreshing,
      required this.error,
      required this.rows,
      required this.pending,
      required this.progress,
      required this.onRetry,
      required this.onConnect,
      required this.onDiscover,
      required this.onAdd});

  @override
  Widget build(BuildContext context) => ConstrainedBox(
        constraints: BoxConstraints(maxWidth: wide ? 680 : 560),
        child: Container(
          decoration: BoxDecoration(
            color: ConsolePalette.of(context).chip,
            border: Border.all(color: ConsolePalette.of(context).lineCtl),
            borderRadius: BorderRadius.circular(10),
          ),
          clipBehavior: Clip.antiAlias,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: EdgeInsets.symmetric(
                    horizontal: wide ? 28 : 20, vertical: wide ? 20 : 14),
                child: Row(
                  children: [
                    Expanded(
                        child: Text(
                            rows.isEmpty ? title : '$title (${rows.length})',
                            style: AppDesign.type(wide ? 27 : 21,
                                weight: FontWeight.w500,
                                color: ConsolePalette.of(context).text))),
                    _IconButton(
                        tooltip: refreshing
                            ? 'Scanning for mixers…'
                            : 'Refresh recent mixers',
                        onTap: onRefresh,
                        child: _RefreshGlyph(spinning: refreshing)),
                  ],
                ),
              ),
              Divider(color: ConsolePalette.of(context).line, height: 1),
              if (refreshing)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20, vertical: 10),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text('Scanning for mixers…',
                          style: AppDesign.type(wide ? 18 : 15,
                              color: ConsolePalette.of(context).mutedText)),
                      const SizedBox(height: 8),
                      LinearProgressIndicator(
                        minHeight: 3,
                        color: ConsolePalette.of(context).lime,
                        backgroundColor: ConsolePalette.of(context).line,
                      ),
                    ],
                  ),
                ),
              if (progress != null)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20, vertical: 10),
                  child: Text(progress!,
                      style: AppDesign.type(wide ? 18 : 15,
                          color: ConsolePalette.of(context).mutedText)),
                ),
              if (error != null)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20, vertical: 10),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(error!,
                          style: AppDesign.type(wide ? 18 : 15,
                              color: ConsolePalette.of(context).mute)),
                      if (onRetry != null)
                        TextButton(
                          onPressed: onRetry,
                          style: TextButton.styleFrom(
                              foregroundColor: ConsolePalette.of(context).lime),
                          child: const Text('Retry connection'),
                        ),
                    ],
                  ),
                ),
              if (rows.isEmpty)
                Padding(
                  padding: EdgeInsets.symmetric(
                      horizontal: wide ? 28 : 20, vertical: wide ? 24 : 18),
                  child: Text(
                      refreshing
                          ? 'Scanning for mixers…'
                          : scanned
                              ? 'No mixers found. Check Wi-Fi or Add manually.'
                              : 'No mixers yet - Discover or Add manually.',
                      style: AppDesign.type(wide ? 23 : 17,
                          color: ConsolePalette.of(context).mutedText)),
                )
              else
                ConstrainedBox(
                  constraints: BoxConstraints(maxHeight: wide ? 460 : 370),
                  child: SingleChildScrollView(
                      child: Column(children: [
                    for (var i = 0; i < rows.length; i++) ...[
                      _DeviceRow(
                          key: ValueKey(rows[i].key),
                          row: rows[i],
                          index: i,
                          wide: wide,
                          busy: pending != null,
                          connecting: pending != null &&
                              (pending == rows[i].addr ||
                                  pending == rows[i].name),
                          onConnect: () => onConnect(rows[i])),
                      if (i < rows.length - 1)
                        Divider(
                            color: ConsolePalette.of(context).line, height: 1),
                    ],
                  ])),
                ),
              Divider(color: ConsolePalette.of(context).line, height: 1),
              _BoxActions(wide: wide, onDiscover: onDiscover, onAdd: onAdd),
            ],
          ),
        ),
      );
}

class _DeviceRow extends StatelessWidget {
  final _Row row;
  final int index;
  final bool wide;
  final bool busy;
  final bool connecting;
  final VoidCallback onConnect;
  const _DeviceRow(
      {super.key,
      required this.row,
      required this.index,
      required this.wide,
      required this.busy,
      required this.connecting,
      required this.onConnect});

  // One-shot entrance: the tween always runs 0 -> 1, so poll-loop rebuilds
  // (same end value) never restart a finished animation. The ValueKey on
  // each row keeps element identity stable as rows stream in, and stagger
  // comes from duration only (duration changes don't restart the tween).

  @override
  Widget build(BuildContext context) {
    final animationsOff = MediaQuery.disableAnimationsOf(context);
    final ms = (120 + index * 40).clamp(120, 200);
    final primary = row.connected || row.reachable;
    final label = connecting
        ? 'Connecting…'
        : row.connected
            ? 'Open mixer'
            : row.reachable
                ? 'Connect'
                : 'Retry';
    final content = Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 28 : 20, vertical: wide ? 24 : 20),
      child: Row(
        children: [
          Expanded(
              child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                Text(row.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppDesign.type(wide ? 27 : 21,
                        weight: FontWeight.w500,
                        color: ConsolePalette.of(context).text)),
                SizedBox(height: wide ? 8 : 7),
                Text(
                    row.mixerType == MixerType.dl32s
                        ? 'Mackie DL32S · ${row.addr}'
                        : row.discovered
                            ? 'Discovered - ${row.addr}'
                            : row.addr,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppDesign.type(wide ? 23 : 17,
                        color: ConsolePalette.of(context).mutedText)),
              ])),
          const SizedBox(width: 12),
          SizedBox(
            height: wide ? 56 : 44,
            width: wide ? 124 : 92,
            child: ElevatedButton(
              onPressed: busy || connecting ? null : onConnect,
              style: ElevatedButton.styleFrom(
                  backgroundColor: primary
                      ? ConsolePalette.of(context).lime
                      : ConsolePalette.of(context).control,
                  foregroundColor: primary
                      ? ConsolePalette.of(context).onAccent
                      : ConsolePalette.of(context).text,
                  disabledBackgroundColor: ConsolePalette.of(context).control,
                  shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(6)),
                  padding: EdgeInsets.symmetric(horizontal: wide ? 22 : 14),
                  textStyle: AppDesign.type(wide ? 24 : 18,
                      weight: FontWeight.w600,
                      color: ConsolePalette.of(context).text)),
              child: FittedBox(
                  fit: BoxFit.scaleDown,
                  child: Text(label, maxLines: 1, softWrap: false)),
            ),
          ),
        ],
      ),
    );
    if (animationsOff) return content;
    return TweenAnimationBuilder<double>(
      tween: Tween<double>(begin: 0.0, end: 1.0),
      duration: Duration(milliseconds: ms),
      curve: Curves.easeOut,
      builder: (context, value, child) => Opacity(
        opacity: value,
        child: Transform.translate(
          offset: Offset(0, (1 - value) * 8),
          child: child,
        ),
      ),
      child: content,
    );
  }
}

class _BoxActions extends StatelessWidget {
  final bool wide;
  final VoidCallback? onDiscover;
  final VoidCallback onAdd;
  const _BoxActions(
      {required this.wide, required this.onDiscover, required this.onAdd});

  @override
  Widget build(BuildContext context) {
    final style = TextButton.styleFrom(
        foregroundColor: ConsolePalette.of(context).text,
        backgroundColor: Colors.transparent,
        disabledBackgroundColor: Colors.transparent,
        minimumSize: Size(64, wide ? 56 : 48),
        padding: const EdgeInsets.symmetric(horizontal: 6),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
        textStyle: AppDesign.type(wide ? 24 : 17,
            weight: FontWeight.w500, color: ConsolePalette.of(context).text));
    Widget label(String text) => Flexible(
        fit: FlexFit.loose,
        child: FittedBox(
            fit: BoxFit.scaleDown,
            child: Text(text,
                maxLines: 1,
                softWrap: false,
                textAlign: TextAlign.center,
                style: AppDesign.type(wide ? 24 : 17,
                    weight: FontWeight.w500,
                    color: ConsolePalette.of(context).text))));
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 28 : 16, vertical: wide ? 20 : 12),
      child: Row(
        children: [
          Expanded(
              child: TextButton(
                  style: style,
                  onPressed: onDiscover,
                  child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const _ScanGlyph(),
                        const SizedBox(width: 8),
                        label('Discover mixers'),
                      ]))),
          const SizedBox(width: 12),
          Container(
              width: 1, height: 36, color: ConsolePalette.of(context).line),
          const SizedBox(width: 12),
          Expanded(
              child: TextButton(
                  style: style,
                  onPressed: onAdd,
                  child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        const _PlusGlyph(),
                        const SizedBox(width: 8),
                        label('Add manually'),
                      ]))),
        ],
      ),
    );
  }
}

class _WorkOffline extends StatelessWidget {
  final VoidCallback onTap;
  const _WorkOffline({required this.onTap});

  @override
  Widget build(BuildContext context) => TextButton(
        onPressed: onTap,
        style: TextButton.styleFrom(
            foregroundColor: ConsolePalette.of(context).text,
            backgroundColor: Colors.transparent,
            disabledBackgroundColor: Colors.transparent,
            minimumSize: const Size(200, 56),
            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
            textStyle: AppDesign.type(
                MediaQuery.sizeOf(context).width > 900 ? 26 : 20,
                weight: FontWeight.w400,
                color: ConsolePalette.of(context).text)),
        child: const Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text('Work offline'),
            SizedBox(width: 8),
            _ChevronGlyph(),
          ],
        ),
      );
}

class _HelpGlyph extends StatelessWidget {
  const _HelpGlyph();
  @override
  Widget build(BuildContext context) =>
      const DesignIcon(DesignSymbol.help, size: 28);
}

class _GearGlyph extends StatelessWidget {
  const _GearGlyph();
  @override
  Widget build(BuildContext context) =>
      const DesignIcon(DesignSymbol.settings, size: 28);
}

class _RefreshGlyph extends StatefulWidget {
  final bool spinning;
  const _RefreshGlyph({required this.spinning});
  @override
  State<_RefreshGlyph> createState() => _RefreshGlyphState();
}

class _RefreshGlyphState extends State<_RefreshGlyph>
    with SingleTickerProviderStateMixin {
  late final AnimationController _spin = AnimationController(
      vsync: this, duration: const Duration(milliseconds: 900));

  @override
  void initState() {
    super.initState();
    if (widget.spinning) _spin.repeat();
  }

  @override
  void didUpdateWidget(_RefreshGlyph old) {
    super.didUpdateWidget(old);
    if (widget.spinning && !_spin.isAnimating) {
      _spin.repeat();
    } else if (!widget.spinning && _spin.isAnimating) {
      _spin.stop();
    }
  }

  @override
  void dispose() {
    _spin.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RotationTransition(
      turns: _spin,
      child: Icon(Icons.refresh,
          size: 26,
          color: widget.spinning
              ? ConsolePalette.of(context).lime
              : ConsolePalette.of(context).text7));
}

class _ScanGlyph extends StatelessWidget {
  const _ScanGlyph();
  @override
  Widget build(BuildContext context) =>
      const DesignIcon(DesignSymbol.discover, size: 24);
}

class _PlusGlyph extends StatelessWidget {
  const _PlusGlyph();
  @override
  Widget build(BuildContext context) =>
      const DesignIcon(DesignSymbol.add, size: 24);
}

class _ChevronGlyph extends StatelessWidget {
  const _ChevronGlyph();
  @override
  Widget build(BuildContext context) => Icon(Icons.chevron_right,
      size: 22, color: ConsolePalette.of(context).text7);
}

class _CornerIcon extends StatelessWidget {
  final String tooltip;
  final VoidCallback onTap;
  final Widget child;
  const _CornerIcon(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: tooltip,
        child: Tooltip(
          message: tooltip,
          child: SizedBox(
            width: 44,
            height: 44,
            child: InkWell(
              borderRadius: BorderRadius.circular(28),
              onTap: onTap,
              child: Center(child: child),
            ),
          ),
        ),
      );
}

class _IconButton extends StatelessWidget {
  final String tooltip;
  final VoidCallback? onTap;
  final Widget child;
  const _IconButton(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: tooltip,
        child: Tooltip(
          message: tooltip,
          child: SizedBox(
            width: MediaQuery.sizeOf(context).width > 900 ? 50 : 44,
            height: MediaQuery.sizeOf(context).width > 900 ? 50 : 44,
            child: InkWell(
              borderRadius: BorderRadius.circular(8),
              onTap: onTap,
              child: Center(child: child),
            ),
          ),
        ),
      );
}

class _Dialog extends StatelessWidget {
  final String title;
  final Widget child;
  const _Dialog({required this.title, required this.child});

  @override
  Widget build(BuildContext context) => Dialog(
        backgroundColor: ConsolePalette.of(context).chip,
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(10),
            side: BorderSide(color: ConsolePalette.of(context).lineCtl)),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: SingleChildScrollView(
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(children: [
                    Expanded(
                        child: Text(title,
                            style: AppDesign.type(24,
                                weight: FontWeight.w600,
                                color: ConsolePalette.of(context).text))),
                    _IconButton(
                        tooltip: 'Close',
                        onTap: () => Navigator.pop(context),
                        child: Icon(Icons.close,
                            size: 24, color: ConsolePalette.of(context).text7)),
                  ]),
                  const SizedBox(height: 12),
                  child,
                ],
              ),
            ),
          ),
        ),
      );
}

bool _validHost(String v) {
  final ip = RegExp(r'^(?:\d{1,3}\.){3}\d{1,3}$').hasMatch(v);
  if (ip) {
    return v.split('.').every((p) {
      final n = int.tryParse(p);
      return n != null && n <= 255;
    });
  }
  return RegExp(r'^[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?$').hasMatch(v);
}

class _ManualAddDialog extends StatefulWidget {
  final void Function(String addr, String name, MixerType mixerType) onSubmit;
  const _ManualAddDialog({required this.onSubmit});

  @override
  State<_ManualAddDialog> createState() => _ManualAddDialogState();
}

class _ManualAddDialogState extends State<_ManualAddDialog> {
  final _addr = TextEditingController();
  final _name = TextEditingController();
  String? _error;
  MixerType _mixerType = MixerType.automatic;

  @override
  void dispose() {
    _addr.dispose();
    _name.dispose();
    super.dispose();
  }

  void _submit() {
    final addr = _addr.text.trim();
    if (!_validHost(addr)) {
      setState(() => _error = 'Enter a valid IP address or hostname.');
      return;
    }
    widget.onSubmit(addr, _name.text.trim(), _mixerType);
    Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) => _Dialog(
        title: 'Add mixer manually',
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            MixerTypeField(
              value: _mixerType,
              onChanged: (type) => setState(() => _mixerType = type),
            ),
            const SizedBox(height: 8),
            Text(
              _mixerType == MixerType.dl32s
                  ? 'Connects directly. Loading mixer state can take up to 90 seconds.'
                  : 'Automatically identifies supported mixers at this address.',
              style: AppDesign.type(14,
                  color: ConsolePalette.of(context).mutedText),
            ),
            const SizedBox(height: 14),
            Text('IP address or hostname',
                style: AppDesign.type(18,
                    color: ConsolePalette.of(context).mutedText)),
            const SizedBox(height: 6),
            TextField(
              controller: _addr,
              keyboardType: TextInputType.url,
              autocorrect: false,
              style: AppDesign.type(19, color: ConsolePalette.of(context).text),
              decoration: InputDecoration(
                filled: true,
                fillColor: ConsolePalette.of(context).well,
                border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lineCtl)),
                enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lineCtl)),
                focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lime)),
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 14),
              ),
              onSubmitted: (_) => _submit(),
            ),
            const SizedBox(height: 14),
            Text('Name (optional)',
                style: AppDesign.type(18,
                    color: ConsolePalette.of(context).mutedText)),
            const SizedBox(height: 6),
            TextField(
              controller: _name,
              style: AppDesign.type(19, color: ConsolePalette.of(context).text),
              decoration: InputDecoration(
                filled: true,
                fillColor: ConsolePalette.of(context).well,
                border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lineCtl)),
                enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lineCtl)),
                focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(5),
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).lime)),
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 14),
              ),
              onSubmitted: (_) => _submit(),
            ),
            if (_error != null) ...[
              const SizedBox(height: 10),
              Text(_error!,
                  style: AppDesign.type(17,
                      color: ConsolePalette.of(context).mute)),
            ],
            const SizedBox(height: 18),
            SizedBox(
              height: 52,
              child: ElevatedButton(
                onPressed: _submit,
                style: ElevatedButton.styleFrom(
                    backgroundColor: ConsolePalette.of(context).lime,
                    foregroundColor: ConsolePalette.of(context).onAccent,
                    shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(6)),
                    textStyle: AppDesign.type(19,
                        weight: FontWeight.w600,
                        color: ConsolePalette.of(context).text)),
                child: const Text('Add mixer'),
              ),
            ),
          ],
        ),
      );
}
