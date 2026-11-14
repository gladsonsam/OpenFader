import 'package:material_ui/material_ui.dart';

/// Immutable semantic colors for one app tree. Resolve in build and pass the
/// snapshot into painters; never read a process-wide mutable palette in paint.
@immutable
class ConsolePalette extends ThemeExtension<ConsolePalette> {
  final Brightness brightness;
  final String accent;
  const ConsolePalette(
      {this.brightness = Brightness.dark, this.accent = 'lime'});
  static ConsolePalette of(BuildContext context) =>
      Theme.of(context).extension<ConsolePalette>() ?? const ConsolePalette();
  bool get isLight => brightness == Brightness.light;
  Color pair(int dark, int light) => Color(isLight ? light : dark);

  Color get arrangementLane => pair(0xff171717, 0xffe9edf0);
  Color get arrangementLaneAlt => pair(0xff191919, 0xffe4e9ed);
  Color get arrangementTrack => pair(0xff242424, 0xffbfc8d0);
  Color get arrangementTrackButton => const Color(0xff303030);
  Color get arrangementTrackButtonBorder => const Color(0xff404040);
  Color get arrangementGrid => pair(0xff1f1f1f, 0xffc5ced6);
  Color get arrangementGold => const Color(0xffe0b75f);
  Color get arrangementBlue => const Color(0xff7fb7c9);
  Color get arrangementOrange => const Color(0xffe08f5f);
  Color get arrangementPurple => const Color(0xffc79ae0);
  Color get arrangementGrey => const Color(0xffb5b5b5);
  Color get recorderBar => pair(0xff202020, 0xffcbd2d8);
  Color get recorderSurface => pair(0xff181818, 0xffbfc8d0);
  Color get recorderControl => pair(0xff2a2a2a, 0xffcbd2d8);
  Color get recorderBorder => pair(0xff3a3a3a, 0xff8895a0);
  Color get recorderClock => pair(0xff0b0b0b, 0xffe9edf0);
  Color get recorderOlive => const Color(0xff262b14);
  Color get recorderOliveBorder => const Color(0xff5d6b1f);
  Color get recorderMuted => pair(0xffb5b5b5, 0xff354350);
  Color get background => pair(0xff1c1c1c, 0xffd7dce1);
  Color get selected => pair(0xff232323, 0xffcbd2d8);
  Color get well => pair(0xff080808, 0xffe9edf0);
  Color get panel => pair(0xff0f0f0f, 0xffe4e9ed);
  Color get control => pair(0xff343434, 0xffbfc8d0);
  Color get processingPanel => pair(0xff242424, 0xffbfc8d0);
  Color get navigationTile => pair(0xff181818, 0xffbfc8d0);
  Color get processingSelected =>
      !isLight && accent == 'lime' ? const Color(0xff262b14) : selectionFill;
  Color get stripDivider => pair(0xff101010, 0xff9aa6b1);
  Color get line => pair(0xff2c2c2c, 0xff9aa6b1);
  Color get text => pair(0xffebebeb, 0xff131c24);
  Color get mutedText => pair(0xffa0a0a0, 0xff354350);
  Color get lime => switch (accent) {
        'ocean' => pair(0xff6bc9ff, 0xff00669a),
        'violet' => pair(0xffc5a4ff, 0xff7040b5),
        'amber' => pair(0xffffce66, 0xff855800),
        _ => pair(0xffd6f13f, 0xff536400),
      };
  Color get onAccent =>
      isLight ? const Color(0xffffffff) : const Color(0xff101510);
  Color get selectionFill => lime.withValues(alpha: isLight ? .12 : .09);
  Color get selectedLine => lime;
  // Physical controls and signal bars keep their console colors in both modes.
  // Only the surrounding UI surfaces and readable text adapt to brightness.
  Color get meter => const Color(0xffa8e05f);
  Color get amber => pair(0xffe0a400, 0xff8a5d00);
  Color get mute => pair(0xffd63a2c, 0xffb52520);
  Color get solo => const Color(0xfff5c542);
  Color get onDanger => const Color(0xffffffff);
  Color get onSolo => const Color(0xff272000);
  Color get onTape => const Color(0xff080808);
  Color get onBus => const Color(0xff061f31);
  Color get bus => pair(0xff4aa8ff, 0xff1372b4);
  Color get over => pair(0xff3d1006, 0xfff7d8d5);
  Color get amberBack => pair(0xff3a2600, 0xfff1e2bc);
  Color get greenBack => pair(0xff1a2a00, 0xffdbe9cc);
  Color get slot => pair(0xff161616, 0xffbac2ca);
  Color get slotBorder => pair(0xff2a2a2a, 0xff8895a0);
  Color get cap => const Color(0xff3a3a3a);
  Color get knob => const Color(0xffd8d8d4);
  Color get panShape => pair(0xff4a4a4a, 0xffacb6bf);
  Color get chip => pair(0xff262626, 0xffcbd2d8);
  Color get rtaOutline => pair(0xffa9ae98, 0xff66784c);
  Color get eqPreviewFill => pair(0xff8d9378, 0xff66784c);
  Color get dynamicsFill => success.withValues(alpha: .18);
  Color get eqPreviewCurve => pair(0xfff6f9e4, 0xff304022);
  Color get eqBg => pair(0xff1f2314, 0xffe9efdf);
  Color get comp => pair(0xffff4b06, 0xffbb3900);
  Color get phantom => pair(0xffef5548, 0xffb62820);
  Color get processingBlue => pair(0xff59b0c4, 0xff207083);
  Color get processingPurple => pair(0xffa285d6, 0xff74529f);
  Color get errorContainer => pair(0xff3b1414, 0xffffe4df);
  Color get onErrorContainer => pair(0xffffb4a9, 0xff8e201a);
  Color get success => pair(0xffa8e05f, 0xff376b13);
  Color get successContainer => pair(0xff1a2a12, 0xffdfefd1);
  Color get onSuccessContainer => pair(0xffbce996, 0xff2c570e);
  Color get warningContainer => amberBack;
  Color get onWarningContainer => pair(0xffffce66, 0xff725000);
  Color get overlay => text.withValues(alpha: .08);
  Color get panKnob => const Color(0xffd8d8d4);
  Color get panGrip => const Color(0xff8e8e8a);
  Color get capBorder => const Color(0xff555555);
  Color get faderSlot => const Color(0xff161616);
  Color get faderSlotBorder => const Color(0xff2a2a2a);
  Color get faderAccent => ConsolePalette(accent: accent).lime;
  Color get meterClipBack => const Color(0xff3d1006);
  Color get meterAmberBack => const Color(0xff3a2600);
  Color get meterGreenBack => const Color(0xff1a2a00);
  Color get meterReduction => const Color(0xffff4b06);
  Color get capHighlight => const Color(0xbfffffff);
  Color get capGrip => const Color(0xff151515);
  Color get capGripLight => const Color(0xff585858);
  Color inkOn(Color fill) => fill.computeLuminance() > .179
      ? const Color(0xff080808)
      : const Color(0xffebebeb);
  Color get shadow => const Color(0x66000000);
  Color bandColor(int band) => switch (band) {
        0 => pair(0xffa8e05f, 0xff507d12),
        1 => pair(0xff59b0c4, 0xff995000),
        2 => pair(0xff6f8be0, 0xff7c6400),
        3 => pair(0xffa285d6, 0xff00758a),
        _ => pair(0xffc779c0, 0xff7545b7),
      };
  List<Color> get tapes => const [
        Color(0xffcc7e5b),
        Color(0xffcea94f),
        Color(0xffb5b556),
        Color(0xff6baf83),
        Color(0xff65aaa4),
        Color(0xff739dc7),
        Color(0xffa08ac7),
        Color(0xffc185a1)
      ];

  // Compatibility role names: all remain resolved from this same snapshot.
  Color get page => background;
  Color get headerBg => background;
  Color get lineHeader => line;
  Color get linePanel => line;
  Color get lineCtl => pair(0xff484848, 0xffa4afb9);
  Color get railBorder => line;
  Color get divider => line;
  Color get bankOff => panel;
  Color get tileBg => control;
  Color get tileBorder => line;
  Color get text1 => text;
  Color get text4 => pair(0xffc8c8c8, 0xff293743);
  Color get text7 => mutedText;
  Color get text10 => pair(0xff808080, 0xff475662);
  Color get cyan => lime;
  Color get masterLine => selectedLine;
  Color get meterGreen => meter;
  Color get meterAmber => const Color(0xffe0a400);
  Color get meterRed => const Color(0xffd63a2c);
  Color get meterCover => meterGreenBack;
  Color get peakLine => text;
  Color get clipOn => meterRed;
  Color get clipOff => meterClipBack;
  Color get muteOffBg => control;
  Color get muteOffTx => text;
  Color get muteOffBd => line;
  Color get muteOnBg => mute;
  Color get muteOnBd => mute;
  Color get railBg => well;
  Color get railRing => line;
  Color get scaleTick => mutedText;
  Color get panMarker => text;
  Color get panNotch => lineCtl;
  Color get panFill => mutedText;

  @override
  ConsolePalette copyWith({Brightness? brightness, String? accent}) =>
      ConsolePalette(
          brightness: brightness ?? this.brightness,
          accent: accent ?? this.accent);
  @override
  ConsolePalette lerp(covariant ConsolePalette? other, double t) =>
      other == null || t < .5 ? this : other;
  @override
  bool operator ==(Object other) =>
      other is ConsolePalette &&
      other.brightness == brightness &&
      other.accent == accent;
  @override
  int get hashCode => Object.hash(brightness, accent);
}
