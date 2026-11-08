import '../palette.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
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

  @override
  Widget build(BuildContext context) => Material(
      color: ConsolePalette.of(context).background,
      child: SafeArea(
          bottom: false,
          child: LayoutBuilder(builder: (context, constraints) {
            final controller = widget.controller;
            final mobile = constraints.maxWidth < 600;
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
                  final channels = controller.channels;
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
                                  compact: constraints.maxWidth < 800,
                                  onOpenTools: widget.onHome),
                            // Design bus bar: full-width strip in the
                            // selected bus colour between toolbar + strips.
                            if (!widget.embedded)
                              Container(
                                  height: mobile ? 6 : 8,
                                  color: controller
                                      .masterColor(ConsolePalette.of(context))),
                            // Custom layer banner: names the active layer.
                            // Tapping opens the layer list; the master strip
                            // stays fixed and is never part of a layer.
                            if (controller.isCustomLayerActive)
                              GestureDetector(
                                  behavior: HitTestBehavior.opaque,
                                  onTap: () =>
                                      showMixerLayers(context, controller),
                                  child: Padding(
                                      padding: const EdgeInsets.symmetric(
                                          horizontal: 12, vertical: 6),
                                      child: Row(children: [
                                        Icon(Icons.layers_outlined,
                                            size: 16,
                                            color: ConsolePalette.of(context)
                                                .mutedText),
                                        const SizedBox(width: 6),
                                        Expanded(
                                            child: Text(
                                                controller.activeLayer.name,
                                                maxLines: 1,
                                                overflow: TextOverflow.ellipsis,
                                                style: mixerType(context, 14,
                                                    weight: FontWeight.w600))),
                                        Text(
                                            '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                                            style: mixerType(context, 12,
                                                color:
                                                    ConsolePalette.of(context)
                                                        .mutedText)),
                                      ]))),
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
                                      child: channels.isEmpty
                                          ? Center(
                                              child:
                                                  controller.isCustomLayerActive
                                                      ? Column(
                                                          mainAxisSize:
                                                              MainAxisSize.min,
                                                          children: [
                                                              Text(
                                                                  controller
                                                                      .activeLayer
                                                                      .name,
                                                                  style: mixerType(
                                                                      context,
                                                                      18,
                                                                      weight: FontWeight
                                                                          .w600)),
                                                              const SizedBox(
                                                                  height: 8),
                                                              Padding(
                                                                  padding: const EdgeInsets
                                                                      .symmetric(
                                                                      horizontal:
                                                                          32),
                                                                  child: Text(
                                                                      controller
                                                                          .layerEmptyMessage,
                                                                      textAlign:
                                                                          TextAlign
                                                                              .center,
                                                                      style: mixerType(
                                                                          context,
                                                                          15,
                                                                          color:
                                                                              ConsolePalette.of(context).mutedText))),
                                                              const SizedBox(
                                                                  height: 12),
                                                              OutlinedButton(
                                                                onPressed: () =>
                                                                    showMixerLayerEditor(
                                                                  context,
                                                                  layer: controller
                                                                      .activeLayer,
                                                                  channels:
                                                                      layerChannelOptionsOf(
                                                                          controller),
                                                                ),
                                                                child: const Text(
                                                                    'Edit layer'),
                                                              ),
                                                            ])
                                                      : Text(
                                                          controller.store
                                                              .emptyBankMessage,
                                                          style: mixerType(
                                                              context, 16)))
                                          : ScrollConfiguration(
                                              behavior:
                                                  const MaterialScrollBehavior()
                                                      .copyWith(
                                                          scrollbars: false,
                                                          dragDevices: {
                                                    PointerDeviceKind.touch,
                                                    PointerDeviceKind.mouse,
                                                    PointerDeviceKind.trackpad
                                                  }),
                                              child: ListView.builder(
                                                  controller: _scroll,
                                                  scrollDirection:
                                                      Axis.horizontal,
                                                  itemExtent: stripWidth,
                                                  itemCount: channels.length,
                                                  itemBuilder: (context, i) =>
                                                      strip(channels[i])))),
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
