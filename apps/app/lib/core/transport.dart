// W2a transport: the socket seam under the store. One WS transport over
// `package:web_socket_channel`:
// auto-reconnect with backoff, `hello {binary_meters:true, taps:true}` on
// every connect, and unconsumed mixer writes dropped on reconnect (only
// connection/handshake intent survives an unopened link).
library;

import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:web_socket_channel/web_socket_channel.dart';

/// One engine frame: JSON text or a WS BINARY meter frame.
typedef TransportMessage = Object; // String | Uint8List
typedef Unsub = void Function();

/// Minimal channel shape WsTransport drives (WebSocketChannel compatible).
typedef ChannelFactory = WebSocketChannel Function(Uri url);

/// Only explicit connection/handshake intent survives an unopened link.
/// Mixer writes belong to the old epoch and must never replay on reconnect.
bool queueable(String json) {
  try {
    final c = jsonDecode(json);
    if (c is! Map) return false;
    return const {
      'connect',
      'connect_device',
      'probe_desk',
      'hello',
      'offline',
    }.contains(c['cmd']);
  } catch (_) {
    return false;
  }
}

abstract class Transport {
  String get url;

  /// Open the socket (idempotent; reconnects automatically on drop for WS).
  void connect();

  /// Send one engine command (JSON string from the protocol builders).
  void send(String json);

  Unsub onMessage(void Function(TransportMessage msg) cb);
  Unsub onStatus(void Function(bool online) cb);
  Future<void> close();

  /// Same-socket FIFO command completion; null when unsupported.
  Future<void>? barrier() => null;
}

/// Backoff schedule in ms (mirrors the web DEFAULT_BACKOFF).
const List<int> defaultBackoffMs = [1000, 2000, 5000, 10000];

class WsTransport implements Transport {
  @override
  final String url;
  final ChannelFactory? channelFactory;
  final List<int> backoffMs;
  final bool binaryMeters;
  final bool wantTaps;

  final Set<void Function(TransportMessage)> _msgCbs = {};
  final Set<void Function(bool)> _statusCbs = {};

  WebSocketChannel? _channel;
  StreamSubscription? _sub;
  final List<String> _queue = [];
  Timer? _retryTimer;
  bool _online = false;
  bool _wantOpen = false;
  int _failures = 0;
  bool _manuallyClosed = false;
  int _helloIssued = 0;
  int _helloReceived = 0;

  WsTransport(
    this.url, {
    this.channelFactory,
    this.backoffMs = defaultBackoffMs,
    this.binaryMeters = true,
    this.wantTaps = true,
  });

  @override
  void connect() {
    _wantOpen = true;
    _manuallyClosed = false;
    if (_channel == null) _open();
  }

  void _setOnline(bool v) {
    if (_online == v) return;
    _online = v;
    for (final cb in _statusCbs.toList()) {
      try {
        cb(v);
      } catch (_) {
        // ignore listener failures
      }
    }
  }

  void _clearRetry() {
    _retryTimer?.cancel();
    _retryTimer = null;
  }

  void _scheduleRetry() {
    if (!_wantOpen || _manuallyClosed || _retryTimer != null) return;
    final idx = _failures.clamp(0, backoffMs.length - 1);
    _retryTimer = Timer(Duration(milliseconds: backoffMs[idx]), () {
      _retryTimer = null;
      _open();
    });
  }

  void _open() {
    _clearRetry();
    WebSocketChannel ch;
    try {
      ch = channelFactory != null
          ? channelFactory!(Uri.parse(url))
          : WebSocketChannel.connect(Uri.parse(url));
    } catch (_) {
      _failures++;
      _scheduleRetry();
      return;
    }
    _channel = ch;
    _helloIssued = 0;
    _helloReceived = 0;
    _sub = ch.stream.listen(
      (data) => _onData(ch, data),
      onError: (_) => _down(ch),
      onDone: () => _down(ch),
      cancelOnError: false,
    );
    // The socket is not yet proven open; queue the hello with the same
    // rule as any other handshake intent and flush below on first traffic.
    // `WebSocketChannel` has no open callback, so treat construction as
    // open (failures surface via onError/onDone) and flush immediately.
    _failures = 0;
    _setOnline(true);
    for (final m in _queue.toList()) {
      _queue.remove(m);
      _sendFrame(m);
    }
    try {
      _sendFrame(
          '{"cmd":"hello","binary_meters":$binaryMeters,"taps":$wantTaps}');
    } catch (_) {
      // queued implicitly on the next failure
    }
  }

  void _onData(WebSocketChannel ch, Object? data) {
    if (_channel != ch || _manuallyClosed) return;
    if (data is String) {
      try {
        if ((jsonDecode(data) as Map)['type'] == 'hello') {
          _helloReceived++;
        }
      } catch (_) {
        // malformed frames are ignored by the world
      }
      _emit(data);
    } else if (data is Uint8List) {
      _emit(data);
    } else if (data is List<int>) {
      _emit(Uint8List.fromList(data));
    }
  }

  void _down(WebSocketChannel ch) {
    if (_channel != ch) return;
    _sub?.cancel();
    _sub = null;
    _channel = null;
    _queue.clear();
    _setOnline(false);
    _failures++;
    _scheduleRetry();
  }

  void _emit(TransportMessage msg) {
    for (final cb in _msgCbs.toList()) {
      try {
        cb(msg);
      } catch (_) {
        // ignore listener failures
      }
    }
  }

  @override
  void send(String json) {
    if (_manuallyClosed) return;
    if (_channel != null) {
      try {
        _sendFrame(json);
        return;
      } catch (_) {
        final ch = _channel;
        _channel = null;
        _setOnline(false);
        if (ch != null) _down(ch);
      }
    }
    if (queueable(json)) _queue.add(json);
    if (_wantOpen && !_manuallyClosed && _channel == null) _open();
  }

  void _sendFrame(String json) {
    _channel?.sink.add(json);
    try {
      if ((jsonDecode(json) as Map)['cmd'] == 'hello') _helloIssued++;
    } catch (_) {
      // protocol handles invalid commands
    }
  }

  /// The Nth unicast hello acknowledgement proves every earlier command on
  /// THIS channel finished (the serve inbound loop awaits each command).
  @override
  Future<void>? barrier() {
    final ch = _channel;
    if (ch == null || !_online) {
      return Future.error(
          StateError('Relay socket is unavailable; cleanup is unconfirmed.'));
    }
    final target = _helloIssued + 1;
    final done = Completer<void>();
    late final Unsub offMessage;
    late final Unsub offStatus;
    Timer? timer;
    void finish([Object? error]) {
      timer?.cancel();
      offMessage();
      offStatus();
      if (error == null) {
        if (!done.isCompleted) done.complete();
      } else {
        if (!done.isCompleted) done.completeError(error);
      }
    }

    timer = Timer(const Duration(seconds: 30), () {
      finish(StateError(
          'Relay has not acknowledged queued cleanup. The pending request may still finish.'));
    });
    offMessage = onMessage((_) {
      if (_channel == ch && _helloReceived >= target) finish();
    });
    offStatus = onStatus((up) {
      if (!up || _channel != ch) {
        finish(StateError(
            'Relay connection changed before cleanup was confirmed.'));
      }
    });
    try {
      _sendFrame('{"cmd":"hello"}');
    } catch (e) {
      finish(e);
    }
    return done.future;
  }

  @override
  Unsub onMessage(void Function(TransportMessage msg) cb) {
    _msgCbs.add(cb);
    return () => _msgCbs.remove(cb);
  }

  @override
  Unsub onStatus(void Function(bool online) cb) {
    _statusCbs.add(cb);
    try {
      cb(_online);
    } catch (_) {
      // ignore listener failures
    }
    return () => _statusCbs.remove(cb);
  }

  @override
  Future<void> close() async {
    _manuallyClosed = true;
    _wantOpen = false;
    _clearRetry();
    _queue.clear();
    try {
      await _sub?.cancel();
    } catch (_) {
      // ignore
    }
    _sub = null;
    try {
      await _channel?.sink.close();
    } catch (_) {
      // ignore
    }
    _channel = null;
    _setOnline(false);
  }
}
