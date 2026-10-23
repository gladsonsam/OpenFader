import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../controller.dart';
import '../design.dart';
import '../../u4law.dart';
import '../../widgets/number_entry.dart';
import 'banks.dart';

class MixerToolbar extends StatelessWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Back to the Connections start screen (top-left home button).
  final VoidCallback? onHome;
  final bool compact, showBankControls;
  const MixerToolbar(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome,
      required this.compact,
      this.showBankControls = false});
  @override
  Widget build(BuildContext context) {
    final selected = controller.selected;
    // Phone menu + layout sheet destinations (the connection shortcut was
    // removed from the toolbar; the old Link screen lives here).
    final actions = <(String, MixerDestination)>[
      ('Effects', MixerDestination.fx),
      ('Settings', MixerDestination.settings),
      ('Routing', MixerDestination.routing),
      ('Meter bridge', MixerDestination.meters),
      ('Snapshots', MixerDestination.scenes),
      ('Connection', MixerDestination.connect),
      ('Debug', MixerDestination.debug),
    ];
    return Container(
        height: compact ? 64 : 68,
        padding:
            EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
        decoration: const BoxDecoration(
            color: MixerDesign.background,
            border: Border(bottom: BorderSide(color: MixerDesign.line))),
        child: Row(children: [
          // Brand block: approved logo (symbol + name tablet, symbol phone)
          // returning to Connections. Replaces the old empty slot / home
          // icon and the deleted view tabs.
          _BrandButton(
              compact: compact,
              onHome: onHome,
              trailingGap: compact ? 6.0 : 0.0),
          if (!compact) const Expanded(flex: 140, child: SizedBox()),
          Expanded(
              flex: compact ? 1 : 892,
              child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  height: 52,
                  decoration: BoxDecoration(
                      color: MixerDesign.panel,
                      borderRadius: MixerDesign.radius,
                      border: Border.all(color: MixerDesign.line)),
                  child: Row(children: [
                    // Active-strip labels: tapping opens the channel page.
                    if (!compact &&
                        selected != null &&
                        !selected.master)
                      Expanded(
                          child: GestureDetector(
                              onTap: () =>
                                  onNavigate(MixerDestination.channel),
                              behavior: HitTestBehavior.opaque,
                              child: Column(
                                  mainAxisAlignment:
                                      MainAxisAlignment.center,
                                  crossAxisAlignment:
                                      CrossAxisAlignment.start,
                                  children: [
                                Text('Ch ${selected.number}',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: MixerDesign.type(14,
                                        color: MixerDesign.mutedText)),
                                Text(selected.name,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: MixerDesign.type(14,
                                        color: MixerDesign.mutedText)),
                              ])))
                    else if (compact)
                      Expanded(
                          child: GestureDetector(
                              onTap: selected == null || selected.master
                                  ? null
                                  : () => onNavigate(
                                      MixerDestination.channel),
                              behavior: HitTestBehavior.opaque,
                              child: Text(
                                  selected == null
                                      ? 'OpenFader'
                                      : selected.master
                                          ? selected.name
                                          : 'Ch ${selected.number} · ${selected.name}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: MixerDesign.type(12,
                                      color: MixerDesign.mutedText)))),
                    // Live fader readout: tapping types a dB value.
                    // Listens to the selected strip's fader notifier so
                    // it tracks drags without a page rebuild.
                    Expanded(
                        child: Center(
                            child: _SelectedLevel(
                                controller: controller,
                                compact: compact,
                                onEdit: selected == null
                                    ? null
                                    : () => _typeFader(
                                        context, controller)))),
                    if (!compact)
                      Padding(
                          padding: const EdgeInsets.only(left: 12),
                          child: Text('Fader',
                              style: MixerDesign.type(22,
                                  color: MixerDesign.mutedText))),
                  ]))),
          const SizedBox(width: 4),
          if (!compact) ...[
            const SizedBox(width: 4),
            MixerToolButton(
                label: 'Effects',
                onPressed: () =>
                    onNavigate(MixerDestination.fx),
                child: Text('FX',
                    style: MixerDesign.type(20,
                        color: const Color(0xffb5b5b5),
                        weight: FontWeight.w600))),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Settings',
                onPressed: () =>
                    onNavigate(MixerDestination.settings),
                child: const Icon(Icons.settings_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Routing',
                onPressed: () =>
                    onNavigate(MixerDestination.routing),
                child: const Icon(Icons.account_tree_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Layout',
                onPressed: () =>
                    showMixerLayout(context, controller, onNavigate),
                child: const Icon(Icons.view_column_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
            Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                child: SizedBox(
                    height: 44,
                    child: OutlinedButton(
                        style: OutlinedButton.styleFrom(
                            foregroundColor: MixerDesign.lime,
                            backgroundColor: const Color(0xff262626),
                            side: const BorderSide(color: Color(0xff3a3a3a)),
                            shape: const RoundedRectangleBorder(
                                borderRadius: MixerDesign.radius)),
                        onPressed: () => onNavigate(MixerDestination.scenes),
                        child: const Text('Shows')))),
          ],
          if (compact)
            Padding(
              padding: const EdgeInsets.only(right: 10),
              child: _MenuButton(
                onSelected: (action) => action(),
                items: [
                  if (onHome != null)
                    PopupMenuItem(
                        value: onHome!,
                        child: Text('Connections',
                            style: MixerDesign.type(16))),
                  for (final a in actions)
                    PopupMenuItem(
                        value: () => onNavigate(a.$2),
                        child: Text(a.$1, style: MixerDesign.type(16))),
                  if (showBankControls) ...[
                    const PopupMenuDivider(),
                    PopupMenuItem(
                        value: () => showMixerBanks(context, controller),
                        child: Text('Channels & banks',
                            style: MixerDesign.type(16))),
                    PopupMenuItem(
                        value: () => showMixerBuses(context, controller),
                        child: Text('Master bus',
                            style: MixerDesign.type(16))),
                    if (controller.muteGroupCount > 0)
                      PopupMenuItem(
                          value: () =>
                              showMixerMuteGroups(context, controller),
                          child: Text('Mute groups',
                              style: MixerDesign.type(16))),
                  ],
                ],
              ),
            ),
        ]));
  }
}

/// Selected-strip dB readout. Listens to the selected strip's fader
/// notifier (not the page rebuild) so it tracks the active drag.
/// Tapping types a dB value via [onEdit].
class _SelectedLevel extends StatelessWidget {
  final MixerController controller;
  final bool compact;
  final VoidCallback? onEdit;
  const _SelectedLevel(
      {required this.controller, required this.compact, this.onEdit});
  @override
  Widget build(BuildContext context) {
    final selected = controller.selected;
    if (selected == null) {
      return Text('Connecting…',
          maxLines: 1,
          style: MixerDesign.type(compact ? 18 : 22,
              color: MixerDesign.lime, weight: FontWeight.w700));
    }
    return ValueListenableBuilder<double>(
        valueListenable:
            controller.faderNoteOf(selected.channel, selected.master),
        builder: (context, fader, _) => Semantics(
            button: true,
            label:
                '${selected.master ? 'Main' : selected.name} fader, ${fmtDb(fader)} dB, activate to type a value',
            onTap: onEdit,
            child: GestureDetector(
                onTap: onEdit,
                behavior: HitTestBehavior.opaque,
                child: Text('${fmtDb(fader)} dB',
                    maxLines: 1,
                    style: MixerDesign.type(compact ? 18 : 22,
                        color: MixerDesign.lime,
                        weight: FontWeight.w700)))));
  }
}

/// Typed fader entry: parse dB (or -inf) and move the selected strip there.
Future<void> _typeFader(
    BuildContext context, MixerController controller) async {
  final selected = controller.selected;
  if (selected == null) return;
  final v = await showNumberEntry(
      context: context,
      title: '${selected.master ? 'Main' : 'Ch ${selected.number}'} fader',
      initial: fmtDb(controller.faderNoteOf(selected.channel, selected.master)
          .value),
      min: -60,
      max: 10,
      allowNegInf: true,
      negInfValue: double.negativeInfinity);
  if (v == null) return;
  controller.setPosition(
      selected, v.isFinite ? MixerFaderScale.positionForDb(v) : 0);
}


/// Phone menu: explicit semantics button opening a menu sheet.
class _MenuButton extends StatelessWidget {
  final ValueChanged<VoidCallback> onSelected;
  final List<PopupMenuEntry<VoidCallback>> items;
  const _MenuButton({required this.onSelected, required this.items});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: 'Menu',
      onTap: () => _open(context),
      child: Tooltip(
          message: 'Menu',
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: () => _open(context),
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: const Color(0xff262626),
                          borderRadius: BorderRadius.circular(10),
                          border:
                              Border.all(color: const Color(0xff363636))),
                      child: const Icon(Icons.menu,
                          size: 22, color: Color(0xffb5b5b5)))))));

  Future<void> _open(BuildContext context) async {
    final box = context.findRenderObject() as RenderBox?;
    final overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox?;
    final choice = await showMenu<VoidCallback>(
      context: context,
      color: const Color(0xff262626),
      position: RelativeRect.fromRect(
          box == null ? Rect.zero : Offset.zero & box.size,
          Offset.zero & (overlay?.size ?? Size.zero)),
      items: items,
    );
    if (choice != null) onSelected(choice);
  }
}

/// Approved 44px toolbar button: charcoal fill, hairline edge, 10px radius,
/// 22px icon or FX text in #b5b5b5.
class MixerToolButton extends StatelessWidget {
  final String label;
  final Widget child;
  final VoidCallback onPressed;
  const MixerToolButton(
      {super.key,
      required this.label,
      required this.child,
      required this.onPressed});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: label,
      child: Tooltip(
          message: label,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onPressed,
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: const Color(0xff262626),
                          borderRadius: BorderRadius.circular(10),
                          border:
                              Border.all(color: const Color(0xff363636))),
                      child: child)))));
}

/// Approved brand block (top-left): logo symbol + name on tablet, symbol
/// only on phone. Tapping returns to Connections.
class _BrandButton extends StatelessWidget {
  final bool compact;
  final VoidCallback? onHome;
  final double trailingGap;
  const _BrandButton(
      {required this.compact,
      required this.onHome,
      required this.trailingGap});
  @override
  Widget build(BuildContext context) {
    final logo = SizedBox(
        width: compact ? 44 : 52,
        height: compact ? 44 : 52,
        child: SvgPicture.asset('assets/brand/open-fader-logo.svg',
            semanticsLabel: 'OpenFader'));
    final body = compact
        ? logo
        : Row(mainAxisSize: MainAxisSize.min, children: [
            logo,
            const SizedBox(width: 14),
            Text('OpenFader',
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: MixerDesign.type(28, weight: FontWeight.w600)),
          ]);
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Semantics(
          button: true,
          label: 'OpenFader — Connections',
          onTap: onHome,
          child: Tooltip(
              message: 'Connections',
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onHome,
                  child: Padding(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 4, vertical: 4),
                      child: body)))),
      SizedBox(width: trailingGap),
    ]);
  }
}
