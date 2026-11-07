// Patch matrix: previews stay local; confirmed edits use the injected store.
// The store retains provider identity, access and stale-operation guards.
import 'package:flutter/material.dart';

import '../theme.dart';
import '../ui/console.dart';
import '../core/routing_store.dart';
import '../core/destination_controls.dart';
import '../widgets/destination_control_editor.dart';

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
  RouteStrip? _pickedStrip;
  RouteSrcTile? _pickedSource;
  String? _pickedTarget;
  (int, int)? _dragStart;
  List<(int, RouteSrcTile)> _dragPending = [];
  bool _confirming = false;
  bool _pickingSource = false;
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
    final sources = store.allSrcTiles;
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
    _dragStart ??= (row, column);
    final (firstRow, firstColumn) = _dragStart!;
    final rowStep = row >= firstRow ? 1 : -1;
    final sourceStep = column == firstColumn
        ? 0
        : column > firstColumn
            ? 1
            : -1;
    final pending = <(int, RouteSrcTile)>[];
    for (var offset = 0; offset <= (row - firstRow).abs(); offset++) {
      final sourceIndex = firstColumn + offset * sourceStep;
      if (sourceIndex < 0 || sourceIndex >= sources.length) break;
      pending.add(
          (strips[firstRow + offset * rowStep].strip, sources[sourceIndex]));
    }
    setState(() => _dragPending = pending);
    _preview(strips[row], sources[column]);
  }

  Future<void> _choose(RouteStrip strip, RouteSrcTile source) async {
    if (_confirming) return;
    setState(() {
      _pickedStrip = strip;
      _pickedSource = source;
      _pickedTarget = store.target;
    });
    store.selectStrip(strip.strip);
    final tile = store.allSrcTiles
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
    _confirming = true;
    final target = store.target;
    final epoch = store.controlAccessEpoch;
    await showModalBottomSheet<void>(
      sheetAnimationStyle: AnimationStyle.noAnimation,
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
                    store.allSrcTiles.any((t) =>
                        t.group == tile.group &&
                        t.index == tile.index &&
                        t.operationKey == tile.operationKey);
                return Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                          '${tile.mode ? 'Input mode ·' : 'Route'} ${strip.name}',
                          style: Theme.of(context).textTheme.titleLarge),
                      const SizedBox(height: AppDesign.spaceLg),
                      Text('Current${tile.mode ? ' mode' : ''}: ${strip.src}'),
                      const SizedBox(height: AppDesign.spaceSm),
                      Text(
                          '${tile.mode ? 'New mode' : 'New source'}: ${tile.label}',
                          style: const TextStyle(fontWeight: FontWeight.w700)),
                      const SizedBox(height: AppDesign.spaceLg),
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
                          child: Text(
                              tile.mode ? 'Confirm mode' : 'Confirm route')),
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

  Widget _targetSelector() {
    final p = ConsolePalette.of(context);
    return LayoutBuilder(builder: (context, constraints) {
      final compact =
          MediaQuery.sizeOf(context).width < AppDesign.phoneBreakpoint;
      final tabStyle = Theme.of(context)
          .textTheme
          .labelLarge
          ?.copyWith(fontSize: compact ? 13 : 14);
      var minimum = compact ? 80.0 : 96.0;
      for (final target in store.targets) {
        final text = TextPainter(
            text: TextSpan(text: target.label, style: tabStyle),
            textDirection: Directionality.of(context))
          ..layout();
        final needed = text.width + (compact ? 24 : 32);
        if (needed > minimum) minimum = needed;
      }
      final width = (minimum * store.targets.length +
              8 * (store.targets.length - 1).clamp(0, store.targets.length))
          .clamp(constraints.maxWidth, double.infinity);
      return SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: SizedBox(
              width: width,
              child: Row(spacing: 8, children: [
                for (final target in store.targets)
                  Expanded(
                      child: Semantics(
                          selected: target.key == store.target,
                          child: Container(
                            decoration: BoxDecoration(
                                border: Border(
                                    bottom: BorderSide(
                                        color: target.key == store.target
                                            ? p.cyan
                                            : p.tileBorder,
                                        width: target.key == store.target
                                            ? 3
                                            : 1))),
                            child: TextButton(
                                style: TextButton.styleFrom(
                                    minimumSize: const Size(0, 48),
                                    foregroundColor: target.key == store.target
                                        ? p.cyan
                                        : p.text4,
                                    padding: const EdgeInsets.symmetric(
                                        horizontal: 12)),
                                onPressed: () {
                                  if (target.key == store.target) return;
                                  setState(() {
                                    _previewStrip = null;
                                    _previewSource = null;
                                    _pickedStrip = null;
                                    _pickedSource = null;
                                    _dragStart = null;
                                    _dragPending = [];
                                  });
                                  store.selectTarget(target.key);
                                },
                                child: Text(target.label,
                                    maxLines: 1, style: tabStyle)),
                          ))),
              ])));
    });
  }

  Widget _modeRows() {
    final p = ConsolePalette.of(context);
    final sources = store.allSrcTiles;
    Widget name(String text) => Expanded(
        child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Text(text, maxLines: 1, overflow: TextOverflow.ellipsis)));
    return Column(children: [
      Padding(
          padding: AppDesign.pagePadding,
          child: Row(
              spacing: 8,
              children: [Expanded(child: _targetSelector()), _mapButton()])),
      Container(
          height: 48,
          color: p.well,
          child: Row(children: [
            name('Active input'),
            for (final source in sources)
              Expanded(
                  child: Center(
                      child: Text(source.label,
                          style:
                              const TextStyle(fontWeight: FontWeight.w600)))),
          ])),
      Expanded(
          child: ListView.separated(
              itemCount: store.strips.length,
              separatorBuilder: (_, __) =>
                  Divider(height: 1, color: p.tileBorder),
              itemBuilder: (context, i) {
                final strip = store.strips[i];
                return SizedBox(
                    height: 56,
                    child: Row(children: [
                      name(strip.name),
                      for (final source in sources)
                        Expanded(
                            child: Padding(
                                padding: const EdgeInsets.symmetric(
                                    horizontal: 6, vertical: 4),
                                child: OutlinedButton(
                                  style: OutlinedButton.styleFrom(
                                      minimumSize: const Size(0, 48),
                                      backgroundColor: strip.src == source.label
                                          ? p.cyan
                                          : p.well,
                                      foregroundColor: strip.src == source.label
                                          ? p.onAccent
                                          : p.text4),
                                  onPressed: () => _choose(strip, source),
                                  child: Semantics(
                                      selected: strip.src == source.label,
                                      label: '${strip.name}: ${source.label}',
                                      child: Row(
                                          mainAxisAlignment:
                                              MainAxisAlignment.center,
                                          children: [
                                            if (strip.src == source.label)
                                              const Padding(
                                                  padding:
                                                      EdgeInsets.only(right: 6),
                                                  child: Icon(Icons.check,
                                                      size: 18)),
                                            Flexible(
                                                child: Text(source.label,
                                                    maxLines: 1,
                                                    overflow:
                                                        TextOverflow.ellipsis)),
                                          ])),
                                ))),
                    ]));
              })),
    ]);
  }

  Widget _mapButton() => Builder(builder: (anchor) {
        final action = store.strips.isEmpty || store.allSrcTiles.isEmpty
            ? null
            : () => _quickMap(anchor);
        if (MediaQuery.sizeOf(context).width < AppDesign.phoneBreakpoint) {
          return TextButton(
              onPressed: action,
              style: TextButton.styleFrom(
                  minimumSize: const Size(52, 48),
                  padding: const EdgeInsets.symmetric(horizontal: 10)),
              child: const Text('Map', style: TextStyle(fontSize: 13)));
        }
        return TextButton.icon(
            onPressed: action,
            icon: const Icon(Icons.auto_fix_high_outlined, size: 18),
            label: const Text('Map'),
            style: TextButton.styleFrom(minimumSize: const Size(72, 48)));
      });

  Future<void> _quickMap(BuildContext anchor) async {
    final target = store.target;
    if (store.isModeSelector) {
      final source = await showAnchoredMenu<RouteSrcTile>(anchor, [
        for (final source in store.allSrcTiles)
          PopupMenuItem(
              height: 52, value: source, child: Text('All ${source.label}')),
      ]);
      if (mounted && source != null && store.target == target) {
        await _reviewMappings(
            [for (final strip in store.strips) (strip.strip, source)]);
      }
      return;
    }
    final mode = await showAnchoredMenu<String>(anchor, [
      PopupMenuItem(
          height: 56,
          value: 'forward',
          child: ListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('1:1'),
              subtitle:
                  Text('${store.groupLabel(store.groups.first)} · in order'))),
      PopupMenuItem(
          height: 56,
          value: 'reverse',
          child: ListTile(
              contentPadding: EdgeInsets.zero,
              title: const Text('Reverse 1:1'),
              subtitle: Text(
                  '${store.groupLabel(store.groups.first)} · reverse order'))),
      if (_pickedTarget == store.target &&
          _pickedStrip != null &&
          _pickedSource != null)
        PopupMenuItem(
            height: 64,
            value: 'selected',
            child: ListTile(
                contentPadding: EdgeInsets.zero,
                title: const Text('1:1 from selected point'),
                subtitle:
                    Text('${_pickedSource!.label} to ${_pickedStrip!.name}'))),
    ]);
    if (store.target != target) return;
    if (!mounted || mode == null) return;
    final strips = store.strips;
    // Presets stay within a single bank instead of spilling into USB/network.
    final sourceGroup =
        mode == 'selected' ? _pickedSource?.group : store.groups.firstOrNull;
    final sources =
        store.allSrcTiles.where((s) => s.group == sourceGroup).toList();
    final firstRow = mode == 'selected'
        ? strips.indexWhere((s) => s.id == _pickedStrip?.id)
        : 0;
    final firstColumn = mode == 'selected'
        ? sources.indexWhere((s) =>
            s.group == _pickedSource?.group && s.index == _pickedSource?.index)
        : 0;
    if (firstRow < 0 || firstColumn < 0) return;
    final count = (strips.length - firstRow) < (sources.length - firstColumn)
        ? strips.length - firstRow
        : sources.length - firstColumn;
    await _reviewMappings([
      for (var i = 0; i < count; i++)
        (
          strips[firstRow + i].strip,
          sources[firstColumn + (mode == 'reverse' ? count - 1 - i : i)]
        )
    ]);
  }

  Future<void> _reviewMappings(List<(int, RouteSrcTile)> mappings) async {
    final plan = store.planMappings(mappings);
    if (plan == null || plan.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(
          content: Text(plan == null
              ? 'Cannot map this range: a destination is protected or unavailable.'
              : 'These routes already match.')));
      return;
    }
    _confirming = true;
    final target = store.target;
    await showModalBottomSheet<void>(
        sheetAnimationStyle: AnimationStyle.noAnimation,
        context: context,
        isScrollControlled: true,
        showDragHandle: true,
        builder: (sheetContext) => FractionallySizedBox(
            heightFactor: .7,
            child: SafeArea(
                child: Padding(
                    padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                    child: ListenableBuilder(
                        listenable: store,
                        builder: (context, _) {
                          final valid = store.target == target &&
                              store.planIsCurrent(plan);
                          return Column(
                              crossAxisAlignment: CrossAxisAlignment.stretch,
                              children: [
                                Text(
                                    'Review ${plan.length} change${plan.length == 1 ? '' : 's'}',
                                    style:
                                        Theme.of(context).textTheme.titleLarge),
                                const SizedBox(height: AppDesign.spaceMd),
                                Expanded(
                                    child: ListView.builder(
                                        itemCount: plan.length,
                                        itemBuilder: (context, i) {
                                          final change = plan[i];
                                          return ListTile(
                                              contentPadding: EdgeInsets.zero,
                                              title:
                                                  Text(change.destination.name),
                                              subtitle: Text(
                                                  '${change.destination.src} to ${change.source.label}'));
                                        })),
                                if (!valid)
                                  const Text(
                                      'Routing changed. Close and review a fresh mapping.'),
                                FilledButton(
                                    onPressed: valid
                                        ? () {
                                            final sent = store.applyPlan(plan);
                                            Navigator.pop(sheetContext);
                                            if (sent) {
                                              ScaffoldMessenger.of(this.context)
                                                  .showSnackBar(SnackBar(
                                                      content: Text(
                                                          '${plan.length} routing changes sent')));
                                            }
                                          }
                                        : null,
                                    child:
                                        Text('Apply ${plan.length} changes')),
                                TextButton(
                                    onPressed: () =>
                                        Navigator.pop(sheetContext),
                                    child: const Text('Cancel')),
                              ]);
                        })))));
    if (mounted) setState(() => _confirming = false);
  }

  void _finishDrag() {
    final pending = List<(int, RouteSrcTile)>.of(_dragPending);
    final strip = _previewStrip;
    final source = _previewSource;
    final current = _previewTarget == store.target;
    setState(() {
      _dragStart = null;
      _dragPending = [];
    });
    if (!current || strip == null || source == null) return;
    if (pending.length > 1) {
      _reviewMappings(pending);
    } else {
      _choose(strip, source);
    }
  }

  Widget _mobileRoutes() {
    final p = ConsolePalette.of(context);
    return Column(children: [
      Padding(
          padding: AppDesign.pagePadding,
          child: Row(
              spacing: 8,
              children: [Expanded(child: _targetSelector()), _mapButton()])),
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
    if (_pickingSource || _confirming) return;
    _pickingSource = true;
    store.selectStrip(strip.strip);
    final target = store.target;
    final operation = store.operationKey;
    final gridKey = GlobalKey();
    RouteSrcTile? preview;
    final source = await showModalBottomSheet<RouteSrcTile>(
      sheetAnimationStyle: AnimationStyle.noAnimation,
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
                  final tiles = store.allSrcTiles;
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
                                    const SizedBox(height: AppDesign.spaceXs),
                                    Text('Current · ${strip.src}',
                                        style: TextStyle(color: p.text7)),
                                  ])),
                              IconButton(
                                  tooltip: 'Close',
                                  icon: const Icon(Icons.close),
                                  onPressed: () => Navigator.pop(sheetContext))
                            ])),
                        if (store.canClear || store.routingControls.isNotEmpty)
                          Padding(
                              padding:
                                  const EdgeInsets.symmetric(horizontal: 12),
                              child: Wrap(spacing: 8, children: [
                                if (store.canClear)
                                  TextButton(
                                      onPressed: valid ? _confirmClear : null,
                                      child: const Text('Unpatch')),
                                for (final control in store.routingControls)
                                  TextButton(
                                      onPressed: valid &&
                                              control.value != null &&
                                              store.routingControlIsCurrent(
                                                  control)
                                          ? () => showDialog<void>(
                                              animationStyle:
                                                  AnimationStyle.noAnimation,
                                              context: context,
                                              barrierDismissible: false,
                                              builder:
                                                  (_) => DestinationControlEditor(
                                                      changes: store,
                                                      accessEpoch: () => store
                                                          .controlAccessEpoch,
                                                      isCurrent: store
                                                          .routingControlIsCurrent,
                                                      apply: store
                                                          .setRoutingControl,
                                                      control: control))
                                          : null,
                                      child: Text(
                                          '${control.label}: ${control.valueLabel ?? 'Unavailable'}')),
                              ])),
                        const SizedBox(height: AppDesign.spaceMd),
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
                                                                  child: Text(tile.label,
                                                                      textAlign:
                                                                          TextAlign
                                                                              .center,
                                                                      maxLines:
                                                                          2,
                                                                      style: TextStyle(
                                                                          fontSize:
                                                                              13,
                                                                          fontWeight: FontWeight
                                                                              .w600,
                                                                          color: tile.current
                                                                              ? p.onAccent
                                                                              : p.text1))),
                                                            ]))))));
                                      }),
                                ))),
                        Padding(
                            padding: AppDesign.pagePadding,
                            child: Text(
                                preview == null
                                    ? 'Choose a source to preview the change'
                                    : '${preview!.label} to ${strip.name}',
                                textAlign: TextAlign.center,
                                style: TextStyle(
                                    color:
                                        preview == null ? p.text7 : p.cyan))),
                      ]);
                });
          }))),
    );
    _pickingSource = false;
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
                  if (store.isModeSelector) return _modeRows();
                  if (MediaQuery.sizeOf(context).width <
                      AppDesign.phoneBreakpoint) {
                    return _mobileRoutes();
                  }
                  final strips = store.strips;
                  final sources = store.allSrcTiles;
                  final activeStrip = _previewStrip ?? _pickedStrip;
                  final activeSource = _previewSource ?? _pickedSource;
                  final activeTarget =
                      _previewStrip == null ? _pickedTarget : _previewTarget;
                  final preview = activeTarget == store.target &&
                      strips.any((s) => s.id == activeStrip?.id) &&
                      sources.any((s) =>
                          s.group == activeSource?.group &&
                          s.index == activeSource?.index);
                  return Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Padding(
                            padding: AppDesign.pagePadding,
                            child: Row(spacing: 8, children: [
                              Expanded(child: _targetSelector()),
                              _mapButton(),
                            ])),
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
                                                  child: Row(
                                                      mainAxisSize:
                                                          MainAxisSize.min,
                                                      children: [
                                                    Text('DESTINATION',
                                                        style: TextStyle(
                                                            color: p.text7,
                                                            fontSize: 11)),
                                                    Icon(Icons.arrow_downward,
                                                        color: p.text7,
                                                        size: 12),
                                                  ]))),
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
                                                        color: preview &&
                                                                activeSource?.group ==
                                                                    source
                                                                        .group &&
                                                                activeSource
                                                                        ?.index ==
                                                                    source.index
                                                            ? p.selected
                                                            : p.well,
                                                        border: Border.all(
                                                            color:
                                                                p.tileBorder)),
                                                    child: Text(source.label,
                                                        textAlign:
                                                            TextAlign.center,
                                                        maxLines: 3,
                                                        overflow: TextOverflow
                                                            .ellipsis,
                                                        style: const TextStyle(fontSize: 11, fontWeight: FontWeight.w600)))),
                                        ]),
                                        Expanded(
                                            child: SingleChildScrollView(
                                                child: GestureDetector(
                                          onLongPressStart: (d) {
                                            _dragStart = null;
                                            _dragPending = [];
                                            _drag(d.globalPosition);
                                          },
                                          onLongPressMoveUpdate: (d) =>
                                              _drag(d.globalPosition),
                                          onLongPressCancel: () => setState(() {
                                            _previewStrip = null;
                                            _previewSource = null;
                                            _dragStart = null;
                                            _dragPending = [];
                                          }),
                                          onLongPressEnd: (_) => _finishDrag(),
                                          child: Column(
                                              key: _matrixKey,
                                              children: [
                                                for (final strip in strips)
                                                  Row(children: [
                                                    _destinationLabel(
                                                        strip,
                                                        preview &&
                                                            activeStrip?.id ==
                                                                strip.id),
                                                    for (final source
                                                        in sources)
                                                      Builder(
                                                          builder: (context) {
                                                        final connected =
                                                            strip.src ==
                                                                source.label;
                                                        final rowActive =
                                                            preview &&
                                                                activeStrip
                                                                        ?.id ==
                                                                    strip.id;
                                                        final columnActive =
                                                            preview &&
                                                                activeSource
                                                                        ?.group ==
                                                                    source
                                                                        .group &&
                                                                activeSource
                                                                        ?.index ==
                                                                    source
                                                                        .index;
                                                        final planned =
                                                            _dragPending.any((m) =>
                                                                m.$1 ==
                                                                    strip
                                                                        .strip &&
                                                                m.$2.group ==
                                                                    source
                                                                        .group &&
                                                                m.$2.index ==
                                                                    source
                                                                        .index);
                                                        final highlighted =
                                                            rowActive &&
                                                                    columnActive ||
                                                                planned;
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
                                                            child: Semantics(
                                                                label:
                                                                    '${source.label} to ${strip.name}',
                                                                button: true,
                                                                selected:
                                                                    connected,
                                                                child: SizedBox(
                                                                    width:
                                                                        _cell,
                                                                    height:
                                                                        _row,
                                                                    child: Padding(
                                                                        padding: const EdgeInsets.all(3),
                                                                        child: Material(
                                                                            color: connected
                                                                                ? p.cyan
                                                                                : highlighted || rowActive || columnActive
                                                                                    ? p.selected
                                                                                    : p.well,
                                                                            shape: RoundedRectangleBorder(borderRadius: U4.rBtn, side: BorderSide(color: highlighted ? p.cyan : p.tileBorder, width: highlighted ? 2 : 1)),
                                                                            child: InkWell(
                                                                                borderRadius: U4.rBtn,
                                                                                onTap: () => _choose(strip, source),
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
                                                                                                : p.text7))))))));
                                                      }),
                                                  ]),
                                              ]),
                                        ))),
                                      ]),
                                    ),
                                  ))),
                      ]);
                })),
      );

  Widget _destinationLabel(RouteStrip strip, bool active) => SizedBox(
        width: _labelWidth,
        height: _row,
        child: Builder(
            builder: (anchor) => TextButton(
                  style: TextButton.styleFrom(
                      backgroundColor: active
                          ? ConsolePalette.of(context).selected
                          : ConsolePalette.of(context).page),
                  onPressed: () => _destinationOptions(anchor, strip),
                  child: Align(
                      alignment: Alignment.centerLeft,
                      child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(strip.name,
                                maxLines: 1, overflow: TextOverflow.ellipsis),
                            Text(strip.src,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: TextStyle(
                                    fontSize: 11,
                                    color: ConsolePalette.of(context).text7)),
                          ])),
                )),
      );

  Future<void> _destinationOptions(
      BuildContext anchor, RouteStrip strip) async {
    store.selectStrip(strip.strip);
    final target = store.target;
    final key = store.operationKey;
    final epoch = store.controlAccessEpoch;
    final snapshot = List<DestinationControlVm>.of(store.routingControls);
    final choice = await showAnchoredMenu<String>(anchor, [
      if (store.canClear)
        const PopupMenuItem(value: 'clear', height: 48, child: Text('Unpatch')),
      for (final control in snapshot)
        PopupMenuItem(
            value: 'control:${control.id}',
            height: 48,
            enabled:
                control.value != null && store.routingControlIsCurrent(control),
            child: Text(
                '${control.label}: ${control.valueLabel ?? 'Unavailable'}')),
    ]);
    if (!mounted ||
        choice == null ||
        target != store.target ||
        key != store.operationKey ||
        epoch != store.controlAccessEpoch) {
      return;
    }
    if (choice == 'clear') {
      if (!store.canClear) return;
      await _confirmClear();
    } else if (choice.startsWith('control:')) {
      final id = choice.substring('control:'.length);
      final current = store.routingControls
          .where((control) => control.id == id)
          .firstOrNull;
      if (current == null ||
          current.value == null ||
          !store.routingControlIsCurrent(current)) {
        return;
      }
      await showDialog<void>(
          context: context,
          animationStyle: AnimationStyle.noAnimation,
          barrierDismissible: false,
          builder: (_) => DestinationControlEditor(
              changes: store,
              accessEpoch: () => store.controlAccessEpoch,
              isCurrent: store.routingControlIsCurrent,
              apply: store.setRoutingControl,
              control: current));
    }
  }

  Future<void> _confirmClear() async {
    final strip = store.selectedStrip;
    final key = store.operationKey;
    final name = store.selectedName;
    final confirmed = await showConsoleConfirmation(context,
        title: 'Unpatch $name?',
        message: 'This removes its current source.',
        confirmLabel: 'Unpatch');
    if (confirmed == true && mounted && strip != null) {
      store.clearPatch(strip, operationKey: key);
    }
  }
}
