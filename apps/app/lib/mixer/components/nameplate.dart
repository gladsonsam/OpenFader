import '../../palette.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import '../icons.dart';

class MixerNameplate extends StatelessWidget {
  final MixerChannel channel;
  final double iconHeight, bottomInset, nameHeight, nameOffset, fontSize;
  final bool compact;
  final VoidCallback onSelect, onOpen;
  const MixerNameplate(
      {super.key,
      required this.channel,
      required this.iconHeight,
      this.compact = false,
      this.bottomInset = 0,
      this.nameHeight = 36,
      this.nameOffset = 86,
      this.fontSize = 17,
      required this.onSelect,
      required this.onOpen});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      selected: channel.selected,
      label: '${channel.name}, channel ${channel.number}',
      child: GestureDetector(
          onTap: onSelect,
          onDoubleTap: onOpen,
          behavior: HitTestBehavior.opaque,
          child: LayoutBuilder(builder: (context, box) {
            final sx = box.maxWidth / (compact ? 104.2 : 192.2);
            final tape = channel.master
                ? ConsolePalette.of(context).bus
                : ConsolePalette.of(context).tapes[(channel.channel ?? 0) % 8];
            return Stack(clipBehavior: Clip.none, children: [
              if (!channel.master)
                Positioned(
                    left: (compact ? 6 : 3) * sx,
                    top: 2,
                    child: Text(channel.number,
                        style: mixerType(context, (compact ? 15 : 20) * sx,
                            color: ConsolePalette.of(context).mutedText,
                            weight: FontWeight.w600))),
              Positioned(
                  left: (compact ? 24 : 46) * sx,
                  top: iconHeight * (compact ? 8 / 86 : 4 / 122),
                  width: (compact ? 56 : 103) * sx,
                  height: iconHeight * (compact ? 60 / 86 : 98 / 122),
                  child: channel.master
                      ? const MixerMainArtwork()
                      : MixerArtwork('instrument_${channel.artwork}',
                          width: double.infinity, height: double.infinity)),
              Positioned(
                  left: 0,
                  right: 0,
                  top: nameOffset,
                  height: nameHeight,
                  child: ColoredBox(
                      color: tape,
                      child: Center(
                          child: Text(channel.name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: mixerType(context, fontSize,
                                  color: ConsolePalette.of(context).inkOn(tape),
                                  weight: FontWeight.w600))))),
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
                            child: Text('LR',
                                style: mixerType(
                                    context, (compact ? 10 : 14) * sx,
                                    color:
                                        ConsolePalette.of(context).inkOn(tape),
                                    weight: FontWeight.w700))))),
            ]);
          })));
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
