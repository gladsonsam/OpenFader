import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../core/destination_controls.dart';
import '../core/control_reset.dart';
import '../core/output_geq.dart';
import '../core/output_presets.dart';
import '../core/output_processing.dart';
import '../ui/console.dart';

/// Draw and Flat both open a captured local draft. Only Apply can send writes.
class OutputGeqDraftActions extends StatelessWidget {
  final OutputProcessingStore store;
  const OutputGeqDraftActions({super.key, required this.store});

  void _open(BuildContext context, bool flat) {
    final target = store.captureOutputGeqDraftTarget();
    if (target == null) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text(
            'Wait for verified Graphic EQ settings and an unlocked output.',
          ),
        ),
      );
      return;
    }
    showDialog<void>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      barrierDismissible: false,
      builder: (_) =>
          _OutputGeqEditor(store: store, target: target, flat: flat),
    );
  }

  @override
  Widget build(BuildContext context) => ConsoleActionGroup(
        children: [
          TextButton.icon(
            onPressed: () => _open(context, false),
            icon: const Icon(Icons.gesture),
            label: const Text('Draw'),
          ),
          TextButton.icon(
            onPressed: outputGeqBands(store.outputProcessingControls
                        .where((c) => c.target == store.outputProcessingTarget))
                    .every((c) => c.allowsValue(0))
                ? () => _open(context, true)
                : null,
            icon: const Icon(Icons.horizontal_rule),
            label: const Text('Flat'),
          ),
        ],
      );
}

class _OutputGeqEditor extends StatefulWidget {
  final OutputProcessingStore store;
  final OutputPresetTarget target;
  final bool flat;
  const _OutputGeqEditor({
    required this.store,
    required this.target,
    required this.flat,
  });
  @override
  State<_OutputGeqEditor> createState() => _OutputGeqEditorState();
}

class _OutputGeqEditorState extends State<_OutputGeqEditor> {
  late final List<DestinationControlVm> _bands;
  late final List<double> _gains;
  late final double _min;
  late final double _max;
  int _selected = 0;
  Offset? _previous;
  bool _applying = false;
  bool _finished = false;
  bool _stopping = false;
  String? _invalid;
  String? _message;
  bool get _editable => !_applying && !_finished && _invalid == null;
  bool get _canFlat => _bands.every((c) => c.allowsValue(0));

  @override
  void initState() {
    super.initState();
    _bands = outputGeqBands(widget.target.controls);
    _gains = [for (final band in _bands) band.value!];
    _min = _bands.map((c) => c.min!).reduce(math.min);
    _max = _bands.map((c) => c.max!).reduce(math.max);
    if (widget.flat && _canFlat) _gains.fillRange(0, _gains.length, 0);
    widget.store.addListener(_observe);
  }

  void _observe() {
    if (_applying || _finished || _invalid != null) return;
    final error = widget.store.outputGeqDraftTargetError(widget.target);
    // Staleness is permanent for this draft, even if the peer restores a value.
    if (error != null && mounted) setState(() => _invalid = error);
  }

  @override
  void dispose() {
    widget.store.removeListener(_observe);
    if (_applying) widget.store.cancelOutputPresetRecall();
    super.dispose();
  }

  void _draw(Offset point, Size size) {
    if (!_editable || size.width <= 0 || size.height <= 0) return;
    final current = Offset(
      point.dx.clamp(0, size.width),
      point.dy.clamp(0, size.height),
    );
    final previous = _previous ?? current;
    int index(double x) =>
        (x / size.width * _bands.length).floor().clamp(0, _bands.length - 1);
    final first = index(previous.dx);
    final last = index(current.dx);
    setState(() {
      for (var i = math.min(first, last); i <= math.max(first, last); i++) {
        final center = size.width * (i + .5) / _bands.length;
        final t = first == last || current.dx == previous.dx
            ? 1.0
            : ((center - previous.dx) / (current.dx - previous.dx)).clamp(
                0.0,
                1.0,
              );
        final y = previous.dy + (current.dy - previous.dy) * t;
        _gains[i] = outputGeqDraftGain(
          _bands[i],
          _max - y / size.height * (_max - _min),
        );
      }
      _selected = last;
      _previous = current;
    });
  }

  Future<void> _apply() async {
    if (!_editable) return;
    final error = widget.store.outputGeqDraftTargetError(widget.target);
    if (error != null) {
      setState(() => _invalid = error);
      return;
    }
    setState(() => _applying = true);
    final result = await widget.store.applyOutputGeqDraft(widget.target, {
      for (var i = 0; i < _bands.length; i++) _bands[i].id: _gains[i],
    });
    if (!mounted) return;
    setState(() {
      _applying = false;
      _finished = true;
      _message = result.error ??
          '${result.confirmed} of ${result.total} gains verified by mixer readback.';
    });
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final band = _bands[_selected];
    return PopScope(
      canPop: !_applying,
      child: ConsoleAlertDialog(
        backgroundColor: palette.panel,
        title: Text('${widget.target.name} · Graphic EQ draft'),
        content: SizedBox(
          width: 760,
          child: SingleChildScrollView(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  'Draw a curve or adjust a band. Apply sends ${_bands.length} proposed gains; Cancel discards the draft.',
                ),
                const SizedBox(height: 12),
                Text(
                  'Local proposal · ${_min.toStringAsFixed(1)} to ${_max.toStringAsFixed(1)} dB',
                  style: TextStyle(color: palette.mutedText),
                ),
                const SizedBox(height: 8),
                SizedBox(
                  height: 220,
                  child: LayoutBuilder(
                    builder: (context, constraints) {
                      final size = Size(
                        constraints.maxWidth,
                        constraints.maxHeight,
                      );
                      return Semantics(
                        label:
                            'Local Graphic EQ curve. Use the band selector and gain slider to adjust individual bands.',
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onPanStart: _editable
                              ? (event) {
                                  _previous = null;
                                  _draw(event.localPosition, size);
                                }
                              : null,
                          onPanUpdate: _editable
                              ? (event) => _draw(event.localPosition, size)
                              : null,
                          onPanEnd: (_) => _previous = null,
                          onPanCancel: () => _previous = null,
                          onTapDown: _editable
                              ? (event) {
                                  _previous = null;
                                  _draw(event.localPosition, size);
                                  _previous = null;
                                }
                              : null,
                          child: CustomPaint(
                            painter: _GeqDraftPainter(
                              List.of(_gains),
                              _bands,
                              _min,
                              _max,
                              _selected,
                              palette,
                            ),
                          ),
                        ),
                      );
                    },
                  ),
                ),
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [Text(_bands.first.label), Text(_bands.last.label)],
                ),
                const SizedBox(height: 12),
                DropdownButtonFormField<int>(
                  itemHeight: null,
                  initialValue: _selected,
                  key: ValueKey(_selected),
                  decoration: const InputDecoration(labelText: 'Band'),
                  items: [
                    for (var i = 0; i < _bands.length; i++)
                      DropdownMenuItem(value: i, child: Text(_bands[i].label)),
                  ],
                  onChanged: _editable
                      ? (value) => setState(() => _selected = value!)
                      : null,
                ),
                Text(
                  '${_gains[_selected].toStringAsFixed(band.decimals)} ${band.unit}',
                  textAlign: TextAlign.center,
                ),
                GestureDetector(
                  onDoubleTap: _editable &&
                          numericResetValue(band.min!, band.max!) != null
                      ? () => setState(
                          () => _gains[_selected] = outputGeqDraftGain(band, 0))
                      : null,
                  child: Slider(
                    min: band.min!,
                    max: band.max!,
                    value: _gains[_selected].clamp(band.min!, band.max!),
                    semanticFormatterCallback: (value) =>
                        '${band.label}: ${value.toStringAsFixed(band.decimals)} ${band.unit}, local draft',
                    onChanged: _editable
                        ? (value) => setState(
                              () => _gains[_selected] = outputGeqDraftGain(
                                band,
                                value,
                              ),
                            )
                        : null,
                  ),
                ),
                if (_applying) ...[
                  const LinearProgressIndicator(),
                  const Text(
                    'Applying serially and waiting for verified mixer readback…',
                  ),
                ],
                if (_invalid != null || _message != null)
                  Text(
                    _invalid ?? _message!,
                    style: TextStyle(color: palette.text),
                  ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: _editable && _canFlat
                ? () => setState(() => _gains.fillRange(0, _gains.length, 0))
                : null,
            child: const Text('Flat draft'),
          ),
          if (_applying)
            TextButton(
              onPressed: _stopping
                  ? null
                  : () {
                      widget.store.cancelOutputPresetRecall();
                      setState(() => _stopping = true);
                    },
              child: Text(_stopping ? 'Stopping…' : 'Stop after current write'),
            )
          else
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text(_finished ? 'Close' : 'Cancel'),
            ),
          FilledButton(
            onPressed: _editable ? _apply : null,
            child: const Text('Apply'),
          ),
        ],
      ),
    );
  }
}

class _GeqDraftPainter extends CustomPainter {
  final List<double> gains;
  final List<DestinationControlVm> bands;
  final double min;
  final double max;
  final int selected;
  final ConsolePalette palette;
  _GeqDraftPainter(
    this.gains,
    this.bands,
    this.min,
    this.max,
    this.selected,
    this.palette,
  );

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = palette.well);
    double y(double value) => size.height * (max - value) / (max - min);
    final grid = Paint()
      ..color = palette.line
      ..strokeWidth = 1;
    for (var row = 1; row < 4; row++) {
      canvas.drawLine(
        Offset(0, size.height * row / 4),
        Offset(size.width, size.height * row / 4),
        grid,
      );
    }
    if (min <= 0 && max >= 0) {
      canvas.drawLine(Offset(0, y(0)), Offset(size.width, y(0)), grid);
    }
    final width = size.width / bands.length;
    final curve = Path();
    for (var i = 0; i < bands.length; i++) {
      final x = width * (i + .5);
      // Show each advertised band's permitted vertical span.
      canvas.drawLine(
        Offset(x, y(bands[i].min!)),
        Offset(x, y(bands[i].max!)),
        grid,
      );
      if (i == 0) {
        curve.moveTo(x, y(gains[i]));
      } else {
        curve.lineTo(x, y(gains[i]));
      }
    }
    canvas.drawPath(
      curve,
      Paint()
        ..color = palette.lime
        ..strokeWidth = 2
        ..style = PaintingStyle.stroke,
    );
    canvas.drawCircle(
      Offset(width * (selected + .5), y(gains[selected])),
      4,
      Paint()..color = palette.text,
    );
  }

  @override
  bool shouldRepaint(covariant _GeqDraftPainter oldDelegate) =>
      oldDelegate.palette != palette ||
      oldDelegate.selected != selected ||
      oldDelegate.min != min ||
      oldDelegate.max != max ||
      !identical(oldDelegate.bands, bands) ||
      !identical(oldDelegate.gains, gains);
}
