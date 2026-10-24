import 'dart:async';

import 'package:flutter/material.dart';
import '../palette.dart';

import '../u4law.dart';
import 'meter.dart';

enum FaderAccent { chan, master }

class U4Fader extends StatefulWidget {
  final double value;
  final String dbText;
  final FaderAccent accent;
  final Color? readColor;
  final bool fine;
  final ValueNotifier<double> meter;
  final ValueChanged<double>? onChanged;

  const U4Fader({
    super.key,
    required this.value,
    required this.dbText,
    required this.meter,
    this.accent = FaderAccent.chan,
    this.readColor,
    this.fine = false,
    this.onChanged,
  });

  @override
  State<U4Fader> createState() => _U4FaderState();
}

class _U4FaderState extends State<U4Fader> {
  // Drag bookkeeping (mirrors `drag` + commit throttle).
  Offset? _down;
  double _start = 0;
  bool _armed = false;
  bool _active = false;
  bool _downOnCap = false;
  double? _live;
  Size _size = Size.zero;

  double? _pending;
  Timer? _trailing;
  DateTime _lastCommit = DateTime.fromMillisecondsSinceEpoch(0);
  static const _commitMs = 100;

  Map<String, TextPainter> _labels = {};
  ConsolePalette? _labelPalette;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final palette = ConsolePalette.of(context);
    if (_labelPalette == palette) return;
    for (final label in _labels.values) {
      label.dispose();
    }
    _labelPalette = palette;
    _labels = {
      for (final mark in scaleMarks)
        mark.label: TextPainter(
          text: TextSpan(
            text: mark.label,
            style: TextStyle(
              color: palette.text7,
              fontSize: 10,
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
          textDirection: TextDirection.ltr,
        )..layout(),
    };
  }

  @override
  void dispose() {
    _trailing?.cancel();
    for (final label in _labels.values) {
      label.dispose();
    }
    super.dispose();
  }

  double get _trackH =>
      (_size.height - StripFaderGeo.insetT - StripFaderGeo.insetB)
          .clamp(1.0, double.infinity);
  double get _trackBottom => _size.height - StripFaderGeo.insetB;
  double get _shown {
    final v = _live ?? widget.value;
    return v.isFinite ? v.clamp(0.0, 1.0) : 0.0;
  }

  bool _hitCap(Offset p) {
    final cx = _size.width / 2;
    final cy = _trackBottom - _shown * _trackH;
    const pad = StripFaderGeo.grabPad;
    return (p.dx - cx).abs() <= StripFaderGeo.capW / 2 + pad &&
        (p.dy - cy).abs() <= StripFaderGeo.capH / 2 + pad;
  }

  void _commit(double v) {
    _pending = null;
    _lastCommit = DateTime.now();
    widget.onChanged?.call(v);
  }

  void _offer(double v) {
    if (widget.onChanged == null) return;
    if (DateTime.now().difference(_lastCommit).inMilliseconds >= _commitMs) {
      _trailing?.cancel();
      _trailing = null;
      _commit(v);
    } else {
      _pending = v;
      _trailing ??= Timer(const Duration(milliseconds: _commitMs), () {
        _trailing = null;
        final p = _pending;
        if (p != null) _commit(p);
      });
    }
  }

  void _endDrag() {
    _trailing?.cancel();
    _trailing = null;
    final p = _pending;
    _pending = null;
    if (p != null) {
      _lastCommit = DateTime.now();
      widget.onChanged?.call(p);
      _live = p;
    }
    _armed = false;
    _active = false;
    setState(() {});
  }

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onChanged != null;
    return Opacity(
      opacity: enabled ? 1 : 0.45,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            height: StripFaderGeo.readH,
            child: Center(
              child: Text(
                widget.dbText,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  color: widget.readColor ?? ConsolePalette.of(context).text4,
                  fontSize: StripFaderGeo.readFont,
                  fontFeatures: const [FontFeature.tabularFigures()],
                ),
              ),
            ),
          ),
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                _size = Size(constraints.maxWidth, constraints.maxHeight);
                return GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onVerticalDragDown: (d) {
                    if (!widget.value.isFinite || !enabled) return;
                    if (_armed) return; // second finger never steals the drag
                    _downOnCap = _hitCap(d.localPosition);
                    if (!_downOnCap) return; // off-cap: fall through to scroll
                    _down = d.localPosition;
                    _start = _live ?? widget.value;
                    _armed = true;
                    _active = false;
                  },
                  onVerticalDragUpdate: (d) {
                    if (!_armed || _down == null || !enabled) return;
                    final dx = d.localPosition.dx - _down!.dx;
                    final dyUp = _down!.dy - d.localPosition.dy;
                    if (!_active) {
                      final ax = dx.abs(), ay = dyUp.abs();
                      // Clearly horizontal -> abort, never touch the value.
                      if (ax > StripFaderGeo.lockH && ax > ay) {
                        _armed = false;
                        _down = null;
                        return;
                      }
                      if (ay <= StripFaderGeo.lockV || ay <= ax) return;
                      _active = true;
                    }
                    final v = faderFromDrag(_start, dyUp, _trackH, widget.fine);
                    _live = v;
                    setState(() {});
                    _offer(v);
                  },
                  onVerticalDragEnd: (_) => _endDrag(),
                  onVerticalDragCancel: _endDrag,
                  // Gated unity reset: double-tap snaps to 0 dB only when
                  // already near unity (~±3 dB window), like the gate.
                  onDoubleTap: () {
                    if (!enabled || !_downOnCap) return;
                    if ((widget.value - dbToPos(0)).abs() <= 0.08) {
                      widget.onChanged?.call(dbToPos(0));
                    }
                  },
                  child: Stack(
                    children: [
                      Positioned.fill(
                        child: CustomPaint(
                          painter: _FaderPainter(
                            palette: ConsolePalette.of(context),
                            value: _shown,
                            accent: widget.accent,
                            labels: _labels,
                          ),
                        ),
                      ),
                      Positioned(
                        right: 1,
                        top: 0,
                        bottom: 0,
                        width: 14,
                        child: U4Meter(level: widget.meter),
                      ),
                    ],
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class _FaderPainter extends CustomPainter {
  final ConsolePalette palette;
  final double value;
  final FaderAccent accent;
  final Map<String, TextPainter> labels;

  _FaderPainter(
      {required this.palette,
      required this.value,
      required this.accent,
      required this.labels});

  @override
  void paint(Canvas canvas, Size size) {
    final trackTop = StripFaderGeo.insetT;
    final trackH = size.height - trackTop - StripFaderGeo.insetB;
    if (trackH <= 0) return;
    final trackBottom = size.height - StripFaderGeo.insetB;

    for (final m in scaleMarks) {
      final y = trackBottom - (m.pct / 100) * trackH;
      final tp = labels[m.label]!;
      tp.paint(canvas, Offset(17 - tp.width, y - tp.height / 2));
      final tick = Paint()
        ..color = m.label == '0' ? palette.text1 : palette.scaleTick;
      canvas.drawRect(Rect.fromLTWH(19, y - 0.5, 7, 1), tick);
    }

    final cx = size.width / 2;
    final rail = RRect.fromRectAndRadius(
        Rect.fromLTWH(cx - 2, trackTop, 4, trackH), const Radius.circular(2));
    canvas.drawRRect(rail, Paint()..color = palette.railBg);
    canvas.drawRRect(
        rail,
        Paint()
          ..color = palette.railRing
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1);

    final capY = trackBottom - value * trackH;
    final cap = RRect.fromRectAndRadius(
        Rect.fromCenter(
            center: Offset(cx, capY),
            width: StripFaderGeo.capW,
            height: StripFaderGeo.capH),
        const Radius.circular(3));
    canvas.drawRRect(
        cap.shift(const Offset(0, 4)),
        Paint()
          ..color = palette.shadow.withValues(alpha: .75)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 4));
    final stops = [
      palette.knob,
      palette.cap,
      palette.control,
      palette.panel,
      palette.control,
      palette.cap,
      palette.knob,
    ];
    canvas.drawRRect(
        cap,
        Paint()
          ..shader = LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: stops,
            stops: const [0, 0.07, 0.44, 0.5, 0.56, 0.93, 1],
          ).createShader(cap.outerRect));
    canvas.drawRRect(
        cap,
        Paint()
          ..color = palette.knob.withValues(alpha: .28)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1);
    for (final top in [
      cap.outerRect.top + StripFaderGeo.gripTB,
      cap.outerRect.bottom - StripFaderGeo.gripTB - StripFaderGeo.gripH
    ]) {
      for (var i = 0; i < 4; i++) {
        final y = top + 2 + i * 3.5;
        canvas.drawLine(
            Offset(cap.outerRect.left + StripFaderGeo.gripLR, y),
            Offset(cap.outerRect.right - StripFaderGeo.gripLR, y),
            Paint()
              ..color = i.isEven
                  ? palette.knob.withValues(alpha: .17)
                  : palette.well.withValues(alpha: .4)
              ..strokeWidth = 1);
      }
    }
    final line =
        accent == FaderAccent.master ? palette.masterLine : palette.text1;
    canvas.drawRect(
        Rect.fromLTWH(cap.outerRect.left, capY - 4, StripFaderGeo.capW, 8),
        Paint()
          ..color = line.withValues(alpha: 0.35)
          ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 3));
    canvas.drawRect(
        Rect.fromLTWH(cap.outerRect.left, capY - 1.5, StripFaderGeo.capW, 3),
        Paint()..color = line);
  }

  @override
  bool shouldRepaint(_FaderPainter old) =>
      old.palette != palette || old.value != value || old.accent != accent;
}
