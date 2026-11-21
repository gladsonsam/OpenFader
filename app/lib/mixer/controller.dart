import 'dart:async';
import 'dart:ui' show Color;
import 'package:flutter/foundation.dart';
import '../palette.dart';
import '../engine.dart';
import '../core/channel_name.dart';
import '../core/mixer_provider_policy.dart';
import '../core/access_limits.dart';
import '../screens/channel.dart' show ChannelTab;
import '../core/channel_views.dart' show parameterRange;
import '../settings/prefs.dart';
import '../state/app_store.dart';
import '../u4law.dart';
import '../core/fader_law.dart' show lawForDriver, designLaw;
import '../core/control_reset.dart';
import 'design.dart';
import 'mixer_layers.dart';
import 'store.dart' show CustomMixerView;

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

  /// Legacy simulation processing fixture index; never icon identity.
  final int artwork;
  final ChannelColorChoice? colorChoice;
  final ResolvedChannelIcon icon;
  final double fader, pan;
  final bool muted, solo, selected, master;
  final ValueListenable<double> meter;
  final Map<String, Object> params;
  final List<double>? eqPreview;
  const MixerChannel(
      {required this.key,
      required this.name,
      required this.number,
      required this.channel,
      required this.artwork,
      this.colorChoice,
      this.icon =
          const ResolvedChannelIcon(ChannelIconSelection.followProvider()),
      required this.fader,
      required this.pan,
      required this.muted,
      required this.solo,
      required this.selected,
      required this.master,
      required this.meter,
      required this.params,
      this.eqPreview});
  String get levelLabel => '${fmtDb(fader)} dB';

  bool get showsArtwork => icon.selection.kind != ChannelIconKind.none;
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

  /// Restore is idempotent per connection session. The scoped selection is
  /// applied only after the provider advertises its banks/views, so a slow
  /// handshake cannot overwrite the saved choice with a premature fallback.
  String? _restoredSelectionToken;
  bool _restoringSelection = false;
  bool _sanitizingSelection = false;

  /// Horizontal scroll offset, retained across tab switches so returning
  /// home lands on the same mixer view.
  double scrollOffset = 0;
  bool _disposed = false;
  String _driver;
  MixerController(this.store) : _driver = store.engine.driver {
    store.addListener(_changed);
    PrefsService.instance.addListener(_layersChanged);
    _maybeRestoreSelection();
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
    // Use the identity reported by the provider. One protocol can serve
    // consoles from multiple manufacturers; the UI cannot infer a make.
    return model.isEmpty ? 'Mixer' : model;
  }

  String? get sendsOnFadersBusName =>
      store.masterProcessingTarget.startsWith('bus/') ? store.masterName : null;

  String get mixerScope => store.mixerScope;

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

  /// Active custom layer for this mixer connection, or the built-in All
  /// layer. The selection is scoped to the connection, so changing mixers,
  /// reconnecting remotely or working offline never inherits another
  /// mixer's layer.
  MixerLayer get activeLayer {
    final selection = PrefsService.instance.mixerSelection(store.mixerScope);
    if (selection != null && selection.startsWith(_layerSelectionPrefix)) {
      final id = selection.substring(_layerSelectionPrefix.length);
      for (final layer in PrefsService.instance.mixerLayers) {
        if (layer.id == id) return layer;
      }
    }
    return MixerLayer.all();
  }
  bool get isCustomLayerActive => !activeLayer.isAll;

  static const _layerSelectionPrefix = 'layer:';

  /// Current top-level picker selection: an app layer (`layer:<id>`) or the
  /// live bank/view key (`view:all`, `custom:<id>`, `ch1`, `view:view1_assign`).
  String get selectionKey => isCustomLayerActive
      ? '$_layerSelectionPrefix${activeLayer.id}'
      : store.bank;

  /// Human label for the always-visible layers bar.
  String get selectionLabel {
    final layer = activeLayer;
    if (!layer.isAll) return layer.name;
    if (store.bank == 'view:all') return 'All';
    for (final bank in store.banks) {
      if (bank.key == store.bank) return bank.label;
    }
    return 'All';
  }

  /// Custom view currently shown as a bank, if any.
  CustomMixerView? get activeCustomView {
    if (!activeLayer.isAll || !store.bank.startsWith('custom:')) return null;
    final id = store.bank.substring('custom:'.length);
    return store.customMixerViews.where((view) => view.id == id).firstOrNull;
  }

  /// Layout editing is explicit and only mutates app-owned membership:
  /// a saved layer or a custom view. Native banks and All stay read-only.
  bool get canEditLayout => !activeLayer.isAll || activeCustomView != null;

  /// Raw saved key order, including keys the current desk cannot resolve.
  /// Keeping them means editing a layer on a reconfigured desk does not
  /// silently delete entries that may return later.
  List<String> get editableChannelKeys => !activeLayer.isAll
      ? [...activeLayer.channelKeys]
      : [...?activeCustomView?.stripKeys];

  /// Live channels available to add, filtered by the same surface/access
  /// rules the mixer uses. The master is never part of a layer or view.
  List<LayerChannelOption> get layerChannels => [
        for (final strip in store.engine.strips)
          if (strip.key != store.masterProcessingTarget &&
              strip.key != 'main' &&
              store.isMixerStripVisible(strip) &&
              store.accessTargetAllowed(strip.key))
            (
              key: strip.key,
              name: strip.name.isEmpty ? strip.key : strip.name,
            ),
      ];

  List<LayerChannelOption> get addableLayerChannels {
    final present = editableChannelKeys.toSet();
    return [
      for (final option in layerChannels)
        if (!present.contains(option.key)) option,
    ];
  }

  LayerChannelOption? channelOptionFor(String key) {
    for (final option in layerChannels) {
      if (option.key == key) return option;
    }
    return null;
  }

  /// Resolve saved keys to live mixer strips in saved order. Used by the
  /// in-place layout editor so a draft can render before anything is saved.
  List<MixerChannel> channelsForKeys(Iterable<String> keys) {
    final byKey = {
      for (final strip in store.engine.strips)
        if (strip.key != 'main') strip.key: strip,
    };
    final picked = <Strip>[];
    for (final key in keys) {
      final strip = byKey[key];
      if (strip == null ||
          !store.isMixerStripVisible(strip) ||
          picked.any((candidate) => candidate.key == key)) {
        continue;
      }
      picked.add(strip);
    }
    final present = picked.map((strip) => strip.ch).toSet();
    return [
      for (final strip in picked)
        if (_linkedPair(strip.ch) == null ||
            _linkedPair(strip.ch)!.last != strip.ch ||
            !present.contains(_linkedPair(strip.ch)!.first))
          _channel(strip),
    ];
  }

  /// Re-merge a visible order with the raw saved order so keys the current
  /// desk cannot resolve keep their place during reordering.
  List<String> mergeVisibleOrder(
      List<String> saved, List<String> visibleOrder) {
    final visible = visibleOrder.toSet();
    final merged = <String>[];
    var next = 0;
    for (final key in saved) {
      if (!visible.contains(key)) {
        merged.add(key);
      } else if (next < visibleOrder.length) {
        merged.add(visibleOrder[next++]);
      }
    }
    while (next < visibleOrder.length) {
      merged.add(visibleOrder[next++]);
    }
    return merged;
  }

  /// Custom layers ignore the bank rail: their channel source is the layer's
  /// own [MixerLayer.channelKeys] order, not the bank-filtered surface
  /// strips. The All layer keeps today's bank paging behavior unchanged.
  List<Strip> get _effectiveStrips {
    final layer = activeLayer;
    if (layer.isAll) return _surfaceStrips;
    final byKey = {
      for (final s in store.engine.strips)
        if (s.key != 'main' && store.isMixerStripVisible(s)) s.key: s,
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
  int get effectivePageSize =>
      isCustomLayerActive ? activeLayer.effectiveStripsPerPage : _pageSize;

  int get pageCount => (_effectiveStrips.length / effectivePageSize).ceil();
  int get muteGroupCount => store.engine.muteGroupCount;
  String muteGroupLabel(int group) => store.engine.muteGroupLabel(group);
  bool get hasSolo => store.hasSolo;

  /// Per-strip control notifiers (passthrough to the store): strips listen
  /// to their own ch so fader/mute/pan writes rebuild one strip, never the
  /// page. See AppStore: the hot path updates these in place with no
  /// global notifyListeners().
  ValueListenable<double> faderNoteOf(int? ch, bool master) =>
      master || ch == null ? store.masterFaderNote : store.mixerFaderOf(ch);
  ValueListenable<bool> muteNoteOf(int? ch, bool master) =>
      master || ch == null ? store.masterMutedNote : store.muteOf(ch);
  ValueListenable<double> panNoteOf(int ch) => store.panNoteOf(ch);
  ValueListenable<bool> soloNoteOf(int ch) => store.soloNoteOf(ch);
  List<MixerChannel> get channels => _effectiveStrips
      .skip(_page * effectivePageSize)
      .take(effectivePageSize)
      .map(_channel)
      .toList(growable: false);

  /// Empty-state copy for an app layer or custom view with no resolvable
  /// channels.
  String get layerEmptyMessage {
    final view = activeCustomView;
    if (view != null) return '“${view.name}” has no channels to show.';
    return '“${activeLayer.name}” has no channels to show.';
  }
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
      artwork: demo ? s.ch.clamp(0, 7) : -1,
      colorChoice: store.channelColorChoice(s.ch),
      icon: _icon(s.ch),
      fader: store.mixerFaderOf(s.ch).value,
      pan: store.panOf(s.ch),
      muted: store.muteOf(s.ch).value,
      solo: store.soloOf(s.ch),
      selected: selected ?? _selectedKey == s.key,
      master: false,
      meter: store.meterOf(s.ch),
      eqPreview: store.eqPreviewFor(s.ch),
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
  ResolvedChannelIcon _icon(int channel) {
    final target = store.captureNameTarget(channel);
    return target == null
        ? const ResolvedChannelIcon(ChannelIconSelection.followProvider())
        : store.resolveChannelIcon(target);
  }

  Strip? get _masterStrip => store.engine.strips
      .where((s) => s.key == store.masterProcessingTarget)
      .firstOrNull;

  int? get masterIdentityChannel => _masterStrip?.ch;

  /// Resolve the selected destination's native colour with the same fallback
  /// used by its nameplate, including providers' explicit uncoloured choice.
  Color masterColor(ConsolePalette palette) {
    final choice = master.colorChoice;
    if (choice == null) return palette.bus;
    return choice.rgb == null ? palette.panel : Color(0xff000000 | choice.rgb!);
  }

  MixerChannel get master {
    final target = store.masterProcessingTarget;
    final strip = _masterStrip;
    final bus = target.startsWith('bus/');
    final busNumber = bus ? int.tryParse(target.substring(4)) : null;
    final mainNumber = target.startsWith('main/')
        ? int.tryParse(target.substring(5))
        : null;
    return MixerChannel(
        key: 'master',
        name: store.masterName == 'MAIN' ? 'Main' : store.masterName,
        number:
            target == 'main'
                ? 'LR'
                : (busNumber ?? mainNumber)?.toString() ?? store.masterName,
        channel: null,
        artwork: 8,
        icon: strip == null
            ? const ResolvedChannelIcon(ChannelIconSelection.followProvider())
            : _icon(strip.ch),
        colorChoice: strip != null ? store.channelColorChoice(strip.ch) : null,
        fader: store.masterFader,
        pan: 0,
        muted: store.masterMuted,
        solo: store.soloOf(-1),
        selected: _selectedKey == 'master',
        master: true,
        meter: store.masterMeter,
        eqPreview: strip == null ? null : store.eqPreviewFor(strip.ch),
        params: strip == null
            ? const {}
            : Map.unmodifiable(store.engine.params[strip.ch] ?? const {}));
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
        return strip.key == store.masterProcessingTarget
            ? master
            : _channel(strip, selected: true);
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
      _restoredSelectionToken = null;
    }
    _maybeRestoreSelection();
    _sanitizeSelection();
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
    notifyListeners();
  }

  /// Restore the saved layer/view for this exact mixer connection once the
  /// provider has advertised its capabilities. Nothing is written to the
  /// mixer: only the UI selection and local prefs change.
  void _maybeRestoreSelection() {
    if (_disposed || _restoringSelection) return;
    final engine = store.engine;
    final ready = engine.driver == 'sim'
        ? engine.connected && engine.strips.isNotEmpty
        : engine.connected && engine.capsReady && store.banks.isNotEmpty;
    if (!ready) return;
    final scope = store.mixerScope;
    final token = '${engine.session}:$scope';
    if (_restoredSelectionToken == token) return;
    _restoredSelectionToken = token;
    unawaited(_restoreSelection(scope));
  }

  Future<void> _restoreSelection(String scope) async {
    if (_disposed) return;
    _restoringSelection = true;
    try {
      final prefs = PrefsService.instance;
      final banks = store.banks;
      bool hasBank(String key) => banks.any((bank) => bank.key == key);
      final saved = prefs.mixerSelection(scope);
      String? key = saved;
      if (key != null && key.startsWith(_layerSelectionPrefix)) {
        final id = key.substring(_layerSelectionPrefix.length);
        if (!prefs.mixerLayers.any((layer) => layer.id == id)) {
          key = hasBank('view:all') ? 'view:all' : null;
        }
      } else if (key == null || !hasBank(key)) {
        key = hasBank('view:all') ? 'view:all' : banks.firstOrNull?.key;
      }
      if (key == null || _disposed) return;
      if (!key.startsWith(_layerSelectionPrefix) && store.bank != key) {
        store.setBank(key);
      } else if (key.startsWith(_layerSelectionPrefix) &&
          !hasBank(store.bank)) {
        // Keep the previous native bank alongside a layer when it is valid
        // for this mixer; otherwise start on All if the provider has it.
        final fallback = hasBank('view:all') ? 'view:all' : banks.first.key;
        store.setBank(fallback);
      }
      if (prefs.mixerSelection(scope) != key) {
        await prefs.setMixerSelection(scope, key);
      }
      if (_disposed) return;
      _page = 0;
      // Restore selection is UI-only. Do not open/focus a channel: opening a
      // strip can send a focus command to the desk, and a saved view must not
      // write mixer state or routing when it is restored.
      _selectedKey = channels.firstOrNull?.key;
      notifyListeners();
    } catch (error) {
      // A failed local write must not break the connection; keep the current
      // valid fallback and surface it only in debug logs.
      debugPrint('Could not restore mixer selection: $error');
    } finally {
      _restoringSelection = false;
    }
  }

  /// A saved view can disappear or a desk can stop advertising a bank while
  /// the app is running. Fall back to All instead of rendering another
  /// mixer's selection or an invalid bank.
  void _sanitizeSelection() {
    if (_disposed || _restoringSelection || _sanitizingSelection) return;
    final banks = store.banks;
    if (banks.isEmpty) return;
    final key = PrefsService.instance.mixerSelection(store.mixerScope);
    if (key != null && key.startsWith(_layerSelectionPrefix)) {
      final id = key.substring(_layerSelectionPrefix.length);
      if (PrefsService.instance.mixerLayers.any((layer) => layer.id == id)) {
        return;
      }
    } else if (key != null && banks.any((bank) => bank.key == key)) {
      return;
    }
    if (store.bank != 'view:all' &&
        (key != null || !banks.any((bank) => bank.key == store.bank))) {
      _sanitizingSelection = true;
      store.setBank('view:all');
      _sanitizingSelection = false;
    }
    if (key != null) {
      unawaited(_rememberSelection('view:all'));
    }
  }

  Future<void> _rememberSelection(String key) async {
    final prefs = PrefsService.instance;
    if (prefs.mixerSelection(store.mixerScope) == key) return;
    await prefs.setMixerSelection(store.mixerScope, key);
  }

  /// Layer prefs changed (possibly from Settings while the mixer is open):
  /// clamp the page to the new source and drop a selection the layer cannot
  /// resolve. Prefs notifies on every preference write, so this stays cheap
  /// and never writes prefs back (no notify loop).
  void _layersChanged() {
    if (_disposed) return;
    _sanitizeSelection();
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

  void configurePageSize(int count) {
    if (_pageSize == count) return;
    final first = _page * _pageSize;
    _pageSize = count;
    _page = first ~/ count;
    notifyListeners();
  }

  bool get canOpenMasterProcessing =>
      store.outputProcessingControls.any((control) =>
          control.target == store.masterProcessingTarget &&
          store.destinationControlIsCurrent(control));

  String get masterProcessingUnavailableReason {
    if (!store.accessFamilyAllowed(AccessFamily.masterDsp) ||
        !store.accessTargetAllowed(store.masterProcessingTarget)) {
      return 'Output processing is limited on this device.';
    }
    return 'This connection does not provide processing controls for ${store.masterName}.';
  }

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
    if (!mixerProviderPolicy(store.engine.driver).masterChannelProcessing) {
      return false;
    }
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
      final channel = ch.channel!;
      store.setMixerFader(channel, value);
      // Stereo-locked pair: drive the mate to the SAME engine value so both
      // optimistic notifiers move together and the pair never separates
      // mid-drag. Fader only - pan/mute/solo stay independent. setFader
      // guards per-channel editability, so a locked-out mate is a safe no-op.
      final pair = _linkedPair(channel);
      if (pair != null) {
        final mate = pair.first == channel ? pair.last : pair.first;
        if (mate != channel) store.setMixerFader(mate, value);
      }
    }
  }

  bool canUnity(MixerChannel ch) {
    final law = store.engine.driver == 'sim'
        ? designLaw
        : lawForDriver(store.engine.driver);
    if (law == null) return false;
    final live = ch.master
        ? store.masterFaderNote.value
        : ch.channel != null
            ? store.mixerFaderOf(ch.channel!).value
            : double.nan;
    final muted = ch.master
        ? store.masterMutedNote.value
        : ch.channel != null
            ? store.muteOf(ch.channel!).value
            : true;
    if (!live.isFinite) return false;
    return canResetLevelToUnity(posToDb(live),
        muted: muted, min: law.floorDb, max: law.topDb);
  }

  void unity(MixerChannel ch) {
    if (!canUnity(ch)) return;
    setPosition(ch, MixerFaderScale.positionForDb(0));
  }

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
        (!mixerProviderPolicy(store.engine.driver).offlineValues &&
            (!store.engine.connected ||
                !store.engine.valueKnown(index, 'gain')))) {
      return null;
    }
    return mixerProviderPolicy(store.engine.driver).requiresRoutedPreamp
        ? store.preampGainRangeFor(index)
        : parameterRange(store.paramMetadata('gain'));
  }

  bool hasPhantomOf(MixerChannel ch) =>
      ch.channel != null &&
      !ch.master &&
      store.canEditControl(ch.channel!, 'phantom') &&
      (!mixerProviderPolicy(store.engine.driver).requiresRoutedPreamp ||
          store.preampPhantomAvailableFor(ch.channel!) == true);

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
    if (!store.banks.any((bank) => bank.key == key)) return;
    if (store.bank == key && !isCustomLayerActive) {
      await _rememberSelection(key);
      return;
    }
    _page = 0;
    await _rememberSelection(key);
    store.setBank(key);
    final first = channels.firstOrNull;
    if (first != null) select(first);
    notifyListeners();
  }

  /// Switch the active layer. Unknown ids are ignored (the All fallback in
  /// [activeLayer] would otherwise silently change the view). The selection is
  /// saved for this mixer connection only. Choosing All opens the All bank.
  Future<void> setActiveLayer(String id) async {
    final prefs = PrefsService.instance;
    final known = id == MixerLayer.allLayerId ||
        prefs.mixerLayers.any((layer) => layer.id == id);
    if (!known) return;
    final key = id == MixerLayer.allLayerId
        ? 'view:all'
        : '$_layerSelectionPrefix$id';
    if (selectionKey == key) return;
    _page = 0;
    await _rememberSelection(key);
    if (id == MixerLayer.allLayerId &&
        store.banks.any((bank) => bank.key == 'view:all')) {
      store.setBank('view:all');
    }
    final first = channels.firstOrNull;
    if (first != null) select(first);
    notifyListeners();
  }

  /// Replace the ordered membership of the app layer or custom view that is
  /// currently in Edit layout mode. Never touches mixer state or routing.
  Future<void> saveEditableChannelKeys(List<String> keys) async {
    final clean = <String>[];
    final seen = <String>{};
    for (final key in keys) {
      if (key.isNotEmpty && seen.add(key)) clean.add(key);
    }
    final view = activeCustomView;
    if (view != null) {
      await _saveEditableView(CustomMixerView(
        id: view.id,
        name: view.name,
        stripKeys: List.unmodifiable(clean),
      ));
      return;
    }
    final layer = activeLayer;
    if (layer.isAll) return;
    await _saveEditableLayer(
        layer.copyWith(channelKeys: List.unmodifiable(clean)));
  }

  Future<void> addEditableChannel(String key) async {
    final keys = editableChannelKeys;
    if (keys.contains(key)) return;
    await saveEditableChannelKeys([...keys, key]);
  }

  Future<void> removeEditableChannel(String key) async {
    final keys = editableChannelKeys;
    if (!keys.contains(key)) return;
    await saveEditableChannelKeys([...keys]..remove(key));
  }

  Future<void> reorderEditableChannel(
      String key, int oldIndex, int newIndex) async {
    final visible = channels.map((channel) => channel.key).toList();
    final from = visible.indexOf(key);
    if (from < 0) return;
    final moved = visible.removeAt(from);
    final target = newIndex.clamp(0, visible.length);
    visible.insert(target, moved);
    await saveEditableChannelKeys(
        mergeVisibleOrder(editableChannelKeys, visible));
  }

  Future<void> _saveEditableLayer(MixerLayer layer) async {
    final prefs = PrefsService.instance;
    final layers = [...prefs.mixerLayers];
    final index = layers.indexWhere((candidate) => candidate.id == layer.id);
    if (index < 0) return;
    layers[index] = layer;
    await prefs.setMixerLayers(layers);
  }

  Future<void> _saveEditableView(CustomMixerView view) async {
    final views = [
      for (final existing in store.customMixerViews)
        existing.id == view.id ? view : existing,
    ];
    await store.saveCustomMixerViews(views);
  }

  /// Channel-page swipes follow the complete bank/layer, across mixer pages,
  /// without jumping to the master or a different strip family.
  void moveProcessingSelection(int delta) {
    if (delta == 0 || store.outputProcessingTarget != null) return;
    final all = _effectiveStrips;
    final family = store.stripKey.split('/').first;
    final siblings =
        all.where((s) => s.key.split('/').first == family).toList();
    final current = siblings.indexWhere((s) => s.key == store.stripKey);
    final next = current + delta.sign;
    if (current < 0 || next < 0 || next >= siblings.length) return;
    final target = siblings[next];
    _page = all.indexWhere((s) => s.key == target.key) ~/ effectivePageSize;
    select(_channel(target));
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
              ? store.mixerFaderOf(ch.channel!).value
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
