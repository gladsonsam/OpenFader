import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';
import '../controls/console_button.dart';
import '../motion.dart';
import '../text/console_text.dart';

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
    super.buttonPadding = const EdgeInsets.symmetric(
      horizontal: AppDesign.spaceXs,
    ),
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

/// Standard console dialog: panel surface, title, body and action bar.
/// Dialogs only return decisions or text; callers own validation and actions.
class ConsoleDialog extends StatelessWidget {
  final String title;
  final Widget? content;
  final String? message;
  final List<Widget> actions;
  final double maxWidth;
  final bool scrollable;
  const ConsoleDialog({
    super.key,
    required this.title,
    this.content,
    this.message,
    this.actions = const [],
    this.maxWidth = AppDesign.dialogMaxWidth,
    this.scrollable = true,
  }) : assert(content == null || message == null);

  @override
  Widget build(BuildContext context) => ConsoleAlertDialog(
    backgroundColor: ConsolePalette.of(context).panel,
    scrollable: scrollable,
    constraints: BoxConstraints(maxWidth: maxWidth),
    shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
    title: ConsoleText(title, role: ConsoleTextRole.dialogTitle),
    content:
        content ??
        (message == null
            ? null
            : Text(
                message!,
                style: consoleTextStyle(
                  context,
                  ConsoleTextRole.controlLabel,
                ).copyWith(height: 1.4),
              )),
    actions: actions,
  );
}

/// Shows [builder] without a transition. Dialogs only return decisions or text.
Future<T?> showConsoleDialog<T>(
  BuildContext context, {
  required WidgetBuilder builder,
  bool barrierDismissible = true,
}) => showDialog<T>(
  context: context,
  animationStyle: AnimationStyle.noAnimation,
  barrierDismissible: barrierDismissible,
  builder: builder,
);

/// One outcome of [showConsoleChoice].
class ConsoleDialogAction<T> {
  final String label;
  final T value;
  final ConsoleButtonVariant variant;
  const ConsoleDialogAction({
    required this.label,
    required this.value,
    this.variant = ConsoleButtonVariant.secondary,
  });
}

/// Offers two or three outcomes. Cancel returns null.
Future<T?> showConsoleChoice<T>(
  BuildContext context, {
  required String title,
  String? message,
  required List<ConsoleDialogAction<T>> actions,
}) => showConsoleDialog<T>(
  context,
  builder: (context) => ConsoleDialog(
    title: title,
    message: message,
    actions: [
      ConsoleButton(
        label: 'Cancel',
        variant: ConsoleButtonVariant.quiet,
        size: ConsoleButtonSize.regular,
        onPressed: () => Navigator.pop(context),
      ),
      for (final action in actions)
        ConsoleButton(
          label: action.label,
          variant: action.variant,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context, action.value),
        ),
    ],
  ),
);

/// One row of [showConsoleSelection].
class ConsoleSelectionOption<T> {
  final String label;
  final T value;
  final String? detail;
  final IconData? icon;
  final bool enabled;
  const ConsoleSelectionOption({
    required this.label,
    required this.value,
    this.detail,
    this.icon,
    this.enabled = true,
  });
}

/// Picks one of several options. Cancel returns null. [selected] shows a
/// check mark but does not change what a tap returns.
Future<T?> showConsoleSelection<T>(
  BuildContext context, {
  required String title,
  String? message,
  required List<ConsoleSelectionOption<T>> options,
  T? selected,
}) => showConsoleDialog<T>(
  context,
  builder: (context) => ConsoleDialog(
    title: title,
    content: Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (message != null)
          Padding(
            padding: const EdgeInsets.only(bottom: AppDesign.spaceSm),
            child: Text(
              message,
              style: consoleTextStyle(
                context,
                ConsoleTextRole.controlLabel,
              ).copyWith(height: 1.4),
            ),
          ),
        for (final option in options)
          _SelectionRow<T>(
            option: option,
            selected: selected != null && option.value == selected,
            onTap: () => Navigator.pop(context, option.value),
          ),
      ],
    ),
    actions: [
      ConsoleButton(
        label: 'Cancel',
        variant: ConsoleButtonVariant.quiet,
        size: ConsoleButtonSize.regular,
        onPressed: () => Navigator.pop(context),
      ),
    ],
  ),
);

class _SelectionRow<T> extends StatelessWidget {
  final ConsoleSelectionOption<T> option;
  final bool selected;
  final VoidCallback onTap;
  const _SelectionRow({
    required this.option,
    required this.selected,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final tone = option.enabled
        ? ConsoleTextTone.normal
        : ConsoleTextTone.subtle;
    return Semantics(
      button: true,
      enabled: option.enabled,
      selected: selected,
      label: option.label,
      excludeSemantics: true,
      child: ConsolePressFeedback(
        enabled: option.enabled,
        child: InkWell(
          onTap: option.enabled ? onTap : null,
          borderRadius: AppDesign.radius,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: AppDesign.touchTarget),
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: AppDesign.spaceSm),
              child: Row(
                children: [
                  if (option.icon != null) ...[
                    Icon(
                      option.icon,
                      size: AppDesign.iconSize,
                      color: consoleTextColor(context, tone),
                    ),
                    const SizedBox(width: AppDesign.spaceMd),
                  ],
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        ConsoleText(
                          option.label,
                          role: ConsoleTextRole.controlLabel,
                          tone: tone,
                          weight: selected ? FontWeight.w600 : null,
                        ),
                        if (option.detail != null)
                          ConsoleText(
                            option.detail!,
                            role: ConsoleTextRole.body,
                            tone: ConsoleTextTone.muted,
                          ),
                      ],
                    ),
                  ),
                  if (selected) ...[
                    const SizedBox(width: AppDesign.spaceMd),
                    Icon(
                      Icons.check,
                      size: AppDesign.iconSize,
                      color: consoleTextColor(context, ConsoleTextTone.accent),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
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
    await showConsoleDialog<bool>(
      context,
      builder: (context) => ConsoleDialog(
        title: title,
        message: message,
        actions: [
          ConsoleButton(
            label: 'Cancel',
            size: ConsoleButtonSize.regular,
            onPressed: () => Navigator.pop(context, false),
          ),
          ConsoleButton(
            label: confirmLabel,
            variant: danger
                ? ConsoleButtonVariant.destructive
                : ConsoleButtonVariant.primary,
            size: ConsoleButtonSize.regular,
            onPressed: () => Navigator.pop(context, true),
          ),
        ],
      ),
    ) ??
    false;

/// Returns the entered text without applying domain validation or saving it.
Future<String?> showConsoleNameDialog(
  BuildContext context, {
  required String title,
  required String confirmLabel,
  String initialValue = '',
  String? fieldLabel,
  int? maxLength,
}) => showConsoleTextInput(
  context,
  title: title,
  confirmLabel: confirmLabel,
  initialValue: initialValue,
  fieldLabel: fieldLabel,
  maxLength: maxLength,
);

/// Returns the raw entered text; callers parse and apply it. A non-null
/// [validate] result is shown inline and keeps the dialog open.
Future<String?> showConsoleTextInput(
  BuildContext context, {
  required String title,
  required String confirmLabel,
  String initialValue = '',
  String? fieldLabel,
  String? hintText,
  TextInputType? keyboardType,
  int? maxLength,
  String? Function(String value)? validate,
}) => showConsoleDialog<String>(
  context,
  builder: (context) => _TextInputDialog(
    title: title,
    confirmLabel: confirmLabel,
    initialValue: initialValue,
    fieldLabel: fieldLabel,
    hintText: hintText,
    keyboardType: keyboardType,
    maxLength: maxLength,
    validate: validate,
  ),
);

class _TextInputDialog extends StatefulWidget {
  final String title, confirmLabel, initialValue;
  final String? fieldLabel, hintText;
  final TextInputType? keyboardType;
  final int? maxLength;
  final String? Function(String value)? validate;
  const _TextInputDialog({
    required this.title,
    required this.confirmLabel,
    required this.initialValue,
    this.fieldLabel,
    this.hintText,
    this.keyboardType,
    this.maxLength,
    this.validate,
  });

  @override
  State<_TextInputDialog> createState() => _TextInputDialogState();
}

class _TextInputDialogState extends State<_TextInputDialog> {
  late final controller = TextEditingController(text: widget.initialValue);
  String? error;

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  void submit() {
    final problem = widget.validate?.call(controller.text);
    if (problem != null) {
      setState(() => error = problem);
      return;
    }
    Navigator.pop(context, controller.text);
  }

  @override
  Widget build(BuildContext context) => ConsoleDialog(
    title: widget.title,
    content: TextField(
      controller: controller,
      autofocus: true,
      maxLength: widget.maxLength,
      keyboardType: widget.keyboardType,
      decoration: InputDecoration(
        labelText: widget.fieldLabel,
        hintText: widget.hintText,
        errorText: error,
      ),
      onChanged: (_) {
        if (error != null) setState(() => error = null);
      },
      onSubmitted: (_) => submit(),
    ),
    actions: [
      ConsoleButton(
        label: 'Cancel',
        size: ConsoleButtonSize.regular,
        onPressed: () => Navigator.pop(context),
      ),
      ConsoleButton(
        label: widget.confirmLabel,
        variant: ConsoleButtonVariant.primary,
        size: ConsoleButtonSize.regular,
        onPressed: submit,
      ),
    ],
  );
}
