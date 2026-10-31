import 'package:flutter/material.dart';

/// Shared application UI tokens. Prefer named roles over page-local literals.
/// Reusable controls live in widgets/console_controls.dart and console_button.dart.
/// Runtime colors live in ConsolePalette and are resolved from the widget tree.
abstract final class AppDesign {
  static const radius = BorderRadius.all(Radius.circular(panelRadius));
  static const font = 'Barlow';

  // One immutable scale for application chrome and forms. Keep graph geometry
  // and protocol/fader calculations in their owning modules.
  static const spaceXs = 4.0;
  static const spaceSm = 8.0;
  static const spaceMd = 12.0;
  static const spaceLg = 16.0;
  static const spaceXl = 24.0;
  static const space2xl = 32.0;
  static const pagePadding = EdgeInsets.all(spaceLg);
  static const panelPadding = EdgeInsets.all(spaceMd);
  static const controlPadding =
      EdgeInsets.symmetric(horizontal: spaceMd, vertical: 10);
  static const fieldPadding = EdgeInsets.all(spaceMd);

  static const touchTarget = 44.0;
  static const controlHeight = 48.0;
  static const settingsRowHeight = 68.0;
  static const compactSettingsRowHeight = 64.0;
  static const controlSize = Size(touchTarget, touchTarget);
  static const iconSize = 22.0;
  static const removeIconSize = 16.0;
  static const toggleWidth = 60.0;
  static const toggleHeight = 36.0;
  static const toggleThumb = 28.0;
  static const compactToggleWidth = 50.0;
  static const compactToggleHeight = 30.0;
  static const compactToggleThumb = 22.0;
  static const toolbarCompactHeight = 64.0;
  static const toolbarHeight = 68.0;
  static const readoutHeight = 52.0;
  static const panelRadius = 10.0;
  static const fieldRadius = 8.0;

  static const minorText = 11.0;
  static const caption = 12.0;
  static const smallLabel = 13.0;
  static const body = 14.0;
  static const controlLabel = 16.0;
  static const rowLabel = 17.0;
  static const sectionTitle = 18.0;
  static const pageTitle = 26.0;
  static const parameterValue = 18.0;
  static const compactParameterValue = 16.0;

  static const phoneBreakpoint = 700.0;
  static const compactToolbarBreakpoint = 800.0;
  static const settingsSidebarBreakpoint = 900.0;
  static const menuConstraints =
      BoxConstraints(minWidth: 224, maxWidth: 320, maxHeight: 400);
  static const selectMenuConstraints =
      BoxConstraints(minWidth: 180, maxWidth: 320, maxHeight: 400);
  static const selectMaxWidth = 260.0;
  static const compactSelectMaxWidth = 174.0;
  static const feedbackDuration = Duration(milliseconds: 40);

  /// Larger touch targets (Appearance setting). Read at build time so
  /// pages listening to prefs apply it live.
  static bool largeTargets = true;
  static TextStyle type(double size,
          {Color? color, FontWeight weight = FontWeight.w400}) =>
      TextStyle(
          fontFamily: font,
          height: 1.15,
          fontSize: size,
          color: color,
          fontWeight: weight);
}
