import '../palette.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

/// The channel name stays in its plate, so this draws only the scribble mark.
class MixerMainArtwork extends StatelessWidget {
  const MixerMainArtwork({super.key});
  @override
  Widget build(BuildContext context) => SvgPicture.string(
      '<svg viewBox="0 0 48 48" fill="none" stroke="currentColor" '
      'stroke-width="3" stroke-linecap="round" stroke-linejoin="round">'
      '<path d="M16 40V8m0 0L7 17m9-9l9 9M32 8v32m0 0l-9-9m9 9l9-9"/>'
      '</svg>',
      theme: SvgTheme(currentColor: ConsolePalette.of(context).mutedText));
}
