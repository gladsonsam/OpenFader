import 'package:flutter/material.dart';

/// Shared visual language from docs/design/master-fader-dark-mode.html.
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
  static const lime = Color(0xffa3ed19);
  static const selectedLine = Color(0xffeffb00);
  static const meter = Color(0xffb9f060);
  static const amber = Color(0xffe0a400);
  static const mute = Color(0xffd63a2c);
  static const solo = Color(0xffe8d44a);
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
