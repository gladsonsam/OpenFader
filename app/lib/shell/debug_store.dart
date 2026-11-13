// Shell adapter: debug responsibility. The shell owns polling.
import '../engine.dart';
import '../core/provider_diagnostics.dart';
import '../screens/debug.dart';
import 'store_adapter.dart';

class ShellDebugStore extends ShellStoreAdapter implements DebugStore {
  ShellDebugStore(super.app);

  int _tick = 0;

  @override
  int get tick => _tick;

  @override
  List<double> get spark => List.of(engine.meters);

  @override
  ProviderDiagnostics? get diagnostics => engine.providerDiagnostics;

  @override
  void requestDiagnostics(int offset) => send(Cmds.diagnostics(offset));

  @override
  List<DebugSection> get sections => [
        DebugSection(title: 'link', lines: [
          'client=${engine.clientId}',
          'driver=${engine.driver}',
          'state=${engine.linkState} name=${engine.linkName}',
          'strips=${engine.strips.length} meters=${engine.meters.length}',
        ]),
        DebugSection(title: 'strips', lines: [
          for (final s in engine.strips.take(8))
            'ch${s.ch} ${s.key} "${s.name}" f=${s.fader.toStringAsFixed(3)} m=${s.muted}',
          if (engine.strips.length > 8) '… +${engine.strips.length - 8} more',
        ]),
        DebugSection(title: 'caps', lines: [
          'ready=${engine.capsReady} params=${engine.supportedParams.length}',
          ...engine.supportedParams.take(12),
        ]),
        DebugSection(title: 'live', lines: [
          'main_fader=${engine.mainFader} main_muted=${engine.mainMuted}',
          'solos=${engine.solos} locks=${engine.locks}',
          'scenes=${engine.scenes.length} fx=${engine.fx.length} routing=${engine.routing.length}',
          'gr=${engine.grGate.length}/${engine.grComp.length} spectrum=${engine.spectrum.length}',
        ]),
        if (engine.error.isNotEmpty)
          DebugSection(title: 'error', lines: [engine.error]),
      ];

  @override
  Future<void> refresh() async {
    requestDiagnostics(0);
    captureFrame();
  }

  // Debug observes the shell drain because it must not own another poller.

  void captureFrame() {
    _tick++;
    notifyListeners();
  }

  @override
  void sendHello() => send(Cmds.hello());

  @override
  void sendStats() => send(Cmds.stats());

  @override
  void scan() => send(Cmds.scan());

  @override
  void requestInfo() => send(Cmds.info());
}
