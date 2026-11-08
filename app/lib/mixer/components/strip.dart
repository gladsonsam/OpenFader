import '../../widgets/inline_feedback.dart';
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
class MixerStrip extends StatefulWidget {
  final MixerChannel channel;
  final MixerController controller;
  final VoidCallback onOpen;
  final String? Function()? onOpenProcessing;
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
  State<MixerStrip> createState() => _MixerStripState();
}

class _MixerStripState extends State<MixerStrip> {
  MixerChannel get channel => widget.channel;
  MixerController get controller => widget.controller;
  VoidCallback get onOpen => widget.onOpen;
  ValueChanged<BuildContext>? get onOpenAt => widget.onOpenAt;
  int? get identityChannel => widget.identityChannel;
  ValueChanged<MixerReadoutControl>? get onControlFocus =>
      widget.onControlFocus;
  bool get compact => widget.compact;
  bool? get reserveArtwork => widget.reserveArtwork;
  double get bottomInset => widget.bottomInset;
  final _feedback = InlineFeedbackController();
  @override
  void dispose() {
    _feedback.dispose();
    super.dispose();
  }

  void _openProcessing() {
    final open = widget.onOpenProcessing;
    if (open == null) {
      widget.onOpen();
    } else {
      final error = open();
      if (error != null) _feedback.show(error);
    }
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final g = MixerGeometry(
            phone: compact, width: box.maxWidth, height: box.maxHeight);
        final layout = PrefsService.instance.stripLayout;
        final ch = channel;
        final hasArtwork = ch.showsArtwork;
        var showArtwork =
            (reserveArtwork ?? hasArtwork) && box.maxHeight >= 480;
        var identityHeight = showArtwork
            ? AppDesign.spaceXl + math.min(64.0, box.maxWidth * .55)
            : AppDesign.spaceXl;
        const nameHeight = AppDesign.touchTarget;
        const gap = AppDesign.spaceSm;
        var compactRack =
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
        final visible = [...layout.visibleOrder];
        // Budget the fader first in logical pixels, never as a fraction of the
        // authored portrait mock. Its cell includes the cap and level readout,
        // so a 240-point cell leaves useful travel on short landscape screens.
        const minimumFaderHeight = 240.0;
        final hasFader = visible.contains(StripElement.fader);
        double fixedHeight() =>
            visible.fold<double>(0, (sum, el) => sum + (heights[el] ?? 0));
        double rowSpacing() =>
            gap * math.max(0, visible.length - 1) +
            AppDesign.spaceSm +
            (visible.lastOrNull == StripElement.nameplate
                ? 0.0
                : AppDesign.spaceSm);
        bool faderNeedsSpace() =>
            hasFader &&
            box.maxHeight - fixedHeight() - rowSpacing() < minimumFaderHeight;
        // Processing stays available from the channel screen. Prefer a small
        // EQ preview, then remove the preview entirely before reducing travel.
        if (faderNeedsSpace() && visible.contains(StripElement.processing)) {
          compactRack = true;
          heights[StripElement.processing] = AppDesign.touchTarget;
        }
        if (faderNeedsSpace() && showArtwork) {
          showArtwork = false;
          identityHeight = AppDesign.spaceXl;
          heights[StripElement.nameplate] = identityHeight + nameHeight;
        }
        if (faderNeedsSpace()) visible.remove(StripElement.processing);
        // Pan is secondary to level control on the shortest surfaces; it is
        // still accessible by opening the channel. Mute and Solo stay at hand.
        if (faderNeedsSpace()) visible.remove(StripElement.pan);
        final fixed = fixedHeight();
        final spacing = rowSpacing();
        final contentHeight = math.max(box.maxHeight,
            fixed + spacing + (hasFader ? minimumFaderHeight : 0));
        final positions = <StripElement, Rect>{};
        var y = AppDesign.spaceSm;
        for (final element in visible) {
          final height = element == StripElement.fader
              ? contentHeight - fixed - spacing
              : heights[element] ?? 0;
          positions[element] = Rect.fromLTWH(0, y, box.maxWidth, height);
          y += height + gap;
        }
        double allocated(StripElement element) =>
            positions[element]?.height ?? 0;
        Widget place(StripElement element, Widget child) {
          if (!visible.contains(element)) return const SizedBox.shrink();
          final rect = positions[element];
          if (rect == null) return const SizedBox.shrink();
          // Rows already reserve their full touch targets. Expanding them
          // independently here can cover an adjacent fader's outline.
          final inset = element == StripElement.processing
              ? AppDesign.spaceXs / 2
              : AppDesign.spaceXs;
          return Positioned(
              left: inset,
              right: inset,
              top: rect.top,
              height: rect.height,
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
                          compactRack
                              ? MixerEqPreview(
                                  channel: ch,
                                  compact: true,
                                  showPhantom: controller.hasPhantomOf(ch),
                                  onOpen: _openProcessing)
                              : MixerProcessingRack(
                                  channel: ch,
                                  gainRange: controller.gainRangeOf(ch),
                                  showPhantom: controller.hasPhantomOf(ch),
                                  demo: controller.demo,
                                  onOpen: _openProcessing,
                                  onGain: (v) => controller.setGain(ch, v))),
                      place(
                        StripElement.pan,
                        isMaster || chIndex == null
                            ? MixerPan(
                                name: ch.name,
                                value: 0,
                                visualHeight: allocated(StripElement.pan),
                                onChanged: null)
                            : ValueListenableBuilder<double>(
                                valueListenable: controller.panNoteOf(chIndex),
                                builder: (context, pan, _) => MixerPan(
                                    name: ch.name,
                                    value: pan,
                                    onTouch: () => onControlFocus
                                        ?.call(MixerReadoutControl.pan),
                                    visualHeight: allocated(StripElement.pan),
                                    onChanged: ready & 4 != 0
                                        ? (v) {
                                            onControlFocus
                                                ?.call(MixerReadoutControl.pan);
                                            controller.setPan(ch, v);
                                          }
                                        : null)),
                      ),
                      place(
                        StripElement.mute,
                        ValueListenableBuilder<bool>(
                            valueListenable:
                                controller.muteNoteOf(chIndex, isMaster),
                            builder: (context, muted, _) => MixerToggle(
                                label: ready & 2 != 0 ? 'Mute' : 'Mute · -',
                                visualHeight: allocated(StripElement.mute),
                                fontSize: g.x(compact ? 20 : 32),
                                channelName: ch.name,
                                on: ready & 2 != 0 && muted,
                                activeColor: ConsolePalette.of(context).mute,
                                activeForeground:
                                    ConsolePalette.of(context).onDanger,
                                onPressed: ready & 2 != 0
                                    ? () => controller.toggleMute(ch)
                                    : null)),
                      ),
                      place(
                          StripElement.fader,
                          ListenableBuilder(
                              listenable: Listenable.merge([
                                controller.faderNoteOf(chIndex, isMaster),
                                controller.muteNoteOf(chIndex, isMaster),
                              ]),
                              builder: (context, _) {
                                final fader = controller
                                    .faderNoteOf(chIndex, isMaster)
                                    .value;
                                return Stack(children: [
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
                                                  builder: (context, group, _) =>
                                                      Tooltip(
                                                          message: group,
                                                          child: MixerFader(
                                                              channel: _withFader(
                                                                  ch, fader),
                                                              compact: compact,
                                                              showMeter: layout
                                                                  .showMeter,
                                                              onTouch: () =>
                                                                  onControlFocus
                                                                      ?.call(MixerReadoutControl.volume),
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
                                                              onUnity: controller.canUnity(ch)
                                                                  ? () {
                                                                      onControlFocus
                                                                          ?.call(
                                                                              MixerReadoutControl.volume);
                                                                      controller
                                                                          .unity(
                                                                              ch);
                                                                    }
                                                                  : null)))))),
                                  if (ready & 1 == 0)
                                    Center(
                                        child: Text('-',
                                            style: AppDesign.type(14,
                                                color:
                                                    ConsolePalette.of(context)
                                                        .mutedText))),
                                ]);
                              })),
                      place(
                        StripElement.solo,
                        ValueListenableBuilder<bool>(
                            valueListenable:
                                controller.soloNoteOf(isMaster ? -1 : chIndex!),
                            builder: (context, solo, _) => MixerToggle(
                                label: 'Solo',
                                unavailableReason:
                                    ready & 8 != 0 ? null : 'Solo unavailable',
                                visualHeight: allocated(StripElement.solo),
                                fontSize: g.x(compact ? 20 : 32),
                                channelName: ch.name,
                                on: ready & 8 != 0 && solo,
                                activeColor: ConsolePalette.of(context).solo,
                                activeForeground:
                                    ConsolePalette.of(context).onSolo,
                                onPressed: ready & 8 != 0
                                    ? () => controller.toggleSolo(ch)
                                    : null)),
                      ),
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
                                onRename: !isMaster &&
                                        ((identityChannel ?? chIndex) == null ||
                                            controller.store.captureNameTarget(
                                                    identityChannel ??
                                                        chIndex) ==
                                                null)
                                    ? null
                                    : (anchor, bounds) {
                                        final index =
                                            identityChannel ?? chIndex;
                                        final target = index == null
                                            ? null
                                            : controller.store
                                                .captureNameTarget(index);
                                        if (target != null) {
                                          showChannelNameEditor(
                                              anchor, controller.store, target,
                                              anchorBounds: bounds);
                                        } else {
                                          _feedback.show(
                                              'This output has no channel identity controls on this connection.');
                                        }
                                      })),
                    ]))));
        // Feedback scrolls after the strip instead of taking fader travel.
        return ValueListenableBuilder<String?>(
            valueListenable: _feedback,
            child: content,
            builder: (context, message, content) =>
                contentHeight > box.maxHeight || message != null
                    ? SingleChildScrollView(
                        child: Column(children: [
                        SizedBox(height: contentHeight, child: content),
                        if (message != null)
                          Padding(
                              padding: const EdgeInsets.all(AppDesign.spaceXs),
                              child: Column(children: [
                                Semantics(
                                    liveRegion: true,
                                    child: Text(message,
                                        style: TextStyle(
                                            color: ConsolePalette.of(context)
                                                .text1,
                                            fontSize: 12))),
                                TextButton(
                                    onPressed: _feedback.clear,
                                    child: const Text('Dismiss')),
                              ])),
                      ]))
                    : content!);
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
      icon: ch.icon,
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
