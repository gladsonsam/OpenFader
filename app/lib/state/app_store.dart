// Central store: live engine binding with optimistic-write plus confirm and coalesced sends.
// One store implements the mixer, meters, and channel contracts over constructor
// injection. The shell owns the single 80 ms ticker; screens own no timer.
// Per-strip control state rides stable notifiers updated in place, while structural
// state notifies through ChangeNotifier.
// Mapping gaps, honest not faked: channel writes use canonical desk leaves translated
// in deskLeaf; EQ and dynamics edits stay disabled until the desk value arrives;
// the bus popup picks the master bus shown without re-patching routing; binary frames
// ride the hot path while poll stays the JSON fallback.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../core/access_limits.dart';
import '../core/native_library.dart';
import '../core/routing_access.dart';
import '../settings/prefs.dart';

import '../core/channel_views.dart';
import '../core/output_processing.dart';
import '../core/output_meters.dart';
import '../core/output_presets.dart';
import '../core/output_geq.dart';
import '../core/audio_spectrum_peaks.dart';
import '../core/channel_name.dart';
import '../core/coalescer.dart';
import '../core/processor_presets.dart';
import '../settings/processor_presets.dart';
import '../core/fader_law.dart' show lawForDriver;
import '../core/store.dart' as ps;
import '../engine.dart';
import '../palette.dart';
import '../u4law.dart';
import '../screens/channel.dart';
import '../screens/meters.dart';
import '../mixer/store.dart';

part 'app_store_access.dart';

part 'app_store_library.dart';
part 'app_store_native_library.dart';
part 'app_store_appearance.dart';
part 'app_store_assignments.dart';
part 'app_store_presets.dart';
part 'app_store_input_clipboard.dart';
part 'app_store_input_processor_guard.dart';
part 'app_store_input_processor_recall.dart';
part 'app_store_session.dart';
part 'app_store_poll.dart';
part 'app_store_meters.dart';
part 'app_store_strips.dart';
part 'app_store_solo.dart';
part 'app_store_strip_values.dart';
part 'app_store_control_groups.dart';
part 'app_store_channel.dart';
part 'app_store_input.dart';
part 'app_store_destinations.dart';
part 'app_store_outputs.dart';
part 'app_store_output_presets.dart';

double _linToDb(double l) => (l <= 0 || !l.isFinite)
    ? double.negativeInfinity
    : 20 * math.log(l) / math.ln10;

// Shared composition context: module contracts declare the state and hooks each
// responsibility supplies. Only AppStore composes their concrete implementations.
abstract class _AppStoreContext extends ChangeNotifier
    implements
        MixerStore,
        MetersStore,
        ChannelStore,
        InputProcessorPresetStore,
        OutputProcessingStore,
        _AppStoreOutputsContract,
        _AppStoreOutputPresetsContract,
        _AppStoreCoreContract,
        _AppStoreLibraryContract,
        _AppStoreNativeLibraryContract,
        _AppStoreAppearanceContract,
        _AppStoreAssignmentsContract,
        _AppStorePresetsContract,
        _AppStoreSessionContract,
        _AppStorePollContract,
        _AppStoreMetersContract,
        _AppStoreStripsContract,
        _AppStoreChannelContract,
        _AppStoreDestinationsContract,
        _AppStoreAccessContract {}

abstract interface class _AppStoreCoreContract {
  EngineApi get _api;
  String get _clientId;
  set _clientId(String value);
  WriteCoalescer get _coalescer;
  EngineState get engine;
  bool get _polling;
  set _polling(bool value);
  bool get _binaryMeters;
  set _binaryMeters(bool value);
  ps.StoreMaps get _maps;
  Map<String, (int, int)>? get _bankRangesCache;
  set _bankRangesCache(Map<String, (int, int)>? value);
  List<Strip>? get _stripsCache;
  set _stripsCache(List<Strip>? value);
  List<MixerBank>? get _banksCache;
  set _banksCache(List<MixerBank>? value);
  List<MixerBusRow>? get _busRowsCache;
  set _busRowsCache(List<MixerBusRow>? value);
  List<BridgeGroup>? get _groupsCache;
  set _groupsCache(List<BridgeGroup>? value);
  List<ChannelSendVm>? get _sendsCache;
  set _sendsCache(List<ChannelSendVm>? value);
  void _invalidateDerived();
  double _nowMs();
}

// Composition and resource lifetime only. Responsibility modules own their state
// and expose explicit dependencies; the public store contracts are unchanged.
class AppStore extends _AppStoreContext
    with
        _AppStoreAccess,
        _AppStoreLibrary,
        _AppStoreNativeLibrary,
        _AppStoreAppearance,
        _AppStoreAssignments,
        _AppStorePresets,
        _AppStoreSession,
        _AppStorePoll,
        _AppStoreMeters,
        _AppStoreStrips,
        _AppStoreChannel,
        _AppStoreInput,
        _AppStoreDestinations,
        _AppStoreOutputs,
        _AppStoreOutputPresets
    implements MixerStore, MetersStore, ChannelStore, OutputProcessingStore {
  AppStore(this._api, this._clientId)
      : _coalescer = WriteCoalescer(TimerCoalescerClock()) {
    _blockedAccess = PrefsService.instance.blockedAccess;
    PrefsService.instance.addListener(_accessChanged);
  }

  @override
  final EngineApi _api;
  @override
  String _clientId;
  @override
  final WriteCoalescer _coalescer;

  @override
  final EngineState engine = EngineState();
  @override
  bool _polling = false;
  // A stale library without pollBinary latches binary off, so JSON meters keep rendering.

  @override
  bool _binaryMeters = true;
  @override
  final ps.StoreMaps _maps = ps.StoreMaps();

  // --- derived-list cache (no per-build allocations) ---
  // strips/banks/busRows/groups/sends (+_bankRanges) rebuild only after a
  // control-state mutation nulls them via [_invalidateDerived]. Cached lists
  // are unmodifiable snapshots: callers only read them (verified: no call
  // site sorts/adds/removes on these getters), so sharing one instance across
  // builds is safe and fail-fast if a future caller tries to mutate.
  @override
  Map<String, (int, int)>? _bankRangesCache;
  @override
  List<Strip>? _stripsCache;
  @override
  List<MixerBank>? _banksCache;
  @override
  List<MixerBusRow>? _busRowsCache;
  @override
  List<BridgeGroup>? _groupsCache;
  @override
  List<ChannelSendVm>? _sendsCache;

  // Drop every derived-list cache after any mutator affecting the cached getters.

  @override
  void _invalidateDerived() {
    _bankRangesCache = null;
    _stripsCache = null;
    _banksCache = null;
    _busRowsCache = null;
    _groupsCache = null;
    _sendsCache = null;
  }

  // Short UI leaves translate to canonical desk ids with HPF on band 0 and polarity for invert.

  static final RegExp _eqShortLeaf = RegExp(r'^eq(\d+)_([fg])$');

  static String deskLeaf(String leaf) {
    if (leaf == 'invert') return 'polarity';
    if (leaf == 'eq_hpf_f' || leaf == 'eq0_f') return 'hpf_freq';
    final m = _eqShortLeaf.firstMatch(leaf);
    if (m != null) return 'eq${m[1]}_${m[2] == 'f' ? 'freq' : 'gain'}';
    return leaf;
  }

  @override
  double _nowMs() => DateTime.now().microsecondsSinceEpoch / 1000.0;

  @override
  void dispose() {
    PrefsService.instance.removeListener(_accessChanged);
    _interruptDestinationEdits(
        'Connection closed; the destination setting may have changed.');
    _libraryDisposed = true;
    _interruptAllLibrary('The engine was closed.');
    _stopNativeLibrary();
    _interruptForgetSavedMixers('The engine was closed.');
    _libraryRequests.clear();
    _libraryRecalls.clear();
    _presetDisposed = true;
    _invalidatePresetOperations('The connection was closed.');
    _interruptAssignmentEdit('The connection was closed.');
    _failNameEdits('The connection was closed.');
    _coalescer.clear();
    for (final m in _meters) {
      m.dispose();
    }
    _masterMeter.dispose();
    for (final meter in _outputMeters.values) {
      meter.dispose();
    }
    for (final n in _controlGroupNotes.values) {
      n.dispose();
    }
    for (final n in _faderNotes.values) {
      n.dispose();
    }
    for (final n in _muteNotes.values) {
      n.dispose();
    }
    for (final n in _panNotes.values) {
      n.dispose();
    }
    for (final n in _soloNotes.values) {
      n.dispose();
    }
    for (final n in _sendParamNotes.values) {
      n.dispose();
    }
    _masterFaderNote.dispose();
    _masterMutedNote.dispose();
    for (final t in _taps.values) {
      t.dispose();
    }
    for (final b in _busMeters.values) {
      b.dispose();
    }
    _dynamicsMeters.dispose();
    _levelDb.dispose();
    _spectrum.dispose();
    _audioSpectrum.dispose();
    _audioSpectrumPeaks.dispose();
    for (final note in _controlReadyNotes.values) {
      note.dispose();
    }
    super.dispose();
  }
}
