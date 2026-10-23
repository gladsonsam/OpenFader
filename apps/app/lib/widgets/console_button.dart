import 'package:flutter/material.dart';
import '../design.dart';

/// Shared console action: quiet hover/press feedback, keyboard focus, and a
/// 44-point target. Colour indicates state or intent rather than screen type.
class ConsoleButton extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;
  final bool selected, primary, danger;
  final Color? accent;
  const ConsoleButton(
      {super.key,
      required this.label,
      required this.onPressed,
      this.selected = false,
      this.primary = false,
      this.danger = false,
      this.accent});

  @override
  Widget build(BuildContext context) {
    final active = danger ? AppDesign.mute : accent ?? AppDesign.lime;
    final filled = selected || primary;
    return TextButton(
        onPressed: onPressed,
        style: TextButton.styleFrom(
          backgroundColor: filled ? active : AppDesign.control,
          foregroundColor: filled && !danger ? AppDesign.well : AppDesign.text,
          disabledBackgroundColor: AppDesign.selected,
          disabledForegroundColor: AppDesign.mutedText,
          minimumSize: const Size(44, 44),
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          textStyle: AppDesign.type(13, weight: FontWeight.w600),
          shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
          side: danger && !filled ? BorderSide(color: active) : BorderSide.none,
        ).copyWith(
            animationDuration: MediaQuery.disableAnimationsOf(context)
                ? Duration.zero
                : const Duration(milliseconds: 100),
            side: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.focused)
                    ? const BorderSide(color: AppDesign.selectedLine)
                    : danger && !filled
                        ? BorderSide(color: active)
                        : BorderSide.none)),
        child: Text(label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center));
  }
}
