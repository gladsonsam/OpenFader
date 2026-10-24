import '../../palette.dart';
import 'dart:math' as math;
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import '../demo.dart';

/// Processing preview owns its rendering and gain gesture. Tapping its EQ
/// opens the existing channel editor; hardware curves use actual parameters.
class MixerProcessingRack extends StatelessWidget {
  final MixerChannel channel;
  final bool demo;
  final List<double>? gainRange;
  final VoidCallback onOpen;
  final ValueChanged<double> onGain;
  const MixerProcessingRack(
      {super.key,
      required this.channel,
      required this.demo,
      this.gainRange,
      required this.onOpen,
      required this.onGain});
  @override
  Widget build(BuildContext context) => LayoutBuilder(builder: (context, box) {
        final sy = box.maxHeight / 202;
        final sx = box.maxWidth / 178.2;
        Widget place(double top, double height, Widget child) => Positioned(
            left: 5 * sx,
            right: 5 * sx,
            top: top * sy,
            height: height * sy,
            child: child);
        return DecoratedBox(
            decoration: BoxDecoration(
                color: ConsolePalette.of(context).well,
                borderRadius: MixerDesign.radius,
                border: Border.all(color: ConsolePalette.of(context).line)),
            child: Stack(children: [
              place(
                  5,
                  channel.master ? 57 : 36,
                  LayoutBuilder(
                      builder: (context, gainBox) => Semantics(
                          container: true,
                          label: '${channel.name} gain',
                          slider: !channel.master && gainRange != null,
                          enabled: channel.master || gainRange != null,
                          child: GestureDetector(
                              behavior: HitTestBehavior.opaque,
                              onHorizontalDragUpdate: channel.master ||
                                      gainRange == null
                                  ? null
                                  : (d) => onGain(((channel.params['gain'] as num?)
                                              ?.toDouble() ??
                                          0) +
                                      d.delta.dx /
                                          gainBox.maxWidth *
                                          (gainRange!.last - gainRange!.first)),
                              child: CustomPaint(
                                  painter: _GainPainter(
                                      ConsolePalette.of(context),
                                      channel.params,
                                      channel.master,
                                      demo,
                                      gainRange)))))),
              place(channel.master ? 68 : 47, channel.master ? 58 : 79,
                  MixerEqPreview(channel: channel, onOpen: onOpen)),
              place(
                  132,
                  62,
                  CustomPaint(
                      painter: _DynamicsPainter(
                          ConsolePalette.of(context),
                          channel.params,
                          demo &&
                                  !channel.params.containsKey('gate_gr') &&
                                  !channel.params.containsKey('comp_gr')
                              ? mixerDemoStrips[channel.artwork].compression
                              : null))),
            ]));
      });
}

/// Shared EQ entry point. The compact variant gives phones the same access to
/// channel processing without spending fader travel on gain/dynamics previews.
class MixerEqPreview extends StatelessWidget {
  final MixerChannel channel;
  final bool compact;
  final VoidCallback onOpen;
  const MixerEqPreview({
    super.key,
    required this.channel,
    required this.onOpen,
    this.compact = false,
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
                        _EqPainter(ConsolePalette.of(context), channel.params)),
                if (compact)
                  Positioned(
                      top: 3,
                      left: 5,
                      child: IgnorePointer(
                          child: ExcludeSemantics(
                              child: Text('EQ',
                                  style: mixerType(context, 11,
                                          color:
                                              ConsolePalette.of(context).text,
                                          weight: FontWeight.w700)
                                      .copyWith(
                                          backgroundColor:
                                              ConsolePalette.of(context)
                                                  .eqBg))))),
                if (compact)
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
  final bool master, demo;
  final List<double>? gainRange;
  _GainPainter(
      this.palette, this.params, this.master, this.demo, this.gainRange);
  @override
  void paint(Canvas c, Size s) {
    c.save();
    c.scale(s.width / 168.2, s.height / 36);
    s = const Size(168.2, 36);
    mixerBox(c, Offset.zero & s, palette.panel, radius: 6);
    if (master) {
      for (var n = 0; n < 24; n++) {
        final w = (s.width - 22) / 8;
        mixerBox(c, Rect.fromLTWH(4 + n % 8 * (w + 2), 3 + n ~/ 8 * 6, w, 5),
            palette.lineCtl,
            radius: 1);
      }
      c.restore();
      return;
    }
    final width = s.width * .7;
    mixerBox(c, Rect.fromLTWH(3, 7, width, 11), palette.control,
        radius: 3, border: palette.lineCtl);
    final amount = gainRange == null
        ? 0.0
        : ((((params['gain'] as num?)?.toDouble() ?? gainRange!.first) -
                    gainRange!.first) /
                (gainRange!.last - gainRange!.first))
            .clamp(0.0, 1.0);
    mixerBox(
        c, Rect.fromLTWH(6, 10, (width - 7) * amount, 5), palette.meterCover,
        radius: 2, border: palette.lime);
    mixerBox(c, Rect.fromLTWH(4 + (width - 7) * amount, 8, 5, 9), palette.text4,
        radius: 1);
    // Design: 48V indicator replaces the A/B label (red when phantom is on).
    final phantom = params['phantom'] == true || params['phantom'] == 1;
    mixerText(c, '48V', Offset(s.width - 39, 7), 13,
        color: phantom ? palette.phantom : palette.mutedText,
        weight: FontWeight.w700);
    c.restore();
  }

  @override
  bool shouldRepaint(_GainPainter old) =>
      old.palette != palette ||
      old.master != master ||
      old.demo != demo ||
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
    c.save();
    c.scale(1, s.height / 62);
    s = Size(s.width, 62);
    mixerBox(c, Offset.zero & s, palette.panel, radius: 6);
    for (var i = 0; i < 2; i++) {
      final y = 7.0 + i * 23;
      mixerBox(c, Rect.fromLTWH(4, y, s.width - 14, 15), palette.well,
          radius: 3, border: palette.line, stroke: 2);
      final value = fixture != null && i < fixture!.length
          ? fixture![i] / 100
          : ((params[i == 0 ? 'gate_gr' : 'comp_gr'] as num?)
                      ?.toDouble()
                      .abs() ??
                  0) /
              30;
      final w = (s.width - 12) * value.clamp(0.0, 1.0);
      if (w > 0) {
        mixerBox(c, Rect.fromLTWH(s.width - 6 - w, y + 3, w, 9), palette.comp);
      }
    }
    c.restore();
  }

  @override
  bool shouldRepaint(_DynamicsPainter old) =>
      old.palette != palette ||
      !listEquals(old.fixture, fixture) ||
      !mapEquals(old.params, params);
}

class _EqPainter extends CustomPainter {
  final ConsolePalette palette;
  final Map<String, Object> params;
  _EqPainter(this.palette, this.params);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, palette.eqBg);
    final curve = Path();
    for (var x = 0.0; x <= s.width; x++) {
      final frequency = 20 * math.pow(1000, x / s.width);
      var gain = 0.0;
      if (params['eq_on'] != false) {
        for (var band = 1; band <= 4; band++) {
          final f = (params['eq${band}_freq'] as num?)?.toDouble() ?? 1000;
          final g = (params['eq${band}_gain'] as num?)?.toDouble() ?? 0;
          final q = (params['eq${band}_q'] as num?)?.toDouble() ?? 1;
          final distance = math.log(frequency / math.max(f, 1)) / math.ln2;
          gain += g * math.exp(-distance * distance * q * q / 2);
        }
      }
      final y = (s.height / 2 - gain * s.height / 36).clamp(0.0, s.height);
      if (x == 0) {
        curve.moveTo(x, y);
      } else {
        curve.lineTo(x, y);
      }
    }
    final fill = Path.from(curve)
      ..lineTo(s.width, s.height)
      ..lineTo(0, s.height)
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
      old.palette != palette || !mapEquals(old.params, params);
}
