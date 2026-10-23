// Area D: scenes screen. Numbered snapshot library (save/recall/rename/
// delete/clear via cmd scene_*) plus named show capture/recall/delete
// (cmd capture_scene/recall_scene/delete_scene, list via list_shows).
//
// Wiring: everything flows through [ScenesStore] (constructor injection —
// Area A implements it over the engine; this file only renders ui4 tokens,
// see lib/theme.dart). No Material Card/AppBar/Switch/Slider; plain text
// buttons only.
import 'package:flutter/material.dart';

import '../theme.dart';
import '../palette.dart';
import '../widgets/console_button.dart';

/// One numbered snapshot (engine `scenes` event entry).
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

/// Minimal store contract the scenes screen renders. Implemented by the
/// shell/owner over the live engine or a fixture sim.
abstract class ScenesStore extends ChangeNotifier {
  List<SceneEntryVm> get entries;
  List<String> get shows;
  List<String> get showScenes;
  bool get busy;
  String? get status;
  String? get show; // active show for capture/recall, null = numbered only
  bool get online;

  void save(String name); // scene_save into first free slot
  void recall(int number); // scene_recall
  void rename(int number, String name); // scene_rename
  void remove(int number); // scene_delete
  void clearAll(); // scene_clear
  void selectShow(String show);
  void beginShow();
  void captureShow(String name); // capture_scene(show, name)
  void recallShow(String name); // recall_scene(show, name)
  void deleteShow(String name); // delete_scene(show, name)
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
  final _renameCtl = TextEditingController();

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
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text('SCENES',
                      style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.6,
                          color: ConsolePalette.of(context).text1)),
                  if (!store.online)
                    Padding(
                      padding: EdgeInsets.only(top: 6),
                      child: Text(
                        'Offline — snapshots need a live desk.',
                        style: TextStyle(
                            fontSize: 12,
                            color: ConsolePalette.of(context).text7),
                      ),
                    ),
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
                  _NameRow(
                    controller: _showCtl,
                    hint: store.show == null
                        ? 'New show name…'
                        : 'Scene name in ${store.show}…',
                    actionLabel: store.busy ? 'Working…' : 'Capture',
                    onAction: () {
                      final v = _showCtl.text.trim();
                      if (v.isNotEmpty && !store.busy && store.online) {
                        FocusScope.of(context).unfocus();
                        store.captureShow(v);
                        _showCtl.clear();
                      }
                    },
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
                                : () => store.recallShow(scene),
                            child: const Text('LOAD')),
                        TextButton(
                            onPressed: store.busy
                                ? null
                                : () => store.deleteShow(scene),
                            child: const Text('DELETE')),
                      ]),
                  ],
                  const SizedBox(height: 14),
                  Row(
                    children: [
                      const Expanded(child: _SectionTitle('Snapshots')),
                      if (store.entries.isNotEmpty)
                        GestureDetector(
                          onTap: store.clearAll,
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
                  _NameRow(
                    controller: _saveCtl,
                    hint: 'Snapshot name…',
                    actionLabel: store.busy ? 'Working…' : 'Save',
                    onAction: () {
                      final v = _saveCtl.text.trim();
                      if (v.isNotEmpty && !store.busy && store.online) {
                        FocusScope.of(context).unfocus();
                        store.save(v);
                        _saveCtl.clear();
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
                        mainAxisExtent: 140,
                        mainAxisSpacing: 6,
                        crossAxisSpacing: 6,
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
                                        onSubmitted: (v) {
                                          store.rename(e.number, v.trim());
                                          setState(() => _renaming = null);
                                        },
                                      ),
                                    ),
                                    GestureDetector(
                                      onTap: () {
                                        store.rename(
                                            e.number, _renameCtl.text.trim());
                                        setState(() => _renaming = null);
                                      },
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
                                            child: Text('Current',
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
                                        onTap: () => store.recall(e.number),
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
                                    Row(
                                      children: [
                                        GestureDetector(
                                          onTap: () => store.recall(e.number),
                                          child: Text('RECALL',
                                              style: TextStyle(
                                                  fontSize: 11,
                                                  fontWeight: FontWeight.w700,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .meterGreen)),
                                        ),
                                        const SizedBox(width: 12),
                                        GestureDetector(
                                          onTap: () {
                                            _renameCtl.text = e.name;
                                            setState(
                                                () => _renaming = e.number);
                                          },
                                          child: Text('RENAME',
                                              style: TextStyle(
                                                  fontSize: 11,
                                                  color:
                                                      ConsolePalette.of(context)
                                                          .text7)),
                                        ),
                                        const SizedBox(width: 12),
                                        GestureDetector(
                                          onTap: () => store.remove(e.number),
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
  final VoidCallback onAction;

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
              onSubmitted: (_) => onAction(),
            ),
          ),
        ),
        const SizedBox(width: 6),
        ConsoleButton(label: actionLabel, primary: true, onPressed: onAction),
      ],
    );
  }
}
