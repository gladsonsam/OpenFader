// Channel identity bar. Neutral icon area (44px) above a solid colour name label (32px);
// total 76px in the strip variant. The engine carries no icon or
// tape colour, so the icon area stays empty and the tape colour comes
// from the channel position (CHAN_TAPE, mod 8) via the parent.
import 'package:flutter/material.dart';
import '../palette.dart';

class U4NameBar extends StatelessWidget {
  final String name;
  final Color color;
  final Color text;

  const U4NameBar({
    super.key,
    required this.name,
    required this.color,
    required this.text,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      height: 76,
      clipBehavior: Clip.antiAlias,
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        borderRadius: BorderRadius.only(
          topLeft: Radius.circular(3),
          topRight: Radius.circular(3),
        ),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Icon area: 44px neutral well (no icon data in the engine feed).
          const SizedBox(height: 44),
          Container(
            height: 32,
            alignment: Alignment.center,
            padding: const EdgeInsets.symmetric(horizontal: 6),
            color: color,
            child: Text(
              name,
              textAlign: TextAlign.center,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                color: text,
                fontSize: 16,
                fontWeight: FontWeight.w600,
                height: 1.2,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
