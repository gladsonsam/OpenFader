import '../ui/overlays/console_dialogs.dart';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

/// A bounded selector for native option sets, without assuming their size or
/// interpreting provider labels as channel or FX identifiers.
class CompactChoicePicker<T> extends StatefulWidget {
  final List<T> choices;
  final String Function(T) label;
  final bool Function(T) selected;
  final ValueChanged<T>? onSelected;

  const CompactChoicePicker({
    super.key,
    required this.choices,
    required this.label,
    required this.selected,
    required this.onSelected,
  });

  @override
  State<CompactChoicePicker<T>> createState() => _CompactChoicePickerState<T>();
}

class _CompactChoicePickerState<T> extends State<CompactChoicePicker<T>> {
  String _query = '';

  @override
  Widget build(BuildContext context) {
    final choices = widget.choices
        .where((choice) => widget.label(choice).toLowerCase().contains(_query))
        .toList();
    // AlertDialog measures intrinsic dimensions before laying out its content.
    // Give the viewport a concrete extent and avoid LayoutBuilder here: neither
    // a viewport nor LayoutBuilder can answer those intrinsic measurements.
    final dialogWidth = math.min(420.0, MediaQuery.sizeOf(context).width - 128);
    final columns = dialogWidth >= 280 ? 2 : 1;
    final rowHeight =
        math.max(56.0, MediaQuery.textScalerOf(context).scale(14) * 2 + 20);
    final rows = (choices.length / columns).ceil();
    final gridHeight = math.min(300.0, rows * (rowHeight + 8) - 8);
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (widget.choices.length > 8) ...[
          TextField(
            decoration: const InputDecoration(
              labelText: 'Search options',
              prefixIcon: Icon(Icons.search),
              isDense: true,
            ),
            onChanged: (value) =>
                setState(() => _query = value.trim().toLowerCase()),
          ),
          const SizedBox(height: 12),
        ],
        if (choices.isEmpty)
          const Padding(
            padding: EdgeInsets.all(12),
            child: Text('No matching options'),
          )
        else
          SizedBox(
            height: gridHeight,
            child: GridView.builder(
              primary: false,
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: columns,
                mainAxisExtent: rowHeight,
                crossAxisSpacing: 8,
                mainAxisSpacing: 8,
              ),
              itemCount: choices.length,
              itemBuilder: (context, index) {
                final choice = choices[index];
                final label = widget.label(choice);
                return Tooltip(
                  message: label,
                  child: ChoiceChip(
                    label: SizedBox(
                      width: double.infinity,
                      child: Text(label,
                          maxLines: 2, overflow: TextOverflow.ellipsis),
                    ),
                    selected: widget.selected(choice),
                    onSelected: widget.onSelected == null
                        ? null
                        : (_) => widget.onSelected!(choice),
                  ),
                );
              },
            ),
          ),
      ],
    );
  }
}

/// Immediate selection with live invalidation when the target changes.
Future<T?> showCompactChoiceDialog<T>({
  required BuildContext context,
  required String title,
  required List<T> choices,
  required String Function(T) label,
  required bool Function(T) selected,
  required Listenable changes,
  required bool Function() isCurrent,
}) =>
    showDialog<T>(
      context: context,
      builder: (dialogContext) => ListenableBuilder(
        listenable: changes,
        builder: (context, _) {
          final current = isCurrent();
          return ConsoleAlertDialog(
            title: Text(title),
            scrollable: true,
            content: SizedBox(
              width: 420,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CompactChoicePicker<T>(
                    choices: choices,
                    label: label,
                    selected: selected,
                    onSelected: current
                        ? (choice) {
                            if (isCurrent()) {
                              Navigator.pop(dialogContext, choice);
                            }
                          }
                        : null,
                  ),
                  if (!current)
                    const Text(
                        'The setting or connection changed. Reopen this control.'),
                ],
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(dialogContext),
                child: const Text('Cancel'),
              ),
            ],
          );
        },
      ),
    );
