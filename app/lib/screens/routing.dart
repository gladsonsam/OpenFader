// Patch matrix: previews stay local; confirmed edits use the injected store.
// The store retains provider identity, access and stale-operation guards.
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';

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

class U4RoutingPage extends StatefulWidget {
  final RoutingStore store;
  const U4RoutingPage({super.key, required this.store});

  @override
  State<U4RoutingPage> createState() => _U4RoutingPageState();
}

class _U4RoutingPageState extends State<U4RoutingPage> {
  static const double _labelWidth = 168, _cell = 56, _row = 56;
  final _matrixKey = GlobalKey();
  RouteStrip? _previewStrip;
  RouteSrcTile? _previewSource;
  String? _previewTarget;
  PointerDeviceKind _pointer = PointerDeviceKind.touch;
  bool _confirming = false;
  RoutingStore get store => widget.store;

  void _preview(RouteStrip strip, RouteSrcTile source) {
    if (_confirming) return;
    setState(() {
      _previewStrip = strip;
      _previewSource = source;
      _previewTarget = store.target;
    });
  }

  void _drag(Offset global) {
    final box = _matrixKey.currentContext?.findRenderObject() as RenderBox?;
    if (box == null) return;
    final point = box.globalToLocal(global);
    final column = ((point.dx - _labelWidth) / _cell).floor();
    final row = (point.dy / _row).floor();
    final sources = store.srcTiles;
    final strips = store.strips;
    if (point.dx < _labelWidth ||
        row < 0 ||
        row >= strips.length ||
        column < 0 ||
        column >= sources.length) {
      setState(() {
        _previewStrip = null;
        _previewSource = null;
      });
      return;
    }
    _preview(strips[row], sources[column]);
  }

  Future<void> _choose(RouteStrip strip, RouteSrcTile source,
      {bool direct = false}) async {
    if (_confirming) return;
    store.selectStrip(strip.strip);
    final tile = store.srcTiles
        .where((t) => t.group == source.group && t.index == source.index)
        .firstOrNull;
    if (tile == null || !store.canPatch) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
            content: Text('This destination is unavailable or protected.')));
      }
      return;
    }
    _preview(strip, tile);
    if (tile.current) return;
    if (direct) {
      store.patch(tile);
      return;
    }
    _confirming = true;
    final target = store.target;
    final epoch = store.controlAccessEpoch;
    await showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheetContext) => SafeArea(
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
          child: ListenableBuilder(
              listenable: store,
              builder: (context, _) {
                final valid = store.target == target &&
                    store.canPatch &&
                    store.selectedStrip == strip.strip &&
                    store.controlAccessEpoch == epoch &&
                    store.operationKey == tile.operationKey &&
                    store.srcTiles.any((t) =>
                        t.group == tile.group &&
                        t.index == tile.index &&
                        t.operationKey == tile.operationKey);
                return Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text('Route ${strip.name}',
                          style: Theme.of(context).textTheme.titleLarge),
                      const SizedBox(height: 16),
                      Text('Current: ${strip.src}'),
                      const SizedBox(height: 8),
                      Text('New source: ${tile.label}',
                          style: const TextStyle(fontWeight: FontWeight.w700)),
                      const SizedBox(height: 16),
                      if (!valid)
                        const Text(
                            'Routing changed. Close and select the route again.'),
                      FilledButton(
                          onPressed: valid
                              ? () {
                                  store.patch(tile);
                                  Navigator.pop(sheetContext);
                                }
                              : null,
                          child: const Text('Confirm route')),
                      TextButton(
                          onPressed: () => Navigator.pop(sheetContext),
                          child: const Text('Cancel')),
                    ]);
              }),
        ),
      ),
    );
    if (mounted) {
      setState(() {
        _confirming = false;
        _previewStrip = null;
        _previewSource = null;
      });
    }
  }

  Widget _tabs(List<String> keys, String selected,
          String Function(String) label, ValueChanged<String> pick) =>
      SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
        child: Row(children: [
          for (final key in keys)
            Padding(
              padding: const EdgeInsets.only(right: 8),
              child: ChoiceChip(
                  label: Text(label(key)),
                  selected: key == selected,
                  materialTapTargetSize: MaterialTapTargetSize.padded,
                  onSelected: (_) {
                    setState(() {
                      _previewStrip = null;
                      _previewSource = null;
                    });
                    pick(key);
                  }),
            )
        ]),
      );

  @override
  Widget build(BuildContext context) => Scaffold(
        backgroundColor: ConsolePalette.of(context).page,
        body: SafeArea(
            child: ListenableBuilder(
                listenable: store,
                builder: (context, _) {
                  final p = ConsolePalette.of(context);
                  if (!store.routingSupported) {
                    return const Center(
                        child: Text(
                            'Routing controls are unavailable for this driver.'));
                  }
                  final strips = store.strips;
                  final sources = store.srcTiles;
                  final preview = _previewTarget == store.target &&
                      strips.any((s) => s.id == _previewStrip?.id) &&
                      sources.any((s) =>
                          s.group == _previewSource?.group &&
                          s.index == _previewSource?.index);
                  return Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Padding(
                            padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                            child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Patch matrix',
                                      style: Theme.of(context)
                                          .textTheme
                                          .titleLarge),
                                  const SizedBox(height: 4),
                                  Text('Sources across · destinations down',
                                      style: TextStyle(color: p.text4)),
                                  const SizedBox(height: 4),
                                  Text(
                                      'Mouse: hover to preview, click to route. Touch: tap or hold and drag, then confirm.',
                                      style: TextStyle(
                                          fontSize: 12, color: p.text7)),
                                ])),
                        _tabs(
                            [for (final t in store.targets) t.key],
                            store.target,
                            (key) => store.targets
                                .firstWhere((t) => t.key == key)
                                .label,
                            store.selectTarget),
                        _tabs(store.groups, store.group, store.groupLabel,
                            store.selectGroup),
                        Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 16, vertical: 10),
                            color: p.well,
                            child: Text(
                                preview
                                    ? '${_previewStrip!.name}: ${_previewStrip!.src} → ${_previewSource!.label}'
                                    : '✓ Patched   ·   + Preview   ·   Scroll to see more sources',
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                    color: preview ? p.cyan : p.text4,
                                    fontWeight: FontWeight.w600))),
                        Expanded(
                            child: sources.isEmpty || strips.isEmpty
                                ? const Center(
                                    child: Text(
                                        'Waiting for routing sources and destinations…'))
                                : Scrollbar(
                                    child: SingleChildScrollView(
                                    scrollDirection: Axis.horizontal,
                                    child: SizedBox(
                                      width:
                                          _labelWidth + sources.length * _cell,
                                      child: Column(children: [
                                        Row(children: [
                                          SizedBox(
                                              width: _labelWidth,
                                              height: 64,
                                              child: Center(
                                                  child: Text('DESTINATION ↓',
                                                      style: TextStyle(
                                                          color: p.text7,
                                                          fontSize: 11)))),
                                          for (final source in sources)
                                            Tooltip(
                                                message: source.label,
                                                child: Container(
                                                    width: _cell,
                                                    height: 64,
                                                    alignment: Alignment.center,
                                                    padding:
                                                        const EdgeInsets.all(4),
                                                    decoration: BoxDecoration(
                                                        color: p.well,
                                                        border: Border.all(
                                                            color:
                                                                p.tileBorder)),
                                                    child: Text(source.label,
                                                        textAlign:
                                                            TextAlign.center,
                                                        maxLines: 3,
                                                        overflow: TextOverflow
                                                            .ellipsis,
                                                        style: const TextStyle(
                                                            fontSize: 11,
                                                            fontWeight:
                                                                FontWeight
                                                                    .w600)))),
                                        ]),
                                        Expanded(
                                            child: SingleChildScrollView(
                                                child: GestureDetector(
                                          onLongPressStart: (d) =>
                                              _drag(d.globalPosition),
                                          onLongPressMoveUpdate: (d) =>
                                              _drag(d.globalPosition),
                                          onLongPressCancel: () => setState(() {
                                            _previewStrip = null;
                                            _previewSource = null;
                                          }),
                                          onLongPressEnd: (_) {
                                            final strip = _previewStrip;
                                            final source = _previewSource;
                                            if (strip != null &&
                                                source != null &&
                                                _previewTarget ==
                                                    store.target) {
                                              _choose(strip, source);
                                            }
                                          },
                                          child: Column(
                                              key: _matrixKey,
                                              children: [
                                                for (final strip in strips)
                                                  Row(children: [
                                                    SizedBox(
                                                        width: _labelWidth,
                                                        height: _row,
                                                        child: TextButton(
                                                            onPressed: () => store
                                                                .selectStrip(
                                                                    strip
                                                                        .strip),
                                                            child: Align(
                                                                alignment: Alignment
                                                                    .centerLeft,
                                                                child: Column(
                                                                    mainAxisAlignment:
                                                                        MainAxisAlignment
                                                                            .center,
                                                                    crossAxisAlignment:
                                                                        CrossAxisAlignment
                                                                            .start,
                                                                    children: [
                                                                      Text(
                                                                          strip
                                                                              .name,
                                                                          maxLines:
                                                                              1,
                                                                          overflow:
                                                                              TextOverflow.ellipsis),
                                                                      Text(
                                                                          strip
                                                                              .src,
                                                                          maxLines:
                                                                              1,
                                                                          overflow: TextOverflow
                                                                              .ellipsis,
                                                                          style: TextStyle(
                                                                              fontSize: 11,
                                                                              color: p.text7))
                                                                    ])))),
                                                    for (final source
                                                        in sources)
                                                      Builder(
                                                          builder: (context) {
                                                        final connected =
                                                            strip.src ==
                                                                source.label;
                                                        final highlighted =
                                                            preview &&
                                                                _previewStrip
                                                                        ?.id ==
                                                                    strip.id &&
                                                                _previewSource
                                                                        ?.index ==
                                                                    source
                                                                        .index;
                                                        return MouseRegion(
                                                            onEnter: (_) =>
                                                                _preview(strip,
                                                                    source),
                                                            onExit: (_) {
                                                              if (!_confirming) {
                                                                setState(() {
                                                                  _previewStrip =
                                                                      null;
                                                                  _previewSource =
                                                                      null;
                                                                });
                                                              }
                                                            },
                                                            child: Listener(
                                                                onPointerDown:
                                                                    (event) =>
                                                                        _pointer =
                                                                            event.kind,
                                                                child: Semantics(
                                                                    label: '${source.label} to ${strip.name}',
                                                                    selected: connected,
                                                                    child: SizedBox(
                                                                        width: _cell,
                                                                        height: _row,
                                                                        child: Padding(
                                                                            padding: const EdgeInsets.all(3),
                                                                            child: Material(
                                                                                color: connected
                                                                                    ? p.cyan
                                                                                    : highlighted
                                                                                        ? p.selected
                                                                                        : p.well,
                                                                                shape: RoundedRectangleBorder(borderRadius: U4.rBtn, side: BorderSide(color: highlighted ? p.cyan : p.tileBorder, width: highlighted ? 2 : 1)),
                                                                                child: InkWell(
                                                                                    borderRadius: U4.rBtn,
                                                                                    onTap: () => _choose(strip, source, direct: _pointer == PointerDeviceKind.mouse),
                                                                                    child: Center(
                                                                                        child: Icon(
                                                                                            connected
                                                                                                ? Icons.check_rounded
                                                                                                : highlighted
                                                                                                    ? Icons.add
                                                                                                    : Icons.circle_outlined,
                                                                                            size: connected || highlighted ? 22 : 10,
                                                                                            color: connected
                                                                                                ? p.onAccent
                                                                                                : highlighted
                                                                                                    ? p.cyan
                                                                                                    : p.text7)))))))));
                                                      }),
                                                  ]),
                                              ]),
                                        ))),
                                      ]),
                                    ),
                                  ))),
                        if (store.selectedStrip != null)
                          Padding(
                              padding: const EdgeInsets.symmetric(
                                  horizontal: 12, vertical: 4),
                              child: Wrap(
                                  spacing: 12,
                                  crossAxisAlignment: WrapCrossAlignment.center,
                                  children: [
                                    Text(store.selectedName,
                                        style: const TextStyle(
                                            fontWeight: FontWeight.w600)),
                                    if (store.tapControl case final control?)
                                      TextButton(
                                          onPressed: store.tapIsCurrent(control)
                                              ? () => showDialog<void>(
                                                  context: context,
                                                  barrierDismissible: false,
                                                  builder: (_) =>
                                                      DestinationControlEditor(
                                                          changes: store,
                                                          accessEpoch: () => store
                                                              .controlAccessEpoch,
                                                          isCurrent: store
                                                              .tapIsCurrent,
                                                          apply: store
                                                              .setTapControl,
                                                          control: control))
                                              : null,
                                          child: Text(
                                              'Tap: ${control.valueLabel ?? 'Unavailable'}')),
                                    TextButton(
                                        onPressed: store.canClear
                                            ? () => _confirmClear()
                                            : null,
                                        child: const Text('Unpatch')),
                                  ])),
                      ]);
                })),
      );

  Future<void> _confirmClear() async {
    final strip = store.selectedStrip;
    final key = store.operationKey;
    final name = store.selectedName;
    final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
                title: Text('Unpatch $name?'),
                content: const Text('This removes its current source.'),
                actions: [
                  TextButton(
                      onPressed: () => Navigator.pop(context, false),
                      child: const Text('Cancel')),
                  FilledButton(
                      onPressed: () => Navigator.pop(context, true),
                      child: const Text('Unpatch'))
                ]));
    if (confirmed == true && mounted && strip != null) {
      store.clearPatch(strip, operationKey: key);
    }
  }
}
