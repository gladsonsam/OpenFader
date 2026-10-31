import 'package:flutter/material.dart';

import '../design.dart';
import '../palette.dart';

/// Separate the drawn icon from its touch target so constraints cannot stretch it.
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

/// Anchored, bounded option control with explicit clearance before the arrow.
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

/// Shared panel surface; the Material also keeps descendant ink visible.
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
/// controls retain their target size with a fixed gap between the two.
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

/// Shared console switch: the track size is independent of its touch target.
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
