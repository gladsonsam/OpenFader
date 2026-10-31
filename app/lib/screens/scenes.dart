// Snapshot library plus named show capture and recall.
// Local transactions and guarded provider operations keep separate authorities.
// Screens stay constructor-injected so Area A can implement the store.
import 'package:flutter/material.dart';

import '../engine.dart' show NativeSnapshotsVm, NativeSnapshotsStatus;
import '../theme.dart';
import '../core/native_library.dart';
import '../widgets/native_library.dart';
import '../palette.dart';
import '../widgets/console_button.dart';

class SceneEntryVm {
  final int number;
  final String name;
  final bool current;

  const SceneEntryVm({
    required this.number,
    required this.name,
    this.current = false,
  });
}

// Implemented by the shell over the engine or a fixture sim.
abstract class ScenesStore extends ChangeNotifier
    implements NativeLibraryStore {
  @override
  NativeLibraryVm? get nativeLibrary => null;
  @override
  String? get nativeStatus => null;
  @override
  bool get nativeBusy => false;
  @override
  bool get nativeWritable => false;
  @override
  Object get nativeContext => actionContext;
  @override
  Future<void> nativeAction(
      Map<String, dynamic> action, Object captured) async {}
  @override
  void cancelNativeAction() {}
  List<SceneEntryVm> get entries;
  List<String> get shows;
  List<String> get showScenes;
  bool get busy;
  String? get status;
  String get libraryDescription =>
      'Local library durability is not yet proven. '
      'It is not a native mixer backup.';
  String? get show; // active show for capture/recall, null = numbered only
  bool get online;

  /// Optional passive surface. Stores without native support remain honest.
  NativeSnapshotsVm get nativeSnapshots =>
      const NativeSnapshotsVm.unsupported();

  // Validation rejects stale targets without sending.
  Object get actionContext;
  bool validateActionContext(Object captured);
  bool captureWillOverwrite(String name);

  void save(String name);
  void recall(int number);
  void rename(int number, String name);
  void remove(int number);
  void clearAll();
  void selectShow(String show);
  void beginShow();
  void captureShow(String name);
  void recallShow(String name);
  void deleteShow(String name);
}

class U4ScenesPage extends StatefulWidget {
  final ScenesStore store;

  const U4ScenesPage({super.key, required this.store});

  @override
  State<U4ScenesPage> createState() => _U4ScenesPageState();
}

class _U4ScenesPageState extends State<U4ScenesPage> {
  final _saveCtl = TextEditingController();
  final _showCtl = TextEditingController();
  int? _renaming;
  Object? _renamingContext;
  final _renameCtl = TextEditingController();

  Future<void> _confirm({
    required String title,
    required String body,
    required String label,
    required VoidCallback action,
  }) async {
    final store = widget.store;
    if (store.busy || !store.online) return;
    final captured = store.actionContext;
    final ok = await showDialog<bool>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: ConsolePalette.of(context).chip,
        scrollable: true,
        title: Text(title,
            style: TextStyle(color: ConsolePalette.of(context).text1)),
        content: Text(body,
            style: TextStyle(color: ConsolePalette.of(context).text7)),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel')),
          TextButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text(label)),
        ],
      ),
    );
    if (!mounted || !identical(store, widget.store) || ok != true) return;
    if (store.validateActionContext(captured)) action();
  }

  Future<void> _captureShow() async {
    final store = widget.store;
    final name = _showCtl.text.trim();
    if (name.isEmpty || store.busy || !store.online) return;
    FocusScope.of(context).unfocus();
    final selected = store.show ?? name;
    void capture() {
      store.captureShow(name);
      if (_showCtl.text.trim() == name) _showCtl.clear();
    }

    if (store.captureWillOverwrite(name)) {
      await _confirm(
          title: 'Replace scene?',
          body: 'Replace “$name” in OpenFader show “$selected” with a new '
              'capture? The previous library contents cannot be recovered.',
          label: 'Replace',
          action: capture);
    } else {
      capture();
    }
  }

  void _recallSnapshot(SceneEntryVm entry) {
    final store = widget.store;
    _confirm(
        title: 'Recall snapshot?',
        body: 'Request recall of OpenFader snapshot '
            '${entry.number.toString().padLeft(3, '0')} “${entry.name}”? '
            'This may change mixer settings. Mixer value verification '
            'is unavailable here.',
        label: 'Recall',
        action: () => store.recall(entry.number));
  }

  void _submitRename() {
    final store = widget.store;
    final number = _renaming;
    final captured = _renamingContext;
    if (number != null &&
        captured != null &&
        store.validateActionContext(captured)) {
      store.rename(number, _renameCtl.text.trim());
    }
    setState(() {
      _renaming = null;
      _renamingContext = null;
    });
  }

  @override
  void dispose() {
    _saveCtl.dispose();
    _showCtl.dispose();
    _renameCtl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            final store = widget.store;
            return SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(16, 16, 16, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  NativeLibrarySection(store: store),
                  if (store.nativeSnapshots.driver != 'xair') ...[
                    const SizedBox(height: 10),
                    _NativeSnapshotsSection(view: store.nativeSnapshots),
                  ],
                  const SizedBox(height: 16),
                  const _SectionTitle('OpenFader local library'),
                  if (store.status != null)
                    Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: Text(store.status!,
                            style: TextStyle(
                                color: ConsolePalette.of(context).text4))),
                  if (store.busy)
                    const Padding(
                        padding: EdgeInsets.only(top: 6),
                        child: LinearProgressIndicator(minHeight: 2)),
                  const SizedBox(height: 10),
                  Row(children: [
                    const Expanded(child: _SectionTitle('Shows')),
                    if (store.shows.isNotEmpty)
                      TextButton(
                          onPressed: store.busy ? null : store.beginShow,
                          child: const Text('New show')),
                  ]),
                  const SizedBox(height: 8),
                  _NameRow(
                    controller: _showCtl,
                    hint: store.show == null
                        ? 'New show name…'
                        : 'Scene name in ${store.show}…',
                    actionLabel: store.busy ? 'Working…' : 'Capture',
                    onAction: store.busy || !store.online ? null : _captureShow,
                  ),
                  const SizedBox(height: 6),
                  if (store.shows.isEmpty)
                    Text('no shows yet',
                        style: TextStyle(
                            fontSize: 12,
                            color: ConsolePalette.of(context).text7))
                  else ...[
                    Wrap(spacing: 6, runSpacing: 6, children: [
                      for (final show in store.shows)
                        OutlinedButton(
                            onPressed: store.busy
                                ? null
                                : () => store.selectShow(show),
                            style: OutlinedButton.styleFrom(
                                foregroundColor: show == store.show
                                    ? ConsolePalette.of(context).cyan
                                    : ConsolePalette.of(context).text1),
                            child: Text(show)),
                    ]),
                    for (final scene in store.showScenes)
                      Row(children: [
                        Expanded(
                            child: Text(scene,
                                maxLines: 1, overflow: TextOverflow.ellipsis)),
                        TextButton(
                            onPressed: store.busy
                                ? null
                                : () => _confirm(
                                    title: 'Recall scene?',
                                    body: 'Request recall of “$scene” in '
                                        'OpenFader show “${store.show}”? This '
                                        'may change mixer settings. Completion '
                                        'and mixer value verification are '
                                        'unavailable here.',
                                    label: 'Recall',
                                    action: () => store.recallShow(scene)),
                            child: const Text('Recall')),
                        const SizedBox(width: 8),
                        TextButton(
                            onPressed: store.busy
                                ? null
                                : () => _confirm(
                                    title: 'Delete scene?',
                                    body: 'Delete “$scene” from OpenFader show '
                                        '“${store.show}”? This cannot be undone.',
                                    label: 'Delete',
                                    action: () => store.deleteShow(scene)),
                            child: const Text('Delete')),
                      ]),
                  ],
                  const SizedBox(height: 14),
                  Row(
                    children: [
                      const Expanded(child: _SectionTitle('Snapshots')),
                      if (store.entries.isNotEmpty)
                        GestureDetector(
                          onTap: store.busy
                              ? null
                              : () => _confirm(
                                  title: 'Clear numbered snapshots?',
                                  body: 'Delete all ${store.entries.length} '
                                      'numbered snapshots from the OpenFader '
                                      'engine library? This cannot be undone. '
                                      'Named shows are separate.\n\n'
                                      '${store.entries.map((e) => '${e.number.toString().padLeft(3, '0')} “${e.name}”').join('\n')}',
                                  label: 'Clear all',
                                  action: store.clearAll),
                          child: Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 12, vertical: 8),
                            decoration: BoxDecoration(
                              color: ConsolePalette.of(context).well,
                              border: Border.all(
                                  color: ConsolePalette.of(context).lineCtl),
                              borderRadius: U4.rBtn,
                            ),
                            child: Text('Clear all',
                                textAlign: TextAlign.center,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w700,
                                    color: ConsolePalette.of(context).text1)),
                          ),
                        ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  _NameRow(
                    controller: _saveCtl,
                    hint: 'Snapshot name…',
                    actionLabel: store.busy ? 'Working…' : 'Save',
                    onAction: store.busy || !store.online
                        ? null
                        : () {
                            final v = _saveCtl.text.trim();
                            if (v.isNotEmpty && !store.busy && store.online) {
                              FocusScope.of(context).unfocus();
                              store.save(v);
                              if (store.busy) _saveCtl.clear();
                            }
                          },
                  ),
                  const SizedBox(height: 6),
                  if (store.entries.isEmpty)
                    Text('no snapshots saved',
                        style: TextStyle(
                            fontSize: 12,
                            color: ConsolePalette.of(context).text7))
                  else
                    GridView.builder(
                      shrinkWrap: true,
                      physics: const NeverScrollableScrollPhysics(),
                      gridDelegate:
                          const SliverGridDelegateWithMaxCrossAxisExtent(
                        maxCrossAxisExtent: 520,
                        mainAxisExtent: 112,
                        mainAxisSpacing: 10,
                        crossAxisSpacing: 10,
                      ),
                      itemCount: store.entries.length,
                      itemBuilder: (context, i) {
                        final e = store.entries[i];
                        final renaming = _renaming == e.number;
                        return Container(
                          padding: const EdgeInsets.symmetric(
                              horizontal: 10, vertical: 8),
                          decoration: BoxDecoration(
                            color: e.current
                                ? ConsolePalette.of(context).selected
                                : ConsolePalette.of(context).panel,
                            border: Border.all(
                                color: e.current
                                    ? ConsolePalette.of(context).meterGreen
                                    : ConsolePalette.of(context).meterAmber,
                                width: e.current ? 2 : 1),
                            borderRadius: U4.rBtn,
                          ),
                          child: renaming
                              ? Row(
                                  children: [
                                    Expanded(
                                      child: TextField(
                                        controller: _renameCtl,
                                        autofocus: true,
                                        style: TextStyle(
                                            fontSize: 13,
                                            color: ConsolePalette.of(context)
                                                .text1),
                                        decoration: InputDecoration(
                                          isDense: true,
                                          border: InputBorder.none,
                                          enabledBorder: InputBorder.none,
                                          disabledBorder: InputBorder.none,
                                          focusedBorder: UnderlineInputBorder(
                                              borderSide: BorderSide(
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .cyan)),
                                          filled: false,
                                          contentPadding: EdgeInsets.symmetric(
                                              vertical: 12),
                                          hintText: 'name…',
                                          hintStyle: TextStyle(
                                              color: ConsolePalette.of(context)
                                                  .text10),
                                        ),
                                        onSubmitted: (_) => _submitRename(),
                                      ),
                                    ),
                                    GestureDetector(
                                      onTap: _submitRename,
                                      child: Text('✓',
                                          style: TextStyle(
                                              fontSize: 16,
                                              color: ConsolePalette.of(context)
                                                  .meterGreen)),
                                    ),
                                  ],
                                )
                              : Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  mainAxisAlignment: MainAxisAlignment.center,
                                  children: [
                                    Row(
                                      children: [
                                        Text(
                                          e.number.toString().padLeft(3, '0'),
                                          style: TextStyle(
                                              fontSize: 11,
                                              fontWeight: FontWeight.w700,
                                              color: ConsolePalette.of(context)
                                                  .text7),
                                        ),
                                        if (e.current)
                                          Padding(
                                            padding: EdgeInsets.only(left: 6),
                                            child: Text('Library current',
                                                style: TextStyle(
                                                    fontSize: 10,
                                                    fontWeight: FontWeight.w700,
                                                    color: ConsolePalette.of(
                                                            context)
                                                        .meterGreen)),
                                          ),
                                      ],
                                    ),
                                    Expanded(
                                      child: GestureDetector(
                                        onTap: store.busy
                                            ? null
                                            : () => _recallSnapshot(e),
                                        child: Text(
                                          e.name,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                              fontSize: 14,
                                              fontWeight: FontWeight.w600,
                                              color: ConsolePalette.of(context)
                                                  .text1),
                                        ),
                                      ),
                                    ),
                                    Wrap(
                                      spacing: 8,
                                      runSpacing: 4,
                                      children: [
                                        TextButton(
                                          onPressed: store.busy
                                              ? null
                                              : () => _recallSnapshot(e),
                                          child: Text('RECALL',
                                              style: TextStyle(
                                                  fontSize: 11,
                                                  fontWeight: FontWeight.w700,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .meterGreen)),
                                        ),
                                        TextButton(
                                          onPressed: store.busy
                                              ? null
                                              : () {
                                                  _renameCtl.text = e.name;
                                                  _renamingContext =
                                                      store.actionContext;
                                                  setState(() =>
                                                      _renaming = e.number);
                                                },
                                          child: Text('RENAME',
                                              style: TextStyle(
                                                  fontSize: 11,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .text7)),
                                        ),
                                        TextButton(
                                          onPressed: store.busy
                                              ? null
                                              : () => _confirm(
                                                  title: 'Delete snapshot?',
                                                  body: 'Delete OpenFader '
                                                      'snapshot '
                                                      '${e.number.toString().padLeft(3, '0')} '
                                                      '“${e.name}”? This '
                                                      'cannot be undone.',
                                                  label: 'Delete',
                                                  action: () =>
                                                      store.remove(e.number)),
                                          child: Text('DELETE',
                                              style: TextStyle(
                                                  fontSize: 11,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .mute)),
                                        ),
                                      ],
                                    ),
                                  ],
                                ),
                        );
                      },
                    ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _NativeSnapshotsSection extends StatelessWidget {
  final NativeSnapshotsVm view;

  const _NativeSnapshotsSection({required this.view});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final message = switch (view.status) {
      NativeSnapshotsStatus.unknown => 'Snapshot support not reported',
      NativeSnapshotsStatus.unsupported => view.driver == 'sim'
          ? 'Unavailable in simulator'
          : 'Snapshot reports unsupported',
      NativeSnapshotsStatus.unavailable => 'Unavailable while disconnected',
      NativeSnapshotsStatus.loading => 'Waiting for snapshot reports',
      NativeSnapshotsStatus.ready => 'Observed slots',
    };
    String? reportedName;
    for (final slot in view.slots) {
      if (slot.slot == view.current && slot.name.trim().isNotEmpty) {
        reportedName = slot.name;
      }
    }
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
          color: palette.well,
          border: Border.all(color: palette.railBorder),
          borderRadius: U4.rBtn),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const _SectionTitle('Passive mixer snapshot reports'),
          Text(message, style: TextStyle(fontSize: 12, color: palette.text7)),
          if (view.status == NativeSnapshotsStatus.ready) ...[
            const SizedBox(height: 8),
            Text(
                view.current == null
                    ? 'No native recall slot reported.'
                    : 'Last reported slot ${view.current}'
                        '${reportedName == null ? '' : ' · $reportedName'}',
                style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    color: palette.text1)),
            const SizedBox(height: 8),
            if (view.slots.isEmpty)
              Text('No slot names reported yet.',
                  style: TextStyle(fontSize: 12, color: palette.text7)),
            for (final slot in view.slots)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 6),
                child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('Slot ${slot.slot}',
                          style: TextStyle(fontSize: 12, color: palette.text4)),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(
                            slot.name.trim().isEmpty
                                ? 'Name not reported'
                                : slot.name,
                            style:
                                TextStyle(fontSize: 13, color: palette.text1)),
                      ),
                    ]),
              ),
          ],
        ],
      ),
    );
  }
}

class _SectionTitle extends StatelessWidget {
  final String text;
  const _SectionTitle(this.text);

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Text(text,
          style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w700,
              color: ConsolePalette.of(context).text4)),
    );
  }
}

class _NameRow extends StatelessWidget {
  final TextEditingController controller;
  final String hint;
  final String actionLabel;
  final VoidCallback? onAction;

  const _NameRow({
    required this.controller,
    required this.hint,
    required this.actionLabel,
    required this.onAction,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 10),
            decoration: BoxDecoration(
              color: ConsolePalette.of(context).well,
              border: Border.all(color: ConsolePalette.of(context).railBorder),
              borderRadius: U4.rBtn,
            ),
            child: TextField(
              controller: controller,
              style: TextStyle(
                  fontSize: 13, color: ConsolePalette.of(context).text1),
              decoration: InputDecoration(
                isDense: true,
                border: InputBorder.none,
                enabledBorder: InputBorder.none,
                disabledBorder: InputBorder.none,
                focusedBorder: UnderlineInputBorder(
                    borderSide:
                        BorderSide(color: ConsolePalette.of(context).cyan)),
                filled: false,
                contentPadding: EdgeInsets.symmetric(vertical: 12),
                hintText: hint,
                hintStyle: TextStyle(color: ConsolePalette.of(context).text10),
              ),
              onSubmitted: onAction == null ? null : (_) => onAction?.call(),
            ),
          ),
        ),
        const SizedBox(width: 6),
        ConsoleButton(label: actionLabel, primary: true, onPressed: onAction),
      ],
    );
  }
}
