import 'dart:math' as math;

import 'package:flutter/semantics.dart';
import '../../palette.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import '../icons.dart';
import '../../widgets/channel_symbol.dart';
import '../../core/channel_icon.dart';

class MixerNameplate extends StatelessWidget {
  final MixerChannel channel;
  final double iconHeight, bottomInset, nameHeight, nameOffset, fontSize;
  final bool compact;
  final bool showArtwork;
  final VoidCallback onSelect, onOpen;
  final void Function(BuildContext, Rect)? onRename;
  final ValueChanged<BuildContext>? onOpenAt;
  const MixerNameplate(
      {super.key,
      required this.channel,
      required this.iconHeight,
      this.compact = false,
      this.showArtwork = true,
      this.bottomInset = 0,
      this.nameHeight = 36,
      this.nameOffset = 86,
      this.fontSize = 17,
      required this.onSelect,
      required this.onOpen,
      this.onRename,
      this.onOpenAt});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      selected: channel.selected,
      customSemanticsActions: onRename == null
          ? null
          : {
              const CustomSemanticsAction(label: 'Channel settings'): () =>
                  _rename(context),
            },
      label: '${channel.name}, channel ${channel.number}',
      child: GestureDetector(
          onTap: () {
            onSelect();
            if (onOpenAt != null) {
              onOpenAt!(context);
            } else if (onRename != null) {
              _rename(context);
            } else {
              onOpen();
            }
          },
          onLongPress: onRename == null ? null : () => _rename(context),
          behavior: HitTestBehavior.opaque,
          child: LayoutBuilder(builder: (context, box) {
            final sx = box.maxWidth / (compact ? 104.2 : 192.2);
            final tape = channel.master && channel.colorChoice == null
                ? ConsolePalette.of(context).bus
                : channel.colorChoice != null
                    ? channel.colorChoice!.rgb == null
                        ? ConsolePalette.of(context).panel
                        : Color(0xff000000 | channel.colorChoice!.rgb!)
                    : ConsolePalette.of(context)
                        .tapes[(channel.channel ?? 0) % 8];
            // Center smaller artwork above the tape, with symmetric clearance
            // for the identity row (or the master tab).
            final identityHeight = math.min(iconHeight, nameOffset);
            final numberHeight =
                math.min(identityHeight, (compact ? 20 : 26) * sx);
            final tabHeight = channel.master ? (compact ? 18 : 26) * sx : 0.0;
            final artworkPadding =
                channel.master ? tabHeight : numberHeight + 2 * sx;
            final artworkSize = math.min((compact ? 48 : 86) * sx,
                math.max(0.0, identityHeight - artworkPadding));
            return Stack(clipBehavior: Clip.none, children: [
              if (!channel.master)
                Positioned(
                    left: (compact ? 10 : 12) * sx,
                    right: (compact ? 10 : 12) * sx,
                    top: 0,
                    height: numberHeight,
                    child: Align(
                        alignment: Alignment.centerLeft,
                        child: FittedBox(
                            fit: BoxFit.scaleDown,
                            alignment: Alignment.centerLeft,
                            child: Text(channel.number,
                                maxLines: 1,
                                style: mixerType(
                                    context, (compact ? 15 : 20) * sx,
                                    color: ConsolePalette.of(context).mutedText,
                                    weight: FontWeight.w600))))),
              if (showArtwork)
                Positioned(
                    left: 0,
                    right: 0,
                    top: artworkPadding,
                    height: math.max(0, identityHeight - artworkPadding),
                    child: Center(
                        child: SizedBox.square(
                            dimension: artworkSize,
                            child: ClipRect(
                                child: channel.master &&
                                        channel.icon.selection.kind ==
                                            ChannelIconKind.followProvider
                                    ? const MixerMainArtwork()
                                    : ChannelIconArtwork(channel.icon))))),
              Positioned(
                  left: 0,
                  right: 0,
                  top: nameOffset,
                  height: nameHeight,
                  child: ColoredBox(
                      color: tape,
                      child: Padding(
                          padding: EdgeInsets.only(bottom: bottomInset),
                          child: Center(
                              child: Text(channel.name,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: mixerType(context, fontSize,
                                      color: ConsolePalette.of(context)
                                          .inkOn(tape),
                                      weight: FontWeight.w600)))))),
              if (channel.master)
                Positioned(
                    right: 0,
                    top: nameOffset - (compact ? 18 : 26) * sx,
                    child: ClipPath(
                        clipper: _BusTabClipper(),
                        child: Container(
                            color: tape,
                            padding: EdgeInsets.fromLTRB(18 * sx, 0, 9 * sx, 0),
                            height: (compact ? 18 : 26) * sx,
                            alignment: Alignment.center,
                            child: Text(channel.number,
                                style: mixerType(
                                    context, (compact ? 10 : 14) * sx,
                                    color:
                                        ConsolePalette.of(context).inkOn(tape),
                                    weight: FontWeight.w700))))),
            ]);
          })));

  void _rename(BuildContext context) {
    final box = context.findRenderObject() as RenderBox?;
    if (box == null) return;
    onRename?.call(
        context, Rect.fromLTWH(0, nameOffset, box.size.width, nameHeight));
  }
}

class _BusTabClipper extends CustomClipper<Path> {
  @override
  Path getClip(Size size) => Path()
    ..moveTo(size.height * .78, 0)
    ..lineTo(size.width, 0)
    ..lineTo(size.width, size.height)
    ..lineTo(0, size.height)
    ..close();
  @override
  bool shouldReclip(_BusTabClipper oldClipper) => false;
}
