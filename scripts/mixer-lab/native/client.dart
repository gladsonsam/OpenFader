import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../../../app/lib/engine_protocol.dart';
import '../../../app/lib/core/native_library.dart';
import '../osc.dart';

class NativeProbe {
  NativeProbe(this.options, this.uri, this.reportPath);

  factory NativeProbe.fromArguments(List<String> args) {
    final options = <String, String>{};
    for (var i = 0; i < args.length; i += 2) {
      require(i + 1 < args.length, 'Expected --option value');
      options[args[i]] = args[i + 1];
    }
    final uri = Uri.parse(options['--ws'] ?? '');
    require(
      uri.scheme == 'ws' && uri.host == '127.0.0.1',
      'Fresh loopback host required',
    );
    final reportPath = options['--report'];
    require(reportPath != null, '--report required');
    return NativeProbe(options, uri, reportPath!);
  }

  final Map<String, String> options;
  final Uri uri;
  final String reportPath;
  final osc = NativeOsc();
  final checks = <Map<String, Object?>>[], receipts = <Map<String, dynamic>>[];
  final errors = <Object?>[];
  final pending = <String, Completer<Map<String, dynamic>>>{};
  final originals = <String, Object>{};
  final ownedSlots = <String, int>{};
  final ownedPresets = <String, Map<String, dynamic>>{};
  final initialInventories = <Map<String, dynamic>>[];
  final prerequisiteEvidence = <Map<String, Object?>>[];
  String? nativeInventoryBlocked;
  WebSocket? socket;
  StreamSubscription<dynamic>? subscription;
  Map<String, dynamic>? info;
  int? epoch;
  var serial = 0, infoSerial = 0;
  String? nativeTransportBlocked;
  final runTag = 'OFN${DateTime.now().millisecondsSinceEpoch % 1000000}';

  Future<void> save() async {
    final report = {
      'time_utc': DateTime.now().toUtc().toIso8601String(),
      'host': uri.toString(),
      'scope': 'Native metadata and selected independent wire readback only',
      'checks': checks,
      'receipts': receipts,
      'initial_inventories': initialInventories,
      'native_prerequisites': prerequisiteEvidence,
      'engine_errors': errors,
      'owned_slots': ownedSlots,
      'owned_presets': ownedPresets,
      'limits': [
        'Fresh disposable emulator required; discard fixture after run',
        'Uncertain native receipts are preserved; no audio or complete scene/preset contents claimed',
        'Unavailable descriptor operations are blocked, never executed with invented guards',
        'Cue removal has no supported native action; only lab-created empty cue metadata can be restored',
      ],
    };
    await File(
      reportPath,
    ).writeAsString('${const JsonEncoder.withIndent('  ').convert(report)}\n');
  }

  Future<void> check(String name, Future<void> Function() action) async {
    final watch = Stopwatch()..start();
    try {
      await action();
      checks.add({
        'check': name,
        'status': 'pass',
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('PASS $name');
    } on Blocked catch (error) {
      checks.add({
        'check': name,
        'status': 'blocked',
        'reason': '$error',
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('BLOCKED $name: $error');
    } catch (error) {
      checks.add({
        'check': name,
        'status': 'fail',
        'reason': '$error',
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('FAIL $name: $error');
    }
    await save();
  }

  Future<Map<String, dynamic>> request(
    Map<String, dynamic> action, {
    Map<String, dynamic>? expected,
  }) async {
    if (nativeTransportBlocked != null) throw Blocked(nativeTransportBlocked!);
    final op = action['op'] as String;
    final id = '$runTag-${++serial}';
    final sentEpoch = epoch!;
    final done = Completer<Map<String, dynamic>>();
    pending[id] = done;
    socket!.add(
      jsonEncode({
        'cmd': 'native_library',
        'request_id': id,
        'driver': 'x32',
        'epoch': sentEpoch,
        'action': action,
        'expected': expected,
      }),
    );
    try {
      final event = await done.future.timeout(const Duration(seconds: 20));
      require(
        event['driver'] == 'x32' &&
            event['epoch'] == sentEpoch &&
            event['operation'] == op,
        'Native result correlation mismatch',
      );
      final result = Map<String, dynamic>.from(event['result'] as Map);
      require(
        ['confirmed', 'uncertain', 'rejected'].contains(result['status']),
        'Unknown native receipt status',
      );
      receipts.add({'request_id': id, 'action': action, 'result': result});
      return result;
    } on TimeoutException {
      nativeTransportBlocked = 'A native request exceeded the app 20-second deadline; dependent operations were not replayed';
      socket!.add(
        jsonEncode({'cmd': 'native_library_cancel', 'request_id': id}),
      );
      rethrow;
    } finally {
      pending.remove(id);
    }
  }

  Future<NativeLibraryVm> inspect() async {
    final result = await request({'op': 'inspect'});
    require(result['status'] == 'confirmed', 'Inspect rejected: $result');
    final parsed = NativeLibraryVm.parse(result['inventory']);
    require(
      parsed != null && parsed.connected && parsed.guard['provider'] == 'x32',
      'App rejected native inventory descriptor',
    );
    return parsed!;
  }

  Future<NativeLibraryVm> refresh() async {
    if (nativeInventoryBlocked != null) throw Blocked(nativeInventoryBlocked!);
    final before = await inspect();
    final result = await request({'op': 'refresh'}, expected: before.guard);
    final parsed = NativeLibraryVm.parse(result['inventory']);
    if (parsed == null)
      throw StateError('App rejected refreshed inventory descriptor');
    if (result['status'] != 'confirmed' || !parsed.complete) {
      throw Blocked('Native inventory unavailable: ${result['message']}');
    }
    return parsed;
  }

  Future<Map<String, dynamic>> action(
    Map<String, dynamic> payload, {
    bool reload = true,
  }) async {
    final current = reload ? await refresh() : await inspect();
    final op = payload['op'] as String;
    if (!current.complete || !current.supports(op)) {
      throw Blocked(
        '$op not advertised: ${current.banksNote ?? current.guard['error'] ?? 'inventory incomplete'}',
      );
    }
    final result = await request(payload, expected: current.guard);
    if (result['status'] == 'rejected')
      throw StateError('Native action rejected: ${result['message']}');
    if (result['status'] == 'uncertain' &&
        '${result['message']}'.contains('timed out')) {
      throw Blocked(
        'Native emulator did not confirm $op: ${result['message']}',
      );
    }
    return result;
  }

  Future<void> connect() async {
    socket = await WebSocket.connect(uri.toString())
        .timeout(const Duration(seconds: 10));
    subscription = socket!.listen((raw) {
      if (raw is! String) return;
      final event = jsonDecode(raw) as Map<String, dynamic>;
      if (event['type'] == 'sync_epoch') epoch = event['epoch'] as int;
      if (event['type'] == 'info' && event['driver'] == 'x32') {
        info = event;
        infoSerial++;
      }
      if (event['type'] == 'native_library_result') {
        final done = pending[event['request_id']];
        if (done != null && !done.isCompleted) done.complete(event);
      }
      if (event['type'] == 'error') errors.add(event);
    }, onError: (Object error) => errors.add('$error'));
    socket!.add(Cmds.hello(false, true));
    socket!.add(Cmds.connect('x32', '127.0.0.1'));
    await until(() => info != null && epoch != null, 'X32 connection');
    final before = infoSerial;
    socket!.add(Cmds.info());
    await until(() => infoSerial > before, 'Post-connect command readiness');
  }

  Future<void> finish() async {
    await check(
      'Restore independently captured native fixture values',
      () async {
        for (final entry in originals.entries) {
          await osc.write(entry.key, entry.value);
          await osc.agrees(entry.key, entry.value);
        }
      },
    );
    await save();
    await socket?.close();
    await subscription?.cancel();
    stdout.writeln('Evidence: $reportPath');
    exitCode = checks.any((c) => c['status'] == 'fail')
        ? 1
        : checks.any((c) => c['status'] == 'blocked')
        ? 2
        : 0;
  }
}
