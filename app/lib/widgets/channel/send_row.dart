import 'package:material_ui/material_ui.dart';

import '../../ui/console.dart';
import '../../core/channel_views.dart';

class U4SendRow extends StatelessWidget {
  final ChannelSendVm send;
  final ValueChanged<double> onLevel;
  final VoidCallback onMute;
  final VoidCallback? onTapCycle;

  const U4SendRow({
    super.key,
    required this.send,
    required this.onLevel,
    required this.onMute,
    this.onTapCycle,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).tileBg,
        border: Border.all(color: ConsolePalette.of(context).tileBorder),
        borderRadius: AppDesign.radius,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('S${send.slot} · ${send.destName}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                        fontFamily: AppDesign.font,
                        fontSize: AppDesign.smallLabel,
                        fontWeight: FontWeight.w600,
                        color: ConsolePalette.of(context).text)),
                const SizedBox(height: 6),
                _MiniLevel(
                  value: send.level,
                  onChanged: onLevel,
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          GestureDetector(
            onTap: onMute,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: send.muted
                    ? ConsolePalette.of(context).muteOnBg
                    : ConsolePalette.of(context).tileBg,
                border: Border.all(
                    color: send.muted
                        ? ConsolePalette.of(context).muteOnBd
                        : ConsolePalette.of(context).tileBorder),
                borderRadius: AppDesign.radius,
              ),
              child: Text(send.muted ? 'MUTED' : 'open',
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: AppDesign.font,
                      fontSize: AppDesign.minorText,
                      fontWeight: FontWeight.w700,
                      color: send.muted
                          ? ConsolePalette.of(context).onDanger
                          : ConsolePalette.of(context).mutedText)),
            ),
          ),
          const SizedBox(width: 6),
          GestureDetector(
            onTap: onTapCycle,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 12),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: ConsolePalette.of(context).panel,
                border:
                    Border.all(color: ConsolePalette.of(context).railBorder),
                borderRadius: AppDesign.radius,
              ),
              child: Text(
                send.tap == null
                    ? 'tap ?'
                    : tapLabels[send.tap] ?? 'tap ${send.tap}',
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontFamily: AppDesign.font,
                    fontSize: AppDesign.minorText,
                    fontWeight: FontWeight.w700,
                    color: ConsolePalette.of(context).accent),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _MiniLevel extends StatelessWidget {
  final double value;
  final ValueChanged<double> onChanged;

  const _MiniLevel({required this.value, required this.onChanged});

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onHorizontalDragStart: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      onHorizontalDragUpdate: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      onTapUp: (d) {
        final box = context.findRenderObject() as RenderBox?;
        if (box == null || box.size.width <= 0) return;
        onChanged((d.localPosition.dx / box.size.width).clamp(0.0, 1.0));
      },
      child: SizedBox(
        height: 18,
        child: CustomPaint(
          size: const Size(double.infinity, 18),
          painter: _MiniLevelPainter(
              palette: ConsolePalette.of(context), value: value),
        ),
      ),
    );
  }
}

class _MiniLevelPainter extends CustomPainter {
  final ConsolePalette palette;
  final double value;
  _MiniLevelPainter({required this.palette, required this.value});

  @override
  void paint(Canvas canvas, Size size) {
    final bar = Rect.fromLTWH(0, 7, size.width, 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(bar, const Radius.circular(2)),
      Paint()..color = palette.control,
    );
    final fill = Rect.fromLTWH(0, 7, size.width * value.clamp(0.0, 1.0), 4);
    canvas.drawRRect(
      RRect.fromRectAndRadius(fill, const Radius.circular(2)),
      Paint()..color = palette.accent,
    );
    final x = size.width * value.clamp(0.0, 1.0);
    canvas.drawCircle(Offset(x, 9), 7, Paint()..color = palette.text);
    canvas.drawCircle(
        Offset(x, 9),
        7,
        Paint()
          ..color = palette.lineCtl
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5);
  }

  @override
  bool shouldRepaint(covariant _MiniLevelPainter old) =>
      old.palette != palette || old.value != value;
}
