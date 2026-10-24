part of 'channel.dart';

class _ProcessingButton extends StatelessWidget {
  final String label;
  final String? detail, semanticsLabel;
  final bool active;
  final double fontSize;
  final VoidCallback? onTap;
  const _ProcessingButton(
      {required this.label,
      this.detail,
      this.semanticsLabel,
      this.active = false,
      this.fontSize = 16,
      this.onTap});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      enabled: onTap != null,
      selected: active,
      label: semanticsLabel ?? label,
      child: InkWell(
          onTap: onTap,
          borderRadius: U4.rBtn,
          child: Container(
              constraints: const BoxConstraints(minWidth: 44, minHeight: 44),
              alignment: Alignment.center,
              padding: const EdgeInsets.all(10),
              decoration: BoxDecoration(
                  color: active
                      ? ConsolePalette.of(context).processingSelected
                      : ConsolePalette.of(context).chip,
                  borderRadius: U4.rBtn,
                  border: Border.all(
                      color: active
                          ? ConsolePalette.of(context)
                              .cyan
                              .withValues(alpha: .27)
                          : ConsolePalette.of(context).linePanel)),
              child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: detail == null
                      ? CrossAxisAlignment.center
                      : CrossAxisAlignment.start,
                  children: [
                    Text(label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: AppDesign.type(fontSize,
                            color: active
                                ? ConsolePalette.of(context).cyan
                                : onTap == null
                                    ? ConsolePalette.of(context).text7
                                    : ConsolePalette.of(context).text1,
                            weight: FontWeight.w600)),
                    if (detail != null) ...[
                      const SizedBox(height: 3),
                      Text(detail!,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: AppDesign.type(14,
                              color: active
                                  ? ConsolePalette.of(context).cyan
                                  : ConsolePalette.of(context).text7))
                    ]
                  ]))));
}

/// Relative horizontal drags, 44pt targets, and typed entry share one value.
class _ProcessingSlider extends StatefulWidget {
  final String label, text;
  final double value, min, max;
  final double fontSize;
  final bool enabled, pan, showLabel, compact, solidFill, inlineLabel;
  final Widget? trailing;
  final Color? color;
  final Color? valueColor;
  final VoidCallback? onFocus;
  final ValueChanged<double> onChanged;
  const _ProcessingSlider(
      {required this.label,
      required this.value,
      required this.min,
      required this.max,
      required this.text,
      required this.onChanged,
      this.fontSize = 14,
      this.trailing,
      this.onFocus,
      this.enabled = true,
      this.pan = false,
      this.showLabel = true,
      this.compact = false,
      this.solidFill = false,
      this.inlineLabel = false,
      this.valueColor,
      this.color});
  @override
  State<_ProcessingSlider> createState() => _ProcessingSliderState();
}

class _ProcessingSliderState extends State<_ProcessingSlider> {
  double? _drag;
  Future<void> _type() async {
    widget.onFocus?.call();
    final v = await showNumberEntry(
        context: context,
        title: widget.label,
        initial: widget.value.toStringAsFixed(2),
        min: widget.min,
        max: widget.max);
    if (v != null) widget.onChanged(v);
  }

  @override
  Widget build(BuildContext context) {
    final color = widget.color ?? ConsolePalette.of(context).cyan;
    final v = _drag ?? widget.value;
    final track = Row(children: [
      Expanded(
          child: LayoutBuilder(
              builder: (context, box) => Semantics(
                  slider: true,
                  enabled: widget.enabled,
                  label: widget.label,
                  value: widget.text,
                  onIncrease: widget.enabled
                      ? () => widget.onChanged(
                          (v + (widget.max - widget.min) / 100)
                              .clamp(widget.min, widget.max))
                      : null,
                  onDecrease: widget.enabled
                      ? () => widget.onChanged(
                          (v - (widget.max - widget.min) / 100)
                              .clamp(widget.min, widget.max))
                      : null,
                  child: GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTap: widget.enabled ? _type : null,
                      onHorizontalDragStart: widget.enabled
                          ? (_) {
                              widget.onFocus?.call();
                              setState(() => _drag = widget.value);
                            }
                          : null,
                      onHorizontalDragUpdate: widget.enabled
                          ? (d) {
                              final next = ((_drag ?? widget.value) +
                                      d.delta.dx /
                                          (box.maxWidth - 24)
                                              .clamp(1, double.infinity) *
                                          (widget.max - widget.min))
                                  .clamp(widget.min, widget.max)
                                  .toDouble();
                              setState(() => _drag = next);
                              widget.onChanged(next);
                            }
                          : null,
                      onHorizontalDragEnd: widget.enabled
                          ? (_) => setState(() => _drag = null)
                          : null,
                      onHorizontalDragCancel: widget.enabled
                          ? () => setState(() => _drag = null)
                          : null,
                      child: Opacity(
                          opacity: widget.enabled ? 1 : .45,
                          child: CustomPaint(
                              size: Size(box.maxWidth, 44),
                              painter: _ProcessingTrackPainter(
                                  palette: ConsolePalette.of(context),
                                  n: ((v - widget.min) / (widget.max - widget.min))
                                      .clamp(0, 1),
                                  color: color,
                                  solidFill: widget.solidFill,
                                  bipolar: widget.min == -widget.max,
                                  pan: widget.pan))))))),
      if (widget.trailing != null) ...[
        const SizedBox(width: 6),
        SizedBox(width: 44, child: widget.trailing)
      ],
    ]);
    final label = InkWell(
        onTap: widget.enabled ? _type : null,
        child: Align(
            alignment: Alignment.topCenter,
            child: Row(children: [
              Expanded(
                  child: Text(widget.label,
                      style: AppDesign.type(widget.fontSize,
                          color: ConsolePalette.of(context).text7))),
              Text(widget.text,
                  style: AppDesign.type(widget.fontSize,
                      color: widget.enabled
                          ? (widget.valueColor ?? color)
                          : ConsolePalette.of(context).text7,
                      weight: FontWeight.w600)),
            ])));
    if (!widget.showLabel) return track;
    if (widget.inlineLabel) {
      return SizedBox(
          height: 46,
          child: Row(children: [
            SizedBox(
                width: 20,
                child: InkWell(
                    onTap: widget.enabled ? _type : null,
                    child: Text(widget.label,
                        style: AppDesign.type(18,
                            color: ConsolePalette.of(context).text7)))),
            const SizedBox(width: 10),
            Expanded(child: track),
          ]));
    }
    // The label's 44pt entry target overlaps the track's identical tap action.
    // Visual label spacing stays 20pt without shrinking touch targets.
    return SizedBox(
        height: widget.compact ? 64 : 88,
        child: Stack(children: [
          Positioned(left: 0, right: 0, top: 0, height: 44, child: label),
          Positioned(
              left: 0,
              right: 0,
              top: widget.compact ? 20 : 44,
              height: 44,
              child: track),
        ]));
  }
}

class _ProcessingTrackPainter extends CustomPainter {
  final ConsolePalette palette;
  final double n;
  final Color color;
  final bool pan, solidFill, bipolar;
  const _ProcessingTrackPainter(
      {required this.palette,
      required this.n,
      required this.color,
      this.pan = false,
      this.bipolar = false,
      this.solidFill = false});
  @override
  void paint(Canvas canvas, Size size) {
    final bounds = Offset.zero & size;
    canvas.drawRRect(RRect.fromRectAndRadius(bounds, const Radius.circular(12)),
        Paint()..color = palette.panel);
    canvas.drawRRect(
        RRect.fromRectAndRadius(bounds.deflate(.5), const Radius.circular(12)),
        Paint()
          ..color = palette.linePanel
          ..style = PaintingStyle.stroke);
    final x = 12 + (size.width - 24) * n;
    if (pan) {
      final p = Path()
        ..moveTo(6, 8)
        ..lineTo(size.width / 2, 19)
        ..lineTo(size.width - 6, 8)
        ..lineTo(size.width - 6, 36)
        ..lineTo(size.width / 2, 25)
        ..lineTo(6, 36)
        ..close();
      canvas.drawPath(p, Paint()..color = palette.panShape);
    } else {
      canvas.drawRRect(
          RRect.fromRectAndRadius(
              Rect.fromLTRB(
                  bipolar ? (x < size.width / 2 ? x : size.width / 2) : 6,
                  6,
                  bipolar ? (x > size.width / 2 ? x : size.width / 2) : x,
                  38),
              const Radius.circular(6)),
          Paint()..color = solidFill ? color : color.withValues(alpha: .34));
    }
    final cap = RRect.fromRectAndRadius(
        Rect.fromCenter(center: Offset(x, 22), width: 22, height: 32),
        const Radius.circular(6));
    canvas.drawShadow(Path()..addRRect(cap), palette.well, 3, false);
    canvas.drawRRect(cap, Paint()..color = palette.knob);
    for (var dx = -5.0; dx <= 5; dx += 4) {
      canvas.drawLine(
          Offset(x + dx, 13),
          Offset(x + dx, 31),
          Paint()
            ..color = palette.text10
            ..strokeWidth = 2);
    }
  }

  @override
  bool shouldRepaint(_ProcessingTrackPainter old) =>
      old.palette != palette ||
      old.n != n ||
      old.color != color ||
      old.pan != pan ||
      old.bipolar != bipolar ||
      old.solidFill != solidFill;
}
