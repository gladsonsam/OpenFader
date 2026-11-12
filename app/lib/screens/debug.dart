// Debug inspector: raw engine state for verification, not design display.
// Screens stay constructor-injected so Area A can implement the store.
import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart';

import '../ui/console.dart';

class DebugSection {
  final String title;
  final List<String> lines;

  const DebugSection({required this.title, required this.lines});
}

// Implemented by the shell over the live engine.
abstract class DebugStore extends ChangeNotifier {
  int get tick;
  List<DebugSection> get sections;
  List<double> get spark; // meter levels 0..1

  Future<void> refresh();
  void sendHello();
  void sendStats();
  void scan();
  void requestInfo();
}

class U4DebugPage extends StatefulWidget {
  final DebugStore store;

  const U4DebugPage({super.key, required this.store});

  @override
  State<U4DebugPage> createState() => _U4DebugPageState();
}

class _U4DebugPageState extends State<U4DebugPage> {
  @override
  void initState() {
    super.initState();
    widget.store.refresh();
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
            return SingleChildScrollView(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text('Diagnostics · tick ${store.tick}',
                      style: TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          color: ConsolePalette.of(context).text1)),
                  const SizedBox(height: 8),
                  ConsoleActionGroup(
                    children: [
                      _Probe(label: 'hello', onTap: store.sendHello),
                      _Probe(label: 'stats', onTap: store.sendStats),
                      _Probe(label: 'scan', onTap: store.scan),
                      _Probe(label: 'info', onTap: store.requestInfo),
                      _Probe(label: 'refresh', onTap: () => store.refresh()),
                    ],
                  ),
                  const SizedBox(height: 10),
                  Text('meters',
                      style: TextStyle(
                          fontFamily: 'monospace',
                          fontSize: 12,
                          color: ConsolePalette.of(context).cyan)),
                  const SizedBox(height: 4),
                  Container(
                    decoration: BoxDecoration(
                      color: ConsolePalette.of(context).panel,
                      border:
                          Border.all(color: ConsolePalette.of(context).lineCtl),
                    ),
                    child: CustomPaint(
                      size: const Size(double.infinity, 64),
                      painter: _SparkPainter(
                          levels: store.spark,
                          palette: ConsolePalette.of(context)),
                    ),
                  ),
                  const SizedBox(height: 10),
                  for (final s in store.sections) ...[
                    Text(s.title,
                        style: TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 12,
                            color: ConsolePalette.of(context).cyan)),
                    for (final l in s.lines)
                      Text(l,
                          style: TextStyle(
                              fontFamily: 'monospace',
                              fontSize: 11,
                              height: 1.45,
                              color: ConsolePalette.of(context).text1)),
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

class _SparkPainter extends CustomPainter {
  final List<double> levels;

  final ConsolePalette palette;

  _SparkPainter({required List<double> levels, required this.palette})
      : levels = List<double>.unmodifiable(levels);

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
        Paint()..color = v > 0.966 ? palette.meterRed : palette.cyan,
      );
      canvas.drawRect(
        Rect.fromLTWH(i * w, size.height - h, (w - 1).clamp(1.0, w), 1),
        Paint()..color = palette.peakLine,
      );
    }
  }

  @override
  bool shouldRepaint(_SparkPainter old) =>
      old.palette != palette || !listEquals(old.levels, levels);
}
