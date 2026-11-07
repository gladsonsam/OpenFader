import 'dart:math' as math;
import 'package:flutter/material.dart';
import '../palette.dart';
import 'recorder.dart';

/// A shared time axis for all capture channels. No page-owned ticker: the
/// shell's recorder store supplies each new transport and meter snapshot.
class RecorderArrangement extends StatefulWidget {
  const RecorderArrangement(
      {super.key,
      required this.store,
      required this.previewTakeId,
      required this.onLoopSelection});
  final void Function(int start, int end, int total) onLoopSelection;
  final RecorderStore store;
  final String? previewTakeId;
  @override
  State<RecorderArrangement> createState() => _RecorderArrangementState();
}

class _RecorderArrangementState extends State<RecorderArrangement> {
  double _zoom = 1;
  bool _followRecording = true;
  double? _loopStart;
  double? _loopEnd;
  static const laneHeight = 62.0;
  final _horizontal = ScrollController();
  String? _requestKey;
  String? _desiredKey;
  bool _loadingPeaks = false;
  List<Map<String, dynamic>> _peaks = [];
  List<Map<String, dynamic>> _overviewPeaks = [];
  String? _overviewKey;
  String? _peakError;

  @override
  void initState() {
    super.initState();
    _horizontal.addListener(_onScroll);
  }

  @override
  void didUpdateWidget(covariant RecorderArrangement oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.previewTakeId != widget.previewTakeId) {
      _peaks = [];
      _overviewPeaks = [];
      _overviewKey = null;
      _peakError = null;
      _requestKey = null;
      _desiredKey = null;
      _loopStart = null;
      _loopEnd = null;
      _followRecording = true;
    }
  }

  void _onScroll() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _horizontal.dispose();
    super.dispose();
  }

  void _requestVisible(
      String? id, double frames, double width, double viewport, int takeFrames,
      {bool live = false, int refreshFrames = 1}) {
    if (id == null || takeFrames <= 0 || viewport <= 0) return;
    final offset = _horizontal.hasClients ? _horizontal.offset : 0.0;
    final visibleStart = (offset / width * frames).floor().clamp(0, takeFrames);
    final start =
        live ? visibleStart ~/ refreshFrames * refreshFrames : visibleStart;
    final published =
        live ? takeFrames ~/ refreshFrames * refreshFrames : takeFrames;
    final end =
        ((offset + viewport) / width * frames).ceil().clamp(0, published);
    if (end <= start) return;
    final key = '$id:$start:$end:${viewport.round()}';
    if (_desiredKey != key && !live) {
      _peaks = [];
      _peakError = null;
    }
    _desiredKey = key;
    if (key == _requestKey || _loadingPeaks) return;
    _requestKey = key;
    _loadingPeaks = true;
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      try {
        final chunks = <Map<String, dynamic>>[];
        for (var from = start; from < end; from += 1000000) {
          final to = math.min(end, from + 1000000);
          final buckets = math.min(
              to - from, math.max(1, ((to - from) / frames * width).ceil()));
          chunks.add(await widget.store
              .waveform(id, from, to, buckets.clamp(1, 2048)));
        }
        if (mounted &&
            (_desiredKey == key || live && widget.previewTakeId == id)) {
          setState(() {
            _peaks = chunks;
            _peakError = null;
          });
        }
      } catch (_) {
        if (mounted && _desiredKey == key) {
          setState(() {
            _peaks = [];
            _peakError = live
                ? 'Live waveforms are unavailable; recording continues.'
                : 'Waveform peaks are unavailable for this take.';
          });
        }
      } finally {
        _loadingPeaks = false;
        if (mounted && _desiredKey != key) setState(() {});
      }
    });
  }

  void _requestOverview(String? id, int frames) {
    final key = '$id:$frames';
    if (id == null || frames <= 0 || key == _overviewKey) return;
    _overviewKey = key;
    _overviewPeaks = [];
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      try {
        final chunks = <Map<String, dynamic>>[];
        for (var from = 0; from < frames; from += 1000000) {
          if (!mounted || _overviewKey != key) return;
          final to = math.min(frames, from + 1000000);
          chunks.add(await widget.store.waveform(
              id, from, to, math.max(1, ((to - from) / frames * 512).ceil())));
        }
        if (mounted && _overviewKey == key) {
          setState(() => _overviewPeaks = chunks);
        }
      } catch (_) {
        // An unavailable overview stays empty; the main waveform has its own
        // retry action and never substitutes synthetic samples.
      }
    });
  }

  Color _channelColor(ConsolePalette p, int i) {
    final colors = [
      p.arrangementGold,
      p.meter,
      p.arrangementBlue,
      p.arrangementOrange,
      p.arrangementPurple,
      p.arrangementGrey
    ];
    return colors[i % colors.length];
  }

  @override
  Widget build(BuildContext context) {
    final p = ConsolePalette.of(context);
    final s = widget.store.state;
    final caps = s['capabilities'] is Map ? s['capabilities'] as Map : const {};
    final recording = s['transport'] == 'recording';
    final take = widget.store.takes
        .where((t) => t['complete'] == true && t['id'] == widget.previewTakeId)
        .firstOrNull;
    final laneSource = take ??
        (widget.previewTakeId == null ||
                '${s['transport']}'.toLowerCase().contains('record')
            ? s
            : const <String, dynamic>{});
    final names = recorderChannelNames(laneSource);
    final knownRate = (take?['sample_rate'] as num?)?.toDouble() ??
        (s['sample_rate'] as num?)?.toDouble();
    final rateKnown = knownRate != null && knownRate > 0;
    final laneCount = rateKnown ? recorderChannelCount(laneSource) : 0;
    // Arithmetic placeholder only; no tracks or sample-rate claim until known.
    final rate = rateKnown ? knownRate : 1.0;
    final active = '${s['input_mode']}'.toLowerCase().contains('soundcheck') ||
        '${s['transport']}'.toLowerCase().contains('record');
    final prepared =
        '${s['input_mode']}'.toLowerCase().contains('soundcheck') &&
            widget.previewTakeId == s['selected_take_id'];
    final recordedFrames = (s['recorded_frames'] as num?)?.toInt() ?? 0;
    final frames = recording
        ? math.max(
            rate * 30, ((recordedFrames + 1) / (rate * 30)).ceil() * rate * 30)
        : math.max(
            1.0,
            (take?['duration_frames'] as num?)?.toDouble() ??
                (active ? (s['recorded_frames'] as num?)?.toDouble() : null) ??
                rate * 30);
    final position = (s['transport'] == 'recording'
        ? s['recorded_frames']
        : s['position_frames']) as num?;
    final current =
        (active ? (position?.toDouble() ?? 0) : 0.0).clamp(0.0, frames);
    final enabled = widget.store.connected && !widget.store.pending;
    final canSeek = enabled && prepared && caps['seek'] == true;
    final canLoop = enabled && prepared && caps['loop'] == true;
    final meters = s['live_peaks'] is List ? s['live_peaks'] as List : const [];
    return Container(
      decoration: BoxDecoration(
          color: p.panel,
          border: Border.all(color: p.linePanel),
          borderRadius: BorderRadius.zero),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: SizedBox(
                height: 60,
                child: Row(
                    crossAxisAlignment: CrossAxisAlignment.center,
                    children: [
                      Text('Arrangement',
                          style: TextStyle(
                              color: p.text1,
                              fontWeight: FontWeight.w600,
                              fontSize: 24)),
                      const SizedBox(width: 20),
                      Text('$laneCount tracks',
                          style: TextStyle(color: p.mutedText, fontSize: 20)),
                      const SizedBox(width: 20),
                      if (_peakError != null)
                        Tooltip(
                            message: _peakError!,
                            child: TextButton(
                                onPressed: () => setState(() {
                                      _requestKey = null;
                                      _peakError = null;
                                    }),
                                child: const Text('Retry waveform'))),
                      if (take != null)
                        TextButton(
                            onPressed: () async {
                              final messenger = ScaffoldMessenger.of(context);
                              try {
                                final data = await widget.store
                                    .inspectTake(take['id'] as String);
                                if (!context.mounted) return;
                                final manifest = data['manifest'] as Map;
                                await showDialog<void>(
                                    context: context,
                                    builder: (context) => AlertDialog(
                                          title: Text(
                                              'Take details · ${take['id']}'),
                                          content: SizedBox(
                                              width: 600,
                                              child: SingleChildScrollView(
                                                  child: Column(
                                                      crossAxisAlignment:
                                                          CrossAxisAlignment
                                                              .start,
                                                      children: [
                                                    Text(
                                                        'Manifest: ${manifest['frames']} frames · ${manifest['sample_rate']} Hz · ${manifest['channels']} channels · complete ${manifest['complete']}'),
                                                    for (final h
                                                        in data['wav_headers']
                                                            as List)
                                                      Text(
                                                          'WAV ${h['file']}: ${h['channels']} channels · ${h['bits_per_sample']} bit · ${h['sample_rate']} Hz · ${h['frames']} frames'),
                                                    Text(
                                                        'Measured frames ${data['start_frame']}–${data['end_frame']} (end exclusive), normalized PCM'),
                                                    for (final c
                                                        in data['channels']
                                                            as List)
                                                      SelectableText(
                                                          'Ch ${c['channel']}: min ${(c['min'] as num).toStringAsFixed(8)}  max ${(c['max'] as num).toStringAsFixed(8)}  RMS ${(c['rms'] as num).toStringAsFixed(8)}'),
                                                  ]))),
                                          actions: [
                                            TextButton(
                                                onPressed: () =>
                                                    Navigator.pop(context),
                                                child: const Text('Close'))
                                          ],
                                        ));
                              } catch (e) {
                                messenger.showSnackBar(SnackBar(
                                    content: Text('Inspection failed: $e')));
                              }
                            },
                            child: const Text('Take details')),
                      const Spacer(),
                      if (recording)
                        IconButton(
                            tooltip: _followRecording
                                ? 'Following recording · click to scroll freely'
                                : 'Follow recording playhead',
                            isSelected: _followRecording,
                            onPressed: () => setState(
                                () => _followRecording = !_followRecording),
                            icon: const Icon(Icons.follow_the_signs)),
                      Text('Time zoom', style: TextStyle(color: p.mutedText)),
                      SizedBox(
                          width: 140,
                          child: Slider(
                              value: _zoom,
                              min: 1,
                              max: 8,
                              onChanged: (v) => setState(() => _zoom = v))),
                      TextButton(
                          onPressed: () {
                            setState(() => _zoom = 1);
                            if (_horizontal.hasClients) _horizontal.jumpTo(0);
                          },
                          child: const Text('Fit')),
                    ]))),
        if (laneCount == 0)
          Expanded(
              child: Center(
                  child: Text(
                      take == null
                          ? 'Configure an audio interface to see capture tracks.'
                          : 'Track layout is unavailable for this take.',
                      style: TextStyle(color: p.mutedText))))
        else
          Expanded(child: LayoutBuilder(builder: (context, constraints) {
            final viewport = math.max(1.0, constraints.maxWidth - 300);
            final width = math.max(
                600.0,
                recording
                    ? frames / rate / 30 * viewport * _zoom
                    : viewport * _zoom);
            _requestVisible(
                recording
                    ? s['selected_take_id'] as String?
                    : take?['id'] as String?,
                frames,
                width,
                constraints.maxWidth - 300,
                recording
                    ? recordedFrames
                    : (take?['duration_frames'] as num?)?.toInt() ?? 0,
                live: recording,
                refreshFrames: math.max(1, (rate / 4).round()));
            if (recording && _followRecording) {
              WidgetsBinding.instance.addPostFrameCallback((_) {
                if (!mounted || !_horizontal.hasClients || !_followRecording) {
                  return;
                }
                final target = (current / frames * width - viewport * .8)
                    .clamp(0.0, _horizontal.position.maxScrollExtent);
                if ((_horizontal.offset - target).abs() > .5) {
                  _horizontal.jumpTo(target);
                }
              });
            }
            _requestOverview(recording ? null : take?['id'] as String?,
                (take?['duration_frames'] as num?)?.toInt() ?? 0);
            return SingleChildScrollView(
                scrollDirection: Axis.vertical,
                child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                          width: 300,
                          child: Column(children: [
                            SizedBox(
                                height: 56,
                                child: Center(
                                    child: Text(
                                        'Track · captures available interface inputs\nPer-track arm, mute and solo unavailable',
                                        textAlign: TextAlign.center,
                                        style: TextStyle(
                                            color: p.mutedText,
                                            fontSize: 14)))),
                            for (var i = 0; i < laneCount; i++)
                              Container(
                                  height: laneHeight,
                                  padding: const EdgeInsets.only(right: 8),
                                  decoration: BoxDecoration(
                                      color: p.arrangementTrack,
                                      border: Border(
                                          top: BorderSide(color: p.line))),
                                  child: Stack(children: [
                                    Row(children: [
                                      Container(
                                          width: 7, color: _channelColor(p, i)),
                                      const SizedBox(width: 12),
                                      Text((i + 1).toString(),
                                          style: TextStyle(color: p.mutedText)),
                                      const SizedBox(width: 8),
                                      Expanded(
                                          child: Text(
                                              names[i] ?? 'Channel ${i + 1}',
                                              overflow: TextOverflow.ellipsis,
                                              style: TextStyle(
                                                  color: p.text1,
                                                  fontSize: 22,
                                                  fontWeight:
                                                      FontWeight.w600))),
                                    ]),
                                    Positioned(
                                        left: 7,
                                        right: 0,
                                        bottom: 0,
                                        height: 4,
                                        child: LayoutBuilder(
                                            builder: (context, box) =>
                                                Stack(children: [
                                                  ColoredBox(
                                                      color: p.slot,
                                                      child: SizedBox(
                                                          width: box.maxWidth,
                                                          height: 4)),
                                                  SizedBox(
                                                      width: box.maxWidth *
                                                          (i < meters.length &&
                                                                  meters[i]
                                                                      is num
                                                              ? (meters[i]
                                                                      as num)
                                                                  .toDouble()
                                                                  .clamp(0, 1)
                                                              : 0),
                                                      height: 4,
                                                      child: ColoredBox(
                                                          color: _channelColor(
                                                              p, i))),
                                                ]))),
                                  ])),
                          ])),
                      Expanded(
                          child: SingleChildScrollView(
                              controller: _horizontal,
                              scrollDirection: Axis.horizontal,
                              child: SizedBox(
                                  width: width,
                                  height: 56 + laneHeight * laneCount,
                                  child: GestureDetector(
                                    behavior: HitTestBehavior.opaque,
                                    onTapUp: canSeek
                                        ? (d) => widget.store.command('seek', {
                                              'position_frames':
                                                  (d.localPosition.dx /
                                                          width *
                                                          frames)
                                                      .round()
                                            })
                                        : null,
                                    onHorizontalDragStart: canLoop
                                        ? (d) => setState(() {
                                              _loopStart =
                                                  (d.localPosition.dx / width)
                                                      .clamp(0, 1);
                                              _loopEnd = _loopStart;
                                            })
                                        : null,
                                    onHorizontalDragUpdate: canLoop
                                        ? (d) => setState(() => _loopEnd =
                                            (d.localPosition.dx / width)
                                                .clamp(0, 1))
                                        : null,
                                    onHorizontalDragEnd: canLoop
                                        ? (_) {
                                            if (_loopStart != null &&
                                                _loopEnd != null) {
                                              widget.onLoopSelection(
                                                  (math.min(_loopStart!,
                                                              _loopEnd!) *
                                                          frames)
                                                      .round(),
                                                  (math.max(_loopStart!,
                                                              _loopEnd!) *
                                                          frames)
                                                      .round(),
                                                  frames.round());
                                            }
                                          }
                                        : null,
                                    child: CustomPaint(
                                        painter: _TimelinePainter(
                                            palette: p,
                                            frames: frames,
                                            rate: rate,
                                            position: current,
                                            visibleStart:
                                                (_horizontal.hasClients
                                                        ? _horizontal.offset
                                                        : 0) /
                                                    width,
                                            visibleEnd: ((_horizontal.hasClients
                                                        ? _horizontal.offset
                                                        : 0) +
                                                    viewport) /
                                                width,
                                            clipFrames: recording
                                                ? recordedFrames.toDouble()
                                                : frames,
                                            peaks: _peaks,
                                            names: names,
                                            takeName: recording
                                                ? 'Recording'
                                                : take == null
                                                    ? null
                                                    : ((take['name']
                                                                    is String &&
                                                                (take['name']
                                                                        as String)
                                                                    .trim()
                                                                    .isNotEmpty)
                                                            ? take['name']
                                                            : 'Take ${widget.store.takes.indexOf(take) + 1}')
                                                        .toString(),
                                            colors: List.generate(laneCount,
                                                (i) => _channelColor(p, i)),
                                            loopStart: _loopStart,
                                            loopEnd: _loopEnd)),
                                  )))),
                    ]));
          })),
        SizedBox(
            height: 72,
            child: LayoutBuilder(builder: (context, box) {
              final view = math.max(1.0, box.maxWidth - 300);
              final total = math.max(600.0, view * _zoom);
              final offset = _horizontal.hasClients ? _horizontal.offset : 0.0;
              void move(double x) {
                if (_horizontal.hasClients) {
                  _horizontal.jumpTo(
                      (x / math.max(1, box.maxWidth - 160) * total - view / 2)
                          .clamp(0.0, _horizontal.position.maxScrollExtent));
                }
              }

              return Row(children: [
                const SizedBox(width: 18),
                Text('00:00', style: TextStyle(color: p.mutedText)),
                const SizedBox(width: 12),
                Expanded(
                    child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTapDown: (d) => move(d.localPosition.dx),
                        onHorizontalDragUpdate: (d) => move(d.localPosition.dx),
                        child: SizedBox(
                            height: 44,
                            child: CustomPaint(
                                painter: _OverviewPainter(
                                    p,
                                    _overviewPeaks,
                                    frames,
                                    offset / total,
                                    (view / total).clamp(0, 1),
                                    current / frames,
                                    laneCount))))),
                const SizedBox(width: 12),
                Text(
                    '${(frames / rate ~/ 60).toString().padLeft(2, '0')}:${(frames / rate % 60).floor().toString().padLeft(2, '0')}',
                    style: TextStyle(color: p.mutedText)),
                const SizedBox(width: 18),
              ]);
            })),
      ]),
    );
  }
}

class _TimelinePainter extends CustomPainter {
  const _TimelinePainter(
      {required this.palette,
      required this.frames,
      required this.rate,
      required this.position,
      required this.clipFrames,
      required this.visibleStart,
      required this.visibleEnd,
      required this.peaks,
      required this.names,
      required this.takeName,
      required this.colors,
      this.loopStart,
      this.loopEnd});
  final ConsolePalette palette;
  final Map<int, String> names;
  final String? takeName;
  final List<Color> colors;
  final double frames, rate, position, clipFrames, visibleStart, visibleEnd;
  final double? loopStart, loopEnd;
  final List<Map<String, dynamic>> peaks;
  @override
  void paint(Canvas canvas, Size size) {
    final background = Paint()..color = palette.well;
    canvas.drawRect(Offset.zero & size, background);
    for (var i = 0; i < colors.length; i++) {
      final y = 56.0 + i * _RecorderArrangementState.laneHeight;
      canvas.drawRect(
          Rect.fromLTWH(0, y, size.width, 62),
          Paint()
            ..color = i.isEven
                ? palette.arrangementLane
                : palette.arrangementLaneAlt);
      if (takeName != null) {
        final clipWidth =
            (clipFrames / frames * size.width).clamp(0.0, size.width);
        final rect = Rect.fromLTWH(0, y + 5, clipWidth, 52);
        canvas.drawRRect(
            RRect.fromRectAndRadius(rect, const Radius.circular(5)),
            Paint()..color = colors[i].withValues(alpha: .18));
        canvas.drawRect(
            Rect.fromLTWH(0, y + 5, clipWidth, 15), Paint()..color = colors[i]);
        final label = TextPainter(
            text: TextSpan(
                text: '${names[i] ?? 'Channel ${i + 1}'} · $takeName',
                style: TextStyle(
                    color: palette.onTape,
                    fontSize: 13,
                    fontWeight: FontWeight.w600)),
            textDirection: TextDirection.ltr,
            maxLines: 1,
            ellipsis: '…')
          ..layout(maxWidth: math.max(0, clipWidth - 14));
        if (clipWidth > 14) label.paint(canvas, Offset(7, y + 5));
      }
    }
    if (loopStart != null && loopEnd != null) {
      canvas.drawRect(
          Rect.fromLTRB(math.min(loopStart!, loopEnd!) * size.width, 44,
              math.max(loopStart!, loopEnd!) * size.width, 56),
          Paint()..color = palette.lime);
      canvas.drawRect(
          Rect.fromLTRB(math.min(loopStart!, loopEnd!) * size.width, 0,
              math.max(loopStart!, loopEnd!) * size.width, size.height),
          Paint()..color = palette.selectionFill);
    }
    final grid = Paint()
      ..color = palette.line
      ..strokeWidth = 1;
    for (var i = 0; i <= colors.length; i++) {
      final y = 56.0 + i * _RecorderArrangementState.laneHeight;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), grid);
    }
    final divisions = math.max(1, (size.width / 120).floor());
    final firstDivision =
        (visibleStart * divisions).floor().clamp(0, divisions);
    final lastDivision = (visibleEnd * divisions).ceil().clamp(0, divisions);
    for (var i = firstDivision; i <= lastDivision; i++) {
      final x = size.width * i / divisions;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
      final sec = (frames / rate * i / divisions).floor();
      final text = TextPainter(
          text: TextSpan(
              text:
                  '${(sec ~/ 60).toString().padLeft(2, '0')}:${(sec % 60).toString().padLeft(2, '0')}',
              style: TextStyle(color: palette.recorderMuted, fontSize: 18)),
          textDirection: TextDirection.ltr)
        ..layout();
      text.paint(canvas, Offset(x + 4, 9));
    }
    final wave = Paint()
      ..color = palette.meter
      ..strokeWidth = 1;
    for (final chunk in peaks) {
      final channels = chunk['channels'];
      final start = chunk['start_frame'];
      final end = chunk['end_frame'];
      if (channels is! List || start is! num || end is! num) {
        continue;
      }
      for (var channel = 0;
          channel < math.min(colors.length, channels.length);
          channel++) {
        wave.color = colors[channel];
        final values = channels[channel];
        if (values is! List || values.isEmpty) continue;
        for (var i = 0; i < values.length; i++) {
          final pair = values[i];
          if (pair is! List ||
              pair.length != 2 ||
              pair[0] is! num ||
              pair[1] is! num) {
            continue;
          }
          final frame = start + (end - start) * (i + .5) / values.length;
          final x = frame / frames * size.width;
          final y = 56 +
              channel * _RecorderArrangementState.laneHeight +
              _RecorderArrangementState.laneHeight / 2 +
              7;
          final top = y - (pair[1] as num).toDouble().clamp(-1, 1) * 16;
          final bottom = y - (pair[0] as num).toDouble().clamp(-1, 1) * 16;
          // A silent or very quiet bucket still needs a visible center line.
          canvas.drawLine(
              Offset(x, top), Offset(x, math.max(bottom, top + 1)), wave);
        }
      }
    }
    final x = position / frames * size.width;
    final triangle = Path()
      ..moveTo(x - 7, 0)
      ..lineTo(x + 7, 0)
      ..lineTo(x, 10)
      ..close();
    canvas.drawPath(triangle, Paint()..color = palette.lime);
    canvas.drawLine(
        Offset(x, 0),
        Offset(x, size.height),
        Paint()
          ..color = palette.lime
          ..strokeWidth = 2);
  }

  @override
  bool shouldRepaint(covariant _TimelinePainter old) =>
      old.names != names ||
      old.takeName != takeName ||
      old.peaks != peaks ||
      old.palette != palette ||
      old.frames != frames ||
      old.rate != rate ||
      old.position != position ||
      old.clipFrames != clipFrames ||
      old.visibleStart != visibleStart ||
      old.visibleEnd != visibleEnd ||
      old.loopStart != loopStart ||
      old.loopEnd != loopEnd;
}

class _OverviewPainter extends CustomPainter {
  _OverviewPainter(this.p, this.peaks, this.frames, this.offset, this.viewport,
      this.position, this.laneCount);
  final ConsolePalette p;
  final List<Map<String, dynamic>> peaks;
  final double frames, offset, viewport, position;
  final int laneCount;
  @override
  void paint(Canvas c, Size size) {
    c.drawRRect(
        RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(6)),
        Paint()..color = p.panel);
    final paint = Paint()..color = p.meter.withValues(alpha: .55);
    for (final chunk in peaks) {
      final channels = chunk['channels'];
      final start = chunk['start_frame'];
      final end = chunk['end_frame'];
      if (channels is! List ||
          channels.isEmpty ||
          channels.first is! List ||
          start is! num ||
          end is! num) {
        continue;
      }
      for (var channel = 0;
          channel < math.min(laneCount, channels.length);
          channel++) {
        if (channels[channel] is! List) continue;
        final values = channels[channel] as List;
        for (var i = 0; i < values.length; i++) {
          final pair = values[i];
          if (pair is! List ||
              pair.length != 2 ||
              pair[0] is! num ||
              pair[1] is! num) {
            continue;
          }
          final x = (start + (end - start) * (i + .5) / values.length) /
              frames *
              size.width;
          c.drawLine(
              Offset(
                  x,
                  size.height / 2 -
                      (pair[1] as num).toDouble().clamp(-1, 1) * 16),
              Offset(
                  x,
                  size.height / 2 -
                      (pair[0] as num).toDouble().clamp(-1, 1) * 16),
              paint);
        }
      }
    }
    final rect = Rect.fromLTWH(
        offset * size.width, 0, viewport * size.width, size.height);
    c.drawRect(rect, Paint()..color = p.text.withValues(alpha: .1));
    c.drawRect(
        rect.deflate(1),
        Paint()
          ..color = p.recorderMuted
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2);
    c.drawLine(
        Offset(position * size.width, 0),
        Offset(position * size.width, size.height),
        Paint()
          ..color = p.lime
          ..strokeWidth = 2);
  }

  @override
  bool shouldRepaint(covariant _OverviewPainter old) => true;
}
