part of 'app_store.dart';

// Mixer banks, bus selection and stable strip control notifiers.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreStripsContract {
  Map<int, double> get _confirmedFader;
  Map<int, bool> get _confirmedMute;
  String get _bank;
  set _bank(String value);
  String get _busId;
  set _busId(String value);
  void _syncControlNotes();
  Map<String, (int, int)> get _bankRanges;
  List<MixerBank> get banks;
  ValueNotifier<double> faderOf(int ch);
  void setFader(int ch, double v);
  void _syncSendParamNotes();
}

mixin _AppStoreStrips on _AppStoreContext {
  // --- mixer control state ---
  @override
  final Map<int, double> _confirmedFader = {};
  @override
  final Map<int, bool> _confirmedMute = {};
  @override
  String _bank = 'view:all';
  @override
  String _busId = 'main';
  double _masterFader = 0.75;
  bool _masterMuted = false;

  // --- per-strip control notifiers (stable, updated in place) ---
  // Hot-path writes (fader drags, mute taps, pan drags) update these in
  // place and NEVER call notifyListeners(), so one strip rebuilds instead
  // of the whole shell + all strips. Global notifyListeners() is reserved
  // for structural changes (strips add/remove, bank/bus select, connection).
  // Solo keeps its notifier AND a global notify (infrequent taps; the page
  // solo banner reads hasSolo/soloCount off the store).
  final Map<int, ValueNotifier<String>> _controlGroupNotes = {};
  @override
  ValueListenable<String> controlGroupNoteOf(int ch) =>
      _controlGroupNotes.putIfAbsent(ch, () => ValueNotifier(''));

  final Map<int, ValueNotifier<double>> _faderNotes = {};
  final Map<int, ValueNotifier<bool>> _muteNotes = {};
  final Map<int, ValueNotifier<double>> _panNotes = {};
  final Map<int, ValueNotifier<bool>> _soloNotes = {};
  final Map<int, ValueNotifier<int>> _controlReadyNotes = {};

  // Advertised Solo descriptors carry verified readiness independently of the
  // sparse legacy Solo events. Providers without them still require an echo.
  bool canEditControl(int ch, String field) {
    if (field == 'fader' && !_hasFaderLaw) return false;
    if (field == 'solo') {
      final input = _inputSoloDescriptor(ch);
      if (input != null) {
        return _accessParamAllowed(ch, field) &&
            destinationControlIsCurrent(input) &&
            engine.locks.isEmpty;
      }
      if (ch < 0 || engine.strips.any((s) => s.ch == ch && s.key == 'main')) {
        return _targetSoloReady(ch);
      }
      if (_targetSolo(ch) != null) return _targetSoloReady(ch);
    }
    if (field != 'solo' && !_stripParamApplies(ch, field)) return false;
    return _accessParamAllowed(ch, field) &&
        (engine.driver == 'sim' ||
            (engine.connected &&
                engine.capsReady &&
                (engine.supportedParams.contains(field) || field == 'solo') &&
                engine.valueKnown(ch, field)));
  }

  int _controlReady(int ch) => ch < 0
      ? ((masterControlKnown('fader') ? 1 : 0) |
          (masterControlKnown('mute') ? 2 : 0) |
          (_targetSoloReady(-1) ? 8 : 0))
      : (canEditMixerFader(ch) ? 1 : 0) |
          (canEditControl(ch, 'mute') ? 2 : 0) |
          (canEditControl(ch, 'pan') ? 4 : 0) |
          (canEditControl(ch, 'solo') ? 8 : 0);

  ValueNotifier<int> controlReadyNote(int ch) => _controlReadyNotes.putIfAbsent(
        ch,
        () => ValueNotifier(_controlReady(ch)),
      );

  final Map<int, ValueNotifier<double>> _mixerFaderNotes = {};

  // The surface follows the selected destination, while the channel editor
  // retains its independent LR fader and explicit per-send controls.
  String? _mixerSendLevel(int ch) {
    if (!_busId.startsWith('bus')) return null;
    final target = masterProcessingTarget;
    for (final field in engine.supportedParams) {
      if (!RegExp(r'^send[1-9][0-9]*_level$').hasMatch(field) ||
          !_stripParamApplies(ch, field)) {
        continue;
      }
      final metadata =
          parameterPresentation(engine.parameterMetadata[field], (leaf) {
        final value = ps.optimisticValue(_maps, ch, leaf);
        return value is num ? value.toDouble() : null;
      });
      final destination = metadata?['destination'];
      if (destination is Map && destination['key'] == target) return field;
    }
    return null;
  }

  bool canEditMixerFader(int ch) {
    if (!_busId.startsWith('bus')) return canEditControl(ch, 'fader');
    final field = _mixerSendLevel(ch);
    return field != null &&
        engine.connected &&
        engine.capsReady &&
        _accessParamAllowed(ch, field) &&
        engine.valueKnown(ch, field);
  }

  double _mixerFaderValue(int ch) {
    if (!_busId.startsWith('bus')) return faderOf(ch).value;
    final field = _mixerSendLevel(ch);
    final raw = field == null ? null : ps.optimisticValue(_maps, ch, field);
    return raw is num ? dbToPos(raw.toDouble()) : 0;
  }

  @override
  ValueNotifier<double> mixerFaderOf(int ch) => _mixerFaderNotes.putIfAbsent(
      ch, () => ValueNotifier(_mixerFaderValue(ch)));

  @override
  void setMixerFader(int ch, double value) {
    if (!_busId.startsWith('bus')) {
      setFader(ch, value);
      return;
    }
    if (!canEditMixerFader(ch)) return;
    final field = _mixerSendLevel(ch)!;
    final level = value.clamp(0.0, 1.0);
    final metadata =
        parameterPresentation(engine.parameterMetadata[field], (leaf) {
      final value = ps.optimisticValue(_maps, ch, leaf);
      return value is num ? value.toDouble() : null;
    });
    final floor = (metadata?['min'] as num?)?.toDouble();
    final db = posToDb(level);
    if (!db.isFinite && floor == null) return;
    final protocolValue = db.isFinite ? db : floor!;
    ps.noteOptimistic(_maps, ch, field, protocolValue, _nowMs());
    mixerFaderOf(ch).value = level;
    _coalescer.offer(
        'param:$ch:$field', () => _send(Cmds.param(ch, field, protocolValue)));
    _invalidateDerived();
  }

  int? get _selectedSubgroup => _busId.startsWith('subgroup')
      ? int.tryParse(_busId.substring('subgroup'.length))
      : null;

  bool masterControlKnown(String field) {
    if (field == 'fader' && !_hasFaderLaw) return false;
    if (_busId.startsWith('solo:') || masterProcessingTarget.isEmpty) {
      return false;
    }
    if (!accessTargetAllowed(masterProcessingTarget)) return false;
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
                        int.tryParse(_busId.replaceFirst('bus', '')),
                      )
                    : engine.busMuted.containsKey(
                        int.tryParse(_busId.replaceFirst('bus', '')),
                      ))));
  }

  final ValueNotifier<double> _masterFaderNote = ValueNotifier(0.75);
  final ValueNotifier<bool> _masterMutedNote = ValueNotifier(false);

  // Raw engineering values, keyed by channel + canonical send field.
  final Map<(int, String), ValueNotifier<double?>> _sendParamNotes = {};

  // ------------------------------------------------------------------
  // MixerStore
  // ------------------------------------------------------------------
  @override
  List<Strip> get assignmentStrips => List.unmodifiable(engine.strips);

  @override
  bool isMixerStripVisible(Strip strip) {
    // A stale saved layer or legacy state snapshot cannot extend the current
    // provider's declared geometry. Never derive channel existence from an
    // unhydrated control value (or a permission/lock readiness bit).
    if (engine.topology.isNotEmpty &&
        !engine.topology.any((family) {
          final first = (family['first'] as num).toInt();
          final count = (family['count'] as num).toInt();
          return strip.ch >= first && strip.ch < first + count;
        })) {
      return false;
    }
    if (!engine.capsReady) return true;
    // Sends-on-faders is a surface of source/destination routes, not every
    // strip in the bank. A source without this route has no fader to render.
    return _busId.startsWith('bus')
        ? _mixerSendLevel(strip.ch) != null
        : _stripParamApplies(strip.ch, 'fader');
  }

  @override
  List<Strip> get strips {
    final cached = _stripsCache;
    if (cached != null) return cached;
    final range = _bankRanges[_bank];
    final leaf = _selectedViewLeaf;
    final Iterable<Strip> visible;
    if (leaf != null) {
      visible = _viewMembershipReady(leaf)
          ? _viewCandidates(leaf).where((s) => _viewMember(s.ch, leaf) == true)
          : const <Strip>[];
    } else if (_bank.startsWith('custom:')) {
      final id = _bank.substring(7);
      final view = customMixerViews.where((v) => v.id == id).firstOrNull;
      visible =
          engine.strips.where((s) => view?.stripKeys.contains(s.key) ?? false);
    } else if (_bank == 'view:all') {
      visible = engine.topology.isEmpty
          ? engine.strips
          : engine.strips.where((strip) => engine.topology.any((family) {
                final first = (family['first'] as num).toInt();
                final count = (family['count'] as num).toInt();
                return strip.ch >= first &&
                    strip.ch < first + count &&
                    (family['kind'] == 'in' || family['kind'] == 'ret');
              }));
    } else {
      visible = range == null
          ? engine.strips
          : engine.strips.where((s) => s.ch >= range.$1 && s.ch < range.$2);
    }
    final built = List<Strip>.unmodifiable(
      visible.where(isMixerStripVisible).map(
            (s) => Strip(
              ch: s.ch,
              key: s.key,
              name: s.name,
              fader: _stripFaderValue(s.ch, s.fader),
              muted: _stripMuteValue(s.ch, s.muted),
            ),
          ),
    );
    _stripsCache = built;
    return built;
  }

  List<String> get _viewLeaves {
    final pattern = RegExp(r'^view([1-9][0-9]*)_assign$');
    final leaves = engine.supportedParams.where(pattern.hasMatch).toList();
    leaves.sort(
      (a, b) => int.parse(pattern.firstMatch(a)![1]!)
          .compareTo(int.parse(pattern.firstMatch(b)![1]!)),
    );
    return leaves;
  }

  String? get _selectedViewLeaf =>
      _bank.startsWith('view:') && _bank != 'view:all'
          ? _bank.substring(5)
          : null;

  String _viewLabel(String leaf) =>
      engine.parameterMetadata[leaf]?['label'] as String? ??
      'View ${RegExp(r'\d+').firstMatch(leaf)?.group(0) ?? ''}';

  // A provider may expose View memberships on only some strip families.
  // Hydration/readiness must not wait for unsupported leaves on other families.
  Iterable<Strip> _viewCandidates(String leaf) {
    final scopes = engine.parameterMetadata[leaf]?['scopes'];
    return engine.strips.where((strip) {
      if (strip.key == 'main') return false;
      if (scopes is! List || scopes.isEmpty) return true;
      for (final family in engine.topology) {
        final first = (family['first'] as num?)?.toInt() ?? 0;
        final count = (family['count'] as num?)?.toInt() ?? 0;
        if (strip.ch < first || strip.ch >= first + count) continue;
        final tags = family['params_scope'];
        return tags is List
            ? tags.any(scopes.contains)
            : scopes.contains(family['kind']);
      }
      return false;
    });
  }

  bool? _viewMember(int ch, String leaf) {
    if (!engine.valueKnown(ch, leaf)) return null;
    final value = ps.optimisticValue(_maps, ch, leaf);
    return value is bool ? value : null;
  }

  bool _viewMembershipReady(String leaf) =>
      engine.connected &&
      engine.capsReady &&
      engine.supportedParams.contains(leaf) &&
      engine.strips.isNotEmpty &&
      _viewCandidates(leaf).every((s) => _viewMember(s.ch, leaf) != null);

  @override
  String get emptyBankMessage {
    final leaf = _selectedViewLeaf;
    if (_bank.startsWith('custom:')) return 'No channels in this view.';
    if (leaf == null) return 'Waiting for channels…';
    final label = _viewLabel(leaf);
    if (!_viewMembershipReady(leaf)) return 'Waiting for $label membership…';
    final scopes = engine.parameterMetadata[leaf]?['scopes'];
    final inputs =
        scopes is List && scopes.isNotEmpty && scopes.every((s) => s == 'in');
    return 'No ${inputs ? 'input channels' : 'channels'} in $label.';
  }

  @override
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
                math.min(offset + 16, (f['count'] as num).toInt()),
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
  bool soloOf(int ch) =>
      _targetSolo(ch) != null ? _hardSoloValue(ch) : engine.solos.contains(ch);

  @override
  bool get hasSolo => _hasAnySolo;
  @override
  int get soloCount => _hardSoloCount;

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
    _soloNotes.putIfAbsent(ch, () => ValueNotifier(soloOf(ch)));
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
      if (f.value != uiFader && !(f.value.isNaN && uiFader.isNaN)) {
        f.value = uiFader;
      }
      final m = _muteNotes[ch]!;
      final muted = _stripMuteValue(ch, s.muted);
      if (m.value != muted) m.value = muted;
    }
    final surface = _mixerFaderNotes[ch];
    if (surface != null) {
      final value = _mixerFaderValue(ch);
      if (!(surface.value.isNaN && value.isNaN)) surface.value = value;
    }
    final p = _panNotes[ch]!;
    final pv = panOf(ch);
    if (p.value != pv) p.value = pv;
    final so = _soloNotes[ch]!;
    final sv = soloOf(ch);
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
      key,
      () => ValueNotifier(_sendParamValue(key.$1, key.$2)),
    );
  }

  @override
  void _syncSendParamNotes() {
    for (final entry in _sendParamNotes.entries) {
      entry.value.value = _sendParamValue(entry.key.$1, entry.key.$2);
    }
  }

  @override
  void _syncControlNotes() {
    _syncSendParamNotes();
    for (final entry in _controlGroupNotes.entries) {
      entry.value.value = _controlGroupInfluence(entry.key);
    }
    for (final entry in _controlReadyNotes.entries) {
      entry.value.value = _controlReady(entry.key);
    }
    for (final s in engine.strips) {
      _pushStripNote(s.ch);
    }
    final masterSolo = _soloNotes[-1];
    if (masterSolo != null) masterSolo.value = soloOf(-1);
    final mf = masterFader;
    if (_masterFaderNote.value != mf &&
        !(_masterFaderNote.value.isNaN && mf.isNaN)) {
      _masterFaderNote.value = mf;
    }
    final mm = masterMuted;
    if (_masterMutedNote.value != mm) _masterMutedNote.value = mm;
  }

  @override
  double get masterFader => _protocolFaderToUi(
        _selectedSubgroup != null
            ? engine.subgroupFaders[_selectedSubgroup] ?? 0
            : _busId == 'main'
                ? engine.mainFader ?? _masterFader
                : engine.busFaders[
                        int.tryParse(_busId.replaceFirst('bus', ''))] ??
                    0,
      );

  @override
  bool get masterMuted => _selectedSubgroup != null
      ? engine.subgroupMuted[_selectedSubgroup] ?? false
      : _busId == 'main'
          ? engine.mainMuted ?? _masterMuted
          : engine.busMuted[int.tryParse(_busId.replaceFirst('bus', ''))] ??
              false;

  String _busDisplayName(int index) {
    final key = 'bus/${(index + 1).toString().padLeft(2, '0')}';
    final name =
        engine.strips.where((s) => s.key == key).firstOrNull?.name.trim();
    if (name != null && name.isNotEmpty) return name;
    // Providers without bus strips expose the desk name on destination controls.
    for (final control in engine.destinationControls) {
      final reported = control.targetName?.trim();
      if (control.target == key && reported != null && reported.isNotEmpty) {
        return reported;
      }
    }
    return 'BUS ${index + 1}';
  }

  @override
  String get masterName => _busId.startsWith('solo:')
      ? (_targetSolo(-1)?.targetLabel ?? 'SOLO')
      : _selectedSubgroup != null
          ? 'SUBGROUP ${_selectedSubgroup! + 1}'
          : _busId == 'main'
              ? 'MAIN'
              : _busDisplayName(
                  int.tryParse(_busId.replaceFirst('bus', '')) ?? 0);

  @override
  List<MixerBank> get banks {
    final cached = _banksCache;
    if (cached != null) return cached;
    final built = List<MixerBank>.unmodifiable([
      MixerBank(
        key: 'view:all',
        label: 'All',
        color: const ConsolePalette().lime,
        text: const ConsolePalette().onAccent,
      ),
      for (final view in customMixerViews)
        MixerBank(
            key: 'custom:${view.id}',
            label: view.name,
            color: const ConsolePalette().lime,
            text: const ConsolePalette().onAccent),
      for (final entry in _bankRanges.entries)
        MixerBank(
          key: entry.key,
          label: entry.key.startsWith('ch')
              ? 'Ch ${entry.value.$1 + 1}–${entry.value.$2}'
              : engine.topology.firstWhere(
                  (f) =>
                      (f['first'] as num).toInt() <= entry.value.$1 &&
                      (f['first'] as num).toInt() +
                              (f['count'] as num).toInt() >
                          entry.value.$1,
                )['label'] as String,
          color: const ConsolePalette().lime,
          text: const ConsolePalette().onAccent,
        ),
      if (_viewLeaves.isNotEmpty) ...[
        for (final leaf in _viewLeaves)
          MixerBank(
            key: 'view:$leaf',
            label: _viewLabel(leaf),
            color: const ConsolePalette().lime,
            text: const ConsolePalette().onAccent,
          ),
      ],
    ]);
    _banksCache = built;
    return built;
  }

  String get _mixerViewScope =>
      '${engine.driver}:${engine.model}:${engine.linkIp}';

  @override
  List<CustomMixerView> get customMixerViews =>
      PrefsService.instance.mixerViews(_mixerViewScope);

  @override
  Future<void> saveCustomMixerViews(List<CustomMixerView> views) async {
    final scope = _mixerViewScope;
    await PrefsService.instance.saveMixerViews(scope, views);
    if (scope != _mixerViewScope) return;
    if (_bank.startsWith('custom:') &&
        !views.any((v) => 'custom:${v.id}' == _bank)) {
      _bank = 'view:all';
    }
    _invalidateDerived();
    notifyListeners();
  }

  @override
  String get bank => _bank;

  @override
  List<MixerBusRow> get busRows {
    final cached = _busRowsCache;
    if (cached != null) return cached;
    final built = List<MixerBusRow>.unmodifiable([
      if (_primaryMainTarget.isNotEmpty) MixerBusRow(
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
          name: _busDisplayName(b),
          sub: engine.busMuted[b] == true ? 'muted' : 'open',
          muted: engine.busMuted[b] ?? false,
          active: _busId == 'bus$b',
          meter: _busMeters.putIfAbsent('bus$b', () => ValueNotifier(double.nan)),
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
            // Stable target-keyed meter feed, zero while unavailable.
            meter: _busMeters.putIfAbsent(
              'subgroup$group',
              () => ValueNotifier(double.nan),
            ),
          ),
      ..._soloBusRows,
    ]);
    final allowed = List<MixerBusRow>.unmodifiable(
      built.where(
        (row) => accessTargetAllowed(_processingTargetForAccessBus(row.id)),
      ),
    );
    _busRowsCache = allowed;
    return allowed;
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
    if (_targetSolo(ch) != null) {
      _writeTargetSolo(ch);
      return;
    }
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
  bool get canClearSolos =>
      accessFamilyAllowed(AccessFamily.channelSolo) &&
      !blockedAccess.any((key) => key.startsWith('target:')) &&
      (engine.clearSoloScope != 'advertised' || _advertisedSoloClearReady);

  @override
  void clearSolos() {
    if (!canClearSolos) return;
    if (engine.clearSoloScope == 'advertised') {
      if (_advertisedSoloClearReady) _send(Cmds.clearSolo());
      return;
    }
    if (engine.driver != 'sim' &&
        (!engine.connected ||
            !engine.knownValues.keys.any(
              (ch) => engine.valueKnown(ch, 'solo'),
            ))) {
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
      _coalescer.offer(
        'subgroup_fader:$group',
        () => _send(Cmds.subgroupFader(group, protocolValue)),
      );
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
        'bus_fader:$bus',
        () => _send(Cmds.busFader(bus, protocolValue)),
      );
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
        'subgroup_mute:$group',
        () => _send(Cmds.subgroupMute(group, muted)),
      );
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
    if (!accessTargetAllowed(_processingTargetForAccessBus(id))) return;
    if (_busId == id || !busRows.any((row) => row.id == id)) return;
    _busId = id;
    _syncControlNotes();
    _syncMeters(_nowMs());
    _invalidateDerived();
    notifyListeners();
  }

  @override
  void setBank(String key) {
    if (!banks.any((bank) => bank.key == key)) return;
    _bank = key;
    _invalidateDerived();
    notifyListeners();
  }
}
