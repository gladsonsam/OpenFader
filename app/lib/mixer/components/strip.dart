import '../../palette.dart';
import '../../design.dart';
import '../../settings/prefs.dart';
import '../../widgets/channel_name_editor.dart';
import 'dart:math' as math;
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
/// Live control state (fader/mute/pan/solo) arrives via the controller's
/// per-strip notifiers: each control rebuilds from its own
/// [ValueListenableBuilder] so a fader drag wakes one strip, never the
/// shell or sibling strips. The [MixerChannel] snapshot still carries
/// structural identity (key/name/artwork/selection/meter/params).
class MixerStrip extends StatelessWidget {
  final MixerChannel channel;
  final MixerController controller;
  final VoidCallback onOpen;
  final VoidCallback? onOpenProcessing;
  final ValueChanged<BuildContext>? onOpenAt;
  final int? identityChannel;
  final ValueChanged<MixerReadoutControl>? onControlFocus;
  final bool compact;

  /// A bank supplies one value for aligned rows; standalone strips use their
  /// own artwork metadata.
  final bool? reserveArtwork;
  final double bottomInset;
  const MixerStrip(
      {super.key,
      required this.channel,
      required this.controller,
      this.reserveArtwork,
      this.onControlFocus,
      this.compact = false,
      this.bottomInset = 0,
      required this.onOpen,
      this.onOpenProcessing,
      this.onOpenAt,
      this.identityChannel});
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final g = MixerGeometry(
            phone: compact, width: box.maxWidth, height: box.maxHeight);
        final layout = PrefsService.instance.stripLayout;
        final ch = channel;
        final hasArtwork = ch.master ||
            (controller.demo && !ch.hasIconChoices) ||
            (ch.iconChoice != null &&
                ch.iconChoice!.symbol.isNotEmpty &&
                ch.iconChoice!.symbol != 'none');
        final showArtwork =
            (reserveArtwork ?? hasArtwork) && box.maxHeight >= 480;
        final identityHeight = showArtwork
            ? AppDesign.spaceXl + math.min(64.0, box.maxWidth * .55)
            : AppDesign.spaceXl;
        const nameHeight = AppDesign.touchTarget;
        const gap = AppDesign.spaceSm;
        final compactRack =
            compact || layout.compactProcessing || box.maxHeight < 520;
        final heights = <StripElement, double>{
          StripElement.processing: compactRack
              ? AppDesign.touchTarget
              : (g.rackH * (3 / layout.faderWeight).clamp(.6, 1.5))
                  .clamp(96.0, 144.0),
          StripElement.pan: AppDesign.touchTarget,
          StripElement.mute: AppDesign.controlHeight,
          StripElement.solo: AppDesign.controlHeight,
          StripElement.nameplate: identityHeight + nameHeight,
        };
        final visible = layout.visibleOrder;
        final fixed =
            visible.fold<double>(0, (sum, el) => sum + (heights[el] ?? 0));
        final spacing =
            gap * math.max(0, visible.length - 1) + AppDesign.spaceSm * 2;
        final contentHeight = math.max(
            box.maxHeight,
            fixed +
                spacing +
                (visible.contains(StripElement.fader)
                    ? AppDesign.touchTarget * 2
                    : 0));
        final positions = <StripElement, Rect>{};
        var y = AppDesign.spaceSm;
        for (final element in visible) {
          final height = element == StripElement.fader
              ? contentHeight - fixed - spacing
              : heights[element] ?? 0;
          positions[element] = Rect.fromLTWH(0, y, box.maxWidth, height);
          y += height + gap;
        }
        double allocated(StripElement element, double fallback) =>
            positions[element]?.height ?? 0;
        Widget place(
            StripElement element, double y, double height, Widget child,
            {bool touch = false}) {
          if (!layout.visible.contains(element)) return const SizedBox.shrink();
          final rect = positions[element];
          if (rect == null) return const SizedBox.shrink();
          y = rect.top;
          height = rect.height;
          final target = touch ? g.target(height) : height;
          return Positioned(
              left: AppDesign.spaceXs,
              right: AppDesign.spaceXs,
              top: y - (target - height) / 2,
              height: target,
              child: child);
        }

        final isMaster = ch.master;
        final chIndex = ch.channel;
        final content = ValueListenableBuilder<int>(
            valueListenable: controller.store
                .controlReadyNote(isMaster ? -1 : chIndex ?? -1),
            builder: (context, ready, _) => ColoredBox(
                color: ConsolePalette.of(context).background,
                child: DecoratedBox(
                    decoration: BoxDecoration(
                        border: Border(
                            right: BorderSide(
                                color:
                                    ConsolePalette.of(context).stripDivider))),
                    child: Stack(children: [
                      place(
                          StripElement.processing,
                          g.rackY,
                          g.rackH,
                          compactRack
                              ? MixerEqPreview(
                                  channel: ch,
                                  compact: true,
                                  showPhantom: controller.hasPhantomOf(ch),
                                  onOpen: onOpenProcessing ?? onOpen)
                              : MixerProcessingRack(
                                  channel: ch,
                                  gainRange: controller.gainRangeOf(ch),
                                  showPhantom: controller.hasPhantomOf(ch),
                                  demo: controller.demo,
                                  onOpen: onOpenProcessing ?? onOpen,
                                  onGain: (v) => controller.setGain(ch, v))),
                      place(
                          StripElement.pan,
                          g.panY,
                          g.panH,
                          isMaster || chIndex == null
                              ? MixerPan(
                                  name: ch.name,
                                  value: 0,
                                  visualHeight:
                                      allocated(StripElement.pan, g.panH),
                                  onChanged: null)
                              : ValueListenableBuilder<double>(
                                  valueListenable:
                                      controller.panNoteOf(chIndex),
                                  builder: (context, pan, _) => MixerPan(
                                      name: ch.name,
                                      value: pan,
                                      onTouch: () => onControlFocus
                                          ?.call(MixerReadoutControl.pan),
                                      visualHeight:
                                          allocated(StripElement.pan, g.panH),
                                      onChanged: ready & 4 != 0
                                          ? (v) {
                                              onControlFocus?.call(
                                                  MixerReadoutControl.pan);
                                              controller.setPan(ch, v);
                                            }
                                          : null)),
                          touch: true),
                      place(
                          StripElement.mute,
                          g.muteY,
                          g.muteH,
                          ValueListenableBuilder<bool>(
                              valueListenable:
                                  controller.muteNoteOf(chIndex, isMaster),
                              builder: (context, muted, _) => MixerToggle(
                                  label: ready & 2 != 0 ? 'Mute' : 'Mute · —',
                                  visualHeight:
                                      allocated(StripElement.mute, g.muteH),
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
                          StripElement.fader,
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
                                                child: ValueListenableBuilder<
                                                        String>(
                                                    valueListenable: controller
                                                        .store
                                                        .controlGroupNoteOf(
                                                            chIndex ?? -1),
                                                    builder: (context, group, _) => Tooltip(
                                                        message: group,
                                                        child: MixerFader(
                                                            channel: _withFader(ch, fader),
                                                            compact: compact,
                                                            showMeter: layout.showMeter,
                                                            onTouch: () => onControlFocus?.call(MixerReadoutControl.volume),
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
                                                              controller
                                                                  .select(ch);
                                                            },
                                                            onUnity: () {
                                                              onControlFocus?.call(
                                                                  MixerReadoutControl
                                                                      .volume);
                                                              controller
                                                                  .unity(ch);
                                                            })))))),
                                    if (ready & 1 == 0)
                                      Center(
                                          child: Text('—',
                                              style: AppDesign.type(14,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .mutedText))),
                                  ]))),
                      place(
                          StripElement.solo,
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
                                  visualHeight:
                                      allocated(StripElement.solo, g.soloH),
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
                      if (layout.visible.contains(StripElement.nameplate))
                        Positioned(
                            left: 0,
                            right: 0,
                            top: positions[StripElement.nameplate]?.top ?? 0,
                            height:
                                positions[StripElement.nameplate]?.height ?? 0,
                            child: MixerNameplate(
                                channel: ch,
                                showArtwork: showArtwork && hasArtwork,
                                iconHeight: identityHeight,
                                nameHeight: nameHeight,
                                nameOffset: identityHeight,
                                fontSize: g.nameText.clamp(12.0, 18.0),
                                compact: compact,
                                onSelect: () => controller.select(ch),
                                onOpen: onOpen,
                                onOpenAt: onOpenAt,
                                onRename: (identityChannel ?? chIndex) ==
                                            null ||
                                        controller.store.captureNameTarget(
                                                identityChannel ?? chIndex) ==
                                            null
                                    ? null
                                    : () {
                                        final target = controller.store
                                            .captureNameTarget(
                                                identityChannel ?? chIndex);
                                        if (target != null) {
                                          showChannelNameEditor(context,
                                              controller.store, target);
                                        }
                                      })),
                    ]))));
        return contentHeight > box.maxHeight
            ? SingleChildScrollView(
                child: SizedBox(height: contentHeight, child: content))
            : content;
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
      params: ch.params,
      eqPreview: ch.eqPreview);
}
