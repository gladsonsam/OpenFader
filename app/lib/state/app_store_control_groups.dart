part of 'app_store.dart';

// manufacturer mixer software guide pp54–55: actual gain is the algebraic dB sum of the
// member and assigned VCA masters; any off fader forces off. This is control
// indication, separate from the member's own fader/mute and from audio meters.
extension _ControlGroupInfluence on _AppStoreContext {
  String _controlGroupInfluence(int channel) {
    final labels = <String>[];
    final toDb =
        engine.driver == 'sim' ? posToDb : lawForDriver(engine.driver).toDb;
    final member = engine.strips.where((s) => s.ch == channel).firstOrNull;
    if (member == null) return '';
    var known = engine.valueKnown(channel, 'fader');
    var level = _stripFaderValue(channel, member.fader);
    var off = level <= 0;
    var effective = toDb(level);
    var muted = _stripMuteValue(channel, member.muted);
    var muteKnown = engine.valueKnown(channel, 'mute');
    var group = 1;
    for (final family in engine.topology.where((f) => f['kind'] == 'dca')) {
      final first = (family['first'] as num).toInt();
      final count = (family['count'] as num).toInt();
      for (var n = 0; n < count; n++, group++) {
        final field = 'dca${group}_assign';
        if (!_stripParamApplies(channel, field)) continue;
        if (!engine.valueKnown(channel, field)) {
          known = false;
          continue;
        }
        if (ps.optimisticValue(_maps, channel, field) != true) continue;
        final master =
            engine.strips.where((s) => s.ch == first + n).firstOrNull;
        if (master == null) {
          known = false;
          continue;
        }
        labels.add(master.name);
        if (engine.valueKnown(master.ch, 'fader')) {
          level = _stripFaderValue(master.ch, master.fader);
          off |= level <= 0;
          effective += toDb(level);
        } else {
          known = false;
        }
        if (engine.valueKnown(master.ch, 'mute')) {
          muted |= _stripMuteValue(master.ch, master.muted);
        } else {
          muteKnown = false;
        }
      }
    }
    if (labels.isEmpty) return '';
    final gain = !known
        ? '—'
        : off
            ? '−∞'
            : effective.toStringAsFixed(1);
    return '${labels.join(' + ')} · effective $gain dB${muted ? ' · muted' : !muteKnown ? ' · mute —' : ''}';
  }
}
