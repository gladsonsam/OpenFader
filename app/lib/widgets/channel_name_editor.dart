import 'package:flutter/material.dart';

import '../core/channel_name.dart';
import '../core/processor_presets.dart';
import 'processor_preset_library.dart';
import '../design.dart';
import '../palette.dart';
import 'channel_symbol.dart';
import 'channel_quick_assign.dart';

Future<void> showChannelNameEditor(BuildContext context, ChannelNameStore store,
        ChannelNameTarget target) {
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
  return showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _NameEditor(store: store, target: target, presets: presets),
  );
}

class _NameEditor extends StatefulWidget {
  final ChannelNameStore store;
  final ChannelNameTarget target;
  final List<ProcessorPresetTarget> presets;
  const _NameEditor({required this.store, required this.target,
      required this.presets});
  @override
  State<_NameEditor> createState() => _NameEditorState();
}

class _NameEditorState extends State<_NameEditor> {
  late final _text = TextEditingController(text: widget.target.name);
  bool _pending = false;
  String? _error;
  bool _colorSent = false;
  bool _iconSent = false;
  String? _linkStatus;

  Future<void> _setLink(bool linked) async {
    final original = widget.store.linkedForChannel(widget.target);
    if (_pending || original == null) return;
    final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
              title: Text(linked ? 'Link channels?' : 'Unlink channels?'),
              content: const Text(
                  'Stereo linking can change how the paired channels share processing and levels.'),
              actions: [
                TextButton(
                    onPressed: () => Navigator.pop(context, false),
                    child: const Text('Cancel')),
                TextButton(
                    onPressed: () => Navigator.pop(context, true),
                    child: Text(linked ? 'Link' : 'Unlink')),
              ],
            ));
    if (!mounted || confirmed != true) return;
    if (widget.store.linkedForChannel(widget.target) != original) {
      setState(() => _linkStatus = 'Link state changed. Review and try again.');
      return;
    }
    setState(() => _pending = true);
    final error = await widget.store.linkChannel(widget.target, linked);
    if (!mounted) return;
    setState(() {
      _pending = false;
      _linkStatus = error ?? 'Link change sent to mixer';
    });
  }

  Future<void> _presets() => showDialog<void>(
      context: context,
      builder: (context) => SimpleDialog(
          title: const Text('Presets'),
          children: [
            for (final target in widget.presets)
              SimpleDialogOption(
                  onPressed: () {
                    Navigator.pop(context);
                    showProcessorPresetLibrary(this.context,
                        widget.store as InputProcessorPresetStore,
                        target.kind, target);
                  },
                  child: Text(target.kind.label)),
          ]));

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    if (_pending) return;
    setState(() {
      _pending = true;
      _error = null;
    });
    final error = await widget.store.renameChannel(widget.target, _text.text);
    if (!mounted) return;
    if (error == null) {
      Navigator.of(context).pop();
    } else {
      setState(() {
        _pending = false;
        _error = error;
      });
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: widget.store,
      builder: (context, _) {
        final p = ConsolePalette.of(context);
        final unavailable = widget.store.nameEditError(widget.target);
        final canRename = !_pending &&
            unavailable == null &&
            _text.text != widget.target.name;
        return PopScope(
          canPop: !_pending,
          child: AlertDialog(
            backgroundColor: p.panel,
            title: Text('Channel ${widget.target.channel + 1}',
                style: AppDesign.type(18, color: p.text7)),
            content: SizedBox(
              width: 320,
              child: SingleChildScrollView(
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                TextField(
                  controller: _text,
                  autocorrect: false,
                  enableSuggestions: false,
                  autofocus: false,
                  enabled: !_pending && unavailable == null,
                  style: AppDesign.type(18, color: p.text7),
                  decoration: InputDecoration(
                    labelText: 'Channel name',
                    labelStyle: AppDesign.type(14, color: p.mutedText),
                    filled: true,
                    fillColor: p.well,
                    errorText: _error ?? unavailable,
                    errorMaxLines: 4,
                    errorStyle: AppDesign.type(13, color: p.mute),
                    enabledBorder: OutlineInputBorder(
                        borderSide: BorderSide(color: p.line)),
                    focusedBorder: OutlineInputBorder(
                        borderSide: BorderSide(color: p.cyan)),
                  ),
                  onChanged: (_) => setState(() => _error = null),
                  onSubmitted: (_) {
                    if (unavailable == null &&
                        _text.text != widget.target.name) {
                      _save();
                    }
                  },
                ),
                if (_pending)
                  Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Text('Applying change…',
                          style: AppDesign.type(14, color: p.mutedText))),
                if (widget.store.nameCanBeEmpty(widget.target))
                  Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton(
                        onPressed: _pending ||
                                unavailable != null ||
                                _text.text.isEmpty
                            ? null
                            : () => setState(() {
                                  _text.clear();
                                  _error = null;
                                }),
                        child: const Text('Default name'),
                      )),
                Align(
                  alignment: Alignment.centerLeft,
                  child: TextButton(
                    onPressed: _pending ||
                            widget.store
                                .colorsForChannel(widget.target)
                                .isEmpty ||
                            widget.store.colorEditError(widget.target) != null
                        ? null
                        : () async {
                            final sent = await showDialog<bool>(
                                context: context,
                                barrierDismissible: false,
                                builder: (_) => _AppearanceEditor(
                                    field: 'color',
                                    store: widget.store,
                                    target: widget.target));
                            if (mounted && sent == true) {
                              setState(() => _colorSent = true);
                            }
                          },
                    child: Row(children: [
                      const Icon(Icons.palette_outlined, size: 20),
                      const SizedBox(width: 12),
                      Expanded(
                          child: Text(
                              _colorSent ? 'Color · sent to mixer' : 'Color')),
                      const Icon(Icons.chevron_right, size: 20),
                    ]),
                  ),
                ),
                Align(
                  alignment: Alignment.centerLeft,
                  child: TextButton(
                    onPressed: _pending ||
                            widget.store
                                .iconsForChannel(widget.target)
                                .isEmpty ||
                            widget.store.iconEditError(widget.target) != null
                        ? null
                        : () async {
                            final sent = await showDialog<bool>(
                                context: context,
                                barrierDismissible: false,
                                builder: (_) => _AppearanceEditor(
                                    field: 'icon',
                                    store: widget.store,
                                    target: widget.target));
                            if (mounted && sent == true) {
                              setState(() => _iconSent = true);
                            }
                          },
                    child: Row(children: [
                      const Icon(Icons.image_outlined, size: 20),
                      const SizedBox(width: 12),
                      Expanded(
                          child: Text(
                              _iconSent ? 'Icon · sent to mixer' : 'Icon')),
                      const Icon(Icons.chevron_right, size: 20),
                    ]),
                  ),
                ),
                if (widget.store.assignmentsForChannel(widget.target).isNotEmpty)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: TextButton(
                      onPressed: _pending ? null : () => showChannelQuickAssign(
                          context, widget.store, widget.target),
                      child: const Row(children: [
                        Icon(Icons.playlist_add_check, size: 20),
                        SizedBox(width: 12),
                        Expanded(child: Text('Quick Assign')),
                        Icon(Icons.chevron_right, size: 20),
                      ]),
                    ),
                  ),
                if (widget.presets.isNotEmpty)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: TextButton(
                      onPressed: _pending ? null : _presets,
                      child: const Row(children: [
                        Icon(Icons.library_music_outlined, size: 20),
                        SizedBox(width: 12),
                        Expanded(child: Text('Presets')),
                        Icon(Icons.chevron_right, size: 20),
                      ]),
                    ),
                  ),
                Divider(color: p.line),
                SwitchListTile.adaptive(
                  contentPadding: const EdgeInsets.symmetric(horizontal: 8),
                  title: Text(widget.store
                              .linkPartnerForChannel(widget.target) !=
                          null
                      ? 'Link with channel ${widget.store.linkPartnerForChannel(widget.target)! + 1}'
                      : 'Stereo link'),
                  subtitle: Text(widget.store.linkEditError(widget.target) ??
                      _linkStatus ??
                      (widget.store.linkedForChannel(widget.target) == true
                          ? 'Linked'
                          : 'Unlinked')),
                  value: widget.store.linkedForChannel(widget.target) == true,
                  onChanged: _pending ||
                          widget.store.linkEditError(widget.target) != null
                      ? null
                      : _setLink,
                ),
              ])),
            ),
            actions: [
              TextButton(
                  onPressed: _pending ? null : () => Navigator.pop(context),
                  child: Text('Close',
                      style: AppDesign.type(16, color: p.mutedText))),
              TextButton(
                  onPressed: canRename ? _save : null,
                  child: Text('Rename',
                      style: AppDesign.type(16,
                          color: canRename ? p.cyan : p.mutedText))),
            ],
          ),
        );
      });
}

class _AppearanceEditor extends StatefulWidget {
  final String field;
  final ChannelNameStore store;
  final ChannelNameTarget target;
  const _AppearanceEditor(
      {required this.field, required this.store, required this.target});
  @override
  State<_AppearanceEditor> createState() => _AppearanceEditorState();
}

class _AppearanceEditorState extends State<_AppearanceEditor> {
  bool get _icons => widget.field == 'icon';
  double? get _current => _icons
      ? widget.store.iconForChannel(widget.target)
      : widget.store.colorForChannel(widget.target);
  String? get _unavailable => _icons
      ? widget.store.iconEditError(widget.target)
      : widget.store.colorEditError(widget.target);
  List<({double value, String label, int? rgb, String? symbol})> get _choices =>
      _icons
          ? [
              for (final c in widget.store.iconsForChannel(widget.target))
                (value: c.value, label: c.label, rgb: null, symbol: c.symbol)
            ]
          : [
              for (final c in widget.store.colorsForChannel(widget.target))
                (value: c.value, label: c.label, rgb: c.rgb, symbol: null)
            ];
  late double? _selected = _current;
  bool _pending = false;
  String? _error;

  Future<void> _save() async {
    final selected = _selected;
    if (_pending || selected == null) return;
    setState(() {
      _pending = true;
      _error = null;
    });
    final error = await (_icons
        ? widget.store.iconChannel(widget.target, selected)
        : widget.store.colorChannel(widget.target, selected));
    if (!mounted) return;
    if (error == null) {
      Navigator.pop(context, true);
    } else {
      setState(() {
        _pending = false;
        _error = error;
      });
    }
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.store,
        builder: (context, _) {
          final p = ConsolePalette.of(context);
          final choices = _choices;
          final unavailable = _unavailable;
          final enabled = !_pending && unavailable == null;
          return PopScope(
            canPop: !_pending,
            child: AlertDialog(
              backgroundColor: p.panel,
              title: Text(
                  'Channel ${widget.target.channel + 1} ${widget.field}',
                  style: AppDesign.type(18, color: p.text7)),
              content: SizedBox(
                width: 320,
                child: ConstrainedBox(
                    constraints: BoxConstraints(
                        maxHeight: MediaQuery.sizeOf(context).height * .5),
                    child: SingleChildScrollView(
                        child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Wrap(spacing: 8, runSpacing: 8, children: [
                          for (final choice in choices)
                            Semantics(
                              label: choice.label,
                              button: true,
                              selected: _selected == choice.value,
                              child: Tooltip(
                                message: choice.label,
                                child: InkWell(
                                  onTap: enabled
                                      ? () => setState(() {
                                            _selected = choice.value;
                                            _error = null;
                                          })
                                      : null,
                                  child: Container(
                                    width: _icons ? 44 : 48,
                                    height: _icons ? 44 : 48,
                                    decoration: BoxDecoration(
                                        color: choice.rgb == null
                                            ? p.well
                                            : Color(0xff000000 | choice.rgb!),
                                        borderRadius: BorderRadius.circular(4),
                                        border: Border.all(
                                            color: _selected == choice.value
                                                ? p.cyan
                                                : p.line,
                                            width: _selected == choice.value
                                                ? 3
                                                : 1)),
                                    child: _icons
                                        ? choice.symbol == 'none'
                                            ? Center(
                                                child: Text('Clear',
                                                    style: AppDesign.type(11,
                                                        color: p.text7)))
                                            : Padding(
                                                padding:
                                                    const EdgeInsets.all(5),
                                                child: ChannelSymbol(
                                                    choice.symbol))
                                        : choice.rgb == null
                                            ? Center(
                                                child: Text(choice.label,
                                                    style: AppDesign.type(11,
                                                        color: p.text7)))
                                            : null,
                                  ),
                                ),
                              ),
                            ),
                        ]),
                        const SizedBox(height: 12),
                        Text(
                            _pending
                                ? 'Sending ${widget.field}…'
                                : _error ??
                                    unavailable ??
                                    choices
                                        .where((c) => c.value == _selected)
                                        .firstOrNull
                                        ?.label ??
                                    (_icons
                                        ? 'Choose an icon'
                                        : 'Choose a color'),
                            style: AppDesign.type(14,
                                color: _error != null || unavailable != null
                                    ? p.mute
                                    : p.mutedText)),
                      ],
                    ))),
              ),
              actions: [
                TextButton(
                    onPressed: _pending ? null : () => Navigator.pop(context),
                    child: const Text('Cancel')),
                TextButton(
                    onPressed: enabled &&
                            choices.any((c) => c.value == _selected) &&
                            _selected != _current
                        ? _save
                        : null,
                    child: const Text('Apply')),
              ],
            ),
          );
        },
      );
}
