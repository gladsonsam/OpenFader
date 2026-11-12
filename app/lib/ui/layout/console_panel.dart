import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';

class ConsolePanel extends StatelessWidget {
  final Widget child;
  final EdgeInsetsGeometry padding;
  final Color? color;
  const ConsolePanel(
      {super.key,
      required this.child,
      this.padding = AppDesign.panelPadding,
      this.color});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Material(
        animationDuration: Duration.zero,
        color: color ?? palette.panel,
        shape: RoundedRectangleBorder(
            borderRadius: AppDesign.radius,
            side: BorderSide(color: palette.linePanel)),
        clipBehavior: Clip.antiAlias,
        child: Padding(padding: padding, child: child));
  }
}

/// Common preference/action row. Long text wraps in its own lane; trailing
