import 'package:flutter/material.dart';

import '../core/quick_access.dart';
import '../core/destination_controls.dart';
import '../palette.dart';

/// This panel never owns navigation, a poll loop, or a command queue.
class QuickAccessPanel extends StatefulWidget {
  final QuickAccessStore store;
  const QuickAccessPanel({super.key, required this.store});

  @override
  State<QuickAccessPanel> createState() => _QuickAccessPanelState();
}

class _QuickAccessPanelState extends State<QuickAccessPanel> {
  late final int _session = widget.store.session;
  final Set<String> _pending = {};
  String? _error;

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
                    'Provider-supported FX mute and per-slot tempo. '
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
                  if (store.clearSoloLabel case final label?) ...[
                    const Divider(),
                    const Text(
                      'Provider coverage: input solos only. '
                      'This action has no independent completion confirmation.',
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
                  if (groups.isEmpty && store.clearSoloLabel == null)
                    const Padding(
                      padding: EdgeInsets.symmetric(vertical: 16),
                      child: Text(
                        'No supported Quick Access actions are advertised by this provider.',
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
