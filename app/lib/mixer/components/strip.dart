import '../../widgets/inline_feedback.dart';
import '../../settings/prefs.dart';
import '../../widgets/channel_name_editor.dart';
import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import '../../ui/console.dart';
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
        final shortSurface = box.maxHeight < 800;
        final gap = shortSurface ? AppDesign.spaceXs : AppDesign.spaceSm;
        var processingMode = layout.compactProcessing
            ? MixerProcessingMode.eq
            : MixerProcessingMode.full;
        final fullRackHeight =
            (g.rackH * (3 / layout.faderWeight).clamp(.6, 1.5))
                .clamp(MixerProcessingRack.fullMinHeight, 120.0);
        final heights = <StripElement, double>{
          StripElement.processing: layout.compactProcessing
              ? AppDesign.touchTarget
              : fullRackHeight,
          StripElement.pan: AppDesign.touchTarget,
          StripElement.mute: AppDesign.touchTarget,
          StripElement.solo: AppDesign.touchTarget,
          StripElement.nameplate:
              AppDesign.space2xl + AppDesign.spaceXl + bottomInset,
        };
        final visible = [...layout.visibleOrder];
        // The cap needs room at both ends of the rail. Keep a usable well
        // while still allowing the compact dynamics rack at short heights.
        final minimumFaderHeight = math.max(200.0, box.maxHeight * .45);
        double fixedHeight() =>
            visible.fold<double>(0, (sum, el) => sum + (heights[el] ?? 0));
        // Pan and the toggles already have clear space inside their
        // touch rows. Only add a gap between rows whose paint fills the box.
        bool hasPaintInset(StripElement element) =>
            element == StripElement.pan ||
            element == StripElement.mute ||
            element == StripElement.solo;
        double gapBefore(int index) {
          if (hasPaintInset(visible[index])) return 0;
          if (index == 0) return AppDesign.spaceSm;
          return hasPaintInset(visible[index - 1]) ? 0 : gap;
        }

        double rowSpacing() {
          final last = visible.last;
          final bottom = last == StripElement.nameplate || hasPaintInset(last)
              ? 0.0
              : AppDesign.spaceSm;
          return visible.indexed.fold<double>(
              bottom, (sum, row) => sum + gapBefore(row.$1));
        }

        bool faderNeedsSpace([double reserved = 0]) =>
            box.maxHeight - fixedHeight() - rowSpacing() - reserved <
            minimumFaderHeight;
        if (visible.contains(StripElement.processing)) {
          if (faderNeedsSpace() && processingMode == MixerProcessingMode.full) {
            processingMode = MixerProcessingMode.dynamics;
            heights[StripElement.processing] =
                MixerProcessingRack.dynamicsMinHeight;
          }
          if (faderNeedsSpace()) {
            processingMode = MixerProcessingMode.eq;
            heights[StripElement.processing] = AppDesign.touchTarget;
          }
          if (faderNeedsSpace()) visible.remove(StripElement.processing);
        }
        // Pan is secondary to level control on the shortest surfaces; it is
        // still accessible by opening the channel. Mute and Solo stay at hand.
        if (faderNeedsSpace()) {
          visible.remove(StripElement.pan);
        }
        const roomyNameHeight = AppDesign.space2xl + AppDesign.spaceSm;
        const roomyIdentityHeight = AppDesign.touchTarget;
        const largerIdentityHeight = AppDesign.controlHeight + AppDesign.spaceXs;
        const largerNameHeight = AppDesign.controlHeight;
        const extraToggleHeight =
            AppDesign.controlHeight - AppDesign.touchTarget;
        final extraHeight = roomyIdentityHeight - AppDesign.space2xl +
            roomyNameHeight - AppDesign.spaceXl +
            (visible.contains(StripElement.mute) ? extraToggleHeight : 0) +
            (visible.contains(StripElement.solo) ? extraToggleHeight : 0);
        final roomy = box.maxWidth >= AppDesign.touchTarget * 2 &&
            !faderNeedsSpace(extraHeight);
        final largerIdentity = roomy &&
            !faderNeedsSpace(extraHeight +
                largerIdentityHeight - roomyIdentityHeight +
                largerNameHeight - roomyNameHeight);
        final (identityHeight, tapeHeight, nameFontSize) = largerIdentity
            ? (largerIdentityHeight, largerNameHeight, AppDesign.dialogTitle)
            : roomy
                ? (roomyIdentityHeight, roomyNameHeight, AppDesign.sectionTitle)
                : (
                    AppDesign.space2xl,
                    AppDesign.spaceXl,
                    g.nameText.clamp(AppDesign.caption, AppDesign.sectionTitle)
                  );
        // Tape grows by the system bottom inset so names clear the nav bar.
        final nameHeight = tapeHeight + bottomInset;
        if (roomy) {
          heights[StripElement.nameplate] = identityHeight + nameHeight;
          heights[StripElement.mute] = AppDesign.controlHeight;
          heights[StripElement.solo] = AppDesign.controlHeight;
        }
        final toggleHeight = roomy
            ? AppDesign.actionLargeHeight
            : shortSurface
                ? AppDesign.actionDenseHeight
                : AppDesign.actionCompactHeight;
        final panIndex = visible.indexOf(StripElement.pan);
        // Extend Pan's drag area into the meter lane above it. A tap in
        // that lane still opens processing. EQ and the toggles stay separate.
        final panOverhang = panIndex > 0 &&
                visible[panIndex - 1] == StripElement.processing &&
                processingMode != MixerProcessingMode.eq
            ? AppDesign.touchTarget - AppDesign.spaceXl - AppDesign.spaceXs
            : 0.0;
        if (panOverhang > 0) {
          heights[StripElement.processing] =
              heights[StripElement.processing]! + panOverhang;
          heights[StripElement.pan] = AppDesign.touchTarget - panOverhang;
        }
        final fixed = fixedHeight();
        final spacing = rowSpacing();
        final contentHeight = math.max(box.maxHeight,
            fixed + spacing + minimumFaderHeight);
        final positions = <StripElement, Rect>{};
        var y = 0.0;
        for (final (index, element) in visible.indexed) {
          y += gapBefore(index);
          final height = element == StripElement.fader
              ? contentHeight - fixed - spacing
              : heights[element] ?? 0;
          positions[element] = Rect.fromLTWH(0, y, box.maxWidth, height);
          y += height;
        }
        Widget place(StripElement element, Widget child,
            {double leadingHitInset = 0}) {
          // Hidden rows must stay positioned children. A plain zero-size
          // child would be a non-positioned Stack child, and the Stack would
          // then size itself to that child whenever its width is loose (the
          // edit item and the strip's own scroll fallback both do that),
          // collapsing the whole body to zero width.
          if (!visible.contains(element)) {
            return const Positioned.fill(child: SizedBox.shrink());
          }
          final rect = positions[element];
          if (rect == null) {
            return const Positioned.fill(child: SizedBox.shrink());
          }
          const inset = AppDesign.spaceXs;
          return Positioned(
              left: inset,
              right: inset,
              top: rect.top - leadingHitInset,
              height: rect.height + leadingHitInset,
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
                          MixerProcessingCell(
                              channel: ch,
                              controller: controller,
                              mode: processingMode,
                              roomy: roomy,
                              dynamicsExtraHeight: panOverhang,
                              onOpen: _openProcessing)),
                      place(
                          StripElement.pan,
                          MixerPanCell(
                              channel: ch,
                              controller: controller,
                              ready: ready,
                              leadingHitInset: panOverhang,
                              onMeterTap:
                                  panOverhang > 0 ? _openProcessing : null,
                              onControlFocus: onControlFocus),
                          leadingHitInset: panOverhang),
                      place(
                          StripElement.mute,
                          MixerToggleCell(
                              channel: ch,
                              controller: controller,
                              kind: MixerToggleKind.mute,
                              ready: ready,
                              visualHeight: toggleHeight)),
                      place(
                          StripElement.fader,
                          MixerFaderCell(
                              channel: ch,
                              controller: controller,
                              ready: ready,
                              compact: compact,
                              showMeter: layout.showMeter,
                              onControlFocus: onControlFocus)),
                      place(
                          StripElement.solo,
                          MixerToggleCell(
                              channel: ch,
                              controller: controller,
                              kind: MixerToggleKind.solo,
                              ready: ready,
                              visualHeight: toggleHeight)),
                      if (visible.contains(StripElement.nameplate))
                        Positioned(
                            left: 0,
                            right: 0,
                            top: positions[StripElement.nameplate]?.top ?? 0,
                            height:
                                positions[StripElement.nameplate]?.height ?? 0,
                            child: MixerNameplate(
                                channel: ch,
                                showArtwork: ch.showsArtwork,
                                iconHeight: identityHeight,
                                nameHeight: nameHeight,
                                bottomInset: bottomInset,
                                nameOffset: identityHeight,
                                fontSize: nameFontSize,
                                allowNameWrap: roomy,
                                compact: compact,
                                onSelect: () => controller.select(ch),
                                onOpen: onOpen,
                                onOpenAt: onOpenAt,
                                onRename: _nameplateRename(isMaster, chIndex))),
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
                                                .text,
                                            fontSize: AppDesign.caption))),
                                ConsoleButton(
                                    label: 'Dismiss',
                                    variant: ConsoleButtonVariant.quiet,
                                    onPressed: _feedback.clear),
                              ])),
                      ]))
                    : content!);
      });

  /// Rename is offered only where this connection exposes a name target.
  /// Outputs without one stay tappable but never open an editor they cannot
  /// commit to.
  void Function(BuildContext, Rect)? _nameplateRename(
      bool isMaster, int? chIndex) {
    if (isMaster) return null;
    final index = identityChannel ?? chIndex;
    final target =
        index == null ? null : controller.store.captureNameTarget(index);
    if (target == null) return null;
    return (anchor, bounds) {
      final current =
          index == null ? null : controller.store.captureNameTarget(index);
      if (current != null) {
        showChannelNameEditor(anchor, controller.store, current,
            anchorBounds: bounds);
      } else {
        _feedback.show(
            'This output has no channel identity controls on this connection.');
      }
    };
  }
}
