import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import '../engine.dart';
import 'host_transport.dart';

/// Connects the desktop UI to the independently running local host.
class DesktopHost implements HostTransport {
  DesktopHost._(this.endpoint, this.token, this.instanceId);
  @override
  Uri endpoint;
  String token;
  String instanceId;
  WebSocket? _socket;
  final List<String> _events = [];
  Future<void>? _connecting;

  static String get _directory => Platform.isMacOS
      ? '${Platform.environment['HOME']}/Library/Application Support/OpenFader/Recorder'
      : '${Platform.environment['LOCALAPPDATA']}\\OpenFader\\Recorder';

  static Future<bool> _privatePath(String path,
      {required bool directory}) async {
    final type = await FileSystemEntity.type(path, followLinks: false);
    if (type !=
        (directory
            ? FileSystemEntityType.directory
            : FileSystemEntityType.file)) {
      return false;
    }
    if (Platform.isMacOS) {
      final stat = await Process.run('/usr/bin/stat', ['-f', '%u %Lp', path]);
      final user = await Process.run('/usr/bin/id', ['-u']);
      if (stat.exitCode != 0 || user.exitCode != 0) return false;
      final fields = '${stat.stdout}'.trim().split(' ');
      final mode =
          fields.length == 2 ? int.tryParse(fields[1], radix: 8) : null;
      return fields.length == 2 &&
          fields[0] == '${user.stdout}'.trim() &&
          mode != null &&
          (mode & 0x3f) == 0 &&
          (mode & (directory ? 0x1c0 : 0x180)) == (directory ? 0x1c0 : 0x180);
    }
    // Match the host's protected current-user and SYSTEM ACL before reading
    // the bearer credential. Translation failures must reject the descriptor.
    const script =
        r'''$ErrorActionPreference='Stop'; $a=Get-Acl -LiteralPath $env:OPENFADER_DESCRIPTOR_PATH; $u=[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value; if ($a.Owner -ne (New-Object System.Security.Principal.SecurityIdentifier($u)).Translate([System.Security.Principal.NTAccount]).Value -or !$a.AreAccessRulesProtected) { exit 1 }; foreach($r in $a.Access) { $sid=$r.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value; if($r.IsInherited -or ($r.AccessControlType -eq 'Allow' -and $sid -ne $u -and $sid -ne 'S-1-5-18')) { exit 1 } }''';
    final result = await Process.run(
        'powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script],
        environment: {'OPENFADER_DESCRIPTOR_PATH': path});
    return result.exitCode == 0;
  }

  static Future<DesktopHost?> _discover() async {
    try {
      final file = File('$_directory${Platform.pathSeparator}host.json');
      if (!await _privatePath(_directory, directory: true) ||
          !await _privatePath(file.path, directory: false)) {
        return null;
      }
      final descriptor = jsonDecode(await file.readAsString());
      if (descriptor is! Map ||
          descriptor['version'] != 1 ||
          descriptor['instance_id'] is! String ||
          descriptor['token'] is! String ||
          (descriptor['token'] as String).length < 32 ||
          descriptor['endpoint'] is! String) {
        return null;
      }
      final uri = Uri.parse(descriptor['endpoint'] as String);
      if (uri.scheme != 'http' ||
          uri.host != '127.0.0.1' ||
          !uri.hasPort ||
          uri.userInfo.isNotEmpty ||
          uri.path.isNotEmpty ||
          uri.hasQuery ||
          uri.hasFragment) {
        return null;
      }
      final host = DesktopHost._(uri, descriptor['token'] as String,
          descriptor['instance_id'] as String);
      final identity = await host._request('host');
      if (identity['version'] != 1 ||
          identity['instance_id'] != host.instanceId) {
        return null;
      }
      return host;
    } catch (_) {
      return null;
    }
  }

  static Future<DesktopHost> start() async {
    var host = await _discover();
    if (host != null) return host;
    final executable = Platform.environment['OPENFADER_SERVE_PATH'] ??
        (Platform.isMacOS
            ? '${File(Platform.resolvedExecutable).parent.path}/openfader-serve'
            : '${File(Platform.resolvedExecutable).parent.path}\\openfader-serve.exe');
    final process = await Process.start(executable, ['--desktop-host'],
        mode: ProcessStartMode.detached);
    // A competing launch can win the host lock. Discovery, not child exit,
    // determines whether startup succeeded.
    for (var i = 0; i < 100; i++) {
      host = await _discover();
      if (host != null) return host;
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    throw StateError(
        'Could not discover a secure recorder host (launch PID ${process.pid}).');
  }

  Future<Map<String, dynamic>> _request(String path,
      {bool post = false}) async {
    final client = HttpClient();
    try {
      final uri = endpoint.resolve('/api/recorder/v1/$path');
      final request = await (post ? client.postUrl(uri) : client.getUrl(uri))
          .timeout(const Duration(seconds: 3));
      request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      if (post) {
        request.headers.contentType = ContentType.json;
        request.write('{}');
      }
      final response =
          await request.close().timeout(const Duration(seconds: 3));
      final text = await utf8.decoder
          .bind(response)
          .join()
          .timeout(const Duration(seconds: 3));
      final value = jsonDecode(text);
      if (value is! Map) throw const FormatException('Invalid host response');
      if (response.statusCode != 200 || (post && value['ok'] != true)) {
        throw StateError(
            '${value['error'] ?? 'Host returned HTTP ${response.statusCode}'}');
      }
      return Map<String, dynamic>.from(value);
    } finally {
      client.close(force: true);
    }
  }

  @override
  bool get supportsAdministration => true;

  @override
  Future<Map<String, dynamic>> recorderRequest(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) async {
    final client = HttpClient();
    try {
      final uri = endpoint.resolve('/api/recorder/v1/$path');
      final request =
          await (body == null ? client.getUrl(uri) : client.postUrl(uri))
              .timeout(timeout);
      request.headers.set(HttpHeaders.acceptHeader, 'application/json');
      request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      if (body != null) {
        request.headers.contentType = ContentType.json;
        request.write(jsonEncode(body));
      }
      final response = await request.close().timeout(timeout);
      final text = await utf8.decoder.bind(response).join().timeout(timeout);
      if (response.statusCode < 200 || response.statusCode >= 300) {
        throw HttpException(
            'Recorder host returned HTTP ${response.statusCode}: $text');
      }
      final value = jsonDecode(text);
      if (value is! Map) {
        throw const FormatException('Invalid recorder response');
      }
      return Map<String, dynamic>.from(value);
    } finally {
      client.close(force: true);
    }
  }

  Future<Map<String, dynamic>> remoteAdminRequest(String path,
      {Map<String, dynamic>? body}) async {
    final client = HttpClient();
    try {
      final uri = endpoint.resolve('/api/remote/v1/$path');
      final request =
          await (body == null ? client.getUrl(uri) : client.postUrl(uri))
              .timeout(const Duration(seconds: 4));
      request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      if (body != null) {
        request.headers.contentType = ContentType.json;
        request.write(jsonEncode(body));
      }
      final response =
          await request.close().timeout(const Duration(seconds: 4));
      final text = await utf8.decoder
          .bind(response)
          .join()
          .timeout(const Duration(seconds: 4));
      if (response.statusCode < 200 || response.statusCode >= 300) {
        throw HttpException(
            'Remote administration HTTP ${response.statusCode}: $text');
      }
      final value = jsonDecode(text);
      if (value is! Map) {
        throw const FormatException('Invalid administration response');
      }
      if (value['error'] != null) throw StateError('${value['error']}');
      return Map<String, dynamic>.from(value);
    } finally {
      client.close(force: true);
    }
  }

  @override
  Future<void> quitHost() async {
    await _request('shutdown', post: true);
  }

  Future<void> _connect() =>
      _connecting ??= _open().whenComplete(() => _connecting = null);
  Future<void> _open() async {
    if (_socket != null) return;
    try {
      await _openSocket();
    } catch (_) {
      final replacement = await start();
      endpoint = replacement.endpoint;
      token = replacement.token;
      instanceId = replacement.instanceId;
      await _openSocket();
    }
  }

  Future<void> _openSocket() async {
    final socket = await WebSocket.connect(
            endpoint.replace(scheme: 'ws', path: '/ws').toString(),
            headers: {HttpHeaders.authorizationHeader: 'Bearer $token'})
        .timeout(const Duration(seconds: 3));
    _socket = socket;
    socket.listen((data) {
      if (data is String) _events.add(data);
    }, onDone: () {
      if (identical(_socket, socket)) _socket = null;
    }, onError: (_) {
      if (identical(_socket, socket)) _socket = null;
    });
    socket.add(Cmds.hello(false));
  }

  @override
  Future<String> boot() async {
    await _connect();
    return 'desktop';
  }

  @override
  Future<void> cmd(String clientId, String json) async {
    await _connect();
    _socket!.add(json);
  }

  @override
  Future<List<String>> poll(String clientId, int cap) async {
    await _connect();
    final count = min(cap, _events.length);
    final result = _events.sublist(0, count);
    _events.removeRange(0, count);
    return result;
  }

  @override
  Future<Uint8List> pollBinary(String clientId, int cap) async =>
      throw UnsupportedError('JSON transport');
  @override
  Future<void> resync(String clientId) async {
    await _connect();
    _socket!.add(Cmds.info());
  }
}
