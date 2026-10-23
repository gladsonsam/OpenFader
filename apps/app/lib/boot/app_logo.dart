// OpenMix launch mark: four mini channel faders in the console palette.
// Pure widgets (no image asset), so it paints on the very first frame
// with zero asset-load cost and works on every platform.
library;

import 'package:flutter/material.dart';

import '../theme.dart';

class AppLogo extends StatelessWidget {
  final double size;

  const AppLogo({super.key, this.size = 96});

  /// Cap position per fader, as a fraction from the top of the track.
  static const _caps = <double>[0.22, 0.48, 0.34, 0.62];

  @override
  Widget build(BuildContext context) {
    final trackW = size * 0.045;
    final trackH = size * 0.56;
    final capW = size * 0.115;
    final capH = size * 0.055;
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        color: U4.well,
        border: Border.all(color: U4.line),
        borderRadius: BorderRadius.circular(size * 0.24),
      ),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          for (final frac in _caps)
            Padding(
              padding: EdgeInsets.symmetric(horizontal: size * 0.045),
              child: SizedBox(
                width: capW,
                height: trackH,
                child: Stack(
                  alignment: Alignment.topCenter,
                  children: [
                    Positioned.fill(
                      child: Align(
                        child: Container(
                          width: trackW,
                          height: trackH,
                          decoration: BoxDecoration(
                            color: U4.control,
                            borderRadius: BorderRadius.circular(trackW / 2),
                          ),
                        ),
                      ),
                    ),
                    Positioned(
                      top: (trackH - capH) * frac,
                      child: Container(
                        width: capW,
                        height: capH,
                        decoration: BoxDecoration(
                          color: U4.cyan,
                          borderRadius: BorderRadius.circular(capH / 2),
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
  }
}
