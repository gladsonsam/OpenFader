// Shell-owned connection discovery, intent and recovery adapter.
import 'dart:async';
import 'dart:io';
import '../core/wifi_net.dart';
import '../connection/recovery.dart';
import '../settings/prefs.dart';
import '../engine.dart';
import '../screens/connection.dart';
import 'store_adapter.dart';

class ShellConnectionStore extends ShellStoreAdapter
    implements ConnectionStore {
  ShellConnectionStore(super.app) {
    _lastError = app.visibleError;
    app.addListener(_settle);
    _recovery = ConnectionRecovery(_recover)..addListener(_recoveryChanged);
  }

  late final ConnectionRecovery _recovery;

  void _recoveryChanged() {
    if (!isDisposed) notifyListeners();
  }

  void tickRecovery() => _recovery.tick(
        enabled: PrefsService.instance.reconnectLast,
        busy: _probing,
        connected: engine.connected,
        state: engine.linkState,
        driver: engine.driver,
        host: engine.linkIp,
      );

  Future<bool> _recover(RecoveryTarget target) async {
    final run = _connectionRun;
    if (!await WifiNet.prepareConnection(target.host)) {
      throw StateError(
          'Could not prepare the network route to ${target.host}.');
    }
    if (isDisposed ||
        run != _connectionRun ||
        !PrefsService.instance.reconnectLast) {
      return false;
    }
    app.clearErrors();
    await app.sendCommand(Cmds.connect(target.driver, target.host));
    return !isDisposed &&
        run == _connectionRun &&
        engine.connected &&
        engine.driver == target.driver &&
        engine.linkIp == target.host;
  }

  bool _scanning = false;
  String _lastScan = 'never';
  Timer? _scanTimer;

  // Desk unicast can be lost, so one probe shot is not enough.

  String _lastProbe = '';
  MixerType _lastMixerType = MixerType.automatic;
  String _connectionProgress = '';
  int _connectionRun = 0;
  int get connectionRun => _connectionRun;
  int _probeAttempt = 0;
  bool _probing = false;

  // Scanning ends on an engine answer; the timer is only a fallback so the spinner cannot stick.

  String _lastError = '';
  String? _deviceRequest;
  String? _operationError;
  final Map<String, String> _checks = {};
  final Map<String, String> _canonicalHosts = {};

  @override
  void dispose() {
    _connectionRun++;
    _recovery.dispose();
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
      final found = engine.devices.length;
      _lastScan =
          '${complete?['operation']} · $found ${found == 1 ? 'mixer' : 'mixers'} · ${_fmtTime(DateTime.now())}';
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
    if (_recovery.active) return 'Reconnecting · ${engine.linkName}';
    if (engine.linkState == 'reconnecting') {
      return 'Connection lost · ${engine.linkName}';
    }
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
    if (_recovery.status.isNotEmpty) return _recovery.status;
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

  // Failures persist until a good poll or the next scan or probe clears them.

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
            mixerType: MixerType.automatic,
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
    WifiNet.ensureScanPermissions().then((granted) async {
      if (isDisposed || _deviceRequest != request) return;
      if (!granted) {
        _scanning = false;
        _deviceRequest = null;
        _operationError = 'Local network permission is required.';
        notifyListeners();
        return;
      }
      final prepared = await WifiNet.scanStart();
      if (!prepared && !isDisposed && _deviceRequest == request) {
        _scanning = false;
        _deviceRequest = null;
        _operationError =
            'Could not prepare local network discovery. Check Wi-Fi and try again.';
        notifyListeners();
        return;
      }
      if (isDisposed || _deviceRequest != request) {
        await WifiNet.scanStop();
        return;
      }
      send(hosts == null
          ? Cmds.discoverDevices(request)
          : Cmds.refreshDevices(request, hosts));
      // Permission dialogs can take arbitrarily long. Start the network
      // budget only after the user grants access and the request is sent.
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
    });
  }

  @override
  void probe(String addr, {MixerType mixerType = MixerType.automatic}) {
    // Ignore repeated submissions for the same pending target and type.

    if (_probing || addr.trim().isEmpty) return;
    _cancelConnection();
    _lastProbe = addr.trim();
    _lastMixerType = mixerType;
    unawaited(_probeLoop(_lastProbe));
  }

  // Explicit Retry preserves the selected type; automatic recovery uses the active provider key.

  @override
  Future<void> retryProbe() {
    _cancelConnection();
    return _probeLoop(_lastProbe.isNotEmpty ? _lastProbe : engine.linkIp);
  }

  Future<void> _probeLoop(String addr) async {
    if (addr.isEmpty || _probing || isDisposed) return;
    final run = ++_connectionRun;
    final direct = _lastMixerType != MixerType.automatic;
    bool cancelled() => isDisposed || run != _connectionRun;
    bool connected() =>
        engine.connected &&
        engine.linkIp == addr &&
        (!direct || engine.driver == _lastMixerType.driver);
    _probing = true;
    _operationError = null;
    _probeAttempt = 0;
    _connectionProgress = _lastMixerType == MixerType.automatic
        ? 'Connecting to $_lastProbe…'
        : 'Connecting to ${_lastMixerType.label} at $_lastProbe…';
    app.clearErrors();
    notifyListeners();
    try {
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
      if (!await WifiNet.prepareConnection(addr)) {
        _operationError =
            'Could not prepare the network connection to $addr. Check Wi-Fi or VPN access.';
        return;
      }
      if (cancelled()) return;
      // One handshake only: restarting while a desk streams its initial state
      // prevents slower protocols from ever finishing (including auto-detected ones).
      const attempts = 1;
      for (var i = 0; i < attempts; i++) {
        _probeAttempt = i + 1;
        _connectionProgress = direct
            ? 'Connecting to ${_lastMixerType.label} at $_lastProbe…'
            : 'Connecting to mixer at $_lastProbe…';
        notifyListeners();
        send(direct
            ? Cmds.connect(_lastMixerType.driver, addr)
            : Cmds.probe(addr));
        // Wait for this handshake to finish before starting another connection.

        final deadline = DateTime.now().add(const Duration(seconds: 90));
        while (!cancelled() && DateTime.now().isBefore(deadline)) {
          if (connected() || app.visibleError.isNotEmpty) break;
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        if (cancelled() || connected() || app.visibleError.isNotEmpty) break;
      }
      if (!cancelled() && !connected() && app.visibleError.isEmpty) {
        _operationError = direct
            ? 'Still waiting for ${_lastMixerType.label} at $_lastProbe after 90 seconds. '
                'The connection may still complete; Retry starts a new connection.'
            : 'Still waiting for a supported mixer at $_lastProbe after 90 seconds. Retry starts a new connection.';
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
    _recovery.cancel();
    _probing = false;
    _connectionProgress = '';
  }

  @override
  void connectDevice(String id) {
    if (_probing) return;
    final host = hostOf(id);
    if (host.isNotEmpty) {
      final device = devices.firstWhere((d) => d.id == id);
      probe(host, mixerType: device.mixerType);
      return;
    }
    _cancelConnection();
    send(Cmds.connectDevice(id));
  }

  @override
  void connectSim() {
    _cancelConnection();
    unawaited(WifiNet.unbindWifi());
    send(Cmds.connectSim());
  }

  @override
  void disconnect() {
    _cancelConnection();
    unawaited(WifiNet.unbindWifi());
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
