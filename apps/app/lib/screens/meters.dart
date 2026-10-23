// Meter-bridge screen: grouped 0..-50 dB ladders with bottom scribbles, peak-hold + clip where
// the engine has the data. Read-only: ladders never dispatch, so this file
// sends no fader/mute/pan commands.
//
// Wiring: renders through [MetersStore] (constructor injection — no global
// singleton, so this never fights the Area A worker). Levels come ONLY
// from the live engine path via the store: offline every ladder reads
// -inf (dark, never sim-animated, never faked). The screen owns no meter
// loop of its own; freshness comes from the single shell-owned 80 ms loop
// (AppShell -> AppStore.pollMeters) and this page only sends
// `spectrum_target` (the visible strip) while visible, clearing it on
// dispose — mirroring the web overlay behaviour.
import 'dart:async';

import 'package:flutter/material.dart';

import '../theme.dart';
import '../palette.dart';

/// Meter scale (X-Air Edit pattern: 0..-50 dB).
const bridgeTopDb = 0.0;
const bridgeFloorDb = -50.0;

/// One ladder: a global strip slot on the live world.
class BridgeLadder {
  final int ch;
  final String? label; // live strip name when online, null offline
  final String sub; // family-relative number, e.g. `3`

  const BridgeLadder({required this.ch, this.label, required this.sub});
}

/// One pattern group: ladders when the desk has the strips, else collapsed
/// with a reason (never phantom ladders).
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

/// Live tap for one strip. Level is dB; [double.negativeInfinity] means
/// honestly no data (offline / no verified tap) and renders dark.
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

/// Minimal store contract the bridge renders. Implemented by the
/// shell/owner over the live engine; offline the groups collapse and every
/// tap reads -inf.
abstract class MetersStore extends ChangeNotifier {
  bool get online;
  bool get demo;
  String? get deskName;
  List<BridgeGroup> get groups;
  BridgeTap tapOf(int ch);

  /// 80 ms meter-loop target: pull engine levels into the taps in place.
  Future<void> pollMeters();

  /// `spectrum_target` cmd surface: the visible strip (or null to clear).
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
  int? _target;

  @override
  void initState() {
    super.initState();
    widget.store.addListener(_retarget);
    WidgetsBinding.instance.addPostFrameCallback((_) => _retarget());
  }

  /// Keep the spectrum target on the first visible ladder.
  void _retarget() {
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
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            final liveCount =
                store.groups.fold<int>(0, (n, g) => n + g.ladders.length);
            return Column(
              children: [
                _BridgeHeader(
                  online: store.online,
                  demo: store.demo,
                  deskName: store.deskName,
                  liveCount: liveCount,
                  onClose:
                      widget.onClose ?? () => Navigator.of(context).maybePop(),
                ),
                Expanded(
                  child: SingleChildScrollView(
                    padding: const EdgeInsets.fromLTRB(12, 10, 12, 16),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        if (!store.online)
                          Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 10, vertical: 8),
                            margin: const EdgeInsets.only(bottom: 14),
                            decoration: BoxDecoration(
                              color: ConsolePalette.of(context).panel,
                              border: Border.all(
                                  color: ConsolePalette.of(context).divider),
                              borderRadius:
                                  const BorderRadius.all(Radius.circular(4)),
                            ),
                            child: Text(
                              'Offline — ladders are dark until a live link is up. Connect for live meters.',
                              style: TextStyle(
                                  fontFamily: U4.font,
                                  fontSize: 12,
                                  color: ConsolePalette.of(context).text7),
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
                      ],
                    ),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _BridgeHeader extends StatelessWidget {
  final bool online;
  final bool demo;
  final String? deskName;
  final int liveCount;
  final VoidCallback onClose;

  const _BridgeHeader({
    required this.online,
    required this.demo,
    required this.deskName,
    required this.liveCount,
    required this.onClose,
  });

  @override
  Widget build(BuildContext context) {
    final badge = !online
        ? (
            'OFFLINE',
            ConsolePalette.of(context).solo,
            ConsolePalette.of(context).onSolo
          )
        : demo
            ? (
                'DEMO',
                ConsolePalette.of(context).solo,
                ConsolePalette.of(context).onSolo
              )
            : (
                'LIVE · ${deskName ?? ''}',
                ConsolePalette.of(context).successContainer,
                ConsolePalette.of(context).onSuccessContainer
              );
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        border: Border(
            bottom: BorderSide(color: ConsolePalette.of(context).lineHeader)),
      ),
      child: Row(
        children: [
          Text('METERS',
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 15,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.6,
                  color: ConsolePalette.of(context).text1)),
          const Spacer(),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
            decoration: BoxDecoration(
              color: badge.$2,
              borderRadius: U4.rBtn,
            ),
            child: Text(badge.$1,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: badge.$3)),
          ),
          const SizedBox(width: 8),
          Text('$liveCount ladders',
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 11,
                  color: ConsolePalette.of(context).text7)),
          const SizedBox(width: 8),
          GestureDetector(
            onTap: onClose,
            child: Container(
              width: 44,
              height: 44,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: ConsolePalette.of(context).control,
                border: Border.all(color: ConsolePalette.of(context).lineCtl),
                borderRadius: U4.rBtn,
              ),
              child: const Text('✕',
                  textAlign: TextAlign.center,
                  style: TextStyle(fontFamily: U4.font, fontSize: 16)),
            ),
          ),
        ],
      ),
    );
  }
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
    return Container(
      padding: const EdgeInsets.fromLTRB(10, 8, 10, 10),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).panel,
        border: Border.all(color: ConsolePalette.of(context).linePanel),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(group.title,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      color: ConsolePalette.of(context).text1)),
              if (!online)
                Padding(
                  padding: EdgeInsets.only(left: 8),
                  child: Text('no data',
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: 10,
                          color: ConsolePalette.of(context).text7)),
                ),
            ],
          ),
          const SizedBox(height: 6),
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
            Text(group.title,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).text7)),
            const SizedBox(width: 10),
            Expanded(
              child: Text(reason,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 11,
                      color: ConsolePalette.of(context).text10)),
            ),
          ],
        ),
      ),
    );
  }
}

/// One read-only bridge ladder: CustomPainter column + scribble below.
class _Ladder extends StatefulWidget {
  final BridgeLadder ladder;
  final BridgeTap tap;

  const _Ladder({required this.ladder, required this.tap});

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
      width: 52,
      child: Column(
        children: [
          CustomPaint(
            size: const Size(44, 229),
            painter: _painter,
          ),
          const SizedBox(height: 4),
          Text(
            widget.ladder.label ?? '—',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: 11,
                color: ConsolePalette.of(context).text1),
          ),
          Text(
            widget.ladder.sub,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            textAlign: TextAlign.center,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: 10,
                color: ConsolePalette.of(context).text7),
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
              fontFeatures: [FontFeature.tabularFigures()]),
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

    // Column well.
    canvas.drawRect(
        Rect.fromLTWH(colL, colTop, colW, h), Paint()..color = palette.well);

    // Fill from the bottom up, banded red >= -6 / amber >= -12 / green.
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
          canvas.drawRect(Rect.fromLTWH(colL, top, colW, yLo - top),
              Paint()..color = band.c);
        }
      }
    }

    // Scale ticks + labels (0/-12/-24/-36/-50) in the left gutter.
    for (final mark in const [0.0, -12.0, -24.0, -36.0, -50.0]) {
      final y = _yOf(mark, colTop, h);
      canvas.drawRect(Rect.fromLTWH(colL - 4, y - 0.5, 4, 1),
          Paint()..color = palette.scaleTick);
      final tp = _labels[mark == 0 ? '0' : mark.toInt().toString()]!;
      tp.paint(canvas, Offset(colL - 5 - tp.width, y - tp.height / 2));
    }

    // Peak-hold line (2px), hidden without data.
    final peak = tap.peakDb.value;
    if (hasData && peak.isFinite && peak > bridgeFloorDb) {
      final py = _yOf(peak.clamp(bridgeFloorDb, bridgeTopDb), colTop, h)
          .clamp(colTop, colTop + h - 2);
      canvas.drawRect(
          Rect.fromLTWH(colL, py, colW, 2), Paint()..color = palette.peakLine);
    }

    // Clip square above the column.
    canvas.drawRect(
      Rect.fromLTWH(colL + 1, 0, clipS, clipS),
      Paint()..color = tap.clip.value ? palette.clipOn : palette.clipOff,
    );
  }

  @override
  bool shouldRepaint(_LadderPainter old) =>
      old.tap != tap || old.palette != palette;
}
