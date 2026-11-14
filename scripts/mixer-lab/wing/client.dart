import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'tuple.dart';

class WingProbe {
  WingProbe(this.uri, this.socket) {
    subscription = socket.listen((raw) {
      final Map<String, dynamic> e;
      if (raw is String) {
        e = jsonDecode(raw) as Map<String, dynamic>;
      } else if (raw is List<int>) {
        // hello(taps:true) negotiates binary taps independently of meters.
        final bytes = Uint8List.fromList(raw);
        require(
          bytes.length >= 13 && bytes[0] == 3 && bytes[1] == 1,
          'Unexpected binary frame',
        );
        final view = ByteData.sublistView(bytes);
        final count = view.getUint16(2, Endian.little);
        require(bytes.length == 13 + count * 2, 'Malformed binary tap frame');
        const names = ['in', 'post', 'gr_gate', 'gr_comp'];
        final id = bytes[12];
        e = {
          'type': 'taps',
          'tap': id < names.length ? names[id] : 'unknown-$id',
          'levels': [
            for (var i = 0; i < count; i++)
              view.getUint16(13 + i * 2, Endian.little) /
                  65535 *
                  (id >= 2 ? 24 : 1),
          ],
        };
      } else {
        throw StateError('Unexpected WebSocket frame');
      }
      sequence++;
      latest[e['type'] as String] = e;
      if (e['type'] == 'caps' && e['driver'] == 'wing') caps = e;
      if (e['type'] == 'hello') helloSequence = sequence;
      if (e['type'] == 'error') errors.add(e);
      if (e['type'] == 'sync_epoch') {
        values.clear();
        observed.clear();
        tapValues.clear();
        tapObserved.clear();
        meterValues = null;
      }
      if (e['type'] == 'meters') {
        meterValues = e['levels'] as List;
        meterSequence = sequence;
      }
      if (e['type'] == 'taps') {
        final tap = e['tap'] as String;
        final levels = e['levels'] as List;
        if (!['in', 'post'].contains(tap)) unexpectedTaps.add(tap);
        tapValues[tap] = levels;
        tapObserved[tap] = sequence;
        if (levels.isNotEmpty) tapCounts[tap] = (tapCounts[tap] ?? 0) + 1;
      }
      if (e['type'] == 'param_invalidated') {
        for (final field in (e['fields'] as List).whereType<String>()) {
          values.remove('${e['ch']}/$field');
          observed.remove('${e['ch']}/$field');
        }
      }
      if (e['type'] == 'params') {
        for (final entry in (e['values'] as Map).entries) {
          final key = '${e['ch']}/${entry.key}';
          values[key] = entry.value;
          observed[key] = sequence;
        }
      }
      if (e['type'] == 'param') {
        final key = '${e['ch']}/${e['field']}';
        values[key] = e['value'];
        observed[key] = sequence;
      }
    });
  }
  final Uri uri;
  final WebSocket socket;
  late final StreamSubscription<dynamic> subscription;
  final native = NativeOsc(port: 2223, wingReplies: true);
  final results = <Map<String, Object?>>[];
  final originals = <String, Object>{};
  final integerDomains = <String, (int, int)>{};
  final invalidColorOriginals = <String, Map<String, Object>>{};
  final colorDomains = <String, (int, int)>{};
  final errors = <Map<String, dynamic>>[];
  final latest = <String, Map<String, dynamic>>{};
  final values = <String, Object?>{}, observed = <String, int>{};
  final tapValues = <String, List>{},
      tapObserved = <String, int>{},
      tapCounts = <String, int>{};
  final unexpectedTaps = <String>[];
  List? meterValues;
  int meterSequence = 0;
  Map<String, dynamic>? caps;
  int sequence = 0, helloSequence = 0;
  List<Map> get families => (caps!['topology'] as List).cast<Map>();
  Future<void> barrier() async {
    final start = helloSequence;
    socket.add(Cmds.hello(false, true));
    await until(() => helloSequence > start, 'WING command barrier');
  }

  Future<List<Map<String, dynamic>>> command(
    String value, {
    bool reject = false,
  }) async {
    final start = errors.length;
    socket.add(value);
    await barrier();
    final found = errors.sublist(start);
    require(
      reject ? found.isNotEmpty : found.isEmpty,
      reject ? 'Expected explicit rejection' : 'Command error: $found',
    );
    return found;
  }

  Future<void> check(
    String name,
    Future<void> Function() action, {
    String evidence = 'protocol_readback_binding_not_individually_audited',
  }) async {
    final watch = Stopwatch()..start();
    try {
      await action();
      results.add({
        'check': name,
        'status': 'pass',
        'sidecar_evidence': evidence,
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('PASS $name');
    } on Blocked catch (e) {
      results.add({'check': name, 'status': 'blocked', 'reason': e.reason});
      stdout.writeln('BLOCKED $name: $e');
    } catch (e) {
      results.add({'check': name, 'status': 'fail', 'reason': '$e'});
      stdout.writeln('FAIL $name: $e');
    }
  }

  Future<Object> remember(String path) async {
    if (originals.containsKey(path)) return originals[path]!;
    final value = await native.read(path);
    originals[path] = value;
    return value;
  }

  Future<int> readInteger(String path, int min, int max) async {
    final tuple = await readWingTuple(native, path);
    final position = tuple['normalized'] as double;
    final third = tuple['third'];
    require(
      third is int && position.isFinite && position >= 0 && position <= 1,
      'Invalid integer tuple $tuple',
    );
    final actual = min + (position * (max - min)).round();
    require(
      third == actual || third == actual - min,
      'Integer tuple conflicts with domain $min..$max: $tuple',
    );
    results.add({
      'check': 'Native integer tuple $path',
      'status': 'observation',
      'domain_min': min,
      'domain_max': max,
      'tuple': tuple,
      'derived_value': actual,
    });
    return actual;
  }

  /// Retain the live native domain and original value before fixture staging.
  /// Invalid boot values cannot be restored by an OSC write and require disposal.
  Future<int> captureColor(String path) async {
    final tuple = await readWingTuple(native, path);
    final value = int.tryParse(tuple['display'] as String);
    require(value != null, 'Unparseable colour tuple $tuple');
    if (!colorDomains.containsKey(path)) {
      final split = path.lastIndexOf('/');
      final definition = await readWingIntegerDefinition(
        native,
        path.substring(0, split),
        path.substring(split + 1),
      );
      final min = definition['min'] as int, max = definition['max'] as int;
      require(
        min == 1 && max >= 12,
        'Unsupported live colour domain: $definition',
      );
      colorDomains[path] = (min, max);
      results.add({
        'check': 'Independent colour native definition $path',
        'status': 'observation',
        'definition': definition,
      });
    }
    final (min, max) = colorDomains[path]!;
    if (!originals.containsKey(path) &&
        !invalidColorOriginals.containsKey(path)) {
      results.add({
        'check': 'Initial native colour tuple $path',
        'status': 'observation',
        'tuple': tuple,
        'domain_min': min,
        'domain_max': max,
      });
      if (value! < min || value > max) {
        invalidColorOriginals[path] = tuple;
      } else {
        originals[path] = value;
        integerDomains[path] = (min, max);
      }
    }
    return value!;
  }

  Future<void> initializeColor(String path) async {
    final (min, max) = colorDomains[path]!;
    await native.write(path, min);
    await agreesInteger(path, min, min, max);
  }

  Future<void> agreesColor(String path, int expected) async {
    final (min, max) = colorDomains[path]!;
    await agreesInteger(path, expected, min, max);
  }

  Future<int> rememberInteger(String path, int min, int max) async {
    if (originals[path] case final int existing) return existing;
    final value = await readInteger(path, min, max);
    originals[path] = value;
    integerDomains[path] = (min, max);
    return value;
  }

  Future<void> agreesInteger(
    String path,
    int expected,
    int min,
    int max,
  ) async {
    final actual = await readInteger(path, min, max);
    require(
      actual == expected,
      '$path expected $expected; tuple indicated $actual',
    );
  }

  Future<void> change(String path, Object expected, String cmd) async {
    await remember(path);
    await command(cmd);
    await native.agrees(path, expected);
  }

  Future<void> agreesEventually(String path, Object expected) async {
    final deadline = DateTime.now().add(const Duration(seconds: 5));
    Object? actual;
    do {
      actual = await native.read(path);
      if (same(actual, expected)) return;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    } while (DateTime.now().isBefore(deadline));
    throw StateError('$path expected $expected, independently read $actual');
  }

  Future<void> attach() async {
    socket.add(Cmds.hello(false, true));
    socket.add(Cmds.connect('wing', '127.0.0.1'));
    await until(
      () =>
          caps != null &&
          latest['info']?['driver'] == 'wing' &&
          helloSequence > 0,
      'WING attachment',
    );
    await barrier();
  }

  Future<void> finish(String path, String? scenario) async {
    final failures = <String>[];
    for (final e in originals.entries) {
      try {
        await native.write(e.key, e.value);
        if (integerDomains[e.key] case final domain?) {
          await agreesInteger(e.key, e.value as int, domain.$1, domain.$2);
        } else {
          await native.agrees(e.key, e.value);
        }
      } catch (error) {
        failures.add('${e.key}: $error');
      }
    }
    results.add({
      'check': 'Restore captured valid-domain fixture values',
      'status': failures.isEmpty ? 'pass' : 'fail',
      'failures': failures,
      'leaf_count': originals.length,
    });
    if (invalidColorOriginals.isNotEmpty) {
      results.add({
        'check': 'Restore original out-of-domain colour values',
        'status': 'blocked',
        'requires_fixture_reset': true,
        'reason': 'The disposable Sidecar fixture rejects OSC colour 0 by clamping to its native minimum 1; these original invalid values cannot be restored through OSC. Dispose the owned fixture.',
        'originals': invalidColorOriginals,
      });
    }
    await subscription.cancel();
    await socket.close();
    await File(path).writeAsString(
      '${const JsonEncoder.withIndent('  ').convert({
        'fixture': 'wing',
        'scenario': scenario ?? 'full',
        'checks': results,
        'caps': caps,
        'engine_errors': errors,
        'limits': ['Sidecar protocol evidence only; no physical DSP, audio, USB, preamp or iOS proof', 'Stored/shadow values only prove OSC behavior, not an emulated signal path', 'Native show and output capabilities require separate verification'],
      })}\n',
    );
    exitCode = results.any((r) => r['status'] == 'fail')
        ? 1
        : results.any((r) => r['status'] == 'blocked')
        ? 2
        : 0;
  }
}
