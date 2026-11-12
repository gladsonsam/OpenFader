import '../design.dart';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';
import 'package:flutter/foundation.dart';

import '../core/output_meters.dart';
import '../palette.dart';

/// Read-only level telemetry. No timer or dependency on the application store.
class OutputLevelMeter extends StatelessWidget {
  final ValueListenable<OutputMeterVm> meter;
  final bool showCompressorGr;
  final String label;
  const OutputLevelMeter({
    super.key,
    required this.meter,
    this.showCompressorGr = false,
    this.label = 'Output level',
  });

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<OutputMeterVm>(
        valueListenable: meter,
        builder: (context, value, _) {
          final palette = ConsolePalette.of(context);
          return Padding(
            padding: const EdgeInsets.symmetric(
                horizontal: AppDesign.spaceMd, vertical: AppDesign.spaceSm),
            child: Tooltip(
              message: value.inferred
                  ? 'Read-only measured level. Output mapping awaits physical correlation; tap position is unspecified.'
                  : 'Read-only measured output level',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    value.available ? label : '$label unavailable',
                  ),
                  if (showCompressorGr && value.compressorGr != null)
                    Tooltip(
                      message: value.compressorGr!.reductionDb == null
                          ? 'The provider has no verified output GR sample. Wire layout and scale await capture.'
                          : value.compressorGr!.inferred
                              ? 'Measured attenuation; output mapping awaits physical correlation.'
                              : 'Measured compressor attenuation in dB',
                      child: Text(
                        value.compressorGr!.reductionDb == null
                            ? 'Compressor GR unavailable'
                            : 'Compressor GR: ${[
                                for (var i = 0;
                                    i < value.compressorGr!.reductionDb!.length;
                                    i++)
                                  '${value.compressorGr!.reductionDb!.length == 2 ? (i == 0 ? "L " : "R ") : ""}${value.compressorGr!.reductionDb![i].toStringAsFixed(1)} dB',
                              ].join(' · ')}',
                      ),
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
