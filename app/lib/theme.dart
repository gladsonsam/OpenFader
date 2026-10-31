import 'package:flutter/material.dart';
import 'design.dart';
import 'palette.dart';

/// Shared geometry and fixed channel identity colors. Runtime UI colors
/// belong to ConsolePalette so light/dark and accent stay coherent.
abstract final class U4 {
  static const font = AppDesign.font;
  static const headerH = AppDesign.toolbarCompactHeight;
  static const r3 = Radius.circular(AppDesign.panelRadius);
  static const r5 = Radius.circular(AppDesign.panelRadius);
  static const r8 = Radius.circular(AppDesign.panelRadius);
  static const rStrip = AppDesign.radius;
  static const rWell = AppDesign.radius;
  static const rBtn = AppDesign.radius;

  /// Console-tape palette for input channels (designData.ts CHAN_TAPE):
  /// one colour per channel position (mod 8).
  static const tape = <List<Color>>[
    [Color(0xFFD64545), Color(0xFFFFFFFF)],
    [Color(0xFFE8853D), Color(0xFF331503)],
    [Color(0xFFE3C341), Color(0xFF2E2503)],
    [Color(0xFF5CDB5C), Color(0xFF10240F)],
    [Color(0xFF22D3E6), Color(0xFF062A30)],
    [Color(0xFF4F8FF7), Color(0xFFFFFFFF)],
    [Color(0xFF8E4EC6), Color(0xFFFFFFFF)],
    [Color(0xFFE93D82), Color(0xFFFFFFFF)],
  ];

  /// Bank rail colours (bankDefs + BANK_TINT in shellStyle.ts).
  static const banks = <({String key, String label, Color color, Color text})>[
    (
      key: 'ch1',
      label: 'Ch 1-16',
      color: Color(0xFF5CDB5C),
      text: Color(0xFF10240F)
    ),
    (
      key: 'fx',
      label: 'FxRet',
      color: Color(0xFF5CDB5C),
      text: Color(0xFF10240F)
    ),
    (
      key: 'mix',
      label: 'Mix',
      color: Color(0xFF22D3E6),
      text: Color(0xFF062A30)
    ),
    (
      key: 'grp',
      label: 'Grp',
      color: Color(0xFF2563EB),
      text: Color(0xFFFFFFFF)
    ),
    (
      key: 'mtx',
      label: 'Mtx',
      color: Color(0xFFC026D3),
      text: Color(0xFFFFFFFF)
    ),
    (
      key: 'dca',
      label: 'DCA',
      color: Color(0xFFDC2626),
      text: Color(0xFFFFFFFF)
    ),
  ];
}

/// One theme for the mixer, detail pages, dialogs, menus and startup screen.
ThemeData u4Theme(
    {Brightness brightness = Brightness.dark, String accent = 'lime'}) {
  final p = ConsolePalette(brightness: brightness, accent: accent);
  final scheme = ColorScheme(
    brightness: brightness,
    surface: p.page,
    surfaceContainer: p.panel,
    surfaceContainerHighest: p.control,
    primary: p.cyan,
    onPrimary: p.onAccent,
    secondary: p.cyan,
    onSecondary: p.onAccent,
    error: p.meterRed,
    onError: p.onDanger,
    onSurface: p.text1,
    onSurfaceVariant: p.text7,
    outline: p.lineCtl,
    outlineVariant: p.linePanel,
  );
  final theme = ThemeData(
    useMaterial3: true,
    brightness: brightness,
    extensions: [p],
    colorScheme: scheme,
    scaffoldBackgroundColor: p.page,
    canvasColor: p.page,
    cardColor: p.panel,
    dividerColor: p.divider,
    fontFamily: U4.font,
    splashFactory: NoSplash.splashFactory,
  );
  final shape = RoundedRectangleBorder(borderRadius: U4.rBtn);
  return theme.copyWith(
    textTheme: theme.textTheme
        .apply(bodyColor: p.text1, displayColor: p.text1)
        .copyWith(
            bodyMedium: AppDesign.type(AppDesign.body, color: p.text),
            bodySmall: AppDesign.type(AppDesign.caption, color: p.text),
            bodyLarge: AppDesign.type(AppDesign.controlLabel, color: p.text),
            titleSmall: AppDesign.type(AppDesign.controlLabel,
                color: p.text, weight: FontWeight.w600)),
    textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
      animationDuration: AppDesign.feedbackDuration,
      foregroundColor: p.text1,
      backgroundColor: p.control,
      minimumSize: AppDesign.controlSize,
      shape: shape,
      textStyle: AppDesign.type(AppDesign.body, weight: FontWeight.w600),
    ).copyWith(
            overlayColor: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.pressed)
                    ? p.text.withValues(alpha: .16)
                    : states.contains(WidgetState.hovered)
                        ? p.text.withValues(alpha: .06)
                        : null))),
    outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
      animationDuration: AppDesign.feedbackDuration,
      foregroundColor: p.cyan,
      minimumSize: AppDesign.controlSize,
      side: BorderSide(color: p.lineCtl),
      shape: shape,
    )),
    filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
      animationDuration: AppDesign.feedbackDuration,
      foregroundColor: p.onAccent,
      backgroundColor: p.cyan,
      minimumSize: AppDesign.controlSize,
      shape: shape,
    )),
    iconButtonTheme: IconButtonThemeData(
        style: IconButton.styleFrom(
      animationDuration: AppDesign.feedbackDuration,
      foregroundColor: p.text7,
      iconSize: AppDesign.iconSize,
      minimumSize: AppDesign.controlSize,
      shape: shape,
    )),
    inputDecorationTheme: InputDecorationThemeData(
      filled: true,
      fillColor: p.well,
      labelStyle: TextStyle(color: p.text7),
      hintStyle: TextStyle(color: p.text10),
      contentPadding: AppDesign.fieldPadding,
      border: OutlineInputBorder(
          borderRadius: U4.rBtn, borderSide: BorderSide(color: p.lineCtl)),
      enabledBorder: OutlineInputBorder(
          borderRadius: U4.rBtn, borderSide: BorderSide(color: p.lineCtl)),
      focusedBorder: OutlineInputBorder(
          borderRadius: U4.rBtn, borderSide: BorderSide(color: p.cyan)),
    ),
    popupMenuTheme: PopupMenuThemeData(
        color: p.selected,
        surfaceTintColor: Colors.transparent,
        shape: shape,
        textStyle: AppDesign.type(15)),
    dialogTheme: DialogThemeData(
        backgroundColor: p.panel,
        surfaceTintColor: Colors.transparent,
        shape: shape),
    bottomSheetTheme: BottomSheetThemeData(
        backgroundColor: p.page, surfaceTintColor: Colors.transparent),
    tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(color: p.control, borderRadius: U4.rBtn),
        textStyle: AppDesign.type(12)),
    sliderTheme: theme.sliderTheme.copyWith(
        activeTrackColor: p.cyan,
        thumbColor: p.text1,
        inactiveTrackColor: p.control,
        overlayColor: p.cyan.withValues(alpha: .08)),
    scrollbarTheme: const ScrollbarThemeData(
        thumbVisibility: WidgetStatePropertyAll(false)),
  );
}
