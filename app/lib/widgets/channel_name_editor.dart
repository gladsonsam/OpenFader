import 'package:flutter/material.dart';
import '../core/channel_name.dart';
import '../design.dart';
import '../palette.dart';
import 'channel_symbol.dart';

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
  bool _iconSent = false;
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
                                  builder: (_) => _AppearanceEditor(
                                      field: 'color',
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
                if (widget.store.iconsForChannel(widget.target).isNotEmpty)
                  Align(
                    alignment: Alignment.centerLeft,
                    child: TextButton(
                      onPressed: _pending ||
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
                      child: Text(_iconSent
                          ? 'Icon sent to mixer · Edit icon'
                          : 'Channel icon'),
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
