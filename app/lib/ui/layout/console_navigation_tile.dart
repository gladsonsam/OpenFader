import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';
import '../motion.dart';
import '../text/console_text.dart';

/// Full-width tappable row that opens a page or action. Disabled when
/// [onTap] is null.
class ConsoleNavigationTile extends StatelessWidget {
  final String label;
  final String? detail;
  final Widget? leading;
  final Widget? trailing;
  final VoidCallback? onTap;
  final bool selected, showChevron;

  /// Panel background with a hairline border and a bold label, for tiles that
  /// stand alone on a page rather than in a list.
  final bool outlined;
  const ConsoleNavigationTile({
    super.key,
    required this.label,
    required this.onTap,
    this.detail,
    this.leading,
    this.trailing,
    this.selected = false,
    this.showChevron = true,
    this.outlined = false,
  });

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final enabled = onTap != null;
    final tone = enabled ? ConsoleTextTone.normal : ConsoleTextTone.subtle;
    final end =
        trailing ??
        (!showChevron
            ? null
            : Icon(
                Icons.chevron_right,
                size: AppDesign.iconSize,
                color: consoleTextColor(context, tone),
              ));
    return Semantics(
      button: true,
      enabled: enabled,
      selected: selected,
      label: label,
      excludeSemantics: true,
      child: ConsolePressFeedback(
        enabled: enabled,
        child: Material(
          animationDuration: MediaQuery.disableAnimationsOf(context)
              ? Duration.zero
              : ConsoleMotion.state,
          color: selected
              ? palette.selected
              : outlined
              ? palette.panel
              : palette.navigationTile,
          shape: RoundedRectangleBorder(
            borderRadius: outlined
                ? BorderRadius.circular(AppDesign.panelRadius)
                : AppDesign.radius,
            side: selected
                ? BorderSide(color: palette.accent)
                : outlined
                ? BorderSide(color: palette.linePanel)
                : BorderSide.none,
          ),
          child: InkWell(
            onTap: onTap,
            borderRadius: outlined
                ? BorderRadius.circular(AppDesign.panelRadius)
                : AppDesign.radius,
            child: ConstrainedBox(
              constraints: const BoxConstraints(
                minHeight: AppDesign.controlHeight,
              ),
              child: Padding(
                padding: AppDesign.controlPadding,
                child: Row(
                  children: [
                    if (leading != null) ...[
                      IconTheme.merge(
                        data: IconThemeData(
                          size: AppDesign.iconSize,
                          color: consoleTextColor(context, tone),
                        ),
                        child: leading!,
                      ),
                      const SizedBox(width: AppDesign.spaceMd),
                    ],
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          ConsoleText(
                            label,
                            role: outlined
                                ? ConsoleTextRole.rowLabel
                                : ConsoleTextRole.controlLabel,
                            weight: outlined ? FontWeight.w600 : null,
                            tone: tone,
                          ),
                          if (detail != null)
                            ConsoleText(
                              detail!,
                              role: outlined
                                  ? ConsoleTextRole.smallLabel
                                  : ConsoleTextRole.body,
                              tone: ConsoleTextTone.muted,
                            ),
                        ],
                      ),
                    ),
                    if (end != null) ...[
                      const SizedBox(width: AppDesign.spaceMd),
                      end,
                    ],
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
