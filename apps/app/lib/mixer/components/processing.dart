import 'dart:math' as math;
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import '../reference.dart';

/// Processing preview owns its rendering and gain gesture. Tapping its EQ
/// opens the existing channel editor; hardware curves use actual parameters.
class MixerProcessingRack extends StatelessWidget {
  final MixerChannel channel;
  final bool demo;
  final VoidCallback onOpen;
  final ValueChanged<double> onGain;
  const MixerProcessingRack(
      {super.key,
      required this.channel,
      required this.demo,
      required this.onOpen,
      required this.onGain});
  @override
  Widget build(BuildContext context) => DecoratedBox(
      decoration: BoxDecoration(
          color: MixerDesign.panel,
          borderRadius: MixerDesign.radius,
          border: Border.all(color: MixerDesign.line)),
      child: Padding(
          padding: const EdgeInsets.all(5),
          child: Column(children: [
            SizedBox(
                height: 24,
                child: LayoutBuilder(
                    builder: (context, box) => Semantics(
                        container: true,
                        label: '${channel.name} gain',
                        slider: !channel.master,
                        child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            onHorizontalDragUpdate: channel.master
                                ? null
                                : (d) => onGain(
                                    ((channel.params['gain'] as num?)
                                                ?.toDouble() ??
                                            0) +
                                        d.delta.dx / box.maxWidth * 72),
                            child: CustomPaint(
                                size: Size(box.maxWidth, 24),
                                painter: _GainPainter(
                                    channel.params, channel.master, demo)))))),
            const SizedBox(height: 5),
            Expanded(
                child: MixerEqPreview(
                    channel: channel, demo: demo, onOpen: onOpen)),
            const SizedBox(height: 5),
            SizedBox(
                height: 26,
                child: CustomPaint(
                    size: const Size(double.infinity, 26),
                    painter: _DynamicsPainter(
                        channel.params,
                        demo &&
                                !channel.params.containsKey('gate_gr') &&
                                !channel.params.containsKey('comp_gr')
                            ? masterFaderReference[channel.artwork].compression
                            : null))),
          ])));
}

/// Shared EQ entry point. The compact variant gives phones the same access to
/// channel processing without spending fader travel on gain/dynamics previews.
class MixerEqPreview extends StatelessWidget {
  final MixerChannel channel;
  final bool demo, compact;
  final VoidCallback onOpen;
  const MixerEqPreview({
    super.key,
    required this.channel,
    required this.demo,
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
                if (demo &&
                    !channel.params.keys.any((key) => key.startsWith('eq')))
                  MixerArtwork('eq_${channel.artwork}',
                      width: double.infinity,
                      height: double.infinity,
                      fit: BoxFit.fill)
                else
                  CustomPaint(painter: _EqPainter(channel.params)),
                if (compact)
                  Positioned(
                      top: 3,
                      left: 5,
                      child: IgnorePointer(
                          child: ExcludeSemantics(
                              child: Text('EQ',
                                  style: MixerDesign.type(9,
                                      color: MixerDesign.text,
                                      weight: FontWeight.w600))))),
                Material(
                    color: Colors.transparent,
                    child: InkWell(
                        onTap: onOpen,
                        overlayColor: WidgetStateProperty.resolveWith(
                            (states) => states.contains(WidgetState.pressed)
                                ? Colors.white.withValues(alpha: .16)
                                : null))),
              ]))));
}

class _GainPainter extends CustomPainter {
  final Map<String, Object> params;
  final bool master, demo;
  _GainPainter(this.params, this.master, this.demo);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, const Color(0xff181818), radius: 6);
    if (master) {
      for (var n = 0; n < 24; n++) {
        final w = (s.width - 22) / 8;
        mixerBox(c, Rect.fromLTWH(4 + n % 8 * (w + 2), 3 + n ~/ 8 * 6, w, 5),
            const Color(0xff555555),
            radius: 1);
      }
      return;
    }
    final width = s.width * .7;
    mixerBox(c, Rect.fromLTWH(3, 7, width, 11), const Color(0xff252525),
        radius: 3, border: const Color(0xff474747));
    final amount = ((((params['gain'] as num?)?.toDouble() ?? 0) + 12) / 72)
        .clamp(0.0, 1.0);
    mixerBox(c, Rect.fromLTWH(6, 10, (width - 7) * amount, 5),
        const Color(0xff31490d),
        radius: 2, border: const Color(0xff7a9f2b));
    mixerBox(c, Rect.fromLTWH(4 + (width - 7) * amount, 8, 5, 9),
        const Color(0xffa3a5a0),
        radius: 1);
    mixerText(c, 'A', Offset(s.width - 24, 7), 9,
        color: const Color(0xff737373), weight: FontWeight.w700);
    mixerText(c, 'B', Offset(s.width - 11, 7), 9,
        color: const Color(0xffbda91d), weight: FontWeight.w700);
  }

  @override
  bool shouldRepaint(_GainPainter old) =>
      old.master != master ||
      old.demo != demo ||
      !mapEquals(old.params, params);
}

class _DynamicsPainter extends CustomPainter {
  final Map<String, Object> params;
  final List<double>? fixture;
  _DynamicsPainter(this.params, this.fixture);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, const Color(0xff181818), radius: 6);
    for (var i = 0; i < 2; i++) {
      final y = 3.0 + i * 12;
      mixerBox(c, Rect.fromLTWH(4, y, s.width - 8, 8), const Color(0xff121212),
          radius: 3, border: const Color(0xff333333), stroke: 2);
      final value = fixture != null && i < fixture!.length
          ? fixture![i] / 100
          : ((params[i == 0 ? 'gate_gr' : 'comp_gr'] as num?)
                      ?.toDouble()
                      .abs() ??
                  0) /
              30;
      final w = (s.width - 12) * value.clamp(0.0, 1.0);
      if (w > 0) {
        mixerBox(c, Rect.fromLTWH(s.width - 6 - w, y + 2, w, 4),
            const Color(0xffff4b06));
      }
    }
  }

  @override
  bool shouldRepaint(_DynamicsPainter old) =>
      !listEquals(old.fixture, fixture) || !mapEquals(old.params, params);
}

class _EqPainter extends CustomPainter {
  final Map<String, Object> params;
  _EqPainter(this.params);
  @override
  void paint(Canvas c, Size s) {
    mixerBox(c, Offset.zero & s, const Color(0xff233306));
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
    c.drawPath(fill, Paint()..color = const Color(0xb5969d88));
    c.drawPath(
        curve,
        Paint()
          ..color = const Color(0xffeef2e9)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.4);
  }

  @override
  bool shouldRepaint(_EqPainter old) => !mapEquals(old.params, params);
}
