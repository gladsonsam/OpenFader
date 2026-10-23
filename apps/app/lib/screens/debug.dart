// Area D: world-debug inspector. Raw engine
// inspector (verification only, not design): connection/link/info/deny,
// strip table, meter sparkline (CustomPainter), caps/counts, plus probe
// buttons (fader/mute/hello/stats/scan/info).
//
// Wiring: everything flows through [DebugStore] (constructor injection —
// Area A implements it over the engine via cmdStats/Hello; this file only
// renders ui4 tokens, see lib/theme.dart). Monospace text, plain buttons.
import 'dart:async';

import 'package:flutter/material.dart';

import '../theme.dart';
import '../widgets/console_button.dart';

/// One inspector section (title + preformatted lines).
class DebugSection {
  final String title;
  final List<String> lines;

  const DebugSection({required this.title, required this.lines});
}

/// Minimal store contract the debug screen renders. Implemented by the
/// shell/owner over the live engine.
abstract class DebugStore extends ChangeNotifier {
  int get tick;
  List<DebugSection> get sections;
  List<double> get spark; // meter levels 0..1

  Future<void> refresh(); // snapshot (+ sweep)
  void sendHello(); // cmd hello
  void sendStats(); // cmd stats
  void nudgeFader(); // ch0 fader +0.05
  void toggleMute(); // ch0 mute toggle
  void scan(); // cmd scan
  void requestInfo(); // cmd info
}

class U4DebugPage extends StatefulWidget {
  final DebugStore store;

  const U4DebugPage({super.key, required this.store});

  @override
  State<U4DebugPage> createState() => _U4DebugPageState();
}

class _U4DebugPageState extends State<U4DebugPage> {
  Timer? _loop;

  @override
  void initState() {
    super.initState();
    widget.store.refresh();
    _loop = Timer.periodic(
        const Duration(milliseconds: 500), (_) => widget.store.refresh());
  }

  @override
  void dispose() {
    _loop?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            return SingleChildScrollView(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text('world-debug · tick ${store.tick}',
                      style: const TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          color: U4.text1)),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    children: [
                      _Probe(label: 'ch0 fader +', onTap: store.nudgeFader),
                      _Probe(label: 'ch0 mute', onTap: store.toggleMute),
                      _Probe(label: 'hello', onTap: store.sendHello),
                      _Probe(label: 'stats', onTap: store.sendStats),
                      _Probe(label: 'scan', onTap: store.scan),
                      _Probe(label: 'info', onTap: store.requestInfo),
                      _Probe(label: 'refresh', onTap: () => store.refresh()),
                    ],
                  ),
                  const SizedBox(height: 10),
                  const Text('meters',
                      style: TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          color: U4.cyan)),
                  const SizedBox(height: 4),
                  Container(
                    decoration: BoxDecoration(
                      color: U4.panel,
                      border: Border.all(color: U4.lineCtl),
                    ),
                    child: CustomPaint(
                      size: const Size(double.infinity, 64),
                      painter: _SparkPainter(levels: store.spark),
                    ),
                  ),
                  const SizedBox(height: 10),
                  for (final s in store.sections) ...[
                    Text(s.title,
                        style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            color: U4.cyan)),
                    for (final l in s.lines)
                      Text(l,
                          style: const TextStyle(
                              fontFamily: 'monospace',
                              fontSize: 11,
                              height: 1.45,
                              color: U4.text1)),
                    const SizedBox(height: 8),
                  ],
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _Probe extends StatelessWidget {
  final String label;
  final VoidCallback onTap;

  const _Probe({required this.label, required this.onTap});

  @override
  Widget build(BuildContext context) {
    return ConsoleButton(label: label, onPressed: onTap);
  }
}

/// Meter sparkline: one bar per slot, red past 0.966, white peak tick.
class _SparkPainter extends CustomPainter {
  final List<double> levels;

  const _SparkPainter({required this.levels});

  @override
  void paint(Canvas canvas, Size size) {
    final n = levels.length;
    if (n == 0 || size.height <= 0) return;
    final w = size.width / n;
    for (var i = 0; i < n; i++) {
      final v = levels[i].clamp(0.0, 1.0);
      final h = v * size.height;
      canvas.drawRect(
        Rect.fromLTWH(i * w, size.height - h, (w - 1).clamp(1.0, w), h),
        Paint()..color = v > 0.966 ? const Color(0xFFFF0000) : U4.cyan,
      );
      canvas.drawRect(
        Rect.fromLTWH(i * w, size.height - h, (w - 1).clamp(1.0, w), 1),
        Paint()..color = Colors.white,
      );
    }
  }

  @override
  bool shouldRepaint(_SparkPainter old) => !identical(old.levels, levels);
}
