import 'dart:io';
import 'dart:typed_data';
import 'dart:convert';

import '../osc.dart';

/// Raw WING query evidence. This deliberately preserves all three OSC values;
/// it does not infer whether an integer is an absolute value or step index.
Future<Map<String, Object>> readWingTuple(NativeOsc native, String path) async {
  final socket = await RawDatagramSocket.bind(InternetAddress.loopbackIPv4, 0);
  try {
    final bytes = native.packet(path);
    require(
      socket.send(bytes, native.address, native.port) == bytes.length,
      'OSC query failed',
    );
    await for (final event in socket.timeout(const Duration(seconds: 2))) {
      if (event != RawSocketEvent.read) continue;
      final datagram = socket.receive();
      if (datagram == null ||
          datagram.address != native.address ||
          datagram.port != native.port)
        continue;
      final data = datagram.data, end = data.indexOf(0);
      if (end < 0 || utf8.decode(data.sublist(0, end)) != path) continue;
      final tagAt = ((end + 4) ~/ 4) * 4, tagEnd = data.indexOf(0, tagAt);
      require(tagEnd >= 0, 'Malformed WING tuple tags');
      final tag = utf8.decode(data.sublist(tagAt, tagEnd));
      require(
        tag == ',sfi' || tag == ',sff',
        'Expected numeric WING tuple, got $tag',
      );
      final textAt = ((tagEnd + 4) ~/ 4) * 4, textEnd = data.indexOf(0, textAt);
      require(textEnd >= 0, 'Malformed WING display text');
      final valueAt = ((textEnd + 4) ~/ 4) * 4;
      require(data.length == valueAt + 8, 'Malformed WING tuple length');
      final values = ByteData.sublistView(data, valueAt);
      final normalized = values.getFloat32(0);
      final Object value = tag == ',sfi'
          ? values.getInt32(4)
          : values.getFloat32(4);
      return {
        'path': path,
        'tags': tag,
        'display': utf8.decode(data.sublist(textAt, textEnd)),
        'normalized': normalized,
        'third': value,
      };
    }
    throw StateError('WING tuple socket closed');
  } finally {
    socket.close();
  }
}

/// Query the parent's native DD and retain the original blob as evidence.
Future<Map<String, Object>> readWingParameterDefinition(
  NativeOsc native,
  String parent,
  String leaf,
) async {
  final socket = await RawDatagramSocket.bind(InternetAddress.loopbackIPv4, 0);
  try {
    final packet = <int>[
      ...oscString(parent),
      ...oscString(',b'),
      0,
      0,
      0,
      1,
      0xdd,
      0,
      0,
      0,
    ];
    require(
      socket.send(packet, native.address, native.port) == packet.length,
      'DD query failed',
    );
    await for (final event in socket.timeout(const Duration(seconds: 2))) {
      if (event != RawSocketEvent.read) continue;
      final reply = socket.receive();
      if (reply == null ||
          reply.address != native.address ||
          reply.port != native.port)
        continue;
      final data = reply.data, end = data.indexOf(0);
      if (end < 0 || utf8.decode(data.sublist(0, end)) != parent) continue;
      final tagAt = ((end + 4) ~/ 4) * 4, tagEnd = data.indexOf(0, tagAt);
      if (tagEnd < 0 || utf8.decode(data.sublist(tagAt, tagEnd)) != ',b')
        continue;
      final at = ((tagEnd + 4) ~/ 4) * 4;
      require(at + 4 <= data.length, 'Truncated DD blob length');
      final length = ByteData.sublistView(data, at, at + 4).getUint32(0);
      require(at + 4 + length <= data.length, 'Truncated DD blob');
      final blob = data.sublist(at + 4, at + 4 + length);
      final view = ByteData.sublistView(blob);
      var pos = 0;
      while (pos < blob.length) {
        final token = blob[pos++];
        if (token == 0xde) break;
        require(token == 0xdf && pos + 2 <= blob.length, 'Invalid DD record');
        var size = view.getUint16(pos);
        pos += 2;
        if (size == 0) {
          require(pos + 4 <= blob.length, 'Truncated DD size');
          size = view.getUint32(pos);
          pos += 4;
        }
        final end = pos + size;
        require(end <= blob.length && pos + 11 <= end, 'Truncated DD body');
        var i = pos + 10;
        final nameLen = blob[i++];
        require(i + nameLen < end, 'Truncated DD name');
        final name = utf8.decode(blob.sublist(i, i + nameLen));
        i += nameLen;
        final labelLen = blob[i++];
        require(i + labelLen + 2 <= end, 'Truncated DD label');
        i += labelLen;
        final flags = view.getUint16(i);
        i += 2;
        if (name == leaf) {
          final type = (flags >> 4) & 15;
          require(
            (type == 4 && i + 8 == end) ||
                ((type == 1 || type == 2) && i + 12 == end),
            'Unsupported numeric DD for $parent/$leaf',
          );
          return {
            'path': '$parent/$leaf',
            'hash': view.getInt32(pos + 4),
            'index': view.getUint16(pos + 8),
            'kind': type == 4
                ? 'integer'
                : type == 1
                ? 'linear'
                : 'log',
            'min': type == 4 ? view.getInt32(i) : view.getFloat32(i),
            'max': type == 4 ? view.getInt32(i + 4) : view.getFloat32(i + 4),
            if (type != 4) 'steps': view.getInt32(i + 8),
            'flags': flags,
            'raw_blob_base64': base64Encode(blob),
          };
        }
        pos = end;
      }
      throw StateError('DD did not contain $parent/$leaf');
    }
    throw StateError('DD socket closed');
  } finally {
    socket.close();
  }
}

Future<Map<String, Object>> readWingIntegerDefinition(
  NativeOsc native,
  String parent,
  String leaf,
) async {
  final definition = await readWingParameterDefinition(native, parent, leaf);
  require(definition['kind'] == 'integer', 'Expected integer DD: $definition');
  return definition;
}
