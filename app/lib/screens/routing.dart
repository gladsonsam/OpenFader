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

  Widget _selector(List<String> keys, String selected,
      String Function(String) label, ValueChanged<String> pick) {
    if (keys.isEmpty) return const SizedBox.shrink();
    final p = ConsolePalette.of(context);
    return DropdownButtonFormField<String>(
      initialValue: keys.contains(selected) ? selected : keys.first,
      key: ValueKey('$selected:${keys.join(',')}'),
      isExpanded: true,
      decoration: InputDecoration(
          isDense: true,
          filled: true,
          fillColor: p.well,
          contentPadding:
              const EdgeInsets.symmetric(horizontal: 12, vertical: 14),
          border: OutlineInputBorder(
              borderRadius: U4.rBtn,
              borderSide: BorderSide(color: p.tileBorder))),
      items: [
        for (final key in keys)
          DropdownMenuItem(
              value: key,
              child: Text(label(key),
                  maxLines: 1, overflow: TextOverflow.ellipsis))
      ],
      onChanged: (value) {
        if (value == null) return;
        setState(() {
          _previewStrip = null;
          _previewSource = null;
        });
        pick(value);
      },
    );
  }

  Widget _targetSelector() => _selector(
      [for (final t in store.targets) t.key],
      store.target,
      (key) => store.targets.firstWhere((t) => t.key == key).label,
      store.selectTarget);

  Widget _mobileRoutes() {
    final p = ConsolePalette.of(context);
    return Column(children: [
      Padding(padding: const EdgeInsets.all(12), child: _targetSelector()),
      Expanded(
          child: store.strips.isEmpty
              ? const Center(child: Text('Waiting for destinations…'))
              : ListView.separated(
                  padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
                  itemCount: store.strips.length,
                  separatorBuilder: (_, __) =>
                      Divider(height: 1, color: p.tileBorder),
                  itemBuilder: (context, index) {
                    final strip = store.strips[index];
                    return ListTile(
                      minTileHeight: 72,
                      contentPadding: const EdgeInsets.symmetric(horizontal: 8),
                      leading: SizedBox(
                          width: 28,
                          child: Center(
                              child: Text('${index + 1}',
                                  style: TextStyle(
                                      color: p.text7, fontSize: 12)))),
                      title: Text(strip.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                              fontSize: 16, fontWeight: FontWeight.w600)),
                      subtitle: Padding(
                          padding: const EdgeInsets.only(top: 4),
                          child: Text(strip.src,
                              style: TextStyle(color: p.cyan, fontSize: 13))),
                      trailing: Icon(Icons.chevron_right, color: p.text7),
                      onTap: () => _pickSource(strip),
                    );
                  })),
    ]);
  }

  Future<void> _pickSource(RouteStrip strip) async {
    store.selectStrip(strip.strip);
    final target = store.target;
    final operation = store.operationKey;
    final gridKey = GlobalKey();
    RouteSrcTile? preview;
    final source = await showModalBottomSheet<RouteSrcTile>(
      context: context,
      isScrollControlled: true,
      showDragHandle: true,
      builder: (sheetContext) => FractionallySizedBox(
          heightFactor: .82,
          child: SafeArea(child: StatefulBuilder(builder: (context, update) {
            return ListenableBuilder(
                listenable: store,
                builder: (context, _) {
                  final p = ConsolePalette.of(context);
                  final tiles = store.srcTiles;
                  final valid = store.target == target &&
                      store.selectedStrip == strip.strip &&
                      store.operationKey == operation &&
                      store.canPatch;
                  void drag(Offset global) {
                    final box = gridKey.currentContext?.findRenderObject()
                        as RenderBox?;
                    if (box == null) return;
                    final point = box.globalToLocal(global);
                    final col = (point.dx / (box.size.width / 4)).floor();
                    final row = (point.dy / 72).floor();
                    final i = row * 4 + col;
                    update(() => preview = point.dx >= 0 &&
                            point.dx < box.size.width &&
                            point.dy >= 0 &&
                            col >= 0 &&
                            col < 4 &&
                            i >= 0 &&
                            i < tiles.length
                        ? tiles[i]
                        : null);
                  }

                  return Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Padding(
                            padding: const EdgeInsets.fromLTRB(20, 0, 12, 12),
                            child: Row(children: [
                              Expanded(
                                  child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                    Text(strip.name,
                                        style: Theme.of(context)
                                            .textTheme
                                            .titleLarge),
                                    const SizedBox(height: 4),
                                    Text('Current · ${strip.src}',
                                        style: TextStyle(color: p.text7)),
                                  ])),
                              IconButton(
                                  tooltip: 'Close',
                                  icon: const Icon(Icons.close),
                                  onPressed: () => Navigator.pop(sheetContext))
                            ])),
                        Padding(
                            padding: const EdgeInsets.symmetric(horizontal: 16),
                            child: _selector(
                                store.groups, store.group, store.groupLabel,
                                (group) {
                              update(() => preview = null);
                              store.selectGroup(group);
                            })),
                        if (store.canClear || store.tapControl != null)
                          Padding(
                              padding:
                                  const EdgeInsets.symmetric(horizontal: 12),
                              child: Wrap(spacing: 8, children: [
                                if (store.canClear)
                                  TextButton(
                                      onPressed: valid ? _confirmClear : null,
                                      child: const Text('Unpatch')),
                                if (store.tapControl case final control?)
                                  TextButton(
                                      onPressed: valid &&
                                              store.tapIsCurrent(control)
                                          ? () => showDialog<void>(
                                              context: context,
                                              barrierDismissible: false,
                                              builder: (_) =>
                                                  DestinationControlEditor(
                                                      changes: store,
                                                      accessEpoch: () => store
                                                          .controlAccessEpoch,
                                                      isCurrent:
                                                          store.tapIsCurrent,
                                                      apply:
                                                          store.setTapControl,
                                                      control: control))
                                          : null,
                                      child: Text(
                                          'Tap: ${control.valueLabel ?? 'Unavailable'}')),
                              ])),
                        const SizedBox(height: 12),
                        if (!valid)
                          const Padding(
                              padding: EdgeInsets.all(16),
                              child: Text(
                                  'This destination is unavailable or routing has changed.')),
                        Expanded(
                            child: SingleChildScrollView(
                                padding:
                                    const EdgeInsets.symmetric(horizontal: 12),
                                child: GestureDetector(
                                  onLongPressStart: valid
                                      ? (d) => drag(d.globalPosition)
                                      : null,
                                  onLongPressMoveUpdate: valid
                                      ? (d) => drag(d.globalPosition)
                                      : null,
                                  onLongPressCancel: () =>
                                      update(() => preview = null),
                                  onLongPressEnd: valid
                                      ? (_) {
                                          if (preview != null &&
                                              !preview!.current) {
                                            Navigator.pop(
                                                sheetContext, preview);
                                          }
                                        }
                                      : null,
                                  child: GridView.builder(
                                      key: gridKey,
                                      shrinkWrap: true,
                                      physics:
                                          const NeverScrollableScrollPhysics(),
                                      gridDelegate:
                                          const SliverGridDelegateWithFixedCrossAxisCount(
                                              crossAxisCount: 4,
                                              mainAxisExtent: 72),
                                      itemCount: tiles.length,
                                      itemBuilder: (context, index) {
                                        final tile = tiles[index];
                                        final active =
                                            preview?.group == tile.group &&
                                                preview?.index == tile.index;
                                        return Padding(
                                            padding: const EdgeInsets.all(4),
                                            child: Material(
                                                color: tile.current
                                                    ? p.cyan
                                                    : active
                                                        ? p.selected
                                                        : p.well,
                                                shape: RoundedRectangleBorder(
                                                    borderRadius: U4.rBtn,
                                                    side: BorderSide(
                                                        color: active
                                                            ? p.cyan
                                                            : p.tileBorder,
                                                        width: active ? 2 : 1)),
                                                child: InkWell(
                                                    borderRadius: U4.rBtn,
                                                    onTap: valid &&
                                                            !tile.current
                                                        ? () => Navigator.pop(
                                                            sheetContext, tile)
                                                        : null,
                                                    child: Semantics(
                                                        label: tile.label,
                                                        button: true,
                                                        enabled: valid &&
                                                            !tile.current,
                                                        selected: tile.current,
                                                        child: Center(
                                                            child: Row(
                                                                mainAxisSize:
                                                                    MainAxisSize
                                                                        .min,
                                                                children: [
                                                              if (tile.current)
                                                                Padding(
                                                                    padding: const EdgeInsets
                                                                        .only(
                                                                        right:
                                                                            4),
                                                                    child: Icon(
                                                                        Icons
                                                                            .check,
                                                                        size:
                                                                            16,
                                                                        color: p
                                                                            .onAccent)),
                                                              Flexible(
                                                                  child: Text(
                                                                      tile.displayGroupLabel == null
                                                                          ? tile
                                                                              .label
                                                                          : '${tile.index}',
                                                                      textAlign:
                                                                          TextAlign
                                                                              .center,
                                                                      maxLines:
                                                                          2,
                                                                      style: TextStyle(
                                                                          fontSize: tile.displayGroupLabel == null
                                                                              ? 12
                                                                              : 20,
                                                                          fontWeight: FontWeight
                                                                              .w600,
                                                                          color: tile.current
                                                                              ? p.onAccent
                                                                              : p.text1))),
                                                            ]))))));
                                      }),
                                ))),
                        Padding(
                            padding: const EdgeInsets.all(16),
                            child: Text(
                                preview == null
                                    ? 'Choose a source to preview the change'
                                    : '${preview!.label} → ${strip.name}',
                                textAlign: TextAlign.center,
                                style: TextStyle(
                                    color:
                                        preview == null ? p.text7 : p.cyan))),
                      ]);
                });
          }))),
    );
    if (!mounted || source == null) return;
    if (store.target != target ||
        store.operationKey != operation ||
        !store.canPatch) {
      return;
    }
    await _choose(strip, source);
  }

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
                  if (MediaQuery.sizeOf(context).width < 700) {
                    return _mobileRoutes();
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
                            padding: const EdgeInsets.all(10),
                            child: Row(children: [
                              Expanded(child: _targetSelector()),
                              const SizedBox(width: 10),
                              Expanded(
                                  child: _selector(store.groups, store.group,
                                      store.groupLabel, store.selectGroup)),
                              const SizedBox(width: 4),
                              const Tooltip(
                                  message:
                                      'Hover to preview, click to route. On touch, hold and drag, then release to confirm.',
                                  child: SizedBox(
                                      width: 44,
                                      height: 44,
                                      child:
                                          Icon(Icons.info_outline, size: 20))),
                            ])),
                        Container(
                            padding: const EdgeInsets.symmetric(
                                horizontal: 16, vertical: 8),
                            color: p.well,
                            child: Text(
                                preview
                                    ? '${_previewStrip!.name}: ${_previewStrip!.src} → ${_previewSource!.label}'
                                    : '${strips.length} destinations · ${sources.length} sources',
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(color: p.cyan))),
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
                                                                    button: true,
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
