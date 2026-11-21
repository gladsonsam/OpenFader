import '../../palette.dart';
import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';
import '../controller.dart';
import '../design.dart';
import '../../ui/motion.dart';
import '../../design.dart';

/// Which live state a [MixerToggleCell] binds to. Both kinds share the same
/// readiness gate, touch target and semantic toggle contract.
enum MixerToggleKind { mute, solo }

/// Strip-level mute or solo row. The cell owns the live notifier so a tap
/// rebuilds one strip, not the shell or its siblings.
class MixerToggleCell extends StatelessWidget {
  final MixerChannel channel;
  final MixerController controller;
  final MixerToggleKind kind;
  final int ready;
  final double visualHeight;
  const MixerToggleCell(
      {super.key,
      required this.channel,
      required this.controller,
      required this.kind,
      required this.ready,
      required this.visualHeight});

  @override
  Widget build(BuildContext context) {
    final ch = channel;
    final isMaster = ch.master;
    final chIndex = ch.channel;
    switch (kind) {
      case MixerToggleKind.mute:
        return ValueListenableBuilder<bool>(
            valueListenable: controller.muteNoteOf(chIndex, isMaster),
            builder: (context, muted, _) => MixerToggle(
                label: 'Mute',
                unavailableReason:
                    ready & 2 != 0 ? null : 'Mute unavailable',
                visualHeight: visualHeight,
                fontSize: AppDesign.body,
                channelName: ch.name,
                on: ready & 2 != 0 && muted,
                activeColor: ConsolePalette.of(context).mute,
                activeForeground: ConsolePalette.of(context).onDanger,
                onPressed:
                    ready & 2 != 0 ? () => controller.toggleMute(ch) : null));
      case MixerToggleKind.solo:
        return ValueListenableBuilder<bool>(
            valueListenable:
                controller.soloNoteOf(isMaster ? -1 : (chIndex ?? -1)),
            builder: (context, solo, _) => MixerToggle(
                label: 'Solo',
                unavailableReason:
                    ready & 8 != 0 ? null : 'Solo unavailable',
                visualHeight: visualHeight,
                fontSize: AppDesign.body,
                channelName: ch.name,
                on: ready & 8 != 0 && solo,
                activeColor: ConsolePalette.of(context).solo,
                activeForeground: ConsolePalette.of(context).onSolo,
                onPressed:
                    ready & 8 != 0 ? () => controller.toggleSolo(ch) : null));
    }
  }
}

/// Shared mute/solo control. A minimum 44-point hit target is independent of
/// the strip density, and screen readers receive both name and toggle state.
class MixerToggle extends StatelessWidget {
  final String label, channelName;
  final String? unavailableReason;
  final bool on;
  final Color activeColor, activeForeground;
  final double visualHeight, fontSize;
  final double horizontalInset;
  final VoidCallback? onPressed;
  const MixerToggle(
      {super.key,
      required this.label,
      required this.channelName,
      this.unavailableReason,
      required this.on,
      this.visualHeight = AppDesign.touchTarget,
      this.fontSize = AppDesign.sectionTitle,
      this.horizontalInset = 0,
      required this.activeColor,
      required this.activeForeground,
      required this.onPressed});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      container: true,
      excludeSemantics: true,
      enabled: onPressed != null,
      onTap: onPressed,
      toggled: on,
      label:
          '$channelName $label${unavailableReason == null ? '' : ', $unavailableReason'}',
      child: Tooltip(
          message: unavailableReason ?? '$label $channelName',
          child: ConsolePressFeedback(
              enabled: onPressed != null,
              child: SizedBox(
                  height: math.max(AppDesign.touchTarget, visualHeight),
                  width: double.infinity,
                  child: Center(
                      child: LayoutBuilder(
                          builder: (context, bounds) {
                            final faceWidth = math.max(
                                0.0, bounds.maxWidth - 2 * horizontalInset);
                            return TextButton(
                              onPressed: onPressed,
                              style: TextButton.styleFrom(
                                      backgroundColor: on
                                          ? activeColor
                                          : ConsolePalette.of(context).control,
                                      foregroundColor: on
                                          ? activeForeground
                                          : ConsolePalette.of(context).text,
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: AppDesign.spaceXs),
                                      minimumSize:
                                          Size(faceWidth, visualHeight),
                                      maximumSize: horizontalInset > 0
                                          ? Size(faceWidth, double.infinity)
                                          : null,
                                      visualDensity: VisualDensity.standard,
                                      tapTargetSize:
                                          MaterialTapTargetSize.padded,
                                      shape: const RoundedRectangleBorder(
                                          borderRadius: AppDesign.radius),
                                      textStyle: mixerType(context, fontSize,
                                          weight: FontWeight.w600))
                                  .copyWith(
                                animationDuration:
                                    MediaQuery.disableAnimationsOf(context)
                                        ? Duration.zero
                                        : ConsoleMotion.state,
                                overlayColor:
                                    WidgetStateProperty.resolveWith((states) {
                                  if (states.contains(WidgetState.pressed)) {
                                    return ConsolePalette.of(context)
                                        .text
                                        .withValues(alpha: .16);
                                  }
                                  if (states.contains(WidgetState.hovered)) {
                                    return ConsolePalette.of(context)
                                        .text
                                        .withValues(alpha: .06);
                                  }
                                  return Colors.transparent;
                                }),
                                side: WidgetStateProperty.resolveWith(
                                    (states) => states
                                            .contains(WidgetState.focused)
                                        ? BorderSide(
                                            color: ConsolePalette.of(context)
                                                .selectedLine)
                                        : const BorderSide(
                                            color: Colors.transparent)),
                              ),
                              child:
                                  ExcludeSemantics(
                                      child: Text(label,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis)));
                          }))))));
}
