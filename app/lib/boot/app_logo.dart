library;

import 'package:material_ui/material_ui.dart';
import 'package:flutter_svg/flutter_svg.dart';

class AppLogo extends StatelessWidget {
  final double size;

  const AppLogo({super.key, this.size = 96});

  @override
  Widget build(BuildContext context) => SvgPicture.asset(
        'assets/brand/open-fader-logo.svg',
        width: size,
        height: size,
        semanticsLabel: 'OpenFader',
      );
}
