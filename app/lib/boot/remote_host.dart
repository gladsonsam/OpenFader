import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';

import '../engine_protocol.dart';
import 'host_transport.dart';

/// TLS endpoint and challenge learned before sending any credentials.
class RemoteServer {
  const RemoteServer(this.endpoint, this.fingerprint, this.previousFingerprint,
      this.passwordRequired);
  final Uri endpoint;
  final String fingerprint;
  final String? previousFingerprint;
  final bool passwordRequired;
  bool get identityChanged =>
      previousFingerprint != null && previousFingerprint != fingerprint;
}

/// TLS is established and the actual peer certificate checked before the
/// HttpClient receives a socket and can send any request bytes.
class RemoteHost implements HostTransport {
  RemoteHost._(this.endpoint, this._fingerprint, this.clientId, this._token);
  @override
  final Uri endpoint;
  final String _fingerprint;
  final String clientId;
  String? _token;
  WebSocket? _socket;
  Future<void>? _connecting;
  final List<String> _events = [];
  bool _disposed = false;

  static HttpClient _client(Uri endpoint, String fingerprint) {
    final context = SecurityContext(withTrustedRoots: false);
    final client = HttpClient();
    client.findProxy = (_) => 'DIRECT';
    // HTTP sees an already secured socket. Using an http/ws URI internally
    // prevents a second TLS handshake; the factory never opens a plain socket.
    client.connectionFactory = (uri, proxyHost, proxyPort) async {
      if (proxyHost != null ||
          uri.host != endpoint.host ||
          uri.port != endpoint.port) {
        throw StateError('Connection outside pinned endpoint');
      }
      var cancelled = false;
      SecureSocket? connected;
      final future = SecureSocket.connect(endpoint.host, endpoint.port,
              context: context,
              onBadCertificate: (cert) =>
                  sha256.convert(cert.der).toString() == fingerprint)
          .then<Socket>((socket) {
        connected = socket;
        final cert = socket.peerCertificate;
        if (cancelled ||
            cert == null ||
            sha256.convert(cert.der).toString() != fingerprint) {
          socket.destroy();
          throw const HandshakeException('Remote certificate pin mismatch');
        }
        return socket;
      });
      return ConnectionTask.fromSocket<Socket>(future, () {
        cancelled = true;
        connected?.destroy();
      });
    };
    return client;
  }

  static Uri parseAddress(String address) {
    var input = address.trim();
    if (input.isEmpty || RegExp(r'\s').hasMatch(input)) {
      throw const FormatException('Enter a server IP address or hostname.');
    }
    final numeric = InternetAddress.tryParse(input);
    if (numeric?.type == InternetAddressType.IPv6) input = '[$input]';
    final uri = Uri.tryParse(input.contains('://') ? input : 'https://$input');
    if (uri == null ||
        uri.scheme != 'https' ||
        uri.host.isEmpty ||
        uri.userInfo.isNotEmpty ||
        (uri.path.isNotEmpty && uri.path != '/') ||
        uri.hasQuery ||
        uri.hasFragment ||
        uri.port < 1 ||
        uri.port > 65535) {
      throw const FormatException(
          'Enter an IP address or hostname, optionally with a port. HTTPS is required.');
    }
    return uri.replace(port: uri.hasPort ? uri.port : 8845, path: '');
  }

  static String pinKey(Uri endpoint) =>
      'remote_server_pin_${sha256.convert(utf8.encode(endpoint.toString()))}';

  static Future<RemoteServer> inspect(String address,
      {String? trustedFingerprint}) async {
    final endpoint = parseAddress(address);
    // This socket only learns the server identity; it sends no application data.
    final socket = await SecureSocket.connect(endpoint.host, endpoint.port,
        context: SecurityContext(withTrustedRoots: false),
        onBadCertificate: (_) => true,
        timeout: const Duration(seconds: 4));
    final cert = socket.peerCertificate;
    socket.destroy();
    if (cert == null) {
      throw const HandshakeException('Server has no TLS certificate');
    }
    final fingerprint = sha256.convert(cert.der).toString();
    final challenge = await _request(endpoint, fingerprint, '/connect');
    if (challenge['password_required'] is! bool ||
        challenge['fingerprint'] != fingerprint) {
      throw const FormatException('Invalid server connection response');
    }
    return RemoteServer(endpoint, fingerprint, trustedFingerprint,
        challenge['password_required'] as bool);
  }

  static Future<RemoteHost> connect(RemoteServer server,
      {String? password, bool trustIdentityChange = false}) async {
    if (server.identityChanged && !trustIdentityChange) {
      throw const HandshakeException('Server identity changed');
    }
    final result = await _request(
        server.endpoint, server.fingerprint, '/connect',
        body: {if (password != null) 'password': password});
    if (result['client_id'] is! String ||
        (result['client_id'] as String).isEmpty ||
        result['token'] is! String ||
        (result['token'] as String).isEmpty) {
      throw const FormatException('Invalid server authentication response');
    }
    return RemoteHost._(server.endpoint, server.fingerprint,
        result['client_id'] as String, result['token'] as String);
  }

  static Future<Map<String, dynamic>> _request(
      Uri endpoint, String fingerprint, String path,
      {String? token,
      Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) async {
    final client = _client(endpoint, fingerprint);
    try {
      final uri = endpoint.replace(scheme: 'http', path: path);
      final req =
          await (body == null ? client.getUrl(uri) : client.postUrl(uri))
              .timeout(timeout);
      req.followRedirects = false;
      req.headers.set(HttpHeaders.acceptHeader, 'application/json');
      if (token != null) {
        req.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      }
      if (body != null) {
        req.headers.contentType = ContentType.json;
        req.write(jsonEncode(body));
      }
      final response = await req.close().timeout(timeout);
      final text = await utf8.decoder.bind(response).join().timeout(timeout);
      if (response.statusCode == 401 || response.statusCode == 403) {
        throw StateError(
            'Server password incorrect, or access denied (HTTP ${response.statusCode})');
      }
      if (response.statusCode == 429) {
        throw StateError(
            'Too many attempts. Wait a moment before trying again.');
      }
      if (response.statusCode < 200 || response.statusCode >= 300) {
        throw HttpException(
            'Remote host returned HTTP ${response.statusCode}: $text');
      }
      final value = jsonDecode(text);
      if (value is! Map) throw const FormatException('Invalid host response');
      return Map<String, dynamic>.from(value);
    } finally {
      client.close(force: true);
    }
  }

  String get _credential =>
      _token ?? (throw StateError('Remote host disconnected'));
  @override
  bool get supportsAdministration => false;
  @override
  Future<void> quitHost() async =>
      throw UnsupportedError('Remote host administration unavailable');
  @override
  Future<Map<String, dynamic>> recorderRequest(String path,
      {Map<String, dynamic>? body,
      Duration timeout = const Duration(seconds: 4)}) {
    if (!RegExp(r'^[a-z]+(?:/[a-z]+)*$').hasMatch(path)) {
      throw ArgumentError.value(path);
    }
    return _request(endpoint, _fingerprint, '/api/recorder/v1/$path',
        token: _credential, body: body, timeout: timeout);
  }

  /// Permanently drops in-memory credentials and closes the active socket.
  Future<void> disconnect() async {
    _disposed = true;
    _token = null;
    final socket = _socket;
    _socket = null;
    await socket?.close();
    _events.clear();
  }

  Future<void> _connect() =>
      _connecting ??= _open().whenComplete(() => _connecting = null);
  Future<void> _open() async {
    final token = _credential;
    if (_socket != null) return;
    final client = _client(endpoint, _fingerprint);
    try {
      final socket = await WebSocket.connect(
              endpoint.replace(scheme: 'ws', path: '/ws').toString(),
              headers: {HttpHeaders.authorizationHeader: 'Bearer $token'},
              customClient: client)
          .timeout(const Duration(seconds: 3));
      if (_disposed) {
        await socket.close();
        throw StateError('Remote host disconnected');
      }
      _socket = socket;
      socket.listen((data) {
        if (data is String) _events.add(data);
      }, onDone: () {
        if (identical(_socket, socket)) _socket = null;
        client.close(force: true);
      }, onError: (_) {
        if (identical(_socket, socket)) _socket = null;
        client.close(force: true);
      });
      socket.add(Cmds.hello(false));
    } catch (_) {
      client.close(force: true);
      rethrow;
    }
  }

  @override
  Future<String> boot() async {
    await _connect();
    return 'remote';
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
