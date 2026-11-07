import '../ui/console.dart';
import 'package:flutter/material.dart';

import '../core/processor_presets.dart';

/// The target is captured by the trigger, before any modal or I/O. Every
/// callback uses this identity even if selection changes behind the dialog.
Future<void> showProcessorPresetLibrary(
        BuildContext context,
        ProcessorPresetStore store,
        ProcessorPresetKind kind,
        ProcessorPresetTarget? target) =>
    showDialog<void>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        barrierDismissible: false,
        builder: (_) =>
            _ProcessorPresetLibrary(store: store, kind: kind, target: target));

class _ProcessorPresetLibrary extends StatefulWidget {
  final ProcessorPresetStore store;
  final ProcessorPresetKind kind;
  final ProcessorPresetTarget? target;
  const _ProcessorPresetLibrary(
      {required this.store, required this.kind, required this.target});

  @override
  State<_ProcessorPresetLibrary> createState() =>
      _ProcessorPresetLibraryState();
}

class _ProcessorPresetLibraryState extends State<_ProcessorPresetLibrary> {
  List<ProcessorPreset> _presets = const [];
  bool _busy = false;
  String? _error;
  String? _result;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    final target = widget.target;
    if (target == null) return;
    setState(() => _busy = true);
    try {
      final presets = await widget.store.loadProcessorPresets(target);
      if (mounted) setState(() => _presets = presets);
    } catch (error) {
      if (mounted) setState(() => _error = 'Could not load presets: $error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _run(Future<void> Function() action) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _error = null;
      _result = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) setState(() => _error = '$error');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _reload() async {
    final presets = await widget.store.loadProcessorPresets(widget.target!);
    if (mounted) setState(() => _presets = presets);
  }

  Future<void> _name({ProcessorPreset? preset}) => _run(() async {
        final target = widget.target!;
        final changed = await showDialog<bool>(
            animationStyle: AnimationStyle.noAnimation,
            context: context,
            barrierDismissible: false,
            builder: (_) => _PresetNameDialog(
                store: widget.store,
                target: target,
                initialName: preset?.name ?? '',
                title: preset == null
                    ? 'Save ${widget.kind.label} preset'
                    : 'Rename preset',
                action: preset == null ? 'Save' : 'Rename',
                apply: (name) => preset == null
                    ? widget.store.saveProcessorPreset(target, name)
                    : widget.store
                        .renameProcessorPreset(target, preset.id, name)));
        if (changed == true) await _reload();
      });

  Future<bool> _confirm(String title, String message, String action) =>
      showConsoleConfirmation(context,
          title: title, message: message, confirmLabel: action);

  Future<void> _recall(ProcessorPreset preset) => _run(() async {
        final target = widget.target!;
        if (!await _confirm(
            'Recall ${widget.kind.label} preset?',
            'Recall “${preset.name}” on ${target.name} '
                '(channel ${target.channel + 1})?\n\n'
                '${preset.values.length} saved settings. Bypass stays unchanged.',
            'Recall')) {
          return;
        }
        final result =
            await widget.store.recallProcessorPreset(target, preset.id);
        if (!mounted) return;
        setState(() {
          _result =
              '${result.confirmed} of ${result.total} settings confirmed.';
          _error = result.error;
        });
      });

  Future<void> _copy() => _run(() async {
        final error = (widget.store as InputProcessorPresetStore)
            .copyInputProcessor(widget.target!);
        if (mounted) {
          setState(() {
            _error = error;
            if (error == null) {
              _result = '${widget.kind.label} processor copied.';
            }
          });
        }
      });

  Future<void> _paste() => _run(() async {
        final target = widget.target!;
        if (!await _confirm(
          'Paste ${widget.kind.label} processor?',
          'Paste copied settings on ${target.name} (channel ${target.channel + 1})?\n\nBypass stays unchanged.',
          'Paste',
        )) {
          return;
        }
        final result = await (widget.store as InputProcessorPresetStore)
            .pasteInputProcessor(target);
        if (mounted) {
          setState(() {
            _result =
                '${result.confirmed} of ${result.total} settings confirmed.';
            _error = result.error;
          });
        }
      });

  Future<void> _delete(ProcessorPreset preset) => _run(() async {
        if (!await _confirm('Delete preset?',
            'Delete “${preset.name}” from this device?', 'Delete')) {
          return;
        }
        final error =
            await widget.store.deleteProcessorPreset(widget.target!, preset.id);
        if (error != null) {
          if (mounted) setState(() => _error = error);
          return;
        }
        await _reload();
      });

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: widget.store,
      builder: (context, _) {
        final p = ConsolePalette.of(context);
        final target = widget.target;
        final unavailable = target == null
            ? '${widget.kind.label} presets are unavailable for this channel.'
            : widget.store.presetTargetError(target);
        final enabled = !_busy && unavailable == null;
        return PopScope(
            canPop: !_busy,
            child: ConsoleAlertDialog(
                backgroundColor: p.panel,
                title: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('${widget.kind.label} presets',
                          style: AppDesign.type(22, color: p.text1)),
                      const SizedBox(height: 4),
                      Text('On this device',
                          style: AppDesign.type(14, color: p.mutedText)),
                    ]),
                content: SizedBox(
                    width: 520,
                    height: (MediaQuery.sizeOf(context).height * .52)
                        .clamp(160.0, 520.0),
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          if (target != null)
                            Text(
                                '${target.name} · ${target.driver.toUpperCase()}',
                                style: AppDesign.type(16, color: p.text1)),
                          const SizedBox(height: 8),
                          FilledButton.icon(
                              style: FilledButton.styleFrom(
                                  foregroundColor: p.text1,
                                  backgroundColor: p.control),
                              onPressed: enabled ? () => _name() : null,
                              icon: const Icon(Icons.add),
                              label: const Text('Save current')),
                          if (widget.store
                              case final InputProcessorPresetStore
                                  clipboard) ...[
                            const SizedBox(height: AppDesign.spaceSm),
                            ConsoleActionGroup(spacing: 8, children: [
                              TextButton.icon(
                                  onPressed: enabled ? _copy : null,
                                  icon: const Icon(Icons.copy),
                                  label: const Text('Copy processor')),
                              Tooltip(
                                  message: target == null
                                      ? 'Processor unavailable.'
                                      : clipboard.processorPasteError(target) ??
                                          'Paste copied processor',
                                  child: TextButton.icon(
                                      onPressed: enabled &&
                                              clipboard.processorPasteError(
                                                      target!) ==
                                                  null
                                          ? _paste
                                          : null,
                                      icon: const Icon(Icons.paste),
                                      label: const Text('Paste processor'))),
                            ]),
                          ],
                          const SizedBox(height: 8),
                          Expanded(
                              child: _presets.isEmpty
                                  ? Center(
                                      child: Text(
                                          _busy
                                              ? 'Loading presets…'
                                              : 'No user presets saved.',
                                          style: AppDesign.type(15,
                                              color: p.mutedText)))
                                  : ListView.separated(
                                      itemCount: _presets.length,
                                      separatorBuilder: (context, index) =>
                                          Divider(color: p.line),
                                      itemBuilder: (context, index) {
                                        final preset = _presets[index];
                                        return Column(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              Text(preset.name,
                                                  style: AppDesign.type(18,
                                                      color: p.text1)),
                                              Text(
                                                  'User preset · ${preset.values.length} settings',
                                                  style: AppDesign.type(13,
                                                      color: p.mutedText)),
                                              const SizedBox(
                                                  height: AppDesign.spaceSm),
                                              ConsoleActionGroup(children: [
                                                TextButton(
                                                    onPressed: enabled
                                                        ? () => _recall(preset)
                                                        : null,
                                                    child: Text('Recall',
                                                        style: TextStyle(
                                                            color: p.cyan))),
                                                TextButton(
                                                    onPressed: enabled
                                                        ? () => _name(
                                                            preset: preset)
                                                        : null,
                                                    child:
                                                        const Text('Rename')),
                                                TextButton(
                                                    onPressed: enabled
                                                        ? () => _delete(preset)
                                                        : null,
                                                    child:
                                                        const Text('Delete')),
                                              ]),
                                            ]);
                                      })),
                          if (_busy)
                            Padding(
                                padding: const EdgeInsets.only(top: 8),
                                child: Text('Please wait…',
                                    style: AppDesign.type(14,
                                        color: p.mutedText))),
                          if (_result != null)
                            Padding(
                                padding: const EdgeInsets.only(top: 8),
                                child: Text(_result!,
                                    style: AppDesign.type(14, color: p.text1))),
                          if (_error != null || unavailable != null)
                            Flexible(
                                child: SingleChildScrollView(
                                    padding: const EdgeInsets.only(top: 8),
                                    child: Text(_error ?? unavailable!,
                                        style: AppDesign.type(14,
                                            color: p.mute)))),
                        ])),
                actions: [
                  if (_error != null && !_busy && unavailable == null)
                    TextButton(
                        onPressed: () {
                          setState(() => _error = null);
                          _load();
                        },
                        child: Text('Reload',
                            style: AppDesign.type(16, color: p.cyan))),
                  TextButton(
                      onPressed: _busy ? null : () => Navigator.pop(context),
                      child: Text('Close',
                          style: AppDesign.type(16, color: p.mutedText))),
                ]));
      });
}

class _PresetNameDialog extends StatefulWidget {
  final ProcessorPresetStore store;
  final ProcessorPresetTarget target;
  final String initialName;
  final String title;
  final String action;
  final Future<String?> Function(String name) apply;
  const _PresetNameDialog(
      {required this.store,
      required this.target,
      required this.initialName,
      required this.title,
      required this.action,
      required this.apply});

  @override
  State<_PresetNameDialog> createState() => _PresetNameDialogState();
}

class _PresetNameDialogState extends State<_PresetNameDialog> {
  late final _text = TextEditingController(text: widget.initialName);
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _apply() async {
    if (_busy) return;
    final invalid = validateProcessorPresetName(_text.text);
    if (invalid != null) {
      setState(() => _error = invalid);
      return;
    }
    setState(() {
      _busy = true;
      _error = null;
    });
    String? error;
    try {
      error = await widget.apply(_text.text);
    } catch (failure) {
      error = '$failure';
    }
    if (!mounted) return;
    if (error == null) {
      Navigator.pop(context, true);
    } else {
      setState(() {
        _busy = false;
        _error = error;
      });
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: widget.store,
      builder: (context, _) {
        final p = ConsolePalette.of(context);
        final unavailable = widget.store.presetTargetError(widget.target);
        return PopScope(
            canPop: !_busy,
            child: ConsoleAlertDialog(
                backgroundColor: p.panel,
                title: Text(widget.title,
                    style: AppDesign.type(20, color: p.text1)),
                content: SizedBox(
                    width: 360,
                    child: TextField(
                        controller: _text,
                        autocorrect: false,
                        enableSuggestions: false,
                        autofocus: true,
                        enabled: !_busy && unavailable == null,
                        style: AppDesign.type(18, color: p.text1),
                        decoration: InputDecoration(
                            labelText: 'Preset name',
                            labelStyle: AppDesign.type(14, color: p.mutedText),
                            filled: true,
                            fillColor: p.well,
                            errorText: _error ?? unavailable,
                            errorMaxLines: 6,
                            errorStyle: AppDesign.type(13, color: p.mute),
                            enabledBorder: OutlineInputBorder(
                                borderSide: BorderSide(color: p.line)),
                            focusedBorder: OutlineInputBorder(
                                borderSide: BorderSide(color: p.cyan))),
                        onChanged: (_) => setState(() => _error = null),
                        onSubmitted: (_) {
                          if (unavailable == null) _apply();
                        })),
                actions: [
                  TextButton(
                      onPressed: _busy ? null : () => Navigator.pop(context),
                      child: Text('Cancel',
                          style: AppDesign.type(16, color: p.mutedText))),
                  TextButton(
                      onPressed: _busy || unavailable != null ? null : _apply,
                      child: Text(_busy ? 'Please wait…' : widget.action,
                          style: AppDesign.type(16, color: p.cyan))),
                ]));
      });
}
