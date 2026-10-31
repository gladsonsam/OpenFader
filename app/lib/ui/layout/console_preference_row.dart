import 'package:flutter/material.dart';

import '../../design.dart';
import '../../palette.dart';

class ConsolePreferenceRow extends StatelessWidget {
  final String label;
  final String? description;
  final Widget trailing;
  final Color? foreground;
  final double minHeight;
  const ConsolePreferenceRow(
      {super.key,
      required this.label,
      this.description,
      required this.trailing,
      this.foreground,
      this.minHeight = AppDesign.settingsRowHeight});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Container(
      constraints: BoxConstraints(minHeight: minHeight),
      padding: const EdgeInsets.symmetric(
          horizontal: AppDesign.spaceLg, vertical: AppDesign.spaceMd),
      child: Row(children: [
        Expanded(
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(label,
              style: AppDesign.type(
                      wide ? AppDesign.rowLabel : AppDesign.controlLabel,
                      color: foreground ?? palette.text1,
                      weight: FontWeight.w500)
                  .copyWith(height: 1.3)),
          if (description != null) ...[
            const SizedBox(height: AppDesign.spaceXs),
            Text(description!,
                style: AppDesign.type(AppDesign.body, color: palette.text7)
                    .copyWith(height: 1.4)),
          ],
        ])),
        SizedBox(width: wide ? AppDesign.spaceXl : AppDesign.spaceLg),
        trailing,
      ]),
    );
  }
}
