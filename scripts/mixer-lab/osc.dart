import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

void require(bool condition, String reason) {
  if (!condition) throw StateError(reason);
}

Future<void> until(bool Function() ready, String what) async {
  final end = DateTime.now().add(const Duration(seconds: 15));
  while (!ready()) {
    if (DateTime.now().isAfter(end)) throw TimeoutException(what);
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}

List<int> oscString(String value) {
  final bytes = [...utf8.encode(value), 0];
  while (bytes.length % 4 != 0) bytes.add(0);
  return bytes;
}

bool same(Object? actual, Object? expected) => actual is num && expected is num
    ? (actual - expected).abs() < 0.000025
    : actual == expected;

class Blocked implements Exception {
  final String reason;
  Blocked(this.reason);
  @override
  String toString() => reason;
}

/// Independent wire client; it does not consume OpenFader's parameter cache.
class NativeOsc {
  final InternetAddress address;
  final int port;
  final bool wingReplies;

  NativeOsc({
    InternetAddress? address,
    this.port = 10023,
    this.wingReplies = false,
  }) : address = address ?? InternetAddress.loopbackIPv4;

  List<int> packet(String path, [Object? value]) {
    require(
      value == null || value is int || value is double || value is String,
      'Unsupported OSC value',
    );
    final word = ByteData(4);
    if (value is int) word.setInt32(0, value);
    if (value is double) word.setFloat32(0, value);
    return [
      ...oscString(path),
      ...oscString(
        value == null
            ? ','
            : value is String
            ? ',s'
            : value is int
            ? ',i'
            : ',f',
      ),
      if (value is String)
        ...oscString(value)
      else if (value != null)
        ...word.buffer.asUint8List(),
    ];
  }

  Future<void> write(String path, Object value) async {
    final socket = await RawDatagramSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
    );
    try {
      final bytes = packet(path, value);
      require(
        socket.send(bytes, address, port) == bytes.length,
        'OSC send failed',
      );
    } finally {
      socket.close();
    }
  }

  Future<Object> read(String path) async {
    final socket = await RawDatagramSocket.bind(
      InternetAddress.loopbackIPv4,
      0,
    );
    try {
      final bytes = packet(path);
      require(
        socket.send(bytes, address, port) == bytes.length,
        'OSC query failed',
      );
      await for (final event in socket.timeout(const Duration(seconds: 2))) {
        if (event != RawSocketEvent.read) continue;
        final reply = socket.receive();
        if (reply == null ||
            reply.address.address != address.address ||
            reply.port != port)
          continue;
        final data = reply.data;
        final end = data.indexOf(0);
        if (end < 0 || utf8.decode(data.sublist(0, end)) != path) continue;
        final tagsAt = ((end + 4) ~/ 4) * 4;
        final tagsEnd = data.indexOf(0, tagsAt);
        require(tagsEnd >= 0, 'Malformed OSC tags');
        var tags = utf8.decode(data.sublist(tagsAt, tagsEnd));
        var at = ((tagsEnd + 4) ~/ 4) * 4;
        if (wingReplies && (tags == ',sff' || tags == ',sfi')) {
          // WING replies carry display text, normalized position, then native value.
          final textEnd = data.indexOf(0, at);
          require(textEnd >= 0, 'Malformed WING display text');
          at = ((textEnd + 4) ~/ 4) * 4 + 4;
          tags = tags == ',sfi' ? ',i' : ',f';
        }
        if (tags == ',s') {
          final textEnd = data.indexOf(0, at);
          require(textEnd >= 0, 'Malformed OSC string');
          return utf8.decode(data.sublist(at, textEnd));
        }
        require(data.length == at + 4, 'Expected one OSC value at $path');
        final word = ByteData.sublistView(data, at);
        if (tags == ',i') return word.getInt32(0);
        if (tags == ',f') {
          final value = word.getFloat32(0);
          require(value.isFinite, 'Non-finite OSC value at $path');
          return value;
        }
        throw StateError('Unsupported OSC reply $path $tags');
      }
      throw StateError('OSC socket closed');
    } finally {
      socket.close();
    }
  }

  Future<void> agrees(String path, Object expected) async {
    final actual = await read(path);
    require(
      same(actual, expected),
      '$path expected $expected, independently read $actual',
    );
  }
}

/// Numeric probes also allow the desk's asynchronous apply window.
class OscReference extends NativeOsc {
  OscReference(InternetAddress address, {super.port, super.wingReplies})
    : super(address: address);

  @override
  Future<num> read(String path) async {
    final value = await super.read(path);
    require(value is num, 'Expected numeric OSC reply at $path');
    return value as num;
  }

  @override
  Future<void> agrees(String path, Object expected) async {
    require(expected is num, 'Expected numeric OSC value');
    final end = DateTime.now().add(const Duration(seconds: 5));
    do {
      final actual = await read(path);
      if ((actual - (expected as num)).abs() <= 0.00002) return;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    } while (DateTime.now().isBefore(end));
    throw StateError('$path did not independently read back $expected');
  }
}
