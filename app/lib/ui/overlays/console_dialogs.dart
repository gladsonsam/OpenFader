import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';
import '../controls/console_button.dart';

/// Standard dialog action spacing, including when the action bar stacks on
/// narrow screens. Material's default overflow spacing is zero.
class ConsoleAlertDialog extends AlertDialog {
  const ConsoleAlertDialog({
    super.key,
    super.icon,
    super.iconPadding,
    super.iconColor,
    super.title,
    super.titlePadding,
    super.titleTextStyle,
    super.content,
    super.contentPadding,
    super.contentTextStyle,
    super.actions,
    super.actionsPadding,
    super.actionsAlignment,
    super.actionsOverflowAlignment,
    super.actionsOverflowDirection,
    super.actionsOverflowButtonSpacing = AppDesign.spaceSm,
    super.buttonPadding =
        const EdgeInsets.symmetric(horizontal: AppDesign.spaceXs),
    super.backgroundColor,
    super.elevation,
    super.shadowColor,
    super.surfaceTintColor,
    super.semanticLabel,
    super.insetPadding,
    super.clipBehavior,
    super.shape,
    super.alignment,
    super.constraints,
    super.scrollable = false,
  });
}

/// Collects a decision only. Callers retain captured-target/access validation
/// and perform the action after the dialog closes.
Future<bool> showConsoleConfirmation(
  BuildContext context, {
  required String title,
  required String message,
  required String confirmLabel,
  bool danger = false,
}) async =>
    await showDialog<bool>(
      context: context,
      animationStyle: AnimationStyle.noAnimation,
      builder: (context) => ConsoleAlertDialog(
        backgroundColor: ConsolePalette.of(context).panel,
        scrollable: true,
        constraints: const BoxConstraints(maxWidth: AppDesign.dialogMaxWidth),
        shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
        title: Text(title,
            style:
                AppDesign.type(AppDesign.dialogTitle, weight: FontWeight.w600)),
        content: Text(message,
            style:
                AppDesign.type(AppDesign.controlLabel).copyWith(height: 1.4)),
        actions: [
          ConsoleButton(
              label: 'Cancel',
              fontSize: AppDesign.controlLabel,
              onPressed: () => Navigator.pop(context, false)),
          ConsoleButton(
              label: confirmLabel,
              fontSize: AppDesign.controlLabel,
              primary: true,
              danger: danger,
              onPressed: () => Navigator.pop(context, true)),
        ],
      ),
    ) ??
    false;

/// Returns the entered text without applying domain validation or saving it.
/// The initial value remains local to this invocation, including while typing.
Future<String?> showConsoleNameDialog(
  BuildContext context, {
  required String title,
  required String confirmLabel,
  String initialValue = '',
  String? fieldLabel,
  int? maxLength,
}) {
  var draft = initialValue;
  return showDialog<String>(
    context: context,
    animationStyle: AnimationStyle.noAnimation,
    builder: (context) => ConsoleAlertDialog(
      backgroundColor: ConsolePalette.of(context).panel,
      scrollable: true,
      constraints: const BoxConstraints(maxWidth: AppDesign.dialogMaxWidth),
      shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
      title: Text(title,
          style:
              AppDesign.type(AppDesign.dialogTitle, weight: FontWeight.w600)),
      content: TextFormField(
          initialValue: initialValue,
          autofocus: true,
          maxLength: maxLength,
          decoration: InputDecoration(labelText: fieldLabel),
          onChanged: (value) => draft = value),
      actions: [
        ConsoleButton(
            label: 'Cancel',
            fontSize: AppDesign.controlLabel,
            onPressed: () => Navigator.pop(context)),
        ConsoleButton(
            label: confirmLabel,
            fontSize: AppDesign.controlLabel,
            primary: true,
            onPressed: () => Navigator.pop(context, draft)),
      ],
    ),
  );
}
