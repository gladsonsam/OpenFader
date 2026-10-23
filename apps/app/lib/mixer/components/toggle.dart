import 'package:flutter/material.dart';
import '../design.dart';

/// Shared mute/solo control. A minimum 44-point hit target is independent of
/// the strip density, and screen readers receive both name and toggle state.
class MixerToggle extends StatelessWidget {
  final String label, channelName;
  final bool on;
  final Color activeColor;
  final double visualHeight, fontSize;
  final VoidCallback? onPressed;
  const MixerToggle(
      {super.key,
      required this.label,
      required this.channelName,
      required this.on,
      this.visualHeight = 44,
      this.fontSize = 21,
      required this.activeColor,
      required this.onPressed});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      container: true,
      excludeSemantics: true,
      enabled: onPressed != null,
      onTap: onPressed,
      toggled: on,
      label: '$channelName $label',
      child: Tooltip(
          message: '$label $channelName',
          child: SizedBox(
              height: 44,
              width: double.infinity,
              child: Center(
                  child: SizedBox(
                      width: double.infinity,
                      child: TextButton(
                          onPressed: onPressed,
                          style: TextButton.styleFrom(
                                  backgroundColor:
                                      on ? activeColor : MixerDesign.control,
                                  foregroundColor:
                                      on && activeColor == MixerDesign.solo
                                          ? const Color(0xff1b1a05)
                                          : MixerDesign.text,
                                  padding:
                                      const EdgeInsets.symmetric(horizontal: 4),
                                  minimumSize: Size(44, visualHeight),
                                  visualDensity: VisualDensity.standard,
                                  tapTargetSize: MaterialTapTargetSize.padded,
                                  shape: const RoundedRectangleBorder(
                                      borderRadius: MixerDesign.radius),
                                  textStyle: MixerDesign.type(fontSize,
                                      weight: FontWeight.w600))
                              .copyWith(
                            animationDuration:
                                MediaQuery.disableAnimationsOf(context)
                                    ? Duration.zero
                                    : const Duration(milliseconds: 100),
                            overlayColor:
                                WidgetStateProperty.resolveWith((states) {
                              if (states.contains(WidgetState.pressed)) {
                                return Colors.white.withValues(alpha: .16);
                              }
                              if (states.contains(WidgetState.hovered)) {
                                return Colors.white.withValues(alpha: .06);
                              }
                              return Colors.transparent;
                            }),
                            side: WidgetStateProperty.resolveWith((states) =>
                                states.contains(WidgetState.focused)
                                    ? const BorderSide(
                                        color: MixerDesign.selectedLine)
                                    : const BorderSide(
                                        color: Colors.transparent)),
                          ),
                          child: ExcludeSemantics(child: Text(label))))))));
}
