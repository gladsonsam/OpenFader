import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';

class MixerNameplate extends StatelessWidget {
  final MixerChannel channel;
  final double iconHeight, bottomInset;
  final bool compact;
  final VoidCallback onSelect, onOpen;
  const MixerNameplate(
      {super.key,
      required this.channel,
      required this.iconHeight,
      this.compact = false,
      this.bottomInset = 0,
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
          child: Column(children: [
            if (iconHeight > 0)
              SizedBox(
                  height: iconHeight + (compact ? 20 : 0),
                  child: Stack(children: [
                    Positioned(
                        left: compact ? 6 : 2,
                        top: compact ? 8 : 1,
                        child:
                            Text(channel.number, style: MixerDesign.type(11))),
                    Center(
                        child: channel.master
                            ? Text(compact ? '' : channel.name,
                                style: MixerDesign.type(compact ? 18 : 23,
                                    weight: FontWeight.w600))
                            : MixerArtwork('instrument_${channel.artwork}',
                                width: iconHeight, height: iconHeight)),
                  ])),
            // Design: the master bar carries the bus colour + an LR tab
            // sitting on its top edge.
            Stack(clipBehavior: Clip.none, children: [
              Container(
                  // Keep a slim footer at the screen edge; the home inset must
                  // not be added to its height and steal fader travel.
                  height: compact ? (bottomInset > 0 ? 32 : 24) : 32,
                  width: double.infinity,
                  alignment: compact ? Alignment.topCenter : Alignment.center,
                  padding:
                      EdgeInsets.only(left: 3, right: 3, top: compact ? 2 : 0),
                  color: channel.master
                      ? MixerDesign.bus
                      : MixerDesign.tapes[(channel.channel ?? 0) % 8],
                  child: Text(
                      channel.master && iconHeight > 0 && !compact
                          ? ''
                          : channel.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: MixerDesign.type(compact ? 12 : 15,
                          color: const Color(0xff161616),
                          weight: FontWeight.w600))),
              if (channel.master)
                Positioned(
                    right: 0,
                    top: compact ? -13 : -18,
                    child: Container(
                        padding: EdgeInsets.symmetric(
                            horizontal: 9,
                            vertical: compact ? 1 : 3),
                        color: MixerDesign.bus,
                        child: Text('LR',
                            style: MixerDesign.type(compact ? 10 : 14,
                                color: const Color(0xff161616),
                                weight: FontWeight.w700)))),
            ]),
          ])));
}
