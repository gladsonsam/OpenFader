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
  bool get hasNativeFile;
  Future<void> importNativeFile();
  Future<void> exportNativeFile();
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
      animationStyle: AnimationStyle.noAnimation,
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
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => AlertDialog(
        title: Text(
          slot == null
              ? 'Capture simulated strips'
              : 'Rename provider snapshot',
        ),
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
        if (slot != null) 'slot': slot['slot'],
      }, captured);
    }
  }

  Future<void> _scope(
    BuildContext context,
    NativeLibraryVm view,
    Map<String, dynamic> slot,
  ) async {
    final captured = store.nativeContext;
    final fields = view.scopeFields!;
    final draft = (slot['scope'] as String).split('');
    final result = await showDialog<String>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setState) {
          return AlertDialog(
            title: Text('Recall scope · ${slot['slot']}'),
            content: SizedBox(
              width: 560,
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Text(
                      'Select which settings are recalled from this snapshot.',
                    ),
                    for (final group in [
                      'Channels and outputs',
                      'FX',
                      'Parameters',
                      'Globals',
                    ]) ...[
                      Padding(
                        padding: const EdgeInsets.only(top: 16),
                        child: Text(
                          group,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                      ),
                      for (final field in fields.where(
                        (f) => f['group'] == group,
                      ))
                        CheckboxListTile(
                          dense: true,
                          contentPadding: EdgeInsets.zero,
                          title: Text(field['label'] as String),
                          value: draft[field['index'] as int] == '+',
                          onChanged: (value) => setState(
                            () => draft[field['index'] as int] = value == true
                                ? '+'
                                : '-',
                          ),
                        ),
                    ],
                  ],
                ),
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('Cancel'),
              ),
              TextButton(
                onPressed: () => Navigator.pop(context, draft.join()),
                child: const Text('Apply'),
              ),
            ],
          );
        },
      ),
    );
    if (result == null || !context.mounted) return;
    if (store.nativeBusy ||
        !store.nativeWritable ||
        store.nativeContext != captured ||
        store.nativeLibrary?.connected != true) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(
          content: Text('Mixer or access changed. Reopen recall scope to edit.'),
        ),
      );
      return;
    }
    await store.nativeAction({
      'op': 'set_scope',
      'slot': slot['slot'],
      'scope': result,
    }, captured);
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
                          view?.guard['provider'] == 'xair'
                              ? 'Read snapshot names and scopes?'
                              : 'Download current show?',
                          view?.guard['provider'] == 'xair'
                              ? 'Read the names and recall scopes of all 64 mixer snapshot slots.'
                              : 'Download and inspect the stored-show ZIP. This does not save a file or certify current unsaved edits or native restoration. The stored snapshot index is refreshed; current edits remain unverified.',
                        )
                      : null,
                  child: const Text('Transfer / refresh reports'),
                ),
              if (view?.supports('export_file') == true)
                TextButton(
                  onPressed: enabled
                      ? () => _confirm(
                          context,
                          {'op': 'export_file'},
                          'Preserve native show ZIP?',
                          'Download, validate and save the original stored-show ZIP on this device. Unknown file bytes are retained. Current unsaved edits and exact restoration are unverified; this is not a full native backup.',
                        )
                      : null,
                  child: const Text('Save native show ZIP'),
                ),
              if (view?.supports('inspect_file') == true)
                TextButton(
                  onPressed: enabled ? store.importNativeFile : null,
                  child: const Text('Import ZIP for inspection'),
                ),
              if (store.hasNativeFile)
                TextButton(
                  onPressed: enabled ? store.exportNativeFile : null,
                  child: const Text('Export preserved ZIP'),
                ),
              if (view?.supports('store') == true)
                TextButton(
                  onPressed:
                      enabled &&
                          store.nativeWritable &&
                          view!.slots.length < view.capacity
                      ? () => _confirm(
                          context,
                          {'op': 'store'},
                          'Save new native snapshot?',
                          'Store current mixer state at the next user position. Native naming is retained; use Rename after save. Current-edit coverage and safes remain unverified. No automatic retry.',
                        )
                      : null,
                  child: const Text('Save native snapshot'),
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
                    ? () => store.nativeAction({
                        'op': 'read_show',
                      }, store.nativeContext)
                    : null,
                child: const Text('Check current show'),
              ),
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
                onPressed:
                    enabled &&
                        view.connected &&
                        store.nativeWritable &&
                        view.supports('recall')
                    ? () => _confirm(
                        context,
                        {'op': 'recall', 'slot': view.guard['default_slot']},
                        'Recall Default 0?',
                        'This resets the mixer to the native default snapshot. '
                            'It may change levels, routing and processing. Safes are not verified here. No automatic retry.',
                      )
                    : null,
                child: const Text('0 · Default — recall only'),
              ),
            if (view.slots.isEmpty)
              Text(
                view.complete ? 'No captured slots.' : 'No names reported. This does not prove the show is empty.',
                style: TextStyle(color: palette.text7),
              ),
            for (final slot in view.slots)
              Wrap(
                crossAxisAlignment: WrapCrossAlignment.center,
                spacing: 8,
                children: [
                  Text(
                    '${slot['slot']} · ${slot['name'] == '' ? '(unnamed)' : slot['name']}'
                    '${view.guard['current'] == slot['slot'] ? ' · last reported recall' : ''}',
                    style: TextStyle(color: palette.text1),
                  ),
                  if (view.supports('set_scope'))
                    if (view.scopeFields != null && view.hasScope(slot))
                      TextButton(
                        onPressed:
                            enabled && view.connected && store.nativeWritable
                            ? () => _scope(context, view, slot)
                            : null,
                        child: const Text('Recall scope'),
                      )
                    else
                      Text(
                        'Recall scope unavailable',
                        style: TextStyle(color: palette.text7),
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
                  if (view.supports('replace'))
                    TextButton(
                      onPressed:
                          enabled && view.connected && store.nativeWritable
                          ? () => _confirm(
                              context,
                              {'op': 'replace', 'slot': slot['slot']},
                              view.authority == 'mixer'
                                  ? 'Update native snapshot?'
                                  : 'Replace simulated capture?',
                              view.guard['provider'] == 'xair'
                                  ? 'Save current mixer settings to this slot, replacing its contents. The mixer’s snapshot scope applies.'
                                  : view.authority == 'mixer'
                                  ? 'Store current mixer state over this user position. The provider verifies stored files by readback. Current-edit coverage and safes remain unverified. No automatic retry.'
                                  : 'Overwrite this memory-only capture with current simulated fader, mute, pan and name values. No native file is created.',
                            )
                          : null,
                      child: Text(
                        view.guard['provider'] == 'xair'
                            ? 'Save to slot'
                            : 'Replace',
                      ),
                    ),
                  if (view.supports('rename'))
                    TextButton(
                      onPressed:
                          enabled && view.connected && store.nativeWritable
                          ? () => _name(context, slot)
                          : null,
                      child: const Text('Rename'),
                    ),
                  if (view.supports('delete'))
                    TextButton(
                      onPressed:
                          enabled && view.connected && store.nativeWritable
                          ? () => _confirm(
                              context,
                              {'op': 'delete', 'slot': slot['slot']},
                              view.authority == 'mixer'
                                  ? 'Delete native snapshot?'
                                  : 'Delete simulated slot?',
                              view.guard['provider'] == 'xair'
                                  ? 'Delete this mixer snapshot. Refresh the slot names before another change.'
                                  : view.authority == 'mixer'
                                  ? 'Delete this stored snapshot permanently. Later native positions compact; refresh before another operation. No automatic retry.'
                                  : 'Delete this memory-only simulated strip capture? It cannot be undone.',
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
