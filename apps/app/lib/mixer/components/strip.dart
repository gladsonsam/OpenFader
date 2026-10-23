import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import 'fader.dart';
import 'nameplate.dart';
import 'pan.dart';
import 'processing.dart';
import 'toggle.dart';

/// The same strip composition serves every layout. Only density and available
/// space change; controls and callbacks never have a mobile/desktop fork.
class MixerStrip extends StatelessWidget {
  final MixerChannel channel;
  final MixerController controller;
  final VoidCallback onOpen;
  final bool compact;
  final double bottomInset;
  const MixerStrip(
      {super.key,
      required this.channel,
      required this.controller,
      this.compact = false,
      this.bottomInset = 0,
      required this.onOpen});
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final short = box.maxHeight < 540;
        final tiny = box.maxHeight < 330;
        final showRack = controller.view == MixerView.processing &&
            box.maxHeight >= (compact ? 360 : 480);
        final rackHeight = compact ? 56.0 : (short ? 110.0 : 146.0);
        final iconHeight = tiny
            ? 0.0
            : compact || short
                ? 28.0
                : (box.maxHeight > 760 ? 80.0 : 52.0);
        final ch = channel;
        return ColoredBox(
            color: ch.selected ? MixerDesign.selected : MixerDesign.background,
            child: DecoratedBox(
                decoration: const BoxDecoration(
                    border:
                        Border(right: BorderSide(color: Color(0xff101010)))),
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      SizedBox(height: compact || short ? 4 : 12),
                      if (showRack) ...[
                        Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 5),
                            child: SizedBox(
                                height: rackHeight,
                                child: compact
                                    ? MixerEqPreview(
                                        channel: ch,
                                        demo: controller.demo,
                                        compact: true,
                                        onOpen: onOpen)
                                    : MixerProcessingRack(
                                        channel: ch,
                                        demo: controller.demo,
                                        onOpen: onOpen,
                                        onGain: (v) =>
                                            controller.setGain(ch, v)))),
                        const SizedBox(height: 6),
                      ],
                      if (!tiny)
                        Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 5),
                            child: MixerPan(
                                name: ch.name,
                                value: ch.pan,
                                onChanged: ch.master
                                    ? null
                                    : (v) => controller.setPan(ch, v))),
                      const SizedBox(height: 5),
                      Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 5),
                          child: MixerToggle(
                              label: 'Mute',
                              visualHeight: compact ? 36 : 44,
                              fontSize: compact ? 17 : 21,
                              channelName: ch.name,
                              on: ch.muted,
                              activeColor: MixerDesign.mute,
                              onPressed: () => controller.toggleMute(ch))),
                      const SizedBox(height: 8),
                      Expanded(
                          child: Padding(
                              padding:
                                  const EdgeInsets.symmetric(horizontal: 6),
                              child: MixerFader(
                                  channel: ch,
                                  onPosition: (v) =>
                                      controller.setPosition(ch, v),
                                  onSelect: () => controller.select(ch),
                                  onUnity: () => controller.unity(ch)))),
                      const SizedBox(height: 8),
                      Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 5),
                          child: ch.master
                              ? const SizedBox(height: 44)
                              : MixerToggle(
                                  label: 'Solo',
                                  visualHeight: compact ? 30 : 44,
                                  fontSize: compact ? 15 : 21,
                                  channelName: ch.name,
                                  on: ch.solo,
                                  activeColor: MixerDesign.solo,
                                  onPressed: () => controller.toggleSolo(ch))),
                      const SizedBox(height: 4),
                      MixerNameplate(
                          channel: ch,
                          iconHeight: iconHeight,
                          compact: compact,
                          bottomInset: bottomInset,
                          onSelect: () => controller.select(ch),
                          onOpen: onOpen),
                    ])));
      });
}
