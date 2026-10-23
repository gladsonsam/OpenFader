import 'package:flutter/material.dart';
import 'design.dart';

/// Compatibility names for the shared graphite console design. Screen and
/// DSP code retain their token contract while matching the main mixer.
abstract final class U4 {
  static const font = AppDesign.font;
  static const page = AppDesign.background;
  static const panel = AppDesign.panel;
  static const well = AppDesign.well;
  static const control = AppDesign.control;
  static const selected = AppDesign.selected;
  static const headerBg = AppDesign.background;
  static const headerH = 64.0;
  static const lineHeader = AppDesign.line;
  static const linePanel = AppDesign.line;
  static const lineCtl = Color(0xff484848);
  static const railBorder = AppDesign.line;
  static const divider = AppDesign.line;
  static const bankOff = AppDesign.panel;
  static const tileBg = AppDesign.control;
  static const tileBorder = AppDesign.line;
  static const text1 = AppDesign.text;
  static const text4 = Color(0xffc8c8c8);
  static const text7 = AppDesign.mutedText;
  static const text10 = Color(0xff808080);
  // The legacy name is retained for consumers; active controls are now lime.
  static const cyan = AppDesign.lime;
  static const masterLine = AppDesign.selectedLine;
  static const meterGreen = AppDesign.meter;
  static const meterAmber = AppDesign.amber;
  static const meterRed = AppDesign.mute;
  static const meterCover = Color(0xff233306);
  static const peakLine = AppDesign.text;
  static const clipOn = AppDesign.mute;
  static const clipOff = Color(0xff3a130c);
  static const muteOffBg = AppDesign.control;
  static const muteOffTx = AppDesign.text;
  static const muteOffBd = AppDesign.line;
  static const muteOnBg = AppDesign.mute;
  static const muteOnBd = AppDesign.mute;
  static const railBg = AppDesign.well;
  static const railRing = AppDesign.line;
  static const scaleTick = AppDesign.mutedText;
  static const panMarker = AppDesign.text;
  static const panNotch = Color(0xff555555);
  static const panFill = Color(0xffaaaaaa);
  static const r3 = Radius.circular(10);
  static const r5 = Radius.circular(10);
  static const r8 = Radius.circular(10);
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
ThemeData u4Theme() {
  const scheme = ColorScheme.dark(
    surface: U4.page,
    surfaceContainer: U4.panel,
    surfaceContainerHighest: U4.control,
    primary: U4.cyan,
    onPrimary: U4.well,
    secondary: U4.cyan,
    onSecondary: U4.well,
    error: U4.meterRed,
    onSurface: U4.text1,
    onSurfaceVariant: U4.text7,
    outline: U4.lineCtl,
    outlineVariant: U4.linePanel,
  );
  final theme = ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: U4.page,
    canvasColor: U4.page,
    cardColor: U4.panel,
    dividerColor: U4.divider,
    fontFamily: U4.font,
    splashFactory: NoSplash.splashFactory,
  );
  final shape = RoundedRectangleBorder(borderRadius: U4.rBtn);
  return theme.copyWith(
    textTheme: theme.textTheme
        .apply(bodyColor: U4.text1, displayColor: U4.text1)
        .copyWith(
            bodyMedium: AppDesign.type(14),
            bodySmall: AppDesign.type(12),
            bodyLarge: AppDesign.type(16),
            titleSmall: AppDesign.type(16, weight: FontWeight.w600)),
    textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
      foregroundColor: U4.text1,
      backgroundColor: U4.control,
      minimumSize: const Size(44, 44),
      shape: shape,
      textStyle: AppDesign.type(14, weight: FontWeight.w600),
    ).copyWith(
            overlayColor: WidgetStateProperty.resolveWith(
                (states) => states.contains(WidgetState.pressed)
                    ? Colors.white.withValues(alpha: .16)
                    : states.contains(WidgetState.hovered)
                        ? Colors.white.withValues(alpha: .06)
                        : null))),
    outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
      foregroundColor: U4.cyan,
      minimumSize: const Size(44, 44),
      side: const BorderSide(color: U4.lineCtl),
      shape: shape,
    )),
    filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
      foregroundColor: U4.well,
      backgroundColor: U4.cyan,
      minimumSize: const Size(44, 44),
      shape: shape,
    )),
    iconButtonTheme: IconButtonThemeData(
        style: IconButton.styleFrom(
      foregroundColor: U4.text7,
      minimumSize: const Size(44, 44),
      shape: shape,
    )),
    inputDecorationTheme: InputDecorationThemeData(
      filled: true,
      fillColor: U4.well,
      labelStyle: const TextStyle(color: U4.text7),
      hintStyle: const TextStyle(color: U4.text10),
      contentPadding: const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
      border: OutlineInputBorder(
          borderRadius: U4.rBtn,
          borderSide: const BorderSide(color: U4.lineCtl)),
      enabledBorder: OutlineInputBorder(
          borderRadius: U4.rBtn,
          borderSide: const BorderSide(color: U4.lineCtl)),
      focusedBorder: OutlineInputBorder(
          borderRadius: U4.rBtn, borderSide: const BorderSide(color: U4.cyan)),
    ),
    popupMenuTheme: PopupMenuThemeData(
        color: U4.selected,
        surfaceTintColor: Colors.transparent,
        shape: shape,
        textStyle: AppDesign.type(15)),
    dialogTheme: DialogThemeData(
        backgroundColor: U4.panel,
        surfaceTintColor: Colors.transparent,
        shape: shape),
    bottomSheetTheme: const BottomSheetThemeData(
        backgroundColor: U4.page, surfaceTintColor: Colors.transparent),
    tooltipTheme: TooltipThemeData(
        decoration: BoxDecoration(color: U4.control, borderRadius: U4.rBtn),
        textStyle: AppDesign.type(12)),
    sliderTheme: theme.sliderTheme.copyWith(
        activeTrackColor: U4.cyan,
        thumbColor: U4.text1,
        inactiveTrackColor: U4.control,
        overlayColor: U4.cyan.withValues(alpha: .08)),
    scrollbarTheme: const ScrollbarThemeData(
        thumbVisibility: WidgetStatePropertyAll(false)),
  );
}
