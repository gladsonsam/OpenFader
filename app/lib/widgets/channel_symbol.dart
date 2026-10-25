import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../palette.dart';

/// OpenFader artwork keyed by provider-supplied semantics, never wire IDs.
class ChannelSymbol extends StatelessWidget {
  final String? symbol;
  const ChannelSymbol(this.symbol, {super.key});

  @override
  Widget build(BuildContext context) {
    if (symbol == 'none') return const SizedBox.shrink();
    final p = ConsolePalette.of(context);
    final paths = _paths[symbol];
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

  static const _keys =
      '<path d="M10 38h44v17H10zM16 39v15m6-15v15m7-15v15m7-15v15m7-15v15m6-15v15M19 39v7m13-7v7m13-7v7"/>';
  static const _paths = <String, String>{
    'kick':
        '<circle cx="32" cy="31" r="23"/><circle cx="32" cy="31" r="19"/><path d="m16 49-5 10m37-10 5 10"/>',
    'snare':
        '<ellipse cx="32" cy="22" rx="25" ry="7"/><path d="M7 22v22c0 9 50 9 50 0V22M15 26v17m11-14v17m12-17v17m11-20v17M7 43c4-7 46-7 50 0"/>',
    'tom':
        '<ellipse cx="32" cy="13" rx="21" ry="6"/><path d="M11 13v38c0 8 42 8 42 0V13M17 17v8m10-6v7m11-7v7m9-9v8M17 43v10m10-8v10m11-10v10m9-12v10"/>',
    'hihat':
        '<ellipse cx="32" cy="18" rx="24" ry="7"/><path d="M8 25c9 7 39 7 48 0M32 10v46m0-6L19 60m13-10 13 10"/>',
    'cymbals':
        '<ellipse cx="21" cy="21" rx="17" ry="5"/><ellipse cx="44" cy="36" rx="16" ry="5"/><path d="M21 15v38m23-23v28M13 59l8-6 8 6m7 3 8-4 8 4"/>',
    'drumkit':
        '<circle cx="31" cy="41" r="16"/><path d="M8 25h18v12H8zM36 20h18v13H36zM7 15h18m-9-5v34M43 7h17m-8-4v40M18 54l-4 6m30-6 4 6"/>',
    'cowbell': '<path d="M24 10h16l9 46H15zM25 10V5h14v5M18 49h28"/>',
    'bass1':
        '<path d="m32 35 23-29 5 3-23 30c10 9 4 21-7 21-12 0-21-10-15-20 5-8 10 2 17-5zM25 46l5 4M49 11l10 8M53 6l9 7"/><path d="m28 46 28-37"/>',
    'bass2':
        '<path d="m28 33 25-29 6 5-25 29c8-1 13 6 8 15-6 10-19 9-26 1-10-13-1-24 7-18zM23 47l10 7M51 7l10 9M46 12l10 9M27 48 55 8"/>',
    'guitar1':
        '<path d="m30 33 23-28 6 5-24 29c10 9 5 21-7 21-12 0-22-10-15-20 5-7 11-1 17-7zM26 47l29-38"/><circle cx="28" cy="45" r="4"/>',
    'guitar2':
        '<path d="m29 34 24-29 6 5-24 29 12-2-8 20-21 3-7-12 9-19 3 10zM24 49 55 9M21 50l10 7"/>',
    'guitar3':
        '<path d="m29 33 24-28 6 5-24 28 18-3-33 26 2-22-14 3zM28 48 55 9"/>',
    'acoustic1':
        '<path d="m31 31 22-26 6 5-22 26c9 6 8 12 2 14-3 12-15 15-24 8-12-10-8-19 2-23 2-9 8-10 14-4zM24 48 55 9"/><circle cx="28" cy="42" r="4"/>',
    'acoustic2':
        '<path d="m32 29 21-24 6 5-22 26c5 7 9 7 11 6-1 12-14 22-26 18-17-5-17-19-5-24 1-9 6-14 15-7zM26 48 55 9"/><circle cx="29" cy="41" r="4"/>',
    'strings1':
        '<path d="M29 7h6v19c10-3 15 5 7 11 11 7 7 23-10 23S11 44 22 37c-8-6-3-14 7-11zM29 14h6M32 25v28M24 48h16M55 6 45 58"/>',
    'strings2':
        '<path d="M29 6h6v19c12-3 18 7 9 14 12 9 4 19-12 19S8 48 20 39c-9-7-3-17 9-14zM26 11h12M32 23v39M22 47h20M53 8l-7 49"/>',
    'trumpet':
        '<path d="M8 40h34l16-12v29L42 45H16c-13 0-13-18 0-18h25M18 20v18m9-18v18m9-18v18M14 20h8m1 0h8m1 0h8M7 28H3"/>',
    'piano':
        '<path d="M10 55V24C10 9 24 3 34 8c9 4 4 16 13 19 12 4 10 15 7 20M10 37h44M15 55v6m34-6v6"/>$_keys',
    'keyboards':
        '<path d="M10 7h44v20H10zM16 16v11m7-11v11m7-11v11m7-11v11m7-11v11m6-11v11M15 11h8m22 0h5M15 32h34"/>$_keys',
    'synth':
        '<path d="M10 16h44v22H10zM17 22v8m7-8v8m7-8v8m7-8v8m7-8v8m5-8v8"/>$_keys',
    'amp':
        '<rect x="10" y="6" width="44" height="52" rx="3"/><circle cx="32" cy="39" r="13"/><circle cx="17" cy="16" r="2"/><circle cx="27" cy="16" r="2"/><circle cx="37" cy="16" r="2"/><circle cx="47" cy="16" r="2"/>',
    'vocal1':
        '<path d="M19 35V21c0-21 26-21 26 0v14l7 8-14-3M19 35l-7 8 14-3M24 36v7C11 47 8 51 8 59h48c0-8-3-12-16-16v-7"/><ellipse cx="32" cy="24" rx="10" ry="15"/>',
    'vocal2':
        '<path d="M21 17c0-15 22-15 22 0v9c0 7-5 14-11 14s-11-7-11-14zM21 16l7-8 15 9M25 38v7C11 48 8 52 8 59h48c0-7-3-11-17-14v-7"/>',
    'group':
        '<circle cx="32" cy="20" r="10"/><circle cx="12" cy="26" r="7"/><circle cx="52" cy="26" r="7"/><path d="M15 58v-9c0-21 34-21 34 0v9zM14 38C3 38 2 45 2 52h9m39 0h12c0-7-1-14-12-14"/>',
    'phone':
        '<rect x="18" y="4" width="28" height="56" rx="5"/><path d="M21 12h22v36H21zM28 8h8"/><circle cx="32" cy="54" r="2"/>',
    'wedge': '<path d="m8 51 35-35 13 9v30H8zM13 49l30-28 8 6-24 22"/>',
    'wedges':
        '<path d="m3 38 27-27 12 9v13M3 38h15M18 55l27-28 15 10v20H18zM24 51l21-20 10 8-14 12"/>',
    'mixer':
        '<path d="M12 5v13m0 9v32M26 5v29m0 9v16M40 5v6m0 9v39M54 5v36m0 9v9M7 18h10v9H7zM21 34h10v9H21zM35 11h10v9H35zM49 41h10v9H49z"/>',
    'arches':
        '<path d="M6 51a26 26 0 0 1 52 0M13 51a19 19 0 0 1 38 0M20 51a12 12 0 0 1 24 0M27 51a5 5 0 0 1 10 0M5 57h54"/>',
    'clock':
        '<circle cx="32" cy="32" r="25"/><path d="M32 12v20l13 12M32 7v3m0 44v3M7 32h3m44 0h3"/>',
    'pa':
        '<path d="M5 6h24v52H5zM35 6h24v52H35zM11 12h12v8H11zM41 12h12v8H41z"/><circle cx="17" cy="40" r="9"/><circle cx="47" cy="40" r="9"/>',
    'speaker':
        '<rect x="5" y="13" width="54" height="38" rx="2"/><circle cx="25" cy="32" r="13"/><circle cx="25" cy="32" r="6"/><circle cx="49" cy="23" r="3"/><circle cx="49" cy="41" r="3"/>',
    'prohibited': '<circle cx="32" cy="32" r="25"/><path d="M14 50 50 14"/>',
  };
}
