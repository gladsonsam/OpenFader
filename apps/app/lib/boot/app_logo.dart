// The launch screen uses the same vector mark as the rest of the app.
library;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class AppLogo extends StatelessWidget {
  final double size;

  const AppLogo({super.key, this.size = 96});

  @override
  Widget build(BuildContext context) => SvgPicture.asset(
        'assets/brand/open-fader-meter.svg',
        width: size,
        height: size,
        semanticsLabel: 'OpenFader',
      );
}
