import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/services.dart' show Clipboard, ClipboardData;

import '../widgets/inline_feedback.dart';

import 'dart:convert';
import 'dart:io';
import 'dart:math';

import 'package:file_picker/file_picker.dart';
import 'package:path/path.dart' as path;
import 'package:shared_preferences/shared_preferences.dart';

import '../core/native_library_store.dart';

import '../ui/console.dart';
import 'recorder_arrangement.dart';

/// Recorder data and commands are supplied by the shell's host adapter.
abstract class RecorderStore extends ChangeNotifier {
  bool get canAdminRemote;
  Map<String, dynamic> get remoteStatus;
  List<String> get remoteClients;
  String? get remoteError;
  bool get remotePending;
  Future<void> remoteAction(String action, [String? value]);
  bool get connected;
  bool get pending;
  String? get error;
  Map<String, dynamic> get state;
  List<Map<String, dynamic>> get devices;
  List<Map<String, dynamic>> get takes;
  Map<int, String> get channelNames;
  Future<void> command(String action, [Map<String, dynamic> args = const {}]);
  Future<bool> quitHost();
  Future<void> refreshDevices();
  Future<Map<String, dynamic>> inspectTake(String takeId);
  Future<Map<String, dynamic>> waveform(
    String takeId,
    int start,
    int end,
    int buckets,
  );
}

// Host supplies state.channels and take.channels (or manifest.channels), 1..64.
// Optional channel_names is ordered; soundcheck_reason explains unavailable VSC.
int recorderChannelCount(dynamic value) {
  if (value is Map) {
    return recorderChannelCount(value['channels'] ?? value['manifest']);
  }
  if (value is List) return value.length.clamp(0, 64);
  if (value is num &&
      value == value.roundToDouble() &&
      value >= 1 &&
      value <= 64) {
    return value.toInt();
  }
  return 0;
}

Map<int, String> recorderChannelNames(Map source) {
  final manifest = source['manifest'];
  final names =
      source['channel_names'] ??
      (manifest is Map ? manifest['channel_names'] : null);
  final channels =
      source['channels'] ?? (manifest is Map ? manifest['channels'] : null);
  final list = names is List
      ? names
      : channels is List
      ? channels
      : null;
  return {
    // A connected network mixer's strips do not establish the audio interface's
    // USB send mapping. Only names carried by this capture/take are authoritative.
    if (list != null)
      for (var i = 0; i < list.length && i < 64; i++)
        if (list[i] is String && (list[i] as String).isNotEmpty)
          i: list[i] as String
        else if (list[i] is Map &&
            list[i]['name'] is String &&
            (list[i]['name'] as String).isNotEmpty)
          i: list[i]['name'] as String,
  };
}

/// Track-to-destination mapping; conflicts are shown on the affected rows.
class _TrackMappingDialog extends StatefulWidget {
  const _TrackMappingDialog({
    required this.title,
    required this.trackNames,
    required this.options,
    required this.mapping,
    required this.confirmLabel,
    required this.destination,
    this.header = const [],
    this.confirmation,
  });

  final String title, confirmLabel;

  /// Singular noun for a mapping target, such as "output".
  final String destination;
  final List<Widget> header;
  final String? confirmation;
  final Map<int, String> trackNames;
  final List<(int, String)> options;
  final List<int?> mapping;

  @override
  State<_TrackMappingDialog> createState() => _TrackMappingDialogState();
}

class _TrackMappingDialogState extends State<_TrackMappingDialog> {
  late final _mapping = List<int?>.of(widget.mapping);
  late final _options = [(-1, 'Unassigned'), ...widget.options];
  bool _submitted = false;

  String _track(int track) => widget.trackNames[track] == null
      ? 'Track ${track + 1}'
      : '${track + 1} · ${widget.trackNames[track]}';

  String _option(int value) =>
      _options.where((o) => o.$1 == value).firstOrNull?.$2 ?? '$value';

  @override
  Widget build(BuildContext context) {
    final assigned = _mapping.whereType<int>().toList();
    final shared = <int, List<int>>{};
    for (var track = 0; track < _mapping.length; track++) {
      final value = _mapping[track];
      if (value != null) (shared[value] ??= []).add(track);
    }
    shared.removeWhere((_, tracks) => tracks.length < 2);
    final conflicts = shared.values.fold(0, (n, tracks) => n + tracks.length);
    final error = assigned.isEmpty
        ? 'Assign at least one track.'
        : conflicts > 0
        ? 'Give each track its own ${widget.destination}.'
        : null;
    return ConsoleDialog(
      title: widget.title,
      content: SizedBox(
        width: 420,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            for (final (index, line) in widget.header.indexed) ...[
              if (index > 0) const SizedBox(height: AppDesign.spaceXs),
              line,
            ],
            if (widget.header.isNotEmpty)
              const SizedBox(height: AppDesign.spaceMd),
            Text.rich(
              TextSpan(
                text:
                    '${assigned.length} of ${_mapping.length} tracks assigned',
                children: [
                  if (conflicts > 0)
                    TextSpan(
                      text: ' · $conflicts in conflict',
                      style: TextStyle(
                        color: consoleTextColor(
                          context,
                          ConsoleTextTone.warning,
                        ),
                      ),
                    ),
                ],
              ),
              style: consoleTextStyle(
                context,
                ConsoleTextRole.controlLabel,
              ).copyWith(fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: AppDesign.spaceSm),
            for (var track = 0; track < _mapping.length; track++)
              Padding(
                padding: const EdgeInsets.only(bottom: AppDesign.spaceXs),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Expanded(
                          child: ConsoleText(
                            _track(track),
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        const SizedBox(width: AppDesign.spaceSm),
                        Expanded(
                          child: ConsoleSelect<int>(
                            label: 'Destination for ${_track(track)}',
                            value: _mapping[track] ?? -1,
                            options: _options,
                            onChanged: (value) => setState(
                              () => _mapping[track] = value < 0 ? null : value,
                            ),
                          ),
                        ),
                      ],
                    ),
                    if (shared[_mapping[track]] case final tracks?)
                      Semantics(
                        liveRegion: true,
                        child: ConsoleText(
                          '${_option(_mapping[track]!)} is also used by '
                          '${tracks.where((t) => t != track).map(_track).join(', ')}',
                          role: ConsoleTextRole.caption,
                          tone: ConsoleTextTone.warning,
                        ),
                      ),
                  ],
                ),
              ),
            if (_submitted && error != null)
              Semantics(
                liveRegion: true,
                child: ConsoleText(
                  error,
                  role: ConsoleTextRole.caption,
                  tone: ConsoleTextTone.warning,
                ),
              ),
            if (widget.confirmation case final text?) ...[
              const SizedBox(height: AppDesign.spaceSm),
              ConsoleText(text, weight: FontWeight.w600),
            ],
          ],
        ),
      ),
      actions: [
        ConsoleButton(
          label: 'Cancel',
          size: ConsoleButtonSize.regular,
          variant: ConsoleButtonVariant.quiet,
          onPressed: () => Navigator.pop(context),
        ),
        ConsoleButton(
          label: widget.confirmLabel,
          size: ConsoleButtonSize.regular,
          variant: ConsoleButtonVariant.primary,
          onPressed: () {
            if (error == null) {
              Navigator.pop(context, List<int?>.of(_mapping));
            } else {
              setState(() => _submitted = true);
            }
          },
        ),
      ],
    );
  }
}

/// Parses seconds or m:ss(.fff); null when the text is not a time.
double? _parseTime(String value) {
  final parts = value.trim().split(':');
  if (parts.length == 1) {
    final seconds = double.tryParse(parts[0]);
    return seconds != null && seconds.isFinite ? seconds : null;
  }
  if (parts.length != 2 || !RegExp(r'^\d+$').hasMatch(parts[0])) return null;
  final minutes = int.tryParse(parts[0]);
  final seconds = double.tryParse(parts[1]);
  if (minutes == null || seconds == null || seconds < 0 || seconds >= 60) {
    return null;
  }
  return minutes * 60 + seconds;
}

/// Formats seconds as m:ss with up to [decimals] fractional digits.
String _formatTime(double seconds, {int decimals = 2}) {
  final scale = pow(10, decimals).toInt();
  final units = (seconds * scale).round();
  final minutes = units ~/ (60 * scale);
  final rest = units - minutes * 60 * scale;
  final fraction = (rest % scale)
      .toString()
      .padLeft(decimals, '0')
      .replaceFirst(RegExp(r'0+$'), '');
  return '$minutes:${(rest ~/ scale).toString().padLeft(2, '0')}'
      '${fraction.isEmpty ? '' : '.$fraction'}';
}

const _timeFormatHint = 'Use seconds or m:ss, for example 0:12.5.';

class _GoToTimeDialog extends StatefulWidget {
  const _GoToTimeDialog({required this.duration, required this.rate});
  final double duration;
  final double rate;
  @override
  State<_GoToTimeDialog> createState() => _GoToTimeDialogState();
}

class _GoToTimeDialogState extends State<_GoToTimeDialog> {
  final controller = TextEditingController();
  String? error;
  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  String get _length => _formatTime(widget.duration / widget.rate);

  void submit() {
    final seconds = _parseTime(controller.text);
    if (seconds == null) {
      setState(() => error = _timeFormatHint);
      return;
    }
    if (seconds < 0 || seconds * widget.rate > widget.duration) {
      setState(() => error = 'Enter a time from 0:00 to $_length.');
      return;
    }
    Navigator.pop(context, (seconds * widget.rate).round());
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return ConsoleDialog(
      title: 'Go to time',
      content: SizedBox(
        width: 280,
        child: TextField(
          controller: controller,
          autofocus: true,
          style: TextStyle(color: p.text),
          decoration: InputDecoration(
            labelText: 'Position',
            hintText: 'm:ss or seconds',
            helperText: 'Take length $_length',
            errorText: error,
            errorMaxLines: 2,
          ),
          onChanged: (_) {
            if (error != null) setState(() => error = null);
          },
          onSubmitted: (_) => submit(),
        ),
      ),
      actions: [
        ConsoleButton(
          variant: ConsoleButtonVariant.quiet,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context),
          label: 'Cancel',
        ),
        ConsoleButton(
          variant: ConsoleButtonVariant.primary,
          size: ConsoleButtonSize.regular,
          onPressed: submit,
          label: 'Go',
        ),
      ],
    );
  }
}

class _LoopDialog extends StatefulWidget {
  const _LoopDialog({
    required this.frames,
    required this.rate,
    this.start,
    this.end,
  });
  final int frames;
  final double rate;
  final int? start, end;
  @override
  State<_LoopDialog> createState() => _LoopDialogState();
}

class _LoopDialogState extends State<_LoopDialog> {
  late final TextEditingController start, end;
  late final String _initialStart, _initialEnd;
  String? startError, endError;
  @override
  void initState() {
    super.initState();
    _initialStart = _formatTime((widget.start ?? 0) / widget.rate, decimals: 3);
    _initialEnd = _formatTime(
      (widget.end ?? widget.frames) / widget.rate,
      decimals: 3,
    );
    start = TextEditingController(text: _initialStart);
    end = TextEditingController(text: _initialEnd);
  }

  @override
  void dispose() {
    start.dispose();
    end.dispose();
    super.dispose();
  }

  String get _length => _formatTime(widget.frames / widget.rate);

  /// Unedited fields keep their exact frame; edited ones are parsed.
  int? _frame(TextEditingController field, String initial, int? exact) {
    if (field.text.trim() == initial && exact != null) return exact;
    final seconds = _parseTime(field.text);
    return seconds == null ? null : (seconds * widget.rate).round();
  }

  void submit() {
    final a = _frame(start, _initialStart, widget.start ?? 0);
    final b = _frame(end, _initialEnd, widget.end ?? widget.frames);
    String? range(int? frame) => frame == null
        ? _timeFormatHint
        : frame < 0 || frame > widget.frames
        ? 'Enter a time from 0:00 to $_length.'
        : null;
    var startMessage = range(a), endMessage = range(b);
    if (startMessage == null && endMessage == null && a! >= b!) {
      endMessage = 'End must be after the start.';
    }
    if (startMessage != null || endMessage != null) {
      setState(() {
        startError = startMessage;
        endError = endMessage;
      });
      return;
    }
    Navigator.pop(context, [a!, b!]);
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    InputDecoration field(String label, String? error) => InputDecoration(
      labelText: label,
      hintText: 'm:ss or seconds',
      errorText: error,
      errorMaxLines: 2,
    );
    return ConsoleDialog(
      title: 'Loop range',
      content: SizedBox(
        width: 280,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            ConsoleText(
              'Take length $_length',
              role: ConsoleTextRole.caption,
              tone: ConsoleTextTone.muted,
            ),
            const SizedBox(height: AppDesign.spaceMd),
            TextField(
              controller: start,
              autofocus: true,
              style: TextStyle(color: p.text),
              decoration: field('Start', startError),
              onChanged: (_) {
                if (startError != null) setState(() => startError = null);
              },
              onSubmitted: (_) => submit(),
            ),
            const SizedBox(height: AppDesign.spaceSm),
            TextField(
              controller: end,
              style: TextStyle(color: p.text),
              decoration: field('End', endError),
              onChanged: (_) {
                if (endError != null) setState(() => endError = null);
              },
              onSubmitted: (_) => submit(),
            ),
          ],
        ),
      ),
      actions: [
        ConsoleButton(
          variant: ConsoleButtonVariant.quiet,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context),
          label: 'Cancel',
        ),
        ConsoleButton(
          variant: ConsoleButtonVariant.primary,
          size: ConsoleButtonSize.regular,
          onPressed: submit,
          label: 'Set loop',
        ),
      ],
    );
  }
}

class RecorderPage extends StatefulWidget {
  const RecorderPage({
    super.key,
    required this.store,
    required this.nativeStore,
    required this.onMixer,
    required this.onFx,
    required this.onSettings,
    required this.onTools,
    required this.mixerName,
  });
  final VoidCallback onMixer, onFx, onSettings, onTools;
  final String mixerName;
  final RecorderStore store;
  final NativeLibraryStore nativeStore;

  @override
  State<RecorderPage> createState() => _RecorderPageState();
}

class _RecorderPageState extends State<RecorderPage> {
  final _feedback = InlineFeedbackController();
  final _problem = InlineFeedbackController();
  String? _capturePreviewId;
  RecorderStore get store => widget.store;
  String? _draftId, _draftName, _draftDeviceId;
  bool _startingRecord = false;
  int? _loopStart, _loopEnd;
  Object? _loopTake;
  String? _previewTakeId;
  bool _startingPlay = false;

  Map<String, dynamic> _snapshotLinks = {};
  Map<String, String> _importNames = {};
  static const _snapshotKey = 'recorder.native_snapshot_links.v1';
  static const _importNamesKey = 'recorder.import_names.v1';
  static const _manualMappingKey = 'recorder.manual_mapping.v1';

  @override
  void initState() {
    super.initState();
    store.addListener(_observeCapture);
    _loadSnapshotLinks();
    _loadImportNames();
  }

  void _observeCapture() {
    final id = _draftId;
    if (!mounted || id == null) return;
    if (store.state['selected_take_id'] == id ||
        store.takes.any((t) => t['id'] == id)) {
      setState(() {
        _previewTakeId = id;
        _draftId = _draftName = _draftDeviceId = null;
      });
    }
  }

  @override
  void dispose() {
    store.removeListener(_observeCapture);
    _feedback.dispose();
    _problem.dispose();
    super.dispose();
  }

  Future<void> _loadImportNames() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final value = jsonDecode(prefs.getString(_importNamesKey) ?? '{}');
      if (mounted && value is Map) {
        setState(
          () => _importNames = {
            for (final entry in value.entries)
              if (entry.key is String && entry.value is String)
                entry.key as String: entry.value as String,
            ..._importNames,
          },
        );
      }
    } catch (_) {
      _fail('Could not read imported take names on this device.');
    }
  }

  Future<void> _selectImportedTake(
    String id,
    String source,
    String format,
  ) async {
    if (!mounted) return;
    final name = format == 'wav'
        ? path.basenameWithoutExtension(source)
        : path.basename(path.normalize(source));
    setState(() {
      _previewTakeId = id;
      _draftId = _draftName = _draftDeviceId = null;
      _loopStart = _loopEnd = null;
      _loopTake = null;
      _importNames = {..._importNames, id: name};
    });
    try {
      final prefs = await SharedPreferences.getInstance();
      if (!await prefs.setString(_importNamesKey, jsonEncode(_importNames))) {
        throw StateError('Preferences write failed');
      }
      _snapshotMessage('Imported "$name". Ready to play.');
    } catch (_) {
      _fail('Take imported. Its name could not be saved on this device.');
    }
  }

  Future<void> _loadSnapshotLinks() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final value = jsonDecode(prefs.getString(_snapshotKey) ?? '{}');
      if (mounted && value is Map) {
        setState(() => _snapshotLinks = Map<String, dynamic>.from(value));
      }
    } catch (_) {
      _fail('Could not read snapshot links on this device.');
    }
  }

  void _snapshotMessage(String text) {
    if (mounted) {
      _problem.clear();
      _feedback.show(text);
    }
  }

  /// Shows a failure beside the transport, where the action started.
  void _fail(String text) {
    if (mounted) {
      _feedback.clear();
      _problem.show(text);
    }
  }

  bool _takeAvailable(String id) =>
      store.takes.any((t) => t['id'] == id && t['complete'] == true);

  Future<void> _unlinkSnapshot(String takeId) async {
    final updated = Map<String, dynamic>.from(_snapshotLinks)..remove(takeId);
    try {
      final prefs = await SharedPreferences.getInstance();
      if (!await prefs.setString(_snapshotKey, jsonEncode(updated))) {
        throw StateError('Preferences write failed');
      }
      if (mounted) setState(() => _snapshotLinks = updated);
    } catch (e) {
      _fail('Could not remove the snapshot link: $e');
    }
  }

  Future<bool> _continueWithoutSnapshot() async {
    if (!mounted) return false;
    return showConsoleConfirmation(
      context,
      title: 'Linked snapshot unavailable',
      message: 'The linked snapshot cannot be verified on the connected mixer.',
      confirmLabel: 'Continue without snapshot',
    );
  }

  Future<void> _linkSnapshot(String takeId) async {
    final native = widget.nativeStore;
    if (!store.canAdminRemote || native.nativeBusy || !native.nativeWritable) {
      _fail(
        'Linking snapshots needs the local recorder and a writable mixer connection.',
      );
      return;
    }
    await native.nativeAction({'op': 'inspect'}, native.nativeContext);
    if (!mounted) return;
    final view = native.nativeLibrary;
    if (view == null || !view.connected || !view.supports('recall')) {
      _fail(
        native.nativeStatus ?? 'This mixer does not support snapshot recall.',
      );
      return;
    }
    final captured = native.nativeContext;
    final selected = await showConsoleSelection<Map<String, dynamic>>(
      context,
      title: 'Link native mixer snapshot',
      message: 'Stored on the mixer, separate from OpenFader scenes. Manage them in Scenes.',
      options: [
        for (final slot in view.slots)
          ConsoleSelectionOption(
            label:
                '${slot['slot']} · ${slot['name'] == '' ? '(unnamed)' : slot['name']}',
            value: slot,
          ),
        if (_snapshotLinks.containsKey(takeId))
          const ConsoleSelectionOption(
            label: 'Remove link',
            value: <String, dynamic>{},
          ),
      ],
    );
    if (!mounted || selected == null) return;
    if (native.nativeContext != captured ||
        !native.nativeWritable ||
        !_takeAvailable(takeId)) {
      _fail('The mixer or take changed. Choose the snapshot again.');
      return;
    }
    final updated = Map<String, dynamic>.from(_snapshotLinks);
    if (selected.isEmpty) {
      updated.remove(takeId);
    } else {
      updated[takeId] = {
        'slot': selected['slot'],
        'name': selected['name'],
        'label': view.label,
        'authority': view.authority,
      };
    }
    try {
      final prefs = await SharedPreferences.getInstance();
      if (native.nativeContext != captured ||
          !native.nativeWritable ||
          !_takeAvailable(takeId)) {
        _fail('The mixer or take changed. Choose the snapshot again.');
        return;
      }
      if (!await prefs.setString(_snapshotKey, jsonEncode(updated))) {
        throw StateError('Preferences write failed');
      }
      if (mounted) setState(() => _snapshotLinks = updated);
    } catch (e) {
      _fail('Could not save the snapshot link: $e');
    }
  }

  bool get _safeToRecall =>
      store.state['input_mode'] == 'live' &&
      store.state['transport'] == 'stopped' &&
      !store.pending &&
      store.connected;

  Future<bool> _offerSnapshotRecall(String takeId) async {
    final link = _snapshotLinks[takeId];
    if (link is! Map) return true;
    final native = widget.nativeStore;
    if (kIsWeb ||
        !store.canAdminRemote ||
        !_safeToRecall ||
        native.nativeBusy ||
        !native.nativeWritable) {
      return _continueWithoutSnapshot();
    }
    await native.nativeAction({'op': 'inspect'}, native.nativeContext);
    if (!mounted) return false;
    final view = native.nativeLibrary;
    bool matches() =>
        view != null &&
        view.connected &&
        view.supports('recall') &&
        view.label == link['label'] &&
        view.authority == link['authority'] &&
        view.slots.any(
          (s) => s['slot'] == link['slot'] && s['name'] == link['name'],
        );
    if (!matches()) {
      return _continueWithoutSnapshot();
    }
    final captured = native.nativeContext;
    final recall = await showConsoleChoice<bool>(
      context,
      title: 'Recall linked mixer snapshot?',
      message:
          'Slot ${link['slot']} “${link['name']}” on ${link['label']}. Recall '
          'changes mixer settings; check this is the right mixer.',
      actions: [
        const ConsoleDialogAction(
          label: 'Continue without recall',
          value: false,
        ),
        const ConsoleDialogAction(
          label: 'Recall snapshot',
          value: true,
          variant: ConsoleButtonVariant.primary,
        ),
      ],
    );
    if (!mounted || recall == null) return false;
    if (!_safeToRecall ||
        native.nativeContext != captured ||
        !native.nativeWritable ||
        !matches() ||
        !_takeAvailable(takeId)) {
      _fail('The mixer or take changed. Try the recall again.');
      return false;
    }
    if (!recall) return true;
    await native.nativeAction({'op': 'recall', 'slot': link['slot']}, captured);
    if (!mounted) return false;
    final status = native.nativeStatus;
    _snapshotMessage(
      status ?? 'Recall not confirmed. Check the mixer before continuing.',
    );
    return native.nativeLastConfirmed &&
        _safeToRecall &&
        _takeAvailable(takeId);
  }

  bool _transferBusy = false;
  String? _transferLabel;

  bool get _localTransferAvailable =>
      !kIsWeb &&
      store.canAdminRemote &&
      (Platform.isMacOS || Platform.isWindows || Platform.isLinux);

  Future<void> _rejectLinkedParents(String value) async {
    var current = path.absolute(value);
    while (true) {
      if (await FileSystemEntity.type(current, followLinks: false) ==
          FileSystemEntityType.link) {
        throw StateError(
          'Choose a location without symbolic links ($current).',
        );
      }
      final parent = path.dirname(current);
      if (parent == current) break;
      current = parent;
    }
  }

  Future<void> _copyTransfer(String source, String destination) async {
    final type = await FileSystemEntity.type(source, followLinks: false);
    if (type == FileSystemEntityType.link ||
        type == FileSystemEntityType.notFound) {
      throw StateError(
        type == FileSystemEntityType.link
            ? 'Symbolic links cannot be transferred ($source).'
            : 'The file or folder is missing ($source).',
      );
    }
    if (await FileSystemEntity.type(destination, followLinks: false) !=
        FileSystemEntityType.notFound) {
      throw StateError('$destination already exists. Choose another folder.');
    }
    if (type == FileSystemEntityType.file) {
      final target = File(destination);
      await target.create(exclusive: true);
      try {
        final sink = target.openWrite();
        try {
          await sink.addStream(File(source).openRead());
        } finally {
          await sink.close();
        }
      } catch (_) {
        await target.delete();
        rethrow;
      }
    } else if (type == FileSystemEntityType.directory) {
      await Directory(destination).create();
      try {
        await for (final child in Directory(source).list(followLinks: false)) {
          await _copyTransfer(
            child.path,
            path.join(destination, path.basename(child.path)),
          );
        }
      } catch (_) {
        await Directory(destination).delete(recursive: true);
        rethrow;
      }
    } else {
      throw StateError('Only files and folders can be transferred ($source).');
    }
  }

  Future<void> _transferTake({required bool importing, String? takeId}) async {
    if (_transferBusy || store.pending) return;
    if (store.state['input_mode'] != 'live' ||
        store.state['transport'] != 'stopped') {
      _fail('Stop playback and return to live before transferring a take.');
      return;
    }
    if (!_localTransferAvailable) {
      _fail('Import and export are available only on the recorder computer.');
      return;
    }
    final format = await showConsoleSelection<String>(
      context,
      title: importing ? 'Import take' : 'Export take',
      options: importing
          ? const [
              ConsoleSelectionOption(
                label: 'Multichannel WAV file',
                value: 'wav',
              ),
              ConsoleSelectionOption(
                label: 'Native take folder',
                value: 'native',
              ),
              ConsoleSelectionOption(
                label: 'Folder of mono WAV tracks',
                value: 'tracks',
              ),
            ]
          : const [
              ConsoleSelectionOption(
                label: 'Separate tracks',
                detail: 'One WAV file per track.',
                value: 'tracks',
              ),
              ConsoleSelectionOption(
                label: 'Stereo mix',
                detail: 'All tracks at equal level, without mixer processing.',
                value: 'mix',
              ),
            ],
    );
    if (!mounted || format == null) return;
    String? selected;
    if (importing && format == 'wav') {
      selected = (await FilePicker.pickFiles(
        type: FileType.custom,
        allowedExtensions: ['wav'],
      )).firstOrNull?.path;
    } else {
      selected = await FilePicker.getDirectoryPath(
        dialogTitle: importing
            ? 'Choose take folder'
            : 'Choose export destination',
      );
    }
    if (!mounted || selected == null) return;
    setState(() {
      _transferBusy = true;
      _transferLabel = importing ? 'Importing take…' : 'Exporting take…';
    });
    String? staged;
    String? exportDestination;
    var safeToClean = false;
    try {
      final hostRoot = store.state['transfer_directory'];
      if (hostRoot is! String || !path.isAbsolute(hostRoot)) {
        throw StateError(
          'The recorder has no transfer folder. Restart the recorder host.',
        );
      }
      final root = hostRoot;
      await _rejectLinkedParents(root);
      await _rejectLinkedParents(selected);
      if (!importing) {
        final filename = switch (format) {
          'wav' => '$takeId.wav',
          'mix' => '$takeId-mix.wav',
          'tracks' => '$takeId-tracks',
          _ => '$takeId-native',
        };
        exportDestination = path.join(selected, filename);
        if (await FileSystemEntity.type(
              exportDestination,
              followLinks: false,
            ) !=
            FileSystemEntityType.notFound) {
          throw StateError(
            '"$filename" already exists in that folder. Choose another folder.',
          );
        }
      }
      await Directory(root).create(recursive: true);
      if (await FileSystemEntity.type(root, followLinks: false) !=
          FileSystemEntityType.directory) {
        throw StateError(
          'The recorder transfer folder is not a folder ($root).',
        );
      }
      final nonce = List.generate(
        16,
        (_) => Random.secure().nextInt(256).toRadixString(16).padLeft(2, '0'),
      ).join();
      final name = 'transfer-$nonce';
      staged = path.join(root, name);
      if (importing) {
        await _copyTransfer(selected, staged);
        safeToClean = true;
      }
      final id = importing
          ? 'take-${DateTime.now().toUtc().microsecondsSinceEpoch}-$nonce'
          : takeId!;
      if (!importing && !_takeAvailable(id)) {
        throw StateError('The take is no longer on the recorder.');
      }
      if (store.pending ||
          !store.connected ||
          store.state['transport'] != 'stopped') {
        throw StateError(
          store.connected
              ? 'Stop the recorder, then try again.'
              : 'The recorder host disconnected. Reconnect, then try again.',
        );
      }
      safeToClean = false;
      await store.command(importing ? 'import_take' : 'export_take', {
        'take_id': id,
        'transfer_name': name,
        'format': format,
      });
      if (store.error != null) {
        throw StateError('The recorder reported: ${store.error}');
      }
      // The host acknowledged the transfer. A failed destination copy can
      // discard staging; the source take and import source remain intact.
      safeToClean = true;
      if (!importing) {
        final destination = exportDestination!;
        if (mounted) {
          setState(() => _transferLabel = 'Saving export to destination…');
        }
        await _copyTransfer(staged, destination);
        _snapshotMessage('Take exported to $destination');
      } else {
        await _selectImportedTake(id, selected, format);
      }
    } catch (e) {
      debugPrint('Take transfer failed: $e; staging: $staged');
      final reason = switch (e) {
        StateError(:final message) => message,
        FileSystemException(:final path?, :final osError?) =>
          '${osError.message} ($path).',
        FileSystemException(:final message) => message,
        _ => '$e',
      };
      _fail('${importing ? 'Import' : 'Export'} failed. $reason');
    } finally {
      if (safeToClean && staged != null) {
        try {
          final entity = await FileSystemEntity.type(
            staged,
            followLinks: false,
          );
          if (entity == FileSystemEntityType.directory) {
            await Directory(staged).delete(recursive: true);
          }
          if (entity == FileSystemEntityType.file) await File(staged).delete();
        } catch (e) {
          _fail('Transfer finished, but temporary files remain at $staged.');
        }
      }
      if (mounted) {
        setState(() {
          _transferBusy = false;
          _transferLabel = null;
        });
      }
    }
  }

  Future<Map<String, dynamic>?> _soundcheckArguments(
    String takeId,
    Map<String, dynamic> take,
    String deviceId,
  ) async {
    final options = store.state['soundcheck_mapping'];
    final rawSources = options is Map ? options['capture_sources'] : null;
    final rawOutputs = options is Map ? options['output_mapping'] : null;
    if (rawSources is! List ||
        rawOutputs is! List ||
        rawSources.length != rawOutputs.length) {
      _fail(
        'Mixer routing is still being checked. Press Play again in a moment.',
      );
      return null;
    }
    final sources = rawSources.map((source) => '$source').toList();
    final available = [
      for (var lane = 0; lane < sources.length; lane++)
        if (sources[lane].isNotEmpty && rawOutputs[lane] is int) lane,
    ];
    if (available.isEmpty) {
      _fail('The mixer reports no inputs that can play back the take.');
      return null;
    }
    final manifest = take['manifest'];
    final saved =
        take['playback_sources'] ??
        (manifest is Map ? manifest['playback_sources'] : null);
    final recorded =
        saved ??
        take['capture_sources'] ??
        (manifest is Map ? manifest['capture_sources'] : null);
    final channels = recorderChannelCount(take);
    final hasMapping = recorded is List && recorded.length == channels;
    final coversInputs =
        hasMapping &&
        available.every(
          (lane) =>
              recorded.where((source) => source == sources[lane]).length == 1,
        );
    final coversSavedSubset =
        saved is List &&
        hasMapping &&
        recorded.any((source) => source is String && source.isNotEmpty) &&
        recorded
            .where((source) => source != '')
            .every(
              (source) =>
                  available.where((lane) => sources[lane] == source).length ==
                      1 &&
                  recorded.where((other) => other == source).length == 1,
            );
    if (coversInputs || coversSavedSubset) {
      return {'take_id': takeId, 'expected_device_id': deviceId};
    }
    final names = recorderChannelNames(take);
    final sourceNames = store.state['capture_source_names'];
    final mapping = List<int?>.generate(channels, (track) {
      if (hasMapping) {
        final lane = available
            .where((lane) => sources[lane] == recorded[track])
            .firstOrNull;
        if (lane != null) return lane;
      }
      return track < available.length ? available[track] : null;
    });
    if (!mounted) return null;
    final confirmed = await showConsoleDialog<List<int?>>(
      context,
      builder: (_) => _TrackMappingDialog(
        title: 'Map tracks to mixer inputs',
        header: const [
          ConsoleText(
            'OpenFader switches these inputs for playback and restores them '
            'when you return live. Unassigned tracks are silent.',
            role: ConsoleTextRole.caption,
            tone: ConsoleTextTone.muted,
          ),
        ],
        trackNames: names,
        options: [
          for (final lane in available)
            (
              lane,
              sourceNames is List &&
                      lane < sourceNames.length &&
                      '${sourceNames[lane]}'.trim().isNotEmpty
                  ? '${sourceNames[lane]}'
                  : 'USB return ${(rawOutputs[lane] as int) + 1}',
            ),
        ],
        mapping: mapping,
        confirmLabel: 'Save mapping and play',
        destination: 'input',
      ),
    );
    if (confirmed == null ||
        !mounted ||
        store.pending ||
        !store.connected ||
        store.state['selected_device_id'] != deviceId ||
        !_takeAvailable(takeId)) {
      return null;
    }
    return {
      'take_id': takeId,
      'expected_device_id': deviceId,
      'mapping_confirmed': true,
      'capture_mapping': confirmed,
      'expected_capture_sources': sources,
    };
  }

  Future<void> _playSelected(String? takeId) async {
    if (_startingPlay || store.pending) return;
    void message(String text) => _fail(text);
    _problem.clear();

    final mode = '${store.state['input_mode']}';
    if (mode == 'soundcheck') {
      await store.command(
        store.state['transport'] == 'playing' ? 'pause' : 'play',
      );
      return;
    }
    if (takeId == null) {
      message('Select a completed take to play.');
      return;
    }
    final take = store.takes.where((t) => t['id'] == takeId).firstOrNull;
    final takeChannels = recorderChannelCount(take);
    final takeNames = recorderChannelNames(take ?? {});
    if (takeChannels < 1 || takeChannels > 64) {
      message('This take has $takeChannels tracks. Playback supports 1 to 64.');
      return;
    }
    setState(() => _startingPlay = true);
    try {
      await store.refreshDevices();
      if (!mounted) return;
      final devices = store.devices
          .where(
            (d) =>
                (d['output_channels'] as num? ?? 0) > 0 &&
                (d['output_channels'] as num? ?? 0) <= 64,
          )
          .toList();
      if (devices.isEmpty) {
        message(
          'No playback interface found. Connect one to the recorder computer, '
          'then press Play again.',
        );
        return;
      }
      final takeRate = take?['sample_rate'];
      bool matchesRate(Map<String, dynamic> device) =>
          device['sample_rate'] == takeRate;
      if (!devices.any(matchesRate)) {
        message(
          'No interface is set to $takeRate Hz, the take’s rate. Change the '
          'interface rate in the system audio settings, then press Play again.',
        );
        return;
      }
      var deviceId = store.state['selected_device_id'] as String?;
      if (!devices.any((d) => d['id'] == deviceId && matchesRate(d))) {
        deviceId = await showConsoleSelection<String>(
          context,
          title: 'Choose playback interface',
          message: 'The take is $takeRate Hz.',
          options: [
            for (final d in devices)
              ConsoleSelectionOption(
                label: '${d['name']}',
                detail: matchesRate(d)
                    ? '${d['output_channels']} outputs · ${d['sample_rate']} Hz'
                    : '${d['sample_rate']} Hz · set it to $takeRate Hz to use it',
                value: '${d['id']}',
                enabled: matchesRate(d),
              ),
          ],
        );
        if (deviceId == null ||
            !mounted ||
            !_canPrepare ||
            !_takeAvailable(takeId)) {
          return;
        }
        await store.command('select_device', {'device_id': deviceId});
        if (store.error != null) {
          message('Could not select the interface: ${store.error}');
          return;
        }
      }
      if (!mounted) return;
      final selected = devices.firstWhere((d) => d['id'] == deviceId);
      final outputs = (selected['output_channels'] as num).toInt();
      await store.command('prepare_soundcheck');
      if (!mounted || store.error != null) {
        if (store.error != null) {
          message('Playback setup failed: ${store.error}');
        }
        return;
      }
      final caps = store.state['capabilities'] as Map? ?? {};
      if (caps['soundcheck'] == true) {
        // The provider revalidates its routing profile and physical state on
        // every entry. Playback never recalls a mixer scene or processing.
        final args = await _soundcheckArguments(takeId, take ?? {}, deviceId!);
        if (args == null) return;
        await store.command('enter_soundcheck', args);
      } else {
        var mapping = List<int?>.generate(
          takeChannels,
          (i) => i < outputs ? i : null,
        );
        final mappingContext = {
          'device_id': deviceId,
          'sample_rate': selected['sample_rate'],
          'output_channels': outputs,
          'take_channels': takeChannels,
        };
        try {
          final prefs = await SharedPreferences.getInstance();
          final saved = jsonDecode(prefs.getString(_manualMappingKey) ?? '{}');
          final entries = saved is Map ? saved['mapping'] : null;
          if (saved is Map &&
              mappingContext.entries.every((e) => saved[e.key] == e.value) &&
              entries is List &&
              entries.length == takeChannels &&
              entries.every(
                (v) => v == null || v is int && v >= 0 && v < outputs,
              )) {
            final assigned = entries.whereType<int>().toList();
            if (assigned.isNotEmpty &&
                assigned.toSet().length == assigned.length) {
              mapping = List<int?>.from(entries);
            }
          }
        } catch (_) {
          message('The saved mapping could not be read. Check each output.');
        }
        if (!mounted) return;
        final chosen = await showConsoleDialog<List<int?>>(
          context,
          builder: (_) => _TrackMappingDialog(
            title: 'Manual playback',
            header: [
              ConsoleText(
                '${selected['name']} · $takeRate Hz · $outputs outputs',
                weight: FontWeight.w600,
              ),
              ConsoleText(
                'Automatic routing unavailable: '
                '${'${store.state['soundcheck_reason'] ?? 'no verified mixer routing profile'}'.replaceFirst(RegExp(r'\.$'), '')}.',
                role: ConsoleTextRole.caption,
                tone: ConsoleTextTone.muted,
              ),
            ],
            trackNames: takeNames,
            options: [for (var n = 0; n < outputs; n++) (n, 'Output ${n + 1}')],
            mapping: mapping,
            confirmation:
                'Playing confirms output levels are safe and the mixer is '
                'routed to these outputs.',
            confirmLabel: 'Play with this mapping',
            destination: 'output',
          ),
        );
        if (chosen == null ||
            !mounted ||
            store.pending ||
            !store.connected ||
            !_takeAvailable(takeId) ||
            store.state['selected_device_id'] != deviceId) {
          return;
        }
        await store.command('enter_manual_soundcheck', {
          'take_id': takeId,
          'output_mapping': chosen,
          'routing_confirmed': true,
          'expected_device_id': deviceId,
        });
        if (store.error == null) {
          try {
            final prefs = await SharedPreferences.getInstance();
            if (!await prefs.setString(
              _manualMappingKey,
              jsonEncode({...mappingContext, 'mapping': chosen}),
            )) {
              throw StateError('Preferences write failed');
            }
          } catch (_) {
            message('Playing. This mapping could not be saved on this device.');
          }
        }
      }
      if (store.error != null) {
        message('Playback could not start: ${store.error}');
      }
    } catch (error) {
      message('Playback could not start: $error');
    } finally {
      if (mounted) setState(() => _startingPlay = false);
    }
  }

  Future<void> _returnManualLive() async {
    if (store.pending) return;
    await store.command('stop_manual_output');
    if (store.error != null || !mounted) {
      if (store.error != null) {
        _fail('Could not stop playback output: ${store.error}');
      }
      return;
    }
    if (!mounted || store.pending) return;
    final restored = await showConsoleDialog<bool>(
      context,
      builder: (context) => ConsoleDialog(
        title: 'Restore live inputs',
        message:
            'Playback has stopped. Switch the mixer back to its live '
            'inputs, then confirm.',
        actions: [
          ConsoleButton(
            variant: ConsoleButtonVariant.quiet,
            size: ConsoleButtonSize.regular,
            onPressed: () => Navigator.pop(context, false),
            label: 'Not yet',
          ),
          ConsoleButton(
            variant: ConsoleButtonVariant.primary,
            size: ConsoleButtonSize.regular,
            onPressed: () => Navigator.pop(context, true),
            label: 'Live inputs restored',
          ),
        ],
      ),
    );
    if (restored == true && mounted) {
      await store.command('return_manual_live', {'routing_restored': true});
      if (mounted && store.error != null) {
        _fail('Could not return to live: ${store.error}');
      }
    }
  }

  static String _time(dynamic frames, dynamic rate) {
    final seconds = (frames is num && rate is num && rate > 0)
        ? (frames / rate).floor()
        : 0;
    return '${(seconds ~/ 3600).toString().padLeft(2, '0')}:${((seconds ~/ 60) % 60).toString().padLeft(2, '0')}:${(seconds % 60).toString().padLeft(2, '0')}';
  }

  static String _clock(dynamic frames, dynamic rate) {
    final hundredths = frames is num && rate is num && rate > 0
        ? (frames * 100 / rate).floor() % 100
        : 0;
    return '${_time(frames, rate)}.${hundredths.toString().padLeft(2, '0')}';
  }

  static String _takeDate(dynamic id) {
    final match = RegExp(r'^take-(\d+)').firstMatch('$id');
    final micros = int.tryParse(match?.group(1) ?? '');
    if (micros == null) return '';
    try {
      final d = DateTime.fromMicrosecondsSinceEpoch(
        micros,
        isUtc: true,
      ).toLocal();
      return ' · ${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')} ${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';
    } catch (_) {
      return '';
    }
  }

  static int _takeTimestamp(Map<String, dynamic> take) =>
      int.tryParse(
        RegExp(r'^take-(\d+)').firstMatch('${take['id']}')?.group(1) ?? '',
      ) ??
      0;

  String _takeName(Map<String, dynamic> take) {
    final manifest = take['manifest'];
    final value = take['name'] ?? (manifest is Map ? manifest['name'] : null);
    if (value is String && value.trim().isNotEmpty) return value;
    return _importNames[take['id']] ??
        'Take ${store.takes.indexWhere((t) => t['id'] == take['id']) + 1}';
  }

  bool get _canPrepare =>
      store.connected &&
      !_transferBusy &&
      !store.pending &&
      store.state['available'] == true &&
      store.state['input_mode'] == 'live' &&
      store.state['transport'] == 'stopped';

  Future<void> _newTake() async {
    if (!_canPrepare) return;
    final draft = await showConsoleDialog<({String name, String deviceId})>(
      context,
      builder: (_) => _PrepareTakeDialog(store: store),
    );
    if (!mounted || draft == null || !_canPrepare) return;
    setState(() {
      _draftId = 'take-${DateTime.now().toUtc().microsecondsSinceEpoch}';
      _draftName = draft.name;
      _draftDeviceId = draft.deviceId;
      _previewTakeId = null;
      _loopStart = _loopEnd = null;
      _loopTake = null;
    });
  }

  Future<void> _record() async {
    if (!_canPrepare || _startingRecord) return;
    final deviceId = store.state['selected_device_id'] as String?;
    if (deviceId == null) {
      _showSetup(context, store);
      _snapshotMessage('Choose an audio interface, then press Record.');
      return;
    }
    if (_draftId != null && _draftDeviceId != deviceId) {
      _fail(
        'The interface changed. Create the named take again from New take.',
      );
      return;
    }
    final now = DateTime.now();
    final id = _draftId ?? 'take-${now.toUtc().microsecondsSinceEpoch}';
    final name = _draftName ?? 'Take ${store.takes.length + 1}';
    setState(() {
      _startingRecord = true;
      _draftId = id;
      _draftName = name;
      _draftDeviceId = deviceId;
      _loopStart = _loopEnd = null;
      _loopTake = null;
    });
    await store.command('start_recording', {'take_id': id, 'name': name});
    if (!mounted) return;
    setState(() => _startingRecord = false);
    _observeCapture();
  }

  Future<void> _returnLive() async {
    if (store.state['manual_routing'] == true) {
      await _returnManualLive();
      return;
    }
    await store.command('return_live');
    if (mounted && store.error != null) {
      _fail('Could not return to live: ${store.error}');
    } else if (mounted && store.state['input_mode'] == 'live') {
      _snapshotMessage(
        'Live inputs restored. Your mixer adjustments are kept.',
      );
    }
  }

  Future<void> _goToTime(Map<String, dynamic> take) async {
    final id = take['id'];
    final rate = (take['sample_rate'] as num?)?.toDouble() ?? 0;
    final duration = (take['duration_frames'] as num?)?.toDouble() ?? 0;
    if (rate <= 0 || duration <= 0) return;
    final frames = await showConsoleDialog<int>(
      context,
      builder: (_) => _GoToTimeDialog(duration: duration, rate: rate),
    );
    if (frames != null &&
        store.connected &&
        !store.pending &&
        store.state['selected_take_id'] == id &&
        (store.state['capabilities'] as Map?)?['seek'] == true) {
      await store.command('seek', {'position_frames': frames});
    }
  }

  Future<void> _setLoop(Map<String, dynamic> take) async {
    final id = take['id'];
    final frames = (take['duration_frames'] as num?)?.toInt() ?? 0;
    final rate = (take['sample_rate'] as num?)?.toDouble() ?? 0;
    if (frames <= 0 || rate <= 0) return;
    final range = await showConsoleDialog<List<int>>(
      context,
      builder: (_) => _LoopDialog(
        frames: frames,
        rate: rate,
        start: _loopTake == id ? _loopStart : null,
        end: _loopTake == id ? _loopEnd : null,
      ),
    );
    if (range == null ||
        !store.connected ||
        store.pending ||
        store.state['selected_take_id'] != id ||
        (store.state['capabilities'] as Map?)?['loop'] != true) {
      return;
    }
    await store.command('set_loop', {
      'start_frame': range[0],
      'end_frame': range[1],
      'enabled': true,
    });
    if (mounted && store.error == null) {
      setState(() {
        _loopStart = range[0];
        _loopEnd = range[1];
        _loopTake = id;
      });
    }
  }

  Future<void> _inspectTake(Map<String, dynamic> take) async {
    try {
      final data = await store.inspectTake(take['id'] as String);
      if (!mounted) return;
      final manifest = data['manifest'] as Map;
      await showConsoleDialog<void>(
        context,
        builder: (context) => ConsoleDialog(
          title: 'Take details · ${take['id']}',
          scrollable: false,
          content: SizedBox(
            width: 600,
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Manifest: ${manifest['frames']} frames · ${manifest['sample_rate']} Hz · ${manifest['channels']} channels · complete ${manifest['complete']}',
                  ),
                  for (final h in data['wav_headers'] as List)
                    Text(
                      'WAV ${h['file']}: ${h['channels']} channels · ${h['bits_per_sample']} bit · ${h['sample_rate']} Hz · ${h['frames']} frames',
                    ),
                  Text(
                    'Measured frames ${data['start_frame']}–${data['end_frame']} (end exclusive), normalized PCM',
                  ),
                  for (final c in data['channels'] as List)
                    SelectableText(
                      'Ch ${c['channel']}: min ${(c['min'] as num).toStringAsFixed(8)}  max ${(c['max'] as num).toStringAsFixed(8)}  RMS ${(c['rms'] as num).toStringAsFixed(8)}',
                    ),
                ],
              ),
            ),
          ),
          actions: [
            ConsoleButton(
              variant: ConsoleButtonVariant.quiet,
              size: ConsoleButtonSize.regular,
              onPressed: () => Navigator.pop(context),
              label: 'Close',
            ),
          ],
        ),
      );
    } catch (e) {
      _fail('Could not read take details: $e');
    }
  }

  Future<void> _clearLoop(Map<String, dynamic> take) async {
    final frames = (take['duration_frames'] as num?)?.toInt();
    if (frames == null || frames <= 0 || store.pending) return;
    await store.command('set_loop', {
      'start_frame': 0,
      'end_frame': frames,
      'enabled': false,
    });
    if (mounted && store.error == null) {
      setState(() => _loopStart = _loopEnd = null);
    }
  }

  @override
  Widget build(BuildContext context) => _workspace(context);

  /// Feedback for the last action, shown under the transport that started it.
  Widget _notice(InlineFeedbackController controller, ConsoleTextTone tone) =>
      ValueListenableBuilder<String?>(
        valueListenable: controller,
        builder: (context, message, _) => message == null
            ? const SizedBox.shrink()
            : Padding(
                padding: const EdgeInsets.only(left: AppDesign.spaceLg),
                child: Row(
                  children: [
                    Expanded(
                      child: Semantics(
                        liveRegion: true,
                        child: ConsoleText(
                          message,
                          role: ConsoleTextRole.caption,
                          tone: tone,
                        ),
                      ),
                    ),
                    ConsoleIconButton(
                      label: 'Dismiss message',
                      onPressed: controller.clear,
                      child: const Icon(Icons.close, size: 18),
                    ),
                  ],
                ),
              ),
      );

  Widget _workspace(BuildContext context) => ListenableBuilder(
    listenable: store,
    builder: (context, _) {
      final p = ConsolePalette.of(context);
      final s = store.state;
      final transport = '${s['transport'] ?? 'stopped'}';
      final mode = '${s['input_mode'] ?? 'live'}';
      final soundcheck = mode == 'soundcheck';
      final recovery = mode.toLowerCase().contains('recovery');
      // The restore panel owns the only return action while manual routing is out.
      final manualRestore = recovery && s['manual_routing'] == true;
      final recording = transport == 'recording';
      final selectedTake = s['selected_take_id'];
      if (recording && selectedTake is String) {
        _capturePreviewId = selectedTake;
        _previewTakeId = selectedTake;
      } else if (_capturePreviewId != null) {
        _previewTakeId = _capturePreviewId;
        if (store.takes.any((take) => take['id'] == _capturePreviewId)) {
          _capturePreviewId = null;
        }
      }
      final completed = store.takes.where((t) => t['complete'] == true).toList()
        ..sort((a, b) => _takeTimestamp(b).compareTo(_takeTimestamp(a)));
      final previewId = recording || soundcheck
          ? selectedTake as String?
          : _draftId != null
          ? null
          : _previewTakeId ??
                (completed.any((t) => t['id'] == selectedTake)
                    ? selectedTake as String?
                    : completed.firstOrNull?['id'] as String?);
      final take = store.takes.where((t) => t['id'] == previewId).firstOrNull;
      final prepared = soundcheck && previewId == selectedTake;
      final enabled =
          store.connected && !store.pending && s['available'] == true;
      final caps = s['capabilities'] is Map
          ? s['capabilities'] as Map
          : const {};
      final device = store.devices
          .where((d) => d['id'] == s['selected_device_id'])
          .firstOrNull;
      final rate = recording
          ? s['sample_rate']
          : take?['sample_rate'] ?? s['sample_rate'];
      final title = recording
          ? (take != null ? _takeName(take) : _draftName ?? 'Recording')
          : _draftName ?? (take != null ? _takeName(take) : 'Recorder');
      final recordReason = !store.connected
          ? 'Recorder computer unavailable. Reconnect to continue.'
          : s['available'] != true
          ? '${s['unavailable_reason'] ?? 'Recording is unavailable on this host.'}'
          : recovery
          ? 'Restore live routing before recording.'
          : soundcheck
          ? 'Return to live inputs to record again.'
          : store.pending || _startingRecord
          ? 'Waiting for the recorder…'
          : recording
          ? null
          : caps['recording'] != true
          ? 'Recording is unavailable. Check audio setup.'
          : _draftId != null && _draftDeviceId != s['selected_device_id']
          ? 'The audio interface changed. Review the named take.'
          : device == null
          ? '${s['device_restore_reason'] ?? 'Choose an audio interface in Audio setup.'}'
          : (device['input_channels'] as num? ?? 0) <= 0
          ? 'This interface has no capture inputs. Choose another in Audio setup.'
          : null;
      Widget panel({required Widget child}) => Container(
        padding: const EdgeInsets.all(16),
        color: p.panel,
        child: child,
      );
      final empty =
          take == null &&
          _draftId == null &&
          !recording &&
          !soundcheck &&
          !recovery;
      final canExport = take?['complete'] == true && _localTransferAvailable;
      final liveStopped = s['input_mode'] == 'live' && transport == 'stopped';
      final headerActions = Wrap(
        spacing: AppDesign.spaceSm,
        runSpacing: AppDesign.spaceXs,
        children: [
          ConsoleButton(
            label: 'Browse takes',
            icon: Icons.folder_open_outlined,
            onPressed: store.pending ? null : () => _browseTakes(previewId),
          ),
          ConsoleButton(
            label: 'Audio setup',
            icon: Icons.usb,
            onPressed: () => _showSetup(context, store),
          ),
          if (canExport)
            ConsoleButton(
              label: 'Export',
              icon: Icons.file_download_outlined,
              onPressed:
                  store.connected &&
                      liveStopped &&
                      !store.pending &&
                      !_transferBusy
                  ? () => _transferTake(
                      importing: false,
                      takeId: take!['id'] as String,
                    )
                  : null,
            ),
        ],
      );
      final header = LayoutBuilder(
        builder: (context, constraints) {
          final narrow = constraints.maxWidth < AppDesign.narrowBreakpoint;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  ConsoleIconButton(
                    label: 'Mixer',
                    onPressed: widget.onMixer,
                    child: const Icon(Icons.equalizer),
                  ),
                  Expanded(
                    child: ConsoleText(
                      title,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      role: ConsoleTextRole.sectionTitle,
                    ),
                  ),
                  if (!narrow) headerActions,
                  PopupMenuButton<String>(
                    tooltip: 'Recorder actions',
                    onSelected: (action) {
                      switch (action) {
                        case 'new':
                          _newTake();
                        case 'clear_loop':
                          if (take != null && prepared && enabled) {
                            _clearLoop(take);
                          }
                        case 'delete':
                          if (take != null) {
                            _deleteTake(
                              store,
                              take['id'] as String,
                              _takeName(take),
                            );
                          }
                        case 'details':
                          if (take != null) _inspectTake(take);
                        case 'snapshot':
                          if (take != null) _linkSnapshot(take['id'] as String);
                        case 'recall':
                          if (take != null && _safeToRecall) {
                            _offerSnapshotRecall(take['id'] as String);
                          }
                        case 'unlink':
                          if (take != null) {
                            _unlinkSnapshot(take['id'] as String);
                          }
                        case 'fx':
                          widget.onFx();
                        case 'settings':
                          widget.onSettings();
                        case 'tools':
                          widget.onTools();
                      }
                    },
                    itemBuilder: (_) => [
                      PopupMenuItem(
                        value: 'new',
                        enabled: _canPrepare,
                        child: const Text('New take'),
                      ),
                      if (prepared &&
                          caps['loop'] == true &&
                          (s['loop_enabled'] == true ||
                              (_loopTake == previewId && _loopStart != null)))
                        PopupMenuItem(
                          value: 'clear_loop',
                          enabled: enabled,
                          child: const Text('Clear loop'),
                        ),
                      if (take != null)
                        PopupMenuItem(
                          value: 'delete',
                          enabled: _canPrepare && !_transferBusy,
                          child: const Text('Delete take…'),
                        ),
                      if (take?['complete'] == true) ...[
                        const PopupMenuItem(
                          value: 'details',
                          child: Text('Take details'),
                        ),
                        PopupMenuItem(
                          value: 'snapshot',
                          enabled: enabled && !recording && !soundcheck,
                          child: const Text('Link mixer snapshot'),
                        ),
                        if (_snapshotLinks.containsKey(take?['id'])) ...[
                          PopupMenuItem(
                            value: 'recall',
                            enabled: _safeToRecall,
                            child: const Text('Recall linked snapshot…'),
                          ),
                          const PopupMenuItem(
                            value: 'unlink',
                            child: Text('Remove snapshot link'),
                          ),
                        ],
                      ],
                    ],
                  ),
                ],
              ),
              if (narrow)
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppDesign.spaceSm,
                  ),
                  child: headerActions,
                ),
            ],
          );
        },
      );
      final canRecord =
          _canPrepare &&
          caps['recording'] == true &&
          !_startingRecord &&
          (device == null || (device['input_channels'] as num? ?? 0) > 0) &&
          (_draftId == null || _draftDeviceId == s['selected_device_id']);
      final canPlay =
          enabled &&
          !recording &&
          !recovery &&
          !_startingPlay &&
          !_transferBusy &&
          (take?['complete'] == true || soundcheck);
      final canSeek =
          enabled && prepared && caps['seek'] == true && take != null;
      final canLoop =
          enabled && prepared && caps['loop'] == true && take != null;
      final position = _clock(
        recording
            ? s['recorded_frames']
            : prepared
            ? s['position_frames']
            : 0,
        rate,
      );
      final transportLabel = recording
          ? 'Recording'
          : soundcheck
          ? switch (transport) {
              'playing' => 'Playing',
              'paused' => 'Paused',
              _ => 'Stopped',
            }
          : null;
      final recordAction = ConsoleButton(
        label: recording ? 'Stop recording' : 'Record',
        icon: recording ? Icons.stop : Icons.fiber_manual_record,
        variant: recording
            ? ConsoleButtonVariant.danger
            : ConsoleButtonVariant.secondary,
        onPressed: recording
            ? (store.pending ? null : () => store.command('stop'))
            : canRecord
            ? _record
            : null,
      );
      final transportControls = Wrap(
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: AppDesign.spaceMd,
        runSpacing: AppDesign.spaceSm,
        children: [
          if (!recovery)
            Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: AppDesign.spaceSm,
              runSpacing: AppDesign.spaceXs,
              children: [
                if (!recording)
                  ConstrainedBox(
                    constraints: const BoxConstraints(minWidth: 110),
                    child: ConsoleButton(
                      label: _startingPlay
                          ? 'Preparing…'
                          : transport == 'playing'
                          ? 'Pause'
                          : transport == 'paused'
                          ? 'Resume'
                          : soundcheck
                          ? 'Play'
                          : 'Play take',
                      icon: transport == 'playing'
                          ? Icons.pause
                          : Icons.play_arrow,
                      variant: ConsoleButtonVariant.primary,
                      onPressed: canPlay
                          ? () => _playSelected(previewId)
                          : null,
                    ),
                  ),
                if (soundcheck)
                  ConsoleIconButton(
                    label: 'Stop playback',
                    onPressed: enabled ? () => store.command('stop') : null,
                    child: const Icon(Icons.stop),
                  ),
                if (!soundcheck) recordAction,
                Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      position,
                      style: TextStyle(
                        color: recording ? p.meterRed : p.text,
                        fontFamily: 'Barlow Condensed',
                        fontSize: AppDesign.entryValue,
                        fontFeatures: const [FontFeature.tabularFigures()],
                      ),
                    ),
                    if (transportLabel != null)
                      ConsoleText(
                        transportLabel,
                        role: ConsoleTextRole.caption,
                        tone: ConsoleTextTone.muted,
                      ),
                  ],
                ),
              ],
            ),
          if (soundcheck || recovery && !manualRestore)
            Wrap(
              crossAxisAlignment: WrapCrossAlignment.center,
              spacing: AppDesign.spaceSm,
              runSpacing: AppDesign.spaceXs,
              children: [
                if (prepared) ...[
                  ConsoleButton(
                    label: 'Go to time',
                    onPressed: canSeek ? () => _goToTime(take) : null,
                  ),
                  ConsoleButton(
                    icon: Icons.repeat,
                    label: 'Loop range',
                    onPressed: canLoop ? () => _setLoop(take) : null,
                  ),
                ],
                ConsoleButton(
                  label: 'Return to live',
                  onPressed: enabled ? _returnLive : null,
                ),
              ],
            ),
        ],
      );
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            color: p.recorderSurface,
            padding: const EdgeInsets.fromLTRB(8, 12, 8, 8),
            child: header,
          ),
          if (!empty && !manualRestore)
            Container(
              color: p.recorderBar,
              padding: const EdgeInsets.symmetric(
                horizontal: AppDesign.spaceLg,
                vertical: AppDesign.spaceSm,
              ),
              child: transportControls,
            ),
          _notice(_problem, ConsoleTextTone.warning),
          _notice(_feedback, ConsoleTextTone.normal),
          if (_transferBusy)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Row(
                children: [
                  const SizedBox(
                    width: 18,
                    height: 18,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  ),
                  const SizedBox(width: 12),
                  Expanded(child: Text(_transferLabel ?? 'Transferring take…')),
                ],
              ),
            ),
          if (recordReason != null &&
              !empty &&
              !soundcheck &&
              !recording &&
              !recovery &&
              store.connected &&
              !store.pending)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: Text(
                'Recording · $recordReason',
                style: consoleTextStyle(
                  context,
                  ConsoleTextRole.caption,
                  tone: ConsoleTextTone.muted,
                ),
              ),
            ),
          if (prepared && (caps['seek'] != true || caps['loop'] != true))
            Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AppDesign.spaceLg,
              ),
              child: ConsoleText(
                caps['seek'] != true && caps['loop'] != true
                    ? 'Seeking and looping are unavailable for this playback session.'
                    : caps['seek'] != true
                    ? 'Seeking is unavailable for this playback session.'
                    : 'Looping is unavailable for this playback session.',
                role: ConsoleTextRole.caption,
                tone: ConsoleTextTone.muted,
              ),
            ),
          if ((s['xruns'] as num? ?? 0) > 0)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Text(
                'Audio dropouts: ${s['xruns']}',
                style: consoleTextStyle(
                  context,
                  ConsoleTextRole.caption,
                  tone: ConsoleTextTone.warning,
                ),
              ),
            ),
          if (!store.connected ||
              store.error != null ||
              s['error'] != null ||
              s['capture_error'] != null)
            ValueListenableBuilder<String?>(
              valueListenable: _problem,
              builder: (context, shown, _) {
                final hostError = !store.connected
                    ? 'Recorder host unavailable. Check that it is running on '
                          'the recorder computer.'
                    : store.error ??
                          (s['capture_error'] != null
                              ? 'Recording problem: ${s['capture_error']}'
                              : 'Recorder host error: ${s['error']}');
                // Action failures already quote the host error beside the action.
                if (store.error != null &&
                    shown?.contains(store.error!) == true) {
                  return const SizedBox.shrink();
                }
                return Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AppDesign.spaceLg,
                    vertical: AppDesign.spaceXs,
                  ),
                  child: ConsoleText(
                    hostError,
                    role: ConsoleTextRole.caption,
                    tone: ConsoleTextTone.warning,
                  ),
                );
              },
            ),
          if (manualRestore)
            panel(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const ConsoleText(
                    'Restore live inputs on the mixer',
                    tone: ConsoleTextTone.warning,
                    weight: FontWeight.w600,
                  ),
                  const SizedBox(height: AppDesign.spaceXs),
                  const ConsoleText(
                    'Playback and recording stay blocked until you confirm.',
                    role: ConsoleTextRole.caption,
                    tone: ConsoleTextTone.muted,
                  ),
                  const SizedBox(height: AppDesign.spaceSm),
                  ConsoleButton(
                    variant: ConsoleButtonVariant.primary,
                    onPressed: enabled ? _returnManualLive : null,
                    label: 'Confirm live inputs',
                  ),
                ],
              ),
            ),
          if (recovery && s['manual_routing'] != true)
            SizedBox(
              height: 220,
              child: SingleChildScrollView(
                child: Column(
                  children: [
                    if (recovery) ...[
                      panel(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            ConsoleText(
                              'Routing recovery required',
                              role: ConsoleTextRole.sectionTitle,
                              tone: ConsoleTextTone.warning,
                              weight: FontWeight.bold,
                            ),
                            const SizedBox(height: 8),
                            const ConsoleText(
                              'Mixer routing changed for playback was not confirmed '
                              'as restored. Recording is blocked until recovery completes.',
                            ),
                            const SizedBox(height: 12),
                            ConsoleButton(
                              variant: ConsoleButtonVariant.primary,
                              onPressed: enabled && transport == 'stopped'
                                  ? () async {
                                      final confirmed =
                                          await showConsoleConfirmation(
                                            context,
                                            title: 'Rebind recovery?',
                                            message:
                                                'Confirm this is the same physical mixer. Its current '
                                                'routing is checked against the journal; nothing is changed.',
                                            confirmLabel:
                                                'Confirm mixer and rebind',
                                          );
                                      if (confirmed) {
                                        await store.command('rebind_recovery');
                                        if (mounted && store.error != null) {
                                          _fail(
                                            'Rebind failed: ${store.error}',
                                          );
                                        }
                                      }
                                    }
                                  : null,
                              label: 'Rebind recovery',
                            ),
                            const SizedBox(height: 8),
                            ConsoleButton(
                              onPressed: enabled && transport == 'stopped'
                                  ? () async {
                                      final confirmed =
                                          await showConsoleConfirmation(
                                            context,
                                            title: 'Keep current routing?',
                                            message:
                                                'Use after restoring the mixer yourself. The host checks '
                                                'routing matches the original, then archives the journal. '
                                                'Nothing is changed.',
                                            confirmLabel:
                                                'Keep routing and archive',
                                          );
                                      if (confirmed) {
                                        await store.command(
                                          'accept_current_routing',
                                          {'accept_current': true},
                                        );
                                        if (mounted && store.error != null) {
                                          _fail(
                                            'Routing does not match yet: ${store.error}',
                                          );
                                        }
                                      }
                                    }
                                  : null,
                              label: 'Accept current routing and archive',
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 16),
                    ],
                  ],
                ),
              ),
            ),
          Expanded(
            child: empty
                ? _emptyWorkspace(recordReason, recordAction)
                : RecorderArrangement(
                    store: store,
                    previewTakeId: previewId,
                    takeName:
                        _draftName ?? (take != null ? _takeName(take) : null),
                    loopStartFrame: _loopTake == previewId ? _loopStart : null,
                    loopEndFrame: _loopTake == previewId ? _loopEnd : null,
                    onLoopSelection: (start, end, total) => setState(() {
                      _loopStart = start;
                      _loopEnd = end;
                      _loopTake = store.state['selected_take_id'];
                    }),
                  ),
          ),
        ],
      );
    },
  );

  Widget _emptyWorkspace(String? recordReason, Widget recordAction) => Align(
    alignment: const Alignment(0, -.35),
    child: SingleChildScrollView(
      padding: const EdgeInsets.all(AppDesign.spaceXl),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: AppDesign.dialogMaxWidth),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            const ConsoleText('No take open', role: ConsoleTextRole.sheetTitle),
            const SizedBox(height: AppDesign.spaceSm),
            ConsoleText(
              _localTransferAvailable
                  ? 'Import audio for playback or record a new take.'
                  : 'Choose a saved take in Browse takes. Import is available on the recorder computer.',
              tone: ConsoleTextTone.muted,
            ),
            const SizedBox(height: AppDesign.spaceLg),
            Wrap(
              spacing: AppDesign.spaceSm,
              runSpacing: AppDesign.spaceXs,
              children: [
                if (_localTransferAvailable)
                  ConsoleButton(
                    label: 'Import take',
                    icon: Icons.file_upload_outlined,
                    size: ConsoleButtonSize.regular,
                    variant: ConsoleButtonVariant.primary,
                    onPressed: _canPrepare
                        ? () => _transferTake(importing: true)
                        : null,
                  ),
                recordAction,
              ],
            ),
            if (recordReason != null && store.connected) ...[
              const SizedBox(height: AppDesign.spaceSm),
              ConsoleText(
                'Recording · $recordReason',
                role: ConsoleTextRole.caption,
                tone: ConsoleTextTone.muted,
              ),
            ],
          ],
        ),
      ),
    ),
  );

  Future<void> _recoverTake(String id) async {
    final confirmed = await showConsoleConfirmation(
      context,
      title: 'Recover take?',
      message:
          'Keeps audio up to the last saved checkpoint and discards the rest.',
      confirmLabel: 'Recover take',
    );
    if (confirmed) {
      await store.command('recover_take', {'take_id': id, 'confirm': true});
    }
  }

  Future<void> _deleteTake(RecorderStore store, String id, String name) async {
    final confirmed = await showConsoleConfirmation(
      context,
      title: 'Delete take?',
      message:
          'Permanently delete “$name” and its recorded audio from the recorder computer? This cannot be undone.',
      confirmLabel: 'Delete take',
      danger: true,
    );
    if (!confirmed || !mounted) return;
    await store.command('delete_take', {'take_id': id, 'confirm': true});
    if (!mounted || store.takes.any((take) => take['id'] == id)) return;
    setState(() {
      if (_previewTakeId == id) _previewTakeId = null;
      if (_capturePreviewId == id) _capturePreviewId = null;
      if (_loopTake == id) {
        _loopTake = null;
        _loopStart = null;
        _loopEnd = null;
      }
    });
  }

  Future<void> _browseTakes(String? previewTakeId) async {
    if (store.pending) return;
    final id = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      constraints: const BoxConstraints(maxWidth: 880),
      builder: (context) => _TakeBrowser(
        store: store,
        previewTakeId: previewTakeId,
        nameOf: _takeName,
        dateOf: (take) => _takeDate(take['id']).replaceFirst(' · ', ''),
        durationOf: (take) =>
            _time(take['duration_frames'], take['sample_rate']),
        onImport: _transferBusy
            ? null
            : () async {
                Navigator.pop(context);
                await _transferTake(importing: true);
              },
        onRecover: _canPrepare && store.canAdminRemote ? _recoverTake : null,
        onExport: _localTransferAvailable && !_transferBusy
            ? (id) async {
                Navigator.pop(context);
                await _transferTake(importing: false, takeId: id);
              }
            : null,
      ),
    );
    if (!mounted || id == null) return;
    // Switching the viewed take never changes active playback or mixer routing.
    if (store.state['input_mode'] != 'live' ||
        store.state['transport'] != 'stopped' ||
        store.pending) {
      _fail('Return to live inputs before opening another take.');
      return;
    }
    setState(() {
      _previewTakeId = id;
      _draftId = _draftName = _draftDeviceId = null;
      _loopStart = _loopEnd = null;
      _loopTake = null;
    });
  }

  static void _showSetup(BuildContext context, RecorderStore store) {
    showModalBottomSheet<void>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      constraints: const BoxConstraints(maxWidth: 720),
      builder: (context) => ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final p = ConsolePalette.of(context);
          final s = store.state;
          final selectedDevice = s['selected_device_id'];
          final enabled =
              store.connected && !store.pending && s['available'] == true;
          final recording = '${s['transport']}'.toLowerCase().contains(
            'record',
          );
          final soundcheck = '${s['input_mode']}'.toLowerCase().contains(
            'soundcheck',
          );
          final device = store.devices
              .where((d) => d['id'] == selectedDevice)
              .firstOrNull;
          final canSelect =
              enabled &&
              !recording &&
              !soundcheck &&
              s['input_mode'] == 'live' &&
              s['transport'] == 'stopped';
          Future<void> chooseInterface() async {
            final id = await showConsoleSelection<String>(
              context,
              title: 'Choose audio interface',
              selected: device?['id'] as String?,
              options: [
                for (final item in store.devices)
                  ConsoleSelectionOption(
                    value: '${item['id']}',
                    label: '${item['name']}',
                    detail:
                        '${item['input_channels']} inputs · '
                        '${item['output_channels']} outputs · ${item['sample_rate']} Hz',
                  ),
              ],
            );
            if (id == null ||
                id == selectedDevice ||
                !context.mounted ||
                !store.connected ||
                store.pending ||
                store.state['available'] != true ||
                store.state['input_mode'] != 'live' ||
                store.state['transport'] != 'stopped' ||
                !store.devices.any((d) => d['id'] == id)) {
              return;
            }
            await store.command('select_device', {'device_id': id});
          }

          Widget panel({required Widget child}) => Container(
            padding: const EdgeInsets.all(12),
            color: p.panel,
            child: child,
          );
          return SizedBox(
            height: MediaQuery.sizeOf(context).height * .75,
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      const Expanded(
                        child: ConsoleText(
                          'Audio setup',
                          role: ConsoleTextRole.sheetTitle,
                        ),
                      ),
                      ConsoleIconButton(
                        label: 'Close audio setup',
                        onPressed: () => Navigator.pop(context),
                        child: const Icon(Icons.close),
                      ),
                    ],
                  ),
                  panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Row(
                          children: [
                            const Expanded(
                              child: ConsoleText(
                                'Audio interface',
                                role: ConsoleTextRole.sectionTitle,
                              ),
                            ),
                            ConsoleButton(
                              variant: ConsoleButtonVariant.quiet,
                              onPressed:
                                  store.pending ||
                                      !store.connected ||
                                      s['device_scan_pending'] == true
                                  ? null
                                  : store.refreshDevices,
                              label: 'Refresh',
                              icon: Icons.refresh,
                            ),
                          ],
                        ),
                        const SizedBox(height: 10),
                        if (store.devices.isEmpty)
                          ConsoleText(
                            s['device_scan_error'] != null
                                ? 'Interface scan failed: ${s['device_scan_error']}. '
                                      'Press Refresh to try again.'
                                : s['device_scan_pending'] == true
                                ? 'Scanning audio interfaces…'
                                : 'Connect an audio interface to the recorder computer, then refresh.',
                            tone: s['device_scan_error'] != null
                                ? ConsoleTextTone.warning
                                : ConsoleTextTone.muted,
                          )
                        else ...[
                          if (device != null) ...[
                            ConsoleText(
                              '${device['name']}',
                              weight: FontWeight.w600,
                            ),
                            ConsoleText(
                              '${device['input_channels']} inputs · '
                              '${device['output_channels']} outputs · ${device['sample_rate']} Hz'
                              '${s['disk_free_bytes'] is num ? ' · ${((s['disk_free_bytes'] as num) / 1073741824).toStringAsFixed(1)} GB free' : ''}',
                              tone: ConsoleTextTone.muted,
                            ),
                            const SizedBox(height: AppDesign.spaceSm),
                          ],
                          Align(
                            alignment: Alignment.centerLeft,
                            child: ConsoleButton(
                              label: device == null
                                  ? 'Choose interface'
                                  : 'Change interface',
                              onPressed: canSelect ? chooseInterface : null,
                            ),
                          ),
                          if (recording || soundcheck)
                            const ConsoleText(
                              'Stop recording or return to live to change the interface.',
                              role: ConsoleTextRole.caption,
                              tone: ConsoleTextTone.muted,
                            ),
                        ],
                        if (store.error != null)
                          ConsoleText(
                            store.error!,
                            tone: ConsoleTextTone.warning,
                          ),
                      ],
                    ),
                  ),
                  if (selectedDevice != null) ...[
                    const SizedBox(height: 12),
                    ConsoleButton(
                      variant: ConsoleButtonVariant.quiet,
                      onPressed: enabled && !recording && !soundcheck
                          ? () => showConsoleDialog<void>(
                              context,
                              builder: (_) => _CaptureNamesDialog(store: store),
                            )
                          : null,
                      icon: Icons.account_tree_outlined,
                      label: 'Recording input names',
                    ),
                    // One line: the host's reason when automatic names are
                    // unavailable, otherwise the current naming source.
                    ConsoleText(
                      s['capture_names_automatic'] != true
                          ? 'Custom names are in use.'
                          : s['channel_names_reason'] is String
                          ? s['channel_names_reason'] as String
                          : s['device_match_reason'] is String
                          ? s['device_match_reason'] as String
                          : 'Names follow the mixer’s USB patch.',
                      role: ConsoleTextRole.caption,
                      tone: ConsoleTextTone.muted,
                    ),
                  ],
                  if (s['device_restore_reason'] != null) ...[
                    const SizedBox(height: AppDesign.spaceSm),
                    ConsoleText(
                      '${s['device_restore_reason']}',
                      tone: ConsoleTextTone.warning,
                    ),
                  ],
                  if (store.canAdminRemote) ...[
                    const SizedBox(height: 24),
                    panel(child: _RemoteAdminPanel(store: store)),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

class _CaptureNamesDialog extends StatefulWidget {
  const _CaptureNamesDialog({required this.store});
  final RecorderStore store;
  @override
  State<_CaptureNamesDialog> createState() => _CaptureNamesDialogState();
}

class _CaptureNamesDialogState extends State<_CaptureNamesDialog> {
  late final Map<String, dynamic>? _device;
  late final List<TextEditingController> _labels;
  bool _saving = false;
  bool _edited = false;
  String? _error;
  @override
  void initState() {
    super.initState();
    final s = widget.store.state;
    final device = widget.store.devices
        .where((d) => d['id'] == s['selected_device_id'])
        .firstOrNull;
    _device = device == null ? null : Map<String, dynamic>.of(device);
    final count = recorderChannelCount(
      device == null ? null : device['input_channels'],
    );
    final saved = s['capture_names_automatic'] == true
        ? s['channel_names']
        : s['capture_channel_names'];
    _labels = List.generate(
      count,
      (i) => TextEditingController(
        text:
            saved is List &&
                saved.length == count &&
                saved[i] is String &&
                (saved[i] as String).trim().isNotEmpty
            ? saved[i] as String
            : 'Input ${i + 1}',
      ),
    );
  }

  @override
  void dispose() {
    for (final label in _labels) {
      label.dispose();
    }
    super.dispose();
  }

  Future<void> _save() async {
    final store = widget.store;
    if (!_edited && store.state['capture_names_automatic'] == true) {
      Navigator.pop(context);
      return;
    }
    final device = store.devices
        .where((d) => d['id'] == _device?['id'])
        .firstOrNull;
    final names = _labels.map((c) => c.text.trim()).toList();
    if (_device == null ||
        device == null ||
        _labels.isEmpty ||
        [
          'id',
          'input_channels',
          'output_channels',
          'sample_rate',
        ].any((key) => device[key] != _device[key]) ||
        store.state['selected_device_id'] != _device['id'] ||
        store.state['transport'] != 'stopped' ||
        store.state['input_mode'] != 'live') {
      setState(
        () =>
            _error = 'The interface changed. Close and open input names again.',
      );
      return;
    }
    if (names.any(
      (name) =>
          name.isEmpty ||
          name.runes.length > 120 ||
          utf8.encode(name).length > 512 ||
          name.runes.any((c) => c < 32 || c >= 127 && c <= 159),
    )) {
      setState(
        () => _error = names.any((name) => name.isEmpty)
            ? 'Input ${names.indexWhere((name) => name.isEmpty) + 1} needs a name.'
            : 'Use up to 120 characters, without control characters.',
      );
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    await store.command('set_capture_channel_names', {
      'device_id': _device['id'],
      'channel_names': names,
    });
    if (!mounted) return;
    if (store.error != null) {
      setState(() {
        _saving = false;
        _error = store.error;
      });
      return;
    }
    final saved = store.state['capture_channel_names'];
    if (saved is! List ||
        saved.length != names.length ||
        List.generate(
          names.length,
          (i) => saved[i] == names[i],
        ).contains(false)) {
      setState(() {
        _saving = false;
        _error = 'The recorder did not confirm these names. Try saving again.';
      });
      return;
    }
    Navigator.pop(context);
  }

  Future<void> _useAutoNames() async {
    if (_saving || _device == null) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    await widget.store.command('set_capture_channel_names', {
      'device_id': _device['id'],
      'auto': true,
    });
    if (!mounted) return;
    if (widget.store.error != null ||
        widget.store.state['capture_names_automatic'] != true) {
      setState(() {
        _saving = false;
        _error =
            widget.store.error ??
            'The recorder did not confirm automatic names. Try again.';
      });
      return;
    }
    Navigator.pop(context);
  }

  Future<void> _chooseMixerInput(int input) async {
    var query = '';
    final inputs = widget.store.channelNames.entries.toList()
      ..sort((a, b) => a.key.compareTo(b.key));
    final name = await showConsoleDialog<String>(
      context,
      builder: (context) => StatefulBuilder(
        builder: (context, update) {
          final matches = inputs
              .where(
                (e) => '${e.key + 1} ${e.value}'.toLowerCase().contains(query),
              )
              .toList();
          return ConsoleDialog(
            title: 'Choose input channel · ${inputs.length} inputs',
            scrollable: false,
            content: SizedBox(
              width: 440,
              height: min(480, MediaQuery.sizeOf(context).height * .55),
              child: Column(
                children: [
                  TextField(
                    decoration: const InputDecoration(
                      hintText: 'Search by channel number or name',
                      prefixIcon: Icon(Icons.search),
                    ),
                    onChanged: (value) =>
                        update(() => query = value.trim().toLowerCase()),
                  ),
                  const SizedBox(height: 12),
                  Expanded(
                    child: matches.isEmpty
                        ? const Center(child: Text('No input channels match.'))
                        : ListView.builder(
                            itemCount: matches.length,
                            itemBuilder: (context, i) => ListTile(
                              title: Text(
                                '${matches[i].key + 1} · ${matches[i].value}',
                              ),
                              onTap: () =>
                                  Navigator.pop(context, matches[i].value),
                            ),
                          ),
                  ),
                ],
              ),
            ),
            actions: [
              ConsoleButton(
                variant: ConsoleButtonVariant.quiet,
                size: ConsoleButtonSize.regular,
                onPressed: () => Navigator.pop(context),
                label: 'Cancel',
              ),
            ],
          );
        },
      ),
    );
    if (!mounted || name == null) return;
    _labels[input].text = name;
    _edited = true;
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    final mixerNames =
        widget.store.channelNames.entries
            .where((e) => e.value.trim().isNotEmpty)
            .toList()
          ..sort((a, b) => a.key.compareTo(b.key));
    return ConsoleDialog(
      title: 'Input names and mixer mapping',
      scrollable: false,
      content: SizedBox(
        width: 520,
        height: min(480, MediaQuery.sizeOf(context).height * .55),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              '${_device?['name'] ?? 'No interface selected'}',
              style: TextStyle(color: p.text, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            Text(
              'Auto follows the mixer’s USB patch. Names do not change routing.',
              style: TextStyle(color: p.mutedText),
            ),
            const SizedBox(height: 12),
            Align(
              alignment: Alignment.centerLeft,
              child: ConsoleButton(
                variant: ConsoleButtonVariant.quiet,
                onPressed: _saving ? null : _useAutoNames,
                icon: Icons.auto_fix_high_outlined,
                label: 'Auto from USB patch',
              ),
            ),
            if (widget.store.state['channel_names_reason'] is String)
              Text(
                widget.store.state['channel_names_reason'] as String,
                style: TextStyle(color: p.mutedText),
              ),
            const SizedBox(height: 8),
            Expanded(
              child: ListView.builder(
                itemCount: _labels.length,
                itemBuilder: (context, i) => Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: Row(
                    children: [
                      SizedBox(
                        width: 60,
                        child: Text(
                          'Input ${i + 1}',
                          style: TextStyle(color: p.mutedText),
                        ),
                      ),
                      Expanded(
                        child: TextField(
                          controller: _labels[i],
                          onChanged: (_) => _edited = true,
                          enabled: !_saving,
                          decoration: const InputDecoration(
                            labelText: 'Recorded track name',
                          ),
                        ),
                      ),
                      if (mixerNames.isNotEmpty)
                        ConsoleIconButton(
                          label: 'Choose mixer input channel',
                          onPressed: _saving
                              ? null
                              : () => _chooseMixerInput(i),
                          child: const Icon(Icons.account_tree_outlined),
                        ),
                    ],
                  ),
                ),
              ),
            ),
            if (_error != null) Text(_error!, style: TextStyle(color: p.amber)),
          ],
        ),
      ),
      actions: [
        ConsoleButton(
          variant: ConsoleButtonVariant.quiet,
          size: ConsoleButtonSize.regular,
          onPressed: _saving ? null : () => Navigator.pop(context),
          label: 'Cancel',
        ),
        ConsoleButton(
          variant: ConsoleButtonVariant.primary,
          size: ConsoleButtonSize.regular,
          onPressed: _saving || _labels.isEmpty ? null : _save,
          label: _saving ? 'Saving…' : 'Save input names',
        ),
      ],
    );
  }
}

class _PrepareTakeDialog extends StatefulWidget {
  const _PrepareTakeDialog({required this.store});
  final RecorderStore store;
  @override
  State<_PrepareTakeDialog> createState() => _PrepareTakeDialogState();
}

class _PrepareTakeDialogState extends State<_PrepareTakeDialog> {
  final _name = TextEditingController();
  String? _deviceId, _error;
  bool _preparing = false;
  @override
  void initState() {
    super.initState();
    _deviceId = widget.store.state['selected_device_id'] as String?;
    _name.text = 'Take ${widget.store.takes.length + 1}';
    widget.store.refreshDevices();
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  Future<void> _prepare() async {
    final store = widget.store;
    final name = _name.text.trim();
    if (name.isEmpty ||
        name.runes.length > 120 ||
        utf8.encode(name).length > 512 ||
        name.runes.any((c) => c < 32 || (c >= 127 && c <= 159))) {
      setState(
        () => _error = name.isEmpty
            ? 'Enter a take name.'
            : 'Use up to 120 characters, without control characters.',
      );
      return;
    }
    final id = _deviceId;
    if (id == null || _preparing || store.pending) return;
    setState(() {
      _preparing = true;
      _error = null;
    });
    if (store.state['selected_device_id'] != id) {
      await store.command('select_device', {'device_id': id});
    }
    if (!mounted) return;
    final device = store.devices.where((d) => d['id'] == id).firstOrNull;
    if (store.error != null ||
        store.state['selected_device_id'] != id ||
        store.state['input_mode'] != 'live' ||
        store.state['transport'] != 'stopped' ||
        device == null ||
        (device['input_channels'] as num? ?? 0) <= 0) {
      setState(() {
        _preparing = false;
        _error = store.error != null
            ? 'Could not select the interface: ${store.error}'
            : device == null || (device['input_channels'] as num? ?? 0) <= 0
            ? 'This interface has no inputs. Choose another.'
            : 'Return to live inputs and stop playback first.';
      });
      return;
    }
    Navigator.pop(context, (name: name, deviceId: id));
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.store,
    builder: (context, _) {
      final store = widget.store;
      final p = ConsolePalette.of(context);
      final devices = store.devices
          .where((d) => (d['input_channels'] as num? ?? 0) > 0)
          .toList();
      final device = devices.where((d) => d['id'] == _deviceId).firstOrNull;
      final busy = _preparing || store.pending;
      return ConsoleDialog(
        title: 'New take',
        scrollable: false,
        content: SizedBox(
          width: 440,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                TextField(
                  controller: _name,
                  autofocus: true,
                  enabled: !busy,
                  textCapitalization: TextCapitalization.sentences,
                  maxLength: 120,
                  decoration: InputDecoration(
                    labelText: 'Take name',
                    errorText: _error,
                  ),
                  onSubmitted: (_) {
                    if (device != null) _prepare();
                  },
                ),
                const SizedBox(height: 16),
                DropdownButtonFormField<String>(
                  key: ValueKey(
                    '${_deviceId}_${devices.map((d) => d['id']).join(',')}',
                  ),
                  initialValue: device == null ? null : _deviceId,
                  isExpanded: true,
                  decoration: const InputDecoration(
                    labelText: 'Capture interface',
                  ),
                  hint: const Text('Choose an audio interface'),
                  items: [
                    for (final d in devices)
                      DropdownMenuItem(
                        value: '${d['id']}',
                        child: Text(
                          '${d['name']}',
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                  ],
                  onChanged: busy
                      ? null
                      : (id) => setState(() => _deviceId = id),
                ),
                if (device != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Text(
                      'Records all ${device['input_channels']} inputs · ${device['sample_rate']} Hz',
                      style: TextStyle(color: p.mutedText),
                    ),
                  ),
                if (devices.isEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Text(
                      store.state['device_scan_pending'] == true
                          ? 'Scanning audio interfaces…'
                          : '${store.state['device_scan_error'] ?? 'No capture interfaces found. Connect an interface, then refresh.'}',
                      style: TextStyle(color: p.mutedText),
                    ),
                  ),
                Align(
                  alignment: Alignment.centerLeft,
                  child: ConsoleButton(
                    variant: ConsoleButtonVariant.quiet,
                    onPressed:
                        busy || store.state['device_scan_pending'] == true
                        ? null
                        : store.refreshDevices,
                    icon: Icons.refresh,
                    label: 'Refresh interfaces',
                  ),
                ),
                const SizedBox(height: 8),
                ConsoleText(
                  'Recording starts when you press Record.',
                  tone: ConsoleTextTone.muted,
                ),
              ],
            ),
          ),
        ),
        actions: [
          ConsoleButton(
            variant: ConsoleButtonVariant.quiet,
            size: ConsoleButtonSize.regular,
            onPressed: busy ? null : () => Navigator.pop(context),
            label: 'Cancel',
          ),
          ConsoleButton(
            variant: ConsoleButtonVariant.primary,
            size: ConsoleButtonSize.regular,
            onPressed: !busy && store.connected && device != null
                ? _prepare
                : null,
            label: _preparing ? 'Preparing…' : 'Create take',
          ),
        ],
      );
    },
  );
}

class _TakeBrowser extends StatefulWidget {
  const _TakeBrowser({
    required this.store,
    required this.nameOf,
    required this.previewTakeId,
    required this.dateOf,
    required this.durationOf,
    this.onImport,
    this.onRecover,
    this.onExport,
  });
  final RecorderStore store;
  final String? previewTakeId;
  final String Function(Map<String, dynamic>) nameOf, dateOf, durationOf;
  final Future<void> Function()? onImport;
  final Future<void> Function(String)? onRecover;
  final Future<void> Function(String)? onExport;
  @override
  State<_TakeBrowser> createState() => _TakeBrowserState();
}

class _TakeBrowserState extends State<_TakeBrowser> {
  String _query = '', _sort = 'newest';
  int _timestamp(Map<String, dynamic> t) =>
      int.tryParse(
        RegExp(r'^take-(\d+)').firstMatch('${t['id']}')?.group(1) ?? '',
      ) ??
      0;
  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: widget.store,
    builder: (context, _) {
      final p = ConsolePalette.of(context);
      final store = widget.store;
      final takes = store.takes
          .where(
            (t) => '${widget.nameOf(t)} ${widget.dateOf(t)} ${t['id']}'
                .toLowerCase()
                .contains(_query),
          )
          .toList();
      takes.sort(
        (a, b) => _sort == 'name'
            ? widget
                  .nameOf(a)
                  .toLowerCase()
                  .compareTo(widget.nameOf(b).toLowerCase())
            : _sort == 'oldest'
            ? _timestamp(a).compareTo(_timestamp(b))
            : _timestamp(b).compareTo(_timestamp(a)),
      );
      final canOpen =
          store.state['input_mode'] == 'live' &&
          store.state['transport'] == 'stopped' &&
          !store.pending;
      final canTransfer =
          !kIsWeb &&
          store.canAdminRemote &&
          (Platform.isMacOS || Platform.isWindows || Platform.isLinux);
      return SizedBox(
        height: MediaQuery.sizeOf(context).height * .85,
        child: SafeArea(
          top: false,
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: const ConsoleText(
                        'Browse takes',
                        role: ConsoleTextRole.sheetTitle,
                      ),
                    ),
                    ConsoleIconButton(
                      label: 'Close takes',
                      onPressed: () => Navigator.pop(context),
                      child: const Icon(Icons.close),
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                TextField(
                  onChanged: (v) =>
                      setState(() => _query = v.trim().toLowerCase()),
                  decoration: const InputDecoration(
                    hintText: 'Search takes',
                    prefixIcon: Icon(Icons.search),
                  ),
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    Expanded(
                      child: DropdownButton<String>(
                        value: _sort,
                        isExpanded: true,
                        items: const [
                          DropdownMenuItem(
                            value: 'newest',
                            child: Text('Newest first'),
                          ),
                          DropdownMenuItem(
                            value: 'oldest',
                            child: Text('Oldest first'),
                          ),
                          DropdownMenuItem(value: 'name', child: Text('Name')),
                        ],
                        onChanged: (v) {
                          if (v != null) setState(() => _sort = v);
                        },
                      ),
                    ),
                    const SizedBox(width: 12),
                    if (canTransfer)
                      ConsoleButton(
                        label: 'Import take',
                        icon: Icons.file_upload_outlined,
                        onPressed: canOpen ? widget.onImport : null,
                      ),
                  ],
                ),
                if (!canOpen)
                  Text(
                    store.state['transport'] == 'recording'
                        ? 'Recording continues. Stop recording to open another take.'
                        : 'Return to live inputs and stop playback to open another take.',
                    style: TextStyle(color: p.amber),
                  ),
                if (!canTransfer)
                  Text(
                    'Import and export are available on the recorder computer.',
                    style: TextStyle(color: p.mutedText),
                  ),
                const SizedBox(height: 8),
                Expanded(
                  child: takes.isEmpty
                      ? Center(
                          child: Text(
                            _query.isEmpty
                                ? canTransfer
                                      ? 'No saved takes. Import a take to play it here.'
                                      : 'No saved takes on the recorder computer.'
                                : 'No takes match your search.',
                            textAlign: TextAlign.center,
                            style: TextStyle(color: p.mutedText),
                          ),
                        )
                      : ListView.separated(
                          itemCount: takes.length,
                          separatorBuilder: (context, index) =>
                              Divider(color: p.linePanel, height: 1),
                          itemBuilder: (context, index) {
                            final take = takes[index];
                            final complete = take['complete'] == true;
                            final date = widget.dateOf(take);
                            return ListTile(
                              contentPadding: const EdgeInsets.symmetric(
                                horizontal: 4,
                                vertical: 6,
                              ),
                              leading: Icon(
                                complete
                                    ? Icons.audio_file_outlined
                                    : Icons.warning_amber_rounded,
                                color: complete ? p.mutedText : p.amber,
                              ),
                              title: Text(
                                widget.nameOf(take),
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                  color: p.text,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                              subtitle: Text(
                                '${widget.durationOf(take)} · ${recorderChannelCount(take)} tracks${date.isEmpty ? '' : ' · $date'}${complete ? '' : ' · Recovery needed'}',
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                              ),
                              selected: take['id'] == widget.previewTakeId,
                              onTap: complete && canOpen
                                  ? () => Navigator.pop(
                                      context,
                                      take['id'] as String,
                                    )
                                  : null,
                              trailing: complete
                                  ? (canTransfer
                                        ? ConsoleIconButton(
                                            label:
                                                'Export ${widget.nameOf(take)}',
                                            onPressed:
                                                canOpen &&
                                                    widget.onExport != null
                                                ? () => widget.onExport!(
                                                    take['id'] as String,
                                                  )
                                                : null,
                                            child: const Icon(
                                              Icons.file_download_outlined,
                                            ),
                                          )
                                        : Icon(
                                            Icons.chevron_right,
                                            color: p.mutedText,
                                          ))
                                  : widget.onRecover != null
                                  ? ConsoleButton(
                                      variant: ConsoleButtonVariant.quiet,
                                      onPressed: canOpen
                                          ? () => widget.onRecover!(
                                              take['id'] as String,
                                            )
                                          : null,
                                      label: 'Recover',
                                    )
                                  : null,
                            );
                          },
                        ),
                ),
              ],
            ),
          ),
        ),
      );
    },
  );
}

class _RemoteAdminPanel extends StatefulWidget {
  const _RemoteAdminPanel({required this.store});
  final RecorderStore store;
  @override
  State<_RemoteAdminPanel> createState() => _RemoteAdminPanelState();
}

class _RemoteAdminPanelState extends State<_RemoteAdminPanel> {
  final password = TextEditingController();
  @override
  void dispose() {
    password.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final p = ConsolePalette.of(context);
    final enabled = store.remoteStatus['enabled'] == true;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          enabled
              ? 'App connections enabled · port ${store.remoteStatus['port']}'
              : 'App connections disabled',
          style: TextStyle(color: p.mutedText),
        ),
        const SizedBox(height: 8),
        Text(
          'Other OpenFader apps connect to this computer’s address, port ${store.remoteStatus['port'] ?? 8845}.',
          style: TextStyle(color: p.mutedText),
        ),
        const SizedBox(height: 8),
        Text(
          store.remoteStatus['password_required'] == true
              ? 'Password required'
              : 'No password required',
          style: TextStyle(color: p.text),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: password,
          obscureText: true,
          enableSuggestions: false,
          autocorrect: false,
          enabled: !store.remotePending,
          style: TextStyle(color: p.text),
          decoration: const InputDecoration(
            labelText: 'Optional server password',
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 8),
        Text(
          'Also used for browser access. Changing it disconnects clients.',
          style: TextStyle(color: p.mutedText),
        ),
        const SizedBox(height: 12),
        ConsoleActionGroup(
          children: [
            ConsoleButton(
              onPressed: store.remotePending || password.text.isEmpty
                  ? null
                  : () async {
                      await store.remoteAction('password', password.text);
                      if (mounted && store.remoteError == null) {
                        password.clear();
                        setState(() {});
                      }
                    },
              label: 'Set password',
            ),
            ConsoleButton(
              onPressed:
                  store.remotePending ||
                      store.remoteStatus['password_required'] != true
                  ? null
                  : () => store.remoteAction('password', ''),
              label: 'Remove password',
            ),
          ],
        ),
        const SizedBox(height: 12),
        if (kIsWeb || !Platform.isMacOS)
          Text(
            'Microphone access is managed by the operating system and audio interface driver.',
            style: TextStyle(color: p.mutedText),
          ),
        ConsoleActionGroup(
          children: [
            if (!kIsWeb && Platform.isMacOS)
              ConsoleButton(
                onPressed: store.remotePending
                    ? null
                    : () => store.remoteAction('allowRecording'),
                label: 'Allow recording',
              ),
            ConsoleButton(
              onPressed: store.remotePending || enabled
                  ? null
                  : () => store.remoteAction('enable'),
              label: 'Enable',
            ),
            ConsoleButton(
              onPressed: store.remotePending || !enabled
                  ? null
                  : () => store.remoteAction('disable'),
              label: 'Disable',
            ),
          ],
        ),
        if (store.remoteError != null)
          ConsoleText(store.remoteError!, tone: ConsoleTextTone.warning),
        const SizedBox(height: 16),
        Divider(color: p.linePanel),
        const SizedBox(height: 8),
        _WebHostingSettings(store: store),
        const SizedBox(height: 16),
        Text(
          'Authorized clients',
          style: TextStyle(color: p.text, fontWeight: FontWeight.bold),
        ),
        if (store.remoteClients.isEmpty)
          Text('No authorized clients', style: TextStyle(color: p.mutedText)),
        for (final client in store.remoteClients)
          Row(
            children: [
              Expanded(
                child: SelectableText(client, style: TextStyle(color: p.text)),
              ),
              ConsoleButton(
                variant: ConsoleButtonVariant.quiet,
                onPressed: store.remotePending
                    ? null
                    : () => store.remoteAction('revoke', client),
                label: 'Revoke',
              ),
            ],
          ),
      ],
    );
  }
}

class _WebHostingSettings extends StatefulWidget {
  final RecorderStore store;
  const _WebHostingSettings({required this.store});
  @override
  State<_WebHostingSettings> createState() => _WebHostingSettingsState();
}

class _WebHostingSettingsState extends State<_WebHostingSettings> {
  final _feedback = InlineFeedbackController();
  final _port = TextEditingController(text: '8846');
  bool _portDirty = false;

  @override
  void dispose() {
    _feedback.dispose();
    _port.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final status = store.remoteStatus;
    final palette = ConsolePalette.of(context);
    final enabled = status['web_enabled'] == true;
    final available = status['web_assets_available'] == true;
    final serverPort = status['web_port'];
    if (!_portDirty &&
        serverPort is num &&
        _port.text != serverPort.toString()) {
      _port.text = serverPort.toString();
    }
    final port = int.tryParse(_port.text);
    final validPort = port != null && port >= 1 && port <= 65535;
    final reason = status['web_reason'];
    final urls = status['web_urls'] is List
        ? (status['web_urls'] as List)
              .whereType<String>()
              .where((url) => url.isNotEmpty)
              .toList()
        : const <String>[];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        InlineFeedbackView(controller: _feedback),
        SwitchListTile.adaptive(
          contentPadding: EdgeInsets.zero,
          title: const Text('Host browser app'),
          subtitle: Text(
            enabled
                ? 'Browser access is enabled'
                : 'Open OpenFader in a browser on another device',
          ),
          value: enabled,
          onChanged:
              store.remotePending || !enabled && (!available || !validPort)
              ? null
              : (value) async {
                  await store.remoteAction(
                    value ? 'web_enable' : 'web_disable',
                    value ? _port.text : null,
                  );
                  if (mounted && store.remoteError == null) {
                    setState(() => _portDirty = false);
                  }
                },
        ),
        const SizedBox(height: 8),
        TextField(
          controller: _port,
          enabled: !enabled && !store.remotePending,
          keyboardType: TextInputType.number,
          decoration: InputDecoration(
            labelText: 'Browser port',
            helperText: enabled
                ? 'Turn off browser hosting to change its port.'
                : 'Default 8846 · separate from app connections',
            errorText: validPort ? null : 'Enter a port from 1 to 65535.',
          ),
          onChanged: (_) => setState(() => _portDirty = true),
        ),
        const SizedBox(height: 12),
        Text(
          'Uses the server password above.',
          style: TextStyle(color: palette.mutedText),
        ),
        if (reason is String && reason.isNotEmpty) ...[
          const SizedBox(height: 8),
          Text(
            reason,
            style: TextStyle(
              color: available ? palette.mutedText : palette.amber,
            ),
          ),
        ] else if (!available) ...[
          const SizedBox(height: 8),
          Text(
            'This host build does not include the browser app.',
            style: TextStyle(color: palette.amber),
          ),
        ],
        if (enabled) ...[
          const SizedBox(height: 12),
          Text(
            'Open in a browser',
            style: TextStyle(color: palette.text, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 4),
          if (urls.isEmpty)
            Text(
              'Use http://<this computer’s LAN IP or hostname>:${status['web_port'] ?? _port.text}',
              style: TextStyle(color: palette.mutedText),
            ),
          for (final url in urls)
            Row(
              children: [
                Expanded(
                  child: SelectableText(
                    url,
                    style: TextStyle(color: palette.text),
                  ),
                ),
                ConsoleIconButton(
                  label: 'Copy browser address',
                  onPressed: () async {
                    await Clipboard.setData(ClipboardData(text: url));
                    if (context.mounted) {
                      _feedback.show('Browser address copied');
                    }
                  },
                  child: const Icon(Icons.copy, size: 18),
                ),
              ],
            ),
        ],
      ],
    );
  }
}
