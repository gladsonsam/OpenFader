import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'controller.dart';
import 'design.dart';
import 'geometry.dart';
import '../settings/prefs.dart';
import 'components/banks.dart';
import 'components/strip.dart';
import 'components/toolbar.dart';

/// Responsive composition only. Shared controls are in components/ and all
/// selection, paging, demo initialisation and writes are in MixerController.
class MixerPage extends StatefulWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Return to the Connections start screen (top-left home button).
  final VoidCallback? onHome;
  const MixerPage(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome});
  @override
  State<MixerPage> createState() => _MixerPageState();
}

class _MixerPageState extends State<MixerPage> {
  late final ScrollController _scroll;
  final _focus = FocusNode(debugLabel: 'Mixer keyboard');
  int? _configuredSize;
  String? _bank;
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
      color: MixerDesign.background,
      child: SafeArea(
          bottom: false,
          child: LayoutBuilder(builder: (context, constraints) {
            final controller = widget.controller;
            final mobile = constraints.maxWidth < 600;
            final sideRail = !mobile;
            const pageSize = 8;
            if (_configuredSize != pageSize) {
              _configuredSize = pageSize;
              WidgetsBinding.instance.addPostFrameCallback((_) {
                if (mounted) controller.configurePageSize(pageSize);
              });
            }
            return ListenableBuilder(
                // Rebuild on prefs too so large-targets/interface changes
                // apply without reopening the screen (rare notifications).
                listenable:
                    Listenable.merge([controller, PrefsService.instance]),
                builder: (context, _) {
                  if (_bank != controller.store.bank ||
                      _page != controller.page) {
                    _bank = controller.store.bank;
                    _page = controller.page;
                    WidgetsBinding.instance.addPostFrameCallback((_) {
                      if (mounted && _scroll.hasClients) _scroll.jumpTo(0);
                    });
                  }
                  final channels = controller.channels;
                  final railWidth = sideRail
                      ? MixerGeometry.railWidth(constraints.maxWidth)
                      : 0.0;
                  final stripWidth =
                      MixerGeometry.stripWidth(constraints.maxWidth, mobile);
                  Widget strip(MixerChannel ch) => MixerStrip(
                      key: ValueKey(ch.key),
                      channel: ch,
                      controller: controller,
                      compact: mobile,
                      bottomInset:
                          mobile ? MediaQuery.paddingOf(context).bottom : 0,
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
                            MixerToolbar(
                                controller: controller,
                                onNavigate: widget.onNavigate,
                                onHome: widget.onHome,
                                compact: constraints.maxWidth < 800,
                                showBankControls: mobile),
                            // Design bus bar: full-width strip in the
                            // selected bus colour between toolbar + strips.
                            Container(
                                height: mobile ? 6 : 8, color: MixerDesign.bus),
                            if (controller.error.isNotEmpty)
                              Material(
                                  color: const Color(0xff54221d),
                                  child: InkWell(
                                      onTap: () => widget
                                          .onNavigate(MixerDestination.connect),
                                      child: Padding(
                                          padding: const EdgeInsets.symmetric(
                                              horizontal: 12, vertical: 6),
                                          child: Row(children: [
                                            const Icon(Icons.info_outline,
                                                size: 16,
                                                color: MixerDesign.text),
                                            const SizedBox(width: 8),
                                            Expanded(
                                                child: Text(controller.error,
                                                    maxLines: 2,
                                                    overflow:
                                                        TextOverflow.ellipsis,
                                                    style:
                                                        MixerDesign.type(12)))
                                          ])))),
                            if (controller.hasSolo && !sideRail)
                              SizedBox(
                                  height: 34,
                                  child: Row(children: [
                                    const SizedBox(width: 10),
                                    Text('SOLO ${controller.store.soloCount}',
                                        style: MixerDesign.type(12,
                                            color: MixerDesign.solo)),
                                    const Spacer(),
                                    TextButton(
                                        onPressed: controller.store.clearSolos,
                                        child: Text('Clear solos',
                                            style: MixerDesign.type(12,
                                                color: MixerDesign.solo))),
                                  ])),
                            Expanded(
                                child: Row(
                                    crossAxisAlignment:
                                        CrossAxisAlignment.stretch,
                                    children: [
                                  Expanded(
                                      child: channels.isEmpty
                                          ? Center(
                                              child: Text(
                                                  'Waiting for channels…',
                                                  style: MixerDesign.type(16)))
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
