// H1 SceneButton: the header scene-dropdown button. Padded scene number +
// scene name, chevron painted with a CustomPainter, highlighted when the
// Scenes page is open.
library;

import 'package:flutter/material.dart';

import '../theme.dart';

class U4SceneButton extends StatelessWidget {
  final String num;
  final String name;

  /// aria label, e.g. 'Scene 5 Changeover, open scene list'.
  final String label;
  final bool active;

  /// Phone density: cap width and ellipsize the scene name.
  final bool narrow;
  final VoidCallback? onOpen;

  const U4SceneButton({
    super.key,
    required this.num,
    required this.name,
    required this.label,
    this.active = false,
    this.narrow = false,
    this.onOpen,
  });

  @override
  Widget build(BuildContext context) {
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      onTap: onOpen,
      child: GestureDetector(
        onTap: onOpen,
        behavior: HitTestBehavior.opaque,
        child: Container(
          height: narrow ? 44 : 46,
          constraints: BoxConstraints(maxWidth: narrow ? double.infinity : 280),
          padding: EdgeInsets.symmetric(horizontal: narrow ? 8 : 12),
          decoration: BoxDecoration(
            color: active ? U4.selected : U4.tileBg,
            border: Border.all(color: active ? U4.lineCtl : U4.divider),
            borderRadius: U4.rBtn,
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Flexible(
                child: RichText(
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  text: TextSpan(
                    children: [
                      TextSpan(
                        text: num.isEmpty ? '' : '$num  ',
                        style: const TextStyle(fontSize: 15, color: U4.text10),
                      ),
                      TextSpan(
                        text: name,
                        style: const TextStyle(
                            fontSize: 15,
                            fontWeight: FontWeight.w600,
                            color: U4.text1),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 10),
              const CustomPaint(
                size: Size(16, 16),
                painter: _ChevronPainter(),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Down-chevron (inline SVG path `M6 9l6 6 6-6`).
class _ChevronPainter extends CustomPainter {
  const _ChevronPainter();

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = U4.text7
      ..strokeWidth = 2.2
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..style = PaintingStyle.stroke;
    final path = Path()
      ..moveTo(size.width * 0.25, size.height * 0.375)
      ..lineTo(size.width * 0.5, size.height * 0.625)
      ..lineTo(size.width * 0.75, size.height * 0.375);
    canvas.drawPath(path, p);
  }

  @override
  bool shouldRepaint(_ChevronPainter old) => false;
}
