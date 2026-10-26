// Mixer data/write contract, independent of any screen composition.
import '../core/channel_name.dart';
import 'dart:ui' show Color;
import 'package:flutter/foundation.dart';
import '../engine.dart';

/// One bank-rail segment (bankDefs in the web layer).
class MixerBank {
  final String key;
  final String label;
  final Color color;
  final Color text;

  const MixerBank({
    required this.key,
    required this.label,
    required this.color,
    required this.text,
  });
}

class MixerBusRow {
  final String id;
  final String name;
  final String sub;
  final bool muted;
  final bool active;
  final ValueNotifier<double> meter; // 0..1 fill fraction

  const MixerBusRow({
    required this.id,
    required this.name,
    required this.sub,
    required this.muted,
    required this.active,
    required this.meter,
  });
}

/// Minimal store contract the mixer screen renders. Implemented by the
/// shell/owner over the live engine or a fixture sim. Meters arrive via
/// stable [ValueNotifier]s updated in place by [pollMeters] so the 80 ms
/// loop never rebuilds widgets; per-strip control state (fader/mute/pan)
/// rides the same pattern so one strip rebuilds from its own notifier while
/// the shell and siblings stay quiet. Structural state (strips/bank/bus,
/// solo banner) notifies through [ChangeNotifier].
abstract class MixerStore extends ChangeNotifier implements ChannelNameStore {
  List<Strip> get strips;
  double panOf(int ch);
  bool soloOf(int ch);
  bool get hasSolo;
  int get soloCount;
  ValueNotifier<double> meterOf(int ch);
  ValueNotifier<double> get masterMeter;
  double get masterFader;
  bool get masterMuted;
  String get masterName;
  List<MixerBank> get banks;
  String get bank;
  String get emptyBankMessage;
  List<MixerBusRow> get busRows;

  /// Stable per-strip control notifiers, updated in place on the hot path
  /// (fader/mute/pan drags and taps) without a global notify. Strips listen
  /// to their own ch only.
  ValueNotifier<double> faderOf(int ch);
  ValueNotifier<bool> muteOf(int ch);
  ValueNotifier<double> panNoteOf(int ch);
  ValueNotifier<bool> soloNoteOf(int ch);
  ValueNotifier<double> get masterFaderNote;
  ValueNotifier<bool> get masterMutedNote;

  /// 80 ms meter-loop target: pull engine
  /// levels into the meter notifiers in place; no setState here.
  Future<void> pollMeters();

  void setFader(int ch, double v); // cmdFader
  void setMute(int ch, bool muted); // Mute
  void toggleSolo(int ch); // Solo
  bool get canClearSolos;
  void clearSolos(); // ClearSolo
  void setPan(int ch, double v); // cmdParam pan
  void setMasterFader(double v); // MainFader
  void setMasterMuted(bool muted);
  void selectBus(String id); // bus master pick
  void setBank(String key);
}
