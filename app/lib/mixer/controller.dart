import 'dart:async';
import 'package:flutter/foundation.dart';
import '../engine.dart';
import '../core/channel_name.dart';
import '../core/channel_views.dart' show parameterRange;
import '../state/app_store.dart';
import '../u4law.dart';
import 'design.dart';
import 'demo.dart';

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

enum MixerReadoutControl { volume, pan, gain }

typedef MixerReadoutFocus = ({String? key, MixerReadoutControl control});

/// Immutable strip input shared by phone/tablet/desktop. Meter notifications
/// are independent of control snapshots and go straight to the meter painter.
class MixerChannel {
  final String key, name, number;
  final int? channel;
  final int artwork;
  final ChannelColorChoice? colorChoice;
  final ChannelIconChoice? iconChoice;
  final bool hasIconChoices;
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
      this.colorChoice,
      this.iconChoice,
      this.hasIconChoices = false,
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

  /// Readout focus is independent of strip selection. Focus changes rebuild
  /// only the toolbar; control writes retain their existing store behavior.
  final readoutFocus = ValueNotifier<MixerReadoutFocus>(
      (key: null, control: MixerReadoutControl.volume));

  MixerChannel? get readoutChannel {
    final key = readoutFocus.value.key;
    if (key == 'master') return master;
    for (final strip in store.strips) {
      if (strip.key == key) return _channel(strip);
    }
    return selected;
  }

  void _focusReadout(MixerChannel ch, MixerReadoutControl control) {
    readoutFocus.value = (key: ch.key, control: control);
  }

  int _page = 0;
  int _pageSize = 8;

  /// Horizontal scroll offset, retained across tab switches so returning
  /// home lands on the same mixer view.
  double scrollOffset = 0;
  bool _demoSeeded = false;
  bool _allowDemoSeed;
  bool _seeding = false;
  bool _disposed = false;
  String _driver;
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
  int get page => _page;
  int get pageSize => _pageSize;
  List<int>? _linkedPair(int channel) => store.engine.stereoPairs
      .where((pair) =>
          pair.contains(channel) &&
          store.engine.valueKnown(pair.first, 'link') &&
          store.engine.params[pair.first]?['link'] == true)
      .firstOrNull;

  List<Strip> get _surfaceStrips {
    final strips = store.strips.where((s) => s.key != 'main').toList();
    final visible = strips.map((s) => s.ch).toSet();
    return strips.where((s) {
      final pair = _linkedPair(s.ch);
      return pair == null || pair.last != s.ch || !visible.contains(pair.first);
    }).toList(growable: false);
  }

  int get pageCount => (_surfaceStrips.length / _pageSize).ceil();
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
  List<MixerChannel> get channels => _surfaceStrips
      .skip(_page * _pageSize)
      .take(_pageSize)
      .map(_channel)
      .toList(growable: false);
  MixerChannel _channel(Strip s, {bool? selected}) => MixerChannel(
      key: s.key,
      name: s.name.isEmpty ? s.key.replaceAll('/', '').toUpperCase() : s.name,
      number: _linkedPair(s.ch) != null
          ? '${_linkedPair(s.ch)!.first + 1}–${_linkedPair(s.ch)!.last + 1}'
          : '${s.ch + 1}',
      channel: s.ch,
      artwork: _artwork(s),
      colorChoice: store.channelColorChoice(s.ch),
      iconChoice: store.channelIconChoice(s.ch),
      hasIconChoices: store.hasChannelIconChoices,
      fader: store.faderOf(s.ch).value,
      pan: store.panOf(s.ch),
      muted: store.muteOf(s.ch).value,
      solo: store.soloOf(s.ch),
      selected: selected ?? _selectedKey == s.key,
      master: false,
      meter: store.meterOf(s.ch),
      params: Map.unmodifiable({
        ...?store.engine.params[s.ch],
        // Gain edits are optimistic; engine snapshots lag behind the drag.
        if (store.channelIndex == s.ch && store.paramNum('gain') != null)
          'gain': store.paramNum('gain')!,
        if (store.engine.grGate.containsKey(s.ch))
          'gate_gr': store.engine.grGate[s.ch]!,
        if (store.engine.grComp.containsKey(s.ch))
          'comp_gr': store.engine.grComp[s.ch]!,
      }));
  int _artwork(Strip s) {
    final name = s.name.toLowerCase().replaceAll(' ', '');
    for (var i = 0; i < mixerDemoStrips.length - 1; i++) {
      if (mixerDemoStrips[i].name.toLowerCase().replaceAll(' ', '') == name) {
        return i;
      }
    }
    return demo && s.ch < 8 ? s.ch : 7;
  }

  MixerChannel get master => MixerChannel(
      key: 'master',
      name: store.masterName == 'MAIN' ? 'Main' : store.masterName,
      number:
          store.masterName == 'MAIN' ? 'LR' : store.masterName.split(' ').last,
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

  /// Processing follows the store's open channel, including direct navigation.
  /// Highlight its real strip without changing mixer selection or bank/scroll.
  MixerChannel? get selectedProcessing {
    for (final strip in store.engine.strips) {
      if (strip.key != 'main' && strip.ch == store.channelIndex) {
        return _channel(strip, selected: true);
      }
    }
    return null;
  }

  void _changed() {
    if (_disposed) return;
    if (_driver != store.engine.driver) {
      _driver = store.engine.driver;
      _selectedKey = null;
      readoutFocus.value = (key: null, control: MixerReadoutControl.volume);
      _page = 0;
      _allowDemoSeed = false;
    }
    for (final strip in store.strips) {
      if (strip.key != _selectedKey) continue;
      final pair = _linkedPair(strip.ch);
      if (pair != null && pair.last == strip.ch) {
        final primary =
            store.strips.where((s) => s.ch == pair.first).firstOrNull;
        if (primary != null) {
          _selectedKey = primary.key;
          if (readoutFocus.value.key == strip.key) {
            readoutFocus.value =
                (key: primary.key, control: readoutFocus.value.control);
          }
        }
      }
      break;
    }
    if (_page >= pageCount) _page = 0;
    if (_selectedKey != null &&
        _selectedKey != 'master' &&
        !store.strips.any((s) => s.key == _selectedKey)) {
      _selectedKey = channels.firstOrNull?.key;
    }
    _selectedKey ??= channels.firstOrNull?.key;
    final focusKey = readoutFocus.value.key;
    if (focusKey != null &&
        focusKey != 'master' &&
        !store.strips.any((s) => s.key == focusKey)) {
      readoutFocus.value = (key: null, control: MixerReadoutControl.volume);
    }
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
      final s = strips[i], ref = mixerDemoStrips[i];
      await store.sendCommand(Cmds.param(s.ch, 'name', ref.name));
      if (_disposed || !demo || !_allowDemoSeed) break;
      store.setFader(s.ch, MixerFaderScale.toEngine(ref.value));
      store.setPan(s.ch, (ref.pan - 50) / 36);
      store.setMute(s.ch, false);
    }
    if (!_disposed && demo && _allowDemoSeed) {
      store.setMasterFader(MixerFaderScale.toEngine(mixerDemoStrips[8].value));
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

  void select(MixerChannel channel) {
    _focusReadout(channel, MixerReadoutControl.volume);
    final selectionChanged = _selectedKey != channel.key;
    _selectedKey = channel.key;
    // A driver change or direct navigation can leave the open channel behind
    // the mixer selection. openChannel is a no-op when already aligned.
    if (channel.channel != null) store.openChannel(channel.channel!);
    if (selectionChanged) notifyListeners();
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
    if (ch.master) return;
    _focusReadout(ch, MixerReadoutControl.pan);
    store.setPan(ch.channel!, value);
  }

  List<double>? gainRangeOf(MixerChannel ch) {
    final index = ch.channel;
    if (ch.master ||
        index == null ||
        !store.engine.capsReady ||
        !store.engine.supportedParams.contains('gain') ||
        (store.engine.driver != 'sim' &&
            (!store.engine.connected ||
                !store.engine.valueKnown(index, 'gain')))) {
      return null;
    }
    return parameterRange(store.paramMetadata('gain'));
  }

  void setGain(MixerChannel ch, double value) {
    final range = gainRangeOf(ch);
    if (range == null || !value.isFinite) return;
    _focusReadout(ch, MixerReadoutControl.gain);
    store.openChannel(ch.channel!);
    store.cmdParam('gain', value.clamp(range.first, range.last));
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
    readoutFocus.dispose();
    super.dispose();
  }
}
