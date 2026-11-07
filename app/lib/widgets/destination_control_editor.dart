import 'package:flutter/material.dart';

import '../core/destination_controls.dart';
import '../palette.dart';
import 'compact_choice_picker.dart';

class DestinationControlEditor extends StatefulWidget {
  final Listenable changes;
  final int Function() accessEpoch;
  final bool Function(DestinationControlVm) isCurrent;
  final Future<String?> Function(DestinationControlVm, double) apply;
  final DestinationControlVm control;
  const DestinationControlEditor(
      {super.key,
      required this.changes,
      required this.accessEpoch,
      required this.isCurrent,
      required this.apply,
      required this.control});
  @override
  State<DestinationControlEditor> createState() =>
      _DestinationControlEditorState();
}

class _DestinationControlEditorState extends State<DestinationControlEditor> {
  late double _selected = widget.control.value!;
  late final int _accessEpoch;
  bool get _current =>
      widget.accessEpoch() == _accessEpoch && widget.isCurrent(widget.control);
  bool _sending = false;
  String? _error;
  TextEditingController? _number;

  @override
  void initState() {
    super.initState();
    _accessEpoch = widget.accessEpoch();
    if (widget.control.numeric) {
      final value = widget.control.value!;
      _number = TextEditingController(
          text: value.toStringAsFixed(widget.control.decimals));
    }
  }

  @override
  void dispose() {
    _number?.dispose();
    super.dispose();
  }

  Future<void> _apply() async {
    if (!_current) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    final error = await widget.apply(widget.control, _selected);
    if (!mounted) return;
    if (error == null) {
      Navigator.of(context).pop();
    } else {
      setState(() {
        _sending = false;
        _error = error;
      });
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
      listenable: widget.changes,
      builder: (context, _) {
        final control = widget.control;
        final current = _current;
        final palette = ConsolePalette.of(context);
        return PopScope(
            canPop: !_sending,
            child: AlertDialog(
                scrollable: true,
                title: Text('${control.label} · ${control.destinationName}'),
                content: SizedBox(
                    width: 420,
                    child: Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          Text(control.description),
                          if (control.authority == 'accepted') ...[
                            const SizedBox(height: 8),
                            const Text(
                                'The displayed value is accepted but not independently confirmed.'),
                          ],
                          const SizedBox(height: 8),
                          if (control.numeric)
                            TextField(
                                controller: _number,
                                enabled: current && !_sending,
                                keyboardType: TextInputType.numberWithOptions(
                                    decimal: true, signed: control.min! < 0),
                                decoration: InputDecoration(
                                    labelText: control.label,
                                    suffixText: control.unit,
                                    helperText:
                                        '${control.min!.toStringAsFixed(control.decimals)}–${control.max!.toStringAsFixed(control.decimals)}${control.unit.isEmpty ? '' : ' ${control.unit}'}',
                                    errorText: control.allowsValue(_selected)
                                        ? null
                                        : 'Enter a value within this range.'),
                                onChanged: (text) => setState(() {
                                      _selected =
                                          double.tryParse(text) ?? double.nan;
                                    })),
                          if (control.choices.isNotEmpty)
                            CompactChoicePicker<DestinationChoiceVm>(
                                choices: control.choices,
                                label: (choice) => choice.label,
                                selected: (choice) => choice.value == _selected,
                                onSelected: current && !_sending
                                    ? (choice) =>
                                        setState(() => _selected = choice.value)
                                    : null),
                          if (!current && !_sending) ...[
                            const SizedBox(height: 8),
                            const Text(
                                'Connection or setting changed. Close and reopen this control.'),
                          ],
                          if (_error != null) ...[
                            const SizedBox(height: 8),
                            Text(_error!,
                                style: TextStyle(color: palette.meterAmber)),
                          ],
                        ])),
                actions: [
                  TextButton(
                      onPressed:
                          _sending ? null : () => Navigator.of(context).pop(),
                      child: const Text('Cancel')),
                  TextButton(
                      onPressed: current &&
                              !_sending &&
                              control.allowsValue(_selected) &&
                              _selected != control.value
                          ? _apply
                          : null,
                      child: Text(_sending
                          ? 'Sending…'
                          : 'Apply to ${control.targetLabel}')),
                ]));
      });
}
