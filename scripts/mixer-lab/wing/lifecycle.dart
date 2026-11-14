import '../../../app/lib/engine_protocol.dart';
import '../osc.dart';
import 'client.dart';

extension WingLifecycle on WingProbe {
  Future<void> lifecyclePass() async {
    final invalidNow = <String>[];
    for (final family in families) {
      for (var n = 1; n <= (family['count'] as int); n++) {
        final path = '/${family['key']}/$n/col';
        if (await captureColor(path) == 0) invalidNow.add(path);
      }
    }
    await check('Initial invalid-colour fixture correctly fails closed on scene capture', () async {
      if (invalidNow.isEmpty) {
        throw Blocked(
          'No currently invalid colour in this fixture; initial invalid-colour capture case unavailable',
        );
      }
      const number = 999;
      if ((latest['scenes']?['scenes'] as List? ?? []).any(
        (s) => s['n'] == number,
      )) {
        throw Blocked('Reserved local scene slot is occupied');
      }
      var saved = false;
      try {
        try {
          await command(Cmds.sceneSave(number, 'WING invalid colour fixture'));
          saved = true;
        } catch (error) {
          require(
            '$error'.contains('color unavailable'),
            'Scene capture failed for another reason: $error',
          );
          throw Blocked(
            'Independent raw tuples show invalid native colour 0; scene capture correctly rejected incomplete state. Paths: $invalidNow',
          );
        }
        throw StateError(
          'Scene capture accepted an independently invalid colour',
        );
      } finally {
        if (saved) await command(Cmds.sceneDelete(number));
      }
    });
    for (final path in invalidNow) {
      await initializeColor(path);
    }
    results.add({
      'check': 'Valid-domain disposable fixture colour precondition',
      'status': 'observation',
      'initialized_to': 1,
      'paths': invalidNow,
      'reason': 'Conditional scene regression only; original invalid values require fixture disposal.',
    });
    // Capture the original model and its active native leaves before any
    // model switch. finish() restores the model before those leaves.
    const modelPath = '/ch/1/flt/mdl';
    const originalLeaves = {
      'TILT': ['tilt'],
      'MAX': ['low', 'proc'],
      'AP1': ['f'],
      'AP2': ['f', 'q'],
    };
    final originalModel = await remember(modelPath);
    if (!originalLeaves.containsKey(originalModel)) {
      throw Blocked('Unknown original filter model $originalModel');
    }
    for (final leaf in originalLeaves[originalModel]!) {
      await remember('/ch/1/flt/$leaf');
    }
    const fixtures = [
      (0.0, 'TILT', 'wing:filter_tilt', 'tilt', 3.0, 1.0, 'MAX'),
      (1.0, 'MAX', 'wing:filter_low', 'low', 100.0, 2.0, 'AP1'),
      (2.0, 'AP1', 'wing:filter_ap1_freq', 'f', 100.0, 3.0, 'AP2'),
      (3.0, 'AP2', 'wing:filter_ap2_q', 'q', 0.5, 0.0, 'TILT'),
    ];
    for (final f in fixtures) {
      await check(
        'Valid-domain disposable fixture: local scene restores ${f.$2} filter model and active parameter',
        () async {
          const number = 999, fader = '/ch/1/fdr';
          if ((latest['scenes']?['scenes'] as List? ?? []).any(
            (s) => s['n'] == number,
          )) {
            throw Blocked('Reserved local scene slot is occupied');
          }
          await remember(fader);
          var owned = false;
          try {
            await command(Cmds.param(0, 'wing:filter_model', f.$1));
            await native.agrees(modelPath, f.$2);
            await command(Cmds.param(0, f.$3, f.$5));
            await agreesEventually('/ch/1/flt/${f.$4}', f.$5);
            await command(Cmds.fader(0, 84.0 / 154));
            await agreesEventually(fader, -60.0);
            await command(Cmds.sceneSave(number, 'WING laboratory ${f.$2}'));
            owned = true;
            await command(Cmds.param(0, 'wing:filter_model', f.$6));
            await native.agrees(modelPath, f.$7);
            await command(Cmds.fader(0, 108.0 / 154));
            await agreesEventually(fader, -36.0);
            await command(Cmds.sceneRecall(number));
            await agreesEventually(modelPath, f.$2);
            await agreesEventually('/ch/1/flt/${f.$4}', f.$5);
            await agreesEventually(fader, -60.0);
          } finally {
            if (owned) await command(Cmds.sceneDelete(number));
          }
        },
      );
    }
    Future<void> freshTaps(int marker, Map<String, int> counts) async {
      await until(
        () => ['in', 'post'].every(
          (tap) =>
              (tapObserved[tap] ?? 0) > marker &&
              (tapCounts[tap] ?? 0) >= (counts[tap] ?? 0) + 2,
        ),
        'Two fresh WING native meter tap frames per advertised tap',
      );
      for (final tap in ['in', 'post']) {
        final levels = tapValues[tap]!;
        require(
          levels.length == 84 && levels.every((v) => v is num && v.isFinite),
          'Invalid $tap meter frame; native telemetry covers 76 audio strips and DCA 1–8',
        );
      }
      require(
        unexpectedTaps.isEmpty,
        'Unadvertised meter taps: $unexpectedTaps',
      );
    }

    await check(
      'Advertised meter taps have repeated fresh finite native frames',
      () async {
        final advertised = (caps!['meter_taps'] as List).toSet();
        require(
          advertised.length == 2 && advertised.containsAll({'in', 'post'}),
          'Unexpected WING meter tap descriptors: $advertised',
        );
        final marker = sequence, counts = Map<String, int>.from(tapCounts);
        await freshTaps(marker, counts);
        await until(
          () => meterValues != null && meterValues!.isNotEmpty,
          'WING meter frame',
        );
        final levels = meterValues!;
        require(
          levels.length == 84 && levels.every((x) => x is num && x.isFinite),
          'Invalid WING meters; DCA 9–16 have no documented meter slots',
        );
      },
    );
    await check(
      'Disconnect rejects writes and reconnect publishes fresh epoch',
      () async {
        final old = latest['sync_epoch']?['epoch'];
        require(old is int, 'No initial epoch');
        const path = '/ch/1/fdr';
        await remember(path);
        final original = await native.read(path);
        await command(Cmds.disconnect());
        final disconnected = latest['sync_epoch']?['epoch'];
        require(
          disconnected != old && latest['link']?['state'] == 'disconnected',
          'Disconnect did not invalidate epoch',
        );
        await until(
          () => meterValues != null && meterValues!.isEmpty,
          'Disconnected meter invalidation',
        );
        await Future<void>.delayed(const Duration(milliseconds: 250));
        require(
          tapValues.values.every((levels) => levels.isEmpty),
          'Nonempty tap data persisted after disconnect: $tapValues',
        );
        await command(Cmds.fader(0, 0.5), reject: true);
        await native.agrees(path, original);
        final marker = sequence, counts = Map<String, int>.from(tapCounts);
        await command(Cmds.connect('wing', '127.0.0.1'));
        require(
          latest['sync_epoch']?['epoch'] != disconnected &&
              latest['link']?['state'] == 'connected',
          'Reconnect failed',
        );
        await freshTaps(marker, counts);
        await change(path, -60.0, Cmds.fader(0, 84.0 / 154));
      },
    );
    results.add({
      'check': 'Native show and library workflows',
      'status': 'blocked',
      'reason': 'Headless Sidecar host publishes no native show; local scenes are distinct.',
    });
    results.add({
      'check': 'Native output routing and hardware audio',
      'status': 'blocked',
      'reason': 'Output capabilities and physical signal paths are not independently verified by this fixture.',
    });
  }
}
