import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../core/channel_icon.dart';
import '../palette.dart';

/// Renders OpenFader catalog IDs, independent of provider-native values.
class ChannelSymbol extends StatelessWidget {
  final String? symbol;
  const ChannelSymbol(this.symbol, {super.key});
  @override
  Widget build(BuildContext context) {
    if (symbol == 'none') return const SizedBox.shrink();
    final p = ConsolePalette.of(context);
    final paths = openFaderIcon(symbol)?.svgPaths;
    if (paths == null) {
      return Icon(Icons.image_not_supported_outlined, color: p.mutedText);
    }
    return SvgPicture.string(
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" '
      'fill="none" stroke="currentColor" stroke-width="2.5" '
      'stroke-linecap="round" stroke-linejoin="round">$paths</svg>',
      theme: SvgTheme(currentColor: p.mutedText),
    );
  }
}

class ChannelIconArtwork extends StatelessWidget {
  final ResolvedChannelIcon icon;
  const ChannelIconArtwork(this.icon, {super.key});
  @override
  Widget build(BuildContext context) {
    final selection = icon.selection;
    final artwork = switch (selection.kind) {
      ChannelIconKind.none => const SizedBox.shrink(),
      ChannelIconKind.catalog => ChannelSymbol(selection.id),
      ChannelIconKind.imported when icon.imagePath != null => Image.file(
          File(icon.imagePath!),
          fit: BoxFit.contain,
          errorBuilder: (context, error, stack) => Icon(
              Icons.broken_image_outlined,
              color: ConsolePalette.of(context).mutedText)),
      ChannelIconKind.imported => Icon(Icons.broken_image_outlined,
          color: ConsolePalette.of(context).mutedText),
      ChannelIconKind.followProvider => const ChannelSymbol('mixer'),
    };
    return icon.reason == null
        ? artwork
        : Tooltip(message: icon.reason!, child: artwork);
  }
}
