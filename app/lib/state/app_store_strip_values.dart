part of 'app_store.dart';

extension _StripValues on _AppStoreContext {
  // Capabilities are per family, not the union of every advertised strip.
  bool _stripParamApplies(int ch, String field) {
    if (!engine.supportedParams.contains(field)) return false;
    final metadata = engine.parameterMetadata[field];
    final rows = metadata?['presentations'];
    if (metadata?.containsKey('presentations') == true && rows is! List) return false;
    if (rows is List) {
      final explicit = rows.whereType<Map>()
          .where((row) => row.containsKey('applicable_strip_keys')).toList();
      if (explicit.length > 1) return false;
      if (explicit.isNotEmpty) {
        final keys = explicit.single['applicable_strip_keys'];
        final key = engine.strips.where((strip) => strip.ch == ch).firstOrNull?.key;
        final knownKeys = engine.strips.map((strip) => strip.key).toSet();
        if (keys is! List || keys.any((key) => key is! String || key.isEmpty || !knownKeys.contains(key)) ||
            keys.toSet().length != keys.length || key == null || !keys.contains(key)) {
          return false;
        }
      }
    }
    final scopes = metadata?['scopes'];
    final family = _presetFamily(ch);
    final tags = family?['params_scope'];
    final kinds = tags is List ? tags : [family?['kind']];
    return scopes is List && scopes.any(kinds.contains);
  }

  // Unknown laws cannot safely be converted to or from UI dB positions.
  bool get _hasFaderLaw =>
      engine.driver == 'sim' || lawForDriver(engine.driver) != null;

  double _protocolFaderToUi(double value) {
    if (engine.driver == 'sim') return value;
    final law = lawForDriver(engine.driver);
    return law == null ? double.nan : dbToPos(law.toDb(value));
  }

  double _uiFaderToProtocol(double value) {
    if (engine.driver == 'sim') return value;
    final law = lawForDriver(engine.driver);
    return law == null ? double.nan : law.fromDb(posToDb(value));
  }

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
