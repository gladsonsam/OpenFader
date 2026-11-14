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

  void submit() {
    final parts = controller.text.trim().split(':');
    double? seconds;
    if (parts.length == 1) {
      seconds = double.tryParse(parts[0]);
    } else if (parts.length == 2 && RegExp(r'^\d+$').hasMatch(parts[0])) {
      final minutes = int.tryParse(parts[0]);
      final remainder = double.tryParse(parts[1]);
      if (minutes != null &&
          remainder != null &&
          remainder >= 0 &&
          remainder < 60) {
        seconds = minutes * 60 + remainder;
      }
    }
    if (seconds == null ||
        !seconds.isFinite ||
        seconds < 0 ||
        seconds * widget.rate > widget.duration) {
      setState(
        () => error = 'Enter a time within the take (seconds or mm:ss).',
      );
      return;
    }
    Navigator.pop(context, (seconds * widget.rate).round());
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return ConsoleAlertDialog(
      backgroundColor: p.panel,
      title: Text('Go to time', style: TextStyle(color: p.text1)),
      content: TextField(
        controller: controller,
        autofocus: true,
        style: TextStyle(color: p.text1),
        decoration: InputDecoration(
          labelText: 'Seconds or mm:ss',
          labelStyle: TextStyle(color: p.mutedText),
          errorText: error,
        ),
        onSubmitted: (_) => submit(),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: submit, child: const Text('Go')),
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
  String? error;
  @override
  void initState() {
    super.initState();
    start = TextEditingController(
      text: ((widget.start ?? 0) / widget.rate).toStringAsFixed(3),
    );
    end = TextEditingController(
      text: ((widget.end ?? widget.frames) / widget.rate).toStringAsFixed(3),
    );
  }

  @override
  void dispose() {
    start.dispose();
    end.dispose();
    super.dispose();
  }

  double? parse(String value) {
    final parts = value.trim().split(':');
    if (parts.length == 1) return double.tryParse(parts[0]);
    if (parts.length != 2 || !RegExp(r'^\d+$').hasMatch(parts[0])) return null;
    final minutes = int.tryParse(parts[0]);
    final seconds = double.tryParse(parts[1]);
    if (minutes == null || seconds == null || seconds < 0 || seconds >= 60) {
      return null;
    }
    return minutes * 60 + seconds;
  }

  void submit() {
    final a = parse(start.text), b = parse(end.text);
    if (a == null ||
        b == null ||
        !a.isFinite ||
        !b.isFinite ||
        a < 0 ||
        a >= b ||
        b * widget.rate > widget.frames ||
        (a * widget.rate).round() >= (b * widget.rate).round()) {
      setState(() => error = 'Enter a valid range within the take.');
      return;
    }
    Navigator.pop(context, [
      (a * widget.rate).round(),
      (b * widget.rate).round(),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return ConsoleAlertDialog(
      backgroundColor: p.panel,
      title: Text('Loop range', style: TextStyle(color: p.text1)),
      content: SizedBox(
        width: 280,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: start,
              autofocus: true,
              style: TextStyle(color: p.text1),
              decoration: InputDecoration(
                labelText: 'Start (seconds or mm:ss)',
                errorText: error,
              ),
              onSubmitted: (_) => submit(),
            ),
            TextField(
              controller: end,
              style: TextStyle(color: p.text1),
              decoration: const InputDecoration(
                labelText: 'End (seconds or mm:ss)',
              ),
              onSubmitted: (_) => submit(),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: submit, child: const Text('Apply')),
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
      _snapshotMessage('Could not read imported take names on this device.');
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
      _snapshotMessage(
        'Take imported and selected. Could not save its name on this device.',
      );
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
      _snapshotMessage('Could not read snapshot links on this device.');
    }
  }

  void _snapshotMessage(String text) {
    if (mounted) {
      _feedback.show(text);
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
      _snapshotMessage('Could not remove the snapshot link: $e');
    }
  }

  Future<bool> _continueWithoutSnapshot() async {
    if (!mounted) return false;
    return await showDialog<bool>(
          context: context,
          builder: (context) => ConsoleAlertDialog(
            title: const Text('Linked snapshot unavailable'),
            content: const Text(
              'The linked mixer snapshot cannot be verified. Continue without recalling it, or cancel to reconnect and inspect the mixer.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('Cancel'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('Continue without snapshot'),
              ),
            ],
          ),
        ) ??
        false;
  }

  Future<void> _linkSnapshot(String takeId) async {
    final native = widget.nativeStore;
    if (!store.canAdminRemote || native.nativeBusy || !native.nativeWritable) {
      _snapshotMessage(
        'Connect to a local recorder and writable mixer provider, then inspect its snapshots.',
      );
      return;
    }
    await native.nativeAction({'op': 'inspect'}, native.nativeContext);
    if (!mounted) return;
    final view = native.nativeLibrary;
    if (view == null || !view.connected || !view.supports('recall')) {
      _snapshotMessage(
        native.nativeStatus ??
            'Native snapshot recall is unavailable for this provider.',
      );
      return;
    }
    final captured = native.nativeContext;
    final selected = await showDialog<Map<String, dynamic>>(
      context: context,
      builder: (context) => SimpleDialog(
        title: const Text('Link native mixer snapshot'),
        children: [
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 24),
            child: Text(
              'A device-local reference, separate from OpenFader scenes. Create or manage native snapshots in Scenes, then inspect here. Verify the mixer before recall.',
            ),
          ),
          for (final slot in view.slots)
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, slot),
              child: Text(
                '${slot['slot']} · ${slot['name'] == '' ? '(unnamed)' : slot['name']}',
              ),
            ),
          if (_snapshotLinks.containsKey(takeId))
            SimpleDialogOption(
              onPressed: () => Navigator.pop(context, <String, dynamic>{}),
              child: const Text('Remove link'),
            ),
        ],
      ),
    );
    if (!mounted || selected == null) return;
    if (native.nativeContext != captured ||
        !native.nativeWritable ||
        !_takeAvailable(takeId)) {
      _snapshotMessage(
        'The provider or take changed. Inspect and select again.',
      );
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
        _snapshotMessage(
          'The provider or take changed. Inspect and select again.',
        );
        return;
      }
      if (!await prefs.setString(_snapshotKey, jsonEncode(updated))) {
        throw StateError('Preferences write failed');
      }
      if (mounted) setState(() => _snapshotLinks = updated);
    } catch (e) {
      _snapshotMessage('Could not save the snapshot link: $e');
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
    final recall = await showDialog<bool>(
      context: context,
      builder: (context) => ConsoleAlertDialog(
        title: const Text('Recall linked mixer snapshot?'),
        content: Text(
          'Slot ${link['slot']} “${link['name']}” in ${link['label']}. Recall may change mixer settings. The provider name does not identify a particular physical mixer. Return to live restores routing only.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Continue without recall'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Recall snapshot'),
          ),
        ],
      ),
    );
    if (!mounted || recall == null) return false;
    if (!_safeToRecall ||
        native.nativeContext != captured ||
        !native.nativeWritable ||
        !matches() ||
        !_takeAvailable(takeId)) {
      _snapshotMessage('The provider or take changed. Inspect and try again.');
      return false;
    }
    if (!recall) return true;
    await native.nativeAction({'op': 'recall', 'slot': link['slot']}, captured);
    if (!mounted) return false;
    final status = native.nativeStatus;
    _snapshotMessage(
      status ?? 'Snapshot recall result unavailable. Inspect the mixer before continuing.',
    );
    return native.nativeLastConfirmed &&
        _safeToRecall &&
        _takeAvailable(takeId);
  }

  bool _transferBusy = false;

  Future<void> _rejectLinkedParents(String value) async {
    var current = path.absolute(value);
    while (true) {
      if (await FileSystemEntity.type(current, followLinks: false) ==
          FileSystemEntityType.link) {
        throw StateError(
          'Symbolic links cannot be used for take transfers: $current',
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
        'Symbolic links and missing files cannot be transferred.',
      );
    }
    if (await FileSystemEntity.type(destination, followLinks: false) !=
        FileSystemEntityType.notFound) {
      throw StateError('Destination already exists: $destination');
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
      throw StateError('Unsupported file type: $source');
    }
  }

  Future<void> _transferTake({required bool importing, String? takeId}) async {
    if (_transferBusy || store.pending) return;
    if (store.state['input_mode'] != 'live' ||
        store.state['transport'] != 'stopped') {
      _snapshotMessage(
        'Stop playback and return to live before transferring a take.',
      );
      return;
    }
    if (!store.canAdminRemote ||
        !(Platform.isMacOS || Platform.isWindows || Platform.isLinux)) {
      _snapshotMessage(
        'Take transfer is available only on a local desktop recorder host.',
      );
      return;
    }
    final format = await showDialog<String>(
      context: context,
      builder: (context) => SimpleDialog(
        title: Text(importing ? 'Import take' : 'Export take'),
        children: [
          SimpleDialogOption(
            onPressed: () => Navigator.pop(context, 'wav'),
            child: const Text('Multichannel WAV file'),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.pop(context, 'native'),
            child: const Text('Native take folder'),
          ),
          SimpleDialogOption(
            onPressed: () => Navigator.pop(context, 'tracks'),
            child: Text(
              importing
                  ? 'Mono WAV tracks folder (OpenFader export)'
                  : 'Mono WAV tracks folder',
            ),
          ),
          if (!importing)
            const Padding(
              padding: EdgeInsets.symmetric(horizontal: 24),
              child: Text(
                'Multichannel WAV is limited to 4 GiB. For longer takes, export mono tracks or a native take folder.',
              ),
            ),
        ],
      ),
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
    setState(() => _transferBusy = true);
    String? staged;
    String? exportDestination;
    var safeToClean = false;
    try {
      final root = Platform.isMacOS
          ? path.join(
              Platform.environment['HOME'] ?? '',
              'Library',
              'Application Support',
              'OpenFader',
              'Recorder',
              'transfers',
            )
          : Platform.isLinux
          ? path.join(
              (Platform.environment['XDG_DATA_HOME']?.startsWith('/') ?? false)
                  ? Platform.environment['XDG_DATA_HOME']!
                  : path.join(
                      Platform.environment['HOME'] ?? '',
                      '.local',
                      'share',
                    ),
              'OpenFader',
              'Recorder',
              'transfers',
            )
          : path.join(
              Platform.environment['APPDATA'] ?? '',
              'OpenFader',
              'Recorder',
              'transfers',
            );
      if ((Platform.isMacOS
                  ? Platform.environment['HOME']
                  : Platform.isLinux
                  ? (Platform.environment['XDG_DATA_HOME']?.startsWith('/') ==
                            true
                        ? Platform.environment['XDG_DATA_HOME']
                        : Platform.environment['HOME'])
                  : Platform.environment['APPDATA'])
              ?.isNotEmpty !=
          true) {
        throw StateError('Recorder data directory is unavailable.');
      }
      await _rejectLinkedParents(root);
      await _rejectLinkedParents(selected);
      if (!importing) {
        final filename = switch (format) {
          'wav' => '$takeId.wav',
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
            'An export named "$filename" already exists. Choose another folder.',
          );
        }
      }
      await Directory(root).create(recursive: true);
      if (await FileSystemEntity.type(root, followLinks: false) !=
          FileSystemEntityType.directory) {
        throw StateError('Recorder staging directory is not a directory.');
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
        throw StateError('Take is no longer available.');
      }
      if (store.pending ||
          !store.connected ||
          store.state['transport'] != 'stopped') {
        throw StateError(
          'Stop the recorder and connect to the host before transferring.',
        );
      }
      safeToClean = false;
      await store.command(importing ? 'import_take' : 'export_take', {
        'take_id': id,
        'transfer_name': name,
        'format': format,
      });
      if (store.error != null) throw StateError(store.error!);
      // The host acknowledged the transfer. A failed destination copy can
      // discard staging; the source take and import source remain intact.
      safeToClean = true;
      if (!importing) {
        final destination = exportDestination!;
        await _copyTransfer(staged, destination);
        _snapshotMessage('Take exported to $destination');
      } else {
        await _selectImportedTake(id, selected, format);
      }
    } catch (e) {
      debugPrint('Take transfer failed: $e; staging: $staged');
      _snapshotMessage('Transfer failed: ${e is StateError ? e.message : e}');
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
          _snapshotMessage('Could not remove staging data at $staged: $e');
        }
      }
      if (mounted) setState(() => _transferBusy = false);
    }
  }

  Future<void> _playSelected(String? takeId) async {
    if (_startingPlay || store.pending) return;
    void message(String text) {
      if (mounted) {
        _feedback.show(text);
      }
    }

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
      message(
        'This take has an unsupported channel layout (maximum 64 tracks).',
      );
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
          'No supported playback interface found. Connect an interface and refresh devices.',
        );
        return;
      }
      var deviceId = store.state['selected_device_id'] as String?;
      if (!devices.any((d) => d['id'] == deviceId)) {
        deviceId = await showDialog<String>(
          context: context,
          builder: (context) => SimpleDialog(
            title: const Text('Choose playback interface'),
            children: [
              for (final d in devices)
                SimpleDialogOption(
                  onPressed: () => Navigator.pop(context, '${d['id']}'),
                  child: Text(
                    '${d['name']} · ${d['output_channels']} outputs · ${d['sample_rate']} Hz',
                  ),
                ),
            ],
          ),
        );
        if (deviceId == null || !mounted) return;
        await store.command('select_device', {'device_id': deviceId});
        if (store.error != null) {
          message(store.error!);
          return;
        }
      }
      if (!mounted) return;
      final selected = devices.where((d) => d['id'] == deviceId).firstOrNull;
      if (selected == null) {
        message('The selected interface is unavailable.');
        return;
      }
      final outputs = (selected['output_channels'] as num).toInt();
      if (take?['sample_rate'] != selected['sample_rate']) {
        message('The take and interface sample rates must match.');
        return;
      }
      final caps = store.state['capabilities'] as Map? ?? {};
      if (caps['soundcheck'] == true) {
        // The provider revalidates its routing profile and physical state on
        // every entry. Playback never recalls a mixer scene or processing.
        await store.command('enter_soundcheck', {'take_id': takeId});
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
          message(
            'Could not read the previous mapping. Review the outputs before playing.',
          );
        }
        if (!mounted) return;
        final chosen = await showDialog<List<int?>>(
          context: context,
          builder: (context) => StatefulBuilder(
            builder: (context, update) => ConsoleAlertDialog(
              title: const Text('Prepare manual playback'),
              content: SizedBox(
                width: 420,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      '${selected['name']} · ${selected['sample_rate']} Hz · $outputs outputs',
                    ),
                    if (caps['soundcheck'] != true)
                      Padding(
                        padding: const EdgeInsets.symmetric(vertical: 8),
                        child: Text(
                          'Automatic routing unavailable: ${store.state['soundcheck_reason'] ?? 'no verified mixer profile'}',
                        ),
                      ),
                    const Text(
                      'Route the mixer inputs to these interface returns and set safe output levels before playing. OpenFader will not change mixer routing. Restore live inputs yourself afterwards.',
                    ),
                    const SizedBox(height: 8),
                    const Text(
                      'Unassigned tracks are silent. Each assigned output must be unique.',
                    ),
                    Flexible(
                      child: ListView.builder(
                        shrinkWrap: true,
                        itemCount: takeChannels,
                        itemBuilder: (context, i) => Row(
                          children: [
                            Expanded(
                              child: Text(
                                takeNames[i] == null
                                    ? 'Track ${i + 1}'
                                    : '${i + 1} · ${takeNames[i]}',
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                            DropdownButton<int?>(
                              value: mapping[i],
                              items: [
                                const DropdownMenuItem<int?>(
                                  value: null,
                                  child: Text('Unassigned'),
                                ),
                                for (var n = 0; n < outputs; n++)
                                  DropdownMenuItem<int?>(
                                    value: n,
                                    child: Text('Output ${n + 1}'),
                                  ),
                              ],
                              onChanged: (v) => update(() => mapping[i] = v),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: const Text('Cancel'),
                ),
                FilledButton(
                  onPressed: () {
                    final assigned = mapping.whereType<int>().toList();
                    if (assigned.isEmpty ||
                        assigned.toSet().length != assigned.length) {
                      message(
                        'Assign at least one track and use each output only once.',
                      );
                      return;
                    }
                    Navigator.pop(context, mapping);
                  },
                  child: const Text('Routing is ready · play'),
                ),
              ],
            ),
          ),
        );
        if (chosen == null || !mounted) return;
        if (!mounted ||
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
            message('Could not remember this output mapping on this device.');
          }
        }
      }
      if (store.error != null) message(store.error!);
    } finally {
      if (mounted) setState(() => _startingPlay = false);
    }
  }

  Future<void> _returnManualLive() async {
    if (store.pending) return;
    await store.command('stop_manual_output');
    if (store.error != null || !mounted) {
      if (store.error != null) _snapshotMessage(store.error!);
      return;
    }
    if (!mounted || store.pending) return;
    final restored = await showDialog<bool>(
      context: context,
      builder: (context) => ConsoleAlertDialog(
        title: const Text('Restore live inputs'),
        content: const Text(
          'Playback output is stopped. Restore the mixer’s physical live input routing yourself, then confirm below. OpenFader does not change the mixer in manual mode.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Not yet'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Live routing restored'),
          ),
        ],
      ),
    );
    if (restored == true && mounted) {
      await store.command('return_manual_live', {'routing_restored': true});
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
      !store.pending &&
      store.state['available'] == true &&
      store.state['input_mode'] == 'live' &&
      store.state['transport'] == 'stopped';

  Future<void> _newTake() async {
    if (!_canPrepare) return;
    final draft = await showDialog<({String name, String deviceId})>(
      context: context,
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
      _snapshotMessage('Choose your audio interface once, then press Record.');
      return;
    }
    if (_draftId != null && _draftDeviceId != deviceId) {
      _snapshotMessage(
        'The interface changed. Review or create your named take again.',
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
    if (mounted && store.error == null && store.state['input_mode'] == 'live') {
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
    final frames = await showDialog<int>(
      context: context,
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
    final range = await showDialog<List<int>>(
      context: context,
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
      await showDialog<void>(
        context: context,
        builder: (context) => ConsoleAlertDialog(
          title: Text('Take details · ${take['id']}'),
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
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (e) {
      _snapshotMessage('Inspection failed: $e');
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
  Widget build(BuildContext context) => InlineFeedbackScope(
    controller: _feedback,
    expand: true,
    builder: _workspace,
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
      final narrow = MediaQuery.sizeOf(context).width < 650;
      Widget header = SizedBox(
        height: 48,
        child: Row(
          children: [
            IconButton(
              tooltip: 'Adjust mix',
              onPressed: widget.onMixer,
              icon: const Icon(Icons.equalizer),
            ),
            Expanded(
              child: InkWell(
                onTap: store.pending ? null : _browseTakes,
                child: SizedBox(
                  height: 48,
                  child: Row(
                    children: [
                      Flexible(
                        child: Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            color: p.text1,
                            fontSize: 18,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      const Icon(Icons.expand_more, size: 18),
                    ],
                  ),
                ),
              ),
            ),
            IconButton(
              tooltip: 'Audio setup',
              onPressed: () => _showSetup(context, store),
              icon: const Icon(Icons.usb, size: 20),
            ),
            PopupMenuButton<String>(
              tooltip: 'Recorder actions',
              onSelected: (action) {
                switch (action) {
                  case 'browse':
                    _browseTakes();
                  case 'new':
                    _newTake();
                  case 'loop':
                    if (take != null && prepared && enabled) _setLoop(take);
                  case 'clear_loop':
                    if (take != null && prepared && enabled) {
                      _clearLoop(take);
                    }
                  case 'delete':
                    if (take != null) {
                      _deleteTake(store, take['id'] as String, _takeName(take));
                    }
                  case 'details':
                    if (take != null) _inspectTake(take);
                  case 'export':
                    if (take != null) {
                      _transferTake(
                        importing: false,
                        takeId: take['id'] as String,
                      );
                    }
                  case 'snapshot':
                    if (take != null) _linkSnapshot(take['id'] as String);
                  case 'recall':
                    if (take != null && _safeToRecall) {
                      _offerSnapshotRecall(take['id'] as String);
                    }
                  case 'unlink':
                    if (take != null) _unlinkSnapshot(take['id'] as String);
                  case 'fx':
                    widget.onFx();
                  case 'settings':
                    widget.onSettings();
                  case 'tools':
                    widget.onTools();
                }
              },
              itemBuilder: (_) => [
                const PopupMenuItem(
                  value: 'browse',
                  child: Text('Browse takes'),
                ),
                PopupMenuItem(
                  value: 'new',
                  enabled: _canPrepare,
                  child: const Text('New take'),
                ),
                if (prepared && caps['loop'] == true) ...[
                  PopupMenuItem(
                    value: 'loop',
                    enabled: enabled,
                    child: const Text('Loop range…'),
                  ),
                  if (_loopStart != null || s['loop_enabled'] == true)
                    PopupMenuItem(
                      value: 'clear_loop',
                      enabled: enabled,
                      child: const Text('Clear loop'),
                    ),
                ],
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
                    value: 'export',
                    enabled: !_transferBusy && !store.pending,
                    child: const Text('Export take'),
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
          (take?['complete'] == true || soundcheck);
      final canSeek =
          enabled && prepared && caps['seek'] == true && take != null;
      final clock = Text(
        _clock(
          recording
              ? s['recorded_frames']
              : prepared
              ? s['position_frames']
              : 0,
          rate,
        ),
        style: TextStyle(
          color: recording ? p.meterRed : p.text1,
          fontFamily: 'Barlow Condensed',
          fontSize: narrow ? 28 : 30,
          fontFeatures: const [FontFeature.tabularFigures()],
        ),
      );
      Widget transportControls = Row(
        children: [
          if (!soundcheck && !recovery)
            IconButton(
              tooltip: recording ? 'Stop recording' : 'Record',
              style: IconButton.styleFrom(foregroundColor: p.meterRed),
              onPressed: recording
                  ? (store.pending ? null : () => store.command('stop'))
                  : canRecord
                  ? _record
                  : null,
              icon: Icon(recording ? Icons.stop : Icons.fiber_manual_record),
            ),
          if (!recording)
            IconButton(
              tooltip: _startingPlay
                  ? 'Preparing playback'
                  : transport == 'playing'
                  ? 'Pause'
                  : 'Play',
              style: IconButton.styleFrom(foregroundColor: p.lime),
              onPressed: canPlay ? () => _playSelected(previewId) : null,
              icon: _startingPlay
                  ? const SizedBox(
                      width: 20,
                      height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : Icon(
                      transport == 'playing' ? Icons.pause : Icons.play_arrow,
                    ),
            ),
          if (soundcheck)
            IconButton(
              tooltip: 'Stop playback',
              onPressed: enabled ? () => store.command('stop') : null,
              icon: const Icon(Icons.stop),
            ),
          const SizedBox(width: 8),
          Flexible(
            child: Align(
              widthFactor: 1,
              alignment: Alignment.centerLeft,
              child: canSeek
                  ? Tooltip(
                      message: 'Go to time',
                      child: InkWell(
                        onTap: () => _goToTime(take),
                        child: clock,
                      ),
                    )
                  : clock,
            ),
          ),
          if (soundcheck || recovery) const SizedBox(width: 12),
          if (soundcheck || recovery)
            IconButton.filledTonal(
              tooltip: 'Return to live inputs',
              onPressed: enabled ? _returnLive : null,
              icon: const Icon(Icons.input),
            ),
        ],
      );
      return Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          ColoredBox(color: p.recorderSurface, child: header),
          Container(
            color: p.recorderBar,
            padding: const EdgeInsets.symmetric(horizontal: 8),
            child: transportControls,
          ),
          if (recordReason != null &&
              !soundcheck &&
              !recording &&
              !store.pending)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: Text(
                recordReason,
                style: TextStyle(color: p.mutedText, fontSize: 12),
              ),
            ),
          if ((s['xruns'] as num? ?? 0) > 0)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 16),
              child: Text(
                'Audio dropouts: ${s['xruns']}',
                style: TextStyle(color: p.amber, fontSize: 12),
              ),
            ),
          if (!store.connected ||
              store.error != null ||
              s['error'] != null ||
              s['capture_error'] != null)
            Padding(
              padding: const EdgeInsets.all(8),
              child: Text(
                store.error ??
                    '${s['capture_error'] ?? s['error'] ?? 'Recorder host is unavailable.'}',
                style: TextStyle(color: p.amber),
              ),
            ),
          if (recovery && s['manual_routing'] == true)
            panel(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Manual routing needs attention',
                    style: TextStyle(
                      color: p.amber,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'Restore the mixer’s live inputs yourself. Playback remains unavailable until you confirm.',
                    style: TextStyle(color: p.text1),
                  ),
                  FilledButton(
                    onPressed: enabled ? _returnManualLive : null,
                    child: const Text('Restore live inputs'),
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
                            Text(
                              'Routing recovery required',
                              style: TextStyle(
                                color: p.amber,
                                fontWeight: FontWeight.bold,
                                fontSize: 18,
                              ),
                            ),
                            const SizedBox(height: 8),
                            Text(
                              'A routing journal is pending. Recording remains blocked until recovery completes. The host does not display the journal details or independently identify the physical mixer.',
                              style: TextStyle(color: p.text1),
                            ),
                            const SizedBox(height: 8),
                            Text(
                              'Rebind checks a fresh routing snapshot against the journal. It does not change mixer routing. Return to live is a separate step.',
                              style: TextStyle(color: p.text1),
                            ),
                            const SizedBox(height: 12),
                            FilledButton(
                              onPressed: enabled && transport == 'stopped'
                                  ? () async {
                                      final confirmed = await showDialog<bool>(
                                        context: context,
                                        builder: (context) =>
                                            ConsoleAlertDialog(
                                              title: const Text(
                                                'Rebind recovery?',
                                              ),
                                              content: const Text(
                                                'Confirm that the connected mixer is the same physical mixer used for this journal. The app cannot establish physical identity. A fresh routing snapshot will be validated against the journal before rebinding; no routing will be changed by this step.',
                                              ),
                                              actions: [
                                                TextButton(
                                                  onPressed: () =>
                                                      Navigator.pop(
                                                        context,
                                                        false,
                                                      ),
                                                  child: const Text('Cancel'),
                                                ),
                                                FilledButton(
                                                  onPressed: () =>
                                                      Navigator.pop(
                                                        context,
                                                        true,
                                                      ),
                                                  child: const Text(
                                                    'Confirm mixer and rebind',
                                                  ),
                                                ),
                                              ],
                                            ),
                                      );
                                      if (confirmed == true) {
                                        await store.command('rebind_recovery');
                                      }
                                    }
                                  : null,
                              child: const Text('Rebind recovery'),
                            ),
                            const SizedBox(height: 8),
                            OutlinedButton(
                              onPressed: enabled && transport == 'stopped'
                                  ? () async {
                                      final confirmed = await showDialog<bool>(
                                        context: context,
                                        builder: (context) =>
                                            ConsoleAlertDialog(
                                              title: const Text(
                                                'Keep current routing?',
                                              ),
                                              content: const Text(
                                                'Use this after the mixer has already been restored. The host will verify every recorded selector matches its original value, stop playback, and archive the recovery journal. No mixer routing will be changed. Any routing missing from an older journal is accepted as it is now.',
                                              ),
                                              actions: [
                                                TextButton(
                                                  onPressed: () =>
                                                      Navigator.pop(
                                                        context,
                                                        false,
                                                      ),
                                                  child: const Text('Cancel'),
                                                ),
                                                FilledButton(
                                                  onPressed: () =>
                                                      Navigator.pop(
                                                        context,
                                                        true,
                                                      ),
                                                  child: const Text(
                                                    'Keep routing and archive',
                                                  ),
                                                ),
                                              ],
                                            ),
                                      );
                                      if (confirmed == true) {
                                        await store.command(
                                          'accept_current_routing',
                                          {'accept_current': true},
                                        );
                                      }
                                    }
                                  : null,
                              child: const Text(
                                'Accept current routing and archive',
                              ),
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
            child: RecorderArrangement(
              store: store,
              previewTakeId: previewId,
              takeName: _draftName ?? (take != null ? _takeName(take) : null),
              loopStartFrame: _loopTake == previewId ? _loopStart : null,
              loopEndFrame: _loopTake == previewId ? _loopEnd : null,
              onNewTake: _canPrepare ? _newTake : null,
              onAudioSetup: () => _showSetup(context, store),
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

  Future<void> _recoverTake(String id) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => ConsoleAlertDialog(
        title: const Text('Recover take?'),
        content: const Text(
          'Recovery keeps the last saved checkpoint and discards audio recorded after it. Stop recording and playback before continuing.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Recover take'),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await store.command('recover_take', {'take_id': id, 'confirm': true});
    }
  }

  Future<void> _deleteTake(RecorderStore store, String id, String name) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => ConsoleAlertDialog(
        title: const Text('Delete take?'),
        content: Text(
          'Permanently delete “$name” and its recorded audio from the recorder computer? This cannot be undone.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Delete take'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
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

  Future<void> _browseTakes() async {
    if (store.pending) return;
    final id = await showModalBottomSheet<String>(
      context: context,
      isScrollControlled: true,
      useSafeArea: true,
      constraints: const BoxConstraints(maxWidth: 880),
      builder: (context) => _TakeBrowser(
        store: store,
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
      ),
    );
    if (!mounted || id == null) return;
    // Switching the viewed take never changes active playback or mixer routing.
    if (store.state['input_mode'] != 'live' ||
        store.state['transport'] != 'stopped' ||
        store.pending) {
      _snapshotMessage('Return to live inputs before opening another take.');
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
          Widget label(String text) => Text(
            text.toUpperCase(),
            style: TextStyle(color: p.mutedText, fontWeight: FontWeight.bold),
          );
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
                        child: Text(
                          'Audio setup',
                          style: TextStyle(
                            fontSize: 22,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Close audio setup',
                        onPressed: () => Navigator.pop(context),
                        icon: const Icon(Icons.close),
                      ),
                    ],
                  ),
                  panel(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Row(
                          children: [
                            Expanded(child: label('Audio interface')),
                            TextButton(
                              onPressed:
                                  store.pending ||
                                      !store.connected ||
                                      s['device_scan_pending'] == true
                                  ? null
                                  : store.refreshDevices,
                              child: const Text('Refresh devices'),
                            ),
                          ],
                        ),
                        const SizedBox(height: 10),
                        if (store.devices.isEmpty)
                          Text(
                            s['device_scan_error'] != null
                                ? 'Audio device scan failed: ${s['device_scan_error']}'
                                : s['device_scan_pending'] == true
                                ? 'Scanning audio interfaces…'
                                : 'No recording devices available',
                            style: TextStyle(color: p.mutedText),
                          )
                        else
                          DropdownButton<String>(
                            isExpanded: true,
                            value:
                                store.devices.any(
                                  (d) => d['id'] == selectedDevice,
                                )
                                ? '$selectedDevice'
                                : null,
                            hint: const Text('Select an interface'),
                            items: store.devices
                                .map(
                                  (d) => DropdownMenuItem(
                                    value: '${d['id']}',
                                    child: Text(
                                      '${d['name']}  ·  ${d['input_channels']} in / ${d['output_channels']} out',
                                    ),
                                  ),
                                )
                                .toList(),
                            onChanged: enabled && !recording && !soundcheck
                                ? (id) {
                                    if (id != null) {
                                      store.command('select_device', {
                                        'device_id': id,
                                      });
                                    }
                                  }
                                : null,
                          ),
                      ],
                    ),
                  ),
                  if (selectedDevice != null) ...[
                    const SizedBox(height: 12),
                    TextButton.icon(
                      onPressed: enabled && !recording && !soundcheck
                          ? () => showDialog<void>(
                              context: context,
                              builder: (_) => _CaptureNamesDialog(store: store),
                            )
                          : null,
                      icon: const Icon(Icons.account_tree_outlined),
                      label: const Text('Input names and mixer mapping'),
                    ),
                    Text(
                      'Confirm which mixer channel feeds each interface input. Names are saved with recordings; this does not change mixer routing.',
                      style: TextStyle(color: p.mutedText),
                    ),
                  ],
                  if (s['device_restore_reason'] != null)
                    Text(
                      '${s['device_restore_reason']}',
                      style: TextStyle(color: p.amber),
                    ),
                  if (s['device_match_reason'] is String)
                    Text(
                      s['device_match_reason'] as String,
                      style: TextStyle(color: p.mutedText),
                    ),
                  if (s['channel_names_reason'] is String)
                    Text(
                      s['channel_names_reason'] as String,
                      style: TextStyle(color: p.mutedText),
                    ),
                  if (s['disk_free_bytes'] is num)
                    Text(
                      'Available storage: ${((s['disk_free_bytes'] as num) / 1073741824).toStringAsFixed(1)} GB',
                      style: TextStyle(color: p.mutedText),
                    ),
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
    final saved = s['capture_channel_names'];
    _labels = List.generate(
      count,
      (i) => TextEditingController(
        text: saved is List && saved.length == count && saved[i] is String
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
        () => _error = 'The interface or routing changed. Close this dialog and review setup again.',
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
        () => _error = 'Use a name of 1–120 characters for each input, without control characters.',
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
        _error = 'The recorder has not confirmed these names. Refresh setup before retrying.';
      });
      return;
    }
    Navigator.pop(context);
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    final mixerNames = widget.store.channelNames.entries
        .where((e) => e.value.trim().isNotEmpty)
        .toList();
    return ConsoleAlertDialog(
      title: const Text('Input names and mixer mapping'),
      content: SizedBox(
        width: 520,
        height: min(480, MediaQuery.sizeOf(context).height * .55),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(
              '${_device?['name'] ?? 'No interface selected'}',
              style: TextStyle(color: p.text1, fontWeight: FontWeight.w600),
            ),
            const SizedBox(height: 8),
            Text(
              'Match each interface input to the signal it carries. Choose a mixer name or type a label. This saves track names; it does not patch the mixer.',
              style: TextStyle(color: p.mutedText),
            ),
            const SizedBox(height: 12),
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
                          enabled: !_saving,
                          decoration: const InputDecoration(
                            labelText: 'Recorded track name',
                          ),
                        ),
                      ),
                      if (mixerNames.isNotEmpty)
                        PopupMenuButton<String>(
                          tooltip: 'Choose mixer channel',
                          enabled: !_saving,
                          icon: const Icon(Icons.account_tree_outlined),
                          onSelected: (name) => _labels[i].text = name,
                          itemBuilder: (_) => [
                            for (final entry in mixerNames)
                              PopupMenuItem(
                                value: entry.value,
                                child: Text(
                                  '${entry.key + 1} · ${entry.value}',
                                ),
                              ),
                          ],
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
        TextButton(
          onPressed: _saving ? null : () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _saving || _labels.isEmpty ? null : _save,
          child: Text(_saving ? 'Saving…' : 'Save input names'),
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
        () => _error =
            'Enter a take name of 1–120 characters without control characters.',
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
        _error = store.error ?? 'The recorder is not ready. Check the interface and return to live inputs.';
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
      return ConsoleAlertDialog(
        title: const Text('New take'),
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
                      '${device['input_channels']} available inputs · ${device['sample_rate']} Hz\nAll available capture inputs will be recorded.',
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
                  child: TextButton.icon(
                    onPressed:
                        busy || store.state['device_scan_pending'] == true
                        ? null
                        : store.refreshDevices,
                    icon: const Icon(Icons.refresh, size: 18),
                    label: const Text('Refresh interfaces'),
                  ),
                ),
                const SizedBox(height: 8),
                Text(
                  'Prepare the take here, then press Record in the timeline. Audio files are created when recording starts.',
                  style: TextStyle(color: p.mutedText),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: busy ? null : () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: !busy && store.connected && device != null
                ? _prepare
                : null,
            child: Text(_preparing ? 'Preparing…' : 'Create take'),
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
    required this.dateOf,
    required this.durationOf,
    this.onImport,
    this.onRecover,
  });
  final RecorderStore store;
  final String Function(Map<String, dynamic>) nameOf, dateOf, durationOf;
  final Future<void> Function()? onImport;
  final Future<void> Function(String)? onRecover;
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
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      'Browse takes',
                      style: TextStyle(
                        color: p.text1,
                        fontSize: 22,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                  IconButton(
                    tooltip: 'Close takes',
                    onPressed: () => Navigator.pop(context),
                    icon: const Icon(Icons.close),
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
                    IconButton(
                      tooltip: 'Import take',
                      onPressed: canOpen ? widget.onImport : null,
                      icon: const Icon(Icons.file_upload_outlined),
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
              const SizedBox(height: 8),
              Expanded(
                child: takes.isEmpty
                    ? Center(
                        child: Text(
                          _query.isEmpty
                              ? 'No takes yet. Press Record to start.'
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
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                color: p.text1,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                            subtitle: Text(
                              '${widget.durationOf(take)} · ${recorderChannelCount(take)} tracks${date.isEmpty ? '' : ' · $date'}${complete ? '' : ' · Recovery needed'}',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                            onTap: complete && canOpen
                                ? () => Navigator.pop(
                                    context,
                                    take['id'] as String,
                                  )
                                : null,
                            trailing: complete
                                ? Icon(Icons.chevron_right, color: p.mutedText)
                                : widget.onRecover != null
                                ? TextButton(
                                    onPressed: canOpen
                                        ? () => widget.onRecover!(
                                            take['id'] as String,
                                          )
                                        : null,
                                    child: const Text('Recover'),
                                  )
                                : null,
                          );
                        },
                      ),
              ),
            ],
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
          'Connect from another OpenFader app using this computer’s LAN IP address or hostname and port ${store.remoteStatus['port'] ?? 8845}.',
          style: TextStyle(color: p.mutedText),
        ),
        const SizedBox(height: 8),
        Text(
          store.remoteStatus['password_required'] == true
              ? 'Password required'
              : 'No password required',
          style: TextStyle(color: p.text1),
        ),
        const SizedBox(height: 12),
        TextField(
          controller: password,
          obscureText: true,
          enableSuggestions: false,
          autocorrect: false,
          enabled: !store.remotePending,
          style: TextStyle(color: p.text1),
          decoration: const InputDecoration(
            labelText: 'Optional server password',
          ),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 8),
        Text(
          'App and browser connections use this same optional password. Changing it disconnects existing clients.',
          style: TextStyle(color: p.mutedText),
        ),
        const SizedBox(height: 12),
        ConsoleActionGroup(
          children: [
            OutlinedButton(
              onPressed: store.remotePending || password.text.isEmpty
                  ? null
                  : () async {
                      await store.remoteAction('password', password.text);
                      if (mounted && store.remoteError == null) {
                        password.clear();
                        setState(() {});
                      }
                    },
              child: const Text('Set password'),
            ),
            OutlinedButton(
              onPressed:
                  store.remotePending ||
                      store.remoteStatus['password_required'] != true
                  ? null
                  : () => store.remoteAction('password', ''),
              child: const Text('Remove password'),
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
              OutlinedButton(
                onPressed: store.remotePending
                    ? null
                    : () => store.remoteAction('allowRecording'),
                child: const Text('Allow recording'),
              ),
            OutlinedButton(
              onPressed: store.remotePending || enabled
                  ? null
                  : () => store.remoteAction('enable'),
              child: const Text('Enable'),
            ),
            OutlinedButton(
              onPressed: store.remotePending || !enabled
                  ? null
                  : () => store.remoteAction('disable'),
              child: const Text('Disable'),
            ),
          ],
        ),
        const SizedBox(height: 16),
        Divider(color: p.linePanel),
        const SizedBox(height: 8),
        _WebHostingSettings(store: store),
        const SizedBox(height: 16),
        if (store.remoteError != null)
          Text(store.remoteError!, style: TextStyle(color: p.amber)),
        Text(
          'Authorized clients',
          style: TextStyle(color: p.text1, fontWeight: FontWeight.bold),
        ),
        if (store.remoteClients.isEmpty)
          Text('No authorized clients', style: TextStyle(color: p.mutedText)),
        for (final client in store.remoteClients)
          Row(
            children: [
              Expanded(
                child: SelectableText(client, style: TextStyle(color: p.text1)),
              ),
              TextButton(
                onPressed: store.remotePending
                    ? null
                    : () => store.remoteAction('revoke', client),
                child: const Text('Revoke'),
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
          'Uses the optional server password above.',
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
            'The browser app is not available on this host. Install a desktop build that includes the web app.',
            style: TextStyle(color: palette.amber),
          ),
        ],
        if (enabled) ...[
          const SizedBox(height: 12),
          Text(
            'Open in a browser',
            style: TextStyle(color: palette.text1, fontWeight: FontWeight.w600),
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
                    style: TextStyle(color: palette.text1),
                  ),
                ),
                IconButton(
                  tooltip: 'Copy browser address',
                  onPressed: () async {
                    await Clipboard.setData(ClipboardData(text: url));
                    if (context.mounted) {
                      _feedback.show('Browser address copied');
                    }
                  },
                  icon: const Icon(Icons.copy, size: 18),
                ),
              ],
            ),
        ],
      ],
    );
  }
}
