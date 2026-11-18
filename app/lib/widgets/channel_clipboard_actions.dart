import '../ui/console.dart';
import 'package:material_ui/material_ui.dart';

import '../core/channel_clipboard.dart';

/// Footer actions operate exclusively on the popup's captured target.
class ChannelClipboardActions extends StatefulWidget {
  final ChannelClipboardStore store;
  final ChannelProcessingTarget target;
  final ValueChanged<bool> onBusy;
  const ChannelClipboardActions({
    super.key,
    required this.store,
    required this.target,
    required this.onBusy,
  });
  @override
  State<ChannelClipboardActions> createState() =>
      _ChannelClipboardActionsState();
}

class _ChannelClipboardActionsState extends State<ChannelClipboardActions> {
  bool _busy = false, _applying = false;
  String? _status;

  void _setBusy(bool busy) {
    setState(() => _busy = busy);
    widget.onBusy(busy);
  }

  Future<void> _copy() async {
    _setBusy(true);
    final error = await widget.store.copyChannelProcessing(widget.target);
    if (!mounted) return;
    setState(() => _status = error ?? 'Channel DSP copied.');
    _setBusy(false);
  }

  Future<void> _paste() async {
    _setBusy(true);
    try {
      final proposal = await widget.store.prepareChannelProcessingPaste(
        widget.target,
      );
      if (!mounted) return;
      final confirmed = await showConsoleConfirmation(
        context,
        title: 'Paste Processing to ${proposal.destination}?',
        message:
            'Copy from ${proposal.source}: ${proposal.settings} advertised settings.\n\n'
            '$channelProcessingScope\n\n$channelProcessingExclusions\n\n'
            'Settings are applied serially with verified readback. Cancelling or a failed guard can leave a partial result; changes are not rolled back.',
        confirmLabel: 'Paste Processing',
      );
      if (!mounted || !confirmed) return;
      setState(() => _applying = true);
      final result = await widget.store.pasteChannelProcessing(proposal);
      if (!mounted) return;
      setState(
        () => _status = result.error ??
            '${result.confirmed} of ${result.total} settings verified.',
      );
    } catch (error) {
      if (mounted) setState(() => _status = '$error');
    } finally {
      if (mounted) {
        setState(() => _applying = false);
        _setBusy(false);
      }
    }
  }

  @override
  void dispose() {
    if (_busy) widget.store.cancelChannelProcessing();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
        animation: widget.store,
        builder: (context, _) {
          final copyError = widget.store.channelProcessingError(widget.target);
          final pasteError = widget.store.channelProcessingError(
            widget.target,
            paste: true,
          );
          final message = _status ?? copyError ?? pasteError;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Divider(),
              const Text('Channel clipboard · DSP only'),
              const SizedBox(height: 8),
              const Text(channelProcessingScope),
              const SizedBox(height: AppDesign.spaceSm),
              ConsoleActionGroup(
                spacing: 8,
                children: [
                  ConsoleButton(
                    label: 'Copy',
                    onPressed: _busy || copyError != null ? null : _copy,
                  ),
                  ConsoleButton(
                    label: 'Paste Processing',
                    onPressed: _busy || pasteError != null ? null : _paste,
                  ),
                  if (_busy)
                    ConsoleButton(
                      label: 'Cancel operation',
                      onPressed: widget.store.cancelChannelProcessing,
                    ),
                ],
              ),
              if (_busy)
                Text(
                  _applying
                      ? 'Applying and verifying settings…'
                      : 'Capturing verified channel DSP…',
                ),
              if (!_busy && message != null) Text(message),
              const SizedBox(height: 8),
              const Text(channelProcessingExclusions),
            ],
          );
        },
      );
}
