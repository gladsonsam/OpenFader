import 'package:flutter/material.dart';
import '../../palette.dart';
import '../../design.dart';
import 'package:flutter_svg/flutter_svg.dart';

/// Paths use a 24-unit grid (viewBox 0 0 24 24).
enum DesignSymbol {
  menu,
  routing,
  layout,
  settings,
  help,
  refresh,
  discover,
  add,
  close
}

class DesignIcon extends StatelessWidget {
  final DesignSymbol symbol;
  final double size;
  final Color? color;
  const DesignIcon(this.symbol,
      {super.key, this.size = AppDesign.iconSize, this.color});

  @override
  Widget build(BuildContext context) {
    final body = switch (symbol) {
      DesignSymbol.menu => '<path d="M4 6h16M4 12h16M4 18h16"/>',
      DesignSymbol.routing =>
        '<path d="M3 6h4c6 0 4 12 10 12h4M3 18h4c6 0 4-12 10-12h4m-3-3 3 3-3 3m0 6 3 3-3 3"/>',
      DesignSymbol.layout =>
        '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 9v12"/>',
      DesignSymbol.settings =>
        '<path d="M18.56 9.28L18.93 10.45L20.87 10.45L20.87 13.55L18.93 13.55L18.56 14.72L17.99 15.80L19.36 17.18L17.18 19.36L15.80 17.99L14.72 18.56L13.55 18.93L13.55 20.87L10.45 20.87L10.45 18.93L9.28 18.56L8.20 17.99L6.82 19.36L4.64 17.18L6.01 15.80L5.44 14.72L5.07 13.55L3.13 13.55L3.13 10.45L5.07 10.45L5.44 9.28L6.01 8.20L4.64 6.82L6.82 4.64L8.20 6.01L9.28 5.44L10.45 5.07L10.45 3.13L13.55 3.13L13.55 5.07L14.72 5.44L15.80 6.01L17.18 4.64L19.36 6.82L17.99 8.20Z"/><circle cx="12" cy="12" r="3"/>',
      DesignSymbol.help =>
        '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 4 2c-1 .6-1.5 1-1.5 2M12 17h.01"/>',
      DesignSymbol.refresh =>
        '<path d="M20 7v5h-5M4 17v-5h5M6 7a7 7 0 0 1 12-1l2 3M4 15l2 3a7 7 0 0 0 12-1"/>',
      DesignSymbol.discover =>
        '<path d="M4 8V4h4M16 4h4v4M20 16v4h-4M8 20H4v-4M7 12h10m-7-3 3 3-3 3"/>',
      DesignSymbol.add => '<path d="M12 5v14M5 12h14"/>',
      DesignSymbol.close => '<path d="m6 6 12 12M18 6 6 18"/>',
    };
    return SvgPicture.string(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">$body</svg>',
      width: size,
      height: size,
      colorFilter: ColorFilter.mode(
          color ?? ConsolePalette.of(context).text7, BlendMode.srcIn),
    );
  }
}
