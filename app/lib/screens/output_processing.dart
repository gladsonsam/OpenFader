import 'dart:math' as math;
import 'package:flutter/material.dart';

import '../core/destination_controls.dart';
import '../core/output_processing.dart';
import '../palette.dart';
import '../widgets/destination_control_editor.dart';
import 'output_processing_graph.dart';

/// Output topology and controls come entirely from provider descriptors.
/// No strip-count, provider-name or send-slot assumptions live in this page.
class OutputProcessingPage extends StatefulWidget {
  final OutputProcessingStore store;
  final VoidCallback onClose;
  const OutputProcessingPage({
    super.key,
    required this.store,
    required this.onClose,
  });

  @override
  State<OutputProcessingPage> createState() => _OutputProcessingPageState();
}

class _OutputProcessingPageState extends State<OutputProcessingPage> {
  String? _surface;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final store = widget.store;
          final palette = ConsolePalette.of(context);
          final controls = store.outputProcessingControls;
          final targets = <String, String>{
            for (final control in controls)
              control.target: control.destinationName,
          };
          final target = store.outputProcessingTarget;
          final selected =
              controls.where((control) => control.target == target).toList();
          final surfaces =
              selected.map((control) => control.surface).toSet().toList();
          const labels = {
            'output_peq': 'Parametric EQ',
            'output_geq': 'Graphic EQ',
            'output_comp': 'Compressor / Limiter',
            'output_delay': 'Delay',
          };
          surfaces.sort(
            (a, b) => labels.keys
                .toList()
                .indexOf(a)
                .compareTo(labels.keys.toList().indexOf(b)),
          );
          final surface =
              surfaces.contains(_surface) ? _surface : surfaces.firstOrNull;
          final processor =
              selected.where((control) => control.surface == surface).toList();
          return Scaffold(
            backgroundColor: palette.page,
            appBar: AppBar(
              backgroundColor: palette.panel,
              leading: IconButton(
                onPressed: widget.onClose,
                tooltip: 'Mixer',
                icon: const Icon(Icons.arrow_back),
              ),
              title: const Text('Output processing'),
            ),
            body: SafeArea(
              top: false,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Padding(
                    padding: const EdgeInsets.all(12),
                    child: DropdownButtonFormField<String>(
                      key: ValueKey((target, targets.length)),
                      initialValue: targets.containsKey(target) ? target : null,
                      isExpanded: true,
                      decoration: const InputDecoration(labelText: 'Output'),
                      items: [
                        for (final entry in targets.entries)
                          DropdownMenuItem(
                            value: entry.key,
                            child: Text(
                              entry.value,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                      ],
                      onChanged: (value) {
                        if (value != null) store.openOutputProcessing(value);
                      },
                    ),
                  ),
                  if (surfaces.isNotEmpty)
                    SingleChildScrollView(
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                      child: Row(
                        children: [
                          for (final item in surfaces)
                            Padding(
                              padding: const EdgeInsets.only(right: 8),
                              child: ChoiceChip(
                                label: Text(labels[item] ?? item),
                                selected: item == surface,
                                onSelected: (_) =>
                                    setState(() => _surface = item),
                              ),
                            ),
                        ],
                      ),
                    ),
                  Expanded(
                    child: processor.isEmpty
                        ? const Center(
                            child: Text('Output processing is unavailable.'),
                          )
                        : ListView(
                            padding: const EdgeInsets.all(12),
                            children: [
                              if (surface == 'output_peq' ||
                                  surface == 'output_geq' ||
                                  surface == 'output_comp')
                                OutputProcessingGraph(
                                  controls: processor,
                                  surface: surface!,
                                ),
                              const SizedBox(height: 12),
                              if (surface == 'output_geq') ...[
                                for (final control in processor.where(
                                  (c) =>
                                      !RegExp(r'^geq\d+_gain$').hasMatch(c.id),
                                ))
                                  _ControlTile(
                                    key: ValueKey((control.target, control.id)),
                                    store: store,
                                    control: control,
                                  ),
                                const SizedBox(height: 12),
                                SizedBox(
                                  height: 310,
                                  child: SingleChildScrollView(
                                    scrollDirection: Axis.horizontal,
                                    child: Row(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.stretch,
                                      children: [
                                        for (final control in processor.where(
                                          (c) => RegExp(r'^geq\d+_gain$')
                                              .hasMatch(c.id),
                                        ))
                                          SizedBox(
                                            width: 74,
                                            child: _ControlTile(
                                              key: ValueKey((
                                                control.target,
                                                control.id,
                                              )),
                                              store: store,
                                              control: control,
                                              vertical: true,
                                            ),
                                          ),
                                      ],
                                    ),
                                  ),
                                ),
                              ] else
                                for (final control in processor)
                                  _ControlTile(
                                    key: ValueKey((control.target, control.id)),
                                    store: store,
                                    control: control,
                                  ),
                            ],
                          ),
                  ),
                ],
              ),
            ),
          );
        },
      );
}

/// A drag commits once on release. The captured descriptor is retained so a
/// peer edit or connection change during the gesture rejects the stale write.
class _ControlTile extends StatefulWidget {
  final OutputProcessingStore store;
  final DestinationControlVm control;
  final bool vertical;
  const _ControlTile({
    super.key,
    required this.store,
    required this.control,
    this.vertical = false,
  });
  @override
  State<_ControlTile> createState() => _ControlTileState();
}

class _ControlTileState extends State<_ControlTile> {
  DestinationControlVm? _dragControl;
  double? _draft;
  bool _pending = false;

  @override
  void didUpdateWidget(covariant _ControlTile oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.store.destinationControlIsCurrent(widget.control)) {
      _draft = null;
      _dragControl = null;
    }
  }

  Future<void> _commit(DestinationControlVm control, double value) async {
    setState(() {
      _pending = true;
      _draft = null;
      _dragControl = null;
    });
    final error = await widget.store.setDestinationControl(control, value);
    if (!mounted) return;
    setState(() => _pending = false);
    if (error != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(error)));
    }
  }

  void _edit() {
    final control = widget.control;
    if (_pending || !widget.store.destinationControlIsCurrent(control)) return;
    showDialog<void>(
      context: context,
      builder: (_) => DestinationControlEditor(
        control: control,
        changes: widget.store,
        isCurrent: widget.store.destinationControlIsCurrent,
        apply: widget.store.setDestinationControl,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final control = widget.control;
    final palette = ConsolePalette.of(context);
    final enabled =
        !_pending && widget.store.destinationControlIsCurrent(control);
    final value = _draft ?? control.value;
    final logarithmic = control.unit == 'Hz' && (control.min ?? 0) > 0;
    double position(double v) => logarithmic
        ? math.log(v / control.min!) / math.log(control.max! / control.min!)
        : v;
    double engineering(double v) => logarithmic
        ? control.min! * math.pow(control.max! / control.min!, v)
        : v;
    final slider = control.numeric && value != null
        ? Slider(
            min: logarithmic ? 0 : control.min!,
            max: logarithmic ? 1 : control.max!,
            value: position(value.clamp(control.min!, control.max!)),
            label: '${value.toStringAsFixed(control.decimals)} ${control.unit}',
            onChangeStart: enabled ? (_) => _dragControl = control : null,
            onChanged:
                enabled ? (v) => setState(() => _draft = engineering(v)) : null,
            onChangeEnd: enabled
                ? (v) => _commit(_dragControl ?? control, engineering(v))
                : null,
          )
        : null;
    final readout = TextButton(
      onPressed: enabled ? _edit : null,
      child: Text(
        _pending
            ? 'Sending…'
            : (control.id == 'comp_ratio' && control.value == 101
                    ? '∞:1'
                    : control.valueLabel) ??
                'Unavailable',
        textAlign: TextAlign.center,
        style: TextStyle(color: enabled ? palette.lime : palette.mutedText),
      ),
    );
    if (widget.vertical) {
      return DecoratedBox(
        decoration: BoxDecoration(
          color: palette.well,
          border: Border.all(color: palette.line),
        ),
        child: Column(
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(control.label, textAlign: TextAlign.center),
            ),
            Expanded(
              child: RotatedBox(
                quarterTurns: 3,
                child: slider ?? const SizedBox.shrink(),
              ),
            ),
            readout,
          ],
        ),
      );
    }
    return Card(
      color: palette.panel,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Row(
              children: [
                Expanded(child: Text(control.label)),
                if (control.numeric) readout,
              ],
            ),
            if (slider != null) slider,
            if (!control.numeric)
              Wrap(
                spacing: 8,
                children: [
                  for (final choice in control.choices)
                    ChoiceChip(
                      label: Text(choice.label),
                      selected: choice.value == control.value,
                      onSelected: enabled
                          ? (_) => _commit(control, choice.value)
                          : null,
                    ),
                ],
              ),
            if (!control.available)
              Text(
                'Awaiting mixer value',
                style: TextStyle(color: palette.mutedText),
              ),
          ],
        ),
      ),
    );
  }
}
