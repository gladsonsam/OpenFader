import 'package:material_ui/material_ui.dart';

import '../../design.dart';
import '../../palette.dart';

/// Semantic text size and weight. Screens pick a role, never a number.
enum ConsoleTextRole {
  pageTitle(AppDesign.pageTitle, FontWeight.w600),
  dialogTitle(AppDesign.dialogTitle, FontWeight.w600),
  sheetTitle(AppDesign.sheetTitle, FontWeight.w600),
  sectionTitle(AppDesign.sectionTitle, FontWeight.w600),
  rowLabel(AppDesign.rowLabel, FontWeight.w400),
  controlLabel(AppDesign.controlLabel, FontWeight.w400),
  body(AppDesign.body, FontWeight.w400),
  smallLabel(AppDesign.smallLabel, FontWeight.w400),
  caption(AppDesign.caption, FontWeight.w400),
  micro(AppDesign.microText, FontWeight.w400),
  minor(AppDesign.minorText, FontWeight.w400),
  entryValue(AppDesign.entryValue, FontWeight.w600, tabular: true),
  parameterValue(AppDesign.parameterValue, FontWeight.w600, tabular: true),
  compactParameterValue(
    AppDesign.compactParameterValue,
    FontWeight.w600,
    tabular: true,
  );

  final double size;
  final FontWeight weight;
  final bool tabular;
  const ConsoleTextRole(this.size, this.weight, {this.tabular = false});
}

/// Semantic text colour resolved from the active palette.
enum ConsoleTextTone {
  normal,
  muted,
  subtle,
  accent,
  danger,
  warning,
  success,
  onAccent,
}

Color consoleTextColor(BuildContext context, ConsoleTextTone tone) {
  final p = ConsolePalette.of(context);
  return switch (tone) {
    ConsoleTextTone.normal => p.text,
    ConsoleTextTone.muted => p.mutedText,
    ConsoleTextTone.subtle => p.textSubtle,
    ConsoleTextTone.accent => p.accent,
    ConsoleTextTone.danger => p.mute,
    ConsoleTextTone.warning => p.amber,
    ConsoleTextTone.success => p.success,
    ConsoleTextTone.onAccent => p.onAccent,
  };
}

TextStyle consoleTextStyle(
  BuildContext context,
  ConsoleTextRole role, {
  ConsoleTextTone tone = ConsoleTextTone.normal,
  FontWeight? weight,
}) {
  final style = AppDesign.type(
    role.size,
    color: consoleTextColor(context, tone),
    weight: weight ?? role.weight,
  );
  return role.tabular
      ? style.copyWith(fontFeatures: const [FontFeature.tabularFigures()])
      : style;
}

class ConsoleText extends StatelessWidget {
  final String text;
  final ConsoleTextRole role;
  final ConsoleTextTone tone;
  final FontWeight? weight;
  final int? maxLines;
  final TextOverflow? overflow;
  final TextAlign? textAlign;
  const ConsoleText(
    this.text, {
    super.key,
    this.role = ConsoleTextRole.body,
    this.tone = ConsoleTextTone.normal,
    this.weight,
    this.maxLines,
    this.overflow,
    this.textAlign,
  });

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: consoleTextStyle(context, role, tone: tone, weight: weight),
    maxLines: maxLines,
    overflow: overflow,
    textAlign: textAlign,
  );
}
