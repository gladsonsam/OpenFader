import 'package:flutter/material.dart';

/// Shared visual language from the master-fader dark-mode design mock.
/// Layouts choose sizes; every screen shares colour, typography and shape.
abstract final class AppDesign {
  static const background = Color(0xff1c1c1c);
  static const selected = Color(0xff232323);
  static const well = Color(0xff080808);
  static const panel = Color(0xff0f0f0f);
  static const control = Color(0xff343434);
  static const line = Color(0xff2c2c2c);
  static const text = Color(0xffebebeb);
  static const mutedText = Color(0xffa0a0a0);
  static const lime = Color(0xffd6f13f);
  static const selectedLine = Color(0xffd6f13f);
  static const meter = Color(0xffa8e05f);
  static const amber = Color(0xffe0a400);
  static const mute = Color(0xffd63a2c);
  static const solo = Color(0xfff5c542);
  // Design-system accents shared by mixer + channel screens.
  static const bus = Color(0xff4aa8ff);
  static const over = Color(0xff3d1006);
  static const amberBack = Color(0xff3a2600);
  static const greenBack = Color(0xff1a2a00);
  static const slot = Color(0xff161616);
  static const slotBorder = Color(0xff2a2a2a);
  static const cap = Color(0xff3a3a3a);
  static const knob = Color(0xffd8d8d4);
  static const panShape = Color(0xff4a4a4a);
  static const chip = Color(0xff262626);
  static const eqBg = Color(0xff1f2314);
  static const comp = Color(0xffff4b06);
  static const phantom = Color(0xffef5548);
  static const tapes = [
    Color(0xffcc7e5b),
    Color(0xffcea94f),
    Color(0xffb5b556),
    Color(0xff6baf83),
    Color(0xff65aaa4),
    Color(0xff739dc7),
    Color(0xffa08ac7),
    Color(0xffc185a1)
  ];
  static const radius = BorderRadius.all(Radius.circular(10));
  static const font = 'Barlow';
  static TextStyle type(double size,
          {Color color = text, FontWeight weight = FontWeight.w400}) =>
      TextStyle(
          fontFamily: font,
          height: 1.15,
          fontSize: size,
          color: color,
          fontWeight: weight);
}
