library;

import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import '../ui/console.dart';

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
  return showConsoleDialog<double>(
    context,
    builder: (context) => StatefulBuilder(
      builder: (context, setState) => ConsoleDialog(
        title: title,
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
                style: consoleTextStyle(context, ConsoleTextRole.entryValue,
                    tone: ConsoleTextTone.accent),
                decoration: InputDecoration(
                  filled: true,
                  fillColor: ConsolePalette.of(context).well,
                  errorText: error,
                  errorStyle: consoleTextStyle(context, ConsoleTextRole.smallLabel,
                      tone: ConsoleTextTone.danger),
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
                  style: consoleTextStyle(context, ConsoleTextRole.caption,
                      tone: ConsoleTextTone.muted)),
            ],
          ),
        ),
        actions: [
          ConsoleButton(
            label: 'Cancel',
            variant: ConsoleButtonVariant.quiet,
            size: ConsoleButtonSize.regular,
            onPressed: () => Navigator.pop(context),
          ),
          ConsoleButton(
            label: 'Set',
            variant: ConsoleButtonVariant.primary,
            size: ConsoleButtonSize.regular,
            onPressed: () => _submit(context, ctl, setState, min, max,
                allowNegInf, negInfValue, (e) => error = e),
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
