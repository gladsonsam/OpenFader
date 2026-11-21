import '../../palette.dart';
import 'dart:math' as math;
import 'package:flutter/foundation.dart';
import 'package:material_ui/material_ui.dart';
import '../controller.dart';
import '../design.dart';
import '../demo.dart';
import '../../design.dart';

enum MixerProcessingMode { eq, dynamics, full }

/// Strip-level processing preview, sized by the strip's available height.
class MixerProcessingCell extends StatelessWidget {
  final MixerChannel channel;
  final MixerController controller;
  final MixerProcessingMode mode;
  final VoidCallback onOpen;
  const MixerProcessingCell(
      {super.key,
      required this.channel,
      required this.controller,
      required this.mode,
      required this.onOpen});

  @override
  Widget build(BuildContext context) => mode == MixerProcessingMode.eq
      ? MixerEqPreview(
          channel: channel,
          compact: true,
          showPhantom: controller.hasPhantomOf(channel),
          onOpen: onOpen)
      : MixerProcessingRack(
          channel: channel,
          showGain: mode == MixerProcessingMode.full,
          gainRange: controller.gainRangeOf(channel),
          showPhantom: controller.hasPhantomOf(channel),
          demo: controller.demo,
          onOpen: onOpen,
          onGain: (v) => controller.setGain(channel, v));
}

/// Processing preview owns its rendering and gain gesture. Tapping its EQ
/// opens the existing channel editor; hardware curves use actual parameters.
class MixerProcessingRack extends StatefulWidget {
  static const _inset = AppDesign.spaceXs;
  static const _gainHeight = 20.0;
  static const _dynamicsHeight = AppDesign.spaceLg;
  static const dynamicsMinHeight =
      AppDesign.touchTarget + _dynamicsHeight + 3 * _inset;
  static const fullMinHeight = dynamicsMinHeight + _gainHeight + _inset;

  final MixerChannel channel;
  final bool demo;
  final bool showGain;
  final List<double>? gainRange;
  final bool showPhantom;
  final VoidCallback onOpen;
  final ValueChanged<double> onGain;
  const MixerProcessingRack(
      {super.key,
      required this.channel,
      required this.demo,
      required this.showGain,
      this.gainRange,
      this.showPhantom = false,
      required this.onOpen,
      required this.onGain});
  @override
  State<MixerProcessingRack> createState() => _MixerProcessingRackState();
}

class _MixerProcessingRackState extends State<MixerProcessingRack> {
  double? _gainDrag;

  @override
  void didUpdateWidget(covariant MixerProcessingRack oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.channel.key != widget.channel.key ||
        widget.gainRange == null) {
      _gainDrag = null;
    }
  }

  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        const insetY = MixerProcessingRack._inset;
        const gainHeight = MixerProcessingRack._gainHeight;
        const dynamicsHeight = MixerProcessingRack._dynamicsHeight;
        final eqTop =
            widget.showGain ? insetY + gainHeight + insetY : insetY;
        final dynamicsTop = box.maxHeight - insetY - dynamicsHeight;
        Widget place(double top, double height, Widget child) =>
            Positioned(
                left: AppDesign.spaceSm,
                right: AppDesign.spaceSm,
                top: top,
                height: height,
                child: child);
        return DecoratedBox(
            decoration: BoxDecoration(
                color: ConsolePalette.of(context).well,
                borderRadius: AppDesign.radius,
                border: Border.all(color: ConsolePalette.of(context).line)),
            child: Stack(children: [
              if (widget.showGain)
                place(
                  insetY,
                  gainHeight,
                  widget.channel.master || widget.gainRange == null
                      ? Semantics(
                          button: true,
                          label: 'Open ${widget.channel.name} processing',
                          child: InkWell(
                            onTap: widget.onOpen,
                            child: Center(
                              child: Text(widget.channel.name.toUpperCase(),
                                  maxLines: 1, overflow: TextOverflow.ellipsis,
                                  style: mixerType(context, 12,
                                      color: ConsolePalette.of(context).mutedText,
                                      weight: FontWeight.w700)))))
                      : LayoutBuilder(
                      builder: (context, gainBox) => Semantics(
                          container: true,
                          label: '${widget.channel.name} gain',
                          slider: !widget.channel.master &&
                              widget.gainRange != null,
                          enabled:
                              widget.channel.master || widget.gainRange != null,
                          child: GestureDetector(
                              behavior: HitTestBehavior.opaque,
                              onHorizontalDragStart: widget.channel.master ||
                                      widget.gainRange == null
                                  ? null
                                  : (_) => _gainDrag =
                                      (widget.channel.params['gain'] as num?)
                                              ?.toDouble() ??
                                          0,
                              onHorizontalDragUpdate: widget.channel.master ||
                                      widget.gainRange == null
                                  ? null
                                  : (d) {
                                      final current = _gainDrag;
                                      if (current == null) return;
                                      final range = widget.gainRange!;
                                      final next = (current +
                                              d.delta.dx /
                                                  gainBox.maxWidth *
                                                  (range.last - range.first))
                                          .clamp(range.first, range.last);
                                      _gainDrag = next;
                                      widget.onGain(next);
                                    },
                              onHorizontalDragEnd: (_) => _gainDrag = null,
                              onHorizontalDragCancel: () => _gainDrag = null,
                              child: CustomPaint(
                                  painter: _GainPainter(
                                      ConsolePalette.of(context),
                                      widget.channel.params,
                                      widget.demo,
                                      widget.gainRange,
                                      widget.showPhantom)))))),
              place(
                  eqTop,
                  math.max(0, dynamicsTop - AppDesign.spaceXs - eqTop),
                  MixerEqPreview(
                      channel: widget.channel,
                      compact: !widget.showGain,
                      showPhantom: widget.showPhantom,
                      onOpen: widget.onOpen)),
              place(
                  dynamicsTop,
                  dynamicsHeight,
                  Semantics(
                      button: true,
                      label: '${widget.channel.name} dynamics and processing',
                      child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: widget.onOpen,
                          child: CustomPaint(
                              painter: _DynamicsPainter(
                                  ConsolePalette.of(context),
                                  widget.channel.params,
                                  widget.demo &&
                                          !widget.channel.params
                                              .containsKey('gate_gr') &&
                                          !widget.channel.params
                                              .containsKey('comp_gr')
                                      ? mixerDemoStrips[widget.channel.artwork]
                                          .compression
                                      : null))))),
            ]));
      });
}

/// Shared EQ entry point. The compact variant gives phones the same access to
/// channel processing without spending fader travel on gain/dynamics previews.
class MixerEqPreview extends StatelessWidget {
  final MixerChannel channel;
  final bool compact;
  final bool showPhantom;
  final VoidCallback onOpen;
  const MixerEqPreview({
    super.key,
    required this.channel,
    required this.onOpen,
    this.compact = false,
    this.showPhantom = false,
  });

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: '${channel.name} EQ and processing',
      child: Tooltip(
          message: 'Open ${channel.name} processing',
          child: ClipRRect(
              borderRadius: BorderRadius.circular(6),
              child: Stack(fit: StackFit.expand, children: [
                CustomPaint(
                    painter:
                        _EqPainter(ConsolePalette.of(context), channel.eqPreview)),
                if (compact && showPhantom)
                  Positioned(
                      top: 3,
                      right: 5,
                      child: IgnorePointer(
                          child: ExcludeSemantics(
                              child: Text('48V',
                                  style: mixerType(context, 10,
                                          color: (channel.params['phantom'] ==
                                                      true ||
                                                  channel.params['phantom'] ==
                                                      1)
                                              ? ConsolePalette.of(context)
                                                  .phantom
                                              : ConsolePalette.of(context)
                                                  .mutedText,
                                          weight: FontWeight.w700)
                                      .copyWith(
                                          backgroundColor:
                                              ConsolePalette.of(context)
                                                  .eqBg))))),
                Material(
                    color: Colors.transparent,
                    child: InkWell(
                        onTap: onOpen,
                        overlayColor: WidgetStateProperty.resolveWith(
                            (states) => states.contains(WidgetState.pressed)
                                ? ConsolePalette.of(context)
                                    .text
                                    .withValues(alpha: .16)
                                : null))),
              ]))));
}

class _GainPainter extends CustomPainter {
  final ConsolePalette palette;
  final Map<String, Object> params;
  final bool demo;
  final List<double>? gainRange;
  final bool showPhantom;
  _GainPainter(this.palette, this.params, this.demo, this.gainRange, this.showPhantom);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, palette.panel, radius: 6);
    const inset = AppDesign.spaceXs;
    final phantom = params['phantom'] == true || params['phantom'] == 1;
    final label = showPhantom
        ? (TextPainter(
            text: TextSpan(
                text: '48V',
                style: AppDesign.type(AppDesign.microText,
                    color: phantom ? palette.phantom : palette.mutedText,
                    weight: FontWeight.w700)),
            textDirection: TextDirection.ltr,
            maxLines: 1,
          )..layout(maxWidth: math.max(0, s.width - 2 * inset)))
        : null;
    final labelX = s.width - inset - (label?.width ?? 0);
    final trackEnd = labelX - (label == null ? 0 : inset);
    final width = math.max(0.0, trackEnd - inset);
    final trackY = (s.height - 6) / 2;
    if (label != null) {
      label.paint(c, Offset(labelX, (s.height - label.height) / 2));
      label.dispose();
    }
    if (width < 3) return;
    mixerBox(c, Rect.fromLTWH(inset, trackY, width, 6), palette.control,
        radius: 3, border: palette.lineCtl);
    final amount = gainRange == null
        ? 0.0
        : ((((params['gain'] as num?)?.toDouble() ?? gainRange!.first) -
                    gainRange!.first) /
                (gainRange!.last - gainRange!.first))
            .clamp(0.0, 1.0);
    final travel = math.max(0.0, width - 5);
    mixerBox(c, Rect.fromLTWH(inset, trackY + 1, travel * amount, 4),
        palette.meterCover, radius: 2);
    mixerBox(c, Rect.fromLTWH(inset + travel * amount, trackY, 3, 6),
        palette.textSecondary, radius: 1);
  }

  @override
  bool shouldRepaint(_GainPainter old) =>
      old.palette != palette ||
      old.demo != demo ||
      old.showPhantom != showPhantom ||
      !listEquals(old.gainRange, gainRange) ||
      !mapEquals(old.params, params);
}

class _DynamicsPainter extends CustomPainter {
  final ConsolePalette palette;
  final Map<String, Object> params;
  final List<double>? fixture;
  _DynamicsPainter(this.palette, this.params, this.fixture);
  @override
  void paint(Canvas c, Size s) {
    final trackHeight = s.height * 15 / 62;
    final gap = s.height * 8 / 62;
    final top = (s.height - trackHeight * 2 - gap) / 2;
    for (var i = 0; i < 2; i++) {
      final track =
          Rect.fromLTWH(0, top + i * (trackHeight + gap), s.width, trackHeight);
      mixerBox(c, track, palette.well,
          radius: 3, border: palette.line, stroke: 1);
      final value = fixture != null && i < fixture!.length
          ? fixture![i] / 100
          : ((params[i == 0 ? 'gate_gr' : 'comp_gr'] as num?)
                      ?.toDouble()
                      .abs() ??
                  0) /
              30;
      // Both meters use the same well and inset. Reduction stays inside the
      // well even at 100%, instead of spilling past its right/bottom edges.
      final inner = track.deflate((trackHeight / 4).clamp(0.0, 2.0));
      final width = inner.width * value.clamp(0.0, 1.0);
      if (width > 0) {
        c.save();
        c.clipRRect(RRect.fromRectAndRadius(inner, const Radius.circular(2)));
        mixerBox(
            c,
            Rect.fromLTWH(inner.right - width, inner.top, width, inner.height),
            palette.comp,
            radius: 2);
        c.restore();
      }
    }
  }

  @override
  bool shouldRepaint(_DynamicsPainter old) =>
      old.palette != palette ||
      !listEquals(old.fixture, fixture) ||
      !mapEquals(old.params, params);
}

class _EqPainter extends CustomPainter {
  final ConsolePalette palette;
  final List<double>? response;
  _EqPainter(this.palette, this.response);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, palette.eqBg);
    final zeroY = s.height / 2;
    c.drawLine(Offset(0, zeroY), Offset(s.width, zeroY),
        Paint()..color = palette.mutedText.withValues(alpha: .35));
    final values = response;
    if (values == null || values.length < 2) return;
    final curve = Path();
    for (var i = 0; i < values.length; i++) {
      final x = s.width * i / (values.length - 1);
      final y = (s.height / 2 - values[i] * s.height / 36).clamp(0.0, s.height);
      if (i == 0) {
        curve.moveTo(x, y);
      } else {
        curve.lineTo(x, y);
      }
    }
    final fill = Path.from(curve)
      ..lineTo(s.width, zeroY)
      ..lineTo(0, zeroY)
      ..close();
    c.drawPath(
        fill, Paint()..color = palette.eqPreviewFill.withValues(alpha: .85));
    c.drawPath(
        curve,
        Paint()
          ..color = palette.eqPreviewCurve
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.4);
  }

  @override
  bool shouldRepaint(_EqPainter old) =>
      old.palette != palette || !listEquals(old.response, response);
}
