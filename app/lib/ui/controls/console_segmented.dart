import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';

/// Horizontal row of underline-style tabs that share the width equally. The
/// selected tab uses the accent colour and a thicker underline.
class ConsoleTabStrip<T> extends StatelessWidget {
  final List<(T, String)> tabs;
  final T selected;
  final ValueChanged<T> onSelected;
  const ConsoleTabStrip({
    super.key,
    required this.tabs,
    required this.selected,
    required this.onSelected,
  });

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    return Row(
      spacing: AppDesign.spaceSm,
      children: [
        for (final (value, label) in tabs)
          Expanded(
            child: Semantics(
              selected: value == selected,
              child: Container(
                decoration: BoxDecoration(
                  border: Border(
                    bottom: BorderSide(
                      color: value == selected ? p.accent : p.tileBorder,
                      width: value == selected ? 3 : 1,
                    ),
                  ),
                ),
                child: TextButton(
                  style: TextButton.styleFrom(
                    minimumSize: const Size(0, AppDesign.controlHeight),
                    foregroundColor: value == selected ? p.accent : p.textSecondary,
                    padding: const EdgeInsets.symmetric(
                      horizontal: AppDesign.spaceMd,
                    ),
                    textStyle: AppDesign.type(
                      AppDesign.smallLabel,
                      weight: FontWeight.w600,
                    ),
                  ),
                  onPressed: () {
                    if (value != selected) onSelected(value);
                  },
                  child: Text(label, maxLines: 1),
                ),
              ),
            ),
          ),
      ],
    );
  }
}
