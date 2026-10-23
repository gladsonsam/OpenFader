// Offline sim fallback: answers the same Cmd/Event JSON the real engine
// speaks. Same protocol surface as before, so the demo always boots when
// the native library is missing (desktop `flutter run` without jniLibs,
// or any FFI init failure).
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:math' as math;
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
  final Queue<String> _mailbox = Queue<String>();
  final Stopwatch _signalClock = Stopwatch()..start();
  int _lastMeterTick = -1;
  int _binSeq = 0;

  @override
  Future<String> boot() async {
    _mailbox.add(jsonEncode({'type': 'you', 'id': 'client-1'}));
    _snapshot();
    return 'client-1';
  }

  void _snapshot() {
    _mailbox.add(jsonEncode({
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
    }));
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
    _mailbox.add(
      jsonEncode({'type': 'param', 'ch': ch, 'field': field, 'value': value}),
    );
    _snapshot();
  }

  // Latest-only 20 Hz source, sampled by the shell's existing poll loop.
  // Both encodings drain this same source: extra control polls must neither
  // accelerate the signal nor replay it through JSON after binary delivery.
  List<double>? _takeMeterLevels() {
    final tick = _signalClock.elapsedMilliseconds ~/ 50;
    if (tick == _lastMeterTick) return null;
    _lastMeterTick = tick;
    final t = tick * 0.025;
    return [
      for (var i = 0; i < _strips.length; i++)
        _strips[i].muted ? 0.0 : _inputLevel(i, t) * _strips[i].fader
    ];
  }

  double _inputLevel(int i, double t) {
    // Same slow, staggered -36..-12 dB phrases as the native sim.
    final phase = i * 0.7;
    final rate = 0.65 + (i % 5) * 0.07;
    final phrase = (0.5 +
            0.35 * math.sin(t * rate + phase) +
            0.15 * math.sin(t * 1.7 + phase * 1.3))
        .clamp(0.0, 1.0);
    return math.pow(10.0, (-36.0 + 24.0 * phrase) / 20.0).toDouble();
  }

  int _drainCap(int cap) => cap == 0 ? 512 : cap.clamp(1, 4096);

  @override
  Future<List<String>> poll(String clientId, int cap) async {
    final limit = _drainCap(cap);
    final out = <String>[];
    // Keep undelivered controls queued when a caller requests a small cap.
    while (_mailbox.isNotEmpty && out.length < limit) {
      out.add(_mailbox.removeFirst());
    }
    if (out.length < limit) {
      final levels = _takeMeterLevels();
      if (levels != null) {
        out.add(jsonEncode({'type': 'meters', 'levels': levels}));
      }
    }
    return out;
  }

  @override
  Future<Uint8List> pollBinary(String clientId, int cap) async {
    // Same synthetic 20 Hz meter frame as poll(), encoded in the compact
    // `Event::to_binary` layout (protocol.rs: `[u8 type=1, u8 version=1,
    // u16 n, u32 seq, u32 t_ms]` + `n x u16` levels, little-endian),
    // wrapped in the `pollBinary` length prefix (`u32 LE len` + frame).
    final levels = _takeMeterLevels();
    if (levels == null) return Uint8List(0);
    final n = levels.length;
    final frame = ByteData(binHeaderLen + n * 2);
    frame.setUint8(0, binTypeMeters);
    frame.setUint8(1, binVersion);
    frame.setUint16(2, n, Endian.little);
    frame.setUint32(4, _binSeq, Endian.little);
    _binSeq = (_binSeq + 1) & 0xffffffff;
    frame.setUint32(8, (_lastMeterTick * 50) & 0xffffffff, Endian.little);
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
