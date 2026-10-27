part of 'app_store.dart';

// Single shell-driven engine drain and confirmation dispatch. Owns no timer.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStorePollContract {
  Future<void> pollMeters();
}

mixin _AppStorePoll on _AppStoreContext {
  /// Engine wall-clock (ms) of the last poll batch carrying any engine
  /// frames (control or meters). Meters stream at ~20 Hz on a live link,
  /// so staleness here is a reliable dead-link signal for resume verdicts.
  double _lastTrafficMs = 0;
  double get lastTrafficMs => _lastTrafficMs;

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
    _pollDestinationEdits(now);
    _pollAssignmentEdit(now);
    for (final key in _nameEdits.keys.toList()) {
      if (now >= (_nameEditDeadlines[key] ?? double.negativeInfinity)) {
        _finishNameEdit(key,
            'The mixer did not acknowledge the name change. It may have changed.');
      }
    }
    for (final entry in _appearanceEdits.entries.toList()) {
      final error = _appearanceEditError(entry.value.target, entry.value.field);
      if (error != null || now >= entry.value.deadline) {
        _finishAppearanceEdit(
            entry.key,
            error ??
                'The mixer did not acknowledge the ${entry.value.field} change. It may have changed.');
      }
    }
    _pollLibrary(now);
    _pollNativeLibrary(now);
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
      _interruptAssignmentEdit(
          'Connection lost. The assignment result is uncertain.');
      _failNameEdits(
          'Connection lost while waiting for the name confirmation: $e');
      _noteTransportError(e);
      _syncSpectrum();
      return; // never crash the meter loop on a transport hiccup
    }
    var dirty = false;
    var controlsTouched = false;
    if (frames.isNotEmpty) _lastTrafficMs = _nowMs();
    for (final f in frames) {
      try {
        final e = jsonDecode(f);
        if (e is! Map<String, dynamic>) continue;
        final previousSession = engine.session;
        final controlOnly = _controlOnlyFrame(e);
        var frameDirty = engine.applyFrame(e);
        if (e['type'] == 'caps') _observeInputPresetCatalogue();
        _observeOutputProcessingContext();
        if (e['type'] == 'input_processor_result') {
          _receiveInputProcessorResult(e);
        }
        if (e['type'] == 'destination_control_result') {
          _receiveDestinationResult(e);
        }
        if (e['type'] == 'native_library_result') {
          _receiveNativeLibrary(e);
          frameDirty = true;
        }
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
        _receiveAssignmentEvent(e);
        _confirmEcho(e);
        if (e['type'] == 'error') {
          _errorRevision++;
          _engineErrorAt = DateTime.now();
          _stopPresetRecalls(
              'The mixer reported an error during recall: ${e['message'] ?? 'Unknown error'}. '
              'The last setting may have changed; its result is uncertain.');
          final ch = (e['ch'] as num?)?.toInt();
          for (final key in _appearanceEdits.keys.toList()) {
            if (ch == null || ps.splitKey(key)?.ch == ch) {
              _finishAppearanceEdit(key,
                  'Backend rejected the ${_appearanceEdits[key]!.field} change: ${e['message'] ?? 'Unknown error'}');
            }
          }
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
    if (dirty) {
      _invalidateDerived();
      if (_bank.startsWith('view:') && !banks.any((b) => b.key == _bank)) {
        _bank = _bankRanges.keys.firstOrNull ?? 'ch1';
        _invalidateDerived();
      }
    }
    if (controlsTouched || dirty) _syncControlNotes();
    if (dirty) notifyListeners();
  }

  // Binary fan-in for the meter hot path; bad frames decode to null and are skipped.
  void applyBinaryFrame(Uint8List bytes) {
    final f = parseBinary(bytes);
    if (f == null) return;
    _lastTrafficMs = _nowMs();
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
      case 'param_invalidated':
        final ch = (e['ch'] as num?)?.toInt();
        final fields = e['fields'];
        if (ch == null || fields is! List) return;
        for (final field in fields.whereType<String>()) {
          _maps.confirmed[ch]?.remove(field);
          _maps.pending.remove(ps.pendingKey(ch, field));
          _presetVerifiedFields[ch]?.remove(field);
        }
        _invalidateDerived();
        break;
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
}
