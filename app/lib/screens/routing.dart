// Patch matrix: patch goes through cmdPatchSet, unpatch through cmdPatchClear.
// Screens stay constructor-injected so Area A can implement the store.
import 'package:flutter/material.dart';

import '../theme.dart';
import '../palette.dart';
import '../core/destination_controls.dart';
import '../widgets/destination_control_editor.dart';

class RouteStrip {
  final int strip;
  final String id;
  final String name;
  final String src;

  const RouteStrip({
    required this.strip,
    required this.id,
    required this.name,
    required this.src,
  });
}

class RouteSrcTile {
  final String label;
  // Numbered sources reserve space for the index independently of the group.
  // Null keeps the complete label for unnumbered sources.
  final String? displayGroupLabel;
  final String group;
  final int index;
  final String usedBy;
  final bool current;
  final String? operationKey;

  const RouteSrcTile({
    required this.label,
    required this.group,
    required this.index,
    this.displayGroupLabel,
    this.usedBy = '',
    this.operationKey,
    this.current = false,
  });
}

class RoutePatchTarget {
  final String key, label;
  final bool clearable;
  const RoutePatchTarget(this.key, this.label, this.clearable);
}

// Implemented by the shell over the engine or a fixture sim.
abstract class RoutingStore extends ChangeNotifier {
  List<RouteStrip> get strips;
  List<RoutePatchTarget> get targets;
  String get target;
  bool get canClear;
  String? get operationKey;
  void selectTarget(String target);
  String groupLabel(String group);
  List<String> get groups;
  String get group;
  int? get selectedStrip;
  String get selectedName;
  String get selectedSrc;
  List<RouteSrcTile> get srcTiles;
  bool get routingSupported;
  bool get canPatch => routingSupported;
  int get controlAccessEpoch => 0;
  DestinationControlVm? get tapControl => null;
  bool tapIsCurrent(DestinationControlVm control) => false;
  Future<String?> setTapControl(
          DestinationControlVm control, double value) async =>
      'Routing tap unavailable.';

  void selectStrip(int strip);
  void selectGroup(String group);
  void patch(RouteSrcTile tile);
  void clearPatch(int strip, {String? operationKey});
}

class U4RoutingPage extends StatelessWidget {
  final RoutingStore store;

  const U4RoutingPage({super.key, required this.store});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: store,
          builder: (context, _) {
            if (!store.routingSupported) {
              return Center(
                child: Text(
                  'Routing controls are unavailable for this driver.',
                  style: TextStyle(color: ConsolePalette.of(context).text4),
                ),
              );
            }
            final operationKey = store.operationKey;
            final selectedStrip = store.selectedStrip;
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (!store.canPatch)
                  const Padding(
                    padding: EdgeInsets.all(8),
                    child: Text(
                        'Select a current destination to edit; device limits apply.'),
                  ),
                _Header(
                  selectedName: store.selectedName,
                  selectedSrc: store.selectedSrc,
                  onClear: store.selectedStrip == null || !store.canClear
                      ? null
                      : () => store.clearPatch(selectedStrip!,
                          operationKey: operationKey),
                ),
                if (store.tapControl case final control?)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: TextButton(
                      onPressed: store.tapIsCurrent(control)
                          ? () => showDialog<void>(
                                context: context,
                                barrierDismissible: false,
                                builder: (_) => DestinationControlEditor(
                                  changes: store,
                                  accessEpoch: () => store.controlAccessEpoch,
                                  isCurrent: store.tapIsCurrent,
                                  apply: store.setTapControl,
                                  control: control,
                                ),
                              )
                          : null,
                      child: Text(
                          'Tap position: ${control.valueLabel ?? 'Unavailable'}'),
                    ),
                  ),
                if (store.targets.length > 1)
                  _GroupChips(
                    groups: [for (final t in store.targets) t.key],
                    labels: {for (final t in store.targets) t.key: t.label},
                    active: store.target,
                    onPick: store.selectTarget,
                  ),
                _GroupChips(
                  labels: {
                    for (final g in store.groups) g: store.groupLabel(g)
                  },
                  groups: store.groups,
                  active: store.group,
                  onPick: store.selectGroup,
                ),
                Expanded(
                  child: store.strips.isEmpty
                      ? Center(
                          child: Text('waiting for desk…',
                              style: TextStyle(
                                  color: ConsolePalette.of(context).text7)),
                        )
                      : ListView.separated(
                          padding: const EdgeInsets.all(8),
                          itemCount: store.strips.length,
                          separatorBuilder: (_, __) =>
                              const SizedBox(height: 4),
                          itemBuilder: (context, i) {
                            final s = store.strips[i];
                            final on = s.strip == store.selectedStrip;
                            return _RoutingTap(
                              selected: on,
                              onTap: () => store.selectStrip(s.strip),
                              child: Container(
                                padding: const EdgeInsets.symmetric(
                                    horizontal: 10, vertical: 8),
                                decoration: BoxDecoration(
                                  color: on
                                      ? ConsolePalette.of(context).selected
                                      : ConsolePalette.of(context).well,
                                  border: Border.all(
                                      color: on
                                          ? ConsolePalette.of(context)
                                              .selectedLine
                                          : ConsolePalette.of(context)
                                              .tileBorder),
                                  borderRadius: U4.rBtn,
                                ),
                                child: Row(
                                  children: [
                                    Expanded(
                                      child: Text(
                                        s.name,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: TextStyle(
                                            fontSize: 13,
                                            fontWeight: FontWeight.w600,
                                            color: ConsolePalette.of(context)
                                                .text1),
                                      ),
                                    ),
                                    Text(
                                      s.src,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: TextStyle(
                                        fontSize: 12,
                                        fontWeight: FontWeight.w700,
                                        color: s.src == '—'
                                            ? ConsolePalette.of(context).mute
                                            : ConsolePalette.of(context).cyan,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            );
                          },
                        ),
                ),
                Divider(color: ConsolePalette.of(context).divider, height: 1),
                Expanded(
                  child: Padding(
                    padding: const EdgeInsets.all(8),
                    child: store.srcTiles.isEmpty
                        ? Text('no sources in this group',
                            style: TextStyle(
                                fontSize: 12,
                                color: ConsolePalette.of(context).text7))
                        : GridView.builder(
                            gridDelegate:
                                const SliverGridDelegateWithFixedCrossAxisCount(
                              crossAxisCount: 4,
                              mainAxisSpacing: 4,
                              crossAxisSpacing: 4,
                              mainAxisExtent: 52,
                            ),
                            itemCount: store.srcTiles.length,
                            itemBuilder: (context, i) {
                              final t = store.srcTiles[i];
                              final free = t.usedBy.isEmpty && !t.current;
                              return _RoutingTap(
                                selected: t.current,
                                onTap: store.selectedStrip == null ||
                                        !store.canPatch
                                    ? null
                                    : () => store.patch(t),
                                child: Container(
                                  alignment: Alignment.center,
                                  padding:
                                      const EdgeInsets.symmetric(horizontal: 4),
                                  decoration: BoxDecoration(
                                    color: t.current
                                        ? ConsolePalette.of(context).cyan
                                        : free
                                            ? ConsolePalette.of(context).well
                                            : ConsolePalette.of(context)
                                                .control,
                                    border: Border.all(
                                        color: t.current
                                            ? ConsolePalette.of(context).cyan
                                            : ConsolePalette.of(context)
                                                .tileBorder),
                                    borderRadius: U4.rBtn,
                                  ),
                                  child: Column(
                                    mainAxisSize: MainAxisSize.min,
                                    children: [
                                      _SourceLabel(tile: t),
                                      if (!t.current && t.usedBy.isNotEmpty)
                                        Text(
                                          '→ ${t.usedBy}',
                                          textAlign: TextAlign.center,
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                              fontSize: 10,
                                              color: ConsolePalette.of(context)
                                                  .text7),
                                        ),
                                    ],
                                  ),
                                ),
                              );
                            },
                          ),
                  ),
                ),
              ],
            );
          },
        ),
      ),
    );
  }
}

class _SourceLabel extends StatelessWidget {
  final RouteSrcTile tile;

  const _SourceLabel({required this.tile});

  @override
  Widget build(BuildContext context) {
    final style = TextStyle(
      fontSize: 12,
      fontWeight: FontWeight.w700,
      color: tile.current
          ? ConsolePalette.of(context).onAccent
          : ConsolePalette.of(context).text1,
    );
    final groupLabel = tile.displayGroupLabel;
    return Semantics(
      label: tile.label,
      child: ExcludeSemantics(
        child: groupLabel == null
            ? Text(
                tile.label,
                textAlign: TextAlign.center,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: style,
              )
            : Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Flexible(
                    child: Text(
                      groupLabel,
                      textAlign: TextAlign.center,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: style,
                    ),
                  ),
                  Text(' ${tile.index}', style: style),
                ],
              ),
      ),
    );
  }
}

class _Header extends StatelessWidget {
  final String selectedName;
  final String selectedSrc;
  final VoidCallback? onClear;

  const _Header({
    required this.selectedName,
    required this.selectedSrc,
    required this.onClear,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        border: Border(
            bottom: BorderSide(color: ConsolePalette.of(context).lineHeader)),
      ),
      child: Row(
        children: [
          Text('ROUTING',
              style: TextStyle(
                  fontSize: 15,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.6,
                  color: ConsolePalette.of(context).text1)),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              selectedName.isEmpty
                  ? 'pick a strip, then a source'
                  : '$selectedName · $selectedSrc',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontSize: 12, color: ConsolePalette.of(context).text7),
            ),
          ),
          if (onClear != null)
            _RoutingTap(
              onTap: onClear,
              child: Container(
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                decoration: BoxDecoration(
                  color: ConsolePalette.of(context).well,
                  border: Border.all(color: ConsolePalette.of(context).lineCtl),
                  borderRadius: U4.rBtn,
                ),
                child: Text('Unpatch',
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
    );
  }
}

class _GroupChips extends StatelessWidget {
  final List<String> groups;
  final Map<String, String> labels;
  final String active;
  final ValueChanged<String> onPick;

  const _GroupChips({
    required this.groups,
    this.labels = const {},
    required this.active,
    required this.onPick,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 8, 8, 0),
      child: Wrap(
        runSpacing: 6,
        children: [
          for (final g in groups) ...[
            _RoutingTap(
              selected: g == active,
              onTap: () => onPick(g),
              child: Container(
                margin: const EdgeInsets.only(right: 6),
                padding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                decoration: BoxDecoration(
                  color: g == active
                      ? ConsolePalette.of(context).cyan
                      : ConsolePalette.of(context).well,
                  borderRadius: U4.rBtn,
                  border: Border.all(
                      color: g == active
                          ? ConsolePalette.of(context).cyan
                          : ConsolePalette.of(context).railBorder),
                ),
                child: Text(
                  labels[g] ?? g,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w700,
                    color: g == active
                        ? ConsolePalette.of(context).onAccent
                        : ConsolePalette.of(context).text4,
                  ),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _RoutingTap extends StatelessWidget {
  final Widget child;
  final VoidCallback? onTap;
  final bool? selected;
  const _RoutingTap({required this.child, required this.onTap, this.selected});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      enabled: onTap != null,
      selected: selected,
      child: GestureDetector(onTap: onTap, child: child));
}
