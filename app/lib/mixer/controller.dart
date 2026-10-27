import 'dart:async';
import 'package:flutter/foundation.dart';
import '../engine.dart';
import '../core/channel_name.dart';
import '../core/access_limits.dart';
import '../screens/channel.dart' show ChannelTab;
import '../core/channel_views.dart' show parameterRange;
import '../settings/prefs.dart';
import '../state/app_store.dart';
import '../u4law.dart';
import 'design.dart';
import 'demo.dart';
import 'mixer_layers.dart';

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
    PrefsService.instance.addListener(_layersChanged);
    scheduleMicrotask(_maybeSeedDemo);
  }
  bool get demo => store.engine.driver == 'sim';
  bool get connected => store.engine.connected;
  String get connectionLabel => demo
      ? 'Demo'
      : connected
          ? (store.engine.model.isEmpty ? 'Connected' : store.engine.model)
          : 'Disconnected';
  String get connectionDisplayName {
    final model = store.engine.model.trim();
    final make = switch (store.engine.driver) {
      'xair' || 'wing' => 'Behringer',
      'dl32s' => 'Mackie',
      _ => '',
    };
    if (model.isEmpty) return make.isEmpty ? 'Mixer' : make;
    if (make.isEmpty || model.toLowerCase().startsWith(make.toLowerCase())) {
      return model;
    }
    return '$make $model';
  }
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

  /// Active custom layer, or the built-in All layer (unknown ids fall back
  /// to All via [MixerLayersState]).
  MixerLayer get activeLayer => PrefsService.instance.mixerLayersState.activeLayer;
  bool get isCustomLayerActive => !activeLayer.isAll;

  /// Custom layers ignore the bank rail: their channel source is the layer's
  /// own [MixerLayer.channelKeys] order, not the bank-filtered surface
  /// strips. The All layer keeps today's bank paging behavior unchanged.
  List<Strip> get _effectiveStrips {
    final layer = activeLayer;
    if (layer.isAll) return _surfaceStrips;
    final byKey = {
      for (final s in store.engine.strips)
        if (s.key != 'main') s.key: s,
    };
    final picked = <Strip>[];
    for (final key in layer.channelKeys) {
      final strip = byKey[key];
      // Unknown keys (desk reconfigured since the layer was saved) are
      // ignored; duplicates collapse to first occurrence.
      if (strip == null || picked.any((s) => s.key == key)) continue;
      picked.add(strip);
    }
    // Preserve stereo-link collapse with layer-local visibility: a linked
    // secondary drops out only when its primary is also in the layer.
    final present = picked.map((s) => s.ch).toSet();
    return picked.where((s) {
      final pair = _linkedPair(s.ch);
      return pair == null || pair.last != s.ch || !present.contains(pair.first);
    }).toList(growable: false);
  }

  /// Custom layers page by their own density; All keeps the configured size.
  int get effectivePageSize => isCustomLayerActive
      ? activeLayer.effectiveStripsPerPage
      : _pageSize;

  int get pageCount => (_effectiveStrips.length / effectivePageSize).ceil();
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
  List<MixerChannel> get channels => _effectiveStrips
      .skip(_page * effectivePageSize)
      .take(effectivePageSize)
      .map(_channel)
      .toList(growable: false);

  /// Empty-state copy for a custom layer with no resolvable channels.
  String get layerEmptyMessage =>
      '“${activeLayer.name}” has no channels to show.\nLong-press the bank rail to edit layers.';
  String _stripNumber(Strip strip) {
    final family = store.engine.topology.where((row) {
      final first = (row['first'] as num).toInt();
      final count = (row['count'] as num).toInt();
      return strip.ch >= first && strip.ch < first + count;
    }).firstOrNull;
    if (family == null || family['kind'] == 'in') return '${strip.ch + 1}';
    final index = strip.ch - (family['first'] as num).toInt() + 1;
    return switch (family['key']) {
      'aux' => 'AUX',
      'fxret' => 'FX $index',
      'fxsnd' => 'FX $index',
      'bus' => 'BUS $index',
      'dca' => 'DCA $index',
      _ => '${family['label'] ?? 'Strip'} $index',
    };
  }

  MixerChannel _channel(Strip s, {bool? selected}) => MixerChannel(
      key: s.key,
      name: s.name.isEmpty ? s.key.replaceAll('/', '').toUpperCase() : s.name,
      number: _linkedPair(s.ch) != null
          ? '${_linkedPair(s.ch)!.first + 1}–${_linkedPair(s.ch)!.last + 1}'
          : _stripNumber(s),
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

  Strip? get _masterStrip => store.engine.strips
      .where((s) => s.key == store.masterProcessingTarget).firstOrNull;

  int? get masterIdentityChannel =>
      store.masterProcessingTarget.startsWith('bus/') ? _masterStrip?.ch : null;

  MixerChannel get master {
    final target = store.masterProcessingTarget;
    final strip = _masterStrip;
    final bus = target.startsWith('bus/');
    final busNumber = bus ? int.tryParse(target.substring(4)) : null;
    return MixerChannel(
      key: 'master',
      name: store.masterName == 'MAIN' ? 'Main' : store.masterName,
      number: target == 'main' ? 'LR' : busNumber?.toString() ?? store.masterName,
      channel: null,
      artwork: 8,
      colorChoice: bus && strip != null ? store.channelColorChoice(strip.ch) : null,
      fader: store.masterFader,
      pan: 0,
      muted: store.masterMuted,
      solo: store.soloOf(-1),
      selected: _selectedKey == 'master',
      master: true,
      meter: store.masterMeter,
      params: strip == null ? const {} : Map.unmodifiable(store.engine.params[strip.ch] ?? const {}));
  }
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
    if (store.outputProcessingTarget != null) return master;
    for (final strip in store.engine.strips) {
      if (strip.ch == store.channelIndex) {
        return strip.key == 'main' ? master : _channel(strip, selected: true);
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

  /// Layer prefs changed (possibly from Settings while the mixer is open):
  /// clamp the page to the new source and drop a selection the layer cannot
  /// resolve. Prefs notifies on every preference write, so this stays cheap
  /// and never writes prefs back (no notify loop).
  void _layersChanged() {
    if (_disposed) return;
    if (_page >= pageCount) _page = 0;
    if (_selectedKey != null &&
        _selectedKey != 'master' &&
        !_effectiveStrips.any((s) => s.key == _selectedKey)) {
      _selectedKey = channels.firstOrNull?.key;
    }
    final focusKey = readoutFocus.value.key;
    if (focusKey != null &&
        focusKey != 'master' &&
        !store.strips.any((s) => s.key == focusKey)) {
      readoutFocus.value = (key: null, control: MixerReadoutControl.volume);
    }
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

  bool get canOpenMasterProcessing => store.outputProcessingControls
      .any((control) => control.target == store.masterProcessingTarget &&
          store.destinationControlIsCurrent(control));

  bool openMasterProcessing() {
    final target = store.masterProcessingTarget;
    if (!store.accessFamilyAllowed(AccessFamily.masterDsp) ||
        !store.accessTargetAllowed(target)) {
      return false;
    }
    if (canOpenMasterProcessing) {
      store.openOutputProcessing(target);
      return store.outputProcessingTarget == target;
    }
    if (store.engine.driver != 'xair') return false;
    final strip = store.engine.strips.where((s) => s.key == target).firstOrNull;
    if (strip == null) return false;
    store.openChannel(strip.ch);
    store.setTab(ChannelTab.eq);
    return true;
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
    store.toggleSolo(ch.master ? -1 : ch.channel!);
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
        !store.canEditControl(index, 'gain') ||
        !store.engine.capsReady ||
        !store.engine.supportedParams.contains('gain') ||
        (store.engine.driver != 'sim' &&
            (!store.engine.connected ||
                !store.engine.valueKnown(index, 'gain')))) {
      return null;
    }
    return parameterRange(store.paramMetadata('gain'));
  }

  bool hasPhantomOf(MixerChannel ch) => ch.channel != null &&
      !ch.master && store.canEditControl(ch.channel!, 'phantom');

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

  Future<void> setBank(String key) async {
    _page = 0;
    if (isCustomLayerActive) {
      await PrefsService.instance.setActiveMixerLayerId(MixerLayer.allLayerId);
    }
    store.setBank(key);
    final first = channels.firstOrNull;
    if (first != null) select(first);
  }

  /// Switch the active layer. Unknown ids are ignored (the All fallback in
  /// [activeLayer] would otherwise silently change the view). Mirrors
  /// [setBank]: reset to the first page and select its first strip.
  Future<void> setActiveLayer(String id) async {
    final prefs = PrefsService.instance;
    final known = id == MixerLayer.allLayerId ||
        prefs.mixerLayers.any((layer) => layer.id == id);
    if (!known || prefs.activeMixerLayerId == id) return;
    _page = 0;
    await prefs.setActiveMixerLayerId(id);
    // The prefs listener (_layersChanged) already clamped the page and
    // selection; align the open channel like a bank switch does.
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
    PrefsService.instance.removeListener(_layersChanged);
    readoutFocus.dispose();
    super.dispose();
  }
}
