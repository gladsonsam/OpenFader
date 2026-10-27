import 'package:flutter/material.dart';
import '../core/channel_name.dart';
import '../design.dart';
import '../palette.dart';

Future<void> showChannelQuickAssign(BuildContext context,
        ChannelNameStore store, ChannelNameTarget target) =>
    showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _QuickAssign(store: store, target: target),
    );

class _QuickAssign extends StatefulWidget {
  final ChannelNameStore store;
  final ChannelNameTarget target;
  const _QuickAssign({required this.store, required this.target});

  @override
  State<_QuickAssign> createState() => _QuickAssignState();
}

class _QuickAssignState extends State<_QuickAssign> {
  bool _pending = false;
  String? _status;

  Future<void> _assign(String id, bool value, bool expected) async {
    if (_pending) return;
    setState(() {
      _pending = true;
      _status = null;
    });
    final result = await widget.store
        .assignChannel(widget.target, id, value, expected: expected);
    if (!mounted) return;
    setState(() {
      _pending = false;
      _status = result.error ??
          'Assignment matches your choice. Mixer confirmation is unavailable.';
    });
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: widget.store,
      builder: (context, _) {
        final p = ConsolePalette.of(context);
        final choices = widget.store.assignmentsForChannel(widget.target);
        return PopScope(
          canPop: !_pending,
          child: AlertDialog(
            backgroundColor: p.panel,
            title: Text('Channel ${widget.target.channel + 1} · Quick Assign',
                style: AppDesign.type(18, color: p.text7)),
            content: SizedBox(
              width: 320,
              child: ConstrainedBox(
                constraints: BoxConstraints(
                    maxHeight: MediaQuery.sizeOf(context).height * .6),
                child: SingleChildScrollView(
                  child: Column(mainAxisSize: MainAxisSize.min, children: [
                    for (final choice in choices)
                      Builder(builder: (context) {
                        final current = widget.store
                            .assignmentForChannel(widget.target, choice.id);
                        final error = widget.store
                            .assignmentEditError(widget.target, choice.id);
                        return SwitchListTile.adaptive(
                          contentPadding: EdgeInsets.zero,
                          title: Text(choice.label),
                          subtitle: error == null ? null : Text(error),
                          value: current == true,
                          onChanged: _pending ||
                                  current == null ||
                                  error != null
                              ? null
                              : (value) => _assign(choice.id, value, current),
                        );
                      }),
                    if (choices.isEmpty)
                      const Text(
                          'Assignments are no longer available. Reopen the channel popup.'),
                    const SizedBox(height: 12),
                    Text(
                        _pending
                            ? 'Sending assignment…'
                            : _status ??
                                'Change one membership at a time. Switches show the latest assignment state.',
                        style: AppDesign.type(14, color: p.mutedText)),
                  ]),
                ),
              ),
            ),
            actions: [
              TextButton(
                  onPressed: _pending ? null : () => Navigator.pop(context),
                  child: const Text('Close')),
            ],
          ),
        );
      });
}
