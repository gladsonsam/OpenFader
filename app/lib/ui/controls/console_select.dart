import 'package:flutter/material.dart';

import '../../design.dart';
import '../../palette.dart';

class ConsoleSelect<T> extends StatelessWidget {
  final String label;
  final T value;
  final List<(T, String)> options;
  final ValueChanged<T>? onChanged;
  final double maxWidth;
  const ConsoleSelect(
      {super.key,
      required this.label,
      required this.value,
      required this.options,
      required this.onChanged,
      this.maxWidth = AppDesign.selectMaxWidth});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final selectedLabel =
        options.where((o) => o.$1 == value).firstOrNull?.$2 ?? '$value';
    return PopupMenuButton<T>(
      tooltip: label,
      initialValue: value,
      enabled: onChanged != null,
      popUpAnimationStyle: AnimationStyle.noAnimation,
      position: PopupMenuPosition.under,
      color: palette.chip,
      constraints: AppDesign.selectMenuConstraints,
      onSelected: onChanged,
      itemBuilder: (_) => [
        for (final option in options)
          CheckedPopupMenuItem<T>(
              height: AppDesign.controlHeight,
              value: option.$1,
              checked: option.$1 == value,
              child: Text(option.$2,
                  style: AppDesign.type(AppDesign.controlLabel))),
      ],
      child: Semantics(
        button: true,
        enabled: onChanged != null,
        child: Container(
          constraints: BoxConstraints(
              minHeight: AppDesign.touchTarget, maxWidth: maxWidth),
          padding: AppDesign.controlPadding,
          decoration: BoxDecoration(
              color: palette.control,
              borderRadius: BorderRadius.circular(AppDesign.fieldRadius),
              border: Border.all(color: palette.lineCtl)),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            Flexible(
                child: Text(selectedLabel,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: AppDesign.type(AppDesign.controlLabel,
                        color: palette.text1))),
            const SizedBox(width: AppDesign.spaceMd),
            Icon(Icons.expand_more,
                size: AppDesign.sectionTitle, color: palette.text7),
          ]),
        ),
      ),
    );
  }
}
