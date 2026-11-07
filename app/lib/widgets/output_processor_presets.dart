import '../ui/console.dart';
import 'package:flutter/material.dart';

import '../core/output_presets.dart';
import '../core/output_processing.dart';
import '../core/processor_presets.dart';

/// Capture before opening the modal: selection changes cannot retarget it.
class OutputProcessorPresetActions extends StatelessWidget {
  final OutputProcessingStore store;
  final String surface;
  const OutputProcessorPresetActions(
      {super.key, required this.store, required this.surface});

  @override
  Widget build(BuildContext context) => ConsoleActionGroup(children: [
        TextButton.icon(
            icon: const Icon(Icons.folder_open),
            label: const Text('Presets / Paste'),
            onPressed: () {
              final target = store.captureOutputPresetTarget(surface);
              if (target == null) {
                ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
                    content: Text(
                        'Output presets are unavailable or settings are changing.')));
                return;
              }
              showDialog<void>(
                  animationStyle: AnimationStyle.noAnimation,
                  context: context,
                  barrierDismissible: false,
                  builder: (_) =>
                      _OutputPresetLibrary(store: store, target: target));
            }),
        TextButton.icon(
            icon: const Icon(Icons.copy),
            label: const Text('Copy processor'),
            onPressed: () {
              final target = store.captureOutputPresetTarget(surface);
              final error = target == null
                  ? 'Output presets are unavailable or settings are changing.'
                  : store.copyOutputProcessor(target);
              ScaffoldMessenger.of(context).showSnackBar(SnackBar(
                  content: Text(error ??
                      'Processor copied. Bypass states and global temperature stay unchanged.')));
            }),
      ]);
}

class _OutputPresetLibrary extends StatefulWidget {
  final OutputProcessingStore store;
  final OutputPresetTarget target;
  const _OutputPresetLibrary({required this.store, required this.target});
  @override
  State<_OutputPresetLibrary> createState() => _OutputPresetLibraryState();
}

class _OutputPresetLibraryState extends State<_OutputPresetLibrary> {
  final _name = TextEditingController();
  List<ProcessorPreset> _presets = const [];
  bool _busy = false;
  bool _recalling = false;
  bool _recallSucceeded = false;
  OutputPresetTarget? _completedTarget;
  String? _message;

  @override
  void initState() {
    super.initState();
    _run(_load);
  }

  @override
  void dispose() {
    if (_recalling) widget.store.cancelOutputPresetRecall();
    _name.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final presets = await widget.store.loadOutputPresets(widget.target);
    if (mounted) setState(() => _presets = presets);
  }

  Future<void> _run(Future<void> Function() work) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      await work();
    } catch (error) {
      if (mounted) setState(() => _message = '$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _save() => _run(() async {
        final error =
            await widget.store.saveOutputPreset(widget.target, _name.text);
        if (!mounted) return;
        setState(() => _message = error ?? 'Preset saved on this device.');
        if (error == null) await _load();
      });

  Future<void> _recall(String? id) => _run(() async {
        setState(() => _recalling = true);
        try {
          final result =
              await widget.store.recallOutputPreset(widget.target, id);
          if (mounted) {
            // Retain a post-success snapshot only for status checks. Actions
            // still require reopening; later peer/connection changes must
            // continue to display their real error rather than a success hint.
            final completed = result.error == null
                ? widget.store.captureOutputPresetTarget(widget.target.surface)
                : null;
            setState(() {
              _recallSucceeded = result.error == null;
              if (completed != null &&
                  completed.target == widget.target.target &&
                  completed.session == widget.target.session &&
                  completed.driver == widget.target.driver) {
                _completedTarget = completed;
              }
              _message = result.error ??
                  '${result.confirmed} of ${result.total} settings verified by mixer readback.';
            });
          }
        } finally {
          if (mounted) setState(() => _recalling = false);
        }
      });

  Future<void> _rename(ProcessorPreset preset) => _run(() async {
        final name = await showConsoleNameDialog(context,
            title: 'Rename preset',
            initialValue: preset.name,
            maxLength: kProcessorPresetMaxNameLength,
            confirmLabel: 'Rename');
        if (!mounted || name == null) return;
        final error = await widget.store
            .renameOutputPreset(widget.target, preset.id, name);
        if (error != null) throw StateError(error);
        await _load();
      });

  Future<void> _delete(ProcessorPreset preset) => _run(() async {
        final confirmed = await showConsoleConfirmation(context,
            title: 'Delete preset?',
            message: preset.name,
            confirmLabel: 'Delete',
            danger: true);
        if (confirmed != true) return;
        final error =
            await widget.store.deleteOutputPreset(widget.target, preset.id);
        if (error != null) throw StateError(error);
        await _load();
      });

  @override
  Widget build(BuildContext context) => PopScope(
      canPop: !_busy,
      child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final error = _recalling
                ? null
                : widget.store
                    .outputPresetTargetError(_completedTarget ?? widget.target);
            final enabled = !_busy && !_recallSucceeded && error == null;
            return ConsoleAlertDialog(
                backgroundColor: ConsolePalette.of(context).panel,
                title: Text(
                    '${widget.target.kind!.label} · ${widget.target.name}'),
                content: SizedBox(
                    width: 520,
                    child: SingleChildScrollView(
                        child: Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.stretch,
                            children: [
                          const Text(
                              'Recall or paste applies this processor only. Bypass/on states and global temperature stay unchanged. Writes are ordered; cancellation stops before the next setting. Changes already sent are not rolled back.'),
                          const SizedBox(height: 12),
                          if (error != null) Text(error),
                          if (error == null && _recallSucceeded)
                            const Text(
                                'Close and reopen presets to use the updated processor state.'),
                          if (_message != null) Text(_message!),
                          if (_busy) const LinearProgressIndicator(),
                          TextField(
                              controller: _name,
                              enabled: enabled,
                              maxLength: kProcessorPresetMaxNameLength,
                              decoration: const InputDecoration(
                                  labelText: 'Preset name')),
                          ConsoleActionGroup(children: [
                            TextButton(
                                onPressed: enabled ? _save : null,
                                child: const Text('Save current')),
                            TextButton(
                                onPressed: enabled &&
                                        widget.store.hasOutputProcessorClipboard
                                    ? () => _recall(null)
                                    : null,
                                child: const Text('Paste processor')),
                          ]),
                          if (_presets.isEmpty && !_busy)
                            const Text(
                                'No local presets saved for this processor.'),
                          for (final preset in _presets)
                            Column(
                                crossAxisAlignment: CrossAxisAlignment.stretch,
                                children: [
                                  ListTile(
                                      contentPadding: EdgeInsets.zero,
                                      title: Text(preset.name),
                                      subtitle: Text(
                                          '${preset.values.length} settings')),
                                  ConsoleActionGroup(children: [
                                    TextButton(
                                        onPressed: enabled
                                            ? () => _recall(preset.id)
                                            : null,
                                        child: const Text('Recall')),
                                    PopupMenuButton<String>(
                                        popUpAnimationStyle:
                                            AnimationStyle.noAnimation,
                                        enabled: enabled,
                                        onSelected: (action) =>
                                            action == 'rename'
                                                ? _rename(preset)
                                                : _delete(preset),
                                        itemBuilder: (_) => const [
                                              PopupMenuItem(
                                                  value: 'rename',
                                                  child: Text('Rename')),
                                              PopupMenuItem(
                                                  value: 'delete',
                                                  child: Text('Delete'))
                                            ]),
                                  ]),
                                ]),
                        ]))),
                actions: [
                  if (_recalling)
                    TextButton(
                        onPressed: () {
                          widget.store.cancelOutputPresetRecall();
                          setState(() => _message =
                              'Cancellation requested. An in-flight setting may still complete.');
                        },
                        child: const Text('Cancel recall')),
                  TextButton(
                      onPressed: _busy ? null : () => Navigator.pop(context),
                      child: const Text('Close')),
                ]);
          }));
}
