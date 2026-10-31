import 'package:flutter/material.dart';
import '../palette.dart';
import '../design.dart';

/// Shared console action: quiet hover/press feedback, keyboard focus, and a
/// 44-point target. Colour indicates state or intent rather than screen type.
class ConsoleButton extends StatefulWidget {
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
  State<ConsoleButton> createState() => _ConsoleButtonState();
}

class _ConsoleButtonState extends State<ConsoleButton> {
  bool _pressed = false;

  void _setPressed(bool value) {
    if (_pressed == value) return;
    setState(() => _pressed = value);
  }

  @override
  Widget build(BuildContext context) {
    final active = widget.danger
        ? ConsolePalette.of(context).mute
        : widget.accent ?? ConsolePalette.of(context).lime;
    final filled = widget.selected || widget.primary;
    final animationsOff = MediaQuery.disableAnimationsOf(context);
    return GestureDetector(
      onTapDown:
          widget.onPressed == null ? null : (_) => _setPressed(true),
      onTapUp: widget.onPressed == null ? null : (_) => _setPressed(false),
      onTapCancel: () => _setPressed(false),
      child: AnimatedScale(
        scale: _pressed ? 0.97 : 1.0,
        duration: animationsOff
            ? Duration.zero
            : const Duration(milliseconds: 40),
        curve: Curves.easeOut,
        child: TextButton(
        onPressed: widget.onPressed,
        style: TextButton.styleFrom(
          backgroundColor: filled ? active : ConsolePalette.of(context).control,
          foregroundColor: filled
              ? (widget.danger
                  ? ConsolePalette.of(context).onDanger
                  : ConsolePalette.of(context).onAccent)
              : ConsolePalette.of(context).text,
          disabledBackgroundColor: ConsolePalette.of(context).selected,
          disabledForegroundColor: ConsolePalette.of(context).mutedText,
          minimumSize: const Size(44, 44),
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
          textStyle: AppDesign.type(13, weight: FontWeight.w600),
          shape: const RoundedRectangleBorder(borderRadius: AppDesign.radius),
          side: widget.danger && !filled ? BorderSide(color: active) : BorderSide.none,
        ).copyWith(
            animationDuration: animationsOff
                ? Duration.zero
                : const Duration(milliseconds: 40),
            side: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.focused)
                    ? BorderSide(color: ConsolePalette.of(context).selectedLine)
                    : widget.danger && !filled
                        ? BorderSide(color: active)
                        : BorderSide.none)),
        child: Text(widget.label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center)),
      ),
    );
  }
}
