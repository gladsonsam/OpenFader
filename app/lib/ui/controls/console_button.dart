import 'package:flutter/material.dart';
import '../../palette.dart';
import '../../design.dart';
import '../motion.dart';

/// Shared console action: quiet hover/press feedback, keyboard focus, and a
/// 44-point target. Colour indicates state or intent rather than screen type.
class ConsoleButton extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;
  final bool selected, primary, danger;
  final Color? accent;
  final IconData? icon;
  final double fontSize;
  const ConsoleButton(
      {super.key,
      required this.label,
      required this.onPressed,
      this.selected = false,
      this.primary = false,
      this.danger = false,
      this.accent,
      this.icon,
      this.fontSize = AppDesign.smallLabel});

  @override
  Widget build(BuildContext context) {
    final active = danger
        ? ConsolePalette.of(context).mute
        : accent ?? ConsolePalette.of(context).lime;
    final filled = selected || primary;
    final animationsOff = MediaQuery.disableAnimationsOf(context);
    return ConsolePressFeedback(
      enabled: onPressed != null,
      child: TextButton(
          onPressed: onPressed,
          style: TextButton.styleFrom(
            backgroundColor:
                filled ? active : ConsolePalette.of(context).control,
            foregroundColor: filled
                ? (danger
                    ? ConsolePalette.of(context).onDanger
                    : ConsolePalette.of(context).onAccent)
                : ConsolePalette.of(context).text,
            disabledBackgroundColor: ConsolePalette.of(context).selected,
            disabledForegroundColor: ConsolePalette.of(context).mutedText,
            minimumSize: AppDesign.controlSize,
            padding: AppDesign.controlPadding,
            textStyle: AppDesign.type(fontSize, weight: FontWeight.w600),
            shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
            side:
                danger && !filled ? BorderSide(color: active) : BorderSide.none,
          ).copyWith(
              animationDuration:
                  animationsOff ? Duration.zero : ConsoleMotion.state,
              side: WidgetStateProperty.resolveWith((states) => states
                      .contains(WidgetState.focused)
                  ? BorderSide(color: ConsolePalette.of(context).selectedLine)
                  : danger && !filled
                      ? BorderSide(color: active)
                      : BorderSide.none)),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            if (icon != null) ...[
              Icon(icon, size: AppDesign.iconSize),
              const SizedBox(width: AppDesign.spaceSm),
            ],
            Flexible(
                child: Text(label,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textAlign: TextAlign.center)),
          ])),
    );
  }
}
