import '../../palette.dart';
import 'package:material_ui/material_ui.dart';
import '../design.dart';
import '../../ui/motion.dart';

/// Shared mute/solo control. A minimum 44-point hit target is independent of
/// the strip density, and screen readers receive both name and toggle state.
class MixerToggle extends StatelessWidget {
  final String label, channelName;
  final String? unavailableReason;
  final bool on;
  final Color activeColor, activeForeground;
  final double visualHeight, fontSize;
  final VoidCallback? onPressed;
  const MixerToggle(
      {super.key,
      required this.label,
      required this.channelName,
      this.unavailableReason,
      required this.on,
      this.visualHeight = 44,
      this.fontSize = 21,
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
                  height: visualHeight < 44 ? 44 : visualHeight,
                  width: double.infinity,
                  child: Center(
                      child: SizedBox(
                          height: visualHeight,
                          width: double.infinity,
                          child: TextButton(
                              onPressed: onPressed,
                              style: TextButton.styleFrom(
                                      backgroundColor: on
                                          ? activeColor
                                          : ConsolePalette.of(context).control,
                                      foregroundColor: on
                                          ? activeForeground
                                          : ConsolePalette.of(context).text,
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: 4),
                                      minimumSize: Size(44, visualHeight),
                                      visualDensity: VisualDensity.standard,
                                      tapTargetSize:
                                          MaterialTapTargetSize.shrinkWrap,
                                      shape: const RoundedRectangleBorder(
                                          borderRadius: MixerDesign.radius),
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
                                  ExcludeSemantics(child: Text(label)))))))));
}
