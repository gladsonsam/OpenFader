import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'package:file_picker/file_picker.dart';
import 'package:path/path.dart' as path;
import 'package:shared_preferences/shared_preferences.dart';
import '../core/native_library_store.dart';

import '../palette.dart';
import 'recorder_arrangement.dart';

/// Recorder data and commands are supplied by the shell's host adapter.
abstract class RecorderStore extends ChangeNotifier {
  bool get canAdminRemote;
  Map<String, dynamic> get remoteStatus;
  List<String> get remoteClients;
  String? get pairingJson;
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
      String takeId, int start, int end, int buckets);
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
  final names = source['channel_names'] ??
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
          i: list[i]['name'] as String
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
          () => error = 'Enter a time within the take (seconds or mm:ss).');
      return;
    }
    Navigator.pop(context, (seconds * widget.rate).round());
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return AlertDialog(
      backgroundColor: p.panel,
      title: Text('Go to time', style: TextStyle(color: p.text1)),
      content: TextField(
          controller: controller,
          autofocus: true,
          style: TextStyle(color: p.text1),
          decoration: InputDecoration(
              labelText: 'Seconds or mm:ss',
              labelStyle: TextStyle(color: p.mutedText),
              errorText: error),
          onSubmitted: (_) => submit()),
      actions: [
        TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel')),
        FilledButton(onPressed: submit, child: const Text('Go'))
      ],
    );
  }
}

class _LoopDialog extends StatefulWidget {
  const _LoopDialog(
      {required this.frames, required this.rate, this.start, this.end});
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
        text: ((widget.start ?? 0) / widget.rate).toStringAsFixed(3));
    end = TextEditingController(
        text: ((widget.end ?? widget.frames) / widget.rate).toStringAsFixed(3));
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
    Navigator.pop(
        context, [(a * widget.rate).round(), (b * widget.rate).round()]);
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return AlertDialog(
      backgroundColor: p.panel,
      title: Text('Loop range', style: TextStyle(color: p.text1)),
      content: SizedBox(
          width: 280,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            TextField(
                controller: start,
                autofocus: true,
                style: TextStyle(color: p.text1),
                decoration: InputDecoration(
                    labelText: 'Start (seconds or mm:ss)', errorText: error),
                onSubmitted: (_) => submit()),
            TextField(
                controller: end,
                style: TextStyle(color: p.text1),
                decoration:
                    const InputDecoration(labelText: 'End (seconds or mm:ss)'),
                onSubmitted: (_) => submit()),
          ])),
      actions: [
        TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel')),
        FilledButton(onPressed: submit, child: const Text('Apply'))
      ],
    );
  }
}

class RecorderPage extends StatefulWidget {
  const RecorderPage(
      {super.key,
      required this.store,
      required this.nativeStore,
      required this.onMixer,
      required this.onFx,
      required this.onSettings,
      required this.onTools,
      required this.mixerName});
  final VoidCallback onMixer, onFx, onSettings, onTools;
  final String mixerName;
  final RecorderStore store;
  final NativeLibraryStore nativeStore;

  @override
  State<RecorderPage> createState() => _RecorderPageState();
}

class _RecorderPageState extends State<RecorderPage> {
  RecorderStore get store => widget.store;
  bool _showTakes = true;
  int? _loopStart, _loopEnd, _loopTotal;
  Object? _loopTake;
  String? _previewTakeId;
  String? _importedTakeId;
  final _importedTakeKey = GlobalKey();
  bool _startingPlay = false;

  Map<String, dynamic> _snapshotLinks = {};
  Map<String, String> _importNames = {};
  static const _snapshotKey = 'recorder.native_snapshot_links.v1';
  static const _importNamesKey = 'recorder.import_names.v1';
  static const _manualMappingKey = 'recorder.manual_mapping.v1';

  @override
  void initState() {
    super.initState();
    _loadSnapshotLinks();
    _loadImportNames();
  }

  Future<void> _loadImportNames() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      final value = jsonDecode(prefs.getString(_importNamesKey) ?? '{}');
      if (mounted && value is Map) {
        setState(() => _importNames = {
              for (final entry in value.entries)
                if (entry.key is String && entry.value is String)
                  entry.key as String: entry.value as String,
              ..._importNames,
            });
      }
    } catch (_) {
      _snapshotMessage('Could not read imported take names on this device.');
    }
  }

  Future<void> _selectImportedTake(
      String id, String source, String format) async {
    if (!mounted) return;
    final name = format == 'wav'
        ? path.basenameWithoutExtension(source)
        : path.basename(path.normalize(source));
    setState(() {
      _previewTakeId = id;
      _importedTakeId = id;
      _showTakes = true;
      _loopStart = _loopEnd = _loopTotal = null;
      _loopTake = null;
      _importNames = {..._importNames, id: name};
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final target = _importedTakeKey.currentContext;
      if (mounted && target != null) Scrollable.ensureVisible(target);
    });
    try {
      final prefs = await SharedPreferences.getInstance();
      if (!await prefs.setString(_importNamesKey, jsonEncode(_importNames))) {
        throw StateError('Preferences write failed');
      }
      _snapshotMessage('Imported "$name". Ready to play.');
    } catch (_) {
      _snapshotMessage(
          'Take imported and selected. Could not save its name on this device.');
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
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
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
            builder: (context) => AlertDialog(
                  title: const Text('Linked snapshot unavailable'),
                  content: const Text(
                      'The linked mixer snapshot cannot be verified. Continue without recalling it, or cancel to reconnect and inspect the mixer.'),
                  actions: [
                    TextButton(
                        onPressed: () => Navigator.pop(context, false),
                        child: const Text('Cancel')),
                    FilledButton(
                        onPressed: () => Navigator.pop(context, true),
                        child: const Text('Continue without snapshot'))
                  ],
                )) ??
        false;
  }

  Future<void> _linkSnapshot(String takeId) async {
    final native = widget.nativeStore;
    if (!store.canAdminRemote || native.nativeBusy || !native.nativeWritable) {
      _snapshotMessage(
          'Connect to a local recorder and writable mixer provider, then inspect its snapshots.');
      return;
    }
    await native.nativeAction({'op': 'inspect'}, native.nativeContext);
    if (!mounted) return;
    final view = native.nativeLibrary;
    if (view == null || !view.connected || !view.supports('recall')) {
      _snapshotMessage(native.nativeStatus ??
          'Native snapshot recall is unavailable for this provider.');
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
                    'A device-local reference, separate from OpenFader scenes. Create or manage native snapshots in Scenes, then inspect here. Verify the mixer before recall.')),
            for (final slot in view.slots)
              SimpleDialogOption(
                  onPressed: () => Navigator.pop(context, slot),
                  child: Text(
                      '${slot['slot']} · ${slot['name'] == '' ? '(unnamed)' : slot['name']}')),
            if (_snapshotLinks.containsKey(takeId))
              SimpleDialogOption(
                  onPressed: () => Navigator.pop(context, <String, dynamic>{}),
                  child: const Text('Remove link')),
          ]),
    );
    if (!mounted || selected == null) return;
    if (native.nativeContext != captured ||
        !native.nativeWritable ||
        !_takeAvailable(takeId)) {
      _snapshotMessage(
          'The provider or take changed. Inspect and select again.');
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
        'authority': view.authority
      };
    }
    try {
      final prefs = await SharedPreferences.getInstance();
      if (native.nativeContext != captured ||
          !native.nativeWritable ||
          !_takeAvailable(takeId)) {
        _snapshotMessage(
            'The provider or take changed. Inspect and select again.');
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
    if (!store.canAdminRemote ||
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
        view.slots
            .any((s) => s['slot'] == link['slot'] && s['name'] == link['name']);
    if (!matches()) {
      return _continueWithoutSnapshot();
    }
    final captured = native.nativeContext;
    final recall = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
              title: const Text('Recall linked mixer snapshot?'),
              content: Text(
                  'Slot ${link['slot']} “${link['name']}” in ${link['label']}. Recall may change mixer settings. The provider name does not identify a particular physical mixer. Return to live restores routing only.'),
              actions: [
                TextButton(
                    onPressed: () => Navigator.pop(context, false),
                    child: const Text('Continue without recall')),
                TextButton(
                    onPressed: () => Navigator.pop(context),
                    child: const Text('Cancel')),
                FilledButton(
                    onPressed: () => Navigator.pop(context, true),
                    child: const Text('Recall snapshot')),
              ],
            ));
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
    _snapshotMessage(status ??
        'Snapshot recall result unavailable. Inspect the mixer before continuing.');
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
            'Symbolic links cannot be used for take transfers: $current');
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
          'Symbolic links and missing files cannot be transferred.');
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
              child.path, path.join(destination, path.basename(child.path)));
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
          'Stop playback and return to live before transferring a take.');
      return;
    }
    if (!store.canAdminRemote ||
        !(Platform.isMacOS || Platform.isWindows || Platform.isLinux)) {
      _snapshotMessage(
          'Take transfer is available only on a local desktop recorder host.');
      return;
    }
    final format = await showDialog<String>(
        context: context,
        builder: (context) => SimpleDialog(
              title: Text(importing ? 'Import take' : 'Export take'),
              children: [
                SimpleDialogOption(
                    onPressed: () => Navigator.pop(context, 'wav'),
                    child: const Text('Multichannel WAV file')),
                SimpleDialogOption(
                    onPressed: () => Navigator.pop(context, 'native'),
                    child: const Text('Native take folder')),
                SimpleDialogOption(
                    onPressed: () => Navigator.pop(context, 'tracks'),
                    child: Text(importing
                        ? 'Mono WAV tracks folder (OpenFader export)'
                        : 'Mono WAV tracks folder')),
                if (!importing)
                  const Padding(
                      padding: EdgeInsets.symmetric(horizontal: 24),
                      child: Text(
                          'Multichannel WAV is limited to 4 GiB. For longer takes, export mono tracks or a native take folder.')),
              ],
            ));
    if (!mounted || format == null) return;
    String? selected;
    if (importing && format == 'wav') {
      selected = (await FilePicker.pickFiles(
              type: FileType.custom, allowedExtensions: ['wav']))
          .firstOrNull
          ?.path;
    } else {
      selected = await FilePicker.getDirectoryPath(
          dialogTitle:
              importing ? 'Choose take folder' : 'Choose export destination');
    }
    if (!mounted || selected == null) return;
    setState(() => _transferBusy = true);
    String? staged;
    String? exportDestination;
    var safeToClean = false;
    try {
      final root = Platform.isMacOS
          ? path.join(Platform.environment['HOME'] ?? '', 'Library',
              'Application Support', 'OpenFader', 'Recorder', 'transfers')
          : Platform.isLinux
              ? path.join(
                  (Platform.environment['XDG_DATA_HOME']?.startsWith('/') ??
                          false)
                      ? Platform.environment['XDG_DATA_HOME']!
                      : path.join(Platform.environment['HOME'] ?? '', '.local',
                          'share'),
                  'OpenFader',
                  'Recorder',
                  'transfers')
              : path.join(Platform.environment['APPDATA'] ?? '', 'OpenFader',
                  'Recorder', 'transfers');
      if ((Platform.isMacOS
                  ? Platform.environment['HOME']
                  : Platform.isLinux
                      ? (Platform.environment['XDG_DATA_HOME']
                                  ?.startsWith('/') ==
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
        if (await FileSystemEntity.type(exportDestination,
                followLinks: false) !=
            FileSystemEntityType.notFound) {
          throw StateError(
              'An export named "$filename" already exists. Choose another folder.');
        }
      }
      await Directory(root).create(recursive: true);
      if (await FileSystemEntity.type(root, followLinks: false) !=
          FileSystemEntityType.directory) {
        throw StateError('Recorder staging directory is not a directory.');
      }
      final nonce = List.generate(
          16,
          (_) => Random.secure()
              .nextInt(256)
              .toRadixString(16)
              .padLeft(2, '0')).join();
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
            'Stop the recorder and connect to the host before transferring.');
      }
      safeToClean = false;
      await store.command(importing ? 'import_take' : 'export_take',
          {'take_id': id, 'transfer_name': name, 'format': format});
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
          final entity =
              await FileSystemEntity.type(staged, followLinks: false);
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
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(text)));
      }
    }

    final mode = '${store.state['input_mode']}';
    if (mode == 'soundcheck') {
      await store
          .command(store.state['transport'] == 'playing' ? 'pause' : 'play');
      return;
    }
    if (takeId == null) {
      message('Select a completed take to play.');
      return;
    }
    final take = store.takes.where((t) => t['id'] == takeId).firstOrNull;
    final takeChannels = recorderChannelCount(take);
    if (takeChannels < 1 || takeChannels > 64) {
      message(
          'This take has an unsupported channel layout (maximum 64 tracks).');
      return;
    }
    setState(() => _startingPlay = true);
    try {
      await store.refreshDevices();
      if (!mounted) return;
      final devices = store.devices
          .where((d) =>
              (d['output_channels'] as num? ?? 0) > 0 &&
              (d['output_channels'] as num? ?? 0) <= 64)
          .toList();
      if (devices.isEmpty) {
        message(
            'No supported playback interface found. Connect an interface and refresh devices.');
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
                            onPressed: () =>
                                Navigator.pop(context, '${d['id']}'),
                            child: Text(
                                '${d['name']} · ${d['output_channels']} outputs · ${d['sample_rate']} Hz')),
                    ]));
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
      final choice = caps['soundcheck'] != true
          ? 'manual'
          : await showDialog<String>(
              context: context,
              builder: (context) => SimpleDialog(
                      title: const Text('Choose soundcheck routing'),
                      children: [
                        SimpleDialogOption(
                            onPressed: () => Navigator.pop(context, 'manual'),
                            child: const Text(
                                'Manual routing · any compatible interface')),
                        if (caps['soundcheck'] == true)
                          SimpleDialogOption(
                              onPressed: () =>
                                  Navigator.pop(context, 'automatic'),
                              child: const Text('Automatic mixer routing')),
                      ]));
      if (choice == null || !mounted) return;
      if (!await _offerSnapshotRecall(takeId) ||
          !mounted ||
          store.pending ||
          !store.connected) {
        return;
      }
      if (choice == 'automatic') {
        final confirmed = await showDialog<bool>(
            context: context,
            builder: (context) => AlertDialog(
                    title: const Text('Play virtual soundcheck?'),
                    content: const Text(
                        'Play through the mixer returns. Live input routing will be saved and restored when you return to live.'),
                    actions: [
                      TextButton(
                          onPressed: () => Navigator.pop(context, false),
                          child: const Text('Cancel')),
                      FilledButton(
                          onPressed: () => Navigator.pop(context, true),
                          child: const Text('Play'))
                    ]));
        if (confirmed == true && mounted && !store.pending && store.connected) {
          await store.command('enter_soundcheck', {'take_id': takeId});
        }
      } else {
        var mapping =
            List<int?>.generate(takeChannels, (i) => i < outputs ? i : null);
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
                  (v) => v == null || v is int && v >= 0 && v < outputs)) {
            final assigned = entries.whereType<int>().toList();
            if (assigned.isNotEmpty &&
                assigned.toSet().length == assigned.length) {
              mapping = List<int?>.from(entries);
            }
          }
        } catch (_) {
          message(
              'Could not read the previous mapping. Review the outputs before playing.');
        }
        if (!mounted) return;
        final chosen = await showDialog<List<int?>>(
            context: context,
            builder: (context) => StatefulBuilder(
                builder: (context, update) => AlertDialog(
                      title: const Text('Prepare manual playback'),
                      content: SizedBox(
                          width: 420,
                          child:
                              Column(mainAxisSize: MainAxisSize.min, children: [
                            Text(
                                '${selected['name']} · ${selected['sample_rate']} Hz · $outputs outputs'),
                            if (caps['soundcheck'] != true)
                              Padding(
                                  padding:
                                      const EdgeInsets.symmetric(vertical: 8),
                                  child: Text(
                                      'Automatic routing unavailable: ${store.state['soundcheck_reason'] ?? 'no verified mixer profile'}')),
                            const Text(
                                'Route the mixer inputs to these interface returns and set safe output levels before playing. OpenFader will not change mixer routing. Restore live inputs yourself afterwards.'),
                            const SizedBox(height: 8),
                            const Text(
                                'Unassigned tracks are silent. Each assigned output must be unique.'),
                            Flexible(
                                child: ListView.builder(
                                    shrinkWrap: true,
                                    itemCount: takeChannels,
                                    itemBuilder: (context, i) => Row(children: [
                                          Expanded(
                                              child: Text('Track ${i + 1}')),
                                          DropdownButton<int?>(
                                              value: mapping[i],
                                              items: [
                                                const DropdownMenuItem<int?>(
                                                    value: null,
                                                    child: Text('Unassigned')),
                                                for (var n = 0;
                                                    n < outputs;
                                                    n++)
                                                  DropdownMenuItem<int?>(
                                                      value: n,
                                                      child: Text(
                                                          'Output ${n + 1}'))
                                              ],
                                              onChanged: (v) =>
                                                  update(() => mapping[i] = v))
                                        ])))
                          ])),
                      actions: [
                        TextButton(
                            onPressed: () => Navigator.pop(context),
                            child: const Text('Cancel')),
                        FilledButton(
                            onPressed: () {
                              final assigned =
                                  mapping.whereType<int>().toList();
                              if (assigned.isEmpty ||
                                  assigned.toSet().length != assigned.length) {
                                message(
                                    'Assign at least one track and use each output only once.');
                                return;
                              }
                              Navigator.pop(context, mapping);
                            },
                            child: const Text('Routing is ready · play'))
                      ],
                    )));
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
          'expected_device_id': deviceId
        });
        if (store.error == null) {
          try {
            final prefs = await SharedPreferences.getInstance();
            if (!await prefs.setString(_manualMappingKey,
                jsonEncode({...mappingContext, 'mapping': chosen}))) {
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
        builder: (context) => AlertDialog(
              title: const Text('Restore live inputs'),
              content: const Text(
                  'Playback output is stopped. Restore the mixer’s physical live input routing yourself, then confirm below. OpenFader does not change the mixer in manual mode.'),
              actions: [
                TextButton(
                    onPressed: () => Navigator.pop(context, false),
                    child: const Text('Not yet')),
                FilledButton(
                    onPressed: () => Navigator.pop(context, true),
                    child: const Text('Live routing restored'))
              ],
            ));
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
    final match = RegExp(r'^take-(\d+)$').firstMatch('$id');
    final micros = int.tryParse(match?.group(1) ?? '');
    if (micros == null) return '';
    try {
      final d =
          DateTime.fromMicrosecondsSinceEpoch(micros, isUtc: true).toLocal();
      return ' · ${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')} ${d.hour.toString().padLeft(2, '0')}:${d.minute.toString().padLeft(2, '0')}';
    } catch (_) {
      return '';
    }
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        if (box.maxWidth < 800 || !box.hasBoundedHeight) {
          return _workspace(context);
        }
        final scale = box.maxWidth / 2000;
        return SizedBox.expand(
            child: FittedBox(
                fit: BoxFit.fill,
                child: SizedBox(
                    width: 2000,
                    height: box.maxHeight / scale,
                    child: _workspace(context))));
      });

  Widget _workspace(BuildContext context) => ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final p = ConsolePalette.of(context);
          final s = store.state;
          final transport = '${s['transport'] ?? 'stopped'}';
          final mode = '${s['input_mode'] ?? 'live'}';
          final soundcheck = mode.toLowerCase().contains('soundcheck');
          final recovery = mode.toLowerCase().contains('recovery');
          final needsLive =
              soundcheck || mode.toLowerCase().contains('recovery');
          final recording = transport.toLowerCase().contains('record');
          final selectedTake = s['selected_take_id'];
          final completed =
              store.takes.where((t) => t['complete'] == true).toList();
          if (!completed.any((t) => t['id'] == _previewTakeId)) {
            _previewTakeId =
                (completed.where((t) => t['id'] == selectedTake).firstOrNull ??
                    completed.firstOrNull)?['id'] as String?;
          }
          final previewId = (recording || soundcheck)
              ? selectedTake as String?
              : _previewTakeId;
          final prepared = soundcheck && previewId == selectedTake;
          final selectedDevice = s['selected_device_id'];
          final capabilities =
              s['capabilities'] is Map ? s['capabilities'] as Map : const {};
          final enabled =
              store.connected && !store.pending && s['available'] == true;
          Widget panel({required Widget child}) => Container(
                padding: const EdgeInsets.all(20),
                decoration: BoxDecoration(
                    color: p.panel,
                    border: Border.all(color: p.linePanel),
                    borderRadius: BorderRadius.circular(12)),
                child: child,
              );
          return Column(children: [
            LayoutBuilder(builder: (context, constraints) {
              final scale = (constraints.maxWidth / 2000).clamp(0.45, 1.0);
              Widget fixed(Widget child, double height) =>
                  SingleChildScrollView(
                      scrollDirection: Axis.horizontal,
                      child: SizedBox(
                          width: 2000 * scale,
                          height: height * scale,
                          child: FittedBox(
                              alignment: Alignment.centerLeft,
                              fit: BoxFit.fill,
                              child: SizedBox(
                                  width: 2000, height: height, child: child))));
              Widget headerButton(Widget child, VoidCallback action) =>
                  SizedBox(
                      width: 44,
                      height: 44,
                      child: IconButton(
                          style: IconButton.styleFrom(
                              foregroundColor: p.recorderMuted,
                              shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(9),
                                  side: BorderSide(color: p.recorderBorder))),
                          onPressed: action,
                          icon: child));
              return Column(children: [
                fixed(
                    Container(
                        color: p.recorderSurface,
                        padding: const EdgeInsets.symmetric(horizontal: 32),
                        child: Row(children: [
                          headerButton(
                              const Icon(Icons.home_outlined), widget.onMixer),
                          const SizedBox(width: 20),
                          Text('Recorder',
                              style: TextStyle(
                                  fontFamily: 'Barlow',
                                  fontSize: 34,
                                  fontWeight: FontWeight.w600,
                                  color: p.text)),
                          const Spacer(),
                          Text(widget.mixerName,
                              style: TextStyle(
                                  fontSize: 23, color: p.recorderMuted)),
                          const SizedBox(width: 20),
                          headerButton(
                              const Text('FX',
                                  style: TextStyle(
                                      fontSize: 20,
                                      fontWeight: FontWeight.w600)),
                              widget.onFx),
                          const SizedBox(width: 8),
                          headerButton(const Icon(Icons.settings_outlined),
                              widget.onSettings),
                          const SizedBox(width: 8),
                          TextButton(
                              onPressed: widget.onTools,
                              child: Text('Tools',
                                  style: TextStyle(
                                      color: p.lime,
                                      fontSize: 22,
                                      fontWeight: FontWeight.w600)))
                        ])),
                    104),
                fixed(
                    Container(
                        color: p.recorderBar,
                        padding: const EdgeInsets.symmetric(horizontal: 24),
                        child: Theme(
                            data: Theme.of(context).copyWith(
                                iconButtonTheme: IconButtonThemeData(
                                    style: IconButton.styleFrom(
                                        backgroundColor: p.recorderControl,
                                        foregroundColor: p.recorderMuted,
                                        shape: RoundedRectangleBorder(
                                            borderRadius:
                                                BorderRadius.circular(9),
                                            side: BorderSide(
                                                color: p.recorderBorder)))),
                                textButtonTheme: TextButtonThemeData(
                                    style: TextButton.styleFrom(
                                        foregroundColor: p.recorderMuted,
                                        backgroundColor: p.recorderControl,
                                        textStyle: const TextStyle(
                                            fontFamily: 'Barlow', fontSize: 21),
                                        shape: RoundedRectangleBorder(
                                            borderRadius:
                                                BorderRadius.circular(9),
                                            side: BorderSide(
                                                color: p.recorderBorder))))),
                            child: Row(children: [
                              SizedBox(
                                  width: 60,
                                  height: 56,
                                  child: IconButton(
                                      onPressed: enabled && needsLive
                                          ? () async {
                                              if (s['manual_routing'] == true) {
                                                await _returnManualLive();
                                                return;
                                              }
                                              final confirmed =
                                                  await showDialog<bool>(
                                                context: context,
                                                builder: (context) =>
                                                    AlertDialog(
                                                  title: const Text(
                                                      'Return to live?'),
                                                  content: const Text(
                                                      'Confirm playback output is stopped. This restores the original routing after playback stops.'),
                                                  actions: [
                                                    TextButton(
                                                        onPressed: () =>
                                                            Navigator.pop(
                                                                context, false),
                                                        child: const Text(
                                                            'Cancel')),
                                                    FilledButton(
                                                        onPressed: () =>
                                                            Navigator.pop(
                                                                context, true),
                                                        child: const Text(
                                                            'Return to live')),
                                                  ],
                                                ),
                                              );
                                              if (confirmed == true) {
                                                await store
                                                    .command('return_live');
                                              }
                                            }
                                          : null,
                                      icon: const Icon(Icons.input))),
                              const SizedBox(width: 6),
                              SizedBox(
                                  width: 60,
                                  height: 56,
                                  child: IconButton(
                                      tooltip: transport == 'playing'
                                          ? 'Pause'
                                          : 'Play selected take',
                                      onPressed: store.connected &&
                                              !store.pending &&
                                              !_startingPlay &&
                                              !recording &&
                                              !recovery
                                          ? () => _playSelected(previewId)
                                          : null,
                                      icon: Icon(transport == 'playing'
                                          ? Icons.pause
                                          : Icons.play_arrow))),
                              const SizedBox(width: 6),
                              SizedBox(
                                  width: 60,
                                  height: 56,
                                  child: IconButton(
                                      tooltip: 'Stop',
                                      onPressed: (store.pending ||
                                              (enabled &&
                                                  (recording || soundcheck)))
                                          ? () => store.command('stop')
                                          : null,
                                      icon: const Icon(Icons.stop))),
                              const SizedBox(width: 6),
                              SizedBox(
                                  width: 60,
                                  height: 56,
                                  child: IconButton(
                                      tooltip: 'Record',
                                      onPressed: enabled &&
                                              !needsLive &&
                                              !recording &&
                                              selectedDevice != null &&
                                              capabilities['recording'] == true
                                          ? () => store.command(
                                                  'start_recording', {
                                                'take_id':
                                                    'take-${DateTime.now().toUtc().microsecondsSinceEpoch}'
                                              })
                                          : null,
                                      icon: const Icon(
                                          Icons.fiber_manual_record))),
                              const SizedBox(width: 26),
                              Container(
                                  height: 68,
                                  padding: const EdgeInsets.symmetric(
                                      horizontal: 24),
                                  decoration: BoxDecoration(
                                      color: p.recorderClock,
                                      borderRadius: BorderRadius.circular(10)),
                                  child: Row(children: [
                                    TextButton(
                                        style: TextButton.styleFrom(
                                            padding: EdgeInsets.zero,
                                            minimumSize: Size.zero,
                                            tapTargetSize: MaterialTapTargetSize
                                                .shrinkWrap,
                                            alignment: Alignment.center,
                                            fixedSize:
                                                const Size.fromHeight(68),
                                            backgroundColor: Colors.transparent,
                                            side: BorderSide.none),
                                        onPressed: enabled &&
                                                capabilities['seek'] == true &&
                                                prepared &&
                                                selectedTake != null &&
                                                s['sample_rate'] is num &&
                                                (s['sample_rate'] as num) > 0
                                            ? () async {
                                                final take = store.takes
                                                    .where((t) =>
                                                        t['id'] == selectedTake)
                                                    .firstOrNull;
                                                final duration =
                                                    (take?['duration_frames']
                                                                as num?)
                                                            ?.toDouble() ??
                                                        0;
                                                if (duration <= 0) return;
                                                final frames = await showDialog<
                                                        int>(
                                                    context: context,
                                                    builder: (_) =>
                                                        _GoToTimeDialog(
                                                            duration: duration,
                                                            rate: (s['sample_rate']
                                                                    as num)
                                                                .toDouble()));
                                                if (frames != null &&
                                                    store.connected &&
                                                    !store.pending &&
                                                    store.state[
                                                            'selected_take_id'] ==
                                                        selectedTake &&
                                                    store.state['capabilities']
                                                        is Map &&
                                                    (store.state['capabilities']
                                                            as Map)['seek'] ==
                                                        true) {
                                                  await store.command('seek', {
                                                    'position_frames': frames
                                                  });
                                                }
                                              }
                                            : null,
                                        child: Text(
                                            _clock(
                                                recording
                                                    ? s['recorded_frames']
                                                    : (prepared
                                                        ? s['position_frames']
                                                        : 0),
                                                s['sample_rate']),
                                            style: TextStyle(
                                                color: p.lime,
                                                fontSize: 50,
                                                fontFamily: 'Barlow Condensed',
                                                height: 1,
                                                fontWeight: FontWeight.w600,
                                                fontFeatures: const [
                                                  FontFeature.tabularFigures()
                                                ]),
                                            textHeightBehavior:
                                                const TextHeightBehavior(
                                                    applyHeightToFirstAscent:
                                                        false,
                                                    applyHeightToLastDescent:
                                                        false))),
                                    const SizedBox(width: 22),
                                    Container(
                                        padding:
                                            const EdgeInsets.only(left: 20),
                                        decoration: BoxDecoration(
                                            border: Border(
                                                left: BorderSide(
                                                    color: p.recorderBorder))),
                                        child: Column(
                                            mainAxisSize: MainAxisSize.min,
                                            mainAxisAlignment:
                                                MainAxisAlignment.center,
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              if (!recording)
                                                Text(
                                                    'Position ${(s['position_frames'] is num && s['sample_rate'] is num && (s['sample_rate'] as num) > 0) ? ((s['position_frames'] as num) / (s['sample_rate'] as num)).toStringAsFixed(1) : '0.0'} s',
                                                    style: TextStyle(
                                                        fontSize: 20,
                                                        height: 1.3,
                                                        color:
                                                            p.recorderMuted)),
                                              Text(
                                                  '${recording ? 'Captured' : 'Take length'} ${_time(recording ? s['recorded_frames'] : (completed.where((t) => t['id'] == previewId).firstOrNull)?['duration_frames'], recording ? s['sample_rate'] : (completed.where((t) => t['id'] == previewId).firstOrNull)?['sample_rate'] ?? s['sample_rate'])}',
                                                  style: TextStyle(
                                                      fontSize: 20,
                                                      height: 1.3,
                                                      color: p.recorderMuted))
                                            ]))
                                  ])),
                              const SizedBox(width: 26),
                              Icon(Icons.circle,
                                  size: 10,
                                  color: needsLive ? p.amber : p.lime),
                              const SizedBox(width: 10),
                              Text(
                                  store.pending
                                      ? 'Preparing Soundcheck'
                                      : '${recovery ? 'Recovery required' : soundcheck ? 'Soundcheck' : 'Live'} · ${transport.isEmpty ? 'Stopped' : transport[0].toUpperCase() + transport.substring(1)}',
                                  style: TextStyle(
                                      color: needsLive ? p.amber : p.lime,
                                      fontSize: 23,
                                      fontWeight: FontWeight.w600)),
                              const SizedBox(width: 26),
                              SizedBox(
                                  height: 48,
                                  child: TextButton(
                                      onPressed: null,
                                      child: const Text('Loop'))),
                              const SizedBox(width: 6),
                              SizedBox(
                                  height: 48,
                                  child: TextButton(
                                      onPressed: enabled &&
                                              prepared &&
                                              capabilities['loop'] == true
                                          ? () async {
                                              final take = store.takes
                                                  .where((t) =>
                                                      t['id'] == selectedTake)
                                                  .firstOrNull;
                                              final frames =
                                                  (take?['duration_frames']
                                                              as num?)
                                                          ?.toInt() ??
                                                      0;
                                              final rate =
                                                  (take?['sample_rate'] as num?)
                                                          ?.toDouble() ??
                                                      (s['sample_rate'] as num?)
                                                          ?.toDouble() ??
                                                      0;
                                              if (frames <= 0 || rate <= 0) {
                                                return;
                                              }
                                              final range = await showDialog<
                                                      List<int>>(
                                                  context: context,
                                                  builder: (_) => _LoopDialog(
                                                      frames: frames,
                                                      rate: rate,
                                                      start: _loopTake ==
                                                              selectedTake
                                                          ? _loopStart
                                                          : null,
                                                      end: _loopTake ==
                                                              selectedTake
                                                          ? _loopEnd
                                                          : null));
                                              if (range != null &&
                                                  store.connected &&
                                                  !store.pending &&
                                                  store.state[
                                                          'selected_take_id'] ==
                                                      selectedTake &&
                                                  (store.state['capabilities']
                                                          as Map?)?['loop'] ==
                                                      true) {
                                                await store.command(
                                                    'set_loop', {
                                                  'start_frame': range[0],
                                                  'end_frame': range[1],
                                                  'enabled': true
                                                });
                                                if (mounted &&
                                                    store.error == null) {
                                                  setState(() {
                                                    _loopStart = range[0];
                                                    _loopEnd = range[1];
                                                    _loopTotal = frames;
                                                    _loopTake = selectedTake;
                                                  });
                                                }
                                              }
                                            }
                                          : null,
                                      child: const Text('Set loop'))),
                              const SizedBox(width: 6),
                              SizedBox(
                                  height: 48,
                                  child: TextButton(
                                      onPressed: enabled &&
                                              prepared &&
                                              capabilities['loop'] == true
                                          ? () => store.command('set_loop', {
                                                'start_frame': 0,
                                                'end_frame': _loopTake ==
                                                            selectedTake &&
                                                        _loopTotal != null
                                                    ? _loopTotal
                                                    : (((s['sample_rate']
                                                                        as num?)
                                                                    ?.toDouble() ??
                                                                48000) *
                                                            30)
                                                        .round(),
                                                'enabled': false
                                              })
                                          : null,
                                      child: const Text('Clear loop'))),
                              const Spacer(),
                              FilledButton.icon(
                                  onPressed: widget.onMixer,
                                  icon: const Icon(Icons.tune),
                                  label: const Text('Mixer view')),
                              const SizedBox(width: 8),
                              const Tooltip(
                                  message: 'Markers are not supported',
                                  child: IconButton(
                                      onPressed: null,
                                      icon: Icon(Icons.bookmarks_outlined))),
                              const SizedBox(width: 8),
                              IconButton(
                                  onPressed: () => _showSetup(context, store),
                                  tooltip: 'Settings',
                                  icon: const Icon(Icons.tune))
                            ]))),
                    96),
              ]);
            }),
            if (!store.connected || store.error != null || s['error'] != null)
              Padding(
                  padding: const EdgeInsets.all(8),
                  child: Text(
                      store.error ??
                          '${s['error'] ?? 'Recorder host is unavailable.'}',
                      style: TextStyle(color: p.amber))),
            if (recovery && s['manual_routing'] == true)
              panel(
                  child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                    Text('Manual routing needs attention',
                        style: TextStyle(
                            color: p.amber, fontWeight: FontWeight.bold)),
                    const SizedBox(height: 8),
                    Text(
                        'Restore the mixer’s live inputs yourself. Playback remains unavailable until you confirm.',
                        style: TextStyle(color: p.text1)),
                    FilledButton(
                        onPressed: enabled ? _returnManualLive : null,
                        child: const Text('Restore live inputs')),
                  ])),
            if (recovery && s['manual_routing'] != true)
              SizedBox(
                  height: 220,
                  child: SingleChildScrollView(
                      child: Column(children: [
                    if (recovery) ...[
                      panel(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text('Routing recovery required',
                                style: TextStyle(
                                    color: p.amber,
                                    fontWeight: FontWeight.bold,
                                    fontSize: 18)),
                            const SizedBox(height: 8),
                            Text(
                                'A routing journal is pending. Recording remains blocked until recovery completes. The host does not display the journal details or independently identify the physical mixer.',
                                style: TextStyle(color: p.text1)),
                            const SizedBox(height: 8),
                            Text(
                                'Rebind checks a fresh routing snapshot against the journal. It does not change mixer routing. Return to live is a separate step.',
                                style: TextStyle(color: p.text1)),
                            const SizedBox(height: 12),
                            FilledButton(
                              onPressed: enabled && transport == 'stopped'
                                  ? () async {
                                      final confirmed = await showDialog<bool>(
                                          context: context,
                                          builder: (context) => AlertDialog(
                                                title: const Text(
                                                    'Rebind recovery?'),
                                                content: const Text(
                                                    'Confirm that the connected mixer is the same physical mixer used for this journal. The app cannot establish physical identity. A fresh routing snapshot will be validated against the journal before rebinding; no routing will be changed by this step.'),
                                                actions: [
                                                  TextButton(
                                                      onPressed: () =>
                                                          Navigator.pop(
                                                              context, false),
                                                      child:
                                                          const Text('Cancel')),
                                                  FilledButton(
                                                      onPressed: () =>
                                                          Navigator.pop(
                                                              context, true),
                                                      child: const Text(
                                                          'Confirm mixer and rebind')),
                                                ],
                                              ));
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
                                        builder: (context) => AlertDialog(
                                          title: const Text(
                                              'Keep current routing?'),
                                          content: const Text(
                                              'Use this after the mixer has already been restored. The host will verify every recorded selector matches its original value, stop playback, and archive the recovery journal. No mixer routing will be changed. Any routing missing from an older journal is accepted as it is now.'),
                                          actions: [
                                            TextButton(
                                              onPressed: () =>
                                                  Navigator.pop(context, false),
                                              child: const Text('Cancel'),
                                            ),
                                            FilledButton(
                                              onPressed: () =>
                                                  Navigator.pop(context, true),
                                              child: const Text(
                                                  'Keep routing and archive'),
                                            ),
                                          ],
                                        ),
                                      );
                                      if (confirmed == true) {
                                        await store.command(
                                            'accept_current_routing',
                                            {'accept_current': true});
                                      }
                                    }
                                  : null,
                              child: const Text(
                                  'Accept current routing and archive'),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 16),
                    ],
                  ]))),
            Expanded(child: LayoutBuilder(builder: (context, constraints) {
              final desktop = constraints.maxWidth >= 800;
              return Stack(children: [
                Row(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
                  if (desktop && _showTakes)
                    SizedBox(width: 270, child: _takesBrowser(context, store)),
                  Expanded(
                      child: RecorderArrangement(
                          store: store,
                          previewTakeId: previewId,
                          onLoopSelection: (start, end, total) => setState(() {
                                _loopStart = start;
                                _loopEnd = end;
                                _loopTotal = total;
                                _loopTake = store.state['selected_take_id'];
                              }))),
                ]),
                if (desktop)
                  Positioned(
                      left: _showTakes ? 246 : 0,
                      top: (constraints.maxHeight - 76) / 2,
                      child: SizedBox(
                          width: 24,
                          height: 76,
                          child: IconButton(
                              padding: EdgeInsets.zero,
                              style: IconButton.styleFrom(
                                  backgroundColor: p.recorderControl,
                                  shape: const RoundedRectangleBorder(
                                      borderRadius: BorderRadius.horizontal(
                                          left: Radius.circular(10)))),
                              tooltip: _showTakes ? 'Hide takes' : 'Show takes',
                              onPressed: () =>
                                  setState(() => _showTakes = !_showTakes),
                              icon: Icon(_showTakes
                                  ? Icons.chevron_left
                                  : Icons.chevron_right)))),
              ]);
            })),
          ]);
        },
      );
  Widget _takesBrowser(BuildContext context, RecorderStore store) {
    return ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final p = ConsolePalette.of(context);
          final s = store.state;
          final selectedTake =
              (('${s['transport']}'.toLowerCase().contains('record') ||
                      '${s['input_mode']}'.toLowerCase().contains('soundcheck'))
                  ? s['selected_take_id']
                  : _previewTakeId);
          final enabled =
              store.connected && !store.pending && s['available'] == true;
          final recording =
              '${s['transport']}'.toLowerCase().contains('record');
          final soundcheck =
              '${s['input_mode']}'.toLowerCase().contains('soundcheck');
          String takeSummary(Map<String, dynamic> take) {
            final active = recording && take['id'] == s['selected_take_id'];
            final frames = active
                ? s['recorded_frames'] ?? take['duration_frames']
                : take['duration_frames'];
            final rate = active
                ? s['sample_rate'] ?? take['sample_rate']
                : take['sample_rate'];
            final status = active
                ? s['error'] == null
                    ? 'Recording'
                    : 'Recording error'
                : take['complete'] == true
                    ? 'Complete'
                    : 'Recovery needed';
            return '${_time(frames, rate)}  ·  $status';
          }

          Widget panel({required Widget child}) => Container(
              padding: const EdgeInsets.fromLTRB(16, 20, 16, 20),
              color: p.recorderSurface,
              child: child);
          return SingleChildScrollView(
              child: panel(
                  child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                Text('Takes',
                    style: TextStyle(
                        color: p.recorderMuted,
                        fontSize: 24,
                        fontWeight: FontWeight.w600)),
                const SizedBox(height: 10),
                TextButton.icon(
                    onPressed: _transferBusy || store.pending
                        ? null
                        : () => _transferTake(importing: true),
                    icon: const Icon(Icons.file_upload_outlined),
                    label:
                        Text(_transferBusy ? 'Transferring…' : 'Import take')),
                if (!store.canAdminRemote ||
                    !(Platform.isMacOS ||
                        Platform.isWindows ||
                        Platform.isLinux))
                  Text('Transfer requires a local desktop recorder host.',
                      style: TextStyle(color: p.mutedText)),
                if (store.takes.isEmpty)
                  Text('Your recorded takes will appear here.',
                      style: TextStyle(color: p.mutedText)),
                for (final (index, take) in store.takes.indexed)
                  Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        InkWell(
                            key: take['id'] == _importedTakeId
                                ? _importedTakeKey
                                : null,
                            onTap: !recording &&
                                    !soundcheck &&
                                    take['complete'] == true
                                ? () => setState(() {
                                      _previewTakeId = take['id'] as String;
                                      _loopStart = null;
                                      _loopEnd = null;
                                    })
                                : null,
                            child: Container(
                                padding: const EdgeInsets.all(14),
                                decoration: BoxDecoration(
                                    color: take['id'] == selectedTake
                                        ? p.recorderOlive
                                        : p.recorderSurface,
                                    borderRadius: BorderRadius.circular(10),
                                    border: Border.all(
                                        color: take['id'] == selectedTake
                                            ? p.recorderOliveBorder
                                            : p.recorderSurface)),
                                child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                      Row(children: [
                                        Icon(
                                            take['id'] == selectedTake
                                                ? Icons.graphic_eq
                                                : Icons.audio_file_outlined,
                                            color: p.lime),
                                        const SizedBox(width: 10),
                                        Expanded(
                                            child: Tooltip(
                                                message: '${take['id']}',
                                                child: Text(
                                                    (take['name'] is String &&
                                                            (take['name']
                                                                    as String)
                                                                .trim()
                                                                .isNotEmpty)
                                                        ? take['name'] as String
                                                        : _importNames[
                                                                take['id']] ??
                                                            'Take ${index + 1}',
                                                    maxLines: 1,
                                                    overflow:
                                                        TextOverflow.ellipsis,
                                                    style: TextStyle(
                                                        color: p.text1,
                                                        fontSize: 25,
                                                        fontWeight:
                                                            FontWeight.w600))))
                                      ]),
                                      Text(
                                          '${takeSummary(take)}\n${_takeDate(take['id']).replaceFirst(' · ', '')}',
                                          style: TextStyle(
                                              color: p.mutedText,
                                              fontSize: 19)),
                                    ]))),
                        if (take['id'] == selectedTake &&
                            take['complete'] == true) ...[
                          Text(
                              _snapshotLinks[take['id']] is Map
                                  ? 'Native snapshot: ${_snapshotLinks[take['id']]['slot']} · ${_snapshotLinks[take['id']]['name']} (${_snapshotLinks[take['id']]['label']})'
                                  : 'No native snapshot linked',
                              style: TextStyle(color: p.mutedText)),
                          TextButton(
                              onPressed: _transferBusy || store.pending
                                  ? null
                                  : () => _transferTake(
                                      importing: false,
                                      takeId: take['id'] as String),
                              child: const Text('Export take')),
                          if (_snapshotLinks.containsKey(take['id']))
                            TextButton(
                                onPressed: () =>
                                    _unlinkSnapshot(take['id'] as String),
                                child: const Text('Remove snapshot link')),
                          TextButton(
                              onPressed: enabled && !recording && !soundcheck
                                  ? () => _linkSnapshot(take['id'] as String)
                                  : null,
                              child: const Text('Link native snapshot')),
                        ],
                        const SizedBox(height: 8),
                        SizedBox(
                            height: 48,
                            child: OutlinedButton(
                                onPressed: enabled &&
                                        !recording &&
                                        !soundcheck &&
                                        take['complete'] == true &&
                                        take['id'] == selectedTake
                                    ? () => _playSelected(take['id'] as String)
                                    : null,
                                child: const Text('Soundcheck'))),
                      ]),
              ])));
        });
  }

  static void _showSetup(BuildContext context, RecorderStore store) {
    showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        builder: (context) => ListenableBuilder(
            listenable: store,
            builder: (context, _) {
              final p = ConsolePalette.of(context);
              final s = store.state;
              final selectedDevice = s['selected_device_id'];
              final enabled =
                  store.connected && !store.pending && s['available'] == true;
              final recording =
                  '${s['transport']}'.toLowerCase().contains('record');
              final soundcheck =
                  '${s['input_mode']}'.toLowerCase().contains('soundcheck');
              Widget label(String text) => Text(text.toUpperCase(),
                  style: TextStyle(
                      color: p.mutedText, fontWeight: FontWeight.bold));
              Widget panel({required Widget child}) => Container(
                  padding: const EdgeInsets.all(12),
                  color: p.panel,
                  child: child);
              return SizedBox(
                  height: MediaQuery.sizeOf(context).height * .75,
                  child: SingleChildScrollView(
                      padding: const EdgeInsets.all(16),
                      child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            if (store.canAdminRemote)
                              panel(child: _RemoteAdminPanel(store: store)),
                            panel(
                                child: Column(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.stretch,
                                    children: [
                                  Row(children: [
                                    Expanded(child: label('Audio interface')),
                                    TextButton(
                                        onPressed: store.pending ||
                                                !store.connected ||
                                                s['device_scan_pending'] == true
                                            ? null
                                            : store.refreshDevices,
                                        child: const Text('Refresh devices')),
                                  ]),
                                  const SizedBox(height: 10),
                                  if (store.devices.isEmpty)
                                    Text(
                                        s['device_scan_error'] != null
                                            ? 'Audio device scan failed: ${s['device_scan_error']}'
                                            : s['device_scan_pending'] == true
                                                ? 'Scanning audio interfaces…'
                                                : 'No recording devices available',
                                        style: TextStyle(color: p.mutedText))
                                  else
                                    DropdownButton<String>(
                                        isExpanded: true,
                                        value: store.devices.any((d) =>
                                                d['id'] == selectedDevice)
                                            ? '$selectedDevice'
                                            : null,
                                        hint: const Text('Select an interface'),
                                        items: store.devices
                                            .map((d) => DropdownMenuItem(
                                                value: '${d['id']}',
                                                child: Text(
                                                    '${d['name']}  ·  ${d['input_channels']} in / ${d['output_channels']} out')))
                                            .toList(),
                                        onChanged:
                                            enabled && !recording && !soundcheck
                                                ? (id) {
                                                    if (id != null) {
                                                      store.command(
                                                          'select_device',
                                                          {'device_id': id});
                                                    }
                                                  }
                                                : null),
                                ])),
                          ])));
            }));
  }
}

class _RemoteAdminPanel extends StatefulWidget {
  const _RemoteAdminPanel({required this.store});
  final RecorderStore store;
  @override
  State<_RemoteAdminPanel> createState() => _RemoteAdminPanelState();
}

class _RemoteAdminPanelState extends State<_RemoteAdminPanel> {
  final ip = TextEditingController();
  @override
  void dispose() {
    ip.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final p = ConsolePalette.of(context);
    final enabled = store.remoteStatus['enabled'] == true;
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text('Remote pairing',
          style: TextStyle(
              color: p.text1, fontSize: 18, fontWeight: FontWeight.bold)),
      Text(
          enabled ? 'Enabled · port ${store.remoteStatus['port']}' : 'Disabled',
          style: TextStyle(color: p.mutedText)),
      TextField(
          controller: ip,
          style: TextStyle(color: p.text1),
          decoration: const InputDecoration(labelText: 'LAN IPv4 address'),
          keyboardType: TextInputType.url),
      Wrap(spacing: 8, children: [
        OutlinedButton(
            onPressed: store.remotePending
                ? null
                : () => store.remoteAction('allowRecording'),
            child: const Text('Allow recording')),
        OutlinedButton(
            onPressed: store.remotePending || enabled
                ? null
                : () => store.remoteAction('enable', ip.text),
            child: const Text('Enable')),
        OutlinedButton(
            onPressed: store.remotePending || !enabled
                ? null
                : () => store.remoteAction('pairing', ip.text),
            child: const Text('Create pairing')),
        OutlinedButton(
            onPressed: store.remotePending || !enabled
                ? null
                : () => store.remoteAction('disable'),
            child: const Text('Disable')),
      ]),
      if (store.remoteError != null)
        Text(store.remoteError!, style: TextStyle(color: p.amber)),
      if (store.pairingJson != null) ...[
        SelectableText(store.pairingJson!, style: TextStyle(color: p.text1)),
        TextButton.icon(
            onPressed: () =>
                Clipboard.setData(ClipboardData(text: store.pairingJson!)),
            icon: const Icon(Icons.copy),
            label: const Text('Copy pairing JSON')),
      ],
      Text('Paired clients',
          style: TextStyle(color: p.text1, fontWeight: FontWeight.bold)),
      if (store.remoteClients.isEmpty)
        Text('No paired clients', style: TextStyle(color: p.mutedText)),
      for (final client in store.remoteClients)
        Row(children: [
          Expanded(
              child: SelectableText(client, style: TextStyle(color: p.text1))),
          TextButton(
              onPressed: store.remotePending
                  ? null
                  : () => store.remoteAction('revoke', client),
              child: const Text('Revoke'))
        ]),
    ]);
  }
}
