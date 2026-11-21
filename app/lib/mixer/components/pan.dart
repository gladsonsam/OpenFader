import 'dart:math' as math;

import '../../design.dart';
import '../../palette.dart';
import 'package:material_ui/material_ui.dart';
import '../../u4law.dart';
import '../controller.dart';
import '../design.dart';

/// Strip-level pan row. Master strips and outputs without a channel identity
/// keep the control visible but inert; a channel follows its live pan notifier
/// and the provider readiness bit.
class MixerPanCell extends StatelessWidget {
  final MixerChannel channel;
  final MixerController controller;
  final int ready;
  final double allocatedHeight;
  final ValueChanged<MixerReadoutControl>? onControlFocus;
  const MixerPanCell(
      {super.key,
      required this.channel,
      required this.controller,
      required this.ready,
      required this.allocatedHeight,
      this.onControlFocus});

  @override
  Widget build(BuildContext context) {
    final name = channel.name;
    final visualHeight = math.min(AppDesign.spaceXl, allocatedHeight);
    final chIndex = channel.channel;
    if (channel.master || chIndex == null) {
      return MixerPan(
          name: name, value: 0, visualHeight: visualHeight, onChanged: null);
    }
    return ValueListenableBuilder<double>(
        valueListenable: controller.panNoteOf(chIndex),
        builder: (context, pan, _) => MixerPan(
            name: name,
            value: pan,
            onTouch: () => onControlFocus?.call(MixerReadoutControl.pan),
            visualHeight: visualHeight,
            onChanged: ready & 4 != 0
                ? (v) {
                    onControlFocus?.call(MixerReadoutControl.pan);
                    controller.setPan(channel, v);
                  }
                : null));
  }
}

class MixerPan extends StatefulWidget {
  final String name;
  final double value;
  final double visualHeight;
  final ValueChanged<double>? onChanged;
  final VoidCallback? onTouch;
  const MixerPan(
      {super.key,
      required this.name,
      required this.value,
      this.visualHeight = AppDesign.spaceXl,
      this.onTouch,
      this.onChanged});
  @override
  State<MixerPan> createState() => _MixerPanState();
}

class _MixerPanState extends State<MixerPan> {
  double? _drag;
  double _snap(double value) => value.abs() < .03 ? 0 : value.clamp(-1, 1);
  void _release() => setState(() => _drag = null);
  void _send(double value) => widget.onChanged?.call(_snap(value));
  @override
  Widget build(BuildContext context) => Listener(
      onPointerDown:
          widget.onChanged == null ? null : (_) => widget.onTouch?.call(),
      child: _pan(context));

  Widget _pan(BuildContext context) => LayoutBuilder(
      builder: (context, box) => Semantics(
          label: '${widget.name} pan',
          value: panText(widget.value),
          increasedValue: panText((widget.value + .05).clamp(-1.0, 1.0)),
          decreasedValue: panText((widget.value - .05).clamp(-1.0, 1.0)),
          slider: true,
          enabled: widget.onChanged != null,
          onIncrease:
              widget.onChanged == null ? null : () => _send(widget.value + .05),
          onDecrease:
              widget.onChanged == null ? null : () => _send(widget.value - .05),
          child: Tooltip(
              message: '${panText(widget.value)} · double-tap to centre',
              triggerMode: TooltipTriggerMode.manual,
              child: GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onHorizontalDragStart: widget.onChanged == null
                      ? null
                      : (_) => setState(() => _drag = widget.value),
                  onHorizontalDragUpdate: widget.onChanged == null
                      ? null
                      : (d) {
                          final next = ((_drag ?? widget.value) +
                                  d.delta.dx / ((box.maxWidth - 24) * .36))
                              .clamp(-1.0, 1.0);
                          setState(() => _drag = next);
                          _send(next);
                        },
                  onHorizontalDragEnd: (_) => _release(),
                  onHorizontalDragCancel: _release,
                  onDoubleTap: widget.onChanged == null ? null : () => _send(0),
                  child: SizedBox(
                      height:
                          math.max(AppDesign.touchTarget, widget.visualHeight),
                      width: double.infinity,
                      child: Center(
                          child: SizedBox(
                              width: double.infinity,
                              height: widget.visualHeight,
                              child: CustomPaint(
                                  painter: _PanPainter(
                                      ConsolePalette.of(context),
                                      _drag == null
                                          ? widget.value
                                          : _snap(_drag!),
                                      _drag != null)))))))));
}

class _PanPainter extends CustomPainter {
  final ConsolePalette palette;
  final double value;
  final bool held;
  const _PanPainter(this.palette, this.value, this.held);
  @override
  void paint(Canvas c, Size s) {
    c.save();
    c.scale(1, s.height / AppDesign.touchTarget);
    s = Size(s.width, AppDesign.touchTarget);
    mixerBox(c, Offset.zero & s, palette.well,
        radius: AppDesign.actionRadius,
        border: held ? palette.selectedLine : null);
    final left = MixerStripMetrics.contentInset,
        right = s.width - MixerStripMetrics.contentInset,
        center = s.width / 2;
    final bow = Path()
      ..moveTo(left, 8)
      ..quadraticBezierTo(center, 26, right, 8)
      ..lineTo(right, 36)
      ..quadraticBezierTo(center, 18, left, 36)
      ..close();
    c.drawPath(bow, Paint()..color = palette.panShape);
    final x = center + value * (s.width - 24) * .36;
    if (value.abs() > .001) {
      c.save();
      c.clipPath(bow);
      mixerBox(
          c,
          Rect.fromLTRB(
              x < center ? x : center, 8, x > center ? x : center, 36),
          palette.panFill);
      c.restore();
    }
    const knobWidth = AppDesign.spaceLg;
    mixerBox(c, Rect.fromLTWH(x - knobWidth / 2, 5, knobWidth, 34),
        palette.panKnob, radius: 6);
    for (var n = 0; n < 3; n++) {
      mixerBox(c, Rect.fromLTWH(x - 4 + n * 4, 11, 1, 22), palette.panGrip,
          radius: 1);
    }
    c.restore();
  }

  @override
  bool shouldRepaint(_PanPainter old) =>
      old.palette != palette || old.value != value || old.held != held;
}
