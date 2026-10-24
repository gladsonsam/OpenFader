// Offline sim fallback answering the same Cmd and Event JSON as the real engine.
library;

import 'dart:collection';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import '../engine.dart';
import '../core/channel_views.dart' show canonicalSimDspRange;

class FakeSimEngine implements EngineApi {
  final List<Strip> _strips = List.generate(
      8,
      (i) => Strip(
          ch: i,
          key: 'ch/${(i + 1).toString().padLeft(2, '0')}',
          name: 'CH${(i + 1).toString().padLeft(2, '0')}',
          fader: 0.75,
          muted: false));
  // Every advertised leaf carries an engineering-unit value and survives resync.
  static const _defaults = <String, Object>{
    'fader': 0.75,
    'mute': false,
    'gain': 0.0,
    'trim': 0.0,
    'phantom': false,
    'polarity': false,
    'pan': 0.0,
    'lr_assign': true,
    'usbreturn': 0,
    'hpf_on': false,
    'hpf_freq': 80.0,
    'eq_on': false,
    'gate_on': false,
    'gate_thr': -50.0,
    'gate_range': 60.0,
    'gate_att': 1.0,
    'gate_hold': 50.0,
    'gate_rel': 100.0,
    'comp_on': false,
    'comp_thr': -20.0,
    'comp_ratio': 2.0,
    'comp_knee': 3.0,
    'comp_makeup': 0.0,
    'comp_att': 10.0,
    'comp_hold': 20.0,
    'comp_rel': 100.0,
  };
  final List<Map<String, Object>> _params = List.generate(
      8,
      (ch) => {
            ..._defaults,
            'name': 'CH${(ch + 1).toString().padLeft(2, '0')}',
            'source': 'Local ${ch + 1}',
            for (var band = 1; band <= 4; band++) ...{
              'eq${band}_freq': [100.0, 2000.0, 5000.0, 10000.0][band - 1],
              'eq${band}_gain': 0.0,
              'eq${band}_q': 1.0,
              'eq${band}_type': [1, 2, 2, 4][band - 1],
            },
            for (var slot = 1; slot <= 10; slot++) ...{
              'send${slot}_level': -90.0,
              'send${slot}_pan': 0.0,
              'send${slot}_tap': 4,
              'send${slot}_on': true,
            },
          });
  // Subgroup masters mirror the native sim initial values.
  final List<double> _subgroupFaders = List.filled(6, 0);
  final List<bool> _subgroupMuted = List.filled(6, false);
  final Queue<String> _mailbox = Queue<String>();
  final Map<int, String> _pendingSpectra = {};
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
      'type': 'caps',
      'driver': 'sim',
      'topology': [
        {
          'key': 'ch',
          'kind': 'in',
          'label': 'Inputs',
          'name_prefix': 'CH',
          'first': 0,
          'count': _strips.length,
        }
      ],
      'groups': {'subgroups': _subgroupFaders.length},
      'params': [
        for (final id in _params.first.keys)
          {
            'id': id,
            'scope': 'channel',
            'scopes': ['in'],
            'kind': _params.first[id] is bool
                ? 'bool'
                : _params.first[id] is String
                    ? 'text'
                    : id.endsWith('_type') ||
                            id.endsWith('_tap') ||
                            id == 'usbreturn'
                        ? 'enum'
                        : 'float',
            if (RegExp(r'^eq[1-4]_type$').hasMatch(id)) ...{
              'min': 0,
              'max': 5,
              'unit': '',
            },
            if (canonicalSimDspRange(id) case final range?) ...{
              'min': range.first,
              'max': range.last,
              'kind': 'float',
              'unit': id.endsWith('_freq')
                  ? 'Hz'
                  : RegExp(r'_(att|hold|rel)$').hasMatch(id)
                      ? 'ms'
                      : id == 'gain' ||
                              id == 'trim' ||
                              RegExp(r'_(thr|range|makeup|gain)$').hasMatch(id)
                          ? 'dB'
                          : '',
            },
          }
      ],
    }));
    for (var group = 0; group < _subgroupFaders.length; group++) {
      _echoSubgroup(group);
    }
    for (var ch = 0; ch < _params.length; ch++) {
      _mailbox.add(jsonEncode({
        'type': 'params',
        'ch': ch,
        'values': _params[ch],
        'authority': {for (final field in _params[ch].keys) field: 'verified'},
      }));
    }
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
    final ch = (c['ch'] as num?)?.toInt();
    if (ch != null && (ch < 0 || ch >= _strips.length)) return;
    switch (c['cmd']) {
      case 'refresh_devices':
      case 'discover_devices':
        _mailbox
            .add(jsonEncode({'type': 'devices', 'list': [], 'scanned_at': 0}));
        _mailbox.add(jsonEncode({
          'type': 'device_operation_complete',
          'request_id': c['request_id'],
          'operation': c['cmd'] == 'refresh_devices' ? 'refresh' : 'discover',
          'results': [
            if (c['cmd'] == 'refresh_devices')
              for (final addr in (c['hosts'] as List? ?? const []))
                {'addr': addr, 'status': 'unsupported'},
          ],
          'error': 'Hardware discovery is unavailable in the offline fallback',
        }));
        return;
      case 'param':
        if (ch == null) return;
        final field = c['field'];
        final value = c['value'];
        if (field is! String || !_params[ch].containsKey(field)) return;
        if (field == 'name' &&
            (value is! String || value.isEmpty || value.runes.length > 16)) {
          _mailbox.add(jsonEncode({
            'type': 'error',
            'ch': ch,
            'message': 'Channel names must contain 1–16 characters.',
          }));
          return;
        }
        final current = _params[ch][field];
        if ((current is num && value is num && value.isFinite) ||
            (current is bool && value is bool) ||
            (current is String && value is String)) {
          _params[ch][field] = value as Object;
          if (field == 'fader' && value is num) {
            _strips[ch].fader = value.toDouble();
          }
          if (field == 'name' && value is String) _strips[ch].name = value;
          if (field == 'mute' && value is bool) _strips[ch].muted = value;
          _mailbox.add(jsonEncode({
            'type': 'param',
            'ch': ch,
            'field': field,
            'value': value,
            'authority': 'verified'
          }));
        }
        return;
      case 'scan':
        _mailbox
            .add(jsonEncode({'type': 'devices', 'list': [], 'scanned_at': 0}));
        return;
      case 'probe':
      case 'connect_device':
        _mailbox.add(jsonEncode({
          'type': 'error',
          'message':
              'Hardware connections are unavailable in the offline fallback'
        }));
        return;
      case 'connect':
        if (c['driver'] != null && c['driver'] != 'sim') {
          _mailbox.add(jsonEncode({
            'type': 'error',
            'message':
                'Hardware connections are unavailable in the offline fallback'
          }));
          return;
        }
        _snapshot();
        return;
      case 'subgroup_fader':
      case 'subgroup_mute':
        final group = c['group'];
        if (group is! int || group < 0 || group >= _subgroupFaders.length) {
          return;
        }
        if (c['cmd'] == 'subgroup_fader') {
          final value = c['value'];
          if (value is! num || !value.isFinite || value < 0 || value > 1) {
            return;
          }
          _subgroupFaders[group] = value.toDouble();
        } else {
          final muted = c['muted'];
          if (muted is! bool) return;
          _subgroupMuted[group] = muted;
        }
        _echoSubgroup(group);
        return;
      case 'fader':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].fader = ((c['value'] as num)).toDouble();
        _params[ch]['fader'] = _strips[ch].fader;
        _echo(ch, 'fader', _strips[ch].fader);
        return;
      case 'mute':
        final ch = (c['ch'] as num).toInt();
        _strips[ch].muted = c['muted'] as bool;
        _params[ch]['mute'] = _strips[ch].muted;
        _echo(ch, 'mute', _strips[ch].muted);
        return;
    }
  }

  void _echoSubgroup(int group) {
    _mailbox.add(jsonEncode({
      'type': 'subgroup',
      'group': group,
      'fader': _subgroupFaders[group],
      'muted': _subgroupMuted[group],
      'key': 'subgroup/${(group + 1).toString().padLeft(2, '0')}',
    }));
  }

  void _echo(int ch, String field, Object value) {
    _mailbox.add(
      jsonEncode({
        'type': 'param',
        'ch': ch,
        'field': field,
        'value': value,
        'authority': 'verified'
      }),
    );
    _snapshot();
  }

  // Latest-only 20 Hz source sampled by the existing shell poll loop.
  // Both encodings drain this same source so extra polls never replay it.
  List<double>? _takeMeterLevels() {
    final tick = _signalClock.elapsedMilliseconds ~/ 50;
    if (tick == _lastMeterTick) return null;
    _lastMeterTick = tick;
    final t = tick * 0.025;
    for (var ch = 0; ch < _strips.length; ch++) {
      final level =
          _strips[ch].muted ? 0.0 : _inputLevel(ch, t) * _strips[ch].fader;
      _pendingSpectra[ch] = jsonEncode({
        'type': 'spectrum',
        'ch': ch,
        'bins': _demoSpectrum(ch, level),
      });
    }
    return [
      for (var i = 0; i < _strips.length; i++)
        _strips[i].muted ? 0.0 : _inputLevel(i, t) * _strips[i].fader
    ];
  }

  double _inputLevel(int i, double t) {
    final phase = i * 0.7;
    final rate = 0.65 + (i % 5) * 0.07;
    final phrase = (0.5 +
            0.35 * math.sin(t * rate + phase) +
            0.15 * math.sin(t * 1.7 + phase * 1.3))
        .clamp(0.0, 1.0);
    return math.pow(10.0, (-36.0 + 24.0 * phrase) / 20.0).toDouble();
  }

  // Kept in sync with SimDriver.demo_spectrum.
  List<double> _demoSpectrum(int ch, double level) {
    if (level <= 0) return List.filled(64, 0);
    final envelope =
        ((20 * math.log(level) / math.ln10 + 90) / 70).clamp(0.0, 1.0);
    return List.generate(64, (bin) {
      final x = bin.toDouble();
      double peak(double center, double width) =>
          math.exp(-.5 * math.pow((x - center) / width, 2));
      final profile = .012 +
          .92 * peak(12 + (ch % 3).toDouble(), 3) +
          .55 * peak(23 + (ch % 4).toDouble(), 3.8) +
          .20 * peak(34, 5);
      final texture = .85 + .15 * math.cos(x * 1.9 + ch * .7);
      return (profile * texture * envelope).clamp(0.0, 1.0);
    });
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
    while (_pendingSpectra.isNotEmpty && out.length < limit) {
      out.add(_pendingSpectra.remove(_pendingSpectra.keys.first)!);
    }
    return out;
  }

  @override
  Future<Uint8List> pollBinary(String clientId, int cap) async {
    // Same synthetic meter frame as poll, in the compact binary layout.
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
