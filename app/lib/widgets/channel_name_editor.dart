import 'package:flutter/material.dart';
import '../core/channel_name.dart';
import '../design.dart';
import '../palette.dart';

Future<void> showChannelNameEditor(BuildContext context, ChannelNameStore store,
        ChannelNameTarget target) =>
    showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _NameEditor(store: store, target: target),
    );

class _NameEditor extends StatefulWidget {
  final ChannelNameStore store;
  final ChannelNameTarget target;
  const _NameEditor({required this.store, required this.target});
  @override
  State<_NameEditor> createState() => _NameEditorState();
}

class _NameEditorState extends State<_NameEditor> {
  late final _text = TextEditingController(text: widget.target.name);
  bool _pending = false;
  String? _error;
  bool _colorSent = false;
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
        return PopScope(
          canPop: !_pending,
          child: AlertDialog(
            backgroundColor: p.panel,
            title: Text('Rename channel ${widget.target.channel + 1}',
                style: AppDesign.type(18, color: p.text7)),
            content: SizedBox(
              width: 320,
              child: Column(mainAxisSize: MainAxisSize.min, children: [
                TextField(
                  controller: _text,
                  autocorrect: false,
                  enableSuggestions: false,
                  autofocus: true,
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
                    if (unavailable == null) _save();
                  },
                ),
                if (_pending)
                  Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: Text('Waiting for name confirmation…',
                          style: AppDesign.type(14, color: p.mutedText))),
                if (widget.store.colorsForChannel(widget.target).isNotEmpty)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: TextButton(
                      onPressed: _pending ||
                              widget.store.colorEditError(widget.target) != null
                          ? null
                          : () async {
                              final sent = await showDialog<bool>(
                                  context: context,
                                  barrierDismissible: false,
                                  builder: (_) => _ColorEditor(
                                      store: widget.store,
                                      target: widget.target));
                              if (mounted && sent == true) {
                                setState(() => _colorSent = true);
                              }
                            },
                      child: Text(_colorSent
                          ? 'Color sent to mixer · Edit color'
                          : 'Channel color'),
                    ),
                  ),
              ]),
            ),
            actions: [
              TextButton(
                  onPressed: _pending ? null : () => Navigator.pop(context),
                  child: Text('Cancel',
                      style: AppDesign.type(16, color: p.mutedText))),
              TextButton(
                  onPressed: _pending || unavailable != null ? null : _save,
                  child:
                      Text('Rename', style: AppDesign.type(16, color: p.cyan))),
            ],
          ),
        );
      });
}

class _ColorEditor extends StatefulWidget {
  final ChannelNameStore store;
  final ChannelNameTarget target;
  const _ColorEditor({required this.store, required this.target});
  @override
  State<_ColorEditor> createState() => _ColorEditorState();
}

class _ColorEditorState extends State<_ColorEditor> {
  late double? _selected = widget.store.colorForChannel(widget.target);
  bool _pending = false;
  String? _error;

  Future<void> _save() async {
    final selected = _selected;
    if (_pending || selected == null) return;
    setState(() {
      _pending = true;
      _error = null;
    });
    final error = await widget.store.colorChannel(widget.target, selected);
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
          final choices = widget.store.colorsForChannel(widget.target);
          final unavailable = widget.store.colorEditError(widget.target);
          final enabled = !_pending && unavailable == null;
          return PopScope(
            canPop: !_pending,
            child: AlertDialog(
              backgroundColor: p.panel,
              title: Text('Channel ${widget.target.channel + 1} color',
                  style: AppDesign.type(18, color: p.text7)),
              content: SizedBox(
                width: 320,
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
                                width: 48,
                                height: 48,
                                decoration: BoxDecoration(
                                    color: choice.rgb == null
                                        ? p.well
                                        : Color(0xff000000 | choice.rgb!),
                                    borderRadius: BorderRadius.circular(4),
                                    border: Border.all(
                                        color: _selected == choice.value
                                            ? p.cyan
                                            : p.line,
                                        width:
                                            _selected == choice.value ? 3 : 1)),
                                child: choice.rgb == null
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
                            ? 'Sending color…'
                            : _error ??
                                unavailable ??
                                choices
                                    .where((c) => c.value == _selected)
                                    .firstOrNull
                                    ?.label ??
                                'Choose a color',
                        style: AppDesign.type(14,
                            color: _error != null || unavailable != null
                                ? p.mute
                                : p.mutedText)),
                  ],
                )),
              ),
              actions: [
                TextButton(
                    onPressed: _pending ? null : () => Navigator.pop(context),
                    child: const Text('Cancel')),
                TextButton(
                    onPressed: enabled &&
                            choices.any((c) => c.value == _selected) &&
                            _selected !=
                                widget.store.colorForChannel(widget.target)
                        ? _save
                        : null,
                    child: const Text('Apply')),
              ],
            ),
          );
        },
      );
}
