// Meter bridge: read-only ladders and provider-owned monitor controls.
// Offline ladders read -inf and render dark because levels come only from the live engine.
// Freshness comes from the single shell-owned 80 ms loop; this page sends
// spectrum_target for the visible strip while visible and clears it on dispose.
import '../widgets/inline_feedback.dart';
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../theme.dart';
import '../core/output_meters.dart';
import '../ui/console.dart';
import '../core/destination_controls.dart';

const bridgeTopDb = 0.0;
const bridgeFloorDb = -50.0;

class BridgeLadder {
  final int ch;
  final String? label;
  final String sub;

  const BridgeLadder({required this.ch, this.label, required this.sub});
}

class BridgeGroup {
  final String key;
  final String title;
  final List<BridgeLadder> ladders;
  final String? collapsedReason;

  const BridgeGroup({
    required this.key,
    required this.title,
    required this.ladders,
    this.collapsedReason,
  });
}

class BridgeOutputMeter {
  final String target;
  final String label;
  const BridgeOutputMeter(this.target, this.label);
}

class BridgeOutputGroup {
  final String title;
  final List<BridgeOutputMeter> meters;
  const BridgeOutputGroup(this.title, this.meters);
}

// double.negativeInfinity means honestly no data, so the ladder renders dark.
class BridgeTap {
  final ValueNotifier<double> db;
  final ValueNotifier<double> peakDb;
  final ValueNotifier<bool> clip;

  BridgeTap({
    double db = double.negativeInfinity,
    double peakDb = double.negativeInfinity,
    bool clip = false,
  })  : db = ValueNotifier(db),
        peakDb = ValueNotifier(peakDb),
        clip = ValueNotifier(clip);

  void dispose() {
    db.dispose();
    peakDb.dispose();
    clip.dispose();
  }
}

// Implemented by the shell over the live engine; offline groups collapse.
abstract class MetersStore extends ChangeNotifier {
  bool get online;
  bool get demo;
  String? get deskName;
  List<BridgeGroup> get groups;
  BridgeTap tapOf(int ch);
  List<BridgeOutputGroup> get physicalOutputGroups => const [];
  List<DestinationControlVm> get monitorControls => const [];
  bool monitorControlIsCurrent(DestinationControlVm control) => false;
  Future<String?> setMonitorControl(
    DestinationControlVm control,
    double value,
  ) async =>
      'Monitor control unavailable.';
  ValueListenable<OutputMeterVm> outputMeterFor(String target);

  Future<void> pollMeters();

  void setSpectrumTarget(int? ch);
}

class U4MetersPage extends StatefulWidget {
  final MetersStore store;
  final VoidCallback? onClose;

  const U4MetersPage({super.key, required this.store, this.onClose});

  @override
  State<U4MetersPage> createState() => _U4MetersPageState();
}

class _U4MetersPageState extends State<U4MetersPage> {
  final _feedback = InlineFeedbackController();
  int? _target;
  DestinationControlVm? _focused;

  DestinationControlVm? get _currentFocus {
    final focus = _focused;
    if (focus == null) return null;
    for (final row in widget.store.monitorControls) {
      if (row.epoch == focus.epoch &&
          row.target == focus.target &&
          row.surface == focus.surface &&
          row.id == focus.id &&
          row.numeric &&
          row.choices.isEmpty &&
          row.available &&
          widget.store.monitorControlIsCurrent(row)) {
        return row;
      }
    }
    return null;
  }

  Future<void> _editFocus(DestinationControlVm row) async {
    final store = widget.store;
    var value = row.value!;
    final selected = await showDialog<double>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (dialogContext, setDialogState) => ConsoleAlertDialog(
          title: Text(row.label),
          content: Slider(
            value: value.clamp(row.min!, row.max!),
            min: row.min!,
            max: row.max!,
            label: value.toStringAsFixed(row.decimals),
            onChanged: (next) => setDialogState(() => value = next),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(dialogContext, value),
              child: const Text('Apply'),
            ),
          ],
        ),
      ),
    );
    if (!mounted ||
        selected == null ||
        widget.store != store ||
        _currentFocus?.epoch != row.epoch ||
        _currentFocus?.target != row.target ||
        _currentFocus?.surface != row.surface ||
        _currentFocus?.id != row.id ||
        !store.monitorControlIsCurrent(row) ||
        !row.allowsValue(selected) ||
        selected == row.value) {
      return;
    }
    final error = await widget.store.setMonitorControl(row, selected);
    if (mounted && error != null) {
      _feedback.show(error);
    }
  }

  @override
  void didUpdateWidget(covariant U4MetersPage oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.store != widget.store) {
      oldWidget.store.removeListener(_retarget);
      oldWidget.store.setSpectrumTarget(null);
      _target = null;
      _focused = null;
      widget.store.addListener(_retarget);
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _retarget();
      });
    }
  }

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_retarget);
    WidgetsBinding.instance.addPostFrameCallback((_) => _retarget());
  }

  /// Analyser follows the first visible ladder.
  void _retarget() {
    if (_focused != null && _currentFocus == null) _focused = null;
    int? first;
    for (final g in widget.store.groups) {
      if (g.ladders.isNotEmpty) {
        first = g.ladders.first.ch;
        break;
      }
    }
    if (first != _target) {
      _target = first;
      widget.store.setSpectrumTarget(first);
    }
  }

  @override
  void dispose() {
    widget.store.removeListener(_retarget);
    widget.store.setSpectrumTarget(null);
    _feedback.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return InlineFeedbackScope(
        expand: true,
        controller: _feedback,
        builder: (context) => Scaffold(
              backgroundColor: ConsolePalette.of(context).page,
              body: SafeArea(
                child: ListenableBuilder(
                  listenable: widget.store,
                  builder: (context, _) {
                    final store = widget.store;
                    final focused = _currentFocus;
                    return Column(
                      children: [
                        if (focused != null)
                          Container(
                            width: double.infinity,
                            color: ConsolePalette.of(context).panel,
                            padding: const EdgeInsets.symmetric(horizontal: 12),
                            child: Align(
                              alignment: Alignment.centerLeft,
                              child: TextButton(
                                onPressed: () => _editFocus(focused),
                                child: Text(
                                  '${focused.label}: ${focused.valueLabel}${focused.authority == 'accepted' ? ' (unconfirmed)' : ''}',
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                            ),
                          ),
                        Expanded(
                          child: SingleChildScrollView(
                            padding: AppDesign.pagePadding,
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                if (!store.online)
                                  Container(
                                    padding: const EdgeInsets.symmetric(
                                      horizontal: 10,
                                      vertical: 8,
                                    ),
                                    margin: const EdgeInsets.only(bottom: 14),
                                    decoration: BoxDecoration(
                                      color: ConsolePalette.of(context).panel,
                                      border: Border.all(
                                        color:
                                            ConsolePalette.of(context).divider,
                                      ),
                                      borderRadius: const BorderRadius.all(
                                        Radius.circular(4),
                                      ),
                                    ),
                                    child: Text(
                                      'Offline - ladders are dark until a live link is up. Connect for live meters.',
                                      style: TextStyle(
                                        fontFamily: U4.font,
                                        fontSize: AppDesign.caption,
                                        color: ConsolePalette.of(context).text7,
                                      ),
                                    ),
                                  ),
                                for (final g in store.groups) ...[
                                  if (g.ladders.isNotEmpty)
                                    _LadderGroup(
                                      group: g,
                                      online: store.online,
                                      tapOf: store.tapOf,
                                    )
                                  else
                                    _CollapsedGroup(group: g),
                                  const SizedBox(height: 14),
                                ],
                                for (final g in store.physicalOutputGroups) ...[
                                  _PhysicalOutputs(
                                    group: g,
                                    meterOf: store.outputMeterFor,
                                  ),
                                  const SizedBox(height: 14),
                                ],
                                if (store.monitorControls.isNotEmpty) ...[
                                  ExpansionTile(
                                    expansionAnimationStyle:
                                        AnimationStyle.noAnimation,
                                    tilePadding: const EdgeInsets.symmetric(
                                        horizontal: 12),
                                    title: const Text('Monitor controls',
                                        style: TextStyle(
                                            fontSize: 15,
                                            fontWeight: FontWeight.w600)),
                                    children: [
                                      _MonitorControls(
                                          store: store,
                                          focused: focused,
                                          onSelect: (row) =>
                                              setState(() => _focused = row))
                                    ],
                                  ),
                                  const SizedBox(height: 14),
                                ],
                              ],
                            ),
                          ),
                        ),
                      ],
                    );
                  },
                ),
              ),
            ));
  }
}

class _PhysicalOutputs extends StatelessWidget {
  final BridgeOutputGroup group;
  final ValueListenable<OutputMeterVm> Function(String) meterOf;
  const _PhysicalOutputs({required this.group, required this.meterOf});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return ConsolePanel(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            group.title,
            style: TextStyle(
              fontFamily: U4.font,
              fontSize: AppDesign.caption,
              fontWeight: FontWeight.w700,
              color: palette.text1,
            ),
          ),
          const SizedBox(height: AppDesign.spaceLg),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final meter in group.meters)
                  Padding(
                    padding: const EdgeInsets.only(right: 6),
                    child: _PhysicalLadder(
                        label: meter.label, meter: meterOf(meter.target)),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

// Adapt provider telemetry to the same visual scale as channel meters.
// Listen directly to the existing meter notifier; this owns no polling timer.
class _PhysicalLadder extends StatefulWidget {
  final String label;
  final ValueListenable<OutputMeterVm> meter;
  const _PhysicalLadder({required this.label, required this.meter});
  @override
  State<_PhysicalLadder> createState() => _PhysicalLadderState();
}

class _PhysicalLadderState extends State<_PhysicalLadder> {
  final _taps = [BridgeTap(), BridgeTap()];
  int _count = 1;
  void _update() {
    final value = widget.meter.value;
    final count = value.levels.length == 2 ? 2 : 1;
    if (_count != count) setState(() => _count = count);
    for (var i = 0; i < _taps.length; i++) {
      final level = i < value.levels.length ? value.levels[i] : 0.0;
      _taps[i].db.value = value.available && level > 0
          ? 20 * math.log(level) / math.ln10
          : double.negativeInfinity;
      _taps[i].clip.value = value.available && level >= 1;
    }
  }

  @override
  void initState() {
    super.initState();
    widget.meter.addListener(_update);
    _update();
  }

  @override
  void didUpdateWidget(covariant _PhysicalLadder oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.meter != widget.meter) {
      oldWidget.meter.removeListener(_update);
      widget.meter.addListener(_update);
      _update();
    }
  }

  @override
  void dispose() {
    widget.meter.removeListener(_update);
    for (final tap in _taps) {
      tap.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Tooltip(
      message: widget.meter.value.inferred
          ? '${widget.label}: measured signal; physical mapping is unverified'
          : widget.label,
      child: Row(crossAxisAlignment: CrossAxisAlignment.start, children: [
        for (var i = 0; i < _count; i++)
          _Ladder(
              ladder: BridgeLadder(
                  ch: -1,
                  label: widget.label,
                  sub: _count == 2 ? (i == 0 ? 'L' : 'R') : ''),
              tap: _taps[i],
              height: 160),
      ]));
}

class _LadderGroup extends StatelessWidget {
  final BridgeGroup group;
  final bool online;
  final BridgeTap Function(int ch) tapOf;

  const _LadderGroup({
    required this.group,
    required this.online,
    required this.tapOf,
  });

  @override
  Widget build(BuildContext context) {
    return ConsolePanel(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                group.title,
                style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: AppDesign.caption,
                  fontWeight: FontWeight.w700,
                  color: ConsolePalette.of(context).text1,
                ),
              ),
              if (!online)
                Padding(
                  padding: EdgeInsets.only(left: 8),
                  child: Text(
                    'no data',
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 10,
                      color: ConsolePalette.of(context).text7,
                    ),
                  ),
                ),
            ],
          ),
          const SizedBox(height: AppDesign.spaceLg),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final lad in group.ladders)
                  Padding(
                    padding: const EdgeInsets.only(right: 6),
                    child: RepaintBoundary(
                      child: _Ladder(ladder: lad, tap: tapOf(lad.ch)),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _CollapsedGroup extends StatelessWidget {
  final BridgeGroup group;
  const _CollapsedGroup({required this.group});

  @override
  Widget build(BuildContext context) {
    final reason = group.collapsedReason == 'offline'
        ? 'no data (offline)'
        : (group.collapsedReason ?? '');
    return Opacity(
      opacity: 0.75,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
        decoration: BoxDecoration(
          color: ConsolePalette.of(context).panel,
          border: Border.all(color: ConsolePalette.of(context).selected),
          borderRadius: const BorderRadius.all(Radius.circular(10)),
        ),
        child: Row(
          children: [
            Text(
              group.title,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: AppDesign.caption,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).text7,
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                reason,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: AppDesign.minorText,
                  color: ConsolePalette.of(context).text10,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Ladder extends StatefulWidget {
  final BridgeLadder ladder;
  final BridgeTap tap;
  final double height;

  const _Ladder({required this.ladder, required this.tap, this.height = 229});

  @override
  State<_Ladder> createState() => _LadderState();
}

class _LadderState extends State<_Ladder> {
  _LadderPainter? _painter;

  void _updatePainter() {
    final palette = ConsolePalette.of(context);
    if (_painter?.tap == widget.tap && _painter?.palette == palette) return;
    _painter?.disposeLabels();
    _painter = _LadderPainter(tap: widget.tap, palette: palette);
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _updatePainter();
  }

  @override
  void didUpdateWidget(covariant _Ladder oldWidget) {
    super.didUpdateWidget(oldWidget);
    _updatePainter();
  }

  @override
  void dispose() {
    _painter?.disposeLabels();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 64,
      child: Column(
        children: [
          CustomPaint(size: Size(44, widget.height), painter: _painter),
          const SizedBox(height: AppDesign.spaceMd),
          Text(
            widget.ladder.label ?? '-',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center,
            style: TextStyle(
              fontFamily: U4.font,
              fontSize: AppDesign.caption,
              color: ConsolePalette.of(context).text1,
            ),
          ),
          if (widget.ladder.sub.isNotEmpty)
            Text(
              widget.ladder.sub,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              textAlign: TextAlign.center,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: AppDesign.minorText,
                color: ConsolePalette.of(context).text7,
              ),
            ),
        ],
      ),
    );
  }
}

class _LadderPainter extends CustomPainter {
  final BridgeTap tap;
  final ConsolePalette palette;
  late final Map<String, TextPainter> _labels = {
    for (final l in const ['0', '-12', '-24', '-36', '-50'])
      l: TextPainter(
        text: TextSpan(
          text: l,
          style: TextStyle(
            fontFamily: U4.font,
            color: palette.text10,
            fontSize: 8,
            fontFeatures: [FontFeature.tabularFigures()],
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout(),
  };

  _LadderPainter({required this.tap, required this.palette})
      : super(repaint: Listenable.merge([tap.db, tap.peakDb, tap.clip]));

  void disposeLabels() {
    for (final label in _labels.values) {
      label.dispose();
    }
  }

  static double _yOf(double db, double top, double h) =>
      top + ((bridgeTopDb - db) / (bridgeTopDb - bridgeFloorDb)) * h;

  @override
  void paint(Canvas canvas, Size size) {
    const colW = 8.0;
    final colL = (size.width - colW) / 2;
    const clipS = 6.0;
    const colTop = clipS + 3;
    final h = size.height - colTop;
    if (h <= 0) return;

    final db = tap.db.value;
    final hasData = db.isFinite;
    final lv = hasData ? db.clamp(bridgeFloorDb, bridgeTopDb) : bridgeFloorDb;

    canvas.drawRect(
      Rect.fromLTWH(colL, colTop, colW, h),
      Paint()..color = palette.well,
    );

    if (hasData && lv > bridgeFloorDb) {
      final fillY = _yOf(lv, colTop, h);
      for (final band in [
        (lo: -50.0, hi: -12.0, c: palette.meterGreen),
        (lo: -12.0, hi: -6.0, c: palette.meterAmber),
        (lo: -6.0, hi: 0.0, c: palette.meterRed),
      ]) {
        final yHi = _yOf(band.hi, colTop, h);
        final yLo = _yOf(band.lo, colTop, h);
        final top = fillY > yHi ? fillY : yHi;
        if (top < yLo) {
          canvas.drawRect(
            Rect.fromLTWH(colL, top, colW, yLo - top),
            Paint()..color = band.c,
          );
        }
      }
    }

    for (final mark in const [0.0, -12.0, -24.0, -36.0, -50.0]) {
      final y = _yOf(mark, colTop, h);
      canvas.drawRect(
        Rect.fromLTWH(colL - 4, y - 0.5, 4, 1),
        Paint()..color = palette.scaleTick,
      );
      final tp = _labels[mark == 0 ? '0' : mark.toInt().toString()]!;
      tp.paint(canvas, Offset(colL - 5 - tp.width, y - tp.height / 2));
    }

    final peak = tap.peakDb.value;
    if (hasData && peak.isFinite && peak > bridgeFloorDb) {
      final py = _yOf(
        peak.clamp(bridgeFloorDb, bridgeTopDb),
        colTop,
        h,
      ).clamp(colTop, colTop + h - 2);
      canvas.drawRect(
        Rect.fromLTWH(colL, py, colW, 2),
        Paint()..color = palette.peakLine,
      );
    }

    canvas.drawRect(
      Rect.fromLTWH(colL + 1, 0, clipS, clipS),
      Paint()..color = tap.clip.value ? palette.clipOn : palette.clipOff,
    );
  }

  @override
  bool shouldRepaint(_LadderPainter old) =>
      old.tap != tap || old.palette != palette;
}

class _MonitorControls extends StatelessWidget {
  final MetersStore store;
  final DestinationControlVm? focused;
  final ValueChanged<DestinationControlVm> onSelect;
  const _MonitorControls({
    required this.store,
    required this.focused,
    required this.onSelect,
  });

  Future<void> _apply(
    BuildContext context,
    DestinationControlVm row,
    double value,
  ) async {
    if (!store.monitorControlIsCurrent(row) ||
        !row.allowsValue(value) ||
        value == row.value) {
      return;
    }
    final error = await store.setMonitorControl(row, value);
    if (context.mounted && error != null) {
      InlineFeedbackScope.show(context, error);
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    bool isFocused(DestinationControlVm row) =>
        focused != null &&
        focused!.epoch == row.epoch &&
        focused!.target == row.target &&
        focused!.surface == row.surface &&
        focused!.id == row.id;
    return Container(
      padding: const EdgeInsets.all(AppDesign.spaceMd),
      decoration: BoxDecoration(
        color: palette.panel,
        border: Border.all(color: palette.divider),
        borderRadius: BorderRadius.circular(4),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'Monitor',
            style: TextStyle(
              fontFamily: U4.font,
              fontSize: 16,
              color: palette.text1,
            ),
          ),
          const SizedBox(height: AppDesign.spaceSm),
          ConsoleActionGroup(
            children: [
              for (final row in store.monitorControls)
                if (row.choices.isNotEmpty)
                  PopupMenuButton<double>(
                    popUpAnimationStyle: AnimationStyle.noAnimation,
                    enabled: store.monitorControlIsCurrent(row),
                    tooltip: row.description,
                    color: palette.panel,
                    onSelected: (value) => _apply(context, row, value),
                    itemBuilder: (_) => [
                      for (final choice in row.choices)
                        PopupMenuItem(
                          value: choice.value,
                          child: Text(choice.label),
                        ),
                    ],
                    child: IntrinsicWidth(
                        child: Container(
                      constraints: const BoxConstraints(
                          minHeight: AppDesign.touchTarget),
                      alignment: Alignment.center,
                      padding: AppDesign.controlPadding,
                      decoration: BoxDecoration(
                          color: palette.control,
                          border: Border.all(color: palette.lineCtl),
                          borderRadius: U4.rBtn),
                      child: Text('${row.label}: ${row.valueLabel ?? '-'}',
                          style: AppDesign.type(AppDesign.body,
                              color: palette.text1, weight: FontWeight.w600)),
                    )),
                  )
                else if (row.numeric)
                  Semantics(
                      selected: isFocused(row),
                      child: ConsoleButton(
                        label: row.label,
                        selected: isFocused(row),
                        onPressed: !store.monitorControlIsCurrent(row) ||
                                !row.available
                            ? null
                            : () => onSelect(row),
                      )),
            ],
          ),
        ],
      ),
    );
  }
}
