import 'package:material_ui/material_ui.dart';

import '../core/access_limits.dart';
import '../ui/console.dart';

enum AccessPasscodeAction { set, unlock, change, remove }

/// Dialog captures the policy epoch so an older editor cannot change a new lock.
Future<void> editAccessPasscode(
  BuildContext context,
  AccessLimitsStore store,
  AccessPasscodeAction action,
) async {
  final epoch = store.controlAccessEpoch;
  final values = await showDialog<_Passcodes>(
    animationStyle: AnimationStyle.noAnimation,
    context: context,
    builder: (_) => _AccessPasscodeDialog(action: action),
  );
  if (values == null) return;
  if (store.controlAccessEpoch != epoch) {
    throw StateError('Access policy changed. Reopen the passcode editor.');
  }
  switch (action) {
    case AccessPasscodeAction.set:
      await store.setAccessPasscode(values.next, values.confirmation);
    case AccessPasscodeAction.unlock:
      await store.unlockAccess(values.current);
    case AccessPasscodeAction.change:
      await store.changeAccessPasscode(
          values.current, values.next, values.confirmation);
    case AccessPasscodeAction.remove:
      await store.removeAccessPasscode(values.current);
  }
}

typedef _Passcodes = ({String current, String next, String confirmation});

class _AccessPasscodeDialog extends StatefulWidget {
  const _AccessPasscodeDialog({required this.action});
  final AccessPasscodeAction action;
  @override
  State<_AccessPasscodeDialog> createState() => _AccessPasscodeDialogState();
}

class _AccessPasscodeDialogState extends State<_AccessPasscodeDialog> {
  final current = TextEditingController();
  final next = TextEditingController();
  final confirmation = TextEditingController();

  @override
  void dispose() {
    current.dispose();
    next.dispose();
    confirmation.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final action = widget.action;
    final needsCurrent = action != AccessPasscodeAction.set;
    final needsNext = action == AccessPasscodeAction.set ||
        action == AccessPasscodeAction.change;
    final title = switch (action) {
      AccessPasscodeAction.set => 'Set passcode and lock',
      AccessPasscodeAction.unlock => 'Unlock access limits',
      AccessPasscodeAction.change => 'Change passcode',
      AccessPasscodeAction.remove => 'Remove passcode',
    };
    final palette = ConsolePalette.of(context);
    return ConsoleAlertDialog(
      backgroundColor: palette.background,
      title: Text(title),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Use exactly four characters. This protects local access settings only.',
            ),
            if (needsNext) ...[
              const SizedBox(height: 12),
              const Text(
                  'If you forget it, resetting requires reinstalling the app and may delete local data.'),
            ],
            if (needsCurrent)
              TextField(
                controller: current,
                obscureText: true,
                autocorrect: false,
                enableSuggestions: false,
                decoration: const InputDecoration(
                  labelText: 'Current passcode',
                ),
              ),
            if (needsNext) ...[
              TextField(
                controller: next,
                obscureText: true,
                autocorrect: false,
                enableSuggestions: false,
                decoration: const InputDecoration(labelText: 'New passcode'),
              ),
              TextField(
                controller: confirmation,
                obscureText: true,
                autocorrect: false,
                enableSuggestions: false,
                decoration: const InputDecoration(
                  labelText: 'Confirm passcode',
                ),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, (
            current: current.text,
            next: next.text,
            confirmation: confirmation.text
          )),
          child: Text(title),
        ),
      ],
    );
  }
}
