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
