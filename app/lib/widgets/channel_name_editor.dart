import 'dart:async';
import 'dart:typed_data';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:flutter/semantics.dart';

import '../core/channel_clipboard.dart';
import '../core/channel_name.dart';
import '../core/processor_presets.dart';
import '../design.dart';
import '../palette.dart';
import '../ui/controls/console_action_group.dart';
import '../ui/overlays/anchored_menu.dart';
import 'channel_symbol.dart';

String _identityLabel(ChannelNameTarget target) =>
    target.label ??
    (target.key.startsWith('bus/')
        ? 'Bus ${int.tryParse(target.key.split('/').last) ?? target.key}'
        : 'Channel ${target.channel + 1}');

Future<void> showChannelNameEditor(
    BuildContext context, ChannelNameStore store, ChannelNameTarget target,
    {Rect? anchorBounds}) {
  final processing = store is ChannelClipboardStore
      ? (store as ChannelClipboardStore).captureChannelProcessingTarget(target)
      : null;
  final presets = store is InputProcessorPresetStore
      ? <ProcessorPresetTarget>[
          for (final kind in [
            ProcessorPresetKind.eq,
            ProcessorPresetKind.gate,
            ProcessorPresetKind.compressor,
          ])
            if ((store as InputProcessorPresetStore).captureChannelPresetTarget(
              kind,
              target.channel,
              session: target.session,
              key: target.key,
              accessEpoch: target.accessEpoch,
            )
                case final captured?)
              captured,
        ]
      : <ProcessorPresetTarget>[];
  return showAnchoredPopover<void>(
    context,
    (_) => _NameEditor(
        store: store, target: target, presets: presets, processing: processing),
    anchorBounds: anchorBounds,
  );
}

enum _ChannelPage {
  overview,
  color,
  icon,
  assign,
  more,
  presets,
  library,
  preset,
  presetName,
  info,
  confirm,
}

class _NameEditor extends StatefulWidget {
  final ChannelNameStore store;
  final ChannelNameTarget target;
  final List<ProcessorPresetTarget> presets;
  final ChannelProcessingTarget? processing;
  const _NameEditor({
    required this.store,
    required this.target,
    required this.presets,
    required this.processing,
  });
  @override
  State<_NameEditor> createState() => _NameEditorState();
}

/// One route, one captured identity and one operation owner. Internal pages
/// never substitute a target or create another overlay.
class _NameEditorState extends State<_NameEditor> {
  late final _text = TextEditingController(text: widget.target.name);
  final _presetName = TextEditingController();
  final _history = <_ChannelPage>[_ChannelPage.overview];
  int _pageVersion = 0;
  bool _backward = false;
  String? _assignmentPendingId, _assignmentErrorId, _assignmentFailure;
  bool _pending = false, _clipboardActive = false;
  bool _colorSent = false, _iconSaved = false, _statusError = false;
  String? _nameError, _status;
  String _infoTitle = '', _infoText = '';
  String _confirmTitle = '', _confirmText = '', _confirmAction = '';
  Completer<bool>? _confirmation;
  double? _appearanceValue;
  ChannelIconSelection _iconSelection =
      const ChannelIconSelection.followProvider();
  bool _syncNative = false;
  ProcessorPresetTarget? _presetTarget;
  ProcessorPreset? _preset;
  bool _renamingPreset = false, _libraryLoaded = false;
  List<ProcessorPreset> _library = const [];

  _ChannelPage get _page => _history.last;
  InputProcessorPresetStore get _presetStore =>
      widget.store as InputProcessorPresetStore;
  ChannelClipboardStore get _clipboardStore =>
      widget.store as ChannelClipboardStore;

  void _go(_ChannelPage page) {
    if (_pending) return;
    FocusScope.of(context).unfocus();
    setState(() {
      _status = null;
      _transition(backward: false);
      _history.add(page);
    });
  }

  void _back() {
    if (_pending || _history.length == 1) return;
    if (_page == _ChannelPage.confirm) {
      _resolveConfirmation(false);
      return;
    }
    FocusScope.of(context).unfocus();
    setState(() {
      _transition(backward: true);
      _history.removeLast();
      _status = null;
    });
  }

  void _transition({required bool backward}) {
    _backward = backward;
    _pageVersion++;
  }

  void _message(String message, {bool error = false}) {
    if (!mounted) return;
    setState(() {
      _status = message;
      _statusError = error;
    });
  }

  Future<void> _run(Future<void> Function() action) async {
    if (_pending) return;
    setState(() {
      _pending = true;
      _status = null;
    });
    try {
      await action();
    } catch (error) {
      _message('$error', error: true);
    } finally {
      if (mounted) setState(() => _pending = false);
    }
  }

  Future<bool> _confirm(String title, String message, String action) {
    if (_confirmation != null) return Future.value(false);
    _confirmTitle = title;
    _confirmText = message;
    _confirmAction = action;
    final completion = Completer<bool>();
    _confirmation = completion;
    _go(_ChannelPage.confirm);
    return completion.future;
  }

  void _resolveConfirmation(bool confirmed) {
    final completion = _confirmation;
    if (completion == null || _pending) return;
    setState(() {
      _confirmation = null;
      _transition(backward: true);
      _history.removeLast();
    });
    completion.complete(confirmed);
  }

  void _info(String title, String message) {
    _infoTitle = title;
    _infoText = message;
    _go(_ChannelPage.info);
  }

  @override
  void dispose() {
    if (_clipboardActive) _clipboardStore.cancelChannelProcessing();
    _confirmation?.complete(false);
    _text.dispose();
    _presetName.dispose();
    super.dispose();
  }

  Future<void> _rename() => _run(() async {
        final error =
            await widget.store.renameChannel(widget.target, _text.text);
        if (!mounted) return;
        if (error == null) {
          Navigator.pop(context);
        } else {
          setState(() => _nameError = error);
        }
      });

  void _appearance(bool icons) {
    if (icons) {
      _iconSelection = widget.store.iconForChannel(widget.target);
      _syncNative = false;
    } else {
      _appearanceValue = widget.store.colorForChannel(widget.target);
    }
    _go(icons ? _ChannelPage.icon : _ChannelPage.color);
  }

  Future<void> _applyAppearance() => _run(() async {
        final value = _appearanceValue;
        if (value == null) return;
        final error = await widget.store.colorChannel(widget.target, value);
        if (!mounted) return;
        if (error != null) {
          _message(error, error: true);
        } else {
          setState(() {
            _colorSent = true;
            _transition(backward: true);
            _history.removeLast();
          });
        }
      });

  Future<void> _applyIcon() => _run(() async {
        final result = await widget.store.iconChannel(
            widget.target, _iconSelection,
            syncNative: _syncNative);
        if (!mounted) return;
        if (result.saved) setState(() => _iconSaved = true);
        _message(
            result.saved &&
                    result.native == ChannelIconNativeOutcome.notRequested
                ? 'Saved on this device.'
                : result.message,
            error: !result.saved ||
                result.native == ChannelIconNativeOutcome.failed ||
                result.native == ChannelIconNativeOutcome.unavailable);
      });

  Future<void> _importIcon() => _run(() async {
        final file = await FilePicker.pickFile(
            type: FileType.custom,
            allowedExtensions: const ['png', 'jpg', 'jpeg', 'webp']);
        if (file == null || !mounted) return;
        const limit = 8 * 1024 * 1024;
        if ((await file.length() ?? 0) > limit) {
          throw StateError('Choose an image smaller than 8 MB.');
        }
        final bytes = BytesBuilder(copy: false);
        await for (final chunk in file.readAsByteStream()) {
          if (bytes.length + chunk.length > limit) {
            throw StateError('Choose an image smaller than 8 MB.');
          }
          bytes.add(chunk);
        }
        final selection = await widget.store
            .importChannelIcon(widget.target, bytes.takeBytes());
        if (!mounted) return;
        setState(() {
          _iconSelection = selection;
          _syncNative = false;
        });
        _message('Imported. Apply to use this image.');
      });

  Future<void> _removeImportedIcon(ChannelIconSelection selection) async {
    if (_pending || widget.store.iconEditError(widget.target) != null) return;
    if (!await _confirm(
        'Remove image?',
        'Remove this image from this device? Images used by saved channel icons are kept.',
        'Remove')) {
      return;
    }
    await _run(() async {
      await widget.store.removeUnusedChannelIcon(widget.target, selection);
      if (!mounted) return;
      if (_iconSelection == selection) {
        setState(() {
          _iconSelection = widget.store.iconForChannel(widget.target);
          _syncNative = false;
        });
      }
      _message('Image removed.');
    });
  }

  Future<void> _assign(String id, bool value, bool expected) async {
    if (_pending) return;
    setState(() {
      _assignmentPendingId = id;
      _assignmentErrorId = null;
      _assignmentFailure = null;
      _statusError = false;
    });
    await _run(() async {
      final result = await widget.store
          .assignChannel(widget.target, id, value, expected: expected);
      _message(
          result.error ??
              'Assignment matches your choice. Mixer confirmation is unavailable.',
          error: result.error != null);
    });
    if (mounted) {
      setState(() {
        _assignmentPendingId = null;
        if (_statusError) {
          _assignmentErrorId = id;
          _assignmentFailure = _status;
        }
      });
    }
  }

  Future<void> _link(bool linked) async {
    final original = widget.store.linkedForChannel(widget.target);
    if (_pending || original == null) return;
    if (!await _confirm(
            linked ? 'Link channels?' : 'Unlink channels?',
            'Stereo linking can change how the paired channels share processing and levels.',
            linked ? 'Link' : 'Unlink') ||
        !mounted) {
      return;
    }
    if (widget.store.linkedForChannel(widget.target) != original) {
      _message('Link state changed. Review and try again.', error: true);
      return;
    }
    await _run(() async {
      final error = await widget.store.linkChannel(widget.target, linked);
      _message(error ?? 'Link change sent to mixer', error: error != null);
    });
  }

  void _clipboardInfo() {
    final target = widget.processing!;
    final copyError = _clipboardStore.channelProcessingError(target);
    final pasteError =
        _clipboardStore.channelProcessingError(target, paste: true);
    _info(
        'Processing clipboard',
        '$channelProcessingScope\n\n$channelProcessingExclusions'
            '${copyError == null ? '' : '\n\nCopy: $copyError'}'
            '${pasteError == null || pasteError == copyError ? '' : '\n\nPaste: $pasteError'}');
  }

  Future<void> _copyProcessing() async {
    if (_pending) return;
    _clipboardActive = true;
    try {
      await _run(() async {
        final error =
            await _clipboardStore.copyChannelProcessing(widget.processing!);
        _message(error ?? 'Channel DSP copied.', error: error != null);
      });
    } finally {
      if (mounted) setState(() => _clipboardActive = false);
    }
  }

  Future<void> _pasteProcessing() async {
    if (_pending || _clipboardActive) return;
    setState(() {
      _clipboardActive = true;
      _pending = true;
      _status = null;
    });
    try {
      final proposal = await _clipboardStore
          .prepareChannelProcessingPaste(widget.processing!);
      if (!mounted) return;
      setState(() => _pending = false);
      final source = proposal.source;
      final destination = proposal.destination;
      final settings = proposal.settings;
      if (!await _confirm(
              'Paste processing?',
              'Copy from $source to $destination: $settings advertised settings.\n\n'
                  '$channelProcessingScope\n\n$channelProcessingExclusions\n\n'
                  'Settings are applied serially with verified readback. Cancelling or a failed guard can leave a partial result; changes are not rolled back.',
              'Paste processing') ||
          !mounted) {
        return;
      }
      await _run(() async {
        final result = await _clipboardStore.pasteChannelProcessing(proposal);
        _result(result);
      });
    } catch (error) {
      _message('$error', error: true);
    } finally {
      if (mounted) {
        setState(() {
          _pending = false;
          _clipboardActive = false;
        });
      }
    }
  }

  void _result(ProcessorPresetResult result) {
    final confirmed = result.confirmed;
    final total = result.total;
    _message(result.error ?? '$confirmed of $total settings confirmed.',
        error: result.error != null);
  }

  Future<void> _openLibrary(ProcessorPresetTarget target) async {
    _presetTarget = target;
    _library = const [];
    _libraryLoaded = false;
    _go(_ChannelPage.library);
    if (_presetStore.presetTargetError(target) != null) return;
    await _run(_reloadLibrary);
  }

  Future<void> _reloadLibrary() async {
    final presets = await _presetStore.loadProcessorPresets(_presetTarget!);
    if (mounted) {
      setState(() {
        _library = presets;
        _libraryLoaded = true;
      });
    }
  }

  void _editPresetName({ProcessorPreset? preset}) {
    _renamingPreset = preset != null;
    _preset = preset;
    _presetName.text = preset?.name ?? '';
    _go(_ChannelPage.presetName);
  }

  Future<void> _savePreset() async {
    final invalid = validateProcessorPresetName(_presetName.text);
    if (invalid != null) {
      _message(invalid, error: true);
      return;
    }
    await _run(() async {
      final target = _presetTarget!;
      final error = _renamingPreset
          ? await _presetStore.renameProcessorPreset(
              target, _preset!.id, _presetName.text)
          : await _presetStore.saveProcessorPreset(target, _presetName.text);
      if (!mounted) return;
      if (error != null) {
        _message(error, error: true);
      } else {
        FocusScope.of(context).unfocus();
        setState(() {
          _transition(backward: true);
          _history.removeLast();
          if (_page == _ChannelPage.preset) _history.removeLast();
        });
        await _reloadLibrary();
      }
    });
  }

  Future<void> _recallPreset() async {
    final preset = _preset!;
    final name = preset.name;
    final count = preset.values.length;
    final target = _presetTarget!;
    final destination = target.name;
    if (!await _confirm(
            'Recall preset?',
            'Recall “$name” on $destination?\n\n$count saved settings. Bypass stays unchanged.',
            'Recall') ||
        !mounted) {
      return;
    }
    await _run(() async =>
        _result(await _presetStore.recallProcessorPreset(target, preset.id)));
  }

  Future<void> _deletePreset() async {
    final preset = _preset!;
    final name = preset.name;
    if (!await _confirm(
            'Delete preset?', 'Delete “$name” from this device?', 'Delete') ||
        !mounted) {
      return;
    }
    await _run(() async {
      final error =
          await _presetStore.deleteProcessorPreset(_presetTarget!, preset.id);
      if (!mounted) return;
      if (error != null) {
        _message(error, error: true);
      } else {
        setState(() {
          _transition(backward: true);
          _history.removeLast();
        });
        await _reloadLibrary();
      }
    });
  }

  void _copyProcessor() {
    final error = _presetStore.copyInputProcessor(_presetTarget!);
    _message(error ?? 'Processor copied.', error: error != null);
  }

  Future<void> _pasteProcessor() async {
    final target = _presetTarget!;
    final destination = target.name;
    if (!await _confirm(
            'Paste processor?',
            'Paste copied settings on $destination?\n\nBypass stays unchanged.',
            'Paste') ||
        !mounted) {
      return;
    }
    await _run(
        () async => _result(await _presetStore.pasteInputProcessor(target)));
  }

  Widget _option(String label, IconData icon, VoidCallback? action,
      {String? reason, Widget? leading, bool navigates = true}) {
    final p = ConsolePalette.of(context);
    return Padding(
        padding: const EdgeInsets.only(bottom: AppDesign.spaceSm),
        child: Row(children: [
          Expanded(
              child: TextButton(
                  onPressed: _pending || reason != null ? null : action,
                  style: TextButton.styleFrom(
                      backgroundColor: p.navigationTile,
                      padding: AppDesign.controlPadding,
                      minimumSize: const Size(0, AppDesign.controlHeight)),
                  child: Row(children: [
                    leading ?? Icon(icon, size: AppDesign.iconSize),
                    const SizedBox(width: AppDesign.spaceMd),
                    Expanded(child: Text(label)),
                    if (reason == null && navigates)
                      const Icon(Icons.chevron_right, size: AppDesign.iconSize),
                  ]))),
          if (reason != null) ...[
            const SizedBox(width: AppDesign.spaceSm),
            IconButton(
                tooltip: 'Why is $label unavailable?',
                onPressed: _pending ? null : () => _info(label, reason),
                icon: const Icon(Icons.info_outline)),
          ],
        ]));
  }

  Widget _overview() {
    final p = ConsolePalette.of(context);
    final target = widget.target;
    final colorChoices = widget.store.colorsForChannel(target);
    final colorError = widget.store.colorEditError(target) ??
        (colorChoices.isEmpty
            ? 'This channel has no advertised color choices.'
            : null);
    final iconError = widget.store.iconEditError(target);
    final color = colorChoices
        .where((c) => c.value == widget.store.colorForChannel(target))
        .firstOrNull;
    final assignments = widget.store.assignmentsForChannel(target);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      TextField(
          controller: _text,
          autocorrect: false,
          enableSuggestions: false,
          enabled: !_pending && widget.store.nameEditError(target) == null,
          style: AppDesign.type(AppDesign.sectionTitle, color: p.text),
          decoration: InputDecoration(
              labelText: 'Channel name',
              errorText: _nameError ?? widget.store.nameEditError(target),
              errorMaxLines: 4,
              suffixIcon: widget.store.nameCanBeEmpty(target)
                  ? IconButton(
                      tooltip: 'Use default name',
                      onPressed: _pending ||
                              widget.store.nameEditError(target) != null ||
                              _text.text.isEmpty
                          ? null
                          : () => setState(() {
                                _text.clear();
                                _nameError = null;
                              }),
                      icon: const Icon(Icons.restart_alt))
                  : null),
          onChanged: (_) => setState(() => _nameError = null),
          onSubmitted: (_) {
            if (!_pending &&
                widget.store.nameEditError(target) == null &&
                _text.text != target.name) {
              _rename();
            }
          }),
      const SizedBox(height: AppDesign.spaceMd),
      _option(_colorSent ? 'Color · sent' : 'Color', Icons.palette_outlined,
          () => _appearance(false),
          reason: colorError,
          leading: color?.rgb == null
              ? null
              : Container(
                  width: AppDesign.iconSize,
                  height: AppDesign.iconSize,
                  decoration: BoxDecoration(
                      color: Color(0xff000000 | color!.rgb!),
                      borderRadius: BorderRadius.circular(AppDesign.spaceXs),
                      border: Border.all(color: p.line)))),
      _option('Quick Assign', Icons.playlist_add_check,
          () => _go(_ChannelPage.assign),
          reason: assignments.isEmpty
              ? 'This channel has no available assignments.'
              : null),
      _option(_iconSaved ? 'Icon · saved on device' : 'Icon',
          Icons.image_outlined, () => _appearance(true),
          reason: iconError),
      _option('More', Icons.more_horiz, () => _go(_ChannelPage.more)),
    ]);
  }

  List<ChannelColorChoice> get _appearanceChoices =>
      widget.store.colorsForChannel(widget.target);

  String? get _appearanceError => widget.store.colorEditError(widget.target);

  Widget _appearancePage() {
    final p = ConsolePalette.of(context);
    final choices = _appearanceChoices;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Wrap(
          spacing: AppDesign.spaceSm,
          runSpacing: AppDesign.spaceSm,
          children: [
            for (final c in choices)
              Semantics(
                  label: c.label,
                  button: true,
                  selected: _appearanceValue == c.value,
                  child: Tooltip(
                      message: c.label,
                      child: InkWell(
                          onTap: _pending || _appearanceError != null
                              ? null
                              : () =>
                                  setState(() => _appearanceValue = c.value),
                          borderRadius:
                              BorderRadius.circular(AppDesign.fieldRadius),
                          child: Container(
                              width: AppDesign.controlHeight,
                              height: AppDesign.controlHeight,
                              padding: const EdgeInsets.all(AppDesign.spaceXs),
                              decoration: BoxDecoration(
                                  color: c.rgb == null
                                      ? p.well
                                      : Color(0xff000000 | c.rgb!),
                                  borderRadius: BorderRadius.circular(
                                      AppDesign.fieldRadius),
                                  border: Border.all(
                                      color: _appearanceValue == c.value
                                          ? p.selectedLine
                                          : p.line,
                                      width:
                                          _appearanceValue == c.value ? 3 : 1)),
                              child: c.rgb == null
                                  ? Icon(Icons.palette_outlined,
                                      size: AppDesign.iconSize,
                                      color: p.mutedText)
                                  : null)))),
          ]),
      const SizedBox(height: AppDesign.spaceMd),
      Text(
          _appearanceError ??
              choices
                  .where((c) => c.value == _appearanceValue)
                  .firstOrNull
                  ?.label ??
              'Choose a color',
          style: AppDesign.type(AppDesign.body, color: p.mutedText)),
    ]);
  }

  void _selectIcon(ChannelIconSelection selection) => setState(() {
        _iconSelection = selection;
        _syncNative = false;
        _status = null;
      });

  Widget _iconTile(ChannelIconSelection selection, String label) {
    final p = ConsolePalette.of(context);
    final selected = selection == _iconSelection;
    return Semantics(
        label: label,
        button: true,
        selected: selected,
        child: Tooltip(
            message: label,
            child: InkWell(
                onTap: _pending ||
                        widget.store.iconEditError(widget.target) != null
                    ? null
                    : () => _selectIcon(selection),
                borderRadius: BorderRadius.circular(AppDesign.fieldRadius),
                child: Container(
                    width: AppDesign.controlHeight,
                    height: AppDesign.controlHeight,
                    padding: const EdgeInsets.all(AppDesign.spaceXs),
                    decoration: BoxDecoration(
                        color: p.well,
                        borderRadius:
                            BorderRadius.circular(AppDesign.fieldRadius),
                        border: Border.all(
                            color: selected ? p.selectedLine : p.line,
                            width: selected ? 3 : 1)),
                    child: ChannelIconArtwork(widget.store
                        .resolveChannelIcon(widget.target, selection))))));
  }

  Widget _iconPage() {
    final p = ConsolePalette.of(context);
    final error = widget.store.iconEditError(widget.target);
    final nativeError =
        widget.store.nativeIconEditError(widget.target, _iconSelection);
    final resolved =
        widget.store.resolveChannelIcon(widget.target, _iconSelection);
    final imported = widget.store.importedChannelIcons;
    final local = _iconSelection.kind != ChannelIconKind.followProvider;
    final label = switch (_iconSelection.kind) {
      ChannelIconKind.catalog =>
        openFaderIcon(_iconSelection.id)?.label ?? 'Unavailable icon',
      ChannelIconKind.imported => 'Imported image',
      _ => null,
    };
    final missing = _iconSelection.kind == ChannelIconKind.imported &&
            resolved.imagePath == null ||
        _iconSelection.kind == ChannelIconKind.catalog &&
            openFaderIcon(_iconSelection.id) == null;
    return Padding(
        padding: const EdgeInsets.only(right: AppDesign.spaceSm),
        child:
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
          ConsoleActionGroup(children: [
            ChoiceChip(
                label: const Text('Automatic'),
                tooltip: 'Use the mixer icon when available',
                selected: _iconSelection.kind == ChannelIconKind.followProvider,
                onSelected: _pending || error != null
                    ? null
                    : (_) => _selectIcon(
                        const ChannelIconSelection.followProvider())),
            ChoiceChip(
                label: const Text('None'),
                tooltip: 'Hide the channel icon',
                selected: _iconSelection.kind == ChannelIconKind.none,
                onSelected: _pending || error != null
                    ? null
                    : (_) => _selectIcon(const ChannelIconSelection.none())),
            ActionChip(
                avatar: const Icon(Icons.add_photo_alternate_outlined,
                    size: AppDesign.iconSize),
                label: const Text('Import'),
                tooltip: 'Import PNG, JPEG or WebP, up to 8 MB',
                onPressed: _pending || error != null ? null : _importIcon),
          ]),
          if (error != null || missing) ...[
            const SizedBox(height: AppDesign.spaceSm),
            Text(error ?? resolved.reason!,
                style: AppDesign.type(AppDesign.body, color: p.mute)),
          ] else if (local) ...[
            const SizedBox(height: AppDesign.spaceSm),
            Row(children: [
              if (label != null)
                Expanded(
                    child: Text(label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: AppDesign.type(AppDesign.body, color: p.text)))
              else
                const Spacer(),
              const SizedBox(width: AppDesign.spaceSm),
              Tooltip(
                  message: nativeError ??
                      'An exact mixer icon mapping is available.',
                  child: Text(
                      nativeError == null
                          ? 'Mixer sync available'
                          : 'OpenFader only',
                      style: AppDesign.type(AppDesign.caption,
                          color: p.mutedText))),
            ]),
          ],
          if (local && nativeError == null)
            CheckboxListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('Also send to mixer'),
                value: _syncNative,
                onChanged: _pending
                    ? null
                    : (value) => setState(() => _syncNative = value == true)),
          for (final category
              in openFaderIcons.map((c) => c.category).toSet()) ...[
            const SizedBox(height: AppDesign.spaceMd),
            Text(category,
                style: AppDesign.type(AppDesign.controlLabel, color: p.text)),
            const SizedBox(height: AppDesign.spaceSm),
            Wrap(
                spacing: AppDesign.spaceSm,
                runSpacing: AppDesign.spaceSm,
                children: [
                  for (final icon
                      in openFaderIcons.where((c) => c.category == category))
                    _iconTile(
                        ChannelIconSelection.catalog(icon.id), icon.label),
                ]),
          ],
          if (imported.isNotEmpty) ...[
            const SizedBox(height: AppDesign.spaceMd),
            Text('Imported',
                style: AppDesign.type(AppDesign.controlLabel, color: p.text)),
            const SizedBox(height: AppDesign.spaceSm),
            Wrap(
                spacing: AppDesign.spaceSm,
                runSpacing: AppDesign.spaceSm,
                children: [
                  for (var index = 0; index < imported.length; index++)
                    Row(mainAxisSize: MainAxisSize.min, children: [
                      _iconTile(imported[index], 'Imported image ${index + 1}'),
                      const SizedBox(width: AppDesign.spaceSm),
                      IconButton(
                          tooltip: 'Remove imported image ${index + 1}',
                          onPressed: _pending || error != null
                              ? null
                              : () => _removeImportedIcon(imported[index]),
                          icon: const Icon(Icons.delete_outline,
                              size: AppDesign.iconSize)),
                    ]),
                ]),
          ],
        ]));
  }

  Widget _assignPage() => LayoutBuilder(builder: (context, box) {
        final choices = widget.store.assignmentsForChannel(widget.target);
        final p = ConsolePalette.of(context);
        const gap = AppDesign.spaceSm;
        const minTile = AppDesign.touchTarget + AppDesign.spaceXl;
        final columns =
            ((box.maxWidth + gap) / (minTile + gap)).floor().clamp(1, 4);
        final extent = (box.maxWidth - gap * (columns - 1)) / columns;
        return Wrap(spacing: gap, runSpacing: gap, children: [
          for (final choice in choices)
            Builder(builder: (context) {
              final current =
                  widget.store.assignmentForChannel(widget.target, choice.id);
              final unavailable = widget.store
                      .assignmentEditError(widget.target, choice.id) ??
                  (current == null ? 'Assignment state is unavailable.' : null);
              final selected = current == true;
              final sending = _pending && _assignmentPendingId == choice.id;
              final failed = _assignmentErrorId == choice.id;
              final reason =
                  unavailable ?? (failed ? _assignmentFailure : null);
              final enabled =
                  !_pending && unavailable == null && current != null;
              void toggle() => _assign(choice.id, !current!, current);
              void explain() => _info(choice.label, reason!);
              final indicator = sending
                  ? SizedBox.square(
                      dimension: AppDesign.removeIconSize,
                      child: CircularProgressIndicator(
                          strokeWidth: 2, color: p.selectedLine))
                  : Icon(
                      failed
                          ? Icons.error_outline
                          : unavailable != null
                              ? Icons.lock_outline
                              : selected
                                  ? Icons.check_circle
                                  : Icons.circle_outlined,
                      size: AppDesign.removeIconSize,
                      color: failed
                          ? p.mute
                          : selected
                              ? p.selectedLine
                              : p.mutedText);
              return SizedBox.square(
                  dimension: extent,
                  child: Semantics(
                      label: choice.label,
                      button: true,
                      enabled: enabled,
                      selected: current,
                      toggled: current,
                      value: sending
                          ? 'Sending change'
                          : current == null
                              ? 'State unavailable'
                              : null,
                      onTap: enabled ? toggle : null,
                      customSemanticsActions: !_pending && reason != null
                          ? {
                              const CustomSemanticsAction(
                                  label: 'Why unavailable'): explain,
                            }
                          : null,
                      child: ExcludeSemantics(
                          child: AnimatedContainer(
                              duration: MediaQuery.disableAnimationsOf(context)
                                  ? Duration.zero
                                  : const Duration(milliseconds: 120),
                              curve: Curves.easeOutCubic,
                              decoration: BoxDecoration(
                                  color: selected
                                      ? p.selectionFill
                                      : unavailable != null
                                          ? p.well
                                          : p.navigationTile,
                                  borderRadius: AppDesign.radius,
                                  border: Border.all(
                                      width: 2,
                                      color: failed
                                          ? p.mute
                                          : selected
                                              ? p.selectedLine
                                              : p.line)),
                              child: Material(
                                  color: Colors.transparent,
                                  child: InkWell(
                                      borderRadius: AppDesign.radius,
                                      onTap: enabled
                                          ? toggle
                                          : !_pending && reason != null
                                              ? explain
                                              : null,
                                      onLongPress: !_pending && reason != null
                                          ? explain
                                          : null,
                                      child: Padding(
                                          padding: const EdgeInsets.all(
                                              AppDesign.spaceXs),
                                          child: Column(
                                              mainAxisAlignment:
                                                  MainAxisAlignment.center,
                                              children: [
                                                Text(choice.label,
                                                    maxLines: 3,
                                                    overflow:
                                                        TextOverflow.ellipsis,
                                                    textAlign: TextAlign.center,
                                                    style: AppDesign.type(
                                                        AppDesign.smallLabel,
                                                        color:
                                                            enabled || sending
                                                                ? p.text
                                                                : p.mutedText,
                                                        weight:
                                                            FontWeight.w600)),
                                                const SizedBox(
                                                    height: AppDesign.spaceXs),
                                                indicator,
                                              ]))))))));
            }),
          if (choices.isEmpty)
            const Text('Assignments are no longer available.'),
        ]);
      });

  Widget _morePage() {
    final target = widget.target;
    final partner = widget.store.linkPartnerForChannel(target);
    final label =
        partner == null ? 'Stereo link' : 'Link with channel ${partner + 1}';
    final linkError = widget.store.linkEditError(target);
    final processing = widget.processing;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      if (widget.presets.isNotEmpty)
        _option('Presets', Icons.library_music_outlined,
            () => _go(_ChannelPage.presets)),
      _option(label, Icons.link,
          () => _link(widget.store.linkedForChannel(target) != true),
          reason: linkError,
          leading: Icon(
              widget.store.linkedForChannel(target) == true
                  ? Icons.link
                  : Icons.link_off,
              size: AppDesign.iconSize)),
      if (processing != null) ...[
        const SizedBox(height: AppDesign.spaceSm),
        Row(children: [
          const Expanded(child: Text('Processing clipboard')),
          IconButton(
              tooltip: 'About processing clipboard',
              onPressed: _pending ? null : _clipboardInfo,
              icon: const Icon(Icons.info_outline)),
        ]),
        _option('Copy processing', Icons.copy_outlined, _copyProcessing,
            reason: _clipboardStore.channelProcessingError(processing),
            navigates: false),
        _option('Paste processing', Icons.content_paste, _pasteProcessing,
            reason: _clipboardStore.channelProcessingError(processing,
                paste: true)),
      ],
    ]);
  }

  Widget _libraryPage() {
    final target = _presetTarget!;
    final error = _presetStore.presetTargetError(target);
    final pasteError = error ?? _presetStore.processorPasteError(target);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      _option('Save current', Icons.add, () => _editPresetName(),
          reason: error),
      _option('Copy processor', Icons.copy_outlined, _copyProcessor,
          reason: error, navigates: false),
      _option('Paste processor', Icons.content_paste, _pasteProcessor,
          reason: pasteError),
      if (_libraryLoaded && _library.isEmpty)
        const Padding(
            padding: AppDesign.panelPadding,
            child: Text('No user presets saved.')),
      for (final preset in _library)
        _option(preset.name, Icons.tune, () {
          _preset = preset;
          _go(_ChannelPage.preset);
        }),
      if (error == null)
        TextButton(
            onPressed: _pending ? null : () => _run(_reloadLibrary),
            child: const Text('Reload')),
    ]);
  }

  Widget _body() => switch (_page) {
        _ChannelPage.overview => _overview(),
        _ChannelPage.color => _appearancePage(),
        _ChannelPage.icon => _iconPage(),
        _ChannelPage.assign => _assignPage(),
        _ChannelPage.more => _morePage(),
        _ChannelPage.presets => Column(children: [
            for (final target in widget.presets)
              _option(
                  target.kind.label, Icons.tune, () => _openLibrary(target)),
          ]),
        _ChannelPage.library => _libraryPage(),
        _ChannelPage.preset => Column(children: [
            _option('Recall', Icons.settings_backup_restore, _recallPreset,
                reason: _presetStore.presetTargetError(_presetTarget!)),
            _option('Rename', Icons.edit_outlined,
                () => _editPresetName(preset: _preset),
                reason: _presetStore.presetTargetError(_presetTarget!)),
            _option('Delete', Icons.delete_outline, _deletePreset,
                reason: _presetStore.presetTargetError(_presetTarget!)),
          ]),
        _ChannelPage.presetName => TextField(
            controller: _presetName,
            autocorrect: false,
            enableSuggestions: false,
            enabled: !_pending &&
                _presetStore.presetTargetError(_presetTarget!) == null,
            decoration: InputDecoration(
                labelText: 'Preset name',
                errorText: _presetStore.presetTargetError(_presetTarget!),
                errorMaxLines: 4),
            onSubmitted: (_) {
              if (!_pending) _savePreset();
            }),
        _ChannelPage.info => Text(_infoText),
        _ChannelPage.confirm => Text(_confirmText),
      };

  String get _title => switch (_page) {
        _ChannelPage.overview => _identityLabel(widget.target),
        _ChannelPage.color => 'Color',
        _ChannelPage.icon => 'Icon',
        _ChannelPage.assign => 'Quick Assign',
        _ChannelPage.more => 'More',
        _ChannelPage.presets => 'Presets',
        _ChannelPage.library => '${_presetTarget!.kind.label} presets',
        _ChannelPage.preset => _preset!.name,
        _ChannelPage.presetName =>
          _renamingPreset ? 'Rename preset' : 'Save preset',
        _ChannelPage.info => _infoTitle,
        _ChannelPage.confirm => _confirmTitle,
      };

  Widget? _footer() {
    switch (_page) {
      case _ChannelPage.overview:
        final canRename = !_pending &&
            widget.store.nameEditError(widget.target) == null &&
            _text.text != widget.target.name;
        return FilledButton(
            onPressed: canRename ? _rename : null,
            child: const Text('Rename channel'));
      case _ChannelPage.color:
        final current = widget.store.colorForChannel(widget.target);
        return FilledButton(
            onPressed: !_pending &&
                    _appearanceError == null &&
                    _appearanceValue != current &&
                    _appearanceChoices.any((c) => c.value == _appearanceValue)
                ? _applyAppearance
                : null,
            child: const Text('Apply'));
      case _ChannelPage.icon:
        return FilledButton(
            onPressed: !_pending &&
                    widget.store.iconEditError(widget.target) == null &&
                    (_iconSelection !=
                            widget.store.iconForChannel(widget.target) ||
                        (_syncNative &&
                            widget.store.nativeIconEditError(
                                    widget.target, _iconSelection) ==
                                null))
                ? _applyIcon
                : null,
            child: const Text('Apply'));
      case _ChannelPage.presetName:
        return FilledButton(
            onPressed: _pending ||
                    _presetStore.presetTargetError(_presetTarget!) != null
                ? null
                : _savePreset,
            child: Text(_renamingPreset ? 'Rename' : 'Save'));
      case _ChannelPage.confirm:
        return Row(children: [
          Expanded(
              child: OutlinedButton(
                  onPressed:
                      _pending ? null : () => _resolveConfirmation(false),
                  child: const Text('Cancel'))),
          const SizedBox(width: AppDesign.spaceSm),
          Expanded(
              child: FilledButton(
                  onPressed: _pending ? null : () => _resolveConfirmation(true),
                  child: Text(_confirmAction))),
        ]);
      default:
        return null;
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: widget.store,
      builder: (context, _) {
        final p = ConsolePalette.of(context);
        final pageKey = ValueKey(_pageVersion);
        final direction = _backward ? -1.0 : 1.0;
        final duration = MediaQuery.disableAnimationsOf(context)
            ? Duration.zero
            : const Duration(milliseconds: 180);
        return PopScope(
            canPop: !_pending && !_clipboardActive,
            child: SizedBox(
                height: 450,
                child: Padding(
                    padding: AppDesign.panelPadding,
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          SizedBox(
                              height: AppDesign.controlHeight,
                              child: Row(children: [
                                if (_history.length > 1) ...[
                                  SizedBox.square(
                                      dimension: AppDesign.touchTarget,
                                      child: IconButton(
                                          tooltip: 'Back',
                                          onPressed: _pending ? null : _back,
                                          icon: const Icon(Icons.arrow_back))),
                                  const SizedBox(width: AppDesign.spaceSm),
                                ],
                                Expanded(
                                    child: Text(_title,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: AppDesign.type(
                                            AppDesign.sectionTitle,
                                            color: p.text,
                                            weight: FontWeight.w600))),
                                const SizedBox(width: AppDesign.spaceSm),
                                SizedBox.square(
                                    dimension: AppDesign.touchTarget,
                                    child: IconButton(
                                        tooltip: 'Close channel settings',
                                        onPressed: _pending || _clipboardActive
                                            ? null
                                            : () => Navigator.pop(context),
                                        icon: const Icon(Icons.close))),
                              ])),
                          const SizedBox(height: AppDesign.spaceSm),
                          Expanded(
                              child: ClipRect(
                                  child: AnimatedSwitcher(
                            duration: duration,
                            reverseDuration: duration,
                            switchInCurve: Curves.easeOutCubic,
                            switchOutCurve: Curves.easeInCubic,
                            layoutBuilder: (current, previous) => Stack(
                                fit: StackFit.expand,
                                children: [
                                  ...previous,
                                  if (current != null) current
                                ]),
                            transitionBuilder: (child, animation) {
                              final incoming = child.key == pageKey;
                              final offset = Offset(
                                  (incoming ? direction : -direction) * .08, 0);
                              return IgnorePointer(
                                  ignoring: !incoming,
                                  child: ExcludeSemantics(
                                      excluding: !incoming,
                                      child: FadeTransition(
                                          opacity: animation,
                                          child: SlideTransition(
                                              position: Tween<Offset>(
                                                      begin: offset,
                                                      end: Offset.zero)
                                                  .animate(animation),
                                              child: child))));
                            },
                            child: _ChannelPageContents(
                                key: pageKey,
                                body: _body(),
                                footer: _footer(),
                                status: _pending
                                    ? (_page == _ChannelPage.assign
                                        ? 'Sending assignment…'
                                        : 'Applying change…')
                                    : _status,
                                statusError: _statusError && !_pending,
                                onCancel: _clipboardActive && _pending
                                    ? _clipboardStore.cancelChannelProcessing
                                    : null),
                          ))),
                        ]))));
      });
}

/// Each transitioning page keeps its own scroll position/controller until its
/// outgoing animation finishes. No controller is attached to two viewports.
class _ChannelPageContents extends StatefulWidget {
  final Widget body;
  final Widget? footer;
  final String? status;
  final bool statusError;
  final VoidCallback? onCancel;
  const _ChannelPageContents({
    super.key,
    required this.body,
    this.footer,
    this.status,
    required this.statusError,
    this.onCancel,
  });
  @override
  State<_ChannelPageContents> createState() => _ChannelPageContentsState();
}

class _ChannelPageContentsState extends State<_ChannelPageContents> {
  final _scroll = ScrollController();
  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Expanded(
          child: Scrollbar(
              thumbVisibility: true,
              controller: _scroll,
              child: SingleChildScrollView(
                  controller: _scroll,
                  padding: const EdgeInsets.only(
                      top: AppDesign.spaceSm, bottom: AppDesign.spaceXs),
                  child: widget.body))),
      if (widget.status != null) ...[
        const SizedBox(height: AppDesign.spaceSm),
        Text(widget.status!,
            style: AppDesign.type(AppDesign.body,
                color: widget.statusError ? p.mute : p.mutedText)),
      ],
      if (widget.onCancel != null)
        TextButton(
            onPressed: widget.onCancel, child: const Text('Cancel operation')),
      if (widget.footer != null) ...[
        const SizedBox(height: AppDesign.spaceSm),
        ConstrainedBox(
            constraints: const BoxConstraints(minHeight: AppDesign.touchTarget),
            child: widget.footer),
      ],
    ]);
  }
}
