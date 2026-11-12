import 'package:material_ui/material_ui.dart';

import '../../palette.dart';
import '../../theme.dart';
import 'dart:math' as math;
import '../../design.dart';
import '../../core/channel_views.dart';
import '../../core/audio_spectrum_peaks.dart';
import '../../engine.dart' show AudioSpectrumFrame;
import 'graph_touch_surface.dart';

// The rendered summed response determines both paint and hit-test positions.
Map<int, Offset> _eqNodePositions(
    ChannelEqVm eq, Size size, List<double> response) {
  Offset point(double frequency) {
    final x = EqGraphGeom.xOf(frequency.clamp(20, 20000)) / EqGraphGeom.w;
    final index = x * (response.length - 1);
    final low =
        index.floor().clamp(0, math.max(0, response.length - 1)).toInt();
    final high = math.min(low + 1, response.length - 1);
    final gain = response.isEmpty
        ? 0.0
        : response[low] + (response[high] - response[low]) * (index - low);
    return Offset(
        x * size.width, EqGraphGeom.yOf(gain) / EqGraphGeom.h * size.height);
  }

  final anchors = <int, Offset>{
    for (final band in eq.bands) band.band: point(band.f),
    if (eq.hpfRange != null || eq.hpfOn) 0: point(eq.hpfF),
  };
  final order = [
    if (anchors.containsKey(eq.selectedBand)) eq.selectedBand,
    ...anchors.keys.where((band) => band != eq.selectedBand),
  ];
  return {for (final band in order) band: anchors[band]!};
}

/// Measured bands keep their frequency edges and dB scale; synthetic demo
/// magnitudes stay on their separate path.
class EqCurvePainter extends CustomPainter {
  final ConsolePalette palette;
  final List<double> responseDb; // EqGraphGeom.n + 1 samples
  final ChannelEqVm eq;
  final List<double>? rtaBins; // 0..1 magnitudes, RTA_BINS wide
  final bool rtaActive;
  final AudioSpectrumFrame? audioSpectrum;
  final AudioSpectrumPeaks audioSpectrumPeaks;
  final double rtaFloorDb;
  final double rtaCeilingDb;

  EqCurvePainter({
    required this.palette,
    required this.responseDb,
    required this.eq,
    this.rtaBins,
    this.rtaActive = false,
    this.audioSpectrum,
    this.audioSpectrumPeaks = const AudioSpectrumPeaks.empty(),
    this.rtaFloorDb = -90,
    this.rtaCeilingDb = 0,
  });

  static const _freqTicks = <double>[
    20,
    50,
    100,
    200,
    500,
    1000,
    2000,
    5000,
    10000,
    20000
  ];
  static const _dbTicks = <double>[15, 10, 5, 0, -5, -10, -15];

  @override
  void paint(Canvas canvas, Size size) {
    final sx = size.width / EqGraphGeom.w;
    final sy = size.height / EqGraphGeom.h;
    void label(String text, Offset position,
        {Color? color, double font = 10, bool centered = false}) {
      final painter = TextPainter(
          text: TextSpan(
              text: text,
              style: AppDesign.type(font,
                  color: color ?? palette.text7, weight: FontWeight.w500)),
          textDirection: TextDirection.ltr)
        ..layout();
      painter.paint(
          canvas,
          position -
              Offset(centered ? painter.width / 2 : 0, painter.height / 2));
    }

    final grid = Paint()
      ..color = palette.linePanel
      ..strokeWidth = 1;
    for (var decade = 20.0; decade < 20000; decade *= 10) {
      for (var step = 1; step < 10; step++) {
        final f = decade * step;
        if (f > 20000) continue;
        final x = EqGraphGeom.xOf(f) * sx;
        canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
      }
    }
    for (final db in _dbTicks) {
      final y = EqGraphGeom.yOf(db) * sy;
      canvas.drawLine(
          Offset(0, y),
          Offset(size.width, y),
          db == 0
              ? (Paint()..color = palette.text7.withValues(alpha: .35))
              : grid);
      label(db.toStringAsFixed(0), Offset(-24, y));
    }
    for (final f in _freqTicks) {
      if (size.width < 500 && ![20, 100, 1000, 10000, 20000].contains(f)) {
        continue;
      }
      final x = (EqGraphGeom.xOf(f) * sx).clamp(6.0, size.width - 8);
      label(
          f >= 1000
              ? '${(f / 1000).toStringAsFixed(0)}k'
              : f.toStringAsFixed(0),
          Offset(x, size.height + 16),
          centered: true);
    }
    canvas.save();
    canvas.clipRect(Offset.zero & size);
    // Shade each active filter using the same response as the combined curve.
    void drawContribution(List<double> samples, Color color) {
      final zeroY = EqGraphGeom.yOf(0) * sy;
      final area = Path()..moveTo(0, zeroY);
      for (var i = 0; i < samples.length; i++) {
        area.lineTo(i / (samples.length - 1) * size.width,
            EqGraphGeom.yOf(samples[i]) * sy);
      }
      area.lineTo(size.width, zeroY);
      area.close();
      canvas.drawPath(area, Paint()..color = color.withValues(alpha: .25));
    }

    if (eq.on) {
      if (eq.hpfOn && eq.hpfInResponse) {
        drawContribution(
            eqResponseDb(
                ChannelEqVm(hpfOn: true, hpfF: eq.hpfF, hpfSlope: eq.hpfSlope)),
            palette.bandColor(0));
      }
      for (final band in eq.bands.where((b) => b.on)) {
        drawContribution(eqResponseDb(ChannelEqVm(bands: [band])),
            palette.bandColor(band.band));
      }
    }
    // Reference HTML: adjacent outlined rectangles, one per frequency band.
    // The demo provides explicit synthetic bands; no smoothing or invented
    // frequency movement belongs in the painter.
    final frame = audioSpectrum;
    final bins = rtaBins;
    if (rtaActive && frame != null && rtaCeilingDb > rtaFloorDb) {
      // One block per measured band, clipped to the graph's 20 Hz–20 kHz
      // log-frequency axis. The analyser's dB scale is independent of EQ gain.
      for (var i = 0; i < frame.binsDb.length; i++) {
        final low = frame.bandEdgesHz[i].clamp(20.0, 20000.0);
        final high = frame.bandEdgesHz[i + 1].clamp(20.0, 20000.0);
        if (high <= low) continue;
        final left = EqGraphGeom.xOf(low) * sx;
        final right = EqGraphGeom.xOf(high) * sx;
        final height =
            ((frame.binsDb[i] - rtaFloorDb) / (rtaCeilingDb - rtaFloorDb))
                    .clamp(0.0, 1.0) *
                size.height;
        final gap = math.min(1.0, (right - left) / 2);
        final rect = RRect.fromRectAndRadius(
            Rect.fromLTWH(left + gap / 2, size.height - height,
                right - left - gap, height),
            const Radius.circular(.7));
        canvas.drawRRect(rect,
            Paint()..color = palette.eqPreviewFill.withValues(alpha: .28));
        canvas.drawRRect(
            rect,
            Paint()
              ..color = palette.rtaOutline
              ..style = PaintingStyle.stroke
              ..strokeWidth = .7);
      }
      if (audioSpectrumPeaks.matches(frame)) {
        final marker = Paint()
          ..color = palette.text1
          ..strokeWidth = 1;
        for (var i = 0; i < audioSpectrumPeaks.binsDb.length; i++) {
          final low = frame.bandEdgesHz[i].clamp(20.0, 20000.0);
          final high = frame.bandEdgesHz[i + 1].clamp(20.0, 20000.0);
          if (high <= low) continue;
          final left = EqGraphGeom.xOf(low) * sx;
          final right = EqGraphGeom.xOf(high) * sx;
          final gap = math.min(1.0, (right - left) / 2);
          final height = ((audioSpectrumPeaks.binsDb[i] - rtaFloorDb) /
                      (rtaCeilingDb - rtaFloorDb))
                  .clamp(0.0, 1.0) *
              size.height;
          final y = size.height - height;
          canvas.drawLine(
              Offset(left + gap / 2, y), Offset(right - gap / 2, y), marker);
        }
      }
    } else if (rtaActive && bins != null && bins.isNotEmpty) {
      final count = math.min(32, bins.length);
      final width = size.width / count;
      for (var bar = 0; bar < count; bar++) {
        final first = bar * bins.length ~/ count;
        final end = (bar + 1) * bins.length ~/ count;
        var peak = 0.0;
        for (var bin = first; bin < end; bin++) {
          peak = math.max(peak, bins[bin].clamp(0.0, 1.0));
        }
        final height = peak * size.height * .63;
        if (height <= 0) continue;
        final rect = RRect.fromRectAndRadius(
            Rect.fromLTWH(
                bar * width + .5, size.height - height, width - 1, height),
            const Radius.circular(.7));
        canvas.drawRRect(rect,
            Paint()..color = palette.eqPreviewFill.withValues(alpha: .28));
        canvas.drawRRect(
            rect,
            Paint()
              ..color = palette.rtaOutline
              ..style = PaintingStyle.stroke
              ..strokeWidth = .7);
      }
    }
    // Summed curve + fill to 0 dB.
    if (responseDb.isNotEmpty) {
      final curve = Path();
      final fill = Path();
      final zeroY = EqGraphGeom.yOf(0) * sy;
      for (var i = 0; i < responseDb.length; i++) {
        final x = EqGraphGeom.w * i / (responseDb.length - 1) * sx;
        final y = EqGraphGeom.yOf(responseDb[i]) * sy;
        if (i == 0) {
          curve.moveTo(x, y);
          fill.moveTo(x, zeroY);
          fill.lineTo(x, y);
        } else {
          curve.lineTo(x, y);
          fill.lineTo(x, y);
        }
      }
      final lastX = size.width;
      fill.lineTo(lastX, zeroY);
      fill.close();
      canvas.drawPath(
          fill, Paint()..color = palette.text7.withValues(alpha: .12));
      canvas.drawPath(
        curve,
        Paint()
          ..color = palette.text1
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    canvas.restore();
    final positions = _eqNodePositions(eq, size, responseDb);
    void node(Offset anchor, int band, bool on) {
      final point = positions[band] ?? anchor;
      final color = palette.bandColor(band);
      final radius = size.width < 500 ? 15.0 : 22.0;
      canvas.drawCircle(point, radius, Paint()..color = palette.chip);
      canvas.drawCircle(
          point,
          radius,
          Paint()
            ..color = color.withValues(alpha: on ? 1 : .35)
            ..style = PaintingStyle.stroke
            ..strokeWidth = eq.selectedBand == band ? 3 : 2);
      label(band == 0 ? 'HPF' : '$band', point,
          color: palette.text1, font: band == 0 ? 9 : 14, centered: true);
    }

    for (final band in [
      ...eq.bands.where((band) => band.band != eq.selectedBand),
      ...eq.bands.where((band) => band.band == eq.selectedBand),
    ]) {
      node(
          Offset(EqGraphGeom.xOf(band.f.clamp(20, 20000)) * sx,
              EqGraphGeom.yOf(band.graphGain.clamp(-15, 15)) * sy),
          band.band,
          band.on);
    }
    if (eq.hpfRange != null || eq.hpfOn) {
      node(
          Offset(EqGraphGeom.xOf(eq.hpfF.clamp(20, 20000)) * sx,
              EqGraphGeom.yOf(0) * sy),
          0,
          eq.hpfOn);
    }
  }

  @override
  bool shouldRepaint(covariant EqCurvePainter old) =>
      old.palette != palette ||
      old.responseDb != responseDb ||
      old.eq != eq ||
      old.audioSpectrum != audioSpectrum ||
      old.audioSpectrumPeaks != audioSpectrumPeaks ||
      old.audioSpectrum?.bandEdgesHz != audioSpectrum?.bandEdgesHz ||
      old.rtaFloorDb != rtaFloorDb ||
      old.rtaCeilingDb != rtaCeilingDb ||
      old.rtaBins != rtaBins ||
      old.rtaActive != rtaActive;
}

/// HPF drags edit frequency only.
class U4EqGraph extends StatelessWidget {
  final ChannelEqVm eq;
  final List<double> responseDb;
  final List<double>? rtaBins;
  final bool rtaActive;
  final AudioSpectrumFrame? audioSpectrum;
  final AudioSpectrumPeaks audioSpectrumPeaks;
  final double rtaFloorDb;
  final double rtaCeilingDb;
  final double height;
  final ValueChanged<int> onSelectBand;
  final ValueChanged<int>? onResetGain;
  final void Function(int band, double f, double g) onNode;
  final void Function(int band, double q) onQ;

  const U4EqGraph({
    super.key,
    required this.eq,
    required this.responseDb,
    this.rtaBins,
    this.rtaActive = false,
    this.audioSpectrum,
    this.audioSpectrumPeaks = const AudioSpectrumPeaks.empty(),
    this.rtaFloorDb = -90,
    this.rtaCeilingDb = 0,
    this.height = 344,
    required this.onSelectBand,
    this.onResetGain,
    required this.onNode,
    required this.onQ,
  });

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final outerWidth =
            constraints.maxWidth.isFinite ? constraints.maxWidth : 340.0;
        final w = outerWidth - 44;
        final graphHeight =
            constraints.hasBoundedHeight ? constraints.maxHeight : height;
        final h = graphHeight - 44;
        return Semantics(
            container: true,
            label: 'EQ graph',
            hint:
                'Drag a band to change frequency and gain. Pinch to change bandwidth.',
            child: Container(
                height: height,
                padding: const EdgeInsets.fromLTRB(28, 14, 16, 30),
                decoration: BoxDecoration(
                    color: ConsolePalette.of(context).panel,
                    borderRadius: U4.rWell,
                    border: Border.all(
                        color: ConsolePalette.of(context).linePanel)),
                child: _EqGraphGesture(
                  width: w,
                  height: h,
                  eq: eq,
                  responseDb: responseDb,
                  onResetGain: onResetGain,
                  onSelectBand: onSelectBand,
                  onNode: onNode,
                  onQ: onQ,
                  child: CustomPaint(
                    size: Size(w, h),
                    painter: EqCurvePainter(
                      palette: ConsolePalette.of(context),
                      responseDb: responseDb,
                      eq: eq,
                      rtaBins: rtaBins,
                      audioSpectrum: audioSpectrum,
                      audioSpectrumPeaks: audioSpectrumPeaks,
                      rtaFloorDb: rtaFloorDb,
                      rtaCeilingDb: rtaCeilingDb,
                      rtaActive: rtaActive,
                    ),
                  ),
                )));
      },
    );
  }
}

class _EqGraphGesture extends StatefulWidget {
  final List<double> responseDb;
  final double width;
  final double height;
  final ChannelEqVm eq;
  final ValueChanged<int> onSelectBand;
  final ValueChanged<int>? onResetGain;
  final void Function(int band, double f, double g) onNode;
  final void Function(int band, double q) onQ;
  final Widget child;

  const _EqGraphGesture({
    required this.responseDb,
    required this.width,
    required this.height,
    required this.eq,
    required this.onSelectBand,
    this.onResetGain,
    required this.onNode,
    required this.onQ,
    required this.child,
  });

  @override
  State<_EqGraphGesture> createState() => _EqGraphGestureState();
}

class _EqGraphGestureState extends State<_EqGraphGesture> {
  final Map<int, Offset> _pointers = {};
  int? _dragBand;
  double _f = 20;
  double _g = 0;
  double _q = 1;
  double? _span;
  Duration? _lastTapTime;
  int? _lastTapBand;
  Offset? _tapStart;
  bool _tapCandidate = false;
  Duration? _tapDownTime;

  EqBandVm? get _band {
    for (final band in widget.eq.bands) {
      if (band.band == _dragBand) return band;
    }
    return null;
  }

  int? _hitBand(Offset local) {
    int? best;
    var bestD = 30.0;
    final positions = _eqNodePositions(
        widget.eq, Size(widget.width, widget.height), widget.responseDb);
    for (final entry in positions.entries) {
      final d = (entry.value - local).distance;
      if (d < bestD && !(best == widget.eq.selectedBand && d < 30)) {
        bestD = d;
        best = entry.key;
      }
    }
    return best;
  }

  void _focus(int? band) {
    _dragBand = band;
    if (band == null) return;
    if (band != 0 && _band == null) {
      _dragBand = null;
      return;
    }
    _f = band == 0 ? widget.eq.hpfF : (_band?.f ?? 20);
    _g = band == 0 ? 0 : (_band?.g ?? 0);
    _q = _band?.q ?? 1;
    widget.onSelectBand(band);
  }

  double? get _pointerSpan {
    if (_pointers.length != 2) return null;
    return (_pointers.values.first - _pointers.values.last).distance;
  }

  void _down(PointerDownEvent event) {
    if (_pointers.isEmpty) {
      _focus(_hitBand(event.localPosition));
      _tapStart = event.localPosition;
      _tapCandidate = true;
      _tapDownTime = event.timeStamp;
    } else {
      _tapCandidate = false;
      _lastTapTime = null;
    }
    _pointers[event.pointer] = event.localPosition;
    if (_pointers.length == 2 && _dragBand == null) {
      _focus(widget.eq.selectedBand);
    }
    _span = _pointerSpan;
  }

  void _move(PointerMoveEvent event) {
    final previous = _pointers[event.pointer];
    if (previous == null) return;
    _pointers[event.pointer] = event.localPosition;
    if (_tapStart != null && (event.localPosition - _tapStart!).distance > 12) {
      _tapCandidate = false;
    }
    if (_tapCandidate && _pointers.length == 1) return;
    final band = _dragBand;
    if (band == null) return;
    if (_pointers.length == 2) {
      final span = _pointerSpan!;
      final oldSpan = _span;
      _span = span;
      if (band == 0 || oldSpan == null || oldSpan < 1 || span < 1) return;
      final range = _band?.qRange;
      if (range == null) return;
      // Spread fingers to broaden the band (lower Q).
      _q = (_q * oldSpan / span).clamp(range.first, range.last);
      widget.onQ(band, _q);
    } else if (_pointers.length == 1 && widget.width > 0 && widget.height > 0) {
      final delta = event.localPosition - previous;
      final fRange = band == 0 ? widget.eq.hpfRange : _band?.fRange;
      final gRange = band == 0 ? null : _band?.gRange;
      if (fRange == null && gRange == null) return;
      if (fRange != null) {
        _f = (_f * math.pow(1000, delta.dx / widget.width))
            .clamp(fRange.first, fRange.last)
            .toDouble();
      }
      if (gRange != null) {
        _g = (_g - delta.dy / widget.height * 30)
            .clamp(gRange.first, gRange.last);
      }
      widget.onNode(band, _f, _g);
    }
  }

  void _up(PointerUpEvent event) {
    final band = _dragBand;
    final now = event.timeStamp;
    final quickTap = _tapDownTime != null &&
        now - _tapDownTime! < const Duration(milliseconds: 300);
    if (_tapCandidate && quickTap && band != null && band != 0) {
      final range = _band?.gRange;
      if (_lastTapBand == band &&
          _lastTapTime != null &&
          now - _lastTapTime! < const Duration(milliseconds: 350) &&
          range != null &&
          range.first <= 0 &&
          range.last >= 0) {
        widget.onResetGain?.call(band);
        _lastTapTime = null;
      } else {
        _lastTapBand = band;
        _lastTapTime = now;
      }
    } else {
      _lastTapTime = null;
    }
    _tapCandidate = false;
    _pointers.remove(event.pointer);
    _span = _pointerSpan;
    if (_pointers.isEmpty) _dragBand = null;
  }

  void _cancel(PointerCancelEvent event) {
    _lastTapTime = null;
    _tapCandidate = false;
    _pointers.clear();
    _dragBand = null;
    _span = null;
  }

  @override
  Widget build(BuildContext context) {
    return U4GraphTouchSurface(
      onDown: _down,
      onMove: _move,
      onUp: _up,
      onCancel: _cancel,
      child: widget.child,
    );
  }
}
