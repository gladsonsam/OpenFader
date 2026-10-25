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

import '../core/channel_views.dart';
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

double _linToDb(double l) => (l <= 0 || !l.isFinite)
    ? double.negativeInfinity
    : 20 * math.log(l) / math.ln10;

// A recall owns its expectations independently of pending edits because shell poll advances deadlines.

class _PresetRecall {
  _PresetRecall(this.target, this.generation);
  final ProcessorPresetTarget target;
  final int generation;
  final stopped = Completer<String>();
  _PresetExpectation? waiting;
  String? error;

  void stop(String message) {
    error ??= message;
    if (!stopped.isCompleted) stopped.complete(error!);
    waiting?.finish(error);
  }
}

class _PresetExpectation {
  _PresetExpectation(this.field, this.value, this.deadline);
  final String field;
  final Object value;
  final double deadline;
  final done = Completer<String?>();
  bool accepted = false;
  bool echoed = false;
  bool verified = false;

  void finish(String? error) {
    if (!done.isCompleted) done.complete(error);
  }

  void check() {
    if (accepted && echoed) finish(null);
  }
}

class _LibraryRequest {
  _LibraryRequest(this.id, this.action, this.revision, this.mode, this.session,
      this.owner, this.deadline);
  final String id;
  final LocalLibraryAction action;
  final int revision;
  final LocalLibraryMode mode;
  final int session;
  final Object? owner;
  final double deadline;
  final done = Completer<LocalLibraryCompletion>();
}

class _LibraryRecall {
  _LibraryRecall(this.owner, this.session, this.deadline);
  final Object owner;
  final int session;
  final double deadline;
  final done = Completer<String?>();
}

class _LibraryQuery {
  _LibraryQuery(this.session, this.deadline);
  final int session;
  final double deadline;
  final done = Completer<LocalLibraryStatus>();
}

class _ForgetSavedMixersRequest {
  _ForgetSavedMixersRequest(this.id, this.session, this.deadline);
  final String id;
  final int session;
  final double deadline;
  final done = Completer<void>();
}

class AppStore extends ChangeNotifier
    implements MixerStore, MetersStore, ChannelStore {
  AppStore(this._api, this._clientId)
      : _coalescer = WriteCoalescer(TimerCoalescerClock());

  final EngineApi _api;
  String _clientId;
  final WriteCoalescer _coalescer;

  final EngineState engine = EngineState();
  bool _polling = false;
  // A stale library without pollBinary latches binary off, so JSON meters keep rendering.

  bool _binaryMeters = true;
  final ps.StoreMaps _maps = ps.StoreMaps();

  final Map<String, Completer<String?>> _nameEdits = {};
  final Map<String, String> _nameEditValues = {};
  final Map<String, double> _nameEditDeadlines = {};

  // Library control traffic shares the shell poll. No page timers/pollers.
  final Map<String, _LibraryRequest> _libraryRequests = {};
  final Set<_LibraryRecall> _libraryRecalls = {};
  final List<_LibraryQuery> _libraryQueries = [];
  final Map<String, _ForgetSavedMixersRequest> _forgetSavedMixersRequests = {};
  int _libraryId = 0;
  final String _libraryClientNonce =
      '${DateTime.now().microsecondsSinceEpoch}-${identityHashCode(Object())}';
  int _libraryGeneration = 0;
  bool _libraryResetting = false;
  bool _libraryDisposed = false;
  int get libraryGeneration => _libraryGeneration;
  bool get libraryResetting => _libraryResetting;
  bool get libraryPending =>
      _libraryRequests.isNotEmpty || _libraryRecalls.isNotEmpty;
  bool get libraryActive =>
      _libraryRequests.values.any((request) => !request.done.isCompleted) ||
      _libraryRecalls.any((request) => !request.done.isCompleted);
  bool get libraryWritable =>
      !_libraryDisposed &&
      !_libraryResetting &&
      !libraryPending &&
      engine.localLibrary?.status == LocalLibraryHealth.ready;

  Future<LocalLibraryStatus> queryLocalLibrary() {
    if (_libraryDisposed) {
      return Future.error(StateError('The engine was closed.'));
    }
    final query = _LibraryQuery(engine.session, _nowMs() + 10000);
    _libraryQueries.add(query);
    unawaited(
        Future<void>.sync(() => _api.cmd(_clientId, Cmds.localLibraryStatus()))
            .then((_) {
      _checkLibraryQuery(query, _nowMs());
    }, onError: (Object error) {
      _checkLibraryQuery(query, _nowMs());
      _libraryQueries.remove(query);
      if (!query.done.isCompleted) {
        query.done
            .completeError(StateError('Library status unavailable: $error'));
      }
    }));
    return query.done.future;
  }

  void refreshLocalLibrary() {
    unawaited(queryLocalLibrary().then((_) {}, onError: (Object _) {}));
  }

  Future<int> localLibraryStorageBytes() async {
    final status = await queryLocalLibrary();
    if (status.mode == LocalLibraryMode.memory) return 0;
    final bytes = status.bytes;
    if (bytes == null) {
      throw StateError('Local library storage size is unavailable.');
    }
    return bytes;
  }

  void _interruptLibrary(_LibraryRequest request, String message) {
    if (!request.done.isCompleted) {
      request.done.complete(LocalLibraryCompletion.interrupted(message));
    }
  }

  void cancelLocalLibraryOperations(Object owner, String message) {
    for (final request in _libraryRequests.values) {
      if (identical(request.owner, owner)) _interruptLibrary(request, message);
    }
    for (final request in _libraryRecalls) {
      if (identical(request.owner, owner) && !request.done.isCompleted) {
        request.done.complete(message);
      }
    }
    // Retain dispatched requests until their explicit result. A reset cannot
    // overtake an unresolved write merely because its page stopped waiting.
  }

  Future<String?> requestLocalLibraryRecall(String command, Object owner) {
    if (!libraryWritable || !engine.connected) {
      return Future.value(
          'Recall was not dispatched: library unavailable or busy.');
    }
    final request = _LibraryRecall(owner, engine.session, _nowMs() + 15000);
    _libraryRecalls.add(request);
    notifyListeners();
    unawaited(Future<void>.sync(() => _api.cmd(_clientId, command)).then((_) {
      _checkLibraryRecall(request, _nowMs());
      _libraryRecalls.remove(request);
      if (!request.done.isCompleted) request.done.complete(null);
      if (!_libraryDisposed) notifyListeners();
    }, onError: (Object error) {
      _checkLibraryRecall(request, _nowMs());
      // Transport failure cannot prove the legacy recall stopped. Keep the
      // reservation so reset cannot race a late current-marker update.
      if (!request.done.isCompleted) {
        request.done.complete('Recall transport failed: $error.');
      }
      if (!_libraryDisposed) notifyListeners();
    }));
    return request.done.future;
  }

  void _interruptAllLibrary(String message) {
    for (final request in _libraryRecalls) {
      if (!request.done.isCompleted) request.done.complete(message);
    }
    for (final request in _libraryRequests.values) {
      _interruptLibrary(request, message);
    }
    for (final query in _libraryQueries) {
      if (!query.done.isCompleted) {
        query.done.completeError(StateError(message));
      }
    }
    _libraryQueries.clear();
  }

  Future<LocalLibraryCompletion> mutateLocalLibrary(LocalLibraryAction action,
          {required int expectedRevision, Object? owner}) =>
      _submitLibraryMutation(action,
          expectedRevision: expectedRevision, owner: owner);

  Future<LocalLibraryCompletion> _submitLibraryMutation(
      LocalLibraryAction action,
      {required int expectedRevision,
      Object? owner,
      bool resetting = false}) {
    final status = engine.localLibrary;
    final reset = action.operation == 'reset';
    if (_libraryDisposed ||
        (_libraryResetting && !resetting) ||
        libraryPending ||
        status == null ||
        status.revision != expectedRevision ||
        (!reset && status.status != LocalLibraryHealth.ready)) {
      // No dispatch occurred, so this is a definite local refusal.
      return Future.value(const LocalLibraryCompletion.notDispatched(
          'Library action was not dispatched: refresh the library and review its current state.'));
    }
    final id = 'library-$_libraryClientNonce-${++_libraryId}';
    final request = _LibraryRequest(id, action, expectedRevision, status.mode,
        engine.session, owner, _nowMs() + 15000);
    _libraryRequests[id] = request;
    notifyListeners();
    // Invoke before returning: all mutations enter the engine's serialized
    // commit path in submission order, before reset can inspect the queue.
    unawaited(Future<void>.sync(() => _api.cmd(
            _clientId, Cmds.localLibraryMutate(id, expectedRevision, action)))
        .then((_) {
      _checkLibraryRequest(request, _nowMs());
    }, onError: (Object error) {
      _checkLibraryRequest(request, _nowMs());
      if (identical(_libraryRequests[id], request)) {
        _interruptLibrary(request, 'Library transport failed: $error.');
        if (!_libraryDisposed) notifyListeners();
      }
    }));
    return request.done.future;
  }

  void _receiveLibraryResult(Map<String, dynamic> frame) {
    final id = frame['request_id'];
    final request = id is String ? _libraryRequests[id] : null;
    // Never correlate by list, error, or operation alone.
    if (request == null) return;
    // The awaited poll may have crossed the deadline since its initial sweep.
    // A late authoritative result can release the reservation, never revive
    // an interrupted caller (including one from an earlier session).
    _checkLibraryRequest(request, _nowMs());
    final result = LocalLibraryResult.fromJson(frame);
    if (result == null ||
        result.operation != request.action.operation ||
        result.mode != request.mode ||
        result.revision < request.revision ||
        (result.status == LocalLibraryResultStatus.committed &&
            result.revision <= request.revision)) {
      _interruptLibrary(request,
          'The engine returned an invalid or mismatched library result.');
      return;
    }
    _checkLibraryRequest(request, _nowMs());
    _libraryRequests.remove(id);
    if (!request.done.isCompleted) {
      request.done.complete(LocalLibraryCompletion.result(result));
    }
    engine.invalidateLocalLibraryStatus(result.revision);
    if (engine.localLibrary == null) refreshLocalLibrary();
  }

  void _checkLibraryRecall(_LibraryRecall request, double now) {
    if (!request.done.isCompleted &&
        (_libraryDisposed ||
            request.session != engine.session ||
            now >= request.deadline)) {
      request.done
          .complete('Recall result unconfirmed; the mixer may have changed.');
    }
  }

  void _checkLibraryRequest(_LibraryRequest request, double now) {
    if (_libraryDisposed || request.session != engine.session) {
      _interruptLibrary(
          request, 'Connection changed while awaiting the library result.');
    } else if (now >= request.deadline) {
      _interruptLibrary(
          request, 'No correlated library result arrived before the deadline.');
    }
  }

  void _checkLibraryQuery(_LibraryQuery query, double now) {
    final message = _libraryDisposed || query.session != engine.session
        ? 'Connection changed while awaiting library status.'
        : now >= query.deadline
            ? 'Library status timed out.'
            : null;
    if (message == null) return;
    _libraryQueries.remove(query);
    if (!query.done.isCompleted) query.done.completeError(StateError(message));
  }

  void _pollLibrary(double now) {
    for (final request in _libraryRecalls) {
      _checkLibraryRecall(request, now);
    }
    for (final request in _libraryRequests.values) {
      _checkLibraryRequest(request, now);
    }
    for (final query in _libraryQueries.toList()) {
      _checkLibraryQuery(query, now);
    }
    for (final request in _forgetSavedMixersRequests.values.toList()) {
      _checkForgetSavedMixers(request, now);
    }
  }

  /// Clears only engine-owned remembered addresses. Submission is not proof:
  /// settings may clear Flutter recents only after the exact local ack arrives.
  Future<void> forgetSavedMixers() {
    if (_libraryDisposed) {
      return Future.error(StateError('The engine was closed.'));
    }
    final id = 'forget-saved-mixers-$_libraryClientNonce-${++_libraryId}';
    final request =
        _ForgetSavedMixersRequest(id, engine.session, _nowMs() + 15000);
    _forgetSavedMixersRequests[id] = request;
    unawaited(
        Future<void>.sync(() => _api.cmd(_clientId, Cmds.forgetSavedMixers(id)))
            .then((_) {
      _checkForgetSavedMixers(request, _nowMs());
    }, onError: (Object error) {
      _checkForgetSavedMixers(request, _nowMs());
      _failForgetSavedMixers(request,
          'Remembered-address clearing transport failed: $error. Addresses may have been cleared; the result is unconfirmed.');
    }));
    return request.done.future;
  }

  void _failForgetSavedMixers(
      _ForgetSavedMixersRequest request, String message) {
    _forgetSavedMixersRequests.remove(request.id);
    if (!request.done.isCompleted) {
      request.done.completeError(StateError(message));
    }
  }

  void _interruptForgetSavedMixers(String message) {
    for (final request in _forgetSavedMixersRequests.values.toList()) {
      _failForgetSavedMixers(request,
          '$message Remembered addresses may have been cleared; the result is unconfirmed.');
    }
  }

  void _checkForgetSavedMixers(_ForgetSavedMixersRequest request, double now) {
    if (_libraryDisposed || request.session != engine.session) {
      _failForgetSavedMixers(request,
          'Connection changed while clearing remembered addresses. Addresses may have been cleared; the result is unconfirmed.');
    } else if (now >= request.deadline) {
      _failForgetSavedMixers(request,
          'Remembered-address clearing timed out. Addresses may have been cleared; the result is unconfirmed.');
    }
  }

  void _receiveForgetSavedMixersResult(Map<String, dynamic> frame) {
    final id = frame['request_id'];
    final request = id is String ? _forgetSavedMixersRequests[id] : null;
    if (request == null) return;
    _checkForgetSavedMixers(request, _nowMs());
    if (request.done.isCompleted) return;
    final results = frame['results'];
    final error = frame['error'];
    if (frame['operation'] != 'forget_saved_mixers' ||
        results is! List ||
        results.isNotEmpty ||
        !frame.containsKey('error') ||
        (error != null && error is! String)) {
      _failForgetSavedMixers(request,
          'The engine returned an invalid remembered-address clearing result. Addresses may have been cleared; the result is unconfirmed.');
      return;
    }
    _checkForgetSavedMixers(request, _nowMs());
    if (request.done.isCompleted) return;
    if (error != null) {
      _failForgetSavedMixers(request, error as String);
      return;
    }
    _forgetSavedMixersRequests.remove(id);
    request.done.complete();
  }

  /// Blocks new writes and invalidates dialogs for the entire settings reset.
  /// Engine reset is first; no Flutter-owned data is removed on its failure.
  Future<void> resetLocalLibraryThen(
      Future<void> Function() clearAppData) async {
    if (_libraryResetting || _libraryDisposed) {
      throw StateError('A reset is unavailable or already running.');
    }
    _libraryResetting = true;
    final generation = ++_libraryGeneration;
    notifyListeners();
    var libraryCleared = false;
    try {
      final pending = _libraryRequests.values.toList();
      for (final request in pending) {
        final completion = await request.done.future;
        if (completion.result == null ||
            _libraryRequests.containsKey(request.id)) {
          throw StateError(
              'An earlier library write is still unconfirmed. Reset stopped before deleting app data.');
        }
      }
      for (final request in _libraryRecalls.toList()) {
        final error = await request.done.future;
        if (error != null || _libraryRecalls.contains(request)) {
          throw StateError(
              'An earlier recall is still unconfirmed. Reset stopped.');
        }
      }
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError('Engine attachment changed during reset.');
      }
      final status = await queryLocalLibrary();
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError('Engine attachment changed during reset.');
      }
      final completion = await _submitLibraryMutation(
          const LocalLibraryAction.reset(),
          expectedRevision: status.revision,
          resetting: true);
      if (!completion.committed) throw StateError(completion.message);
      libraryCleared = true;
      if (_libraryDisposed || generation != _libraryGeneration) {
        throw StateError(
            'Engine attachment changed after clearing the library.');
      }
      await clearAppData();
    } catch (error) {
      throw StateError(libraryCleared
          ? 'Partial reset: local scenes and shows were cleared, but remaining app data may be only partly removed: $error'
          : 'Reset stopped before clearing other app data: $error');
    } finally {
      _libraryResetting = false;
      _libraryGeneration++;
      if (!_libraryDisposed) notifyListeners();
    }
  }

  @override
  ChannelNameTarget? captureNameTarget([int? channel]) {
    final ch = channel ?? _channel;
    for (final strip in engine.strips) {
      if (strip.ch != ch || !engine.valueKnown(ch, 'name')) continue;
      final target =
          ChannelNameTarget(engine.session, ch, strip.key, strip.name);
      return nameEditError(target) == null ? target : null;
    }
    return null;
  }

  @override
  String? nameEditError(ChannelNameTarget target) {
    if (!engine.connected || engine.session != target.session) {
      return 'Connection changed or lost. Reopen the name editor.';
    }
    if (!engine.capsReady || !engine.supportedParams.contains('name')) {
      return 'Channel naming is unavailable on this connection.';
    }
    if (!engine.valueKnown(target.channel, 'name') ||
        !engine.strips
            .any((s) => s.ch == target.channel && s.key == target.key)) {
      return 'The channel name is not available. Reopen the name editor.';
    }
    return null;
  }

  void _finishNameEdit(String key, String? error) {
    final edit = _nameEdits.remove(key);
    if (edit == null) return;
    _nameEditValues.remove(key);
    _nameEditDeadlines.remove(key);
    _maps.pending.remove(key);
    edit.complete(error);
  }

  void _failNameEdits(String message) {
    for (final key in _nameEdits.keys.toList()) {
      _finishNameEdit(key, message);
    }
  }

  @override
  Future<String?> renameChannel(ChannelNameTarget target, String name) async {
    final unavailable = nameEditError(target);
    if (unavailable != null) return unavailable;
    // Reject unsafe text, never silently normalize a user's channel identity.
    if (name.trim().isEmpty ||
        name.runes.any((r) =>
            r < 32 || (r >= 127 && r <= 159) || (r >= 0xd800 && r <= 0xdfff))) {
      return 'Enter a name without control characters or invalid Unicode.';
    }
    if (engine.driver == 'sim' && name.runes.length > 16) {
      return 'Use 16 characters or fewer for an offline channel name.';
    }
    // This is the current Rust transport boundary, not a claimed
    // desk/manufacturer mixer software text limit. Reject rather than send truncated UTF-8.
    if (engine.driver == 'dl32s' &&
        (utf8.encode(name).length > 63 || name != name.trim())) {
      return name != name.trim()
          ? 'Remove spaces at the beginning or end of the name.'
          : 'This name is too long for this mixer. Try a shorter name.';
    }
    final current = engine.strips
        .firstWhere((s) => s.ch == target.channel && s.key == target.key)
        .name;
    if (name == current) return 'The channel already has this name.';
    final key = ps.pendingKey(target.channel, 'name');
    if (_maps.pending.containsKey(key) || _nameEdits.containsKey(key)) {
      return 'A name change is already pending.';
    }
    final done = Completer<String?>();
    _nameEdits[key] = done;
    _nameEditValues[key] = name;
    _nameEditDeadlines[key] = _nowMs() + ps.confirmTimeoutMs;
    ps.noteOptimistic(_maps, target.channel, 'name', name, _nowMs());
    unawaited(Future<void>.sync(
            () => _api.cmd(_clientId, Cmds.param(target.channel, 'name', name)))
        .then((_) {}, onError: (Object e) {
      // A late transport failure must not cancel a newer edit of this channel.
      if (identical(_nameEdits[key], done)) {
        _finishNameEdit(key, 'Could not rename channel: $e');
      }
    }));
    return done.future;
  }

  // --- local user processor presets ---
  final ProcessorPresetLibrary _processorPresets = ProcessorPresetLibrary();
  // Display/optimistic state is intentionally separate from values that can
  // substantiate a stored preset or complete a recall. Legacy events without
  // provenance are unknown, never independent readback proof.
  final Map<int, Set<String>> _presetVerifiedFields = {};

  bool _recordParameterAuthority(
      Map<String, dynamic> event, int channel, String field) {
    final authority = event['authority'];
    final verified = event['type'] == 'param'
        ? authority == 'verified'
        : authority is Map && authority[field] == 'verified';
    final fields = _presetVerifiedFields.putIfAbsent(channel, () => {});
    if (verified) {
      fields.add(field);
      if (field == 'name') {
        final key = ps.pendingKey(channel, field);
        final value = event['type'] == 'param'
            ? event['value']
            : (event['values'] as Map?)?[field];
        if (_nameEditValues[key] == value &&
            _nowMs() < (_nameEditDeadlines[key] ?? double.negativeInfinity)) {
          _finishNameEdit(key, null);
        }
      }
    } else {
      fields.remove(field);
    }
    return verified;
  }

  final Map<(int, ProcessorPresetKind), _PresetRecall> _presetRecalls = {};
  final Expando<int> _presetTargets = Expando<int>();
  int _presetGeneration = 0;
  int _presetId = 0;
  bool _presetDisposed = false;

  Map<String, dynamic>? _presetFamily(int channel) {
    for (final family in engine.topology) {
      final first = family['first'];
      final count = family['count'];
      if (first is num &&
          count is num &&
          channel >= first &&
          channel < first + count) {
        return family;
      }
    }
    return null;
  }

  bool _presetFieldApplies(ProcessorPresetTarget target, String field) {
    final scopes = engine.parameterMetadata[field]?['scopes'];
    final family = _presetFamily(target.channel);
    final familyScopes = family?['params_scope'];
    final kinds = familyScopes is List ? familyScopes : [family?['kind']];
    return scopes is List && scopes.any(kinds.contains);
  }

  String? _presetCatalogueError(ProcessorPresetTarget target) {
    final family = _presetFamily(target.channel);
    final familyScopes = family?['params_scope'];
    final kinds = familyScopes is List ? familyScopes : [family?['kind']];
    if (kinds.isEmpty || kinds.any((kind) => kind is! String || kind.isEmpty)) {
      return 'The mixer has not provided this channel’s processor information.';
    }
    // Missing scope metadata must not silently remove a field from a save.
    // Properly scoped rows for other families (for example bands 5/6 on an
    // input) are excluded by _presetFields instead.
    for (final field in engine.supportedParams) {
      if (!processorPresetField(target.kind, field)) continue;
      final scopes = engine.parameterMetadata[field]?['scopes'];
      if (scopes is! List ||
          scopes.isEmpty ||
          scopes.any((scope) => scope is! String || scope.isEmpty)) {
        return 'The mixer has not provided complete ${target.kind.label} information.';
      }
    }
    return null;
  }

  List<String> _presetFields(ProcessorPresetTarget target) => [
        for (final field in engine.supportedParams)
          if (processorPresetField(target.kind, field) &&
              _presetFieldApplies(target, field))
            field,
      ];

  @override
  ProcessorPresetTarget? capturePresetTarget(ProcessorPresetKind kind) {
    final family = _presetFamily(_channel)?['key'];
    if (family is! String || family.isEmpty) return null;
    for (final strip in engine.strips) {
      if (strip.ch != _channel) continue;
      final target = ProcessorPresetTarget(
          session: engine.session,
          channel: strip.ch,
          key: strip.key,
          driver: engine.driver,
          family: family,
          name: strip.name,
          kind: kind);
      _presetTargets[target] = _presetGeneration;
      return presetTargetError(target) == null ? target : null;
    }
    return null;
  }

  @override
  String? presetTargetError(ProcessorPresetTarget target) {
    if (_presetDisposed ||
        _presetTargets[target] != _presetGeneration ||
        !engine.connected ||
        engine.session != target.session ||
        engine.driver != target.driver) {
      return 'Connection changed or lost. Reopen presets.';
    }
    if (!engine.capsReady ||
        _presetFamily(target.channel)?['key'] != target.family ||
        !engine.strips
            .any((s) => s.ch == target.channel && s.key == target.key)) {
      return 'The channel changed or is unavailable. Reopen presets.';
    }
    final catalogueError = _presetCatalogueError(target);
    if (catalogueError != null) return catalogueError;
    if (_presetFields(target).isEmpty) {
      return '${target.kind.label} presets are unavailable for this channel.';
    }
    return null;
  }

  String? _presetReadyError(ProcessorPresetTarget target) {
    final unavailable = presetTargetError(target);
    if (unavailable != null) return unavailable;
    if (_presetRecalls.containsKey((target.channel, target.kind))) {
      return 'Wait for the current ${target.kind.label} recall to finish.';
    }
    // Pending processor edits include bypass flags, even though they are never
    // stored. Reject before starting; held coalescer entries remain pending
    // until an echo or timeout and cannot race this recall.
    for (final key in _maps.pending.keys) {
      final edit = ps.splitKey(key);
      if (edit != null &&
          edit.ch == target.channel &&
          _presetProcessorField(target.kind, edit.id)) {
        return 'Wait for the current ${target.kind.label} changes to be confirmed.';
      }
    }
    return null;
  }

  bool _presetProcessorField(ProcessorPresetKind kind, String field) =>
      processorPresetField(kind, field) ||
      switch (kind) {
        ProcessorPresetKind.eq => field == 'eq_on' ||
            field == 'hpf_on' ||
            RegExp(r'^eq[1-6]_on$').hasMatch(field),
        ProcessorPresetKind.gate =>
          field == 'gate_on' || field == 'gate_filter_on',
        ProcessorPresetKind.compressor =>
          field == 'comp_on' || field == 'comp_filter_on',
      };

  bool _presetEditingBlocked(int channel, String field) =>
      _presetRecalls.values.any((recall) =>
          recall.target.channel == channel &&
          _presetProcessorField(recall.target.kind, field));

  String _presetSettingLabel(String field) {
    final band = RegExp(r'^eq([1-6])_(freq|gain|q|type)$').firstMatch(field);
    if (band != null) {
      final parameter = switch (band[2]) {
        'freq' => 'frequency',
        'gain' => 'gain',
        'q' => 'Q',
        _ => 'shape',
      };
      return 'EQ band ${band[1]} $parameter';
    }
    if (field == 'eq_mode') return 'EQ mode';
    if (field == 'hpf_freq') return 'high-pass frequency';
    if (field == 'hpf_slope') return 'high-pass slope';
    final dynamics = RegExp(r'^(gate|comp)_(.+)$').firstMatch(field);
    if (dynamics != null) {
      final processor = dynamics[1] == 'gate' ? 'gate' : 'compressor';
      final setting = switch (dynamics[2]) {
        'thr' => 'threshold',
        'att' => 'attack',
        'rel' => 'release',
        'det' => 'detector',
        'env' => 'envelope',
        'key_src' => 'key source',
        'filter_type' => 'filter shape',
        'filter_freq' => 'filter frequency',
        'auto' => 'automatic timing',
        _ => dynamics[2]!,
      };
      return '$processor $setting';
    }
    final label = engine.parameterMetadata[field]?['label'];
    return label is String && label.isNotEmpty
        ? label
        : field.replaceAll('_', ' ');
  }

  String? _presetValueError(
      ProcessorPresetTarget target, String field, Object? value,
      {bool requireVerified = false}) {
    if (!processorPresetField(target.kind, field) ||
        !engine.supportedParams.contains(field) ||
        !_presetFieldApplies(target, field)) {
      return 'The saved setting ${_presetSettingLabel(field)} is unavailable on this channel.';
    }
    if (!engine.valueKnown(target.channel, field) ||
        !ps.confirmedKnown(_maps, target.channel, field) ||
        (requireVerified &&
            !(_presetVerifiedFields[target.channel]?.contains(field) ??
                false))) {
      return 'Waiting for the mixer to provide ${_presetSettingLabel(field)}. Try again when it is ready.';
    }
    final metadata = engine.parameterMetadata[field];
    final kind = metadata?['kind'];
    if (kind == 'bool') {
      return value is bool
          ? null
          : 'The saved setting ${_presetSettingLabel(field)} has an invalid value.';
    }
    if ((kind != 'float' && kind != 'enum') ||
        value is! num ||
        !value.isFinite) {
      return 'The saved setting ${_presetSettingLabel(field)} has an invalid type.';
    }
    final min = metadata?['min'];
    final max = metadata?['max'];
    if (min is! num ||
        max is! num ||
        !min.isFinite ||
        !max.isFinite ||
        min > max ||
        value < min ||
        value > max ||
        ((kind == 'enum' ||
                field.endsWith('_type') ||
                field.endsWith('_mode')) &&
            value != value.roundToDouble())) {
      return 'The saved setting ${_presetSettingLabel(field)} is outside the mixer’s supported range.';
    }
    // DL32S exposes narrower raw binary domains than the canonical catalogue.
    // Preserve integers without assuming modern/vintage or soft/hard labels.
    if (target.driver == 'dl32s' &&
        const {'eq_mode', 'gate_mode', 'comp_mode', 'comp_knee'}
            .contains(field) &&
        value != 0 &&
        value != 1) {
      return 'The saved setting ${_presetSettingLabel(field)} must be 0 or 1 for this mixer.';
    }
    return null;
  }

  bool _presetCompatible(
          ProcessorPresetTarget target, ProcessorPreset preset) =>
      preset.driver == target.driver &&
      preset.family == target.family &&
      preset.kind == target.kind;

  @override
  Future<List<ProcessorPreset>> loadProcessorPresets(
      ProcessorPresetTarget target) async {
    final error = presetTargetError(target);
    if (error != null) throw StateError(error);
    final presets = await _processorPresets.load();
    final changed = presetTargetError(target);
    if (changed != null) throw StateError(changed);
    return List<ProcessorPreset>.unmodifiable(
        presets.where((preset) => _presetCompatible(target, preset)));
  }

  @override
  Future<String?> saveProcessorPreset(
      ProcessorPresetTarget target, String name) async {
    final error =
        _presetReadyError(target) ?? validateProcessorPresetName(name);
    if (error != null) return error;
    // Copy synchronously before the first await: neither I/O nor channel
    // navigation can substitute optimistic values or retarget this snapshot.
    final values = <String, Object>{};
    for (final field in _presetFields(target)) {
      final value = _maps.confirmed[target.channel]?[field];
      final invalid =
          _presetValueError(target, field, value, requireVerified: true);
      if (invalid != null) return invalid;
      values[field] = value!;
    }
    final now = DateTime.now();
    final preset = ProcessorPreset(
        id: '${now.microsecondsSinceEpoch}-${_presetId++}',
        name: name.trim(),
        driver: target.driver,
        family: target.family,
        kind: target.kind,
        values: values,
        createdAt: now,
        updatedAt: now);
    try {
      await _processorPresets.save(preset);
      final changed = presetTargetError(target);
      if (changed != null) return 'Preset saved on this device. $changed';
      notifyListeners();
      return null;
    } catch (error) {
      return 'Could not save preset: $error';
    }
  }

  Future<ProcessorPreset> _findProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    final presets = await loadProcessorPresets(target);
    for (final preset in presets) {
      if (preset.id == id) return preset;
    }
    throw StateError('This preset is no longer in the library.');
  }

  @override
  Future<String?> renameProcessorPreset(
      ProcessorPresetTarget target, String id, String name) async {
    try {
      await _findProcessorPreset(target, id);
      final error =
          presetTargetError(target) ?? validateProcessorPresetName(name);
      if (error != null) return error;
      await _processorPresets.rename(id, name);
      if (!_presetDisposed) notifyListeners();
      return null;
    } catch (error) {
      return 'Could not rename preset: $error';
    }
  }

  @override
  Future<String?> deleteProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    try {
      await _findProcessorPreset(target, id);
      final error = presetTargetError(target);
      if (error != null) return error;
      await _processorPresets.delete(id);
      if (!_presetDisposed) notifyListeners();
      return null;
    } catch (error) {
      return 'Could not delete preset: $error';
    }
  }

  void _stopPresetRecalls(String message) {
    for (final recall in _presetRecalls.values) {
      recall.stop(message);
    }
  }

  void _invalidatePresetOperations(String message) {
    _presetGeneration++;
    _stopPresetRecalls(message);
  }

  Future<void> clearProcessorPresets() async {
    _invalidatePresetOperations(
        'The preset library was cleared. Reopen presets.');
    await _processorPresets.clear();
    if (!_presetDisposed) notifyListeners();
  }

  Future<int> processorPresetStorageBytes() async {
    final prefs = await SharedPreferences.getInstance();
    return utf8
        .encode(prefs.getString(kProcessorPresetStorageKey) ?? '')
        .length;
  }

  String? _presetRecallError(_PresetRecall recall) =>
      recall.error ??
      (recall.generation != _presetGeneration
          ? 'The preset operation was cancelled.'
          : presetTargetError(recall.target));

  void _pollPresetRecalls(double now) {
    for (final recall in _presetRecalls.values) {
      final error = _presetRecallError(recall);
      if (error != null) {
        recall.stop(error);
      } else if (recall.waiting case final waiting?) {
        if (!waiting.done.isCompleted && now >= waiting.deadline) {
          recall.stop(
              'The mixer did not confirm ${_presetSettingLabel(waiting.field)}. '
              'That setting may have changed; its result is uncertain.');
        }
      }
    }
  }

  void _confirmPresetValue(int channel, String field, Object value,
      {required bool verified}) {
    // A poll can start before expiry and return after it. Enforce the same
    // deadline at receipt, before any matching frame can complete the wait.
    _pollPresetRecalls(_nowMs());
    for (final recall in _presetRecalls.values) {
      final waiting = recall.waiting;
      if (recall.error != null ||
          recall.target.channel != channel ||
          waiting == null ||
          waiting.field != field ||
          !ps.valuesEqual(value, waiting.value)) {
        continue;
      }
      final error = _presetRecallError(recall);
      if (error != null) {
        recall.stop(error);
      } else {
        waiting.echoed = true;
        waiting.verified = verified;
        waiting.check();
      }
    }
  }

  int _presetFieldOrder(String field) {
    if (field.endsWith('_mode') || field == 'comp_auto') return 0;
    if (field.endsWith('_type') || field == 'hpf_slope') return 1;
    if (field.endsWith('_q')) return 3;
    return 2;
  }

  @override
  Future<ProcessorPresetResult> recallProcessorPreset(
      ProcessorPresetTarget target, String id) async {
    final unavailable = _presetReadyError(target);
    if (unavailable != null) {
      return ProcessorPresetResult(error: unavailable, confirmed: 0, total: 0);
    }
    final recall = _PresetRecall(target, _presetGeneration);
    final slot = (target.channel, target.kind);
    _presetRecalls[slot] = recall;
    for (final field in engine.supportedParams) {
      if (_presetProcessorField(target.kind, field)) {
        _coalescer.cancel('param:${target.channel}:$field');
      }
    }
    notifyListeners();
    var confirmed = 0;
    var acceptedUnverified = 0;
    var total = 0;
    var attempted = 0;
    final skipped = <String>[];
    try {
      final preset = await Future.any<ProcessorPreset>([
        _findProcessorPreset(target, id),
        recall.stopped.future.then((error) => throw StateError(error)),
      ]);
      total = preset.values.length;
      var error = _presetRecallError(recall);
      if (error != null) throw StateError(error);
      final fields = _presetFields(target);
      if (fields.any((field) => !preset.values.containsKey(field))) {
        throw StateError(
            'This preset does not contain every setting required by this processor.');
      }
      // Validate EVERY saved field before issuing a single command. Saved
      // values are never silently clamped or dropped when capabilities differ.
      for (final entry in preset.values.entries) {
        error = _presetValueError(target, entry.key, entry.value);
        if (error != null) throw StateError(error);
      }
      for (final key in _maps.pending.keys) {
        final edit = ps.splitKey(key);
        if (edit != null &&
            edit.ch == target.channel &&
            _presetProcessorField(target.kind, edit.id)) {
          throw StateError(
              'Wait for the current processor changes to be confirmed.');
        }
      }
      final ordered = preset.values.keys.toList()
        ..sort((a, b) {
          final order = _presetFieldOrder(a).compareTo(_presetFieldOrder(b));
          return order == 0 ? a.compareTo(b) : order;
        });
      for (final field in ordered) {
        error = _presetRecallError(recall);
        if (error != null) throw StateError(error);
        error = _presetValueError(target, field, preset.values[field]);
        if (error != null) throw StateError(error);
        final band = RegExp(r'^eq([1-4])_q$').firstMatch(field);
        if (target.driver == 'dl32s' &&
            band != null &&
            preset.values['eq${band[1]}_type'] != 2) {
          // DL32S Q is inactive outside bell mode. Keep the saved Q in the
          // library and explicitly report it as not recalled.
          skipped.add(field);
          continue;
        }
        // Cancel any held callback defensively; readiness already rejected
        // unconfirmed edits before taking this processor's write lock.
        _coalescer.cancel('param:${target.channel}:$field');
        final value = preset.values[field]!;
        final waiting =
            _PresetExpectation(field, value, _nowMs() + ps.confirmTimeoutMs);
        recall.waiting = waiting;
        attempted++;
        ps.noteOptimistic(_maps, target.channel, field, value, _nowMs());
        // Register expectation before dispatch, including synchronous throws.
        unawaited(Future<void>.sync(() =>
                _api.cmd(_clientId, Cmds.param(target.channel, field, value)))
            .then((_) {
          // Submission may finish after the deadline even when the matching
          // frame arrived earlier. Neither half can complete a late recall.
          _pollPresetRecalls(_nowMs());
          if (!identical(recall.waiting, waiting) || recall.error != null) {
            return;
          }
          final changed = _presetRecallError(recall);
          if (changed != null) {
            recall.stop(changed);
          } else {
            waiting.accepted = true;
            waiting.check();
          }
        }, onError: (Object error) {
          if (identical(recall.waiting, waiting)) {
            recall.stop(
                'Could not send ${_presetSettingLabel(field)}: $error. Its result is uncertain.');
          }
        }));
        error = await waiting.done.future;
        if (error != null) throw StateError(error);
        if (waiting.verified) {
          confirmed++;
        } else {
          acceptedUnverified++;
        }
        recall.waiting = null;
        error = _presetRecallError(recall);
        if (error != null) throw StateError(error);
      }
      return ProcessorPresetResult(
          error: acceptedUnverified == 0 && skipped.isEmpty
              ? null
              : [
                  if (acceptedUnverified > 0)
                    '$acceptedUnverified settings were accepted, but the mixer did not verify their values.',
                  if (skipped.isNotEmpty)
                    'Inactive Q settings were kept unchanged: ${skipped.map(_presetSettingLabel).join(', ')}. '
                        'Their saved values remain in the preset.',
                ].join(' '),
          confirmed: confirmed,
          total: total);
    } catch (error) {
      final notSent = total - attempted;
      return ProcessorPresetResult(
          error: '$error '
              '${acceptedUnverified > 0 ? '$acceptedUnverified settings were accepted without value verification. ' : ''}'
              '${notSent > 0 ? '$notSent settings were not sent. ' : ''}'
              'No changes were rolled back.',
          confirmed: confirmed,
          total: total);
    } finally {
      final waiting = recall.waiting;
      if (waiting != null) {
        _maps.pending.remove(ps.pendingKey(target.channel, waiting.field));
      }
      if (identical(_presetRecalls[slot], recall)) _presetRecalls.remove(slot);
      if (!_presetDisposed) {
        _invalidateDerived();
        notifyListeners();
      }
    }
  }

  // --- connection (phase/message/host, reduced) ---
  bool _online = false;
  bool _demo = true;
  String _deskName = 'sim';
  String _message = 'Not connected';

  // --- visible transport errors (never silently swallowed) ---
  // Engine `error` frames already land in engine.error; poll/cmd transport
  // failures (socket/FFI hiccups) previously vanished in catch (_). They
  // now surface here so the Link tab can render them with a timestamp.
  String _transportError = '';
  DateTime? _transportErrorAt;
  String _lastEngineError = '';
  DateTime? _engineErrorAt;
  int _errorRevision = 0;

  // --- mixer control state ---
  final Map<int, double> _confirmedFader = {};
  final Map<int, bool> _confirmedMute = {};
  String _bank = 'ch1';
  String _busId = 'main';
  double _masterFader = 0.75;
  bool _masterMuted = false;

  // --- meter notifiers (stable, updated in place) ---
  final List<ValueNotifier<double>> _meters = [];
  final ValueNotifier<double> _masterMeter = ValueNotifier(0);
  final Map<int, BridgeTap> _taps = {};
  final Map<String, ValueNotifier<double>> _busMeters = {};
  final Map<int, double> _peakAtMs = {};

  // --- per-strip control notifiers (stable, updated in place) ---
  // Hot-path writes (fader drags, mute taps, pan drags) update these in
  // place and NEVER call notifyListeners(), so one strip rebuilds instead
  // of the whole shell + all strips. Global notifyListeners() is reserved
  // for structural changes (strips add/remove, bank/bus select, connection).
  // Solo keeps its notifier AND a global notify (infrequent taps; the page
  // solo banner reads hasSolo/soloCount off the store).
  final Map<int, ValueNotifier<double>> _faderNotes = {};
  final Map<int, ValueNotifier<bool>> _muteNotes = {};
  final Map<int, ValueNotifier<double>> _panNotes = {};
  final Map<int, ValueNotifier<bool>> _soloNotes = {};
  final Map<int, ValueNotifier<int>> _controlReadyNotes = {};

  // Solo is a dedicated protocol event, absent from the param catalogue.
  // A real solo echo establishes availability; caps alone never enables it.
  bool canEditControl(int ch, String field) =>
      engine.driver == 'sim' ||
      (engine.connected &&
          engine.capsReady &&
          (engine.supportedParams.contains(field) || field == 'solo') &&
          engine.valueKnown(ch, field));

  int _controlReady(int ch) => ch < 0
      ? ((masterControlKnown('fader') ? 1 : 0) |
          (masterControlKnown('mute') ? 2 : 0))
      : (canEditControl(ch, 'fader') ? 1 : 0) |
          (canEditControl(ch, 'mute') ? 2 : 0) |
          (canEditControl(ch, 'pan') ? 4 : 0) |
          (canEditControl(ch, 'solo') ? 8 : 0);

  ValueNotifier<int> controlReadyNote(int ch) => _controlReadyNotes.putIfAbsent(
      ch, () => ValueNotifier(_controlReady(ch)));

  int? get _selectedSubgroup => _busId.startsWith('subgroup')
      ? int.tryParse(_busId.substring('subgroup'.length))
      : null;

  bool masterControlKnown(String field) {
    final group = _selectedSubgroup;
    if (group != null) {
      // Capability establishes addressability; each echo hydrates only its
      // own control, including in sim. Never enable from a fallback value.
      return engine.connected &&
          engine.capsReady &&
          group >= 0 &&
          group < engine.subgroupCount &&
          (field == 'fader'
              ? engine.subgroupFaders.containsKey(group)
              : engine.subgroupMuted.containsKey(group));
    }
    return engine.driver == 'sim' ||
        (engine.connected &&
            (_busId == 'main'
                ? (field == 'fader'
                    ? engine.mainFader != null
                    : engine.mainMuted != null)
                : (field == 'fader'
                    ? engine.busFaders.containsKey(
                        int.tryParse(_busId.replaceFirst('bus', '')))
                    : engine.busMuted.containsKey(
                        int.tryParse(_busId.replaceFirst('bus', ''))))));
  }

  final ValueNotifier<double> _masterFaderNote = ValueNotifier(0.75);
  final ValueNotifier<bool> _masterMutedNote = ValueNotifier(false);

  // Raw engineering values, keyed by channel + canonical send field.
  final Map<(int, String), ValueNotifier<double?>> _sendParamNotes = {};

  // --- channel page state ---
  int _channel = 0;
  ChannelTab _tab = ChannelTab.input;
  int _eqBand = 1;

  final ValueNotifier<double> _levelDb = ValueNotifier(double.negativeInfinity);
  final ValueNotifier<List<double>> _spectrum = ValueNotifier(const []);
  final ValueNotifier<AudioSpectrumFrame?> _audioSpectrum = ValueNotifier(null);
  final ValueNotifier<AudioSpectrumPeaks> _audioSpectrumPeaks =
      ValueNotifier(const AudioSpectrumPeaks.empty());
  AudioSpectrumFrame? _lastPeakFrame;
  int? _focusCh;

  // --- derived-list cache (no per-build allocations) ---
  // strips/banks/busRows/groups/sends (+_bankRanges) rebuild only after a
  // control-state mutation nulls them via [_invalidateDerived]. Cached lists
  // are unmodifiable snapshots: callers only read them (verified: no call
  // site sorts/adds/removes on these getters), so sharing one instance across
  // builds is safe and fail-fast if a future caller tries to mutate.
  Map<String, (int, int)>? _bankRangesCache;
  List<Strip>? _stripsCache;
  List<MixerBank>? _banksCache;
  List<MixerBusRow>? _busRowsCache;
  List<BridgeGroup>? _groupsCache;
  List<ChannelSendVm>? _sendsCache;

  // Drop every derived-list cache after any mutator affecting the cached getters.

  void _invalidateDerived() {
    _bankRangesCache = null;
    _stripsCache = null;
    _banksCache = null;
    _busRowsCache = null;
    _groupsCache = null;
    _sendsCache = null;
  }

  // ------------------------------------------------------------------
  // Lifecycle
  // ------------------------------------------------------------------

  // Attach to a booted client id, then resync.

  Future<void> attach(String clientId) async {
    _invalidatePresetOperations('Connection changed. Reopen presets.');
    _interruptAllLibrary('Engine attachment changed.');
    _interruptForgetSavedMixers('Engine attachment changed.');
    _libraryRequests.clear();
    _libraryRecalls.clear();
    _libraryGeneration++;
    engine.resetLocalLibraryStatus();
    _clientId = clientId;
    _binaryMeters = true; // re-arm: a fresh attach may carry a new library
    try {
      await _api.resync(clientId);
    } catch (e) {
      // transports may not implement resync; anything else is visible.
      _noteTransportError(e);
    }
    refreshLocalLibrary();
    _invalidateDerived();
  }

  void setOnline(bool v) {
    if (!v) _stopPresetRecalls('Connection changed or lost. Reopen presets.');
    if (_online == v) return;
    _online = v;
    _message = v ? 'online' : 'Not connected';
    _invalidateDerived(); // groups renders an offline placeholder
    notifyListeners();
  }

  bool get onlineFlag => _online;
  String get statusMessage => _message;

  // Most recent poll or cmd transport failure, or '' when none.

  String get transportError => _transportError;
  DateTime? get transportErrorAt => _transportErrorAt;

  // Each received failure is distinct even when its message repeats.

  int get errorRevision => _errorRevision;

  // Transport failure wins because it explains silent probes, else the engine error frame.

  String get visibleError =>
      _transportError.isNotEmpty ? _transportError : engine.error;

  // Timestamp of the visible error, or null when healthy.

  DateTime? get visibleErrorAt =>
      _transportError.isNotEmpty ? _transportErrorAt : _engineErrorAt;

  void _noteTransportError(Object e) {
    _interruptForgetSavedMixers(
        'A transport error interrupted address clearing: $e.');
    _interruptAllLibrary(
        'A transport error interrupted the library operation: $e.');
    _stopPresetRecalls('A connection error interrupted recall: $e. '
        'The last setting may have changed; its result is uncertain.');
    final s = '$e';
    _errorRevision++;
    _transportError = s;
    _transportErrorAt = DateTime.now();
    notifyListeners();
  }

  // Clear both error displays; the engine re-sets its own on the next error frame.

  void clearErrors() {
    _transportError = '';
    _transportErrorAt = null;
    engine.error = '';
    notifyListeners();
  }

  void _trackEngineError() {
    if (engine.error != _lastEngineError) {
      _lastEngineError = engine.error;
      _engineErrorAt = engine.error.isEmpty ? null : DateTime.now();
    }
  }

  void _send(String json) {
    unawaited(_api.cmd(_clientId, json).then((_) {}, onError: (e) {
      _noteTransportError(e);
    }));
  }

  // Raw Cmd JSON write for commands with no dedicated store method.

  void sendRaw(String json) {
    _prepareConnectionCommand(json);
    _send(json);
  }

  void _clearSessionValues() {
    _interruptAllLibrary(
        'Connection changed while waiting for the library result.');
    _interruptForgetSavedMixers('Connection changed during address clearing.');
    _presetVerifiedFields.clear();
    _invalidatePresetOperations('Connection changed or lost. Reopen presets.');
    _failNameEdits('Connection changed or lost. Reopen the name editor.');
    _coalescer.clear();
    _maps.confirmed.clear();
    _maps.pending.clear();
    _confirmedFader.clear();
    _confirmedMute.clear();
    _bank = 'ch1';
    _busId = 'main';
    _focusCh = null;
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    _invalidateDerived();
  }

  void _prepareConnectionCommand(String json) {
    final command = jsonDecode(json);
    if (command is! Map ||
        (command['cmd'] != 'connect' && command['cmd'] != 'disconnect')) {
      return;
    }
    engine.resetSession();
    _clearSessionValues();
    _syncControlNotes();
    notifyListeners();
  }

  Future<void> sendCommand(String json) async {
    _prepareConnectionCommand(json);
    await _api.cmd(_clientId, json);
    await pollMeters();
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

  double _nowMs() => DateTime.now().microsecondsSinceEpoch / 1000.0;

  // ------------------------------------------------------------------
  // Single shared 80 ms ticker target (driven ONLY by the AppShell loop).
  // ------------------------------------------------------------------
  @override
  Future<void> pollMeters() async {
    if (_polling) return;
    _polling = true;
    try {
      await _pollOnce();
    } catch (error) {
      _stopPresetRecalls('Could not read mixer confirmations: $error');
      rethrow;
    } finally {
      _polling = false;
    }
  }

  Future<void> _pollOnce() async {
    final now = _nowMs();
    for (final key in _nameEdits.keys.toList()) {
      if (now >= (_nameEditDeadlines[key] ?? double.negativeInfinity)) {
        _finishNameEdit(key,
            'The mixer did not provide a verified name readback. The name may have changed.');
      }
    }
    _pollLibrary(now);
    _pollPresetRecalls(now);
    final expired = ps.sweepPending(_maps.pending, now, ps.confirmTimeoutMs);
    if (expired.isNotEmpty) {
      for (final key in expired) {
        _finishNameEdit(key, 'The backend did not confirm the name change.');
        final split = ps.splitKey(key);
        if (split == null) continue;
        if (split.id == 'fader') {
          final confirmed = _confirmedFader[split.ch];
          if (confirmed != null) {
            for (final s in engine.strips) {
              if (s.ch == split.ch) s.fader = confirmed;
            }
          }
        } else if (split.id == 'mute') {
          final confirmed = _confirmedMute[split.ch];
          if (confirmed != null) {
            for (final s in engine.strips) {
              if (s.ch == split.ch) s.muted = confirmed;
            }
          }
        }
      }
      // Confirm-timeout revert: caches + strip notifiers stay in sync,
      // then one structural notify (rare path, not the drag hot path).
      _invalidateDerived();
      _syncControlNotes();
      notifyListeners();
    }
    List<String> frames;
    // Hot meter path (20 Hz): compact binary frames fan straight into
    // the meter notifiers with no JSON parse/alloc. Non-meter events seen
    // here are set aside server-side and come back through poll() below,
    // so the JSON fallback stays lossless. A missing `pollBinary` (stale
    // native library) latches binary off; JSON meters keep rendering.
    if (_binaryMeters) {
      try {
        final blob = await _api.pollBinary(_clientId, 512);
        for (final frame in splitBinaryBatch(blob)) {
          applyBinaryFrame(frame);
        }
      } catch (_) {
        _binaryMeters = false;
      }
    }
    try {
      frames = await _api.poll(_clientId, 512);
    } catch (e) {
      _stopPresetRecalls('Could not read mixer confirmations: $e');
      _failNameEdits(
          'Connection lost while waiting for the name confirmation: $e');
      _noteTransportError(e);
      _syncSpectrum();
      return; // never crash the meter loop on a transport hiccup
    }
    var dirty = false;
    var controlsTouched = false;
    for (final f in frames) {
      try {
        final e = jsonDecode(f);
        if (e is! Map<String, dynamic>) continue;
        final previousSession = engine.session;
        final controlOnly = _controlOnlyFrame(e);
        var frameDirty = engine.applyFrame(e);
        if (e['type'] == 'local_library_result') {
          _receiveLibraryResult(e);
          frameDirty = true;
        } else if (e['type'] == 'local_library_status' &&
            LocalLibraryStatus.fromJson(e) == engine.localLibrary &&
            engine.localLibrary != null) {
          final status = engine.localLibrary!;
          for (final query in _libraryQueries.toList()) {
            _checkLibraryQuery(query, _nowMs());
            if (!query.done.isCompleted) query.done.complete(status);
          }
          _libraryQueries.clear();
        } else if (e['type'] == 'device_operation_complete') {
          _receiveForgetSavedMixersResult(e);
        }
        if (previousSession != engine.session) {
          _clearSessionValues();
          dirty = true;
        }
        // Preserve maxima from every accepted measured frame in this batch,
        // including source/unavailable transitions before its final frame.
        if (e['type'] == 'audio_spectrum') _syncSpectrum();
        _confirmEcho(e);
        if (e['type'] == 'error') {
          _errorRevision++;
          _engineErrorAt = DateTime.now();
          _stopPresetRecalls(
              'The mixer reported an error during recall: ${e['message'] ?? 'Unknown error'}. '
              'The last setting may have changed; its result is uncertain.');
          final ch = (e['ch'] as num?)?.toInt();
          for (final key in _nameEdits.keys.toList()) {
            if (ch == null || ps.splitKey(key)?.ch == ch) {
              _finishNameEdit(key,
                  'Backend rejected the name change: ${e['message'] ?? 'Unknown error'}');
            }
          }
        }
        if (controlOnly) {
          // Control echo (strip/send/main/bus): the strip
          // notifiers pick it up below, no global rebuild.
          controlsTouched = true;
        } else if (frameDirty) {
          dirty = true;
        }
      } catch (error) {
        _stopPresetRecalls('Could not read mixer confirmation: $error');
        _noteTransportError('Invalid engine frame: $error');
      }
    }
    if (_online != engine.connected) {
      _online = engine.connected;
      dirty = true;
    }
    _pollLibrary(_nowMs());
    _pollPresetRecalls(_nowMs());
    _trackEngineError();
    // A successful poll carrying control traffic clears a stale transport
    // failure so a transient hiccup does not stick forever.
    if (_transportError.isNotEmpty && frames.isNotEmpty) {
      _transportError = '';
      _transportErrorAt = null;
      dirty = true;
    }
    if (_online && engine.strips.isNotEmpty) {
      _demo = engine.driver == 'sim';
      _deskName = engine.linkName.isNotEmpty ? engine.linkName : engine.driver;
    }
    _syncMeters(now);
    // Push any control traffic (local optimistic writes already did; echoes
    // land here) into the stable per-strip notifiers. ValueNotifiers only
    // wake their own strip; the shell/page ListenableBuilders stay quiet.
    if (dirty) _invalidateDerived();
    if (controlsTouched || dirty) _syncControlNotes();
    if (dirty) notifyListeners();
  }

  // Binary fan-in for the meter hot path; bad frames decode to null and are skipped.
  void applyBinaryFrame(Uint8List bytes) {
    final f = parseBinary(bytes);
    if (f == null) return;
    engine.applyBinary(f);
    _invalidateDerived();
    _syncMeters(_nowMs());
  }

  // Control-only frames ride notifiers without setting the global dirty flag.

  static const _stripControlFields = {'fader', 'mute', 'muted', 'pan'};

  static final _sendHotField = RegExp(r'^send[1-9]\d*_(pan|level)$');

  bool _isHotControlField(String field) =>
      _stripControlFields.contains(field) || _sendHotField.hasMatch(field);

  bool _controlOnlyFrame(Map<String, dynamic> e) {
    switch (e['type']) {
      case 'param':
        final field = e['field'] as String?;
        return field != null && _isHotControlField(field);
      case 'params':
        final values = e['values'];
        if (values is! Map || values.isEmpty) return false;
        return values.keys.every((k) => k is String && _isHotControlField(k));
      case 'main':
      case 'bus':
      case 'subgroup':
        // Row status is cached, but control echoes stay on the notifier path.
        _busRowsCache = null;
        return true;
      default:
        return false;
    }
  }

  void _confirmEcho(Map<String, dynamic> e) {
    switch (e['type']) {
      case 'param':
        final ch = (e['ch'] as num?)?.toInt();
        final field = e['field'] as String?;
        final v = e['value'];
        if (ch == null || field == null || v == null) return;
        final pv = v as ps.ParamValue;
        ps.confirmInto(_maps.pending, ps.pendingKey(ch, field), pv);
        ps.applyParams(_maps, ch, {field: pv});
        final verified = _recordParameterAuthority(e, ch, field);
        if (verified || e['authority'] == 'accepted') {
          _confirmPresetValue(ch, field, pv, verified: verified);
        }
        if (field == 'fader' && v is num) {
          _confirmedFader[ch] = v.toDouble();
        } else if ((field == 'mute' || field == 'muted') && v is bool) {
          _confirmedMute[ch] = v;
        }
        _invalidateDerived();
        break;
      case 'params':
        final ch = (e['ch'] as num?)?.toInt();
        final values = e['values'];
        if (ch == null || values is! Map) return;
        final pv = <String, ps.ParamValue>{
          for (final kv in values.entries)
            if (kv.key is String && ps.validValue(kv.value))
              kv.key as String: kv.value as ps.ParamValue,
        };
        ps.applyParams(_maps, ch, pv);
        if (pv['fader'] is num) {
          _confirmedFader[ch] = (pv['fader'] as num).toDouble();
        }
        final mute = pv['mute'] ?? pv['muted'];
        if (mute is bool) _confirmedMute[ch] = mute;
        for (final kv in pv.entries) {
          ps.confirmInto(_maps.pending, ps.pendingKey(ch, kv.key), kv.value);
          final verified = _recordParameterAuthority(e, ch, kv.key);
          final authority = e['authority'];
          if (verified ||
              (authority is Map && authority[kv.key] == 'accepted')) {
            _confirmPresetValue(ch, kv.key, kv.value, verified: verified);
          }
        }
        _invalidateDerived();
        break;
      case 'state':
        final raw = (e['strips'] as List?) ?? const [];
        for (final s in raw) {
          if (s is! Map) continue;
          final ch = (s['ch'] as num?)?.toInt();
          if (ch == null) continue;
          final f = s['fader'], m = (s['muted'] ?? s['mute']);
          ps.applyParams(_maps, ch, {
            if (f is num) 'fader': f,
            if (m is bool) 'mute': m,
          });
          if (f is num) {
            _confirmedFader[ch] = f.toDouble();
            ps.confirmInto(
                _maps.pending, ps.pendingKey(ch, 'fader'), f.toDouble());
          }
          if (m is bool) {
            _confirmedMute[ch] = m;
            ps.confirmInto(_maps.pending, ps.pendingKey(ch, 'mute'), m);
          }
        }
        _invalidateDerived();
        break;
    }
  }

  void _syncMeters(double nowMs) {
    while (_meters.length < engine.strips.length) {
      _meters.add(ValueNotifier(0));
    }
    while (_meters.length > engine.strips.length) {
      _meters.removeLast().dispose();
    }
    for (var i = 0; i < engine.strips.length; i++) {
      final lv =
          i < engine.meters.length ? engine.meters[i].clamp(0.0, 1.0) : 0.0;
      _meters[i].value = lv;
      final ch = engine.strips[i].ch;
      final tap = _taps.putIfAbsent(ch, () => BridgeTap());
      final db = _linToDb(lv);
      tap.db.value = db;
      final prevPeak = tap.peakDb.value;
      if (!prevPeak.isFinite || db > prevPeak) {
        tap.peakDb.value = db;
        _peakAtMs[ch] = nowMs;
      } else if (nowMs - (_peakAtMs[ch] ?? nowMs) > 1200) {
        // ~12 dB/s peak fall after a 1.2 s hold.
        tap.peakDb.value = prevPeak - 1.0;
      }
      tap.clip.value = db.isFinite && db >= -0.5;
    }
    final masterKey = _busId == 'main'
        ? 'main'
        : _selectedSubgroup != null
            ? 'subgroup/${(_selectedSubgroup! + 1).toString().padLeft(2, '0')}'
            : 'bus/${((int.tryParse(_busId.replaceFirst('bus', '')) ?? 0) + 1).toString().padLeft(2, '0')}';
    final masterIndex = engine.strips.indexWhere((s) => s.key == masterKey);
    _masterMeter.value = masterIndex >= 0 && masterIndex < engine.meters.length
        ? engine.meters[masterIndex].clamp(0.0, 1.0)
        : 0;
    // Open-channel feeds.
    final ci = engine.strips.indexWhere((s) => s.ch == _channel);
    final lv = ci >= 0 && ci < engine.meters.length
        ? engine.meters[ci].clamp(0.0, 1.0)
        : 0.0;
    _levelDb.value = _linToDb(lv);
    _syncSpectrum();
  }

  void _syncSpectrum() {
    // Legacy synthetic bins remain exclusively on the demo path.
    final bins = engine.driver == 'sim' ? engine.spectrum[_channel] : null;
    _spectrum.value =
        engine.connected && bins != null ? List.of(bins) : const [];
    final frame = engine.audioSpectrum;
    if (frame != null && !frame.isFresh) engine.clearAudioSpectrum();
    _audioSpectrum.value = engine.connected &&
            frame != null &&
            frame.isFresh &&
            (frame.ch == null || frame.ch == _channel)
        ? frame
        : null;
    final measured = _audioSpectrum.value;
    if (measured == null) {
      _clearAudioSpectrumPeaks();
    } else if (!identical(measured, _lastPeakFrame)) {
      final previous = _lastPeakFrame;
      // A gap can outlast expiry between polls even if a fresh frame has
      // already replaced the old one. Do not carry peaks across that gap.
      if (previous != null &&
          measured.receivedAt.difference(previous.receivedAt) >=
              AudioSpectrumFrame.expiry) {
        _clearAudioSpectrumPeaks();
      }
      _lastPeakFrame = measured;
      _audioSpectrumPeaks.value = _audioSpectrumPeaks.value.include(measured);
    }
  }

  void _clearAudioSpectrumPeaks() {
    _lastPeakFrame = null;
    _audioSpectrumPeaks.value = const AudioSpectrumPeaks.empty();
  }

  @override
  void resetAudioSpectrumPeaks() {
    _clearAudioSpectrumPeaks();
    // A visibility change starts a new hold with the next incoming frame,
    // rather than seeding it again from the cached pre-toggle measurement.
    _lastPeakFrame = engine.audioSpectrum;
  }

  // ------------------------------------------------------------------
  // MixerStore
  // ------------------------------------------------------------------
  @override
  List<Strip> get strips {
    final cached = _stripsCache;
    if (cached != null) return cached;
    final range = _bankRanges[_bank];
    final visible = range == null
        ? engine.strips
        : engine.strips.where((s) => s.ch >= range.$1 && s.ch < range.$2);
    final built = List<Strip>.unmodifiable(visible.map((s) => Strip(
          ch: s.ch,
          key: s.key,
          name: s.name,
          fader: _stripFaderValue(s.ch, s.fader),
          muted: _stripMuteValue(s.ch, s.muted),
        )));
    _stripsCache = built;
    return built;
  }

  Map<String, (int, int)> get _bankRanges {
    final cached = _bankRangesCache;
    if (cached != null) return cached;
    final Map<String, (int, int)> built = {
      for (final f in engine.topology)
        for (var offset = 0; offset < (f['count'] as num).toInt(); offset += 16)
          f['key'] == 'ch'
              ? 'ch${offset ~/ 16 + 1}'
              : "${f['key']}:${(f['first'] as num).toInt() + offset}": (
            (f['first'] as num).toInt() + offset,
            (f['first'] as num).toInt() +
                math.min(offset + 16, (f['count'] as num).toInt())
          ),
    };
    _bankRangesCache = built;
    return built;
  }

  @override
  double panOf(int ch) {
    final v = ps.optimisticValue(_maps, ch, 'pan');
    return v is num ? (v.toDouble() / 100).clamp(-1.0, 1.0) : 0.0;
  }

  @override
  bool soloOf(int ch) => engine.solos.contains(ch);

  @override
  bool get hasSolo => engine.solos.isNotEmpty;

  @override
  int get soloCount => engine.solos.length;

  @override
  ValueNotifier<double> meterOf(int ch) {
    final i = engine.strips.indexWhere((s) => s.ch == ch);
    if (i < 0) return _masterMeter;
    return _meters[i];
  }

  @override
  ValueNotifier<double> get masterMeter => _masterMeter;

  // ------------------------------------------------------------------
  // Per-strip control notifiers: stable ValueNotifiers updated in place.
  // Strips listen to their own ch only; the hot drag path never calls
  // notifyListeners() so the shell + sibling strips are not rebuilt.
  // ------------------------------------------------------------------
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

  void _ensureStripNotes(int ch) {
    _faderNotes.putIfAbsent(ch, () {
      double v = 0;
      for (final s in engine.strips) {
        if (s.ch == ch) v = s.fader;
      }
      return ValueNotifier(_protocolFaderToUi(_stripFaderValue(ch, v)));
    });
    _muteNotes.putIfAbsent(ch, () {
      bool m = false;
      for (final s in engine.strips) {
        if (s.ch == ch) m = s.muted;
      }
      return ValueNotifier(_stripMuteValue(ch, m));
    });
    _panNotes.putIfAbsent(ch, () => ValueNotifier(panOf(ch)));
    _soloNotes.putIfAbsent(ch, () => ValueNotifier(engine.solos.contains(ch)));
  }

  @override
  ValueNotifier<double> faderOf(int ch) {
    _ensureStripNotes(ch);
    return _faderNotes[ch]!;
  }

  @override
  ValueNotifier<bool> muteOf(int ch) {
    _ensureStripNotes(ch);
    return _muteNotes[ch]!;
  }

  @override
  ValueNotifier<double> panNoteOf(int ch) {
    _ensureStripNotes(ch);
    return _panNotes[ch]!;
  }

  @override
  ValueNotifier<bool> soloNoteOf(int ch) {
    _ensureStripNotes(ch);
    return _soloNotes[ch]!;
  }

  @override
  ValueNotifier<double> get masterFaderNote => _masterFaderNote;

  @override
  ValueNotifier<bool> get masterMutedNote => _masterMutedNote;

  void _pushStripNote(int ch) {
    controlReadyNote(ch).value = _controlReady(ch);
    _ensureStripNotes(ch);
    for (final s in engine.strips) {
      if (s.ch != ch) continue;
      final f = _faderNotes[ch]!;
      final uiFader = _protocolFaderToUi(_stripFaderValue(ch, s.fader));
      if (f.value != uiFader) f.value = uiFader;
      final m = _muteNotes[ch]!;
      final muted = _stripMuteValue(ch, s.muted);
      if (m.value != muted) m.value = muted;
    }
    final p = _panNotes[ch]!;
    final pv = panOf(ch);
    if (p.value != pv) p.value = pv;
    final so = _soloNotes[ch]!;
    final sv = engine.solos.contains(ch);
    if (so.value != sv) so.value = sv;
  }

  double? _sendParamValue(int ch, String field) {
    final value = ps.optimisticValue(_maps, ch, field);
    return value is num ? value.toDouble() : null;
  }

  @override
  ValueNotifier<double?> sendParamNote(int slot, String param) {
    assert(param == 'pan' || param == 'level');
    final key = (_channel, 'send${slot}_$param');
    return _sendParamNotes.putIfAbsent(
        key, () => ValueNotifier(_sendParamValue(key.$1, key.$2)));
  }

  void _syncSendParamNotes() {
    for (final entry in _sendParamNotes.entries) {
      entry.value.value = _sendParamValue(entry.key.$1, entry.key.$2);
    }
  }

  void _syncControlNotes() {
    _syncSendParamNotes();
    for (final entry in _controlReadyNotes.entries) {
      entry.value.value = _controlReady(entry.key);
    }
    for (final s in engine.strips) {
      _pushStripNote(s.ch);
    }
    final mf = masterFader;
    if (_masterFaderNote.value != mf) _masterFaderNote.value = mf;
    final mm = masterMuted;
    if (_masterMutedNote.value != mm) _masterMutedNote.value = mm;
  }

  @override
  double get masterFader => _protocolFaderToUi(_selectedSubgroup != null
      ? engine.subgroupFaders[_selectedSubgroup] ?? 0
      : _busId == 'main'
          ? engine.mainFader ?? _masterFader
          : engine.busFaders[int.tryParse(_busId.replaceFirst('bus', ''))] ??
              0);

  @override
  bool get masterMuted => _selectedSubgroup != null
      ? engine.subgroupMuted[_selectedSubgroup] ?? false
      : _busId == 'main'
          ? engine.mainMuted ?? _masterMuted
          : engine.busMuted[int.tryParse(_busId.replaceFirst('bus', ''))] ??
              false;

  @override
  String get masterName => _selectedSubgroup != null
      ? 'SUBGROUP ${_selectedSubgroup! + 1}'
      : _busId == 'main'
          ? 'MAIN'
          : 'BUS ${(int.tryParse(_busId.replaceFirst('bus', '')) ?? 0) + 1}';

  @override
  List<MixerBank> get banks {
    final cached = _banksCache;
    if (cached != null) return cached;
    final built = List<MixerBank>.unmodifiable([
      for (final entry in _bankRanges.entries)
        MixerBank(
            key: entry.key,
            label: entry.key.startsWith('ch')
                ? 'Ch ${entry.value.$1 + 1}–${entry.value.$2}'
                : engine.topology.firstWhere((f) =>
                    (f['first'] as num).toInt() <= entry.value.$1 &&
                    (f['first'] as num).toInt() + (f['count'] as num).toInt() >
                        entry.value.$1)['label'] as String,
            color: const ConsolePalette().lime,
            text: const ConsolePalette().onAccent),
    ]);
    _banksCache = built;
    return built;
  }

  @override
  String get bank => _bank;

  @override
  List<MixerBusRow> get busRows {
    final cached = _busRowsCache;
    if (cached != null) return cached;
    final built = List<MixerBusRow>.unmodifiable([
      MixerBusRow(
        id: 'main',
        name: 'MAIN',
        sub: 'LR master',
        muted: engine.mainMuted ?? _masterMuted,
        active: _busId == 'main',
        meter: _masterMeter,
      ),
      for (final b in engine.busFaders.keys.toList()..sort())
        MixerBusRow(
          id: 'bus$b',
          name: 'BUS ${b + 1}',
          sub: engine.busMuted[b] == true ? 'muted' : 'open',
          muted: engine.busMuted[b] ?? false,
          active: _busId == 'bus$b',
          meter: _busMeters.putIfAbsent('bus$b', () => ValueNotifier(0)),
        ),
      if (engine.capsReady)
        for (var group = 0; group < engine.subgroupCount; group++)
          MixerBusRow(
            id: 'subgroup$group',
            name: 'SUBGROUP ${group + 1}',
            sub: !engine.subgroupFaders.containsKey(group) ||
                    !engine.subgroupMuted.containsKey(group)
                ? 'Awaiting desk values'
                : engine.subgroupMuted[group] == true
                    ? 'muted'
                    : 'open',
            muted: engine.subgroupMuted[group] ?? false,
            active: _busId == 'subgroup$group',
            // Subgroups have no dedicated meter feed in the current protocol.
            meter: _busMeters.putIfAbsent(
                'subgroup$group', () => ValueNotifier(0)),
          ),
    ]);
    _busRowsCache = built;
    return built;
  }

  @override
  void setFader(int ch, double v) {
    if (!canEditControl(ch, 'fader')) return;
    final nv = v.clamp(0.0, 1.0);
    final protocolValue = _uiFaderToProtocol(nv);
    for (final s in engine.strips) {
      if (s.ch == ch) s.fader = protocolValue;
    }
    ps.noteOptimistic(_maps, ch, 'fader', protocolValue, _nowMs());
    _ensureStripNotes(ch);
    final n = _faderNotes[ch]!;
    if (n.value != nv) n.value = nv;
    _coalescer.offer('fader:$ch', () => _send(Cmds.fader(ch, protocolValue)));
    // Hot path: no notifyListeners(). The dragged strip rebuilds from
    // faderOf(ch) alone; shell + siblings stay quiet. Caches invalidated
    // so the next structural rebuild sees fresh values.
    _invalidateDerived();
  }

  @override
  void setMute(int ch, bool muted) {
    if (!canEditControl(ch, 'mute')) return;
    for (final s in engine.strips) {
      if (s.ch == ch) s.muted = muted;
    }
    ps.noteOptimistic(_maps, ch, 'mute', muted, _nowMs());
    _ensureStripNotes(ch);
    final n = _muteNotes[ch]!;
    if (n.value != muted) n.value = muted;
    _coalescer.offer('mute:$ch', () => _send(Cmds.mute(ch, muted)));
    // Hot path: no notifyListeners(). The tapped strip rebuilds from
    // muteOf(ch) alone. Caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void toggleSolo(int ch) {
    if (!canEditControl(ch, 'solo')) return;
    final on = !engine.solos.contains(ch);
    if (on) {
      engine.solos.add(ch);
    } else {
      engine.solos.remove(ch);
    }
    _ensureStripNotes(ch);
    final n = _soloNotes[ch]!;
    if (n.value != on) n.value = on;
    _coalescer.offer('solo:$ch', () => _send(Cmds.solo(ch, on)));
    // Solo stays global: the page solo banner reads hasSolo/soloCount.
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void clearSolos() {
    if (engine.driver != 'sim' &&
        (!engine.connected ||
            !engine.knownValues.keys
                .any((ch) => engine.valueKnown(ch, 'solo')))) {
      return;
    }
    engine.solos.clear();
    for (final n in _soloNotes.values) {
      if (n.value) n.value = false;
    }
    _send(Cmds.clearSolo());
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void setPan(int ch, double v) {
    if (!canEditControl(ch, 'pan')) return;
    final nv = v.clamp(-1.0, 1.0);
    ps.noteOptimistic(_maps, ch, 'pan', nv * 100, _nowMs());
    _ensureStripNotes(ch);
    final n = _panNotes[ch]!;
    if (n.value != nv) n.value = nv;
    _coalescer.offer('pan:$ch', () => _send(Cmds.param(ch, 'pan', nv * 100)));
    // Hot path: no notifyListeners(). The dragged strip rebuilds from
    // panNoteOf(ch) alone. Caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void setMasterFader(double v) {
    if (!masterControlKnown('fader')) return;
    final nv = v.clamp(0.0, 1.0);
    // Capture the driver's conversion now, before a queued send can run
    // after a driver/bus change. The UI bottom maps to the driver's floor.
    final protocolValue = _uiFaderToProtocol(nv);
    final group = _selectedSubgroup;
    if (group != null) {
      engine.subgroupFaders[group] = protocolValue;
      _masterFaderNote.value = nv;
      _coalescer.offer('subgroup_fader:$group',
          () => _send(Cmds.subgroupFader(group, protocolValue)));
      _invalidateDerived();
      return;
    }
    _masterFader = protocolValue;
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busFaders[bus] = _masterFader;
      if (_masterFaderNote.value != nv) {
        _masterFaderNote.value = nv;
      }
      _coalescer.offer(
          'bus_fader:$bus', () => _send(Cmds.busFader(bus, protocolValue)));
      // Hot path: master strip rebuilds from masterFaderNote alone.
      // No global notify; caches invalidated for the next structural pass.
      _invalidateDerived();
      return;
    }
    engine.mainFader = _masterFader;
    if (_masterFaderNote.value != nv) {
      _masterFaderNote.value = nv;
    }
    _coalescer.offer('main_fader', () => _send(Cmds.mainFader(protocolValue)));
    // Hot path: master strip rebuilds from masterFaderNote alone.
    // No global notify; caches invalidated for the next structural pass.
    _invalidateDerived();
  }

  @override
  void setMasterMuted(bool muted) {
    if (!masterControlKnown('mute')) return;
    final group = _selectedSubgroup;
    if (group != null) {
      engine.subgroupMuted[group] = muted;
      _masterMutedNote.value = muted;
      _coalescer.offer(
          'subgroup_mute:$group', () => _send(Cmds.subgroupMute(group, muted)));
      _invalidateDerived();
      return;
    }
    _masterMuted = muted;
    if (_busId != 'main') {
      final bus = int.tryParse(_busId.replaceFirst('bus', ''));
      if (bus == null) return;
      engine.busMuted[bus] = muted;
      if (_masterMutedNote.value != muted) _masterMutedNote.value = muted;
      _coalescer.offer('bus_mute:$bus', () => _send(Cmds.busMute(bus, muted)));
      // Hot path: no global notify; caches invalidated.
      _invalidateDerived();
      return;
    }
    engine.mainMuted = muted;
    if (_masterMutedNote.value != muted) _masterMutedNote.value = muted;
    _coalescer.offer('main_mute', () => _send(Cmds.mainMute(muted)));
    // Hot path: no global notify; caches invalidated for the next pass.
    _invalidateDerived();
  }

  @override
  void selectBus(String id) {
    if (_busId == id || !busRows.any((row) => row.id == id)) return;
    _busId = id;
    _syncControlNotes();
    _syncMeters(_nowMs());
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void setBank(String key) {
    _bank = key;
    _invalidateDerived();
    notifyListeners();
  }

  // ------------------------------------------------------------------
  // MetersStore
  // ------------------------------------------------------------------
  @override
  bool get online => _online;

  @override
  bool get demo => _demo;

  @override
  String? get deskName => _deskName;

  @override
  List<BridgeGroup> get groups {
    final cached = _groupsCache;
    if (cached != null) return cached;
    final built = (!_online || engine.strips.isEmpty)
        ? const [
            BridgeGroup(
                key: 'in',
                title: 'Inputs',
                ladders: [],
                collapsedReason: 'offline'),
          ]
        : List<BridgeGroup>.unmodifiable([
            BridgeGroup(
              key: 'in',
              title: 'Inputs',
              ladders: [
                for (final s in engine.strips)
                  BridgeLadder(
                      ch: s.ch,
                      label: s.name.isEmpty ? null : s.name,
                      sub: '${s.ch + 1}'),
              ],
            ),
          ]);
    _groupsCache = built;
    return built;
  }

  @override
  BridgeTap tapOf(int ch) => _taps.putIfAbsent(ch, () => BridgeTap());

  @override
  void setSpectrumTarget(int? ch) {
    // DL32S analysis is passive: neither entry nor meter navigation acquires
    // a named source or writes the desk's shared analyser selector.
    if (engine.driver == 'dl32s') return;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    _send(Cmds.spectrumTarget(ch));
  }

  // ------------------------------------------------------------------
  // ChannelStore
  // ------------------------------------------------------------------
  // Current open channel index.

  int get channelIndex => _channel;

  // Select the open channel on shell navigation.

  void openChannel(int ch) {
    if (_channel == ch) return;
    _channel = ch;
    engine.clearAudioSpectrum();
    engine.spectrum.clear();
    _spectrum.value = const [];
    _audioSpectrum.value = null;
    _clearAudioSpectrumPeaks();
    cmdFocus();
    _invalidateDerived(); // sends are channel-scoped (sendN_on)
    notifyListeners();
  }

  @override
  String get stripKey => 'ch/${(_channel + 1).toString().padLeft(2, '0')}';

  @override
  String get displayName {
    for (final s in engine.strips) {
      if (s.ch == _channel && s.name.isNotEmpty) return s.name;
    }
    return 'CH${(_channel + 1).toString().padLeft(2, '0')}';
  }

  @override
  String get channelNumber => '${_channel + 1}';

  @override
  ValueNotifier<double> faderNote() => faderOf(_channel);

  @override
  void setFaderDb(double db) => setFader(_channel, dbToPos(db));

  @override
  double? paramNum(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, deskLeaf(leaf));
    return v is num ? v.toDouble() : null;
  }

  @override
  bool? paramBool(String leaf) {
    final v = ps.optimisticValue(_maps, _channel, deskLeaf(leaf));
    return v is bool ? v : null;
  }

  @override
  bool get capabilitiesReady => engine.capsReady;

  @override
  Map<String, dynamic>? paramMetadata(String leaf) =>
      parameterPresentation(engine.parameterMetadata[deskLeaf(leaf)], paramNum);

  @override
  bool supports(String leaf) {
    final desk = deskLeaf(leaf);
    if (!engine.capsReady || _presetEditingBlocked(_channel, desk)) {
      return false;
    }
    final presentation = paramMetadata(desk);
    if (presentation?['hidden'] == true ||
        presentation?['available'] == false) {
      return false;
    }
    final sendPan = RegExp(r'^send([1-9]\d*)_pan$').firstMatch(desk);
    if (sendPan != null &&
        !_sendDestinationIsStereoAux(int.parse(sendPan[1]!))) {
      return false;
    }
    // manufacturer mixer software's modern DL32S EQ disables Q while a shelf is selected
    // (reference guide p101). Keep the stored value, but block every edit
    // path, including pinch and a previously focused numeric readout.
    final eqQ = RegExp(r'^eq([1-4])_q$').firstMatch(desk);
    if (engine.driver == 'dl32s' &&
        eqQ != null &&
        EqFilterShape.fromValue(_eqPresentationType(int.parse(eqQ[1]!))) !=
            EqFilterShape.bell) {
      return false;
    }
    return engine.supportedParams.contains(desk) &&
        (engine.driver == 'sim' ||
            (engine.connected && engine.valueKnown(_channel, desk)));
  }

  @override
  void cmdParam(String leaf, Object value) {
    final desk = deskLeaf(leaf);
    if (desk == 'name') return; // Use the captured-target rename contract.
    if (!supports(desk)) return;
    if (value is num && !value.isFinite) return;
    final choices = parameterChoices(paramMetadata(desk));
    if (choices.isNotEmpty &&
        (value is! num ||
            !choices.any((c) => (c.value - value).abs() < .0001))) {
      return;
    }
    // DSP edits (including delayed ChannelWrites flushes) use the current
    // driver limits. Hydration remains untouched, even outside these limits.
    final isDspRange = desk == 'hpf_freq' ||
        RegExp(r'^eq[1-6]_(freq|gain|q)$').hasMatch(desk) ||
        RegExp(r'^(gate|comp)_(thr|range|att|hold|rel|ratio|knee|makeup|filter_freq|mix)$')
            .hasMatch(desk);
    final range = isDspRange ? _editableRange(desk) : null;
    if (isDspRange && (range == null || value is! num)) return;
    final coerced = range == null
        ? value
        : (value as num).toDouble().clamp(range.first, range.last);
    // Capture at enqueue time: navigation must not retarget a queued edit.
    final ch = _channel;
    _maps.confirmed.putIfAbsent(ch, () => {});
    ps.noteOptimistic(_maps, ch, desk, coerced, _nowMs());
    _coalescer.offer(
        'param:$ch:$desk', () => _send(Cmds.param(ch, desk, coerced)));
    _invalidateDerived(); // optimistic send reads feed sends
    if (_sendHotField.hasMatch(desk)) {
      _syncSendParamNotes();
    } else {
      notifyListeners();
    }
  }

  @override
  void cmdFocus() {
    _focusCh = _channel;
    _send(Cmds.focus([_channel]));
  }

  @override
  void cmdLock() => _send(Cmds.lock(_channel));

  @override
  void cmdUnlock() => _send(Cmds.unlock(_channel));

  @override
  void cmdSubgroupAssign(int g, bool on) {
    if (!engine.connected ||
        g < 0 ||
        g >= subgroupCount ||
        subgroupAssigned(g) == null) {
      return;
    }
    engine.subgroupAssign['$_channel:$g'] = on;
    _send(Cmds.subgroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void cmdMuteGroupAssign(int g, bool on) {
    if (!engine.connected ||
        g < 0 ||
        g >= muteGroupCount ||
        muteGroupAssigned(g) == null) {
      return;
    }
    engine.muteGroupAssign['$_channel:$g'] = on;
    _send(Cmds.muteGroupAssign(_channel, g, on));
    _invalidateDerived();
    notifyListeners();
  }

  @override
  ValueNotifier<double> get levelDb => _levelDb;

  @override
  ValueNotifier<List<double>> get spectrum => _spectrum;

  @override
  ValueNotifier<AudioSpectrumFrame?> get audioSpectrum => _audioSpectrum;

  @override
  ValueNotifier<AudioSpectrumPeaks> get audioSpectrumPeaks =>
      _audioSpectrumPeaks;

  @override
  double? grDb(DynKind kind) {
    final m = kind == DynKind.gate
        ? engine.grGate[_channel]
        : engine.grComp[_channel];
    return m != null && m.isFinite ? m : null;
  }

  @override
  ChannelPreampVm get preamp => ChannelPreampVm(
        hasInput: supports('gain'),
        gain: paramNum('gain') ?? 0,
        trim: paramNum('trim') ?? 0,
        phantom: paramBool('phantom') ?? false,
        invert: paramBool('polarity') ?? false,
        delayMs: paramNum('delay_ms') ?? 0,
        delayOn: paramBool('delay_on') ?? false,
      );

  @override
  ChannelSourceVm get source {
    final raw = ps.optimisticValue(_maps, _channel, 'source');
    final selected = paramNum('source_select')?.toInt();
    return ChannelSourceVm(
      srcName: raw is String ? raw : '—',
      sourceA: paramNum('source_a')?.toInt(),
      sourceB: paramNum('source_b')?.toInt(),
      // DL32S wire selector is 1=A / 0=B, verified against manufacturer mixer software.
      // The view model uses the displayed A/B index, 0=A / 1=B.
      selected: selected == null
          ? null
          : engine.driver == 'dl32s'
              ? 1 - selected
              : selected,
      usbReturn: paramNum('usbreturn')?.toInt(),
      levelDb: _levelDb.value,
    );
  }

  // A send-pan leaf alone never proves stereo; only a reported link on the destination does.

  bool _sendDestinationIsStereoAux(int slot) {
    if (slot < 1) return false;
    for (final family in engine.topology) {
      if (family['kind'] != 'bus') continue;
      final first = family['first'];
      final count = family['count'];
      if (first is! num || count is! num || slot > count.toInt()) continue;
      final destination = first.toInt() + slot - 1;
      return engine.valueKnown(destination, 'link') &&
          engine.params[destination]?['link'] == true;
    }
    return false;
  }

  double? sendPan(int slot) {
    if (!supports('send${slot}_pan')) return null;
    final value = paramNum('send${slot}_pan');
    return value == null ? null : value.clamp(-100, 100) / 100;
  }

  // DL32S source IDs stay opaque; only the verified A/B selector switches.

  @override
  void setSourceSelected(int selected) {
    if (selected == 0 || selected == 1) {
      cmdParam(
          'source_select', engine.driver == 'dl32s' ? 1 - selected : selected);
    }
  }

  @override
  void setSendPan(int slot, double pan) =>
      cmdParam('send${slot}_pan', pan.clamp(-1.0, 1.0) * 100);

  @override
  bool? get mainAssigned => paramBool('lr_assign');
  @override
  void setMainAssigned(bool on) => cmdParam('lr_assign', on);
  @override
  int get dcaCount => [
        for (var n = 1; n <= 16; n++)
          if (engine.supportedParams.contains('dca${n}_assign')) n
      ].fold(0, math.max);
  @override
  bool? dcaAssigned(int group) => paramBool('dca${group + 1}_assign');
  @override
  void setDcaAssigned(int group, bool on) =>
      cmdParam('dca${group + 1}_assign', on);

  @override
  void setCompSoftKnee(bool soft) => cmdParam(
      'comp_knee', engine.driver == 'dl32s' ? (soft ? 0 : 1) : (soft ? 3 : 0));

  int? _eqPresentationType(int band) {
    final native = paramNum('eq${band}_type')?.toInt();
    if (engine.driver != 'dl32s') return native;
    // Only live-verified DL32S shapes. Preserve raw values in the engine.
    return switch (native) {
      2 => EqFilterShape.bell.value,
      3 => EqFilterShape.lowShelf.value,
      4 => EqFilterShape.highShelf.value,
      _ => null,
    };
  }

  List<double>? _editableRange(String leaf) {
    if (!supports(leaf) || paramNum(leaf)?.isFinite != true) return null;
    return parameterRange(paramMetadata(leaf));
  }

  @override
  ChannelEqVm get eq => ChannelEqVm(
        on: paramBool('eq_on') ?? false,
        hpfF: paramNum('hpf_freq') ?? 80,
        hpfRange: _editableRange('hpf_freq'),
        hpfOn: paramBool('hpf_on') ?? false,
        hpfSlope: 12,
        selectedBand: _eqBand,
        bands: [
          for (var b = 1; b <= 4; b++)
            EqBandVm(
              band: b,
              name: const ['Low', 'Lo-Mid', 'Hi-Mid', 'High'][b - 1],
              f: paramNum('eq${b}_freq') ??
                  [80.0, 400.0, 1200.0, 6000.0][b - 1],
              g: paramNum('eq${b}_gain') ?? 0,
              q: paramNum('eq${b}_q') ?? 1,
              shelfUsesQ: engine.driver != 'dl32s',
              on: true,
              type: _eqPresentationType(b),
              fRange: _editableRange('eq${b}_freq'),
              gRange: _editableRange('eq${b}_gain'),
              qRange: _editableRange('eq${b}_q'),
            ),
        ],
      );

  ChannelDynVm _dyn(DynKind kind) {
    final p = kind == DynKind.gate ? 'gate' : 'comp';
    return ChannelDynVm(
      kind: kind,
      responseCurve: (paramMetadata('${p}_knee')?['curve'] ??
          paramMetadata('${p}_mode')?['curve']) as String?,
      ranges: {
        for (final prop in [
          'thr',
          'range',
          'att',
          'hold',
          'rel',
          'ratio',
          'knee',
          'makeup',
          'filter_freq',
          'mix'
        ])
          if (_editableRange('${p}_$prop') case final range?) prop: range,
      },
      on: paramBool('${p}_on') ?? false,
      thr: paramNum('${p}_thr') ?? (kind == DynKind.gate ? -50 : -20),
      range: paramNum('${p}_range') ?? 40,
      att: paramNum('${p}_att') ?? (kind == DynKind.gate ? 0.5 : 10),
      hold: paramNum('${p}_hold') ?? 50,
      rel: paramNum('${p}_rel') ?? (kind == DynKind.gate ? 150 : 120),
      ratioIsLimit: paramNum('${p}_ratio') != null &&
          paramNum('${p}_ratio') ==
              paramMetadata('${p}_ratio')?['limiter_value'],
      ratio: paramNum('${p}_ratio') ?? 3,
      knee: paramNum('${p}_knee') ?? 3,
      makeup: paramNum('${p}_makeup') ?? 0,
      mode: paramNum('${p}_mode')?.toInt(),
      detector: paramNum('${p}_det')?.toInt(),
      envelope: paramNum('${p}_env')?.toInt(),
      keySource: paramNum('${p}_key_src')?.toInt(),
      filterOn: paramBool('${p}_filter_on'),
      filterType: paramNum('${p}_filter_type')?.toInt(),
      filterFreq: paramNum('${p}_filter_freq'),
      mix: paramNum('${p}_mix'),
      auto: paramBool('${p}_auto'),
      softKnee: kind == DynKind.comp && paramNum('comp_knee') != null
          ? (engine.driver == 'dl32s'
              ? paramNum('comp_knee') == 0
              : paramNum('comp_knee')! > 0)
          : null,
    );
  }

  @override
  ChannelDynVm get gate => _dyn(DynKind.gate);

  @override
  ChannelDynVm get comp => _dyn(DynKind.comp);

  @override
  List<ChannelSendVm> get sends {
    final cached = _sendsCache;
    if (cached != null) return cached;
    final built = List<ChannelSendVm>.unmodifiable([
      for (var slot = 1; slot <= 16; slot++)
        if (engine.supportedParams.contains('send${slot}_level'))
          ChannelSendVm(
            slot: slot,
            destName: _sendName(slot),
            level: dbToPos(paramNum('send${slot}_level') ?? -90),
            muted: supports('send${slot}_on')
                ? !(paramBool('send${slot}_on') ?? true)
                : paramBool(_sendMuteLeaf(slot)) ?? false,
            canMute:
                supports('send${slot}_on') || supports(_sendMuteLeaf(slot)),
            pan: sendPan(slot),
            tap: paramNum('send${slot}_tap')?.toInt(),
            tapOptions: supports('send${slot}_tap')
                ? const [0, 1, 2, 3, 4, 5]
                : const [],
            grpon: paramBool('send${slot}_grpon'),
          ),
    ]);
    _sendsCache = built;
    return built;
  }

  String _sendMuteLeaf(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
    return slot <= auxCount ? 'aux${slot}_mute' : 'fx${slot - auxCount}_mute';
  }

  String _sendName(int slot) {
    final auxCount = engine.driver == 'dl32s' ? 8 : 6;
    final kind = slot <= auxCount ? 'bus' : 'fxsnd';
    final index = slot <= auxCount ? slot - 1 : slot - auxCount - 1;
    for (final family in engine.topology) {
      if (family['key'] != kind || index >= (family['count'] as num).toInt()) {
        continue;
      }
      final ch = (family['first'] as num).toInt() + index;
      for (final strip in engine.strips) {
        if (strip.ch == ch) return strip.name;
      }
    }
    return slot <= auxCount ? 'Aux $slot' : 'FX ${slot - auxCount}';
  }

  @override
  int get subgroupCount => engine.subgroupCount;

  @override
  int get muteGroupCount => engine.muteGroupCount;

  @override
  bool? subgroupAssigned(int g) => engine.subgroupAssign['$_channel:$g'];

  @override
  bool? muteGroupAssigned(int g) => engine.muteGroupAssign['$_channel:$g'];

  @override
  bool get locked => engine.locks.containsKey(_channel);

  @override
  bool get focused => _focusCh == _channel;

  @override
  ChannelTab get tab => _tab;

  @override
  void setTab(ChannelTab tab) {
    _tab = tab;
    notifyListeners();
  }

  @override
  void selectEqBand(int band) {
    _eqBand = band;
    notifyListeners();
  }

  @override
  void setEqParam(int band, String param, double value) =>
      cmdParam('eq${band}_$param', value);

  @override
  void setEqBandType(int band, EqFilterShape shape) {
    if (band != 1 && band != 4) return;
    if (shape != EqFilterShape.bell &&
        shape !=
            (band == 1 ? EqFilterShape.lowShelf : EqFilterShape.highShelf)) {
      return;
    }
    final leaf = 'eq${band}_type';
    if (!supports(leaf) ||
        EqFilterShape.fromValue(_eqPresentationType(band)) == null) {
      return;
    }
    final native = engine.driver == 'dl32s' && shape == EqFilterShape.lowShelf
        ? 3
        : shape.value;
    cmdParam(leaf, native);
  }

  @override
  void setEqOn(bool on) => cmdParam('eq_on', on);

  @override
  void setEqBandOn(int band, bool on) {
    if (band == 0) {
      cmdParam('hpf_on', on);
      return;
    }
    // Neither hardware driver has a per-band bypass leaf.
  }

  @override
  void setHpfSlope(int slope) {
    // The public VM uses dB/oct, while the catalogue uses a native enum.
    // Neither supported hardware driver advertises a verified slope control.
  }

  @override
  void resetEq() {
    for (var b = 1; b <= 4; b++) {
      cmdParam('eq${b}_g', 0.0);
    }
    notifyListeners();
  }

  @override
  void setDynParam(DynKind kind, String prop, double value) =>
      cmdParam('${kind.name}_$prop', value);

  @override
  void setDynOn(DynKind kind, bool on) => cmdParam('${kind.name}_on', on);

  @override
  void resetDyn(DynKind kind) {
    final defaults = kind == DynKind.gate ? dynResetGate : dynResetComp;
    for (final entry in defaults.entries) {
      if (kind == DynKind.comp && entry.key == 'knee') {
        setCompSoftKnee(true);
      } else {
        cmdParam('${kind.name}_${entry.key}', entry.value);
      }
    }
  }

  @override
  void setSendLevel(int slot, double value) {
    final db = posToDb(value.clamp(0.0, 1.0));
    cmdParam('send${slot}_level', db.isFinite ? db : -90.0);
  }

  @override
  void setSendTap(int slot, int tap) => cmdParam('send${slot}_tap', tap);

  @override
  void setSendMute(int slot, bool muted) {
    if (supports('send${slot}_on')) {
      cmdParam('send${slot}_on', !muted);
    } else {
      cmdParam(_sendMuteLeaf(slot), muted);
    }
  }

  @override
  void togglePhantom() => cmdParam('phantom', !(paramBool('phantom') ?? false));

  @override
  void toggleInvert() =>
      cmdParam('polarity', !(paramBool('polarity') ?? false));

  @override
  void setPreamp(String leaf, double value) => cmdParam(leaf, value);

  @override
  void dispose() {
    _libraryDisposed = true;
    _interruptAllLibrary('The engine was closed.');
    _interruptForgetSavedMixers('The engine was closed.');
    _libraryRequests.clear();
    _libraryRecalls.clear();
    _presetDisposed = true;
    _invalidatePresetOperations('The connection was closed.');
    _failNameEdits('The connection was closed.');
    _coalescer.clear();
    for (final m in _meters) {
      m.dispose();
    }
    _masterMeter.dispose();
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
