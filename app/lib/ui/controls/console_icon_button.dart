import 'package:flutter/material.dart';

import '../../design.dart';
import '../../palette.dart';

class ConsoleIconButton extends StatelessWidget {
  final String label;
  final VoidCallback? onPressed;
  final Widget child;
  final bool selected, filled;
  const ConsoleIconButton(
      {super.key,
      required this.label,
      required this.onPressed,
      required this.child,
      this.selected = false,
      this.filled = false});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Semantics(
      button: true,
      enabled: onPressed != null,
      selected: selected,
      label: label,
      child: Tooltip(
        message: label,
        child: SizedBox.square(
          dimension: AppDesign.touchTarget,
          child: Material(
            animationDuration: Duration.zero,
            color: selected
                ? palette.selected
                : filled
                    ? palette.chip
                    : Colors.transparent,
            shape: RoundedRectangleBorder(
                borderRadius: AppDesign.radius,
                side: filled
                    ? BorderSide(color: selected ? palette.cyan : palette.line)
                    : BorderSide.none),
            child: InkWell(
                onTap: onPressed,
                borderRadius: AppDesign.radius,
                child: Center(child: child)),
          ),
        ),
      ),
    );
  }
}
