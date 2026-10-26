import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart';

import '../core/output_meters.dart';
import '../palette.dart';

/// Read-only level telemetry. No timer or dependency on the application store.
class OutputLevelMeter extends StatelessWidget {
  final ValueListenable<OutputMeterVm> meter;
  const OutputLevelMeter({super.key, required this.meter});

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<OutputMeterVm>(
        valueListenable: meter,
        builder: (context, value, _) {
          final palette = ConsolePalette.of(context);
          return Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            child: Tooltip(
              message: value.inferred
                  ? 'Read-only measured level. Output mapping awaits physical correlation; tap position is unspecified.'
                  : 'Read-only measured output level',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    value.available
                        ? 'Output level'
                        : 'Output level unavailable',
                  ),
                  for (var i = 0; i < value.levels.length; i++)
                    Row(
                      children: [
                        if (value.levels.length == 2)
                          SizedBox(width: 20, child: Text(i == 0 ? 'L' : 'R')),
                        Expanded(
                          child: LinearProgressIndicator(
                            value: value.levels[i],
                            color: palette.meter,
                            backgroundColor: palette.panel,
                          ),
                        ),
                        SizedBox(
                          width: 76,
                          child: Text(
                            value.levels[i] > 0
                                ? '${(20 * math.log(value.levels[i]) / math.ln10).toStringAsFixed(1)} dB'
                                : '−∞ dB',
                            textAlign: TextAlign.end,
                          ),
                        ),
                      ],
                    ),
                ],
              ),
            ),
          );
        },
      );
}
