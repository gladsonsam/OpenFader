import 'package:flutter/material.dart';

import '../core/native_library.dart';
import '../palette.dart';

abstract interface class NativeLibraryStore {
  NativeLibraryVm? get nativeLibrary;
  String? get nativeStatus;
  bool get nativeBusy;
  bool get nativeWritable;
  Object get nativeContext;
  Future<void> nativeAction(Map<String, dynamic> action, Object captured);
  void cancelNativeAction();
}

class NativeLibrarySection extends StatelessWidget {
  final NativeLibraryStore store;
  const NativeLibrarySection({super.key, required this.store});

  Future<void> _confirm(
    BuildContext context,
    Map<String, dynamic> action,
    String title,
    String description,
  ) async {
    final captured = store.nativeContext;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        content: Text(description),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Continue'),
          ),
        ],
      ),
    );
    if (ok == true && context.mounted) {
      await store.nativeAction(action, captured);
    }
  }

  Future<void> _name(BuildContext context, [Map<String, dynamic>? slot]) async {
    final captured = store.nativeContext;
    var enteredName = slot?['name'] as String? ?? '';
    final name = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(slot == null
            ? 'Capture simulated strips'
            : 'Rename provider snapshot'),
        content: TextFormField(
          initialValue: enteredName,
          onChanged: (value) => enteredName = value,
          autofocus: true,
          decoration: const InputDecoration(
            labelText: 'Name (up to 63 UTF-8 bytes)',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, enteredName),
            child: Text(slot == null ? 'Capture' : 'Rename'),
          ),
        ],
      ),
    );
    if (name != null && name.trim().isNotEmpty && context.mounted) {
      await store.nativeAction({
        'op': slot == null ? 'capture' : 'rename',
        'name': name,
        if (slot != null) 'slot': slot['slot']
      }, captured);
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final view = store.nativeLibrary;
    final enabled = !store.nativeBusy;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: palette.well,
        border: Border.all(color: palette.railBorder),
        borderRadius: BorderRadius.circular(8),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            view?.label ?? 'Provider show / snapshot library',
            style: TextStyle(color: palette.text1, fontWeight: FontWeight.w700),
          ),
          Text(
            'Separate from the OpenFader local library.',
            style: TextStyle(color: palette.text7),
          ),
          Wrap(
            spacing: 8,
            children: [
              TextButton(
                onPressed: enabled
                    ? () => store.nativeAction({
                          'op': 'inspect',
                        }, store.nativeContext)
                    : null,
                child: const Text('Inspect provider'),
              ),
              if (view?.supports('refresh') == true)
                TextButton(
                  onPressed: enabled
                      ? () => _confirm(
                            context,
                            {'op': 'refresh'},
                            'Download current show?',
                            'This explicitly transfers Show.zip from the mixer. It is not passive polling and does not save a restorable backup. The reported names remain a partial inventory.',
                          )
                      : null,
                  child: const Text('Transfer / refresh reports'),
                ),
              if (view?.supports('capture') == true)
                TextButton(
                  onPressed: enabled && store.nativeWritable
                      ? () => _name(context)
                      : null,
                  child: const Text('Capture simulated strips'),
                ),
              if (store.nativeBusy)
                TextButton(
                  onPressed: store.cancelNativeAction,
                  child: const Text('Cancel waiting'),
                ),
            ],
          ),
          if (store.nativeBusy) const LinearProgressIndicator(minHeight: 2),
          if (store.nativeStatus != null)
            Text(store.nativeStatus!, style: TextStyle(color: palette.text4)),
          if (view != null) ...[
            if (view.supports('read_show'))
              TextButton(
                  onPressed: enabled
                      ? () => store.nativeAction(
                          {'op': 'read_show'}, store.nativeContext)
                      : null,
                  child: const Text('Check current show')),
            Text(
              '${view.authority} · ${view.complete ? 'complete provider index' : 'partial observed index'} · '
              '${view.capacity} user slots',
              style: TextStyle(color: palette.text7),
            ),
            Text(
              view.limitations,
              style: TextStyle(color: palette.text7, fontSize: 12),
            ),
            if (view.guard['default_slot'] != null)
              TextButton(
                  onPressed: enabled &&
                          view.connected &&
                          store.nativeWritable &&
                          view.supports('recall')
                      ? () => _confirm(
                          context,
                          {'op': 'recall', 'slot': view.guard['default_slot']},
                          'Recall Default 0?',
                          'This resets the mixer to the native default snapshot. '
                              'It may change levels, routing and processing. Safes are not verified here. No automatic retry.')
                      : null,
                  child: const Text('0 · Default — recall only')),
            if (view.slots.isEmpty)
              Text(
                view.complete
                    ? 'No captured slots.'
                    : 'No names reported. This does not prove the show is empty.',
                style: TextStyle(color: palette.text7),
              ),
            for (final slot in view.slots)
              Row(
                children: [
                  Expanded(
                    child: Text(
                      '${slot['slot']} · ${slot['name']}'
                      '${view.guard['current'] == slot['slot'] ? ' · last reported recall' : ''}',
                      style: TextStyle(color: palette.text1),
                    ),
                  ),
                  if (view.supports('recall'))
                    TextButton(
                      onPressed:
                          enabled && view.connected && store.nativeWritable
                              ? () => _confirm(
                                    context,
                                    {'op': 'recall', 'slot': slot['slot']},
                                    'Recall provider snapshot?',
                                    'Recall ${slot['slot']} “${slot['name']}” in ${view.label}? This may change mixer settings. '
                                        'Safe behavior and audio state are not verified here. No automatic retry.',
                                  )
                              : null,
                      child: const Text('Recall'),
                    ),
                  if (view.supports('rename'))
                    TextButton(
                        onPressed:
                            enabled && view.connected && store.nativeWritable
                                ? () => _name(context, slot)
                                : null,
                        child: const Text('Rename')),
                  if (view.supports('delete'))
                    TextButton(
                      onPressed:
                          enabled && view.connected && store.nativeWritable
                              ? () => _confirm(
                                    context,
                                    {'op': 'delete', 'slot': slot['slot']},
                                    'Delete simulated slot?',
                                    'Delete this memory-only simulated strip capture? It cannot be undone.',
                                  )
                              : null,
                      child: const Text('Delete'),
                    ),
                ],
              ),
          ],
        ],
      ),
    );
  }
}
