part of 'channel.dart';

// Phone previews stay schematic without axes or readouts, unlike the tablet graphs.
class _PhoneNavPreview extends StatelessWidget {
  final ChannelStore store;
  final ChannelTab tab;
  final bool active;
  const _PhoneNavPreview(
      {required this.store, required this.tab, required this.active});
  @override
  Widget build(BuildContext context) => tab == ChannelTab.eq
      ? CustomPaint(
          size: Size.infinite,
          painter: _EqPreviewPainter(store.eq, ConsolePalette.of(context)))
      : CustomPaint(
          size: Size.infinite,
          painter: _PhoneOverviewPainter(
              palette: ConsolePalette.of(context),
              tab: tab,
              active: active,
              gain: store.preamp.gain,
              phantom: store.preamp.phantom,
              invert: store.preamp.invert,
              reduction: tab == ChannelTab.input
                  ? null
                  : store.grDb(
                      tab == ChannelTab.gate ? DynKind.gate : DynKind.comp)));
}

class _PhoneOverviewPainter extends CustomPainter {
  final ConsolePalette palette;
  final ChannelTab tab;
  final bool active, phantom, invert;
  final double gain;
  final double? reduction;
  const _PhoneOverviewPainter(
      {required this.palette,
      required this.tab,
      required this.active,
      required this.gain,
      required this.phantom,
      required this.invert,
      required this.reduction});
  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 150, size.height / 120);
    void box(Rect r, Color fill, {Color? edge, double radius = 5}) {
      final rr = RRect.fromRectAndRadius(r, Radius.circular(radius));
      canvas.drawRRect(rr, Paint()..color = fill);
      if (edge != null) {
        canvas.drawRRect(
            rr,
            Paint()
              ..color = edge
              ..style = PaintingStyle.stroke
              ..strokeWidth = 2);
      }
    }

    void text(String label, Offset center, double font, Color color) {
      final p = TextPainter(
          text: TextSpan(
              text: label,
              style:
                  AppDesign.type(font, color: color, weight: FontWeight.w700)),
          textDirection: TextDirection.ltr)
        ..layout();
      p.paint(canvas, center - Offset(p.width / 2, p.height / 2));
    }

    final ink = active ? palette.cyan : palette.text4;
    if (tab == ChannelTab.input) {
      box(const Rect.fromLTWH(6, 26, 138, 20), palette.chip,
          edge: palette.lineCtl);
      final fill = 10 + gain.clamp(0, 60) / 60 * 120;
      box(Rect.fromLTWH(10, 31, fill, 10), palette.processingSelected,
          edge: palette.cyan, radius: 3);
      box(Rect.fromLTWH((14 + fill).clamp(14, 130), 24, 9, 24), palette.panKnob,
          radius: 3);
      text('48V', const Offset(51, 85), 20,
          phantom ? palette.phantom : palette.text7);
      box(const Rect.fromLTWH(93, 68, 40, 34), palette.chip, radius: 10);
      text('Ø', const Offset(113, 85), 22,
          invert ? palette.cyan : palette.text7);
    } else {
      final axis = Path()
        ..moveTo(12, 10)
        ..lineTo(12, 110)
        ..lineTo(112, 110);
      canvas.drawPath(
          axis,
          Paint()
            ..color = palette.lineCtl
            ..style = PaintingStyle.stroke
            ..strokeWidth = 3);
      final curve = Path();
      if (tab == ChannelTab.gate) {
        curve.moveTo(12, 110);
        curve.lineTo(62, 110);
        curve.lineTo(62, 60);
        curve.lineTo(112, 10);
      } else {
        curve.moveTo(12, 110);
        curve.lineTo(42, 80);
        curve.quadraticBezierTo(52, 70, 62, 67.5);
        curve.lineTo(112, 55);
      }
      canvas.drawPath(
          curve,
          Paint()
            ..color = ink
            ..style = PaintingStyle.stroke
            ..strokeWidth = 5
            ..strokeCap = StrokeCap.round
            ..strokeJoin = StrokeJoin.round);
      box(const Rect.fromLTWH(124, 10, 18, 100), palette.well,
          edge: palette.lineCtl);
      final amount = (-(reduction ?? 0) / 20).clamp(0.0, 1.0);
      if (amount > 0) {
        box(Rect.fromLTWH(126.5, 12.5, 13, 95 * amount), palette.meterReduction,
            radius: 3);
      }
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_PhoneOverviewPainter old) =>
      old.palette != palette ||
      old.tab != tab ||
      old.active != active ||
      old.gain != gain ||
      old.phantom != phantom ||
      old.invert != invert ||
      old.reduction != reduction;
}
