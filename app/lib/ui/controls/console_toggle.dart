import 'package:flutter/material.dart';

import '../../design.dart';
import '../../palette.dart';

class ConsoleToggle extends StatelessWidget {
  final String label;
  final bool value;
  final ValueChanged<bool>? onChanged;
  const ConsoleToggle(
      {super.key,
      required this.label,
      required this.value,
      required this.onChanged});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    final width = wide ? AppDesign.toggleWidth : AppDesign.compactToggleWidth;
    final height =
        wide ? AppDesign.toggleHeight : AppDesign.compactToggleHeight;
    final thumb = wide ? AppDesign.toggleThumb : AppDesign.compactToggleThumb;
    void toggle() => onChanged?.call(!value);
    return Semantics(
      button: true,
      enabled: onChanged != null,
      label: '$label ${value ? 'on' : 'off'}',
      onTap: onChanged == null ? null : toggle,
      child: SizedBox(
        width: width,
        height: AppDesign.touchTarget,
        child: InkWell(
          onTap: onChanged == null ? null : toggle,
          borderRadius: AppDesign.radius,
          child: Center(
              child: Container(
            width: width,
            height: height,
            decoration: BoxDecoration(
                color: value ? palette.cyan : palette.control,
                borderRadius: BorderRadius.circular(height / 2),
                border:
                    Border.all(color: value ? palette.cyan : palette.lineCtl)),
            child: Stack(children: [
              Positioned(
                left: value
                    ? width - thumb - AppDesign.spaceXs
                    : AppDesign.spaceXs,
                top: (height - thumb) / 2,
                child: Container(
                    width: thumb,
                    height: thumb,
                    decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: value ? palette.onAccent : palette.text)),
              )
            ]),
          )),
        ),
      ),
    );
  }
}
