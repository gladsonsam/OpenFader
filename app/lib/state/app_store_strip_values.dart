part of 'app_store.dart';

extension _StripValues on _AppStoreContext {
  // Engine state, confirmations and pending edits stay in protocol units.
  // Only UI exposure/writes cross this boundary. SIM already uses the UI
  // law; lawForDriver('sim') would incorrectly select the X-Air fallback.
  double _protocolFaderToUi(double value) => engine.driver == 'sim'
      ? value
      : dbToPos(lawForDriver(engine.driver).toDb(value));

  double _uiFaderToProtocol(double value) => engine.driver == 'sim'
      ? value
      : lawForDriver(engine.driver).fromDb(posToDb(value));

  // Presentation resolves pending edits before confirmed values, even when an
  // older echo has already replaced the engine strip. Keep protocol units here.
  double _stripFaderValue(int ch, double fallback) {
    final value = ps.optimisticValue(_maps, ch, 'fader');
    return value is num ? value.toDouble().clamp(0.0, 1.0) : fallback;
  }

  bool _stripMuteValue(int ch, bool fallback) {
    final value = ps.optimisticValue(_maps, ch, 'mute');
    return value is bool ? value : fallback;
  }
}
