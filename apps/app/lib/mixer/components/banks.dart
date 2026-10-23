import 'package:flutter/material.dart';
import '../controller.dart';
import '../design.dart';
import 'toggle.dart';

Future<void> showMixerLayout(BuildContext context, MixerController controller,
        ValueChanged<MixerDestination> onNavigate,
        {bool? locked, VoidCallback? onLock}) =>
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
                          child: Text('Layout',
                              style: MixerDesign.type(20,
                                  weight: FontWeight.w600))),
                      if (locked != null && onLock != null)
                        ListTile(
                            title: Text(
                                locked ? 'Unlock channel' : 'Lock channel',
                                style: MixerDesign.type(17)),
                            trailing: Icon(
                                locked
                                    ? Icons.lock_outline
                                    : Icons.lock_open_outlined,
                                color: MixerDesign.mutedText),
                            onTap: () {
                              Navigator.pop(context);
                              onLock();
                            }),
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
                      const Divider(color: MixerDesign.line),
                      ListTile(
                          title:
                              Text('Master bus', style: MixerDesign.type(17)),
                          subtitle: Text(
                              controller.store.busRows
                                  .where((b) => b.active)
                                  .map((b) => b.name)
                                  .join(', '),
                              style: MixerDesign.type(13,
                                  color: MixerDesign.mutedText)),
                          onTap: () {
                            Navigator.pop(context);
                            showMixerBuses(context, controller);
                          }),
                      for (final entry in <(String, MixerDestination)>[
                        ('Meter bridge', MixerDestination.meters),
                        ('Connection', MixerDestination.connect),
                        ('Debug', MixerDestination.debug),
                      ])
                        ListTile(
                            title: Text(entry.$1, style: MixerDesign.type(17)),
                            onTap: () {
                              Navigator.pop(context);
                              onNavigate(entry.$2);
                            }),
                    ]))));

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
            ? LayoutBuilder(builder: (context, box) {
                final sx = box.maxWidth / 93;
                final sy = box.maxHeight / 1259;
                // Keep the authored visual scale separate from the hit target.
                // Short viewports scroll rather than compressing/overlapping rows.
                double h(double value) =>
                    (value * sy).clamp(44.0, double.infinity);
                Widget fittedLabel(String text, double size,
                        {Color color = MixerDesign.text,
                        FontWeight weight = FontWeight.w400}) =>
                    FittedBox(
                        fit: BoxFit.scaleDown,
                        child: Text(text,
                            maxLines: 1,
                            softWrap: false,
                            style: MixerDesign.type(size * sx,
                                color: color, weight: weight)));
                Widget title(String text, double height, {double size = 25}) =>
                    SizedBox(
                        height: height * sy,
                        child: Padding(
                            padding: EdgeInsets.symmetric(horizontal: 4 * sx),
                            child: Center(child: fittedLabel(text, size))));
                Widget button(String label, VoidCallback? onTap,
                        {bool active = false,
                        double size = 30,
                        double height = 43,
                        Color? color,
                        FontWeight weight = FontWeight.w400}) =>
                    SizedBox(
                        height: h(height),
                        width: double.infinity,
                        child: Semantics(
                            selected: active,
                            child: TextButton(
                                onPressed: onTap,
                                style: TextButton.styleFrom(
                                    // Override the app's filled, rounded buttons:
                                    // the reference rail has flat text rows.
                                    backgroundColor: Colors.transparent,
                                    shape: const RoundedRectangleBorder(),
                                    padding: EdgeInsets.symmetric(
                                        horizontal: 4 * sx),
                                    minimumSize: const Size(44, 44),
                                    tapTargetSize:
                                        MaterialTapTargetSize.shrinkWrap,
                                    visualDensity: VisualDensity.standard),
                                child: fittedLabel(label, size,
                                    color: active
                                        ? MixerDesign.lime
                                        : color ?? MixerDesign.mutedText,
                                    weight: weight))));
                return SingleChildScrollView(
                    child: Column(children: [
                  title('MUTE', 58),
                  Container(
                      constraints: BoxConstraints(minHeight: 256 * sy),
                      color: MixerDesign.background,
                      child: Column(children: [
                        for (var g = 0; g < count; g++)
                          button(
                              '${g + 1}', () => controller.toggleMuteGroup(g),
                              active: controller.store.engine.muteGroups[g] ==
                                  true),
                      ])),
                  title('MIX', 49),
                  SizedBox(
                      height:
                          (149 * sy).clamp(59 * sy + h(54), double.infinity),
                      width: double.infinity,
                      child: ColoredBox(
                          color: MixerDesign.background,
                          child: Stack(children: [
                            Positioned(
                                left: 7 * sx,
                                top: 8 * sy,
                                right: 7 * sx,
                                child: Align(
                                    alignment: Alignment.centerLeft,
                                    child: fittedLabel('LR', 16,
                                        color: MixerDesign.bus,
                                        weight: FontWeight.w700))),
                            Positioned(
                                top: 59 * sy,
                                left: 0,
                                right: 0,
                                child: button(controller.master.name,
                                    () => showMixerBuses(context, controller),
                                    size: 36,
                                    height: 54,
                                    color: MixerDesign.text,
                                    weight: FontWeight.w600)),
                          ]))),
                  Container(
                      width: double.infinity,
                      constraints: BoxConstraints(minHeight: 360 * sy),
                      padding: EdgeInsets.symmetric(vertical: 6 * sy),
                      decoration: BoxDecoration(
                          color: MixerDesign.background,
                          border: Border.all(color: MixerDesign.cap, width: 2),
                          borderRadius: BorderRadius.circular(14 * sx)),
                      child: Column(children: [
                        for (final bank in banks)
                          button(bank.label, () => controller.setBank(bank.key),
                              active: bank.key == controller.store.bank,
                              size: 31),
                      ])),
                  for (final bus in controller.store.busRows)
                    button(bus.name, () => controller.store.selectBus(bus.id),
                        active: bus.active, size: 31),
                  if (controller.pageCount > 1) ...[
                    button(
                        '‹',
                        controller.page > 0
                            ? () => controller.nextPage(-1)
                            : null),
                    title(
                        '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                        24,
                        size: 16),
                    button(
                        '›',
                        controller.page + 1 < controller.pageCount
                            ? () => controller.nextPage(1)
                            : null),
                  ],
                  if (controller.hasSolo)
                    button('Clear solos', controller.store.clearSolos,
                        size: 18),
                  title('MASTERS', 31, size: 16),
                ]));
              })
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
