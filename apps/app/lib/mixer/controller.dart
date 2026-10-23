import 'dart:async';
import 'package:flutter/foundation.dart';
import '../engine.dart';
import '../state/app_store.dart';
import '../u4law.dart';
import 'design.dart';
import 'reference.dart';

enum MixerView { processing, faders }

enum MixerDestination {
  meters,
  channel,
  scenes,
  fx,
  routing,
  connect,
  settings,
  debug
}

/// Immutable strip input shared by phone/tablet/desktop. Meter notifications
/// are independent of control snapshots and go straight to the meter painter.
class MixerChannel {
  final String key, name, number;
  final int? channel;
  final int artwork;
  final double fader, pan;
  final bool muted, solo, selected, master;
  final ValueListenable<double> meter;
  final Map<String, Object> params;
  const MixerChannel(
      {required this.key,
      required this.name,
      required this.number,
      required this.channel,
      required this.artwork,
      required this.fader,
      required this.pan,
      required this.muted,
      required this.solo,
      required this.selected,
      required this.master,
      required this.meter,
      required this.params});
  String get levelLabel => '${fmtDb(fader)} dB';
}

/// One controller for a session, retained by AppShell across page navigation.
/// It adapts AppStore; it does not own a second poll loop or engine state.
class MixerController extends ChangeNotifier {
  final AppStore store;
  String? _selectedKey;
  int _page = 0;
  int _pageSize = 8;
  bool _demoSeeded = false;
  bool _allowDemoSeed;
  bool _seeding = false;
  bool _disposed = false;
  String _driver;
  MixerView _view = MixerView.processing;
  MixerController(this.store, {bool seedReferenceDemo = false})
      : _allowDemoSeed = seedReferenceDemo,
        _driver = store.engine.driver {
    store.addListener(_changed);
    scheduleMicrotask(_maybeSeedDemo);
  }
  bool get demo => store.engine.driver == 'sim';
  bool get connected => store.engine.connected;
  String get connectionLabel => demo
      ? 'Demo'
      : connected
          ? (store.engine.model.isEmpty ? 'Connected' : store.engine.model)
          : 'Disconnected';
  String get error => store.visibleError;
  MixerView get view => _view;
  int get page => _page;
  int get pageSize => _pageSize;
  int get pageCount =>
      (store.strips.where((s) => s.key != 'main').length / _pageSize).ceil();
  int get muteGroupCount => store.engine.muteGroupCount;
  bool get hasSolo => store.hasSolo;

  /// Per-strip control notifiers (passthrough to the store): strips listen
  /// to their own ch so fader/mute/pan writes rebuild one strip, never the
  /// page. See AppStore: the hot path updates these in place with no
  /// global notifyListeners().
  ValueListenable<double> faderNoteOf(int? ch, bool master) =>
      master || ch == null ? store.masterFaderNote : store.faderOf(ch);
  ValueListenable<bool> muteNoteOf(int? ch, bool master) =>
      master || ch == null ? store.masterMutedNote : store.muteOf(ch);
  ValueListenable<double> panNoteOf(int ch) => store.panNoteOf(ch);
  ValueListenable<bool> soloNoteOf(int ch) => store.soloNoteOf(ch);
  List<MixerChannel> get channels => store.strips
      .where((s) => s.key != 'main')
      .skip(_page * _pageSize)
      .take(_pageSize)
      .map(_channel)
      .toList(growable: false);
  MixerChannel _channel(Strip s) => MixerChannel(
      key: s.key,
      name: s.name.isEmpty ? s.key.toUpperCase() : s.name,
      number: '${s.ch + 1}',
      channel: s.ch,
      artwork: _artwork(s),
      fader: s.fader,
      pan: store.panOf(s.ch),
      muted: s.muted,
      solo: store.soloOf(s.ch),
      selected: _selectedKey == s.key,
      master: false,
      meter: store.meterOf(s.ch),
      params: Map.unmodifiable({
        ...?store.engine.params[s.ch],
        if (store.engine.grGate.containsKey(s.ch))
          'gate_gr': store.engine.grGate[s.ch]!,
        if (store.engine.grComp.containsKey(s.ch))
          'comp_gr': store.engine.grComp[s.ch]!,
      }));
  int _artwork(Strip s) {
    final name = s.name.toLowerCase().replaceAll(' ', '');
    for (var i = 0; i < masterFaderReference.length - 1; i++) {
      if (masterFaderReference[i].name.toLowerCase().replaceAll(' ', '') ==
          name) {
        return i;
      }
    }
    return demo && s.ch < 8 ? s.ch : 7;
  }

  MixerChannel get master => MixerChannel(
      key: 'master',
      name: store.masterName == 'MAIN' ? 'Main' : store.masterName,
      number: 'LR',
      channel: null,
      artwork: 8,
      fader: store.masterFader,
      pan: 0,
      muted: store.masterMuted,
      solo: false,
      selected: _selectedKey == 'master',
      master: true,
      meter: store.masterMeter,
      params: const {});
  MixerChannel? get selected {
    if (_selectedKey == 'master') return master;
    for (final s in store.engine.strips) {
      if (s.key == _selectedKey) return _channel(s);
    }
    return channels.firstOrNull;
  }

  void _changed() {
    if (_disposed) return;
    if (_driver != store.engine.driver) {
      _driver = store.engine.driver;
      _selectedKey = null;
      _page = 0;
      _allowDemoSeed = false;
    }
    if (_page >= pageCount) _page = 0;
    _selectedKey ??= channels.firstOrNull?.key;
    _maybeSeedDemo();
    notifyListeners();
  }

  Future<void> _maybeSeedDemo() async {
    if (_disposed ||
        !demo ||
        !_allowDemoSeed ||
        _demoSeeded ||
        _seeding ||
        store.strips.length < 8) {
      return;
    }
    _seeding = true;
    _demoSeeded = true;
    // Explicitly isolated to the demo driver. No reference names or values
    // are ever written to a desk, including a driver switch during awaits.
    final strips = store.strips.take(8).toList();
    for (var i = 0; i < strips.length; i++) {
      if (_disposed || !demo || !_allowDemoSeed) break;
      final s = strips[i], ref = masterFaderReference[i];
      await store.sendCommand(Cmds.param(s.ch, 'name', ref.name));
      if (_disposed || !demo || !_allowDemoSeed) break;
      store.setFader(s.ch, MixerFaderScale.toEngine(ref.value));
      store.setPan(s.ch, (ref.pan - 50) / 36);
      store.setMute(s.ch, false);
    }
    if (!_disposed && demo && _allowDemoSeed) {
      store.setMasterFader(
          MixerFaderScale.toEngine(masterFaderReference[8].value));
      store.setMasterMuted(false);
      if (_selectedKey == null || _selectedKey == strips.first.key) {
        select(_channel(strips[1]));
      }
    }
    _seeding = false;
  }

  void configurePageSize(int count) {
    if (_pageSize == count) return;
    final first = _page * _pageSize;
    _pageSize = count;
    _page = first ~/ count;
    notifyListeners();
  }

  void setView(MixerView value) {
    if (_view == value) return;
    _view = value;
    notifyListeners();
  }

  void select(MixerChannel channel) {
    if (_selectedKey == channel.key) return;
    _selectedKey = channel.key;
    if (channel.channel != null) store.openChannel(channel.channel!);
    notifyListeners();
  }

  void setPosition(MixerChannel ch, double position) {
    select(ch);
    final value = MixerFaderScale.toEngine(position);
    if (ch.master) {
      store.setMasterFader(value);
    } else {
      store.setFader(ch.channel!, value);
    }
  }

  void unity(MixerChannel ch) => setPosition(ch, .758);
  void toggleMute(MixerChannel ch) {
    if (ch.master) {
      store.setMasterMuted(!store.masterMutedNote.value);
    } else {
      // Read the live notifier, not the snapshot: the page no longer
      // rebuilds on mute taps so ch.muted may be stale.
      final live =
          ch.channel != null ? store.muteOf(ch.channel!).value : ch.muted;
      store.setMute(ch.channel!, !live);
    }
  }

  void toggleSolo(MixerChannel ch) {
    if (!ch.master) store.toggleSolo(ch.channel!);
  }

  void setPan(MixerChannel ch, double value) {
    if (!ch.master) store.setPan(ch.channel!, value);
  }

  void setGain(MixerChannel ch, double value) {
    if (ch.master) return;
    store.openChannel(ch.channel!);
    store.cmdParam('gain', value.clamp(-12, 60));
  }

  void nextPage(int delta) {
    final next = (_page + delta).clamp(0, (pageCount - 1).clamp(0, 999));
    if (next == _page) return;
    _page = next;
    notifyListeners();
  }

  void setBank(String key) {
    _page = 0;
    store.setBank(key);
    final first = channels.firstOrNull;
    if (first != null) select(first);
  }

  void moveSelection(int delta) {
    final all = [...channels, master];
    final index = all.indexWhere((s) => s.key == _selectedKey);
    select(all[(index + delta).clamp(0, all.length - 1)]);
  }

  void nudgeSelected(double delta) {
    final ch = selected;
    if (ch != null) {
      // Live fader: snapshots go stale between global rebuilds now that
      // fader drags no longer notify.
      final live = ch.master
          ? store.masterFaderNote.value
          : ch.channel != null
              ? store.faderOf(ch.channel!).value
              : ch.fader;
      setPosition(ch, MixerFaderScale.fromEngine(live) + delta);
    }
  }

  void toggleMuteGroup(int index) {
    if (index < 0 || index >= muteGroupCount) return;
    store.sendRaw(
        Cmds.muteGroup(index, !(store.engine.muteGroups[index] ?? false)));
  }

  @override
  void dispose() {
    _disposed = true;
    store.removeListener(_changed);
    super.dispose();
  }
}
