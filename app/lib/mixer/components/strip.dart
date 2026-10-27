import '../../palette.dart';
import '../../design.dart';
import '../../settings/prefs.dart';
import '../../widgets/channel_name_editor.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import '../geometry.dart';
import '../strip_layout.dart';
import 'fader.dart';
import 'nameplate.dart';
import 'pan.dart';
import 'processing.dart';
import 'toggle.dart';

/// The same strip composition serves every layout. Only density and available
/// space change; controls and callbacks never have a mobile/desktop fork.
///
/// The strip is a flow [Column] over [StripLayout.visibleOrder]: fixed-height
/// rows for processing/pan/mute/solo/nameplate and one [Expanded] fader cell
/// (flex from [StripLayout.faderWeight]) fusing fader + meter.
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

  double get _processingH => compact ? 62 : 202;
  double get _panH => compact ? 38 : 58;
  double get _muteH => compact ? 40 : 57;
  double get _soloH => compact ? 36 : 57;
  double get _nameH => compact ? 36 : 51;
  double get _iconH => compact ? 86 : 122;

  @override
  Widget build(BuildContext context) {
    final layout = PrefsService.instance.stripLayout;
    final ch = channel;
    final isMaster = ch.master;
    final chIndex = ch.channel;
    final showCompactProcessing = compact || layout.compactProcessing;
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
                child: Padding(
                    padding: EdgeInsets.only(
                        left: compact ? 5 : 8,
                        right: compact ? 3 : 6,
                        top: compact ? 6 : 12,
                        bottom: bottomInset),
                    child: Column(children: [
                      for (final el in layout.visibleOrder)
                        switch (el) {
                          StripElement.processing => SizedBox(
                              height: _processingH,
                              child: showCompactProcessing
                                  ? MixerEqPreview(
                                      channel: ch,
                                      compact: true,
                                      onOpen: onOpen)
                                  : MixerProcessingRack(
                                      channel: ch,
                                      gainRange: controller.gainRangeOf(ch),
                                      demo: controller.demo,
                                      onOpen: onOpen,
                                      onGain: (v) =>
                                          controller.setGain(ch, v))),
                          StripElement.pan => SizedBox(
                              height: MixerGeometry.target(_panH),
                              child: Center(
                                  child: SizedBox(
                                      height: _panH,
                                      child: isMaster || chIndex == null
                                          ? MixerPan(
                                              name: ch.name,
                                              value: 0,
                                              visualHeight: _panH,
                                              onChanged: null)
                                          : ValueListenableBuilder<double>(
                                              valueListenable: controller
                                                  .panNoteOf(chIndex),
                                              builder: (context, pan, _) =>
                                                  ready & 4 == 0
                                                      ? const Center(
                                                          child:
                                                              Text('Pan · —'))
                                                      : MixerPan(
                                                          name: ch.name,
                                                          value: pan,
                                                          onTouch: () =>
                                                              onControlFocus?.call(
                                                                  MixerReadoutControl
                                                                      .pan),
                                                          visualHeight: _panH,
                                                          onChanged: ready & 4 !=
                                                                  0
                                                              ? (v) {
                                                                  onControlFocus?.call(
                                                                      MixerReadoutControl
                                                                          .pan);
                                                                  controller
                                                                      .setPan(
                                                                          ch,
                                                                          v);
                                                                }
                                                              : null))))),
                          StripElement.mute => SizedBox(
                              height: MixerGeometry.target(_muteH),
                              child: Center(
                                  child: SizedBox(
                                      height: _muteH,
                                      child: ValueListenableBuilder<bool>(
                                          valueListenable:
                                              controller.muteNoteOf(
                                                  chIndex, isMaster),
                                          builder: (context, muted, _) =>
                                              MixerToggle(
                                                  label: ready & 2 != 0
                                                      ? 'Mute'
                                                      : 'Mute · —',
                                                  visualHeight: _muteH,
                                                  fontSize:
                                                      compact ? 13 : 15,
                                                  channelName: ch.name,
                                                  on: ready & 2 != 0 && muted,
                                                  activeColor:
                                                      ConsolePalette.of(context)
                                                          .mute,
                                                  activeForeground:
                                                      ConsolePalette.of(context)
                                                          .onDanger,
                                                  onPressed: ready & 2 != 0
                                                      ? () => controller
                                                          .toggleMute(ch)
                                                      : null))))),
                          StripElement.fader => Expanded(
                              flex: (layout.faderWeight * 10).round(),
                              child: ValueListenableBuilder<double>(
                                  valueListenable: controller.faderNoteOf(
                                      chIndex, isMaster),
                                  builder: (context, fader, _) => Stack(
                                          children: [
                                        ExcludeSemantics(
                                            excluding: ready & 1 == 0,
                                            child: IgnorePointer(
                                                ignoring: ready & 1 == 0,
                                                child: Opacity(
                                                    opacity:
                                                        ready & 1 != 0 ? 1 : 0,
                                                    child: MixerFader(
                                                        channel: _withFader(
                                                            ch, fader),
                                                        compact: compact,
                                                        showMeter:
                                                            layout.showMeter,
                                                        onTouch: () =>
                                                            onControlFocus?.call(
                                                                MixerReadoutControl
                                                                    .volume),
                                                        onPosition: (v) {
                                                          onControlFocus?.call(
                                                              MixerReadoutControl
                                                                  .volume);
                                                          controller
                                                              .setPosition(
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
                                                valueListenable:
                                                    controller.store
                                                        .controlGroupNoteOf(
                                                            chIndex),
                                                builder: (context, text, _) =>
                                                    Text(
                                                  text,
                                                  maxLines: 2,
                                                  textAlign: TextAlign.center,
                                                  overflow:
                                                      TextOverflow.ellipsis,
                                                  style: AppDesign.type(10,
                                                      color: ConsolePalette.of(
                                                              context)
                                                          .text7),
                                                ),
                                              ))),
                                        if (ready & 1 == 0)
                                          const Center(
                                              child: Text('Loading…')),
                                      ]))),
                          StripElement.meter => const SizedBox.shrink(),
                          StripElement.solo => SizedBox(
                              height: MixerGeometry.target(_soloH),
                              child: Center(
                                  child: SizedBox(
                                      height: _soloH,
                                      child: ValueListenableBuilder<bool>(
                                          valueListenable: controller
                                              .soloNoteOf(
                                                  isMaster ? -1 : chIndex!),
                                          builder: (context, solo, _) =>
                                              MixerToggle(
                                                  label: 'Solo',
                                                  unavailableReason:
                                                      ready & 8 != 0
                                                          ? null
                                                          : 'Solo unavailable',
                                                  visualHeight: _soloH,
                                                  fontSize:
                                                      compact ? 13 : 15,
                                                  channelName: ch.name,
                                                  on: ready & 8 != 0 && solo,
                                                  activeColor:
                                                      ConsolePalette.of(context)
                                                          .solo,
                                                  activeForeground:
                                                      ConsolePalette.of(context)
                                                          .onSolo,
                                                  onPressed: ready & 8 != 0
                                                      ? () => controller
                                                          .toggleSolo(ch)
                                                      : null))))),
                          StripElement.nameplate => SizedBox(
                              height: _iconH + _nameH,
                              child: MixerNameplate(
                                  channel: ch,
                                  iconHeight: _iconH,
                                  nameHeight: _nameH,
                                  nameOffset: _iconH,
                                  fontSize: compact ? 12 : 14,
                                  compact: compact,
                                  onSelect: () => controller.select(ch),
                                  onOpen: onOpen,
                                  onRename: isMaster ||
                                          chIndex == null ||
                                          controller.store.captureNameTarget(
                                                  chIndex) ==
                                              null
                                      ? null
                                      : () {
                                          final target = controller.store
                                              .captureNameTarget(chIndex);
                                          if (target != null) {
                                            showChannelNameEditor(context,
                                                controller.store, target);
                                          }
                                        })),
                        },
                    ])))));
  }

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
