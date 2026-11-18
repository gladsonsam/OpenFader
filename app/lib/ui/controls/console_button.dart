import 'package:material_ui/material_ui.dart';

import '../../palette.dart';
import '../../design.dart';
import '../motion.dart';

/// Emphasis and intent of a [ConsoleButton].
enum ConsoleButtonVariant {
  /// Default action on a control background.
  secondary,

  /// The single main action of a view or dialog, filled with the accent.
  primary,

  /// Low-emphasis action with no background, such as Cancel in dense spots.
  quiet,

  /// Risky action that is not the main one, outlined in the danger colour.
  danger,

  /// Confirms an irreversible action, filled with the danger colour.
  destructive,
}

/// Horizontal padding of a [ConsoleButton].
enum ConsoleButtonPadding {
  /// [AppDesign.controlPadding].
  standard,

  /// Horizontal [AppDesign.spaceSm] only.
  tight,

  /// None; for fixed-width slots inside an already-sized parent.
  none,
}

/// Label size of a [ConsoleButton]. Every size keeps the 44-point target.
enum ConsoleButtonSize {
  /// Smallest label, for dense inline actions.
  dense(AppDesign.minorText),

  /// Dense toolbars and inline actions.
  compact(AppDesign.smallLabel),

  /// Dialog actions and standard form buttons.
  regular(AppDesign.controlLabel),

  /// Prominent page-level actions.
  large(AppDesign.rowLabel),

  /// Front-door actions with a large glyph.
  hero(AppDesign.entryValue);

  final double fontSize;
  const ConsoleButtonSize(this.fontSize);
}

/// Shared console action: quiet hover/press feedback, keyboard focus, and a
/// 44-point target. Colour indicates state or intent rather than screen type.
class ConsoleButton extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;
  final ConsoleButtonVariant variant;
  final ConsoleButtonSize size;
  final bool selected;
  final Color? accent;
  final IconData? icon;

  /// On-colour used when the button is filled; defaults to the palette's.
  final Color? foreground;

  /// Custom widget before the label (alternative to [icon]).
  final Widget? leading;

  /// Widget after the label, such as a drop-down arrow.
  final Widget? trailing;
  final ConsoleButtonPadding padding;
  final MainAxisAlignment alignment;

  /// Optional muted second line under the label.
  final String? subtitle;
  const ConsoleButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.variant = ConsoleButtonVariant.secondary,
    this.size = ConsoleButtonSize.compact,
    this.selected = false,
    this.accent,
    this.icon,
    this.foreground,
    this.leading,
    this.trailing,
    this.padding = ConsoleButtonPadding.standard,
    this.alignment = MainAxisAlignment.center,
    this.subtitle,
  }) : assert(icon == null || leading == null);

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    final danger =
        variant == ConsoleButtonVariant.danger ||
        variant == ConsoleButtonVariant.destructive;
    final quiet = variant == ConsoleButtonVariant.quiet;
    final active = danger ? p.mute : accent ?? p.lime;
    final filled =
        selected ||
        variant == ConsoleButtonVariant.primary ||
        variant == ConsoleButtonVariant.destructive;
    final outlined = variant == ConsoleButtonVariant.danger && !filled;
    final textColor = filled
        ? (foreground ?? (danger ? p.onDanger : p.onAccent))
        : quiet && accent != null
        ? accent!
        : p.text;
    final animationsOff = MediaQuery.disableAnimationsOf(context);
    return ConsolePressFeedback(
      enabled: onPressed != null,
      child: TextButton(
        onPressed: onPressed,
        style:
            TextButton.styleFrom(
              backgroundColor: filled
                  ? active
                  : quiet
                  ? Colors.transparent
                  : p.control,
              foregroundColor: textColor,
              disabledBackgroundColor: quiet ? Colors.transparent : p.selected,
              disabledForegroundColor: p.mutedText,
              minimumSize: AppDesign.controlSize,
              alignment: switch (alignment) {
                MainAxisAlignment.start => Alignment.centerLeft,
                MainAxisAlignment.end => Alignment.centerRight,
                _ => Alignment.center,
              },
              padding: switch (padding) {
                ConsoleButtonPadding.standard => AppDesign.controlPadding,
                ConsoleButtonPadding.tight => const EdgeInsets.symmetric(
                  horizontal: AppDesign.spaceSm,
                ),
                ConsoleButtonPadding.none => EdgeInsets.zero,
              },
              textStyle: AppDesign.type(size.fontSize, weight: FontWeight.w600),
              shape: const RoundedRectangleBorder(
                borderRadius: AppDesign.radius,
              ),
              side: outlined ? BorderSide(color: active) : BorderSide.none,
            ).copyWith(
              animationDuration: animationsOff
                  ? Duration.zero
                  : ConsoleMotion.state,
              side: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.focused)
                    ? BorderSide(color: p.selectedLine)
                    : outlined
                    ? BorderSide(color: active)
                    : BorderSide.none,
              ),
            ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: alignment,
          children: [
            if (leading != null) ...[
              leading!,
              const SizedBox(width: AppDesign.spaceSm),
            ] else if (icon != null) ...[
              Icon(icon, size: AppDesign.iconSize),
              const SizedBox(width: AppDesign.spaceSm),
            ],
            Flexible(
              child: subtitle == null
                  ? Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      textAlign: alignment == MainAxisAlignment.start
                          ? TextAlign.start
                          : TextAlign.center,
                    )
                  : Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: alignment == MainAxisAlignment.start
                          ? CrossAxisAlignment.start
                          : CrossAxisAlignment.center,
                      children: [
                        Text(label, maxLines: 1, overflow: TextOverflow.ellipsis),
                        Text(
                          subtitle!,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: AppDesign.type(
                            AppDesign.minorText,
                            color: filled ? textColor : p.mutedText,
                            weight: FontWeight.w400,
                          ),
                        ),
                      ],
                    ),
            ),
            if (trailing != null) ...[
              const SizedBox(width: AppDesign.spaceSm),
              trailing!,
            ],
          ],
        ),
      ),
    );
  }
}
