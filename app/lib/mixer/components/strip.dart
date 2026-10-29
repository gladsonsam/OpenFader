import '../../palette.dart';
import '../../design.dart';
import '../../settings/prefs.dart';
import '../../widgets/channel_name_editor.dart';
import 'package:flutter/foundation.dart';
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
  final int? identityChannel;
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
      required this.onOpen,
      this.onOpenProcessing,
      this.identityChannel});
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final g = MixerGeometry(
            phone: compact, width: box.maxWidth, height: box.maxHeight);
        final layout = PrefsService.instance.stripLayout;
        final baseline = listEquals(layout.order, StripLayout.defaultOrder) &&
            StripLayout.defaultOrder.every(layout.visible.contains) &&
            layout.faderWeight == StripLayout.defaultFaderWeight &&
            (!layout.compactProcessing || compact);
        final heights = <StripElement, double>{
          StripElement.processing: layout.compactProcessing ? g.y(62) : g.rackH,
          StripElement.pan: g.panH,
          StripElement.mute: g.muteH,
          StripElement.solo: g.soloH,
          StripElement.nameplate: box.maxHeight - g.scribbleY,
        };
        final positions = <StripElement, Rect>{};
        if (!baseline) {
          final visible = layout.visibleOrder;
          final gap = g.y(compact ? 8 : 10);
          final top = g.rackY;
          final available = (box.maxHeight - top - gap * (visible.length - 1)).clamp(0.0, box.maxHeight);
          final fixed = visible.fold<double>(0, (sum, element) => sum + (heights[element] ?? 0));
          final densityScale = layout.faderWeight > 3
              ? 1 - (layout.faderWeight - 3) * .08
              : 1.0;
          final fitScale = fixed > available ? available / fixed : 1.0;
          final scale = densityScale < fitScale ? densityScale : fitScale;
          final remaining = (available - fixed * scale).clamp(0.0, available);
          final faderHeight = remaining * (layout.faderWeight < 3
              ? .7 + .15 * (layout.faderWeight - 1)
              : 1.0);
          var y = top;
          for (final element in visible) {
            final height = element == StripElement.fader ? faderHeight : (heights[element] ?? 0) * scale;
            positions[element] = Rect.fromLTWH(0, y, box.maxWidth, height);
            y += height + gap;
          }
        }
        final nameplateScale = baseline
            ? 1.0
            : ((positions[StripElement.nameplate]?.height ?? 0) /
                    (box.maxHeight - g.scribbleY))
                .clamp(0.0, 1.0);
        Widget place(StripElement element, double y, double height, Widget child,
            {bool touch = false}) {
          if (!layout.visible.contains(element)) return const SizedBox.shrink();
          if (!baseline) {
            final rect = positions[element];
            if (rect == null) return const SizedBox.shrink();
            y = rect.top;
            height = rect.height;
          }
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
                          compact || layout.compactProcessing
                              ? MixerEqPreview(
                                  channel: ch, compact: true,
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
                                  visualHeight: g.panH,
                                  onChanged: null)
                              : ValueListenableBuilder<double>(
                                  valueListenable:
                                      controller.panNoteOf(chIndex),
                                  builder: (context, pan, _) => MixerPan(
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
                          StripElement.mute,
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
                                                child: MixerFader(
                                                    channel:
                                                        _withFader(ch, fader),
                                                    compact: compact,
                                                    showMeter: layout.showMeter,
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
                                      Center(
                                          child: Text('—',
                                              style: AppDesign.type(14,
                                                  color: ConsolePalette.of(context)
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
                      if (layout.visible.contains(StripElement.nameplate)) Positioned(
                          left: 0,
                          right: 0,
                          top: baseline ? g.scribbleY : positions[StripElement.nameplate]?.top ?? g.scribbleY,
                          height: baseline ? box.maxHeight - g.scribbleY : positions[StripElement.nameplate]?.height ?? 0,
                          child: MixerNameplate(
                              channel: ch,
                              iconHeight: g.scribbleH * nameplateScale,
                              nameHeight: g.nameH * nameplateScale,
                              nameOffset: (g.nameY - g.scribbleY) * nameplateScale,
                              fontSize: g.nameText,
                              compact: compact,
                              onSelect: () => controller.select(ch),
                              onOpen: onOpen,
                              onRename: (identityChannel ?? chIndex) == null ||
                                      controller.store.captureNameTarget(identityChannel ?? chIndex) == null
                                  ? null
                                  : () {
                                      final target = controller.store
                                          .captureNameTarget(identityChannel ?? chIndex);
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
      params: ch.params,
      eqPreview: ch.eqPreview);
}
