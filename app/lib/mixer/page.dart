import 'dart:math' as math;
import 'package:flutter/gestures.dart';
import 'package:material_ui/material_ui.dart';
import '../ui/console.dart';
import 'package:flutter/services.dart';
import 'controller.dart';
import 'design.dart';
import 'geometry.dart';
import '../settings/prefs.dart';
import 'components/banks.dart';
import 'components/layers.dart';
import 'components/strip.dart';
import 'components/toolbar.dart';

/// Responsive composition only. Shared controls are in components/ and all
/// selection, paging, demo initialisation and writes are in MixerController.
class MixerPage extends StatefulWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Return to the Mixer workspace home.
  final VoidCallback? onHome;
  final bool embedded;
  const MixerPage(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome,
      this.embedded = false});
  @override
  State<MixerPage> createState() => _MixerPageState();
}

class _MixerPageState extends State<MixerPage> {
  late final ScrollController _scroll;
  final _focus = FocusNode(debugLabel: 'Mixer keyboard');
  int? _configuredSize;
  String? _bank;
  String? _layerId;
  int? _page;

  /// Explicit mixer layout mode. It only appears for app-owned layers and
  /// custom views; All and native banks stay read-only. The draft keys are
  /// local until Save & use, so Cancel never changes the saved layout.
  List<String>? _draftKeys;
  List<MixerChannel>? _draftChannels;
  String? _editSelectionKey;

  @override
  void initState() {
    super.initState();
    // Retain horizontal scroll across tab switches (channel home returns
    // to the same mixer state).
    _scroll =
        ScrollController(initialScrollOffset: widget.controller.scrollOffset);
    // The first frame is a return to the retained bank/page, not a change
    // of bank/page that should reset its saved horizontal position.
    _bank = widget.controller.store.bank;
    _layerId = widget.controller.activeLayer.id;
    _page = widget.controller.page;
  }

  @override
  void dispose() {
    widget.controller.scrollOffset = _scroll.hasClients ? _scroll.offset : 0;
    _scroll.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _startLayoutEdit(MixerController controller) {
    if (!controller.canEditLayout) {
      showMixerLayers(context, controller);
      return;
    }
    setState(() {
      _draftKeys = controller.editableChannelKeys;
      _draftChannels = controller.channelsForKeys(_draftKeys!);
      _editSelectionKey = controller.selectionKey;
    });
  }

  void _cancelLayoutEdit() {
    setState(() {
      _draftKeys = null;
      _draftChannels = null;
      _editSelectionKey = null;
    });
  }

  Future<void> _saveLayoutEdit(MixerController controller) async {
    final keys = _draftKeys;
    if (keys == null) return;
    await controller.saveEditableChannelKeys(keys);
    if (mounted) _cancelLayoutEdit();
  }

  void _addDraftChannel(String key) {
    if (_draftKeys?.contains(key) ?? false) return;
    setState(() {
      _draftKeys = [...?_draftKeys, key];
      _draftChannels = widget.controller.channelsForKeys(_draftKeys!);
    });
  }

  void _removeDraftChannel(String key) {
    if (!(_draftKeys?.contains(key) ?? false)) return;
    setState(() {
      _draftKeys = [...?_draftKeys]..remove(key);
      _draftChannels = widget.controller.channelsForKeys(_draftKeys!);
    });
  }

  void _moveDraftChannel(String key, int delta) {
    final visible = _draftChannels?.map((channel) => channel.key).toList();
    final keys = _draftKeys;
    if (visible == null || keys == null) return;
    final from = visible.indexOf(key);
    if (from < 0) return;
    final to = (from + delta).clamp(0, visible.length - 1);
    if (to == from) return;
    visible.insert(to, visible.removeAt(from));
    setState(() {
      _draftKeys = widget.controller.mergeVisibleOrder(keys, visible);
      _draftChannels = widget.controller.channelsForKeys(_draftKeys!);
    });
  }

  void _reorderDraftChannel(int oldIndex, int newIndex) {
    final visible = _draftChannels?.map((channel) => channel.key).toList();
    final keys = _draftKeys;
    if (visible == null || keys == null) return;
    if (oldIndex < 0 || oldIndex >= visible.length) return;
    final moved = visible.removeAt(oldIndex);
    visible.insert(newIndex.clamp(0, visible.length), moved);
    setState(() {
      _draftKeys = widget.controller.mergeVisibleOrder(keys, visible);
      _draftChannels = widget.controller.channelsForKeys(_draftKeys!);
    });
  }

  @override
  Widget build(BuildContext context) => Material(
      color: ConsolePalette.of(context).background,
      child: SafeArea(
          bottom: false,
          child: LayoutBuilder(builder: (context, constraints) {
            final controller = widget.controller;
            final mobile = constraints.maxWidth < AppDesign.narrowBreakpoint;
            final sideRail = !mobile;
            return ListenableBuilder(
                // Rebuild on prefs too so large-targets/interface changes
                // apply without reopening the screen (rare notifications).
                listenable:
                    Listenable.merge([controller, PrefsService.instance]),
                builder: (context, _) {
                  final pageSize =
                      controller.store.strips.length.clamp(16, 9999);
                  if (_configuredSize != pageSize) {
                    _configuredSize = pageSize;
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted && _configuredSize == pageSize) {
                        controller.configurePageSize(pageSize);
                      }
                    });
                  }
                  if (_bank != controller.store.bank ||
                      _layerId != controller.activeLayer.id ||
                      _page != controller.page) {
                    _bank = controller.store.bank;
                    _layerId = controller.activeLayer.id;
                    _page = controller.page;
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted && _scroll.hasClients) _scroll.jumpTo(0);
                    });
                  }
                  final selectionKey = controller.selectionKey;
                  final editing = _draftKeys != null &&
                      controller.canEditLayout &&
                      _editSelectionKey == selectionKey;
                  if (_draftKeys != null && !editing) {
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted && _draftKeys != null) _cancelLayoutEdit();
                    });
                  }
                  final channels = editing
                      ? (_draftChannels ?? const <MixerChannel>[])
                      : controller.channels;
                  // Shared footer geometry keeps faders and Solo rows aligned
                  // when only some channels have icons. An icon-free bank can
                  // collapse the artwork band as a whole.
                  final reserveArtwork = channels.any((ch) => ch.showsArtwork);
                  final railWidth = sideRail
                      ? MixerGeometry.railWidth(constraints.maxWidth)
                      : 0.0;
                  final stripWidth = MixerGeometry.stripWidth(
                      constraints.maxWidth,
                      mobile,
                      controller.isCustomLayerActive
                          ? controller.effectivePageSize
                          : 8);
                  // Edit mode needs room for a drag handle, a readable name
                  // and the strip options control even on a phone-width
                  // surface; the list already scrolls horizontally.
                  final editingStripWidth =
                      math.max(stripWidth, AppDesign.touchTarget * 3);
                  Widget strip(MixerChannel ch) => MixerStrip(
                      key: ValueKey(ch.key),
                      channel: ch,
                      controller: controller,
                      reserveArtwork: reserveArtwork,
                      identityChannel:
                          ch.master ? controller.masterIdentityChannel : null,
                      compact: mobile,
                      bottomInset: MediaQuery.paddingOf(context).bottom,
                      onOpenProcessing: ch.master
                          ? () {
                              controller.select(ch);
                              if (controller.openMasterProcessing()) {
                                widget.onNavigate(MixerDestination.channel);
                              } else {
                                return controller
                                    .masterProcessingUnavailableReason;
                              }
                              return null;
                            }
                          : null,
                      onOpenAt: ch.master
                          ? (anchor) => showMixerBuses(anchor, controller)
                          : null,
                      onOpen: () {
                        controller.select(ch);
                        if (ch.master) {
                          showMixerBuses(context, controller);
                        } else {
                          widget.onNavigate(MixerDestination.channel);
                        }
                      });
                  // Layout edit header: the drag handle lives here, not over
                  // the fader, so normal mixing gestures stay untouched. The
                  // menu is a tap alternative for moving or removing a strip.
                  Widget editHeader(MixerChannel ch, int index) {
                    final palette = ConsolePalette.of(context);
                    return Container(
                        height: AppDesign.touchTarget,
                        color: palette.chip,
                        child: Row(children: [
                          Expanded(
                              child: ReorderableDragStartListener(
                                  index: index,
                                  child: Row(children: [
                                    const SizedBox(width: 4),
                                    Semantics(
                                        label: 'Reorder ${ch.name}',
                                        child: Icon(Icons.drag_indicator,
                                            size: 18,
                                            color: palette.mutedText)),
                                    const SizedBox(width: 4),
                                    Expanded(
                                        child: Text(ch.name,
                                            maxLines: 1,
                                            overflow: TextOverflow.ellipsis,
                                            style: mixerType(context, 12,
                                                weight: FontWeight.w600))),
                                  ]))),
                          PopupMenuButton<String>(
                            popUpAnimationStyle: AnimationStyle.noAnimation,
                            tooltip: 'Strip options',
                            icon: Icon(Icons.more_horiz,
                                color: palette.mutedText),
                            color: palette.chip,
                            onSelected: (value) {
                              switch (value) {
                                case 'left':
                                  _moveDraftChannel(ch.key, -1);
                                case 'right':
                                  _moveDraftChannel(ch.key, 1);
                                case 'remove':
                                  _removeDraftChannel(ch.key);
                              }
                            },
                            itemBuilder: (context) => [
                              PopupMenuItem(
                                  value: 'left',
                                  enabled: index > 0,
                                  child: const Text('Move left')),
                              PopupMenuItem(
                                  value: 'right',
                                  enabled: index < channels.length - 1,
                                  child: const Text('Move right')),
                              const PopupMenuItem(
                                  value: 'remove', child: Text('Remove')),
                            ],
                          ),
                        ]));
                  }

                  Widget addStripTile() {
                    final palette = ConsolePalette.of(context);
                    return Builder(
                        builder: (anchor) => Material(
                            color: palette.panel,
                            child: InkWell(
                                onTap: () {
                                  // This tile fills the strip column, so it is
                                  // a full-height drop target while the label
                                  // sits in the middle. Anchor the chooser to
                                  // the tile's top edge: a full-height anchor
                                  // leaves only the space above it, which is
                                  // the bar rather than the strip area.
                                  final box =
                                      anchor.findRenderObject() as RenderBox?;
                                  showMixerLayerAddPanel(
                                    anchor,
                                    controller,
                                    included: () =>
                                        _draftKeys ?? const <String>[],
                                    onAdd: _addDraftChannel,
                                    anchorBounds: box != null && box.hasSize
                                        ? Rect.fromLTWH(
                                            0,
                                            0,
                                            box.size.width,
                                            math.min(box.size.height,
                                                AppDesign.touchTarget))
                                        : null,
                                  );
                                },
                                child: Center(
                                    child: Column(
                                        mainAxisSize: MainAxisSize.min,
                                        children: [
                                      Icon(Icons.add_circle_outline,
                                          size: 30, color: palette.accent),
                                      const SizedBox(height: 6),
                                      Text('Add strip',
                                          style: mixerType(context, 12,
                                              color: palette.mutedText)),
                                    ])))));
                  }

                  Widget editableStripList() =>
                      LayoutBuilder(builder: (context, box) {
                        final itemHeight = box.maxHeight;
                        return ReorderableListView.builder(
                            scrollDirection: Axis.horizontal,
                            scrollController: _scroll,
                            buildDefaultDragHandles: false,
                            itemExtent: editingStripWidth,
                            itemCount: channels.length + 1,
                            onReorderItem: _reorderDraftChannel,
                            itemBuilder: (context, index) {
                              if (index == channels.length) {
                                return SizedBox(
                                    key: const ValueKey('mixer-edit-add'),
                                    width: editingStripWidth,
                                    height: itemHeight,
                                    child: addStripTile());
                              }
                              final ch = channels[index];
                              // The strip positions every row itself, so it
                              // needs the full item width: a loose cross axis
                              // lets the body collapse to zero width.
                              return SizedBox(
                                  key: ValueKey('mixer-edit:${ch.key}'),
                                  width: editingStripWidth,
                                  height: itemHeight,
                                  child: Column(
                                      crossAxisAlignment:
                                          CrossAxisAlignment.stretch,
                                      children: [
                                    editHeader(ch, index),
                                    SizedBox(
                                        height: (itemHeight -
                                                AppDesign.touchTarget)
                                            .clamp(0.0, itemHeight),
                                        child: strip(ch)),
                                  ]));
                            });
                      });
                  Widget emptyState() {
                    final view = controller.activeCustomView;
                    final palette = ConsolePalette.of(context);
                    if (controller.isCustomLayerActive || view != null) {
                      final name =
                          controller.isCustomLayerActive ? controller.activeLayer.name : view!.name;
                      return Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(name,
                                style: mixerType(context, 18,
                                    weight: FontWeight.w600)),
                            const SizedBox(height: 8),
                            Padding(
                                padding:
                                    const EdgeInsets.symmetric(horizontal: 32),
                                child: Text(controller.layerEmptyMessage,
                                    textAlign: TextAlign.center,
                                    style: mixerType(context, 15,
                                        color: palette.mutedText))),
                            const SizedBox(height: 12),
                            ConsoleButton(
                              label: controller.isCustomLayerActive
                                  ? 'Edit layer'
                                  : 'Edit view',
                              onPressed: () => showMixerLayers(context,
                                  controller,
                                  editActive: true),
                            ),
                          ]);
                    }
                    return Text(controller.store.emptyBankMessage,
                        style: mixerType(context, 16));
                  }

                  return CallbackShortcuts(
                      bindings: {
                        const SingleActivator(LogicalKeyboardKey.arrowUp): () =>
                            controller.nudgeSelected(.01),
                        const SingleActivator(LogicalKeyboardKey.arrowDown):
                            () => controller.nudgeSelected(-.01),
                        const SingleActivator(LogicalKeyboardKey.arrowLeft):
                            () => controller.moveSelection(-1),
                        const SingleActivator(LogicalKeyboardKey.arrowRight):
                            () => controller.moveSelection(1),
                        const SingleActivator(LogicalKeyboardKey.keyM): () {
                          final ch = controller.selected;
                          if (ch != null) controller.toggleMute(ch);
                        },
                        const SingleActivator(LogicalKeyboardKey.keyS): () {
                          final ch = controller.selected;
                          if (ch != null) controller.toggleSolo(ch);
                        },
                      },
                      child: Focus(
                          focusNode: _focus,
                          autofocus: true,
                          child: Column(children: [
                            if (!widget.embedded)
                              MixerToolbar(
                                  controller: controller,
                                  onNavigate: widget.onNavigate,
                                  onHome: widget.onHome,
                                  compact: constraints.maxWidth < AppDesign.compactToolbarBreakpoint,
                                  onOpenTools: widget.onHome),
                            // Design bus bar: full-width strip in the
                            // selected bus colour between toolbar + strips.
                            if (!widget.embedded)
                              Container(
                                  height: mobile ? 6 : 8,
                                  color: controller
                                      .masterColor(ConsolePalette.of(context))),
                            // Always-visible layers picker: names the current
                            // selection and exposes Edit layout without
                            // hiding the entry behind Tools.
                            _MixerLayerBar(
                                controller: controller,
                                editing: editing,
                                onStartEdit: () =>
                                    _startLayoutEdit(controller),
                                onCancelEdit: _cancelLayoutEdit,
                                onSaveEdit: () =>
                                    _saveLayoutEdit(controller),
                                onCreate: () => showMixerLayers(context,
                                    controller,
                                    createLayer: true)),
                            if (controller.error.isNotEmpty)
                              Material(
                                  color: ConsolePalette.of(context).over,
                                  child: InkWell(
                                      onTap: () => widget
                                          .onNavigate(MixerDestination.connect),
                                      child: Padding(
                                          padding: const EdgeInsets.symmetric(
                                              horizontal: 12, vertical: 6),
                                          child: Row(children: [
                                            Icon(Icons.info_outline,
                                                size: 16,
                                                color:
                                                    ConsolePalette.of(context)
                                                        .text),
                                            const SizedBox(width: 8),
                                            Expanded(
                                                child: Text(controller.error,
                                                    maxLines: 2,
                                                    overflow:
                                                        TextOverflow.ellipsis,
                                                    style:
                                                        mixerType(context, 12)))
                                          ])))),
                            Expanded(
                                child: Row(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.stretch,
                                    children: [
                                  Expanded(
                                      child: channels.isEmpty && !editing
                                          ? emptyState()
                                          : editing
                                              ? editableStripList()
                                              : ScrollConfiguration(
                                                    behavior: const MaterialScrollBehavior()
                                                        .copyWith(
                                                            scrollbars: false,
                                                            dragDevices: {
                                                          PointerDeviceKind
                                                              .touch,
                                                          PointerDeviceKind
                                                              .mouse,
                                                          PointerDeviceKind
                                                              .trackpad
                                                        }),
                                                    child: ListView.builder(
                                                        controller: _scroll,
                                                        scrollDirection:
                                                            Axis.horizontal,
                                                        itemExtent: stripWidth,
                                                        itemCount:
                                                            channels.length,
                                                        itemBuilder:
                                                            (context, i) =>
                                                                strip(channels[
                                                                    i])))),
                                  SizedBox(
                                      width: stripWidth,
                                      child: strip(controller.master)),
                                  if (sideRail)
                                    SizedBox(
                                        width: railWidth,
                                        child: MixerBanks(
                                            controller: controller,
                                            vertical: true)),
                                ])),
                            if (!sideRail && !mobile)
                              MixerBanks(
                                  controller: controller, vertical: false),
                          ])));
                });
          })));
}

/// Persistent selection bar. It always names the active layer/view and opens
/// the full Layers workspace, so creation and management are never hidden
/// behind a Tools menu. In Edit layout mode it becomes the draft action bar:
/// Cancel discards the draft and Save & use commits it.
class _MixerLayerBar extends StatelessWidget {
  final MixerController controller;
  final bool editing;
  final VoidCallback onStartEdit;
  final VoidCallback onCancelEdit;
  final VoidCallback onSaveEdit;
  final VoidCallback onCreate;
  const _MixerLayerBar(
      {required this.controller,
      required this.editing,
      required this.onStartEdit,
      required this.onCancelEdit,
      required this.onSaveEdit,
      required this.onCreate});
  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final wide = MediaQuery.sizeOf(context).width >=
        AppDesign.compactToolbarBreakpoint;
    return Material(
        color: palette.panel,
        child: Container(
            height: MixerLayerMetrics.barHeight,
            padding: MixerLayerMetrics.barPadding,
            decoration: BoxDecoration(
                border: Border(bottom: BorderSide(color: palette.line))),
            child: editing
                ? Row(children: [
                    Icon(Icons.layers_outlined,
                        size: 18, color: palette.mutedText),
                    const SizedBox(width: MixerLayerMetrics.gap),
                    Expanded(
                        child: Text('Editing ${controller.selectionLabel}',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: mixerType(context, 14,
                                weight: FontWeight.w600))),
                    ConsoleButton(
                        label: 'Cancel',
                        variant: ConsoleButtonVariant.quiet,
                        size: ConsoleButtonSize.dense,
                        onPressed: onCancelEdit),
                    const SizedBox(width: MixerLayerMetrics.gap),
                    ConsoleButton(
                        label: 'Save & use',
                        variant: ConsoleButtonVariant.primary,
                        size: ConsoleButtonSize.dense,
                        onPressed: onSaveEdit),
                  ])
                : Tooltip(
                    message: 'Layers',
                    child: InkWell(
                        onTap: () => showMixerLayers(context, controller),
                        child: Row(children: [
                          Icon(Icons.layers_outlined,
                              size: 18, color: palette.mutedText),
                          const SizedBox(width: MixerLayerMetrics.gap),
                          Expanded(
                              child: Text(controller.selectionLabel,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: mixerType(context, 14,
                                      weight: FontWeight.w600))),
                          if (wide)
                            Text(
                                '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                                style: mixerType(context, 12,
                                    color: palette.mutedText)),
                          if (controller.canEditLayout)
                            ConsoleIconButton(
                                label: 'Edit layout',
                                onPressed: onStartEdit,
                                child: Icon(Icons.edit_outlined,
                                    size: 18, color: palette.mutedText)),
                          ConsoleIconButton(
                              label: 'New layer',
                              onPressed: onCreate,
                              child: Icon(Icons.add,
                                  size: 20, color: palette.accent)),
                          ConsoleIconButton(
                              label: 'Open layers',
                              onPressed: () =>
                                  showMixerLayers(context, controller),
                              child: Icon(Icons.expand_more,
                                  size: 20, color: palette.mutedText)),
                        ])))));
  }
}
