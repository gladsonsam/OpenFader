part of 'app_store.dart';

// Channel names, colors, icons and link edits with acknowledgement tracking.
// This contract exposes only state and hooks needed by other responsibilities.
// Private access stays inside the AppStore library; screens use store contracts.
abstract interface class _AppStoreAppearanceContract {
  Map<
      String,
      ({
        ChannelNameTarget target,
        String field,
        Object value,
        double deadline,
        Completer<String?> done
      })> get _appearanceEdits;
  Map<String, String> get _nameEditValues;
  Map<String, double> get _nameEditDeadlines;
  String? _appearanceEditError(ChannelNameTarget target, String field);
  void _finishAppearanceEdit(String key, String? error);
  void _finishNameEdit(String key, String? error);
  void _failNameEdits(String message);
  Map<String, Completer<String?>> get _nameEdits;
}

mixin _AppStoreAppearance on _AppStoreContext {
  @override
  final Map<
      String,
      ({
        ChannelNameTarget target,
        String field,
        Object value,
        double deadline,
        Completer<String?> done
      })> _appearanceEdits = {};
  @override
  final Map<String, Completer<String?>> _nameEdits = {};
  @override
  final Map<String, String> _nameEditValues = {};
  @override
  final Map<String, double> _nameEditDeadlines = {};

  @override
  ChannelNameTarget? captureNameTarget([int? channel]) {
    final ch = channel ?? _channel;
    for (final strip in engine.strips) {
      if (strip.ch != ch) continue;
      final rawName = ps.optimisticValue(_maps, ch, 'name');
      final target = ChannelNameTarget(
          engine.session, ch, strip.key, rawName is String ? rawName : null);
      return target;
    }
    return null;
  }

  @override
  String? nameEditError(ChannelNameTarget target) {
    if (!_accessParamAllowed(target.channel, 'name')) {
      return 'Channel identity is limited on this device.';
    }
    if (!engine.connected || engine.session != target.session) {
      return 'Connection changed or lost. Reopen the name editor.';
    }
    if (engine.locks.containsKey(target.channel)) {
      return 'This channel is locked by another controller.';
    }
    if (!engine.capsReady || !engine.supportedParams.contains('name')) {
      return 'Channel naming is unavailable on this connection.';
    }
    if (target.name == null ||
        ps.optimisticValue(_maps, target.channel, 'name') is! String ||
        !engine.valueKnown(target.channel, 'name') ||
        !engine.strips
            .any((s) => s.ch == target.channel && s.key == target.key)) {
      return 'The channel name is not available. Reopen the name editor.';
    }
    return null;
  }

  @override
  List<ChannelColorChoice> colorsForChannel(ChannelNameTarget target) =>
      channelColorChoices(engine.parameterMetadata['color']);

  @override
  double? colorForChannel(ChannelNameTarget target) {
    final value = ps.optimisticValue(_maps, target.channel, 'color');
    return value is num && value.isFinite ? value.toDouble() : null;
  }

  ChannelColorChoice? channelColorChoice(int channel) {
    final value = ps.optimisticValue(_maps, channel, 'color');
    return channelColorChoices(engine.parameterMetadata['color'])
        .where((c) => value is num && (c.value - value).abs() < .0001)
        .firstOrNull;
  }

  @override
  List<ChannelIconChoice> iconsForChannel(ChannelNameTarget target) =>
      channelIconChoices(engine.parameterMetadata['icon']);

  @override
  double? iconForChannel(ChannelNameTarget target) {
    final value = ps.optimisticValue(_maps, target.channel, 'icon');
    return value is num && value.isFinite ? value.toDouble() : null;
  }

  @override
  int? linkPartnerForChannel(ChannelNameTarget target) {
    for (final pair in engine.stereoPairs) {
      if (pair.contains(target.channel)) {
        return pair.first == target.channel ? pair.last : pair.first;
      }
    }
    return null;
  }

  @override
  bool? linkedForChannel(ChannelNameTarget target) {
    final value = ps.optimisticValue(_maps, target.channel, 'link');
    return value is bool ? value : null;
  }

  @override
  String? linkEditError(ChannelNameTarget target) =>
      _appearanceEditError(target, 'link');

  @override
  Future<String?> linkChannel(ChannelNameTarget target, bool linked) =>
      _appearanceChannel(target, 'link', linked);

  bool get hasChannelIconChoices =>
      channelIconChoices(engine.parameterMetadata['icon']).isNotEmpty;

  ChannelIconChoice? channelIconChoice(int channel) {
    final value = ps.optimisticValue(_maps, channel, 'icon');
    return channelIconChoices(engine.parameterMetadata['icon'])
        .where((c) => value is num && (c.value - value).abs() < .0001)
        .firstOrNull;
  }

  Iterable<double> _appearanceChoices(ChannelNameTarget target, String field) =>
      field == 'icon'
          ? iconsForChannel(target).map((c) => c.value)
          : colorsForChannel(target).map((c) => c.value);

  @override
  String? colorEditError(ChannelNameTarget target) =>
      _appearanceEditError(target, 'color');
  @override
  String? iconEditError(ChannelNameTarget target) =>
      _appearanceEditError(target, 'icon');

  @override
  String? _appearanceEditError(ChannelNameTarget target, String field) {
    if (!_accessParamAllowed(target.channel, field)) {
      return 'This control is limited on this device.';
    }
    if (!engine.connected ||
        engine.session != target.session ||
        !engine.strips
            .any((s) => s.ch == target.channel && s.key == target.key)) {
      return 'Connection changed or lost. Reopen the $field editor.';
    }
    if (engine.locks.containsKey(target.channel)) {
      return 'This channel is locked by another controller.';
    }
    if (!engine.capsReady ||
        !engine.supportedParams.contains(field) ||
        !engine.valueKnown(target.channel, field) ||
        (field == 'link'
            ? engine.locks.containsKey(linkPartnerForChannel(target)) ||
                linkedForChannel(target) == null ||
                (engine.parameterMetadata[field]?['kind']) != 'bool'
            : _appearanceChoices(target, field).isEmpty) ||
        _presetEditingBlocked(target.channel, field)) {
      return field == 'link'
          ? 'Stereo linking is unavailable on this connection.'
          : 'Channel ${field}s are unavailable on this connection.';
    }
    return null;
  }

  @override
  void _finishAppearanceEdit(String key, String? error) {
    final edit = _appearanceEdits.remove(key);
    if (edit == null) return;
    _maps.pending.remove(key);
    edit.done.complete(error);
  }

  @override
  Future<String?> colorChannel(ChannelNameTarget target, double value) =>
      _appearanceChannel(target, 'color', value);
  @override
  Future<String?> iconChannel(ChannelNameTarget target, double value) =>
      _appearanceChannel(target, 'icon', value);

  Future<String?> _appearanceChannel(
      ChannelNameTarget target, String field, Object value) async {
    final error = _appearanceEditError(target, field);
    if (error != null) return error;
    if (field == 'link'
        ? value is! bool
        : value is! double ||
            !value.isFinite ||
            !_appearanceChoices(target, field).contains(value)) {
      return 'Choose from the $field choices provided by this mixer.';
    }
    final key = ps.pendingKey(target.channel, field);
    if (_appearanceEdits.containsKey(key) || _maps.pending.containsKey(key)) {
      return 'A $field change is already pending.';
    }
    final done = Completer<String?>();
    _appearanceEdits[key] = (
      target: target,
      field: field,
      value: value,
      // Linking copies a complete shared channel state through multiple
      // acknowledged writes; it needs a longer receipt budget than a swatch.
      deadline: _nowMs() + (field == 'link' ? 90000 : ps.confirmTimeoutMs),
      done: done
    );
    ps.noteOptimistic(_maps, target.channel, field, value, _nowMs());
    notifyListeners();
    unawaited(Future<void>.sync(
            () => _command(_clientId, Cmds.param(target.channel, field, value)))
        .then((_) {}, onError: (Object e) {
      if (identical(_appearanceEdits[key]?.done, done)) {
        _finishAppearanceEdit(key, 'Could not send channel $field: $e');
      }
    }));
    return done.future;
  }

  @override
  void _finishNameEdit(String key, String? error) {
    final edit = _nameEdits.remove(key);
    if (edit == null) return;
    _nameEditValues.remove(key);
    _nameEditDeadlines.remove(key);
    _maps.pending.remove(key);
    edit.complete(error);
  }

  @override
  void _failNameEdits(String message) {
    for (final key in _appearanceEdits.keys.toList()) {
      _finishAppearanceEdit(
          key,
          message.replaceAll(
              'name editor', '${_appearanceEdits[key]!.field} editor'));
    }
    for (final key in _nameEdits.keys.toList()) {
      _finishNameEdit(key, message);
    }
  }

  @override
  bool nameCanBeEmpty(ChannelNameTarget target) =>
      parameterTextRules(engine.parameterMetadata['name'])?['allow_empty'] ==
      true;

  @override
  Future<String?> renameChannel(ChannelNameTarget target, String name) async {
    final unavailable = nameEditError(target);
    if (unavailable != null) return unavailable;
    final rules = parameterTextRules(engine.parameterMetadata['name']);
    if ((!nameCanBeEmpty(target) && name.trim().isEmpty) ||
        name.runes.any((r) =>
            r < 32 || (r >= 127 && r <= 159) || (r >= 0xd800 && r <= 0xdfff))) {
      return 'Enter a name without control characters or invalid Unicode.';
    }
    final maxBytes = rules?['max_utf8_bytes'];
    if (maxBytes is num &&
        maxBytes.isFinite &&
        utf8.encode(name).length > maxBytes) {
      return 'This name is too long for this mixer. Try a shorter name.';
    }
    if (rules?['allow_edge_whitespace'] == false && name != name.trim()) {
      return 'Remove spaces at the beginning or end of the name.';
    }
    final current = ps.optimisticValue(_maps, target.channel, 'name');
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
            () => _command(_clientId, Cmds.param(target.channel, 'name', name)))
        .then((_) {}, onError: (Object e) {
      // A late transport failure must not cancel a newer edit of this channel.
      if (identical(_nameEdits[key], done)) {
        _finishNameEdit(key, 'Could not rename channel: $e');
      }
    }));
    return done.future;
  }
}
