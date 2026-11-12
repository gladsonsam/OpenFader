import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';

class X32Probe {
  final Uri uri;
  final WebSocket socket;
  late final StreamSubscription<dynamic> subscription;
  final native = NativeOsc();
  final results = <Map<String, Object?>>[];
  final originals = <String, Object>{};
  final unavailableNativeReads = <String, String>{};
  final unavailableNativeFamilies = <String, String>{};
  final errors = <Map<String, dynamic>>[];
  final destinationReceipts = <String, Map<String, dynamic>>{};
  final latest = <String, Map<String, dynamic>>{};
  final values = <String, Object?>{}, valueSequence = <String, int>{};
  var sequence = 0, infoSequence = 0, helloSequence = 0, meters = 0;
  Map<String, dynamic>? caps;

  List<Map> get families => (caps!['topology'] as List).cast<Map>();
  List<Map> get metadata => (caps!['params'] as List).cast<Map>();

  X32Probe(this.uri, this.socket) {
    subscription = socket.listen((raw) {
      if (raw is! String) return;
      final e = jsonDecode(raw) as Map<String, dynamic>;
      sequence++;
      latest[e['type'] as String] = e;
      if (e['type'] == 'caps' && e['driver'] == 'x32') caps = e;
      if (e['type'] == 'info') infoSequence = sequence;
      if (e['type'] == 'hello') helloSequence = sequence;
      if (e['type'] == 'error') errors.add(e);
      if (e['type'] == 'destination_control_result' &&
          e['request_id'] is String) {
        destinationReceipts[e['request_id'] as String] = e;
      }
      if (e['type'] == 'meters') meters++;
      if (e['type'] == 'sync_epoch') {
        values.clear();
        valueSequence.clear();
      }
      if (e['type'] == 'params')
        for (final entry in (e['values'] as Map).entries) {
          final key = '${e['ch']}/${entry.key}';
          values[key] = entry.value;
          valueSequence[key] = sequence;
        }
      if (e['type'] == 'param') {
        final key = '${e['ch']}/${e['field']}';
        values[key] = e['value'];
        valueSequence[key] = sequence;
      }
    });
  }

  Future<void> barrier() async {
    // Hello is a local unicast acknowledgement on this same serialized
    // command stream. It remains valid while the native desk is disconnected;
    // Info deliberately reports an error when no native session exists.
    final start = helloSequence;
    socket.add(Cmds.hello(false, true));
    await until(() => helloSequence > start, 'Command completion');
  }

  Future<List<Map<String, dynamic>>> command(
    String c, {
    bool reject = false,
  }) async {
    final start = errors.length;
    final sent = jsonDecode(c) as Map<String, dynamic>;
    final destinationId = sent['cmd'] == 'destination_param'
        ? sent['request_id'] as String
        : null;
    if (destinationId != null) destinationReceipts.remove(destinationId);
    socket.add(c);
    await barrier();
    final found = errors.sublist(start);
    if (destinationId != null && found.isEmpty) {
      final receipt = destinationReceipts[destinationId];
      require(
        receipt != null,
        'Missing correlated destination receipt $destinationId',
      );
      require(
        receipt!['epoch'] == sent['epoch'],
        'Destination receipt epoch mismatch',
      );
      if (receipt['accepted'] != true) {
        found.add({
          'type': 'destination_rejection',
          'message': receipt['message'] ?? 'Destination write rejected',
          'request_id': destinationId,
        });
      } else {
        require(
          !reject,
          'Expected destination rejection, received accepted receipt',
        );
        final applied = receipt['applied'];
        require(
          applied is Map &&
              applied['target'] == sent['target'] &&
              applied['id'] == sent['control'] &&
              applied['authority'] == 'verified' &&
              applied['revision'] != null &&
              applied['generation'] != null &&
              same(applied['value'], sent['value']),
          'Destination receipt lacks exact applied-value proof: $receipt',
        );
        require(
          receipt['message'] == null,
          'Destination receipt reports a conflicting latest value: $receipt',
        );
      }
    }
    require(
      reject ? found.isNotEmpty : found.isEmpty,
      reject ? 'Expected explicit rejection' : 'Command error: $found',
    );
    return found;
  }

  Future<void> check(String name, Future<void> Function() f) async {
    final watch = Stopwatch()..start();
    try {
      await f();
      results.add({
        'check': name,
        'status': 'pass',
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('PASS $name');
    } on Blocked catch (e) {
      results.add({
        'check': name,
        'status': 'blocked',
        'reason': e.reason,
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('BLOCKED $name: $e');
    } catch (e) {
      results.add({
        'check': name,
        'status': 'fail',
        'reason': '$e',
        'elapsed_ms': watch.elapsedMilliseconds,
      });
      stdout.writeln('FAIL $name: $e');
    }
  }

  Future<Object> remember(String path) async {
    final absentFamily = RegExp(r'^/config/userrout/(in|out)/')
        .firstMatch(path)?[0];
    final familyReason = unavailableNativeFamilies[absentFamily];
    if (familyReason != null) {
      final reason = familyReason;
      throw Blocked(
        'Pinned Sidecar schema has no $absentFamily node family; representative independent absence: $reason',
      );
    }
    if (unavailableNativeReads[path] case final reason?) {
      throw Blocked(
        'Earlier independent read of $path was unavailable: $reason',
      );
    }
    try {
      final value = await native.read(path);
      originals.putIfAbsent(path, () => value);
      return value;
    } catch (error) {
      // Cache only missing baseline reads. Dynamic FX algorithm parameter reads
      // bypass this function and never reuse absence from a different schema.
      unavailableNativeReads[path] = '$error';
      if (absentFamily != null) {
        // Pin1330613 livedaw-x32/src/map.rs has no userrout subtree.
        // The first full run also independently queried all80 absent leaves.
        unavailableNativeFamilies[absentFamily] = '$path: $error';
      }
      rethrow;
    }
  }

  Future<void> change(String path, Object expected, String c) async {
    await remember(path);
    await command(c);
    await native.agrees(path, expected);
  }

  Future<void> attach() async {
    socket.add(Cmds.hello(false, true));
    socket.add(Cmds.connect('x32', '127.0.0.1'));
    await until(
      () =>
          caps != null &&
          infoSequence > 0 &&
          latest['info']?['driver'] == 'x32' &&
          helloSequence > 0,
      'X32 attachment and initial hello acknowledgement',
    );
    await barrier();
  }
}
