import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import 'banks.dart';

class MixerToolbar extends StatelessWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;
  final bool compact, showBankControls;
  const MixerToolbar(
      {super.key,
      required this.controller,
      required this.onNavigate,
      required this.compact,
      this.showBankControls = false});
  @override
  Widget build(BuildContext context) {
    final selected = controller.selected;
    final actions = <(String, String, MixerDestination)>[
      ('Effects', 'FX', MixerDestination.fx),
      ('Connection', 'connection', MixerDestination.connect),
      ('Settings', 'settings', MixerDestination.settings),
      ('Routing', 'routing', MixerDestination.routing),
      ('Snapshots', 'snapshots', MixerDestination.scenes),
      ('Meters', '≋', MixerDestination.meters),
      ('Debug', 'Debug', MixerDestination.debug),
    ];
    return Container(
        height: compact ? 64 : 68,
        padding:
            EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
        decoration: const BoxDecoration(
            color: MixerDesign.background,
            border: Border(bottom: BorderSide(color: MixerDesign.line))),
        child: Row(children: [
          Container(
              decoration: BoxDecoration(
                  color: const Color(0xff262626),
                  borderRadius: BorderRadius.circular(10)),
              child: Row(children: [
                for (final view in MixerView.values)
                  SizedBox(
                      width: compact ? 42 : 48,
                      height: 44,
                      child: IconButton(
                          tooltip: view == MixerView.processing
                              ? 'Processing view'
                              : 'Fader view',
                          isSelected: controller.view == view,
                          style: IconButton.styleFrom(
                              backgroundColor: controller.view == view
                                  ? const Color(0xff383838)
                                  : Colors.transparent,
                              shape: const RoundedRectangleBorder(
                                  borderRadius: MixerDesign.radius)),
                          onPressed: () => controller.setView(view),
                          icon: MixerArtwork(
                              'tab_${view == MixerView.processing ? 1 : 2}',
                              width: 36,
                              height: 32)))
              ])),
          const SizedBox(width: 8),
          Expanded(
              child: Semantics(
                  button: selected != null && !selected.master,
                  child: InkWell(
                      borderRadius: MixerDesign.radius,
                      onTap: selected == null || selected.master
                          ? null
                          : () => onNavigate(MixerDestination.channel),
                      child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          height: 52,
                          decoration: BoxDecoration(
                              color: MixerDesign.panel,
                              borderRadius: MixerDesign.radius,
                              border: Border.all(color: MixerDesign.line)),
                          child: Row(children: [
                            Expanded(
                                child: Column(
                                    mainAxisAlignment: MainAxisAlignment.center,
                                    crossAxisAlignment:
                                        CrossAxisAlignment.start,
                                    children: [
                                  Text(selected?.name ?? 'OpenMix',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: MixerDesign.type(compact ? 13 : 14,
                                          color: MixerDesign.mutedText)),
                                  Text(selected?.levelLabel ?? 'Connecting…',
                                      maxLines: 1,
                                      style: MixerDesign.type(compact ? 18 : 22,
                                          color: MixerDesign.lime,
                                          weight: FontWeight.w700)),
                                ])),
                            if (!compact)
                              Text('Fader',
                                  style: MixerDesign.type(22,
                                      color: MixerDesign.mutedText)),
                          ]))))),
          const SizedBox(width: 4),
          if (!compact) ...[
            for (final a in actions.take(5))
              MixerToolButton(
                  label: a.$1,
                  artwork: a.$2,
                  badge: a.$3 == MixerDestination.connect && controller.demo
                      ? 'DEMO'
                      : null,
                  onPressed: () => onNavigate(a.$3)),
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
          ] else
            MixerToolButton(
                label: controller.connectionLabel,
                artwork: 'connection',
                badge: controller.demo ? 'DEMO' : null,
                onPressed: () => onNavigate(MixerDestination.connect)),
          PopupMenuButton<VoidCallback>(
            tooltip: 'More mixer tools',
            color: const Color(0xff262626),
            icon: const Icon(Icons.more_horiz, color: MixerDesign.mutedText),
            onSelected: (action) => action(),
            itemBuilder: (_) => [
              if (showBankControls) ...[
                PopupMenuItem(
                    value: () => showMixerBanks(context, controller),
                    child:
                        Text('Channels & banks', style: MixerDesign.type(16))),
                PopupMenuItem(
                    value: () => showMixerBuses(context, controller),
                    child: Text('Master bus', style: MixerDesign.type(16))),
                if (controller.muteGroupCount > 0)
                  PopupMenuItem(
                      value: () => showMixerMuteGroups(context, controller),
                      child: Text('Mute groups', style: MixerDesign.type(16))),
                const PopupMenuDivider(),
              ],
              for (final a in compact ? actions : actions.skip(5))
                PopupMenuItem(
                    value: () => onNavigate(a.$3),
                    child: Text(a.$1, style: MixerDesign.type(16)))
            ],
          ),
        ]));
  }
}

class MixerToolButton extends StatelessWidget {
  final String label, artwork;
  final String? badge;
  final VoidCallback onPressed;
  const MixerToolButton(
      {super.key,
      required this.label,
      required this.artwork,
      this.badge,
      required this.onPressed});
  @override
  Widget build(BuildContext context) => SizedBox(
      width: 44,
      height: 44,
      child: IconButton(
          tooltip: label,
          padding: const EdgeInsets.all(2),
          onPressed: onPressed,
          icon: artwork == 'FX'
              ? Container(
                  width: 30,
                  height: 30,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      border:
                          Border.all(color: const Color(0xff666666), width: 2)),
                  child: Text('FX',
                      style:
                          MixerDesign.type(18, color: MixerDesign.mutedText)))
              : badge == null
                  ? MixerArtwork(artwork, width: 32, height: 32)
                  : SizedBox(
                      width: 32,
                      height: 38,
                      child: Column(children: [
                        MixerArtwork(artwork, width: 26, height: 26),
                        Text(badge!,
                            style: MixerDesign.type(8,
                                color: MixerDesign.amber,
                                weight: FontWeight.w700)),
                      ]))));
}
