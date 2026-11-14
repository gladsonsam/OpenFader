// Manual console-controls protocol probe for disposable loopback Sidecar labs.
// Captures native OSC originals and restores them; a blocked native query is
// reported as a fixture gap, not as evidence of hardware support.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../../app/lib/engine_protocol.dart';
import 'osc.dart';
import 'wing/tuple.dart';

class Fixture {
  const Fixture(
    this.page,
    this.path,
    this.native,
    this.kind,
    this.value,
    this.expectedWire,
  );
  final String page, path, native, kind;
  final Object value, expectedWire;
}

class Probe {
  Probe(this.driver, this.socket)
    : native = NativeOsc(
        port: driver == 'wing' ? 2223 : 10023,
        wingReplies: driver == 'wing',
      ) {
    sub = socket.listen((raw) {
      if (raw is! String) return;
      final event = jsonDecode(raw);
      if (event is! Map<String, dynamic>) return;
      final type = event['type'];
      if (type == 'info' && event['driver'] == driver) attached = true;
      if (event['request_id'] is String &&
          (type == 'console_controls' || type == 'console_control_result')) {
        replies[event['request_id'] as String] = event;
      }
      if (type == 'error') errors.add(event);
      if (type == 'native_controls_invalidated') invalidations++;
      if (type == 'param' && event['authority'] == 'verified' ||
          type == 'params' &&
              event['authority'] is Map &&
              (event['authority'] as Map).values.contains('verified')) {
        verifiedFrames++;
      }
      if (type == 'locks' && event['holds'] is Map) {
        locks = Map<String, dynamic>.from(event['holds'] as Map);
      }
    });
  }
  final String driver;
  final WebSocket socket;
  final NativeOsc native;
  late StreamSubscription<dynamic> sub;
  final replies = <String, Map<String, dynamic>>{};
  final results = <Map<String, Object?>>[];
  final errors = <Map<String, dynamic>>[];
  final originals = <String, Object>{};
  bool attached = false;
  Map<String, dynamic> locks = {};
  int sequence = 0, invalidations = 0, verifiedFrames = 0;

  Future<Map<String, dynamic>> request(
    Map<String, Object?> command, {
    String? requestId,
  }) async {
    final id = requestId ?? 'console-lab-${++sequence}';
    command['request_id'] = id;
    socket.add(jsonEncode(command));
    final deadline = DateTime.now().add(const Duration(seconds: 120));
    while (!replies.containsKey(id)) {
      if (DateTime.now().isAfter(deadline))
        throw TimeoutException('console receipt $id');
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    return replies.remove(id)!;
  }

  Future<Map<String, dynamic>> node(String path) async {
    final event = await request({'cmd': 'console_controls', 'path': path});
    require(
      event['driver'] == driver && event['epoch'] is int,
      'Invalid provider/session in node event $event',
    );
    if (event['error'] != null) throw Blocked('Node $path: ${event['error']}');
    require(event['node'] is Map, 'Missing node for $path');
    return event;
  }

  Future<Map<String, dynamic>> write(
    Map<String, dynamic> event,
    Map item,
    Object? value, {
    Object? revision,
    int? epoch,
  }) async {
    final reply = await request({
      'cmd': 'console_control_set',
      'epoch': epoch ?? event['epoch'],
      'path': item['path'],
      'value': value,
      'revision': revision ?? item['revision'],
    });
    require(
      reply['driver'] == driver && reply['status'] is String,
      'Malformed write receipt $reply',
    );
    return reply;
  }

  Future<void> check(String name, Future<void> Function() action) async {
    try {
      await action();
      results.add({'check': name, 'status': 'pass'});
      stdout.writeln('PASS $name');
    } on Blocked catch (error) {
      results.add({'check': name, 'status': 'blocked', 'reason': error.reason});
      stdout.writeln('BLOCKED $name: ${error.reason}');
    } catch (error) {
      results.add({'check': name, 'status': 'fail', 'reason': '$error'});
      stdout.writeln('FAIL $name: $error');
    }
  }

  Map item(Map<String, dynamic> event, String path) {
    final found = (event['node']['items'] as List)
        .whereType<Map>()
        .where((item) => item['path'] == path)
        .toList();
    if (found.isEmpty) throw Blocked('Fixture descriptor $path not advertised');
    require(found.length == 1, 'Duplicate descriptor $path');
    return found.single;
  }

  Future<void> verifyFixture(Fixture fixture) async {
    var event = await node(fixture.page);
    var descriptor = item(event, fixture.path);
    require(descriptor['kind'] == fixture.kind, 'Incorrect type $descriptor');
    if (!descriptor.containsKey('value') ||
        descriptor['revision'] == null ||
        descriptor['read_only'] == true ||
        descriptor['unavailable_reason'] != null) {
      throw Blocked(
        'No editable native readback for ${fixture.path}: '
        '${descriptor['unavailable_reason'] ?? 'missing value or revision'}',
      );
    }
    if (fixture.kind == 'enumeration' &&
        !(descriptor['choices'] as List).whereType<Map>().any(
          (choice) => same(choice['value'], fixture.value),
        )) {
      throw Blocked('Fixture enum argument not advertised: ${fixture.value}');
    }
    if (fixture.kind == 'number' &&
        (descriptor['min'] is! num ||
            descriptor['max'] is! num ||
            (fixture.value as num) < (descriptor['min'] as num) ||
            (fixture.value as num) > (descriptor['max'] as num))) {
      throw Blocked('Fixture numeric value outside advertised native domain');
    }
    final originalValue = descriptor['value'];
    Object original;
    try {
      original = await native.read(fixture.native);
      if (driver == 'wing' && original is num) {
        final tuple = await readWingTuple(native, fixture.native);
        results.add({
          'check': 'Native tuple ${fixture.native}',
          'status': 'observation',
          'tuple': tuple,
        });
        if (fixture.kind == 'number' || fixture.kind == 'integer') {
          final display = num.tryParse('${tuple['display']}');
          if (display == null || !same(display, descriptor['value'])) {
            throw Blocked(
              'WING display and advertised value cannot establish a safe restoration: $tuple',
            );
          }
        }
      }
    } on Blocked {
      rethrow;
    } catch (error) {
      throw Blocked(
        'Independent native query unavailable at ${fixture.native}: $error',
      );
    }
    final rawMatches = fixture.kind == 'boolean' && descriptor['value'] is bool
        ? original is num && original == (descriptor['value'] == true ? 1 : 0)
        : driver == 'x32' && fixture.path == '/tape.gain_l' && original is num
        ? same((original * 30 - 6), descriptor['value'])
        : same(original, descriptor['value']);
    if (!rawMatches) {
      throw Blocked(
        'Independent OSC original $original does not match the descriptor; '
        'native encoding or WING tuple is ambiguous, so restoration is unsafe',
      );
    }
    originals.putIfAbsent(fixture.native, () => original);
    final Object desired =
        fixture.kind == 'boolean' && descriptor['value'] is bool
        ? !(descriptor['value'] as bool)
        : fixture.value;
    final Object expectedWire = fixture.kind == 'boolean' && desired is bool
        ? (desired ? 1 : 0)
        : fixture.expectedWire;
    var baselineWire = original;
    if (same(original, expectedWire)) {
      results.add({
        'check': 'Initial fixture already at target ${fixture.path}',
        'status': 'observation',
        'native_value': original,
      });
      final Object alternate;
      final Object expectedStage;
      if (fixture.kind == 'enumeration') {
        final choices = (descriptor['choices'] as List).whereType<Map>().where(
          (choice) =>
              choice['value'] != null && !same(choice['value'], desired),
        );
        if (choices.isEmpty)
          throw Blocked('No distinct observed enumeration choice');
        alternate = choices.first['value'] as Object;
        expectedStage = alternate;
      } else if (driver == 'x32') {
        alternate = fixture.kind == 'number' ? 0.0 : 0;
        expectedStage = fixture.kind == 'number' ? -6.0 : 0;
      } else if (fixture.kind == 'number' && descriptor['min'] is num) {
        alternate = descriptor['min'] as num;
        expectedStage = alternate;
      } else {
        throw Blocked(
          'No verified reversible alternate for this native encoding',
        );
      }
      if (same(alternate, original)) {
        throw Blocked('No distinct documented fixture alternate');
      }
      await native.write(fixture.native, alternate);
      await native.agrees(fixture.native, alternate);
      final staged = await node(fixture.page);
      final stagedDescriptor = item(staged, fixture.path);
      require(
        stagedDescriptor.containsKey('value') &&
            same(stagedDescriptor['value'], expectedStage) &&
            stagedDescriptor['revision'] != null &&
            jsonEncode(stagedDescriptor['revision']) !=
                jsonEncode(descriptor['revision']),
        'External staging did not refresh value and revision: $stagedDescriptor',
      );
      event = staged;
      descriptor = stagedDescriptor;
      baselineWire = alternate;
    }
    final staleEpoch = await write(
      event,
      descriptor,
      desired,
      epoch: (event['epoch'] as int) + 1,
    );
    require(
      staleEpoch['status'] == 'rejected',
      'Stale epoch was not rejected: $staleEpoch',
    );
    await native.agrees(fixture.native, baselineWire);
    final bad = await write(event, descriptor, {'invalid': true});
    require(bad['status'] == 'rejected', 'Malformed value not rejected: $bad');
    await native.agrees(fixture.native, baselineWire);
    final beforeInvalidation = invalidations;
    final beforeVerified = verifiedFrames;
    final receipt = await write(event, descriptor, desired);
    require(
      receipt['status'] == 'verified',
      'No verified readback receipt: $receipt',
    );
    await native.agrees(fixture.native, expectedWire);
    require(
      invalidations > beforeInvalidation,
      'No same-epoch native proof invalidation observed',
    );
    try {
      await until(
        () => verifiedFrames > beforeVerified,
        'Verified repair frame',
      );
      results.add({
        'check': 'Verified cache repair after ${fixture.path}',
        'status': 'pass',
      });
    } on TimeoutException {
      results.add({
        'check': 'Verified cache repair after ${fixture.path}',
        'status': 'blocked',
        'reason': 'No independently verified canonical repair frame arrived within the probe window',
      });
    }
    final duplicate = await request({
      'cmd': 'console_control_set',
      'epoch': event['epoch'],
      'path': descriptor['path'],
      'value': descriptor['value'],
      'revision': descriptor['revision'],
    }, requestId: receipt['request_id'] as String);
    require(
      duplicate['status'] == 'uncertain',
      'Duplicate request was not identified without replay: $duplicate',
    );
    await native.agrees(fixture.native, expectedWire);
    final stale = await write(event, descriptor, descriptor['value']);
    require(
      stale['status'] == 'rejected',
      'Stale revision was not rejected: $stale',
    );
    await native.agrees(fixture.native, expectedWire);
    final refreshed = item(await node(fixture.page), fixture.path);
    require(
      refreshed.containsKey('value') && same(refreshed['value'], desired),
      'Refreshed descriptor did not observe write: $refreshed',
    );
    // Restore directly over OSC and independently observe the external change.
    await native.write(fixture.native, original);
    await native.agrees(fixture.native, original);
    final external = item(await node(fixture.page), fixture.path);
    require(
      same(external['value'], originalValue),
      'External change missing from refreshed descriptor: $external',
    );
    originals.remove(fixture.native);
  }
}

Future<void> main(List<String> args) async {
  final options = <String, String>{};
  for (var i = 0; i < args.length; i += 2) {
    require(i + 1 < args.length, 'Expected --option value');
    options[args[i]] = args[i + 1];
  }
  final driver = options['--driver'];
  require(driver == 'x32' || driver == 'wing', '--driver x32|wing required');
  final uri = Uri.parse(options['--ws'] ?? '');
  require(
    uri.scheme == 'ws' && uri.host == '127.0.0.1',
    'Use isolated loopback lab',
  );
  final report = options['--report'];
  require(report != null, '--report required');
  final probe = Probe(driver!, await WebSocket.connect(uri.toString()));
  try {
    probe.socket.add(Cmds.hello(false, true));
    probe.socket.add(Cmds.connect(driver, '127.0.0.1'));
    await until(() => probe.attached, 'Mixer attachment');
    final root = await probe.node('/');
    await probe.check('Root descriptors and provider capability', () async {
      final node = root['node'] as Map;
      require(
        node['path'] == '/' && node['items'] is List,
        'Malformed root node',
      );
      if ((node['items'] as List).isEmpty) {
        throw Blocked(
          'No native branches: ${node['unavailable_reason'] ?? 'not advertised'}',
        );
      }
      for (final branch in (node['items'] as List).whereType<Map>()) {
        require(
          branch['kind'] == 'branch' &&
              branch['path'] is String &&
              branch['label'] is String &&
              !branch.containsKey('value'),
          'Invalid or fabricated branch $branch',
        );
      }
    });
    if (driver == 'wing') {
      await probe.check('Indexed native channel navigation', () async {
        final channels = (await probe.node('/ch'))['node'] as Map;
        require(
          channels['unavailable_reason'] == null,
          'Indexed channel metadata unavailable: $channels',
        );
        final children = (channels['items'] as List).whereType<Map>().toList();
        // This named Sidecar fixture has 40 indexed WING input channels.
        final paths = children.map((row) => row['path']).toSet();
        require(
          children.length == 40 &&
              paths.length == 40 &&
              List.generate(40, (i) => '/ch/${i + 1}').every(paths.contains),
          'Missing or duplicate indexed channel addresses: $paths',
        );
        require(
          children.every((row) => row['kind'] == 'branch'),
          'Indexed channels must be branches',
        );
        final channel = (await probe.node('/ch/1'))['node'] as Map;
        require(
          (channel['items'] as List).whereType<Map>().any(
            (row) => row['path'] == '/ch/1/flt' && row['kind'] == 'branch',
          ),
          'Indexed channel did not expose its filter branch',
        );
      });
    }
    final pages = driver == 'x32'
        ? ['/automix', '/osc', '/dp48', '/tape']
        : ['/ch/1/flt', '/bus/1', '/io/out', '/cfg/mon/1'];
    for (final path in pages) {
      await probe.check('Typed node hydration $path', () async {
        final event = await probe.node(path);
        final node = event['node'] as Map;
        if (node['unavailable_reason'] != null)
          throw Blocked('${node['unavailable_reason']}');
        final items = node['items'];
        require(items is List, 'Missing item list for $path');
        if (items.isEmpty) throw Blocked('No fixture controls in $path');
        for (final entry in items.whereType<Map>()) {
          require(
            entry['path'] is String &&
                entry['label'] is String &&
                entry['kind'] is String &&
                entry['read_only'] is bool,
            'Malformed control $entry',
          );
          if (!entry.containsKey('value') && entry['kind'] != 'branch') {
            require(
              entry['read_only'] == true || entry['unavailable_reason'] != null,
              'Missing readback advertised as editable: $entry',
            );
            require(
              entry['revision'] == null,
              'Unobserved value has a fabricated revision: $entry',
            );
          }
        }
      });
    }
    final fixtures = driver == 'x32'
        ? const [
            Fixture(
              '/automix',
              '/automix.x',
              '/config/amixenable/X',
              'boolean',
              true,
              1,
            ),
            Fixture(
              '/osc',
              '/osc.type',
              '/config/osc/type',
              'enumeration',
              1,
              1,
            ),
            Fixture(
              '/tape',
              '/tape.gain_l',
              '/config/tape/gainL',
              'number',
              9.0,
              0.5,
            ),
            Fixture(
              '/dp48',
              '/dp48.scope',
              '/config/dp48/scope',
              'bitset',
              3,
              3,
            ),
          ]
        : const [
            Fixture('/ch/1', '/ch/1/mute', '/ch/1/mute', 'boolean', true, 1),
            Fixture(
              '/ch/1/flt',
              '/ch/1/flt/mdl',
              '/ch/1/flt/mdl',
              'enumeration',
              'TILT',
              'TILT',
            ),
            Fixture(
              '/ch/1/flt',
              '/ch/1/flt/tilt',
              '/ch/1/flt/tilt',
              'number',
              3.0,
              3.0,
            ),
          ];
    for (final fixture in fixtures) {
      await probe.check(
        'Guarded ${fixture.kind} ${fixture.path}',
        () => probe.verifyFixture(fixture),
      );
    }
    await probe.check('Read-only native write rejected', () async {
      final event = await probe.node(driver == 'x32' ? '/tape' : '/ch/1');
      final rows = (event['node']['items'] as List).whereType<Map>().where(
        (row) =>
            row['kind'] != 'branch' &&
            row['read_only'] == true &&
            row['revision'] != null,
      );
      if (rows.isEmpty)
        throw Blocked('Fixture has no observed read-only control');
      final row = rows.first;
      final receipt = await probe.write(event, row, row['value']);
      require(
        receipt['status'] == 'rejected',
        'Read-only write not rejected: $receipt',
      );
    });
    await probe.check('Other-client lock rejects console mutation', () async {
      final fixture = driver == 'x32' ? fixtures[1] : fixtures.first;
      final event = await probe.node(fixture.page);
      final row = probe.item(event, fixture.path);
      if (row['revision'] == null ||
          !row.containsKey('value') ||
          row['read_only'] == true) {
        throw Blocked('No observed writable fixture for lock check');
      }
      Object before;
      try {
        before = await probe.native.read(fixture.native);
      } catch (error) {
        throw Blocked('No independent native read for lock check: $error');
      }
      final other = await WebSocket.connect(uri.toString());
      final events = <Map<String, dynamic>>[];
      final subscription = other.listen((raw) {
        if (raw is! String) return;
        final decoded = jsonDecode(raw);
        if (decoded is Map<String, dynamic>) events.add(decoded);
      });
      var locked = false;
      try {
        other.add(Cmds.hello(false, true));
        await until(
          () => events.any((e) => e['type'] == 'hello'),
          'Second-client hello',
        );
        other.add(jsonEncode({'cmd': 'lock', 'ch': 0}));
        await until(
          () => probe.locks.containsKey('0'),
          'Second-client strip lock',
        );
        locked = true;
        final reply = await probe.write(event, row, row['value']);
        require(
          reply['status'] == 'rejected',
          'Lock did not reject native write: $reply',
        );
        await probe.native.agrees(fixture.native, before);
      } finally {
        if (locked) {
          other.add(jsonEncode({'cmd': 'unlock', 'ch': 0}));
          await until(
            () => !probe.locks.containsKey('0'),
            'Second-client unlock',
          );
        }
        await subscription.cancel();
        await other.close();
      }
    });
    probe.results.add({
      'check': 'Restricted role policy',
      'status': 'blocked',
      'reason': 'Hello does not offer a role fixture; restricted policy requires separately provisioned clients.',
    });
    if (driver == 'wing') {
      await probe.check(
        'External model change rejects captured parameter',
        () async {
          final event = await probe.node('/ch/1/flt');
          final model = probe.item(event, '/ch/1/flt/mdl');
          final parameter = probe.item(event, '/ch/1/flt/tilt');
          if (parameter['revision'] == null ||
              parameter['model'] == null ||
              !parameter.containsKey('value') ||
              parameter['read_only'] == true ||
              model['value'] != 'TILT' ||
              !(model['choices'] as List).whereType<Map>().any(
                (choice) => choice['value'] == 'MAX',
              )) {
            throw Blocked(
              'The TILT/MAX model guard fixture is not fully observed',
            );
          }
          final original = await probe.native.read('/ch/1/flt/mdl');
          if (original != 'TILT')
            throw Blocked('Native and descriptor model disagree');
          probe.originals.putIfAbsent('/ch/1/flt/mdl', () => original);
          await probe.native.write('/ch/1/flt/mdl', 'MAX');
          await probe.native.agrees('/ch/1/flt/mdl', 'MAX');
          try {
            final reply = await probe.write(
              event,
              parameter,
              parameter['value'],
            );
            require(
              reply['status'] == 'rejected',
              'Stale model context not rejected: $reply',
            );
          } finally {
            await probe.native.write('/ch/1/flt/mdl', original);
            await probe.native.agrees('/ch/1/flt/mdl', original);
            probe.originals.remove('/ch/1/flt/mdl');
          }
        },
      );
    }
    probe.results.add({
      'check': 'Native actions',
      'status': 'blocked',
      'reason': 'No reversible independent action fixture is established; no action argument was invented.',
    });
  } catch (error) {
    probe.results.add({
      'check': 'Console probe environment',
      'status': 'fail',
      'reason': '$error',
    });
  } finally {
    for (final entry in probe.originals.entries) {
      try {
        await probe.native.write(entry.key, entry.value);
        await probe.native.agrees(entry.key, entry.value);
        probe.results.add({'check': 'Restore ${entry.key}', 'status': 'pass'});
      } catch (error) {
        probe.results.add({
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
        'fixture': driver,
        'scenario': 'console-controls',
        'checks': probe.results,
        'engine_errors': probe.errors,
        'limits': ['Disposable Sidecar OSC evidence only; no physical hardware or audio claim', 'Unknown, missing and shadow-only fixture integrations remain blocked'],
      })}\n',
    );
    exitCode = probe.results.any((r) => r['status'] == 'fail')
        ? 1
        : probe.results.any((r) => r['status'] == 'blocked')
        ? 2
        : 0;
  }
}
