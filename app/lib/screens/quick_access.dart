import 'package:flutter/material.dart';

import '../core/quick_access.dart';
import '../core/destination_controls.dart';
import '../palette.dart';
import '../widgets/destination_control_editor.dart';
import '../widgets/talkback_button.dart';

/// This panel never owns navigation, a poll loop, or a command queue.
class QuickAccessPanel extends StatefulWidget {
  final QuickAccessStore store;
  const QuickAccessPanel({super.key, required this.store});

  @override
  State<QuickAccessPanel> createState() => _QuickAccessPanelState();
}

class _QuickAccessPanelState extends State<QuickAccessPanel>
    with WidgetsBindingObserver {
  late final int _session = widget.store.session;
  final Set<String> _pending = {};
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) widget.store.cancelTalkback();
  }

  @override
  void didUpdateWidget(covariant QuickAccessPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.store != widget.store) oldWidget.store.cancelTalkback();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    widget.store.cancelTalkback();
    super.dispose();
  }

  Future<void> _run(DestinationControlVm control, {required bool tap}) async {
    final key = '${control.target}/${control.id}';
    if (_session != widget.store.session || !_pending.add(key)) return;
    setState(() => _error = null);
    String? error;
    try {
      error = tap
          ? await widget.store.tapControl(control)
          : await widget.store.setControl(
              control,
              control.value == control.choices.first.value
                  ? control.choices.last.value
                  : control.choices.first.value,
            );
    } catch (failure) {
      error = 'Could not send this setting: $failure. Its result is uncertain.';
    } finally {
      if (mounted) {
        setState(() {
          _pending.remove(key);
          _error = error;
        });
      }
    }
  }

  Future<void> _edit(DestinationControlVm control) {
    widget.store.cancelTalkback();
    return showDialog<void>(
      context: context,
      builder: (_) => DestinationControlEditor(
        changes: widget.store,
        isCurrent: (control) =>
            _session == widget.store.session && widget.store.isCurrent(control),
        apply: widget.store.setControl,
        control: control,
      ),
    );
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final store = widget.store;
          final palette = ConsolePalette.of(context);
          final currentSession = store.session == _session;
          final groups = <String, List<DestinationControlVm>>{};
          for (final control in store.fxControls) {
            groups.putIfAbsent(control.target, () => []).add(control);
          }
          final globals = <String, List<DestinationControlVm>>{};
          for (final control in store.globalControls) {
            globals.putIfAbsent(control.target, () => []).add(control);
          }
          return Material(
            color: palette.panel,
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          'Quick Access',
                          style: TextStyle(
                            color: palette.text1,
                            fontSize: 20,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Close Quick Access',
                        onPressed: () => Navigator.of(context).pop(),
                        icon: const Icon(Icons.close),
                      ),
                    ],
                  ),
                  const Text(
                    'FX mute and tempo controls. '
                    'FX returns remain independent.',
                  ),
                  if (!currentSession)
                    const Padding(
                      padding: EdgeInsets.only(top: 12),
                      child: Text(
                        'Connection changed. Close and reopen Quick Access.',
                      ),
                    ),
                  for (final controls in groups.values) ...[
                    const SizedBox(height: 16),
                    Text(
                      controls.first.destinationName,
                      style: TextStyle(
                        color: palette.text1,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    for (final control in controls)
                      ListTile(
                        contentPadding: EdgeInsets.zero,
                        title: Text(control.label),
                        subtitle: Text(
                          '${control.valueLabel ?? 'Unavailable'}'
                          '${control.authority == 'accepted' ? ' · Unconfirmed' : ''}\n'
                          '${control.description}',
                        ),
                        trailing: TextButton(
                          onPressed: currentSession &&
                                  store.isCurrent(control) &&
                                  !_pending.contains(
                                    '${control.target}/${control.id}',
                                  )
                              ? () => _run(control, tap: control.canTapTempo)
                              : null,
                          child: Text(
                            control.canTapTempo
                                ? 'Tap tempo'
                                : 'Set ${control.value == control.choices.first.value ? control.choices.last.label : control.choices.first.label}',
                          ),
                        ),
                      ),
                  ],
                  for (final controls in globals.values) ...[
                    const Divider(),
                    Text(
                      controls.first.destinationName,
                      style: TextStyle(
                        color: palette.text1,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                    for (final control in controls)
                      ListTile(
                        contentPadding: EdgeInsets.zero,
                        title: Text(control.label),
                        subtitle: Text(
                          '${control.valueLabel ?? 'Unavailable'}'
                          '${control.authority == 'accepted' ? ' · Unconfirmed' : ''}',
                        ),
                        trailing: TextButton(
                          onPressed: currentSession && store.isCurrent(control)
                              ? () => _edit(control)
                              : null,
                          child: const Text('Edit'),
                        ),
                      ),
                  ],
                  if (store.talkbackGestureControl != null) ...[
                    DropdownButtonFormField<TalkbackModePreference>(
                      key: ValueKey(store.talkbackModePreference),
                      initialValue: store.talkbackModePreference,
                      decoration: const InputDecoration(
                        labelText: 'Talkback mode preference (local)',
                      ),
                      items: const [
                        DropdownMenuItem(
                          value: TalkbackModePreference.momentary,
                          child: Text('Momentary'),
                        ),
                        DropdownMenuItem(
                          value: TalkbackModePreference.latch,
                          child: Text('Latch'),
                        ),
                        DropdownMenuItem(
                          value: TalkbackModePreference.auto,
                          child: Text('Auto'),
                        ),
                      ],
                      onChanged: (mode) {
                        if (mode != null) store.setTalkbackModePreference(mode);
                      },
                    ),
                    const Text('Momentary: hold to talk. Latch: tap to toggle. '
                        'Auto: tap to latch, hold 500 ms or longer for momentary. '
                        '500 ms is the OpenFader threshold; native timing is undocumented. '
                        'Release/cancel preserves pre-existing or changed peer activations.'),
                    TalkbackButton(
                        store: store, currentSession: currentSession),
                    if (store.talkbackGestureError case final error?)
                      Text(error, style: TextStyle(color: palette.meterAmber)),
                  ],
                  if (store.clearSoloLabel case final label?) ...[
                    const Divider(),
                    const Text(
                      'Clears input solos. '
                      'Output solos stay unchanged.',
                    ),
                    Align(
                      alignment: Alignment.centerLeft,
                      child: TextButton(
                        onPressed: currentSession && store.canClearSolo
                            ? () => store.clearSolo(_session)
                            : null,
                        child: Text(label),
                      ),
                    ),
                  ],
                  if (groups.isEmpty &&
                      globals.isEmpty &&
                      store.clearSoloLabel == null)
                    const Padding(
                      padding: EdgeInsets.symmetric(vertical: 16),
                      child: Text(
                        'No Quick Access controls are available for this mixer.',
                      ),
                    ),
                  if (_error != null)
                    Text(_error!, style: TextStyle(color: palette.meterAmber)),
                ],
              ),
            ),
          );
        },
      );
}
