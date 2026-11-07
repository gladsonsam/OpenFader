import 'dart:async';
import 'dart:io';
import '../boot/desktop_host.dart';
import 'dart:math';
import 'package:flutter/services.dart';
import '../screens/recorder.dart';
import '../boot/host_transport.dart';
import 'recorder_command_gate.dart';

class ShellRecorderStore extends RecorderStore {
  ShellRecorderStore({required this.host, required this.names});
  final HostTransport? host;
  final Map<int, String> Function() names;
  @override
  Map<int, String> get channelNames => names();
  bool _connected = false,
      _pending = false,
      _refreshing = false,
      _stateRefreshing = false,
      _disposed = false;
  String? _error;
  Map<String, dynamic> _state = {};
  List<Map<String, dynamic>> _devices = [], _takes = [];
  DateTime _lastRefresh = DateTime.fromMillisecondsSinceEpoch(0);
  late final RecorderCommandGate _gate = RecorderCommandGate(_notify);
  bool _uncertain = false;
  bool _observedUncertainPending = false;
  String? _uncertainTransferInstance;
  final _random = Random.secure();
  @override
  bool get connected => _connected;
  @override
  bool get pending =>
      _pending ||
      _gate.pending ||
      _uncertain ||
      _state['soundcheck_pending'] == true;
  @override
  String? get error => _error;
  @override
  Map<String, dynamic> get state => _state;
  @override
  List<Map<String, dynamic>> get devices => _devices;
  @override
  List<Map<String, dynamic>> get takes => _takes;
  void _notify() {
    if (!_disposed) notifyListeners();
  }

  Future<Map<String, dynamic>> _request(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) async {
    final currentHost = host;
    if (currentHost == null) throw StateError('Desktop host unavailable');
    return currentHost.recorderRequest(path, body: body, timeout: timeout);
  }

  @override
  bool get canAdminRemote => host is DesktopHost;
  @override
  Map<String, dynamic> get remoteStatus => _remoteStatus;
  @override
  List<String> get remoteClients => _remoteClients;
  @override
  String? get remoteError => _remoteError;
  @override
  bool get remotePending => _remotePending;
  Map<String, dynamic> _remoteStatus = {};
  List<String> _remoteClients = [];
  String? _remoteError;
  bool _remotePending = false, _remoteRefreshing = false;
  DateTime _remoteLastRefresh = DateTime.fromMillisecondsSinceEpoch(0);

  Future<String?> _microphonePermission({bool request = false}) async {
    if (!Platform.isMacOS || host is! DesktopHost) return null;
    final status = await const MethodChannel('openfader/audio_permission')
        .invokeMethod<String>(
            request ? 'requestMicrophone' : 'microphoneStatus');
    await (host as DesktopHost).remoteAdminRequest('microphone',
        body: {'authorized': status == 'authorized'});
    return status;
  }

  Future<void> _refreshRemote() async {
    if (!canAdminRemote || _remoteRefreshing || _disposed) return;
    _remoteRefreshing = true;
    _remoteLastRefresh = DateTime.now();
    try {
      await _microphonePermission();
      final desktop = host as DesktopHost;
      final status = await desktop.remoteAdminRequest('status');
      final clients = await desktop.remoteAdminRequest('clients');
      if (_disposed) return;
      _remoteStatus = status;
      _remoteClients = (clients['clients'] is List)
          ? (clients['clients'] as List).whereType<String>().toList()
          : [];
    } catch (e) {
      _remoteError = 'Remote administration: $e';
    } finally {
      _remoteRefreshing = false;
      _notify();
    }
  }

  @override
  Future<void> remoteAction(String action, [String? value]) async {
    if (!canAdminRemote || _remotePending) return;
    _remotePending = true;
    _remoteError = null;
    _notify();
    try {
      if (action == 'allowRecording') {
        if (!Platform.isMacOS) {
          throw StateError(
              'Microphone access is managed by the operating system and audio interface driver.');
        }
        final status = await _microphonePermission(request: true);
        if (status != 'authorized') {
          throw StateError(
              'Enable OpenFader in macOS Privacy & Security → Microphone ($status).');
        }
        return;
      }
      await (host as DesktopHost).remoteAdminRequest(action,
          body: action == 'enable'
              ? {'ip': '0.0.0.0'}
              : action == 'password'
                  ? {'password': value ?? ''}
                  : action == 'revoke'
                      ? {'client_id': value}
                      : {});
    } catch (e) {
      _remoteError = 'Remote administration: $e';
    } finally {
      _remotePending = false;
      _notify();
      await _refreshRemote();
    }
  }

  void tick() {
    if (canAdminRemote &&
        DateTime.now().difference(_remoteLastRefresh) >=
            const Duration(seconds: 2)) {
      unawaited(_refreshRemote());
    }
    if ((_state['transport'] == 'recording' ||
            _state['transport'] == 'playing') &&
        !_stateRefreshing) {
      unawaited(_refreshLiveState());
    }
    if (_refreshing ||
        DateTime.now().difference(_lastRefresh) < const Duration(seconds: 2)) {
      return;
    }
    unawaited(refresh());
  }

  Future<void> _refreshLiveState() async {
    _stateRefreshing = true;
    try {
      final result = await _request('state');
      if (!_disposed) {
        _state = {..._state, ...result};
        _connected = true;
        _notify();
      }
    } catch (_) {
      // The regular refresh reports connection errors and retries.
    } finally {
      _stateRefreshing = false;
    }
  }

  Future<void> refresh() async {
    if (_refreshing || _disposed) return;
    _refreshing = true;
    final transferInstance = _uncertainTransferInstance;
    _lastRefresh = DateTime.now();
    Future<(Map<String, dynamic>?, String?)> read(String path) async {
      try {
        final result = await _request(path);
        // State diagnostics describe capture/playback failures on a reachable
        // host. Keep that state visible and Stop/Recover available.
        if (path != 'state' && result['error'] != null) {
          throw StateError('${result['error']}');
        }
        return (result, null);
      } catch (e) {
        return (null, '$path: $e');
      }
    }

    try {
      final results =
          await Future.wait([read('state'), read('devices'), read('takes')]);
      if (_disposed) return;
      final state = results[0].$1;
      _connected = state != null;
      if (state != null) {
        _state = state;
        if (transferInstance != null &&
            _uncertainTransferInstance == transferInstance &&
            host is DesktopHost &&
            (host as DesktopHost).instanceId == transferInstance) {
          // This state request was queued behind the transfer on the same host.
          // Preserve the unknown-outcome error and staging for inspection.
          _uncertainTransferInstance = null;
          _uncertain = false;
        } else if (_uncertain &&
            _uncertainTransferInstance == null &&
            state['soundcheck_pending'] == true) {
          _observedUncertainPending = true;
        } else if (_uncertain &&
            _observedUncertainPending &&
            state['soundcheck_pending'] == false) {
          _uncertain = false;
          _observedUncertainPending = false;
        }
      }
      final deviceResult = results[1].$1;
      _devices = _list(deviceResult?['devices']);
      if (deviceResult != null) {
        _state = {
          ..._state,
          'device_scan_pending': deviceResult['scan_pending'],
          'device_scan_error': deviceResult['scan_error']
        };
      }
      final takes = results[2].$1;
      if (takes != null) {
        _takes = _list(takes['takes']).map((take) {
          final manifest = take['manifest'];
          return manifest is Map
              ? {
                  ...take,
                  'duration_frames': manifest['frames'],
                  'sample_rate': manifest['sample_rate'],
                  'complete': manifest['complete']
                }
              : take;
        }).toList();
      }
      final failures =
          results.map((result) => result.$2).whereType<String>().toList();
      if (_error == null || _error!.startsWith('Recorder host:')) {
        _error =
            failures.isEmpty ? null : 'Recorder host: ${failures.join('; ')}';
      }
    } finally {
      _refreshing = false;
      _notify();
    }
  }

  List<Map<String, dynamic>> _list(dynamic value) => value is List
      ? value.whereType<Map>().map((e) => Map<String, dynamic>.from(e)).toList()
      : [];
  @override
  Future<void> command(String action, [Map<String, dynamic> args = const {}]) {
    if ((pending && action != 'stop') || _disposed) return Future.value();
    return _gate.run(action, () async {
      if (_disposed) {
        return;
      }
      _error = null;
      _notify();
      final id = List.generate(
              16, (_) => _random.nextInt(256).toRadixString(16).padLeft(2, '0'))
          .join();
      final commandInstance =
          host is DesktopHost ? (host as DesktopHost).instanceId : null;
      try {
        if (action == 'start_recording' &&
            host?.supportsAdministration == true &&
            Platform.isMacOS) {
          final status = await _microphonePermission(request: true);
          if (status != 'authorized') {
            final message = switch (status) {
              'pending' =>
                'The macOS microphone request is still pending. Respond to the system prompt, then retry recording.',
              'denied' =>
                'Microphone access was denied. Enable OpenFader in macOS Privacy & Security → Microphone, then retry.',
              'restricted' => 'Microphone access is restricted by macOS.',
              'notDetermined' =>
                'Microphone access has not yet been decided. Retry recording to request access.',
              _ => 'Could not determine microphone permission ($status).',
            };
            throw StateError(message);
          }
        }
        final result = await _request('command',
            body: {'request_id': id, 'action': action, 'args': args},
            timeout: const Duration(seconds: 60));
        if (result['request_id'] != id || result['ok'] != true) {
          throw StateError(
              '${result['error'] ?? 'Command was not acknowledged'}');
        }
      } on TimeoutException catch (e) {
        _uncertain = true;
        if ((action == 'import_take' || action == 'export_take') &&
            commandInstance != null) {
          _uncertainTransferInstance = commandInstance;
        }
        _error =
            'Command $action response timed out: $e. Host outcome is unknown; waiting for host state.';
      } catch (e) {
        // Permission and command validation failures do not disconnect a
        // reachable host. The refresh below still detects a lost transport.
        if (e is IOException) _connected = false;
        _error = 'Command $action failed: $e';
      } finally {
        _notify();
        await refresh();
      }
    });
  }

  @override
  Future<Map<String, dynamic>> inspectTake(String takeId) async {
    final result = await _request('inspect', body: {'take_id': takeId});
    if (result['error'] != null) throw StateError('${result['error']}');
    return result;
  }

  final Map<String, Map<String, dynamic>> _waveformCache = {};
  @override
  Future<Map<String, dynamic>> waveform(
      String takeId, int start, int end, int buckets) async {
    final key = '$takeId:$start:$end:$buckets';
    final cached = _waveformCache[key];
    if (cached != null) return cached;
    final result = await _request('waveform', body: {
      'take_id': takeId,
      'start_frame': start,
      'end_frame': end,
      'buckets': buckets
    });
    if (result['error'] != null) throw StateError('${result['error']}');
    if (_waveformCache.length >= 24) {
      _waveformCache.remove(_waveformCache.keys.first);
    }
    // A live interval can gain more published PCM bins on the next shell poll.
    if (result['complete'] != false) _waveformCache[key] = result;
    return result;
  }

  @override
  Future<void> refreshDevices() async {
    if (_pending || _disposed) return;
    _pending = true;
    _error = null;
    _notify();
    try {
      await _request('devices/refresh', body: {});
    } catch (e) {
      _error = 'Could not refresh audio devices: $e';
    } finally {
      _pending = false;
      _notify();
      await refresh();
    }
  }

  @override
  Future<bool> quitHost() async {
    if (host == null || _pending) return false;
    _pending = true;
    _error = null;
    _notify();
    try {
      await host!.quitHost();
      return true;
    } catch (e) {
      _error = 'Could not quit host: $e';
      return false;
    } finally {
      _pending = false;
      _notify();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
