import '../ui/console.dart';
import 'package:material_ui/material_ui.dart';

import '../core/processor_presets.dart';

/// The target is captured by the trigger, before any modal or I/O. Every
/// callback uses this identity even if selection changes behind the dialog.
Future<void> showProcessorPresetLibrary(
        BuildContext context,
        ProcessorPresetStore store,
        ProcessorPresetKind kind,
        ProcessorPresetTarget? target) =>
    showConsoleDialog<void>(
        context,
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
        final changed = await showConsoleDialog<bool>(
            context,
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
        final target = widget.target;
        final unavailable = target == null
            ? '${widget.kind.label} presets are unavailable for this channel.'
            : widget.store.presetTargetError(target);
        final enabled = !_busy && unavailable == null;
        return PopScope(
            canPop: !_busy,
            child: ConsoleDialog(
                scrollable: false,
                title: '${widget.kind.label} presets',
                content: SizedBox(
                    width: 520,
                    height: (MediaQuery.sizeOf(context).height * .52)
                        .clamp(160.0, 520.0),
                    child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const ConsoleText('On this device',
                              tone: ConsoleTextTone.muted),
                          const SizedBox(height: 8),
                          if (target != null)
                            ConsoleText(
                                '${target.name} · ${target.driver.toUpperCase()}',
                                role: ConsoleTextRole.controlLabel),
                          const SizedBox(height: 8),
                          ConsoleButton(
                              onPressed: enabled ? () => _name() : null,
                              icon: Icons.add,
                              label: 'Save current'),
                          if (widget.store
                              case final InputProcessorPresetStore
                                  clipboard) ...[
                            const SizedBox(height: AppDesign.spaceSm),
                            ConsoleActionGroup(spacing: 8, children: [
                              ConsoleButton(
                                  onPressed: enabled ? _copy : null,
                                  icon: Icons.copy,
                                  label: 'Copy processor'),
                              Tooltip(
                                  message: target == null
                                      ? 'Processor unavailable.'
                                      : clipboard.processorPasteError(target) ??
                                          'Paste copied processor',
                                  child: ConsoleButton(
                                      onPressed: enabled &&
                                              clipboard.processorPasteError(
                                                      target!) ==
                                                  null
                                          ? _paste
                                          : null,
                                      icon: Icons.paste,
                                      label: 'Paste processor')),
                            ]),
                          ],
                          const SizedBox(height: 8),
                          Expanded(
                              child: _presets.isEmpty
                                  ? Center(
                                      child: ConsoleText(
                                          _busy
                                              ? 'Loading presets…'
                                              : 'No user presets saved.',
                                          tone: ConsoleTextTone.muted))
                                  : ListView.separated(
                                      itemCount: _presets.length,
                                      separatorBuilder: (context, index) =>
                                          Divider(
                                              color: ConsolePalette.of(context)
                                                  .line),
                                      itemBuilder: (context, index) {
                                        final preset = _presets[index];
                                        return Column(
                                            crossAxisAlignment:
                                                CrossAxisAlignment.start,
                                            children: [
                                              ConsoleText(preset.name,
                                                  role: ConsoleTextRole
                                                      .sectionTitle),
                                              ConsoleText(
                                                  'User preset · ${preset.values.length} settings',
                                                  role: ConsoleTextRole
                                                      .smallLabel,
                                                  tone: ConsoleTextTone.muted),
                                              const SizedBox(
                                                  height: AppDesign.spaceSm),
                                              ConsoleActionGroup(children: [
                                                ConsoleButton(
                                                    label: 'Recall',
                                                    onPressed: enabled
                                                        ? () => _recall(preset)
                                                        : null),
                                                ConsoleButton(
                                                    label: 'Rename',
                                                    onPressed: enabled
                                                        ? () => _name(
                                                            preset: preset)
                                                        : null),
                                                ConsoleButton(
                                                    label: 'Delete',
                                                    onPressed: enabled
                                                        ? () => _delete(preset)
                                                        : null),
                                              ]),
                                            ]);
                                      })),
                          if (_busy)
                            Padding(
                                padding: const EdgeInsets.only(top: 8),
                                child: const ConsoleText('Please wait…',
                                    tone: ConsoleTextTone.muted)),
                          if (_result != null)
                            Padding(
                                padding: const EdgeInsets.only(top: 8),
                                child: ConsoleText(_result!)),
                          if (_error != null || unavailable != null)
                            Flexible(
                                child: SingleChildScrollView(
                                    padding: const EdgeInsets.only(top: 8),
                                    child: ConsoleText(_error ?? unavailable!,
                                        tone: ConsoleTextTone.danger))),
                        ])),
                actions: [
                  if (_error != null && !_busy && unavailable == null)
                    ConsoleButton(
                        label: 'Reload',
                        size: ConsoleButtonSize.regular,
                        onPressed: () {
                          setState(() => _error = null);
                          _load();
                        }),
                  ConsoleButton(
                      label: 'Close',
                      size: ConsoleButtonSize.regular,
                      onPressed: _busy ? null : () => Navigator.pop(context)),
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
            child: ConsoleDialog(
                title: widget.title,
                content: SizedBox(
                    width: 360,
                    child: TextField(
                        controller: _text,
                        autocorrect: false,
                        enableSuggestions: false,
                        autofocus: true,
                        enabled: !_busy && unavailable == null,
                        style: consoleTextStyle(
                            context, ConsoleTextRole.sectionTitle),
                        decoration: InputDecoration(
                            labelText: 'Preset name',
                            labelStyle: consoleTextStyle(context, ConsoleTextRole.body,
                                tone: ConsoleTextTone.muted),
                            filled: true,
                            fillColor: p.well,
                            errorText: _error ?? unavailable,
                            errorMaxLines: 6,
                            errorStyle: consoleTextStyle(
                                context, ConsoleTextRole.smallLabel,
                                tone: ConsoleTextTone.danger),
                            enabledBorder: OutlineInputBorder(
                                borderSide: BorderSide(color: p.line)),
                            focusedBorder: OutlineInputBorder(
                                borderSide: BorderSide(color: p.cyan))),
                        onChanged: (_) => setState(() => _error = null),
                        onSubmitted: (_) {
                          if (unavailable == null) _apply();
                        })),
                actions: [
                  ConsoleButton(
                      label: 'Cancel',
                      size: ConsoleButtonSize.regular,
                      onPressed: _busy ? null : () => Navigator.pop(context)),
                  ConsoleButton(
                      label: _busy ? 'Please wait…' : widget.action,
                      variant: ConsoleButtonVariant.primary,
                      size: ConsoleButtonSize.regular,
                      onPressed: _busy || unavailable != null ? null : _apply),
                ]));
      });
}
