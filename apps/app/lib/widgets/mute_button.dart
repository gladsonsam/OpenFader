// MUTE button: full-width h40 base (h48 on touch builds).
// matching the strip phone variant).
import 'package:flutter/material.dart';

import '../theme.dart';

class U4MuteButton extends StatelessWidget {
  final bool muted;
  final double height;
  final VoidCallback? onPressed;

  const U4MuteButton({
    super.key,
    required this.muted,
    this.height = 40,
    this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    return GestureDetector(
      onTap: enabled ? onPressed : null,
      child: Opacity(
        opacity: enabled ? 1 : 0.45,
        child: Container(
          height: height,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: muted ? U4.muteOnBg : U4.muteOffBg,
            border: Border.all(color: muted ? U4.muteOnBd : U4.muteOffBd),
            borderRadius: U4.rBtn,
          ),
          child: Text(
            'MUTE',
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 15,
              fontWeight: FontWeight.w500,
              color: muted ? Colors.white : U4.muteOffTx,
            ),
          ),
        ),
      ),
    );
  }
}
