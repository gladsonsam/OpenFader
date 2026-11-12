import 'package:material_ui/material_ui.dart';

import '../../theme.dart';
import '../../ui/console.dart';

class U4Section extends StatelessWidget {
  final String title;
  final Widget child;
  final Widget? trailing;

  const U4Section(
      {super.key, required this.title, required this.child, this.trailing});

  @override
  Widget build(BuildContext context) {
    return ConsolePanel(
      padding: const EdgeInsets.all(10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(title,
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 12,
                        color: ConsolePalette.of(context).text7)),
              ),
              ?trailing,
            ],
          ),
          const SizedBox(height: 10),
          child,
        ],
      ),
    );
  }
}

/// Custom-painted toggle; do not replace with a Switch widget.
class U4Chip extends StatelessWidget {
  final String label;
  final bool on;
  final bool enabled;
  final VoidCallback? onTap;
  final Color? accent;

  const U4Chip({
    super.key,
    required this.label,
    required this.on,
    this.enabled = true,
    this.onTap,
    this.accent,
  });

  @override
  Widget build(BuildContext context) {
    return Semantics(
        toggled: on,
        child: ConsoleButton(
            label: label,
            selected: on,
            accent: accent,
            onPressed: enabled ? onTap : null));
  }
}
