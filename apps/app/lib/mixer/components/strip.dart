import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import '../geometry.dart';
import 'fader.dart';
import 'nameplate.dart';
import 'pan.dart';
import 'processing.dart';
import 'toggle.dart';

/// The same strip composition serves every layout. Only density and available
/// space change; controls and callbacks never have a mobile/desktop fork.
///
/// Live control state (fader/mute/pan/solo) arrives via the controller's
/// per-strip notifiers: each control rebuilds from its own
/// [ValueListenableBuilder] so a fader drag wakes one strip, never the
/// shell or sibling strips. The [MixerChannel] snapshot still carries
/// structural identity (key/name/artwork/selection/meter/params).
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
        final g = MixerGeometry(
            phone: compact, width: box.maxWidth, height: box.maxHeight);
        Widget place(double y, double height, Widget child,
            {bool touch = false}) {
          final target = touch ? g.target(height) : height;
          return Positioned(
              left: g.left,
              right: g.right,
              top: y - (target - height) / 2,
              height: target,
              child: child);
        }

        final ch = channel;
        final isMaster = ch.master;
        final chIndex = ch.channel;
        return ColoredBox(
            color: ch.selected ? MixerDesign.selected : MixerDesign.background,
            child: DecoratedBox(
                decoration: const BoxDecoration(
                    border:
                        Border(right: BorderSide(color: Color(0xff101010)))),
                child: Stack(children: [
                  place(
                      g.rackY,
                      g.rackH,
                      compact
                          ? MixerEqPreview(
                              channel: ch,
                              demo: controller.demo,
                              compact: true,
                              onOpen: onOpen)
                          : MixerProcessingRack(
                              channel: ch,
                              demo: controller.demo,
                              onOpen: onOpen,
                              onGain: (v) => controller.setGain(ch, v))),
                  place(
                      g.panY,
                      g.panH,
                      isMaster || chIndex == null
                          ? MixerPan(
                              name: ch.name,
                              value: 0,
                              visualHeight: g.panH,
                              onChanged: null)
                          : ValueListenableBuilder<double>(
                              valueListenable: controller.panNoteOf(chIndex),
                              builder: (context, pan, _) => MixerPan(
                                  name: ch.name,
                                  value: pan,
                                  visualHeight: g.panH,
                                  onChanged: (v) => controller.setPan(ch, v))),
                      touch: true),
                  place(
                      g.muteY,
                      g.muteH,
                      ValueListenableBuilder<bool>(
                          valueListenable:
                              controller.muteNoteOf(chIndex, isMaster),
                          builder: (context, muted, _) => MixerToggle(
                              label: 'Mute',
                              visualHeight: g.muteH,
                              fontSize: g.x(compact ? 20 : 32),
                              channelName: ch.name,
                              on: muted,
                              activeColor: MixerDesign.mute,
                              onPressed: () => controller.toggleMute(ch))),
                      touch: true),
                  place(
                      g.faderY,
                      g.faderH,
                      ValueListenableBuilder<double>(
                          valueListenable:
                              controller.faderNoteOf(chIndex, isMaster),
                          builder: (context, fader, _) => MixerFader(
                              channel: _withFader(ch, fader),
                              compact: compact,
                              onPosition: (v) => controller.setPosition(ch, v),
                              onSelect: () => controller.select(ch),
                              onUnity: () => controller.unity(ch)))),
                  place(
                      g.soloY,
                      g.soloH,
                      isMaster || chIndex == null
                          ? MixerToggle(
                              label: 'Solo',
                              channelName: ch.name,
                              on: false,
                              visualHeight: g.soloH,
                              fontSize: g.x(compact ? 20 : 32),
                              activeColor: MixerDesign.solo,
                              onPressed: null)
                          : ValueListenableBuilder<bool>(
                              valueListenable: controller.soloNoteOf(chIndex),
                              builder: (context, solo, _) => MixerToggle(
                                  label: 'Solo',
                                  visualHeight: g.soloH,
                                  fontSize: g.x(compact ? 20 : 32),
                                  channelName: ch.name,
                                  on: solo,
                                  activeColor: MixerDesign.solo,
                                  onPressed: () => controller.toggleSolo(ch))),
                      touch: true),
                  Positioned(
                      left: 0,
                      right: 0,
                      top: g.scribbleY,
                      bottom: 0,
                      child: MixerNameplate(
                          channel: ch,
                          iconHeight: g.scribbleH,
                          nameHeight: g.nameH,
                          nameOffset: g.nameY - g.scribbleY,
                          fontSize: g.nameText,
                          compact: compact,
                          onSelect: () => controller.select(ch),
                          onOpen: onOpen)),
                ])));
      });

  /// Snapshot copy carrying the live fader value for [MixerFader] (level
  /// label + cap position). Identity/selection/meter stay from the snapshot.
  static MixerChannel _withFader(MixerChannel ch, double fader) => MixerChannel(
      key: ch.key,
      name: ch.name,
      number: ch.number,
      channel: ch.channel,
      artwork: ch.artwork,
      fader: fader,
      pan: ch.pan,
      muted: ch.muted,
      solo: ch.solo,
      selected: ch.selected,
      master: ch.master,
      meter: ch.meter,
      params: ch.params);
}
