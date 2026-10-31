library;

import 'package:flutter/material.dart';
import '../palette.dart';
import 'package:flutter/services.dart';

import '../design.dart';

/// Show a numeric entry dialog for [title]. Returns the accepted value or
/// null when cancelled/invalid. [initial] is the live value text, [min]/[max]
/// clamp the accepted range. [allowNegInf] accepts "-inf" as [negInfValue].
Future<double?> showNumberEntry({
  required BuildContext context,
  required String title,
  required String initial,
  required double min,
  required double max,
  bool allowNegInf = false,
  double negInfValue = 0,
}) {
  final ctl = TextEditingController(text: initial);
  String? error;
  return showDialog<double>(
    animationStyle: AnimationStyle.noAnimation,
    context: context,
    builder: (context) => StatefulBuilder(
      builder: (context, setState) => AlertDialog(
        backgroundColor: ConsolePalette.of(context).panel,
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(12),
            side: BorderSide(color: ConsolePalette.of(context).line)),
        title: Text(title, style: AppDesign.type(18, weight: FontWeight.w600)),
        content: SizedBox(
          width: 280,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                controller: ctl,
                autofocus: true,
                keyboardType: const TextInputType.numberWithOptions(
                    signed: true, decimal: true),
                inputFormatters: [
                  FilteringTextInputFormatter.allow(
                      RegExp(r'[-+0-9.,infIN°F ]')),
                ],
                style: AppDesign.type(24,
                    color: ConsolePalette.of(context).lime,
                    weight: FontWeight.w700),
                decoration: InputDecoration(
                  filled: true,
                  fillColor: ConsolePalette.of(context).well,
                  errorText: error,
                  errorStyle: AppDesign.type(13,
                      color: ConsolePalette.of(context).mute),
                  border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(8),
                      borderSide:
                          BorderSide(color: ConsolePalette.of(context).line)),
                  enabledBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(8),
                      borderSide:
                          BorderSide(color: ConsolePalette.of(context).line)),
                  focusedBorder: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(8),
                      borderSide:
                          BorderSide(color: ConsolePalette.of(context).lime)),
                  contentPadding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 12),
                ),
                onSubmitted: (_) => _submit(context, ctl, setState, min, max,
                    allowNegInf, negInfValue, (e) => error = e),
              ),
              const SizedBox(height: 6),
              Text(
                  'Range ${min.toStringAsFixed(1)} … ${max.toStringAsFixed(1)}',
                  style: AppDesign.type(12,
                      color: ConsolePalette.of(context).mutedText)),
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text('Cancel',
                style: AppDesign.type(16,
                    color: ConsolePalette.of(context).mutedText)),
          ),
          TextButton(
            onPressed: () => _submit(context, ctl, setState, min, max,
                allowNegInf, negInfValue, (e) => error = e),
            child: Text('Set',
                style: AppDesign.type(16,
                    color: ConsolePalette.of(context).lime,
                    weight: FontWeight.w600)),
          ),
        ],
      ),
    ),
  );
}

void _submit(
    BuildContext context,
    TextEditingController ctl,
    StateSetter setState,
    double min,
    double max,
    bool allowNegInf,
    double negInfValue,
    void Function(String? e) setError) {
  final raw = ctl.text.trim().toLowerCase().replaceAll(',', '.');
  if (allowNegInf && (raw == '-inf' || raw == '-∞' || raw == '−∞')) {
    Navigator.pop(context, negInfValue);
    return;
  }
  final v = double.tryParse(raw.replaceAll('−', '-'));
  if (v == null || !v.isFinite) {
    setState(() => setError('Enter a number.'));
    return;
  }
  Navigator.pop(context, v.clamp(min, max).toDouble());
}
