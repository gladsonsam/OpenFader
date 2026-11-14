import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../ui/console.dart';
import 'recorder.dart';

/// A shared time axis for all capture channels. No page-owned ticker: the
/// shell's recorder store supplies each new transport and meter snapshot.
class RecorderArrangement extends StatefulWidget {
  const RecorderArrangement({
    super.key,
    required this.store,
    required this.previewTakeId,
    required this.onLoopSelection,
    this.takeName,
    this.loopStartFrame,
    this.loopEndFrame,
    this.onNewTake,
    this.onAudioSetup,
  });
  final void Function(int start, int end, int total) onLoopSelection;
  final String? takeName;
  final int? loopStartFrame, loopEndFrame;
  final VoidCallback? onNewTake;
  final VoidCallback? onAudioSetup;
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
  final _horizontal = ScrollController(keepScrollOffset: false);
  final _vertical = ScrollController(keepScrollOffset: false);
  String? _requestKey;
  String? _desiredKey;
  bool _loadingPeaks = false;
  List<Map<String, dynamic>> _peaks = [];
  List<Map<String, dynamic>> _overviewPeaks = [];
  String? _overviewKey;
  String? _peakError;

  final Map<int, Offset> _touches = {};
  bool _touchNavigation = false;
  double _pinchSpan = 1;
  double _pinchZoom = 1;
  double _pinchAnchor = 0;
  int _gestureRevision = 0;

  void _startPinch() {
    if (_touches.length != 2 || !_horizontal.hasClients) return;
    final points = _touches.values.toList();
    final focus = (points[0] + points[1]) / 2;
    final position = _horizontal.position;
    _pinchSpan = math.max(1, (points[0] - points[1]).distance);
    _pinchZoom = _zoom;
    _pinchAnchor =
        (position.pixels + focus.dx) /
        (position.maxScrollExtent + position.viewportDimension);
    setState(() {
      _touchNavigation = true;
      _followRecording = false;
      _loopStart = _loopEnd = null;
    });
  }

  void _movePinch() {
    if (!_touchNavigation || _touches.length != 2) return;
    final points = _touches.values.toList();
    final focus = (points[0] + points[1]) / 2;
    final next = (_pinchZoom * (points[0] - points[1]).distance / _pinchSpan)
        .clamp(1.0, 8.0);
    final revision = ++_gestureRevision;
    setState(() => _zoom = next);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_horizontal.hasClients || revision != _gestureRevision) {
        return;
      }
      final position = _horizontal.position;
      final target =
          _pinchAnchor *
              (position.maxScrollExtent + position.viewportDimension) -
          focus.dx;
      _horizontal.jumpTo(target.clamp(0.0, position.maxScrollExtent));
    });
  }

  void _releaseTouch(int pointer) {
    _touches.remove(pointer);
    if (_touches.length == 2) _startPinch();
    if (_touches.isEmpty && mounted) setState(() {});
  }

  @override
  void initState() {
    super.initState();
    _horizontal.addListener(_onScroll);
  }

  @override
  void didUpdateWidget(covariant RecorderArrangement oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.loopStartFrame != widget.loopStartFrame ||
        oldWidget.loopEndFrame != widget.loopEndFrame) {
      _loopStart = _loopEnd = null;
    }
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
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted) return;
        if (_vertical.hasClients) _vertical.jumpTo(0);
        if (_horizontal.hasClients) _horizontal.jumpTo(0);
      });
      _touches.clear();
      _touchNavigation = false;
      _gestureRevision++;
    }
  }

  void _onScroll() {
    if (mounted) setState(() {});
  }

  void _setZoom(double value) {
    final next = value.clamp(1.0, 8.0);
    if (next == _zoom) return;
    final anchor = _horizontal.hasClients
        ? (_horizontal.offset + _horizontal.position.viewportDimension / 2) /
              (_horizontal.position.maxScrollExtent +
                  _horizontal.position.viewportDimension)
        : 0.0;
    setState(() => _zoom = next);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_horizontal.hasClients) return;
      final position = _horizontal.position;
      final target = next == 1
          ? 0.0
          : anchor * (position.maxScrollExtent + position.viewportDimension) -
                position.viewportDimension / 2;
      _horizontal.jumpTo(target.clamp(0.0, position.maxScrollExtent));
    });
  }

  @override
  void dispose() {
    _horizontal.dispose();
    _vertical.dispose();
    super.dispose();
  }

  void _requestVisible(
    String? id,
    double frames,
    double width,
    double viewport,
    int takeFrames, {
    bool live = false,
    int refreshFrames = 1,
  }) {
    if (id == null || takeFrames <= 0 || viewport <= 0) return;
    final offset = _horizontal.hasClients ? _horizontal.offset : 0.0;
    final visibleStart = (offset / width * frames).floor().clamp(0, takeFrames);
    final start = live
        ? visibleStart ~/ refreshFrames * refreshFrames
        : visibleStart;
    final published = live
        ? takeFrames ~/ refreshFrames * refreshFrames
        : takeFrames;
    final end = ((offset + viewport) / width * frames).ceil().clamp(
      0,
      published,
    );
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
            to - from,
            math.max(1, ((to - from) / frames * width).ceil()),
          );
          chunks.add(
            await widget.store.waveform(id, from, to, buckets.clamp(1, 2048)),
          );
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
          chunks.add(
            await widget.store.waveform(
              id,
              from,
              to,
              math.max(1, ((to - from) / frames * 512).ceil()),
            ),
          );
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
      p.arrangementGrey,
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
        .where((t) => t['id'] == widget.previewTakeId)
        .firstOrNull;
    final laneSource = recording
        ? s
        : take ??
              (widget.previewTakeId == null ||
                      '${s['transport']}'.toLowerCase().contains('record')
                  ? s
                  : const <String, dynamic>{});
    final names = recorderChannelNames(
      take ??
          {
            ...laneSource,
            'channel_names':
                (s['channel_names'] is List &&
                    (s['channel_names'] as List).isNotEmpty)
                ? s['channel_names']
                : s['capture_channel_names'] ?? laneSource['channel_names'],
          },
    );
    final knownRate =
        (take?['sample_rate'] as num?)?.toDouble() ??
        (s['sample_rate'] as num?)?.toDouble();
    final rateKnown = knownRate != null && knownRate > 0;
    final laneCount = rateKnown ? recorderChannelCount(laneSource) : 0;
    // Arithmetic placeholder only; no tracks or sample-rate claim until known.
    final rate = rateKnown ? knownRate : 1.0;
    final active =
        '${s['input_mode']}'.toLowerCase().contains('soundcheck') ||
        '${s['transport']}'.toLowerCase().contains('record');
    final prepared =
        '${s['input_mode']}'.toLowerCase().contains('soundcheck') &&
        widget.previewTakeId == s['selected_take_id'];
    final recordedFrames = (s['recorded_frames'] as num?)?.toInt() ?? 0;
    final frames = recording
        ? math.max(
            rate * 30,
            ((recordedFrames + 1) / (rate * 30)).ceil() * rate * 30,
          )
        : math.max(
            1.0,
            (take?['duration_frames'] as num?)?.toDouble() ??
                (active ? (s['recorded_frames'] as num?)?.toDouble() : null) ??
                rate * 30,
          );
    final position =
        (s['transport'] == 'recording'
                ? s['recorded_frames']
                : s['position_frames'])
            as num?;
    final current = (active ? (position?.toDouble() ?? 0) : 0.0).clamp(
      0.0,
      frames,
    );
    final enabled = widget.store.connected && !widget.store.pending;
    final canSeek = enabled && prepared && caps['seek'] == true;
    final canLoop = enabled && prepared && caps['loop'] == true;
    final meters = s['live_peaks'] is List ? s['live_peaks'] as List : const [];
    return LayoutBuilder(
      builder: (context, arrangementConstraints) {
        final labelWidth = arrangementConstraints.maxWidth < 600
            ? 120.0
            : 220.0;
        return Container(
          decoration: BoxDecoration(
            color: p.panel,
            border: Border.all(color: p.linePanel),
            borderRadius: BorderRadius.zero,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: Wrap(
                  crossAxisAlignment: WrapCrossAlignment.center,
                  spacing: 8,
                  children: [
                    Text(
                      '$laneCount tracks',
                      style: TextStyle(color: p.mutedText, fontSize: 14),
                    ),
                    if (_peakError != null)
                      Tooltip(
                        message: _peakError!,
                        child: TextButton(
                          onPressed: () => setState(() {
                            _requestKey = null;
                            _peakError = null;
                          }),
                          child: const Text('Retry waveform'),
                        ),
                      ),
                    if (recording)
                      IconButton(
                        tooltip: _followRecording
                            ? 'Following recording · click to scroll freely'
                            : 'Follow recording playhead',
                        isSelected: _followRecording,
                        onPressed: () => setState(
                          () => _followRecording = !_followRecording,
                        ),
                        icon: const Icon(Icons.follow_the_signs),
                      ),
                    if (laneCount > 0)
                      Container(
                        decoration: BoxDecoration(
                          border: Border.all(color: p.linePanel),
                          borderRadius: BorderRadius.circular(8),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              tooltip: 'Zoom out',
                              onPressed: _zoom > 1
                                  ? () => _setZoom(_zoom / 2)
                                  : null,
                              icon: const Icon(Icons.remove, size: 20),
                            ),
                            SizedBox(
                              height: 16,
                              child: VerticalDivider(
                                width: 1,
                                color: p.linePanel,
                              ),
                            ),
                            IconButton(
                              tooltip: 'Zoom in',
                              onPressed: _zoom < 8
                                  ? () => _setZoom(_zoom * 2)
                                  : null,
                              icon: const Icon(Icons.add, size: 20),
                            ),
                          ],
                        ),
                      ),
                    if (laneCount > 0 && _zoom > 1)
                      IconButton(
                        tooltip: 'Show whole take',
                        onPressed: () => _setZoom(1),
                        icon: const Icon(Icons.fit_screen, size: 20),
                      ),
                  ],
                ),
              ),
              if (laneCount == 0)
                Expanded(
                  child: Center(
                    child: Padding(
                      padding: const EdgeInsets.all(24),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(Icons.graphic_eq, size: 40, color: p.mutedText),
                          const SizedBox(height: 12),
                          Text(
                            take == null
                                ? 'No tracks yet'
                                : 'Track layout unavailable',
                            textAlign: TextAlign.center,
                            style: TextStyle(
                              color: p.text1,
                              fontSize: 18,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                          const SizedBox(height: 8),
                          Text(
                            take == null
                                ? 'Set up an audio interface and start a take to see its tracks here.'
                                : 'This take does not have a usable track layout.',
                            textAlign: TextAlign.center,
                            style: TextStyle(color: p.mutedText),
                          ),
                          if (widget.onNewTake != null ||
                              widget.onAudioSetup != null) ...[
                            const SizedBox(height: 12),
                            Wrap(
                              alignment: WrapAlignment.center,
                              spacing: 8,
                              children: [
                                if (widget.onNewTake != null)
                                  TextButton(
                                    onPressed: widget.onNewTake,
                                    child: const Text('New take'),
                                  ),
                                if (widget.onAudioSetup != null)
                                  TextButton(
                                    onPressed: widget.onAudioSetup,
                                    child: const Text('Audio setup'),
                                  ),
                              ],
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                )
              else
                Expanded(
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      final viewport = math.max(
                        1.0,
                        constraints.maxWidth - labelWidth,
                      );
                      final width = math.max(
                        viewport,
                        recording
                            ? frames / rate / 30 * viewport * _zoom
                            : viewport * _zoom,
                      );
                      _requestVisible(
                        recording
                            ? s['selected_take_id'] as String?
                            : take?['id'] as String?,
                        frames,
                        width,
                        constraints.maxWidth - labelWidth,
                        recording
                            ? recordedFrames
                            : (take?['duration_frames'] as num?)?.toInt() ?? 0,
                        live: recording,
                        refreshFrames: math.max(1, (rate / 4).round()),
                      );
                      if (recording && _followRecording) {
                        WidgetsBinding.instance.addPostFrameCallback((_) {
                          if (!mounted ||
                              !_horizontal.hasClients ||
                              !_followRecording) {
                            return;
                          }
                          final target =
                              (current / frames * width - viewport * .8).clamp(
                                0.0,
                                _horizontal.position.maxScrollExtent,
                              );
                          if ((_horizontal.offset - target).abs() > .5) {
                            _horizontal.jumpTo(target);
                          }
                        });
                      }
                      _requestOverview(
                        recording ? null : take?['id'] as String?,
                        (take?['duration_frames'] as num?)?.toInt() ?? 0,
                      );
                      return SingleChildScrollView(
                        controller: _vertical,
                        physics: _touchNavigation && _touches.isNotEmpty
                            ? const NeverScrollableScrollPhysics()
                            : null,
                        scrollDirection: Axis.vertical,
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            SizedBox(
                              width: labelWidth,
                              child: Column(
                                children: [
                                  SizedBox(
                                    height: 56,
                                    child: Center(
                                      child: Text(
                                        'Tracks',
                                        textAlign: TextAlign.center,
                                        style: TextStyle(
                                          color: p.mutedText,
                                          fontSize: 14,
                                        ),
                                      ),
                                    ),
                                  ),
                                  for (var i = 0; i < laneCount; i++)
                                    Container(
                                      height: laneHeight,
                                      padding: const EdgeInsets.only(right: 8),
                                      decoration: BoxDecoration(
                                        color: p.arrangementTrack,
                                        border: Border(
                                          top: BorderSide(color: p.line),
                                        ),
                                      ),
                                      child: Stack(
                                        children: [
                                          Row(
                                            children: [
                                              Container(
                                                width: 7,
                                                color: _channelColor(p, i),
                                              ),
                                              const SizedBox(width: 12),
                                              Text(
                                                (i + 1).toString(),
                                                style: TextStyle(
                                                  color: p.mutedText,
                                                ),
                                              ),
                                              const SizedBox(width: 8),
                                              Expanded(
                                                child: Text(
                                                  names[i] ??
                                                      'Channel ${i + 1}',
                                                  overflow:
                                                      TextOverflow.ellipsis,
                                                  style: TextStyle(
                                                    color: p.text1,
                                                    fontSize: 14,
                                                    fontWeight: FontWeight.w600,
                                                  ),
                                                ),
                                              ),
                                            ],
                                          ),
                                          Positioned(
                                            left: 7,
                                            right: 0,
                                            bottom: 0,
                                            height: 4,
                                            child: LayoutBuilder(
                                              builder: (context, box) => Stack(
                                                children: [
                                                  ColoredBox(
                                                    color: p.slot,
                                                    child: SizedBox(
                                                      width: box.maxWidth,
                                                      height: 4,
                                                    ),
                                                  ),
                                                  SizedBox(
                                                    width:
                                                        box.maxWidth *
                                                        (i < meters.length &&
                                                                meters[i] is num
                                                            ? (meters[i] as num)
                                                                  .toDouble()
                                                                  .clamp(0, 1)
                                                            : 0),
                                                    height: 4,
                                                    child: ColoredBox(
                                                      color: _channelColor(
                                                        p,
                                                        i,
                                                      ),
                                                    ),
                                                  ),
                                                ],
                                              ),
                                            ),
                                          ),
                                        ],
                                      ),
                                    ),
                                ],
                              ),
                            ),
                            Expanded(
                              child: NotificationListener<ScrollStartNotification>(
                                onNotification: (notification) {
                                  if (notification.dragDetails != null &&
                                      _followRecording) {
                                    setState(() => _followRecording = false);
                                  }
                                  return false;
                                },
                                child: Listener(
                                  onPointerDown: (event) {
                                    if (_touches.isEmpty) {
                                      _touchNavigation = false;
                                    }
                                    _touches[event.pointer] =
                                        event.localPosition;
                                    _startPinch();
                                  },
                                  onPointerMove: (event) {
                                    if (!_touches.containsKey(event.pointer)) {
                                      return;
                                    }
                                    _touches[event.pointer] =
                                        event.localPosition;
                                    _movePinch();
                                  },
                                  onPointerUp: (event) =>
                                      _releaseTouch(event.pointer),
                                  onPointerCancel: (event) =>
                                      _releaseTouch(event.pointer),
                                  child: SingleChildScrollView(
                                    physics:
                                        _touchNavigation && _touches.isNotEmpty
                                        ? const NeverScrollableScrollPhysics()
                                        : null,
                                    controller: _horizontal,
                                    scrollDirection: Axis.horizontal,
                                    child: SizedBox(
                                      width: width,
                                      height: 56 + laneHeight * laneCount,
                                      child: GestureDetector(
                                        behavior: HitTestBehavior.opaque,
                                        onDoubleTap: () {
                                          if (!_touchNavigation) {
                                            _setZoom(1);
                                          }
                                        },
                                        onTapUp: canSeek
                                            ? (d) {
                                                if (_touchNavigation) {
                                                  return;
                                                }
                                                widget.store.command('seek', {
                                                  'position_frames':
                                                      (d.localPosition.dx /
                                                              width *
                                                              frames)
                                                          .round(),
                                                });
                                              }
                                            : null,
                                        onLongPressStart: canLoop
                                            ? (d) {
                                                if (_touchNavigation) {
                                                  return;
                                                }
                                                setState(() {
                                                  _loopStart =
                                                      (d.localPosition.dx /
                                                              width)
                                                          .clamp(0, 1);
                                                  _loopEnd = _loopStart;
                                                });
                                              }
                                            : null,
                                        onLongPressMoveUpdate: canLoop
                                            ? (d) {
                                                if (_touchNavigation) {
                                                  return;
                                                }
                                                setState(
                                                  () => _loopEnd =
                                                      (d.localPosition.dx /
                                                              width)
                                                          .clamp(0, 1),
                                                );
                                              }
                                            : null,
                                        onLongPressEnd: canLoop
                                            ? (_) {
                                                if (_touchNavigation) {
                                                  return;
                                                }
                                                if (_loopStart != null &&
                                                    _loopEnd != null) {
                                                  widget.onLoopSelection(
                                                    (math.min(
                                                              _loopStart!,
                                                              _loopEnd!,
                                                            ) *
                                                            frames)
                                                        .round(),
                                                    (math.max(
                                                              _loopStart!,
                                                              _loopEnd!,
                                                            ) *
                                                            frames)
                                                        .round(),
                                                    frames.round(),
                                                  );
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
                                            visibleEnd:
                                                ((_horizontal.hasClients
                                                        ? _horizontal.offset
                                                        : 0) +
                                                    viewport) /
                                                width,
                                            clipFrames: recording
                                                ? recordedFrames.toDouble()
                                                : take == null
                                                ? 0
                                                : frames,
                                            peaks: _peaks,
                                            names: names,
                                            takeName:
                                                widget.takeName ??
                                                (recording
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
                                                          .toString()),
                                            colors: List.generate(
                                              laneCount,
                                              (i) => _channelColor(p, i),
                                            ),
                                            loopStart:
                                                _loopStart ??
                                                (widget.loopStartFrame == null
                                                    ? null
                                                    : widget.loopStartFrame! /
                                                          frames),
                                            loopEnd:
                                                _loopEnd ??
                                                (widget.loopEndFrame == null
                                                    ? null
                                                    : widget.loopEndFrame! /
                                                          frames),
                                          ),
                                        ),
                                      ),
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              if (laneCount > 0)
                SizedBox(
                  height: 56,
                  child: LayoutBuilder(
                    builder: (context, box) {
                      final view = math.max(1.0, box.maxWidth - labelWidth);
                      final total = recording
                          ? math.max(view, frames / rate / 30 * view * _zoom)
                          : view * _zoom;
                      final offset = _horizontal.hasClients
                          ? _horizontal.offset
                          : 0.0;
                      void move(double x, double overviewWidth) {
                        if (_horizontal.hasClients) {
                          _horizontal.jumpTo(
                            (x / math.max(1, overviewWidth) * total - view / 2)
                                .clamp(
                                  0.0,
                                  _horizontal.position.maxScrollExtent,
                                ),
                          );
                        }
                      }

                      return Row(
                        children: [
                          const SizedBox(width: 18),
                          Text('00:00', style: TextStyle(color: p.mutedText)),
                          const SizedBox(width: 12),
                          Expanded(
                            child: LayoutBuilder(
                              builder: (context, overviewBox) =>
                                  GestureDetector(
                                    behavior: HitTestBehavior.opaque,
                                    onTapDown: (d) => move(
                                      d.localPosition.dx,
                                      overviewBox.maxWidth,
                                    ),
                                    onHorizontalDragUpdate: (d) => move(
                                      d.localPosition.dx,
                                      overviewBox.maxWidth,
                                    ),
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
                                          laneCount,
                                        ),
                                      ),
                                    ),
                                  ),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Text(
                            '${(frames / rate ~/ 60).toString().padLeft(2, '0')}:${(frames / rate % 60).floor().toString().padLeft(2, '0')}',
                            style: TextStyle(color: p.mutedText),
                          ),
                          const SizedBox(width: 18),
                        ],
                      );
                    },
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}

class _TimelinePainter extends CustomPainter {
  const _TimelinePainter({
    required this.palette,
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
    this.loopEnd,
  });
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
              : palette.arrangementLaneAlt,
      );
      if (takeName != null && clipFrames > 0) {
        final clipWidth = (clipFrames / frames * size.width).clamp(
          0.0,
          size.width,
        );
        final rect = Rect.fromLTWH(0, y + 5, clipWidth, 52);
        canvas.drawRRect(
          RRect.fromRectAndRadius(rect, const Radius.circular(5)),
          Paint()..color = colors[i].withValues(alpha: .18),
        );
      }
    }
    if (loopStart != null && loopEnd != null) {
      canvas.drawRect(
        Rect.fromLTRB(
          math.min(loopStart!, loopEnd!) * size.width,
          44,
          math.max(loopStart!, loopEnd!) * size.width,
          56,
        ),
        Paint()..color = palette.lime,
      );
      canvas.drawRect(
        Rect.fromLTRB(
          math.min(loopStart!, loopEnd!) * size.width,
          0,
          math.max(loopStart!, loopEnd!) * size.width,
          size.height,
        ),
        Paint()..color = palette.selectionFill,
      );
    }
    final grid = Paint()
      ..color = palette.line
      ..strokeWidth = 1;
    for (var i = 0; i <= colors.length; i++) {
      final y = 56.0 + i * _RecorderArrangementState.laneHeight;
      canvas.drawLine(Offset(0, y), Offset(size.width, y), grid);
    }
    final duration = frames / rate;
    final desiredStep = duration / size.width * 120;
    final magnitude = math.pow(10, (math.log(desiredStep) / math.ln10).floor());
    final normalized = desiredStep / magnitude;
    final step =
        magnitude *
        (normalized <= 1
            ? 1
            : normalized <= 2
            ? 2
            : normalized <= 5
            ? 5
            : 10);
    final firstTick = (visibleStart * duration / step).floor();
    final lastTick = (visibleEnd * duration / step).ceil();
    for (var i = firstTick; i <= lastTick; i++) {
      final seconds = i * step;
      final x = seconds / duration * size.width;
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), grid);
      final digits = step < 1
          ? math.max(1, -(math.log(step) / math.ln10).floor())
          : 0;
      final secondsLabel = (seconds % 60)
          .toStringAsFixed(digits)
          .padLeft(digits == 0 ? 2 : digits + 3, '0');
      final text = TextPainter(
        text: TextSpan(
          text: '${(seconds ~/ 60).toString().padLeft(2, '0')}:$secondsLabel',
          style: TextStyle(color: palette.recorderMuted, fontSize: 18),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
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
      for (
        var channel = 0;
        channel < math.min(colors.length, channels.length);
        channel++
      ) {
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
          final y =
              56 +
              channel * _RecorderArrangementState.laneHeight +
              _RecorderArrangementState.laneHeight / 2;
          final top = y - (pair[1] as num).toDouble().clamp(-1, 1) * 16;
          final bottom = y - (pair[0] as num).toDouble().clamp(-1, 1) * 16;
          // A silent or very quiet bucket still needs a visible center line.
          canvas.drawLine(
            Offset(x, top),
            Offset(x, math.max(bottom, top + 1)),
            wave,
          );
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
        ..strokeWidth = 2,
    );
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
  _OverviewPainter(
    this.p,
    this.peaks,
    this.frames,
    this.offset,
    this.viewport,
    this.position,
    this.laneCount,
  );
  final ConsolePalette p;
  final List<Map<String, dynamic>> peaks;
  final double frames, offset, viewport, position;
  final int laneCount;
  @override
  void paint(Canvas c, Size size) {
    c.drawRRect(
      RRect.fromRectAndRadius(Offset.zero & size, const Radius.circular(6)),
      Paint()..color = p.panel,
    );
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
      for (
        var channel = 0;
        channel < math.min(laneCount, channels.length);
        channel++
      ) {
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
          final x =
              (start + (end - start) * (i + .5) / values.length) /
              frames *
              size.width;
          c.drawLine(
            Offset(
              x,
              size.height / 2 - (pair[1] as num).toDouble().clamp(-1, 1) * 16,
            ),
            Offset(
              x,
              size.height / 2 - (pair[0] as num).toDouble().clamp(-1, 1) * 16,
            ),
            paint,
          );
        }
      }
    }
    final rect = Rect.fromLTWH(
      offset * size.width,
      0,
      viewport * size.width,
      size.height,
    );
    c.drawRect(rect, Paint()..color = p.text.withValues(alpha: .1));
    c.drawRect(
      rect.deflate(1),
      Paint()
        ..color = p.recorderMuted
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2,
    );
    c.drawLine(
      Offset(position * size.width, 0),
      Offset(position * size.width, size.height),
      Paint()
        ..color = p.lime
        ..strokeWidth = 2,
    );
  }

  @override
  bool shouldRepaint(covariant _OverviewPainter old) => true;
}
