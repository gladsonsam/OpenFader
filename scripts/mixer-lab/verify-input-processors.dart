// Manual X32 input processor guard/readback probe on a disposable loopback lab.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../../app/lib/engine_protocol.dart';
import 'osc.dart';

class ProcessorProbe {
  ProcessorProbe(this.socket) {
    sub = socket.listen((raw) {
      if (raw is! String) return;
      final event = jsonDecode(raw);
      if (event is! Map<String, dynamic>) return;
      final type = event['type'];
      if (type == 'caps' && event['driver'] == 'x32') caps = event;
      if (type == 'info' && event['driver'] == 'x32') attached = true;
      if (type == 'sync_epoch' && event['epoch'] is int)
        epoch = event['epoch'] as int;
      if (type == 'input_processor_result' && event['request_id'] is String) {
        replies[event['request_id'] as String] = event;
      }
      if (type == 'locks' && event['holds'] is Map)
        locks = Map<String, dynamic>.from(event['holds'] as Map);
      if (type == 'error') errors.add(event);
    });
  }
  final WebSocket socket;
  late StreamSubscription<dynamic> sub;
  final native = NativeOsc();
  final replies = <String, Map<String, dynamic>>{};
  final checks = <Map<String, Object?>>[];
  final errors = <Map<String, dynamic>>[];
  final originals = <String, Object>{};
  Map<String, dynamic>? caps;
  Map<String, dynamic> locks = {};
  bool attached = false;
  int? epoch;
  int sequence = 0;

  Future<Map<String, dynamic>> request(
    String processor, {
    String? field,
    Object? value,
    Map? guard,
    int? overrideEpoch,
  }) async {
    final id = 'processor-lab-${++sequence}';
    socket.add(
      jsonEncode({
        'cmd': 'input_processor',
        'request_id': id,
        'epoch': overrideEpoch ?? epoch,
        'ch': 0,
        'key': 'ch/01',
        'processor': processor,
        if (field != null) 'field': field,
        if (field != null) 'value': value,
        if (guard != null) 'guard': guard,
      }),
    );
    final end = DateTime.now().add(const Duration(seconds: 180));
    while (!replies.containsKey(id)) {
      if (DateTime.now().isAfter(end))
        throw TimeoutException('Input processor result $id');
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    return replies.remove(id)!;
  }

  Future<Map> capture(String processor) async {
    final event = await request(processor);
    if (event['snapshot'] is! Map) {
      throw Blocked(
        'No complete $processor fixture readback: ${event['message']}',
      );
    }
    final snapshot = event['snapshot'] as Map;
    require(
      snapshot['provider'] == 'x32' &&
          snapshot['processor'] == processor &&
          snapshot['ch'] == 0 &&
          snapshot['rows'] is List &&
          snapshot['links'] is List &&
          snapshot['engine_context'] is Map,
      'Malformed guarded processor snapshot: $snapshot',
    );
    return snapshot;
  }

  Map row(Map snapshot, String id) {
    final matches = (snapshot['rows'] as List)
        .whereType<Map>()
        .where((r) => r['id'] == id)
        .toList();
    if (matches.isEmpty) throw Blocked('Fixture does not expose $id');
    require(matches.length == 1, 'Duplicate processor row $id');
    return matches.single;
  }

  Future<void> check(String name, Future<void> Function() task) async {
    try {
      await task();
      checks.add({'check': name, 'status': 'pass'});
      stdout.writeln('PASS $name');
    } on Blocked catch (error) {
      checks.add({'check': name, 'status': 'blocked', 'reason': error.reason});
      stdout.writeln('BLOCKED $name: $error');
    } catch (error) {
      checks.add({'check': name, 'status': 'fail', 'reason': '$error'});
      stdout.writeln('FAIL $name: $error');
    }
  }

  Future<Object> remember(String path) async {
    if (originals.containsKey(path)) return originals[path]!;
    try {
      final value = await native.read(path);
      originals[path] = value;
      return value;
    } catch (error) {
      throw Blocked('Independent native read unavailable for $path: $error');
    }
  }

  Future<void> reject(
    String processor,
    Map guard,
    String field,
    Object value,
    String nativePath,
    Object before, {
    int? overrideEpoch,
  }) async {
    final response = await request(
      processor,
      field: field,
      value: value,
      guard: guard,
      overrideEpoch: overrideEpoch,
    );
    require(
      response['snapshot'] == null && response['message'] is String,
      'Expected explicit guarded rejection: $response',
    );
    await native.agrees(nativePath, before);
  }
}

Future<void> main(List<String> args) async {
  final options = <String, String>{};
  for (var i = 0; i < args.length; i += 2) {
    require(i + 1 < args.length, 'Expected --option value');
    options[args[i]] = args[i + 1];
  }
  final uri = Uri.parse(options['--ws'] ?? '');
  require(
    uri.scheme == 'ws' && uri.host == '127.0.0.1',
    'Use isolated loopback lab',
  );
  final report = options['--report'];
  require(report != null, '--report required');
  final probe = ProcessorProbe(await WebSocket.connect(uri.toString()));
  try {
    probe.socket.add(Cmds.hello(false, true));
    probe.socket.add(Cmds.connect('x32', '127.0.0.1'));
    await until(
      () => probe.attached && probe.caps != null && probe.epoch != null,
      'X32 processor fixture attachment',
    );
    await probe.check(
      'Provider advertises complete guarded processing',
      () async {
        require(
          probe.caps!['input_processor_guard'] == true &&
              probe.caps!['channel_processing_guard'] == true,
          'Guard capability not advertised',
        );
      },
    );
    for (final processor in [
      'eq',
      'gate',
      'compressor',
      'channel_processing',
    ]) {
      await probe.check(
        '$processor complete typed capture and unlinked witness',
        () async {
          final snapshot = await probe.capture(processor);
          final rows = (snapshot['rows'] as List).whereType<Map>().toList();
          require(
            rows.isNotEmpty &&
                snapshot['domain'] is List &&
                (snapshot['links'] as List).length == 1,
            'Incomplete capture: $snapshot',
          );
          final ids = <String>{};
          for (final r in rows) {
            require(
              r['id'] is String &&
                  ids.add(r['id'] as String) &&
                  r.containsKey('value') &&
                  r['revision'] is int &&
                  r['native'] is String &&
                  r['encoding'] is String,
              'Missing or duplicate readback/guard row $r',
            );
          }
          require(
            (snapshot['links'] as List).single['value'] == false,
            'Stereo witness not verified unlinked',
          );
          final descriptions =
              ((snapshot['engine_context'] as Map)['params'] as List)
                  .whereType<Map>();
          for (final descriptor in descriptions) {
            require(
              descriptor['id'] is String &&
                  descriptor['kind'] is String &&
                  descriptor['scopes'] is List,
              'Invalid typed descriptor $descriptor',
            );
            if (descriptor['kind'] == 'float' || descriptor['kind'] == 'enum') {
              require(
                descriptor['min'] is num &&
                    descriptor['max'] is num &&
                    (descriptor['min'] as num) <= (descriptor['max'] as num),
                'Invalid native range $descriptor',
              );
            }
          }
        },
      );
    }
    await probe.check(
      'Gate threshold guarded write, sibling, epoch and native readback',
      () async {
        const target = '/ch/01/gate/thr', sibling = '/ch/01/gate/hold';
        final original = await probe.remember(target);
        final siblingOriginal = await probe.remember(sibling);
        final guard = await probe.capture('gate');
        final threshold = probe.row(guard, 'gate_thr');
        require(threshold['native'] == target, 'Unexpected threshold mapping');
        // Protocol 4.09-02: normalized .75 encodes -20 dB.
        final desiredWire = same(original, 0.75) ? 0.5 : 0.75;
        final desiredDb = desiredWire == 0.75 ? -20.0 : -40.0;
        await probe.reject(
          'gate',
          guard,
          'gate_thr',
          desiredDb,
          target,
          original,
          overrideEpoch: probe.epoch! + 1,
        );
        await probe.reject('gate', guard, 'gate_thr', 'bad', target, original);
        final siblingWire = same(siblingOriginal, 0.0) ? 1.0 : 0.0;
        await probe.native.write(sibling, siblingWire);
        await probe.native.agrees(sibling, siblingWire);
        await probe.reject(
          'gate',
          guard,
          'gate_thr',
          desiredDb,
          target,
          original,
        );
        await probe.native.write(sibling, siblingOriginal);
        await probe.native.agrees(sibling, siblingOriginal);
        final fresh = await probe.capture('gate');
        final reply = await probe.request(
          'gate',
          field: 'gate_thr',
          value: desiredDb,
          guard: fresh,
        );
        require(
          reply['snapshot'] is Map && reply['message'] == null,
          'Write did not return verified complete snapshot: $reply',
        );
        await probe.native.agrees(target, desiredWire);
        final after = reply['snapshot'] as Map;
        require(
          same(probe.row(after, 'gate_thr')['value'], desiredDb),
          'Snapshot readback differs',
        );
        await probe.reject(
          'gate',
          fresh,
          'gate_thr',
          desiredDb,
          target,
          desiredWire,
        );
      },
    );
    await probe.check(
      'Stereo partner lock blocks channel processor write',
      () async {
        final path = '/ch/01/gate/thr';
        await probe.remember(path);
        final before = await probe.native.read(path);
        final guard = await probe.capture('gate');
        final other = await WebSocket.connect(uri.toString());
        final events = <Map<String, dynamic>>[];
        final sub = other.listen((raw) {
          if (raw is String) {
            final e = jsonDecode(raw);
            if (e is Map<String, dynamic>) events.add(e);
          }
        });
        var locked = false;
        try {
          other.add(Cmds.hello(false, true));
          await until(
            () => events.any((e) => e['type'] == 'hello'),
            'Second client hello',
          );
          other.add(jsonEncode({'cmd': 'lock', 'ch': 1}));
          await until(() => probe.locks.containsKey('1'), 'Partner lock');
          locked = true;
          await probe.reject('gate', guard, 'gate_thr', -20.0, path, before);
        } finally {
          if (locked) {
            other.add(jsonEncode({'cmd': 'unlock', 'ch': 1}));
            await until(() => !probe.locks.containsKey('1'), 'Partner unlock');
          }
          await sub.cancel();
          await other.close();
        }
      },
    );
    await probe.check(
      'Whole channel processing guarded bypass readback',
      () async {
        const path = '/ch/01/gate/on';
        final original = await probe.remember(path);
        if (original != 0 && original != 1) {
          throw Blocked('Native gate bypass has no reversible boolean value');
        }
        final guard = await probe.capture('channel_processing');
        final gate = probe.row(guard, 'gate_on');
        require(
          gate['value'] is bool &&
              gate['native'] == path &&
              gate['value'] == (original == 1),
          'Invalid gate bypass readback',
        );
        final desired = original == 0;
        final reply = await probe.request(
          'channel_processing',
          field: 'gate_on',
          value: desired,
          guard: guard,
        );
        require(
          reply['snapshot'] is Map && reply['message'] == null,
          'Whole-channel write lacks confirmed readback: $reply',
        );
        await probe.native.agrees(path, desired ? 1 : 0);
        await probe.reject(
          'channel_processing',
          guard,
          'gate_on',
          !desired,
          path,
          desired ? 1 : 0,
        );
      },
    );
    probe.checks.add({
      'check': 'Stereo link transition and unobserved ABA',
      'status': 'blocked',
      'reason': 'No independently verified reversible link operation preserving both channel states; missed A→B→A transitions are not detectable and are not claimed.',
    });
    probe.checks.add({
      'check': 'Restricted-role policy',
      'status': 'blocked',
      'reason': 'Hello does not provision a restricted-role fixture; separate policy fixture required.',
    });
  } catch (error) {
    probe.checks.add({
      'check': 'Input processor probe environment',
      'status': 'fail',
      'reason': '$error',
    });
  } finally {
    for (final entry in probe.originals.entries) {
      try {
        await probe.native.write(entry.key, entry.value);
        await probe.native.agrees(entry.key, entry.value);
        probe.checks.add({'check': 'Restore ${entry.key}', 'status': 'pass'});
      } catch (error) {
        probe.checks.add({
          'check': 'Restore ${entry.key}',
          'status': 'fail',
          'reason': '$error',
        });
      }
    }
    await probe.sub.cancel();
    await probe.socket.close();
    await File(report!).writeAsString(
      '${const JsonEncoder.withIndent('  ').convert({
        'fixture': 'x32',
        'scenario': 'input-processors',
        'checks': probe.checks,
        'engine_errors': probe.errors,
        'limits': ['Sidecar evidence only; no physical hardware or signal path claim', 'Observed revisions cannot prove unseen ABA or serve as desk-side compare-and-swap'],
      })}\n',
    );
    exitCode = probe.checks.any((r) => r['status'] == 'fail')
        ? 1
        : probe.checks.any((r) => r['status'] == 'blocked')
        ? 2
        : 0;
  }
}
