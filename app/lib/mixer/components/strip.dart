import '../../palette.dart';
import '../../design.dart';
import '../../widgets/channel_name_editor.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
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
  final ValueChanged<MixerReadoutControl>? onControlFocus;
  final bool compact;
  final double bottomInset;
  const MixerStrip(
      {super.key,
      required this.channel,
      required this.controller,
      this.onControlFocus,
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
        return ValueListenableBuilder<int>(
            valueListenable: controller.store
                .controlReadyNote(isMaster ? -1 : chIndex ?? -1),
            builder: (context, ready, _) => ColoredBox(
                color: ch.selected
                    ? ConsolePalette.of(context).selected
                    : ConsolePalette.of(context).background,
                child: DecoratedBox(
                    decoration: BoxDecoration(
                        border: Border(
                            right: BorderSide(
                                color:
                                    ConsolePalette.of(context).stripDivider))),
                    child: Stack(children: [
                      place(
                          g.rackY,
                          g.rackH,
                          compact
                              ? MixerEqPreview(
                                  channel: ch, compact: true, onOpen: onOpen)
                              : MixerProcessingRack(
                                  channel: ch,
                                  gainRange: controller.gainRangeOf(ch),
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
                                  valueListenable:
                                      controller.panNoteOf(chIndex),
                                  builder: (context, pan, _) => ready & 4 == 0
                                      ? const Center(child: Text('Pan · —'))
                                      : MixerPan(
                                          name: ch.name,
                                          value: pan,
                                          onTouch: () => onControlFocus
                                              ?.call(MixerReadoutControl.pan),
                                          visualHeight: g.panH,
                                          onChanged: ready & 4 != 0
                                              ? (v) {
                                                  onControlFocus?.call(
                                                      MixerReadoutControl.pan);
                                                  controller.setPan(ch, v);
                                                }
                                              : null)),
                          touch: true),
                      place(
                          g.muteY,
                          g.muteH,
                          ValueListenableBuilder<bool>(
                              valueListenable:
                                  controller.muteNoteOf(chIndex, isMaster),
                              builder: (context, muted, _) => MixerToggle(
                                  label: ready & 2 != 0 ? 'Mute' : 'Mute · —',
                                  visualHeight: g.muteH,
                                  fontSize: g.x(compact ? 20 : 32),
                                  channelName: ch.name,
                                  on: ready & 2 != 0 && muted,
                                  activeColor: ConsolePalette.of(context).mute,
                                  activeForeground:
                                      ConsolePalette.of(context).onDanger,
                                  onPressed: ready & 2 != 0
                                      ? () => controller.toggleMute(ch)
                                      : null)),
                          touch: true),
                      place(
                          g.faderY,
                          g.faderH,
                          ValueListenableBuilder<double>(
                              valueListenable:
                                  controller.faderNoteOf(chIndex, isMaster),
                              builder: (context, fader, _) => Stack(children: [
                                    ExcludeSemantics(
                                        excluding: ready & 1 == 0,
                                        child: IgnorePointer(
                                            ignoring: ready & 1 == 0,
                                            child: Opacity(
                                                opacity: ready & 1 != 0 ? 1 : 0,
                                                child: MixerFader(
                                                    channel:
                                                        _withFader(ch, fader),
                                                    compact: compact,
                                                    onTouch: () =>
                                                        onControlFocus?.call(
                                                            MixerReadoutControl
                                                                .volume),
                                                    onPosition: (v) {
                                                      onControlFocus?.call(
                                                          MixerReadoutControl
                                                              .volume);
                                                      controller.setPosition(
                                                          ch, v);
                                                    },
                                                    onSelect: () {
                                                      onControlFocus?.call(
                                                          MixerReadoutControl
                                                              .volume);
                                                      controller.select(ch);
                                                    },
                                                    onUnity: () {
                                                      onControlFocus?.call(
                                                          MixerReadoutControl
                                                              .volume);
                                                      controller.unity(ch);
                                                    })))),
                                    if (!isMaster && chIndex != null)
                                      Positioned(
                                          left: 0,
                                          right: 0,
                                          bottom: 0,
                                          child: IgnorePointer(
                                              child: ValueListenableBuilder<
                                                  String>(
                                            valueListenable: controller.store
                                                .controlGroupNoteOf(chIndex),
                                            builder: (context, text, _) => Text(
                                                text,
                                                maxLines: 2,
                                                textAlign: TextAlign.center,
                                                overflow: TextOverflow.ellipsis,
                                                style: AppDesign.type(10,
                                                    color: ConsolePalette.of(
                                                            context)
                                                        .text7)),
                                          ))),
                                    if (ready & 1 == 0)
                                      const Center(child: Text('Loading…')),
                                  ]))),
                      place(
                          g.soloY,
                          g.soloH,
                          ValueListenableBuilder<bool>(
                              valueListenable: controller
                                  .soloNoteOf(isMaster ? -1 : chIndex!),
                              builder: (context, solo, _) => MixerToggle(
                                  label: 'Solo',
                                  unavailableReason: ready & 8 != 0
                                      ? null
                                      : 'Solo unavailable',
                                  visualHeight: g.soloH,
                                  fontSize: g.x(compact ? 20 : 32),
                                  channelName: ch.name,
                                  on: ready & 8 != 0 && solo,
                                  activeColor: ConsolePalette.of(context).solo,
                                  activeForeground:
                                      ConsolePalette.of(context).onSolo,
                                  onPressed: ready & 8 != 0
                                      ? () => controller.toggleSolo(ch)
                                      : null)),
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
                              onOpen: onOpen,
                              onRename: isMaster ||
                                      chIndex == null ||
                                      controller.store
                                              .captureNameTarget(chIndex) ==
                                          null
                                  ? null
                                  : () {
                                      final target = controller.store
                                          .captureNameTarget(chIndex);
                                      if (target != null) {
                                        showChannelNameEditor(
                                            context, controller.store, target);
                                      }
                                    })),
                    ]))));
      });

  /// Snapshot copy carrying the live fader value for [MixerFader] (level
  /// label + cap position). Identity/selection/meter stay from the snapshot.
  static MixerChannel _withFader(MixerChannel ch, double fader) => MixerChannel(
      key: ch.key,
      name: ch.name,
      number: ch.number,
      channel: ch.channel,
      artwork: ch.artwork,
      colorChoice: ch.colorChoice,
      iconChoice: ch.iconChoice,
      hasIconChoices: ch.hasIconChoices,
      fader: fader,
      pan: ch.pan,
      muted: ch.muted,
      solo: ch.solo,
      selected: ch.selected,
      master: ch.master,
      meter: ch.meter,
      params: ch.params);
}
