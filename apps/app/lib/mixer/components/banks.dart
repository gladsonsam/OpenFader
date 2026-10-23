import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import 'toggle.dart';

Future<void> showMixerBuses(BuildContext context, MixerController controller) =>
    showModalBottomSheet<void>(
        context: context,
        backgroundColor: MixerDesign.background,
        showDragHandle: true,
        builder: (context) => SafeArea(
            child: ListenableBuilder(
                listenable: controller,
                builder: (context, _) => ListView(shrinkWrap: true, children: [
                      ListTile(
                          title: Text('Master bus',
                              style: MixerDesign.type(20,
                                  weight: FontWeight.w600))),
                      for (final bus in controller.store.busRows)
                        ListTile(
                            title: Text(bus.name, style: MixerDesign.type(17)),
                            subtitle: Text(bus.sub,
                                style: MixerDesign.type(13,
                                    color: MixerDesign.mutedText)),
                            trailing: bus.active
                                ? const Icon(Icons.check,
                                    color: MixerDesign.lime)
                                : null,
                            onTap: () {
                              controller.store.selectBus(bus.id);
                              Navigator.pop(context);
                            }),
                    ]))));

class MixerBanks extends StatelessWidget {
  final MixerController controller;
  final bool vertical;
  const MixerBanks(
      {super.key, required this.controller, required this.vertical});
  Widget _pager() =>
      Row(mainAxisAlignment: MainAxisAlignment.center, children: [
        Expanded(
            child: IconButton(
                tooltip: 'Previous channels',
                onPressed:
                    controller.page > 0 ? () => controller.nextPage(-1) : null,
                icon: const Icon(Icons.chevron_left))),
        Text('${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
            style: MixerDesign.type(11)),
        Expanded(
            child: IconButton(
                tooltip: 'Next channels',
                onPressed: controller.page + 1 < controller.pageCount
                    ? () => controller.nextPage(1)
                    : null,
                icon: const Icon(Icons.chevron_right))),
      ]);
  @override
  Widget build(BuildContext context) {
    final banks = controller.store.banks;
    final count = controller.muteGroupCount;
    final status = Text(controller.connectionLabel.toUpperCase(),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: MixerDesign.type(10,
            color: controller.demo ? MixerDesign.amber : MixerDesign.lime,
            weight: FontWeight.w600));
    return Material(
        color: const Color(0xff141414),
        child: vertical
            ? SizedBox(
                width: 96,
                child: ListView(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 4, vertical: 8),
                    children: [
                      Center(child: status),
                      const SizedBox(height: 12),
                      if (count > 0) ...[
                        Center(
                            child: Text('MUTE', style: MixerDesign.type(14))),
                        for (var g = 0; g < count; g++) ...[
                          SizedBox(
                              height: 34,
                              child: TextButton(
                                  style: TextButton.styleFrom(
                                      foregroundColor: controller
                                                  .store.engine.muteGroups[g] ==
                                              true
                                          ? MixerDesign.lime
                                          : MixerDesign.mutedText),
                                  onPressed: () =>
                                      controller.toggleMuteGroup(g),
                                  child: Text('${g + 1}'))),
                          if (g < count - 1) const SizedBox(height: 6),
                        ],
                        const SizedBox(height: 10),
                      ],
                      Center(child: Text('MIX', style: MixerDesign.type(14))),
                      TextButton(
                          onPressed: () => showMixerBuses(context, controller),
                          child: Text(controller.master.name,
                              style: MixerDesign.type(20,
                                  weight: FontWeight.w600))),
                      const Divider(color: MixerDesign.line),
                      for (final bank in banks)
                        Padding(
                            padding: const EdgeInsets.symmetric(vertical: 2),
                            child: TextButton(
                                style: TextButton.styleFrom(
                                    padding: const EdgeInsets.symmetric(
                                        horizontal: 2),
                                    minimumSize: const Size(44, 40),
                                    backgroundColor:
                                        bank.key == controller.store.bank
                                            ? MixerDesign.selected
                                            : null,
                                    foregroundColor:
                                        bank.key == controller.store.bank
                                            ? MixerDesign.lime
                                            : MixerDesign.mutedText,
                                    shape: const RoundedRectangleBorder(
                                        borderRadius: MixerDesign.radius)),
                                onPressed: () => controller.setBank(bank.key),
                                child: Text(bank.label,
                                    textAlign: TextAlign.center,
                                    style: const TextStyle(
                                        fontFamily: MixerDesign.font,
                                        fontSize: 13)))),
                      _pager(),
                      if (controller.hasSolo)
                        TextButton(
                            onPressed: controller.store.clearSolos,
                            child: Text('Clear solos',
                                style: MixerDesign.type(12,
                                    color: MixerDesign.solo))),
                    ]))
            : SizedBox(
                height: 54,
                child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: Row(children: [
                      Expanded(
                          child: DropdownButtonHideUnderline(
                              child: DropdownButton<String>(
                                  isExpanded: true,
                                  dropdownColor: const Color(0xff262626),
                                  value: banks.any(
                                          (b) => b.key == controller.store.bank)
                                      ? controller.store.bank
                                      : null,
                                  hint: status,
                                  style: MixerDesign.type(14),
                                  items: [
                                    for (final b in banks)
                                      DropdownMenuItem(
                                          value: b.key,
                                          child: Text(b.label,
                                              overflow: TextOverflow.ellipsis))
                                  ],
                                  onChanged: (key) {
                                    if (key != null) controller.setBank(key);
                                  }))),
                      SizedBox(width: 114, child: _pager()),
                      if (count > 0)
                        IconButton(
                            tooltip: 'Mute groups',
                            icon:
                                const Icon(Icons.volume_off_outlined, size: 20),
                            onPressed: () =>
                                showMixerMuteGroups(context, controller)),
                    ]))));
  }
}

Future<void> showMixerMuteGroups(
        BuildContext context, MixerController controller) =>
    showModalBottomSheet<void>(
        context: context,
        backgroundColor: MixerDesign.background,
        showDragHandle: true,
        builder: (context) => SafeArea(
            child: ListenableBuilder(
                listenable: controller,
                builder: (context, _) => Padding(
                    padding: const EdgeInsets.all(16),
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      Text('Mute groups', style: MixerDesign.type(20)),
                      const SizedBox(height: 16),
                      Wrap(spacing: 8, runSpacing: 8, children: [
                        for (var g = 0; g < controller.muteGroupCount; g++)
                          SizedBox(
                              width: 88,
                              child: MixerToggle(
                                  label: '${g + 1}',
                                  channelName: 'Mute group',
                                  on: controller.store.engine.muteGroups[g] ??
                                      false,
                                  activeColor: MixerDesign.mute,
                                  onPressed: () =>
                                      controller.toggleMuteGroup(g)))
                      ]),
                    ])))));

Future<void> showMixerBanks(BuildContext context, MixerController controller) =>
    showModalBottomSheet<void>(
        context: context,
        backgroundColor: MixerDesign.background,
        showDragHandle: true,
        builder: (context) => SafeArea(
            child: ListenableBuilder(
                listenable: controller,
                builder: (context, _) => ListView(shrinkWrap: true, children: [
                      Padding(
                          padding: const EdgeInsets.symmetric(horizontal: 16),
                          child: Row(children: [
                            Expanded(
                                child: Text('Channels & banks',
                                    style: MixerDesign.type(20))),
                            IconButton(
                                tooltip: 'Previous channels',
                                onPressed: controller.page > 0
                                    ? () => controller.nextPage(-1)
                                    : null,
                                icon: const Icon(Icons.chevron_left)),
                            Text(
                                '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                                style: MixerDesign.type(13)),
                            IconButton(
                                tooltip: 'Next channels',
                                onPressed:
                                    controller.page + 1 < controller.pageCount
                                        ? () => controller.nextPage(1)
                                        : null,
                                icon: const Icon(Icons.chevron_right)),
                          ])),
                      for (final bank in controller.store.banks)
                        ListTile(
                            title:
                                Text(bank.label, style: MixerDesign.type(17)),
                            trailing: bank.key == controller.store.bank
                                ? const Icon(Icons.check,
                                    color: MixerDesign.lime)
                                : null,
                            onTap: () {
                              controller.setBank(bank.key);
                              Navigator.pop(context);
                            }),
                    ]))));
