// Offline sim fallback: answers the same Cmd/Event JSON the real engine
// speaks. Same protocol surface as before, so the demo always boots when
// the native library is missing (desktop `flutter run` without jniLibs,
// or any FFI init failure).
library;

import 'dart:convert';
import 'dart:typed_data';

import '../engine.dart';

class FakeSimEngine implements EngineApi {
  final List<Strip> _strips = List.generate(
      8,
      (i) => Strip(
          ch: i,
          key: 'ch/${(i + 1).toString().padLeft(2, '0')}',
          name: 'CH${(i + 1).toString().padLeft(2, '0')}',
          fader: 0.75,
          muted: false));
  final List<List<String>> _mailbox = [];
  int _tick = 0;
  int _binTick = 0;
  int _binSeq = 0;

  @override
  Future<String> boot() async {
    _mailbox.add([
      jsonEncode({'type': 'you', 'id': 'client-1'})
    ]);
    _snapshot();
    return 'client-1';
  }

  void _snapshot() {
    _mailbox.add([
      jsonEncode({
        'type': 'state',
        'driver': 'sim',
        'strips': [
          for (final s in _strips)
            {
              'ch': s.ch,
              'key': s.key,
              'name': s.name,
              'fader': s.fader,
              'muted': s.muted
            }
        ],
      })
    ]);
  }

  @override
  Future<void> cmd(String clientId, String json) async {
    final c = jsonDecode(json) as Map<String, dynamic>;
    switch (c['cmd']) {
      case 'connect':
      case 'scan':
      case 'probe':
        _snapshot();
        return;
      case 'fader':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].fader = ((c['value'] as num)).toDouble();
        _echo(ch, 'fader', _strips[ch].fader);
        return;
      case 'mute':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].muted = c['muted'] as bool;
        _echo(ch, 'mute', _strips[ch].muted);
        return;
    }
  }

  void _echo(int ch, String field, Object value) {
    _mailbox.add([
      jsonEncode({'type': 'param', 'ch': ch, 'field': field, 'value': value})
    ]);
    _snapshot();
  }

  List<double> _meterLevels(int tick) => [
        for (var i = 0; i < _strips.length; i++)
          _strips[i].muted
              ? 0.0
              : (_strips[i].fader *
                      (0.55 + 0.35 * ((tick * (i + 3)) % 10) / 10)
                          .clamp(0.0, 1.0))
      ];

  @override
  Future<List<String>> poll(String clientId, int cap) async {
    // One fake 20 Hz meter frame per poll, then the queued control frames.
    _tick++;
    final out = <String>[
      jsonEncode({'type': 'meters', 'levels': _meterLevels(_tick)})
    ];
    for (final batch in _mailbox) {
      out.addAll(batch);
      if (out.length >= cap) break;
    }
    _mailbox.clear();
    return out.take(cap).toList();
  }

  @override
  Future<Uint8List> pollBinary(String clientId, int cap) async {
    // Same synthetic 20 Hz meter frame as poll(), encoded in the compact
    // `Event::to_binary` layout (protocol.rs: `[u8 type=1, u8 version=1,
    // u16 n, u32 seq, u32 t_ms]` + `n x u16` levels, little-endian),
    // wrapped in the `pollBinary` length prefix (`u32 LE len` + frame).
    // Independent tick/seq counters so the binary and JSON paths never
    // disturb each other; the app applies binary first, JSON after.
    _binTick++;
    final levels = _meterLevels(_binTick);
    final n = levels.length;
    final frame = ByteData(binHeaderLen + n * 2);
    frame.setUint8(0, binTypeMeters);
    frame.setUint8(1, binVersion);
    frame.setUint16(2, n, Endian.little);
    frame.setUint32(4, _binSeq++, Endian.little);
    frame.setUint32(8,
        DateTime.now().millisecondsSinceEpoch & 0xffffffff, Endian.little);
    for (var i = 0; i < n; i++) {
      frame.setUint16(binHeaderLen + i * 2,
          (levels[i].clamp(0.0, 1.0) * 65535).round(), Endian.little);
    }
    final frameBytes = frame.buffer.asUint8List();
    final out = Uint8List(4 + frameBytes.length);
    ByteData.sublistView(out).setUint32(0, frameBytes.length, Endian.little);
    out.setRange(4, out.length, frameBytes);
    return out;
  }

  @override
  Future<void> resync(String clientId) async => _snapshot();
}
