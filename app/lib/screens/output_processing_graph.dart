import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../core/destination_controls.dart';
import '../palette.dart';

/// Plots reported engineering values. PEQ shows band positions rather than
/// inventing a response curve from opaque provider filter-shape enums.
class OutputProcessingGraph extends StatelessWidget {
  final List<DestinationControlVm> controls;
  final String surface;
  const OutputProcessingGraph({
    super.key,
    required this.controls,
    required this.surface,
  });
  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final graph = _GraphData.from(controls, surface);
    final unavailableReason = switch (surface) {
      'output_peq' =>
        'Graph unavailable: this provider does not advertise paired frequency and gain ranges for its EQ bands.',
      'output_geq' =>
        'Graph unavailable: this provider does not advertise ranged Graphic EQ bands.',
      'output_comp' =>
        'Graph unavailable: this provider does not advertise the threshold, ratio, and makeup ranges needed for a transfer plot.',
      _ => 'Graph unavailable for this processor.',
    };
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text(
          surface == 'output_peq'
              ? 'Band positions · frequency / gain'
              : surface == 'output_geq'
                  ? 'Graphic EQ · band gain'
                  : 'Transfer reference · input / output',
          style: TextStyle(color: palette.mutedText),
        ),
        if (controls.any((control) => control.authority == 'accepted'))
          Text(
            'Includes unconfirmed values',
            style: TextStyle(color: palette.mutedText),
          ),
        const SizedBox(height: 8),
        if (graph == null)
          Container(
            height: 180,
            alignment: Alignment.center,
            color: palette.well,
            padding: const EdgeInsets.all(16),
            child: Text(
              unavailableReason,
              textAlign: TextAlign.center,
              style: TextStyle(color: palette.mutedText),
            ),
          )
        else
          SizedBox(
            height: 180,
            child: CustomPaint(painter: _GraphPainter(graph, palette)),
          ),
      ],
    );
  }
}

class _GraphBand {
  final String id;
  final double x;
  final double y;
  const _GraphBand(this.id, this.x, this.y);
}

class _GraphData {
  final String surface;
  final double xMin;
  final double xMax;
  final double yMin;
  final double yMax;
  final List<_GraphBand> bands;
  final double? threshold;
  final double? ratio;
  final double? makeup;

  const _GraphData({
    required this.surface,
    required this.xMin,
    required this.xMax,
    required this.yMin,
    required this.yMax,
    this.bands = const [],
    this.threshold,
    this.ratio,
    this.makeup,
  });

  static _GraphData? from(
    List<DestinationControlVm> controls,
    String surface,
  ) {
    if (surface == 'output_peq') {
      final byId = {for (final control in controls) control.id: control};
      final pairs = <({
        String id,
        DestinationControlVm freq,
        DestinationControlVm gain
      })>[];
      for (final freq in controls) {
        final match = RegExp(r'^(.+)_freq$').firstMatch(freq.id);
        if (match == null) continue;
        final id = match.group(1)!;
        final gain = byId['${id}_gain'];
        if (gain != null) pairs.add((id: id, freq: freq, gain: gain));
      }
      if (pairs.isEmpty ||
          pairs.any((pair) =>
              !pair.freq.numeric ||
              pair.freq.unit.toLowerCase() != 'hz' ||
              pair.freq.min! <= 0 ||
              pair.freq.value == null ||
              !pair.gain.numeric ||
              pair.gain.unit.toLowerCase() != 'db' ||
              pair.gain.value == null)) {
        return null;
      }
      final xMin = pairs.map((pair) => pair.freq.min!).reduce(math.min);
      final xMax = pairs.map((pair) => pair.freq.max!).reduce(math.max);
      final yMin = pairs.map((pair) => pair.gain.min!).reduce(math.min);
      final yMax = pairs.map((pair) => pair.gain.max!).reduce(math.max);
      return _GraphData(
        surface: surface,
        xMin: xMin,
        xMax: xMax,
        yMin: yMin,
        yMax: yMax,
        bands: [
          for (final pair in pairs)
            _GraphBand(pair.id, pair.freq.value!, pair.gain.value!),
        ],
      );
    }
    if (surface == 'output_geq') {
      final gains = controls
          .where((control) => RegExp(r'^geq\d+_gain$').hasMatch(control.id))
          .toList();
      if (gains.isEmpty ||
          gains.any((gain) =>
              !gain.numeric ||
              gain.unit.toLowerCase() != 'db' ||
              gain.value == null)) {
        return null;
      }
      return _GraphData(
        surface: surface,
        xMin: 0,
        xMax: gains.length.toDouble(),
        yMin: gains.map((gain) => gain.min!).reduce(math.min),
        yMax: gains.map((gain) => gain.max!).reduce(math.max),
        bands: [
          for (var i = 0; i < gains.length; i++)
            _GraphBand(gains[i].id, i + .5, gains[i].value!),
        ],
      );
    }
    if (surface == 'output_comp') {
      final byId = {for (final control in controls) control.id: control};
      final threshold = byId['comp_thr'];
      final ratio = byId['comp_ratio'];
      final makeup = byId['comp_makeup'];
      if (threshold == null ||
          !threshold.numeric ||
          threshold.unit.toLowerCase() != 'db' ||
          threshold.value == null ||
          ratio?.value == null ||
          ratio!.value! < 1 ||
          makeup == null ||
          !makeup.numeric ||
          makeup.unit.toLowerCase() != 'db' ||
          makeup.value == null) {
        return null;
      }
      return _GraphData(
        surface: surface,
        xMin: threshold.min!,
        xMax: threshold.max!,
        yMin: threshold.min! + makeup.min!,
        yMax: threshold.max! + makeup.max!,
        threshold: threshold.value,
        ratio: ratio.value,
        makeup: makeup.value,
      );
    }
    return null;
  }
}

class _GraphPainter extends CustomPainter {
  final _GraphData graph;
  final ConsolePalette palette;
  _GraphPainter(this.graph, this.palette);
  @override
  void paint(Canvas canvas, Size size) {
    final rect = Offset.zero & size;
    canvas.drawRect(rect, Paint()..color = palette.well);
    final grid = Paint()
      ..color = palette.line
      ..strokeWidth = 1;
    for (var i = 1; i < 6; i++) {
      canvas.drawLine(
        Offset(size.width * i / 6, 0),
        Offset(size.width * i / 6, size.height),
        grid,
      );
      canvas.drawLine(
        Offset(0, size.height * i / 6),
        Offset(size.width, size.height * i / 6),
        grid,
      );
    }
    final ink = Paint()
      ..color = palette.lime
      ..strokeWidth = 2
      ..style = PaintingStyle.stroke;
    double linearY(double value) =>
        size.height *
        (1 - ((value - graph.yMin) / (graph.yMax - graph.yMin)).clamp(0, 1));
    if (graph.surface == 'output_peq') {
      final logMin = math.log(graph.xMin);
      final logSpan = math.log(graph.xMax) - logMin;
      for (final band in graph.bands) {
        final point = Offset(
          size.width * ((math.log(band.x) - logMin) / logSpan).clamp(0, 1),
          linearY(band.y),
        );
        canvas.drawCircle(point, 5, ink);
        final text = TextPainter(
          text: TextSpan(
            text: band.id,
            style: TextStyle(color: palette.text, fontSize: 12),
          ),
          textDirection: TextDirection.ltr,
        )..layout();
        text.paint(canvas, point + const Offset(7, -8));
      }
    } else if (graph.surface == 'output_geq') {
      final width = size.width / graph.bands.length;
      for (var i = 0; i < graph.bands.length; i++) {
        final gain = graph.bands[i].y;
        canvas.drawLine(
          Offset(width * (i + .5), linearY(0)),
          Offset(width * (i + .5), linearY(gain)),
          ink..strokeWidth = math.max(2, width * .6),
        );
      }
    } else {
      final threshold = graph.threshold!;
      final ratio = graph.ratio!;
      final makeup = graph.makeup!;
      final path = Path();
      for (var i = 0; i <= 160; i++) {
        final input = graph.xMin + (graph.xMax - graph.xMin) * i / 160;
        final output = (input <= threshold
                ? input
                : threshold +
                    (ratio >= 101 ? 0 : (input - threshold) / ratio)) +
            makeup;
        final point = Offset(
          size.width * i / 160,
          linearY(output),
        );
        if (i == 0) {
          path.moveTo(point.dx, point.dy);
        } else {
          path.lineTo(point.dx, point.dy);
        }
      }
      canvas.drawPath(path, ink);
    }
  }

  @override
  bool shouldRepaint(covariant _GraphPainter oldDelegate) =>
      oldDelegate.palette != palette || oldDelegate.graph != graph;
}
