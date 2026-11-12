import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;
import 'package:web_socket_channel/web_socket_channel.dart';

import '../engine_protocol.dart';
import 'host_transport.dart';

class RemoteServer {
  const RemoteServer(this.endpoint, this.passwordRequired);
  final Uri endpoint;
  final bool passwordRequired;
  String get fingerprint => '';
  String? get previousFingerprint => null;
  bool get identityChanged => false;
}

/// Same-origin browser transport. The server owns the HttpOnly session cookie;
/// passwords and bearer credentials are never saved in browser storage.
class RemoteHost implements HostTransport {
  RemoteHost._(this.endpoint, this.clientId);
  @override
  final Uri endpoint;
  final String clientId;
  WebSocketChannel? _socket;
  Future<void>? _connecting;
  final _events = <String>[];
  bool _disposed = false;

  static Uri parseAddress(String address) {
    final origin = Uri.parse(Uri.base.origin);
    if (address.isNotEmpty && Uri.parse(address).origin != origin.origin) {
      throw UnsupportedError(
          'Open the other server’s web address in your browser.');
    }
    if (origin.scheme != 'http' && origin.scheme != 'https') {
      throw StateError('Open OpenFader from its hosting server.');
    }
    return origin;
  }

  static String pinKey(Uri endpoint) => 'browser_identity_managed_by_browser';

  static Future<RemoteServer> inspect(String address,
      {String? trustedFingerprint}) async {
    final endpoint = parseAddress(address);
    final challenge = await _request(endpoint, '/web/connect');
    if (challenge['password_required'] is! bool) {
      throw const FormatException('Invalid server connection response');
    }
    return RemoteServer(endpoint, challenge['password_required'] as bool);
  }

  static Future<RemoteHost> connect(RemoteServer server,
      {String? password, bool trustIdentityChange = false}) async {
    final response = await _request(server.endpoint, '/web/connect',
        body: {'password': ?password});
    final id = response['client_id'];
    if (id is! String || id.isEmpty) {
      throw const FormatException('Invalid server authentication response');
    }
    return RemoteHost._(server.endpoint, id);
  }

  static Future<Map<String, dynamic>> _request(Uri endpoint, String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) async {
    final uri = endpoint.replace(path: path);
    final response = await (body == null
            ? http.get(uri, headers: {'Accept': 'application/json'})
            : http.post(uri,
                headers: {
                  'Content-Type': 'application/json',
                  'Accept': 'application/json'
                },
                body: jsonEncode(body)))
        .timeout(timeout);
    if (response.statusCode == 401 || response.statusCode == 403) {
      throw StateError('Server password incorrect, or session expired.');
    }
    if (response.statusCode == 429) {
      throw StateError('Too many attempts. Wait a moment before trying again.');
    }
    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw StateError('Server returned HTTP ${response.statusCode}.');
    }
    final decoded = jsonDecode(response.body);
    if (decoded is! Map) throw const FormatException('Invalid server response');
    return Map<String, dynamic>.from(decoded);
  }

  Future<void> _connect() =>
      _connecting ??= _open().whenComplete(() => _connecting = null);

  Future<void> _open() async {
    if (_disposed) throw StateError('Server disconnected');
    if (_socket != null) return;
    // Fetch exposes an expired session reliably; browser WebSocket failures
    // deliberately hide the HTTP authentication response.
    await _request(endpoint, '/api/recorder/v1/state');
    final socket = WebSocketChannel.connect(endpoint.replace(
        scheme: endpoint.scheme == 'https' ? 'wss' : 'ws', path: '/ws'));
    try {
      await socket.ready.timeout(const Duration(seconds: 5));
    } catch (_) {
      // A failed handshake has no live stream to finish a close handshake.
      // Drain its error without holding the shell's only poll loop open.
      socket.stream.listen((_) {}, onError: (Object _) {});
      unawaited(socket.sink.close().then<void>((_) {}, onError: (Object _) {}));
      throw StateError(
          'Server unavailable or session expired. Reload this page to sign in again.');
    }
    if (_disposed) {
      await socket.sink.close();
      return;
    }
    _socket = socket;
    socket.stream.listen((event) {
      if (event is String) _events.add(event);
    }, onDone: () {
      if (identical(_socket, socket)) _socket = null;
    }, onError: (Object error) {
      if (identical(_socket, socket)) _socket = null;
    });
    socket.sink.add(Cmds.hello(false));
  }

  @override
  bool get supportsAdministration => false;
  @override
  Future<void> quitHost() async =>
      throw UnsupportedError('Close this browser tab to leave OpenFader.');
  @override
  Future<Map<String, dynamic>> recorderRequest(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) {
    if (!RegExp(r'^[a-z]+(?:/[a-z]+)*$').hasMatch(path)) {
      throw ArgumentError.value(path);
    }
    return _request(endpoint, '/api/recorder/v1/$path',
        body: body, timeout: timeout);
  }

  @override
  Future<String> boot() async {
    await _connect();
    return clientId;
  }

  @override
  Future<void> cmd(String clientId, String json) async {
    await _connect();
    _socket!.sink.add(json);
  }

  @override
  Future<List<String>> poll(String clientId, int cap) async {
    await _connect();
    final count = cap.clamp(0, _events.length);
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
    _socket!.sink.add(Cmds.info());
  }

  Future<void> disconnect() async {
    _disposed = true;
    await _socket?.sink.close();
    _socket = null;
    _events.clear();
    await _request(endpoint, '/web/disconnect', body: {});
  }
}
