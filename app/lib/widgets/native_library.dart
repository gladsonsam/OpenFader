import 'inline_feedback.dart';
import 'dart:convert';

import '../core/native_library_store.dart';
import '../ui/console.dart';
import 'package:material_ui/material_ui.dart';

import '../core/native_library.dart';
import '../core/native_library_policy.dart';

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
    final ok = await showConsoleConfirmation(context,
        title: title, message: description, confirmLabel: 'Continue');
    if (ok == true && context.mounted) {
      await store.nativeAction(action, captured);
    }
  }

  Future<void> _name(BuildContext context, [Map<String, dynamic>? slot]) async {
    final captured = store.nativeContext;
    final name = await showConsoleNameDialog(context,
        title: slot == null
            ? 'Capture simulated strips'
            : 'Rename provider snapshot',
        initialValue: slot?['name'] as String? ?? '',
        fieldLabel: 'Name (up to 63 UTF-8 bytes)',
        confirmLabel: slot == null ? 'Capture' : 'Rename');
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
          return ConsoleAlertDialog(
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
                    for (final group in fields
                        .map((field) => field['group'] as String)
                        .toSet()) ...[
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
                          minTileHeight: AppDesign.touchTarget,
                          contentPadding: EdgeInsets.zero,
                          title: Text(field['label'] as String),
                          value: draft[field['index'] as int] == '+',
                          onChanged: (value) => setState(
                            () => draft[field['index'] as int] =
                                value == true ? '+' : '-',
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
      InlineFeedbackScope.show(
          context, 'Mixer or access changed. Reopen recall scope to edit.');
      return;
    }
    await store.nativeAction({
      'op': 'set_scope',
      'slot': slot['slot'],
      'scope': result,
    }, captured);
  }

  /// Slot + name prompt for a provider-described bank. Bounds come from the
  /// descriptor; nothing is assumed about any particular desk.
  Future<Map<String, dynamic>?> _slotName(
    BuildContext context, {
    required String title,
    required int min,
    required int max,
    String initialName = '',
  }) async {
    final formKey = GlobalKey<FormState>();
    final slotCtl = TextEditingController();
    final nameCtl = TextEditingController(text: initialName);
    try {
      final result = await showDialog<Map<String, dynamic>>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (context) => ConsoleAlertDialog(
          title: Text(title),
          content: SizedBox(
            width: 400,
            child: Form(
              key: formKey,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextFormField(
                    controller: slotCtl,
                    validator: (text) {
                      final slot = int.tryParse(text?.trim() ?? '');
                      return slot == null || slot < min || slot > max
                          ? 'Choose a slot from $min to $max.'
                          : null;
                    },
                    keyboardType: TextInputType.number,
                    decoration: InputDecoration(
                      labelText: 'Slot ($min-$max)',
                      hintText: '$min',
                    ),
                  ),
                  const SizedBox(height: AppDesign.spaceSm),
                  TextFormField(
                    controller: nameCtl,
                    validator: (text) {
                      final name = text ?? '';
                      if (name.isEmpty ||
                          name.trim() != name ||
                          utf8.encode(name).length > 63 ||
                          RegExp(r'[\x00-\x1f\x7f-\x9f]').hasMatch(name)) {
                        return 'Use 1–63 UTF-8 bytes, without control characters or surrounding spaces.';
                      }
                      return null;
                    },
                    decoration: const InputDecoration(
                      labelText: 'Name (1-63 bytes, no surrounding space)',
                    ),
                  ),
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
              onPressed: () {
                if (formKey.currentState?.validate() != true) return;
                Navigator.pop(context, {
                  'slot': int.parse(slotCtl.text.trim()),
                  'name': nameCtl.text,
                });
              },
              child: const Text('Continue'),
            ),
          ],
        ),
      );
      return result;
    } finally {
      slotCtl.dispose();
      nameCtl.dispose();
    }
  }

  /// Name-only prompt for provider-described rename rows.
  Future<String?> _newName(BuildContext context, String initial) async {
    final ctl = TextEditingController(text: initial);
    try {
      return await showDialog<String>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (context) => ConsoleAlertDialog(
          title: const Text('Rename desk slot'),
          content: TextField(
            controller: ctl,
            decoration: const InputDecoration(
              labelText: 'Name (1-63 bytes, no surrounding space)',
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(context, ctl.text),
              child: const Text('Rename'),
            ),
          ],
        ),
      );
    } finally {
      ctl.dispose();
    }
  }

  /// Paste a previously exported local archive document for inspection or
  /// import. Anything that is not a JSON object is rejected before dispatch;
  /// schema/kind/node validation happens provider-side with no mixer write
  /// for inspection.
  Future<Map<String, dynamic>?> _archivePaste(
      BuildContext context, String title) async {
    final ctl = TextEditingController();
    try {
      final text = await showDialog<String>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (context) => ConsoleAlertDialog(
          title: Text(title),
          content: SizedBox(
            width: 480,
            child: TextField(
              controller: ctl,
              maxLines: 8,
              decoration: const InputDecoration(
                labelText: 'Paste exported archive JSON',
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
            TextButton(
              onPressed: () => Navigator.pop(context, ctl.text),
              child: const Text('Continue'),
            ),
          ],
        ),
      );
      if (text == null || text.trim().isEmpty) return null;
      final decoded = jsonDecode(text);
      if (decoded is Map<String, dynamic>) return decoded;
      if (decoded is Map) return Map<String, dynamic>.from(decoded);
      return null;
    } catch (_) {
      return null;
    } finally {
      ctl.dispose();
    }
  }

  /// Integer-argument prompt driven by a provider-described copy form.
  Future<Map<String, int>?> _copyArgs(
      BuildContext context, NativeCopyForm form) async {
    final ctls = {
      for (final field in form.fields) field.key: TextEditingController()
    };
    try {
      return await showDialog<Map<String, int>>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (context) => ConsoleAlertDialog(
          title: Text(form.title),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final field in form.fields) ...[
                TextField(
                  controller: ctls[field.key],
                  keyboardType: TextInputType.number,
                  decoration: InputDecoration(
                    labelText: '${field.label} (${field.min}-${field.max})',
                  ),
                ),
                const SizedBox(height: AppDesign.spaceSm),
              ],
            ],
          ),
          actions: [
            TextButton(
                onPressed: () => Navigator.pop(context),
                child: const Text('Cancel')),
            TextButton(
              onPressed: () {
                final args = <String, int>{};
                for (final field in form.fields) {
                  final value =
                      int.tryParse((ctls[field.key]!.text).trim()) ?? -1;
                  if (value < field.min || value > field.max) {
                    Navigator.pop(context);
                    return;
                  }
                  args[field.key] = value;
                }
                Navigator.pop(context, args);
              },
              child: Text(form.submitLabel),
            ),
          ],
        ),
      );
    } finally {
      for (final ctl in ctls.values) {
        ctl.dispose();
      }
    }
  }

  @override
  Widget build(BuildContext context) => InlineFeedbackScope(builder: _build);

  Widget _build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final view = store.nativeLibrary;
    final policy = nativeLibraryPolicy(view?.guard['provider']);
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
          const SizedBox(height: 4),
          Text(
            'Separate from the OpenFader local library.',
            style: TextStyle(color: palette.text7),
          ),
          const SizedBox(height: 8),
          ConsoleActionGroup(
            spacing: 8,
            runSpacing: 8,
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
                            view?.refreshTitle ?? policy.refreshTitle,
                            view?.refreshDescription ??
                                policy.refreshDescription,
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
                  onPressed: enabled &&
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
            for (final header in view.headers)
              Text(header, style: TextStyle(color: palette.text7)),
            Text(
              view.limitations,
              style: TextStyle(color: palette.text7, fontSize: 12),
            ),
            if (view.banks.isEmpty) ...[
              if (view.guard['default_slot'] != null)
                TextButton(
                  onPressed: enabled &&
                          view.connected &&
                          store.nativeWritable &&
                          view.supports('recall')
                      ? () => _confirm(
                            context,
                            {
                              'op': 'recall',
                              'slot': view.guard['default_slot']
                            },
                            'Recall Default 0?',
                            'This resets the mixer to the native default snapshot. '
                                'It may change levels, routing and processing. Safes are not verified here. No automatic retry.',
                          )
                      : null,
                  child: const Text('0 · Default - recall only'),
                ),
              if (view.slots.isEmpty)
                Text(
                  view.complete
                      ? 'No captured slots.'
                      : 'No names reported. This does not prove the show is empty.',
                  style: TextStyle(color: palette.text7),
                ),
              for (final slot in view.slots)
                Padding(
                    padding: const EdgeInsets.only(bottom: AppDesign.spaceSm),
                    child: ConsoleActionGroup(
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
                              onPressed: enabled &&
                                      view.connected &&
                                      store.nativeWritable
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
                            onPressed: enabled &&
                                    view.connected &&
                                    store.nativeWritable
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
                            onPressed: enabled &&
                                    view.connected &&
                                    store.nativeWritable
                                ? () => _confirm(
                                      context,
                                      {'op': 'replace', 'slot': slot['slot']},
                                      view.authority == 'mixer'
                                          ? 'Update native snapshot?'
                                          : 'Replace simulated capture?',
                                      policy.replaceDescription(
                                          view.authority == 'mixer'),
                                    )
                                : null,
                            child: Text(policy.replaceLabel),
                          ),
                        if (view.supports('rename'))
                          TextButton(
                            onPressed: enabled &&
                                    view.connected &&
                                    store.nativeWritable
                                ? () => _name(context, slot)
                                : null,
                            child: const Text('Rename'),
                          ),
                        if (view.supports('delete'))
                          TextButton(
                            onPressed: enabled &&
                                    view.connected &&
                                    store.nativeWritable
                                ? () => _confirm(
                                      context,
                                      {'op': 'delete', 'slot': slot['slot']},
                                      view.authority == 'mixer'
                                          ? 'Delete native snapshot?'
                                          : 'Delete simulated slot?',
                                      policy.deleteDescription(
                                          view.authority == 'mixer'),
                                    )
                                : null,
                            child: const Text('Delete'),
                          ),
                      ],
                    )),
            ],
            if (view.banks.isNotEmpty)
              _DescriptorBanks(
                view: view,
                enabled: enabled,
                onAction: (action) =>
                    store.nativeAction(action, store.nativeContext),
                onConfirm: (action, title, message) =>
                    _confirm(context, action, title, message),
                onNewName: (initial) => _newName(context, initial),
                onSlotName: (title, min, max) =>
                    _slotName(context, title: title, min: min, max: max),
                onCopy: (form) => _copyArgs(context, form),
                onArchivePaste: (title) => _archivePaste(context, title),
              ),
          ],
        ],
      ),
    );
  }
}

/// Provider-described desk banks: slot ranges, row labels, operation ids and
/// argument contracts all come from the descriptor. Slot numbers are shown
/// verbatim and never remapped to OpenFader local scenes; archive transfers
/// are local JSON copies, never desk USB images.
class _DescriptorBanks extends StatelessWidget {
  final NativeLibraryVm view;
  final bool enabled;
  final Future<void> Function(Map<String, dynamic> action) onAction;
  final Future<void> Function(
      Map<String, dynamic> action, String title, String message) onConfirm;
  final Future<String?> Function(String initial) onNewName;
  final Future<Map<String, dynamic>?> Function(String title, int min, int max)
      onSlotName;
  final Future<Map<String, int>?> Function(NativeCopyForm form) onCopy;
  final Future<Map<String, dynamic>?> Function(String title) onArchivePaste;

  const _DescriptorBanks({
    required this.view,
    required this.enabled,
    required this.onAction,
    required this.onConfirm,
    required this.onNewName,
    required this.onSlotName,
    required this.onCopy,
    required this.onArchivePaste,
  });

  String _fill(String template,
      {Object? slot, Object? name, String? bank, Object? number}) {
    return template
        .replaceAll('{slot}', '${slot ?? ''}')
        .replaceAll('{name}', '${name ?? ''}')
        .replaceAll('{bank}', bank ?? '')
        .replaceAll('{number}', '${number ?? slot ?? ''}');
  }

  Future<void> _save(BuildContext context, NativeBankDescriptor bank,
      NativeBankAction save) async {
    final input = await onSlotName(
        save.promptTitle ?? 'Save ${bank.title}', bank.slotMin, bank.slotMax);
    if (input == null || !context.mounted) return;
    await onAction({
      'op': save.op,
      if (bank.bank != null) 'bank': bank.bank,
      'slot': input['slot'],
      'name': input['name'],
    });
  }

  Future<void> _rename(BuildContext context, NativeBankDescriptor bank,
      NativeBankAction rename, Map<String, dynamic> row) async {
    final key = bank.rowKey == 'position' ? 'position' : 'slot';
    final name = await onNewName((row['name'] ?? '') as String);
    if (name == null || !context.mounted) return;
    await onAction({
      'op': rename.op,
      if (bank.bank != null) 'bank': bank.bank,
      key: row[bank.rowKey],
      'name': name,
    });
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    bool writable = enabled && view.connected && view.complete;
    Future<void> act(Map<String, dynamic> a) => onAction(a);
    TextStyle head =
        TextStyle(color: palette.text1, fontWeight: FontWeight.w700);
    TextStyle sub = TextStyle(color: palette.text7, fontSize: 12);
    Widget bankTitle(String t, String? extra) => Padding(
          padding: const EdgeInsets.only(top: 10),
          child: Text(extra == null ? t : '$t · $extra', style: head),
        );
    Widget bankRow(String label, List<Widget> buttons) => Padding(
          padding: const EdgeInsets.only(bottom: AppDesign.spaceSm),
          child: ConsoleActionGroup(children: [
            Text(label, style: TextStyle(color: palette.text1)),
            ...buttons,
          ]),
        );
    TextButton btn(String label, VoidCallback? onPressed) => TextButton(
          onPressed: onPressed,
          child: Text(label),
        );
    VoidCallback? guard(VoidCallback fn) => writable ? fn : null;

    String rowLabel(NativeBankDescriptor bank, Map<String, dynamic> row) {
      final name = (row['name'] as String).isEmpty ? '(unnamed)' : row['name'];
      final prefix = bank.bank == null ? '' : '${bank.bank} ';
      if (bank.rowKey == 'position') {
        return '$prefix${row['position']} · №${row['number']} · $name';
      }
      return '$prefix${row['slot']} · $name';
    }

    Object rowKeyOf(NativeBankDescriptor bank, Map<String, dynamic> row) =>
        row[bank.rowKey] as Object;

    List<Widget> rowButtons(
        NativeBankDescriptor bank, Map<String, dynamic> row) {
      final key = bank.rowKey == 'position' ? 'position' : 'slot';
      final id = rowKeyOf(bank, row);
      final name = row['name'];
      final buttons = <Widget>[];
      final recall = bank.recall;
      if (recall != null && view.supports(recall.op)) {
        buttons.add(btn(
            'Recall',
            guard(() => onConfirm(
                  {
                    'op': recall.op,
                    if (bank.bank != null) 'bank': bank.bank,
                    key: id,
                  },
                  recall.confirmTitle == null
                      ? 'Recall?'
                      : _fill(recall.confirmTitle!,
                          slot: id, name: name, bank: bank.bank),
                  recall.confirmMessage == null
                      ? 'Apply this change on the mixer?'
                      : _fill(recall.confirmMessage!,
                          slot: id, name: name, bank: bank.bank),
                ))));
      }
      final rename = bank.rename;
      if (rename != null && view.supports(rename.op)) {
        buttons.add(
            btn('Rename', guard(() => _rename(context, bank, rename, row))));
      }
      final delete = bank.delete;
      if (delete != null && view.supports(delete.op)) {
        buttons.add(btn(
            'Delete',
            guard(() => onConfirm(
                  {
                    'op': delete.op,
                    if (bank.bank != null) 'bank': bank.bank,
                    key: id,
                  },
                  delete.confirmTitle == null
                      ? 'Delete?'
                      : _fill(delete.confirmTitle!,
                          slot: id, name: name, bank: bank.bank),
                  delete.confirmMessage == null
                      ? 'Apply this change on the mixer?'
                      : _fill(delete.confirmMessage!,
                          slot: id, name: name, bank: bank.bank),
                ))));
      }
      return buttons;
    }

    final archive = view.archive;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (view.banksNote != null) Text(view.banksNote!, style: sub),
        for (final bank in view.banks) ...[
          bankTitle(bank.title, '${bank.rows.length} observed'),
          if ((bank.save != null && view.supports(bank.save!.op)) ||
              (bank.add != null && view.supports(bank.add!.op)))
            Padding(
              padding: const EdgeInsets.symmetric(vertical: AppDesign.spaceSm),
              child: ConsoleActionGroup(children: [
                if (bank.save != null && view.supports(bank.save!.op))
                  btn('Save', guard(() => _save(context, bank, bank.save!))),
                if (bank.add != null && view.supports(bank.add!.op))
                  btn('Add', guard(() async {
                    final add = bank.add!;
                    final input = await onSlotName(
                        add.promptTitle ?? 'Add ${bank.title}',
                        bank.slotMin,
                        bank.slotMax);
                    if (input == null || !context.mounted) return;
                    await act({
                      'op': add.op,
                      if (bank.bank != null) 'bank': bank.bank,
                      add.numberArg ?? 'number': input['slot'],
                      'name': input['name'],
                    });
                  })),
              ]),
            ),
          for (final row in bank.rows) ...[
            if (rowButtons(bank, row).isEmpty)
              Text(rowLabel(bank, row), style: sub)
            else
              bankRow(rowLabel(bank, row), rowButtons(bank, row)),
          ],
        ],
        if (view.copyForms.any((form) => view.supports(form.op)))
          ConsoleActionGroup(children: [
            for (final form in view.copyForms)
              if (view.supports(form.op))
                btn(form.title, guard(() async {
                  final args = await onCopy(form);
                  if (args == null || !context.mounted) return;
                  await act({'op': form.op, ...args});
                })),
          ]),
        if (archive != null) ...[
          bankTitle('Local archives (JSON)', null),
          ConsoleActionGroup(spacing: 8, children: [
            for (final kind in archive.kinds)
              if (kind.slotMin != null && kind.slotMax != null)
                btn('Export ${kind.label}', guard(() async {
                  final input = await onSlotName(
                      'Export ${kind.label}', kind.slotMin!, kind.slotMax!);
                  if (input == null || !context.mounted) return;
                  await act({
                    'op': archive.exportOp,
                    'kind': kind.id,
                    'slot': input['slot'],
                  });
                }))
              else
                btn(
                    'Export ${kind.label}',
                    guard(
                        () => act({'op': archive.exportOp, 'kind': kind.id}))),
            if (view.supports(archive.inspectOp))
              btn('Inspect archive JSON', guard(() async {
                final doc = await onArchivePaste(
                    'Inspect local archive (no mixer write)');
                if (doc == null || !context.mounted) return;
                await act({'op': archive.inspectOp, 'archive': doc});
              })),
            if (view.supports(archive.importOp))
              btn('Import archive JSON', guard(() async {
                final doc = await onArchivePaste(
                    'Import local archive (writes mixer nodes)');
                if (doc == null || !context.mounted) return;
                await onConfirm({'op': archive.importOp, 'archive': doc},
                    archive.importConfirmTitle, archive.importConfirmMessage);
              })),
          ]),
          Text(
            'Local archives are OpenFader JSON copies, not desk USB files.',
            style: sub,
          ),
        ],
      ],
    );
  }
}
