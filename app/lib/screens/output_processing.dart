import 'dart:math' as math;
import 'package:flutter/material.dart';

import '../core/destination_controls.dart';
import '../core/output_processing.dart';
import '../core/output_geq.dart';
import '../palette.dart';
import '../widgets/destination_control_editor.dart';
import '../widgets/output_processor_presets.dart';
import '../widgets/output_geq_editor.dart';
import '../widgets/output_level_meter.dart';
import 'output_processing_graph.dart';

/// Output topology and controls come entirely from provider descriptors.
/// No strip-count, provider-name or send-slot assumptions live in this page.
class OutputProcessingPage extends StatefulWidget {
  final OutputProcessingStore store;
  final VoidCallback onClose;
  final bool embedded;
  const OutputProcessingPage({
    super.key,
    required this.store,
    required this.onClose,
    this.embedded = false,
  });

  @override
  State<OutputProcessingPage> createState() => _OutputProcessingPageState();
}

class _OutputProcessingPageState extends State<OutputProcessingPage> {
  String? _surface;
  String? _focusTarget, _focusSurface, _focusId;
  double? _focusDraft;
  DestinationControlVm? _focusCaptured;
  bool _focusPending = false;
  void _focus(DestinationControlVm control, double? draft, bool pending) {
    setState(() {
      _focusTarget = control.target;
      _focusSurface = control.surface;
      _focusId = control.id;
      _focusDraft = draft;
      _focusCaptured = control;
      _focusPending = pending;
    });
  }

  void _complete(DestinationControlVm control) {
    if (_focusTarget == control.target &&
        _focusSurface == control.surface &&
        _focusId == control.id) {
      setState(() {
        _focusPending = false;
        _focusDraft = null;
      });
    }
  }

  void _clearFocus() {
    _focusTarget = _focusSurface = _focusId = null;
    _focusDraft = null;
    _focusCaptured = null;
    _focusPending = false;
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final store = widget.store;
          final palette = ConsolePalette.of(context);
          final controls = store.outputProcessingControls;
          final targets = store.outputTargets;
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
            'output_controls': 'Output controls',
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
          if (_focusTarget != target ||
              _focusSurface != surface ||
              !processor.any((c) => c.id == _focusId)) {
            _clearFocus();
          }
          if (_focusDraft != null &&
              _focusCaptured != null &&
              !store.destinationControlIsCurrent(_focusCaptured!)) {
            _focusDraft = null;
          }
          final focused = processor.where((c) => c.id == _focusId).firstOrNull;
          return Scaffold(
            backgroundColor: palette.page,
            appBar: widget.embedded && focused == null
                ? null
                : AppBar(
                    toolbarHeight: widget.embedded ? 48 : null,
                    automaticallyImplyLeading: false,
                    backgroundColor: palette.panel,
                    leading: widget.embedded
                        ? null
                        : IconButton(
                            onPressed: widget.onClose,
                            tooltip: 'Mixer',
                            icon: const Icon(Icons.arrow_back),
                          ),
                    title: focused == null
                        ? const Text('Output processing')
                        : TextButton(
                            onPressed: !_focusPending &&
                                    store.destinationControlIsCurrent(focused)
                                ? () {
                                    final captured = focused;
                                    showDialog<void>(
                                        animationStyle:
                                            AnimationStyle.noAnimation,
                                        context: context,
                                        builder: (_) =>
                                            DestinationControlEditor(
                                              accessEpoch: () =>
                                                  store.controlAccessEpoch,
                                              control: captured,
                                              changes: store,
                                              isCurrent: (control) =>
                                                  mounted &&
                                                  _focusTarget ==
                                                      control.target &&
                                                  _focusSurface ==
                                                      control.surface &&
                                                  _focusId == control.id &&
                                                  store.outputProcessingTarget ==
                                                      control.target &&
                                                  store
                                                      .destinationControlIsCurrent(
                                                          control),
                                              apply:
                                                  store.setDestinationControl,
                                            ));
                                  }
                                : null,
                            child: Text(
                                '${focused.label}: ${_focusPending ? 'Sending…' : _focusDraft != null ? '${_focusDraft!.toStringAsFixed(focused.decimals)} ${focused.unit}' : focused.id == 'comp_ratio' && focused.value == 101 ? '∞:1' : focused.valueLabel ?? 'Unavailable'}${!_focusPending && _focusDraft == null && focused.authority == 'accepted' ? ' (unconfirmed)' : ''}',
                                overflow: TextOverflow.ellipsis),
                          ),
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
                        if (value != null) {
                          setState(_clearFocus);
                          store.openOutputProcessing(value);
                        }
                      },
                    ),
                  ),
                  if (target != null)
                    OutputLevelMeter(
                      meter: store.outputMeterFor(target),
                      showCompressorGr: surface == 'output_comp',
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
                                onSelected: (_) => setState(() {
                                  _clearFocus();
                                  _surface = item;
                                }),
                              ),
                            ),
                        ],
                      ),
                    ),
                  if (surface != null && surface != 'output_controls')
                    Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 12),
                        child: OutputProcessorPresetActions(
                            store: store, surface: surface)),
                  if (surface == 'output_geq' &&
                      outputGeqBands(processor).isNotEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 12),
                      child: OutputGeqDraftActions(store: store),
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
                                    onFocus: _focus,
                                    onComplete: _complete,
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
                                              onFocus: _focus,
                                              onComplete: _complete,
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
                                    onFocus: _focus,
                                    onComplete: _complete,
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
  final void Function(DestinationControlVm) onComplete;
  final void Function(DestinationControlVm, double?, bool) onFocus;
  const _ControlTile({
    super.key,
    required this.store,
    required this.control,
    required this.onFocus,
    required this.onComplete,
    this.vertical = false,
  });
  @override
  State<_ControlTile> createState() => _ControlTileState();
}

class _ControlTileState extends State<_ControlTile> {
  DestinationControlVm? _dragControl;
  int? _dragAccessEpoch;
  double? _draft;
  bool _pending = false;

  @override
  void didUpdateWidget(covariant _ControlTile oldWidget) {
    super.didUpdateWidget(oldWidget);
    final captured = _dragControl;
    if (captured != null &&
        !widget.store.destinationControlIsCurrent(captured)) {
      _draft = null;
      // Keep the captured descriptor until release: never substitute a new
      // session/value for a gesture that started against an older one.
    }
  }

  Future<void> _commit(DestinationControlVm control, double value) async {
    if (_dragControl != null &&
        _dragAccessEpoch != widget.store.controlAccessEpoch) {
      setState(() {
        _draft = null;
        _dragControl = null;
      });
      widget.onComplete(control);
      return;
    }
    setState(() {
      _pending = true;
      _draft = null;
      _dragControl = null;
    });
    widget.onFocus(control, null, true);
    final error = await widget.store.setDestinationControl(control, value);
    if (!mounted) return;
    setState(() => _pending = false);
    widget.onComplete(control);
    if (error != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(error)));
    }
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
        ? Semantics(
            label: '${control.destinationName}, ${control.label}',
            child: Slider(
              min: logarithmic ? 0 : control.min!,
              max: logarithmic ? 1 : control.max!,
              value: position(value.clamp(control.min!, control.max!)),
              label:
                  '${value.toStringAsFixed(control.decimals)} ${control.unit}',
              semanticFormatterCallback: (positionValue) =>
                  '${engineering(positionValue).toStringAsFixed(control.decimals)} ${control.unit}'
                  '${control.authority == 'accepted' ? ' (unconfirmed)' : ''}',
              onChangeStart: enabled
                  ? (_) {
                      widget.onFocus(control, value, false);
                      _dragControl = control;
                      _dragAccessEpoch = widget.store.controlAccessEpoch;
                    }
                  : null,
              onChanged: enabled
                  ? (v) {
                      setState(() => _draft = engineering(v));
                      widget.onFocus(control, engineering(v), false);
                    }
                  : null,
              onChangeEnd: enabled
                  ? (v) {
                      final captured = _dragControl;
                      if (captured != null) {
                        _commit(captured, engineering(v));
                      }
                    }
                  : null,
            ),
          )
        : null;
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
              child: TextButton(
                  onPressed: () => widget.onFocus(control, null, _pending),
                  child: Text(control.label, textAlign: TextAlign.center)),
            ),
            Expanded(
              child: RotatedBox(
                quarterTurns: 3,
                child: slider ?? const SizedBox.shrink(),
              ),
            ),
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
                Expanded(
                    child: TextButton(
                        onPressed: () =>
                            widget.onFocus(control, null, _pending),
                        child: Align(
                            alignment: Alignment.centerLeft,
                            child: Text(control.label)))),
              ],
            ),
            if (slider != null) slider,
            if (!control.numeric)
              Wrap(
                spacing: 8,
                children: [
                  for (final choice in control.choices)
                    ChoiceChip(
                      label: Text('${choice.label}'
                          '${choice.value == control.value && control.authority == 'accepted' ? ' (unconfirmed)' : ''}'),
                      selected:
                          control.available && choice.value == control.value,
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
