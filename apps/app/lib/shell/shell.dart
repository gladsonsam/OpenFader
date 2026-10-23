// H1 Shell: AppFrame + header + routed content + bank rail. The header
// fills the rest, and the bank rail docks right on wide builds / bottoms
// on phones (LayoutBuilder, same 600 px breakpoint the screens use for
// their grid switch). No Material AppBar anywhere — [U4ShellHeader] only.
//
// The shell never imports engine or transport code: navigation state
// (`mode`, bank, meters overlay) is owned by the caller (the future
// Area B router or `main.dart`).
library;

import 'package:flutter/material.dart';

import '../theme.dart';
import 'bank_rail.dart';
import 'header_bar.dart';

export 'bank_rail.dart';
export 'header_bar.dart';
export 'scene_button.dart';

class U4Shell extends StatelessWidget {
  final U4ShellHeader header;
  final Widget child;

  /// Bank rail (mixer only — the shell owns the condition, like the web).
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
      backgroundColor: U4.page,
      body: SafeArea(
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
