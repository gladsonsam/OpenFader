// Shell frame: header plus routed content and bank rail.
// The bank rail belongs to the mixer only and is owned by the caller.
library;

import 'package:flutter/material.dart';
import '../palette.dart';

import 'bank_rail.dart';

export 'bank_rail.dart';
export 'header_bar.dart';
export 'scene_button.dart';

class U4Shell extends StatelessWidget {
  final Widget header;
  final Widget child;
  final bool bottomSafeArea;

  /// Bank rail for the mixer only.
  final List<BankRailBank> banks;
  final String activeBank;
  final ValueChanged<String>? onBank;

  /// Meter-bridge overlay (opened from the header meters tile).
  final Widget? overlay;
  final bool overlayOpen;

  /// Width below which the rail bottoms instead of docking right.
  final double narrowBelow;

  const U4Shell({
    super.key,
    required this.header,
    required this.child,
    this.bottomSafeArea = true,
    this.banks = const [],
    this.activeBank = '',
    this.onBank,
    this.overlay,
    this.overlayOpen = false,
    this.narrowBelow = 600,
  });

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      // Modal editors carry their own keyboard insets, so the console keeps its size.
      resizeToAvoidBottomInset: false,
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        bottom: bottomSafeArea,
        child: LayoutBuilder(
          builder: (context, constraints) {
            final narrow = constraints.maxWidth < narrowBelow;
            final showRail = banks.isNotEmpty && onBank != null;
            final rail = showRail
                ? U4BankRail(
                    banks: banks,
                    activeKey: activeBank,
                    onSelect: onBank!,
                    orientation: narrow
                        ? BankRailOrientation.bottom
                        : BankRailOrientation.side,
                  )
                : null;
            final body = narrow || rail == null
                ? Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Expanded(child: child),
                      if (narrow && rail != null) rail,
                    ],
                  )
                : Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Expanded(child: child),
                      rail,
                    ],
                  );
            return Stack(
              children: [
                Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    header,
                    Expanded(child: body),
                  ],
                ),
                if (overlayOpen && overlay != null) overlay!,
              ],
            );
          },
        ),
      ),
    );
  }
}
