import '../design.dart';
import '../../ui/console.dart';
import 'package:flutter/material.dart';
import '../controller.dart';
import 'layers.dart';
import '../store.dart';
import '../../settings/prefs.dart';
import 'toggle.dart';

Future<void> showMixerLayout(BuildContext context, MixerController controller,
    ValueChanged<MixerDestination> onNavigate,
    {bool? locked, VoidCallback? onLock}) async {
  final action = await showAnchoredMenu<VoidCallback>(context, [
    if (locked != null && onLock != null)
      PopupMenuItem(
          height: 48,
          value: onLock,
          child: Text(locked ? 'Unlock channel' : 'Lock channel')),
    for (final bank in controller.store.banks)
      PopupMenuItem(
          height: 48,
          value: () => controller.setBank(bank.key),
          child: Row(children: [
            Expanded(child: Text(bank.label)),
            if (bank.key == controller.store.bank)
              Icon(Icons.check,
                  color: ConsolePalette.of(context).cyan, size: 18),
          ])),
    const PopupMenuDivider(),
    PopupMenuItem(
        height: 48,
        value: () => showMixerLayers(context, controller),
        child: const Text('Layers')),
    PopupMenuItem(
        height: 48,
        value: () => showMixerGroupMembership(context, controller),
        child: const Text('Group members')),
    PopupMenuItem(
        height: 48,
        value: () => showMixerBuses(context, controller),
        child: const Text('Master bus')),
    if (controller.muteGroupCount > 0)
      PopupMenuItem(
          height: 48,
          value: () => showMixerMuteGroups(context, controller),
          child: const Text('Mute groups')),
  ]);
  if (context.mounted) action?.call();
}

Future<void> showMixerBuses(
    BuildContext context, MixerController controller) async {
  final selected = await showAnchoredMenu<String>(context, [
    for (final bus in controller.store.busRows)
      PopupMenuItem(
          value: bus.id,
          height: 56,
          child: Row(children: [
            Expanded(
                child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(bus.name, style: mixerType(context, 16)),
                  Text(bus.sub,
                      style: mixerType(context, 12,
                          color: ConsolePalette.of(context).mutedText)),
                ])),
            if (bus.active)
              Icon(Icons.check,
                  color: ConsolePalette.of(context).lime, size: 20),
          ])),
  ]);
  if (context.mounted &&
      selected != null &&
      controller.store.busRows.any((bus) => bus.id == selected)) {
    controller.store.selectBus(selected);
  }
}

class MixerBanks extends StatelessWidget {
  final MixerController controller;
  final bool vertical;
  const MixerBanks(
      {super.key, required this.controller, required this.vertical});
  Widget _pager(BuildContext context) =>
      Row(mainAxisAlignment: MainAxisAlignment.center, children: [
        Expanded(
            child: IconButton(
                tooltip: 'Previous channels',
                onPressed:
                    controller.page > 0 ? () => controller.nextPage(-1) : null,
                icon: const Icon(Icons.chevron_left))),
        Text('${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
            style: mixerType(context, 11)),
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
    final layers = PrefsService.instance.mixerLayers;
    final count = controller.muteGroupCount;
    final status = Text(controller.connectionLabel.toUpperCase(),
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: mixerType(context, 10,
            color: controller.demo
                ? ConsolePalette.of(context).amber
                : ConsolePalette.of(context).lime,
            weight: FontWeight.w600));
    return Material(
        color: ConsolePalette.of(context).panel,
        child: vertical
            ? LayoutBuilder(builder: (context, box) {
                final sx = box.maxWidth / 93;
                final sy = box.maxHeight / 1259;
                // Keep the authored visual scale separate from the hit target.
                // Short viewports scroll rather than compressing/overlapping rows.
                double h(double value) =>
                    (value * sy).clamp(44.0, double.infinity);
                Widget fittedLabel(String text, double size,
                        {Color? color, FontWeight weight = FontWeight.w400}) =>
                    FittedBox(
                        fit: BoxFit.scaleDown,
                        child: Text(text,
                            maxLines: 1,
                            softWrap: false,
                            style: mixerType(context, size * sx,
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
                                        ? ConsolePalette.of(context).lime
                                        : color ??
                                            ConsolePalette.of(context)
                                                .mutedText,
                                    weight: weight))));
                return Column(children: [
                  Expanded(
                      child: Scrollbar(
                          child: SingleChildScrollView(
                              child: Column(children: [
                    title('MUTE', 58),
                    Container(
                        constraints: BoxConstraints(minHeight: 256 * sy),
                        color: ConsolePalette.of(context).background,
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
                            color: ConsolePalette.of(context).background,
                            child: Stack(children: [
                              Positioned(
                                  left: 7 * sx,
                                  top: 8 * sy,
                                  right: 7 * sx,
                                  child: Align(
                                      alignment: Alignment.centerLeft,
                                      child: fittedLabel(
                                          controller.master.number, 16,
                                          color: controller.masterColor(
                                              ConsolePalette.of(context)),
                                          weight: FontWeight.w700))),
                              Positioned(
                                  top: 59 * sy,
                                  left: 0,
                                  right: 0,
                                  child: Builder(
                                      builder: (anchor) => button(
                                          controller.master.name,
                                          () => showMixerBuses(
                                              anchor, controller),
                                          size: 36,
                                          height: 54,
                                          color:
                                              ConsolePalette.of(context).text,
                                          weight: FontWeight.w600))),
                            ]))),
                    title('OUTPUTS', 40, size: 20),
                    Container(
                        width: double.infinity,
                        padding: EdgeInsets.symmetric(vertical: 8 * sy),
                        decoration: BoxDecoration(
                            color: ConsolePalette.of(context).background,
                            border: Border.all(
                                color: ConsolePalette.of(context).cap,
                                width: 2),
                            borderRadius: BorderRadius.circular(14 * sx)),
                        child: Column(children: [
                          for (final bus in controller.store.busRows)
                            button(bus.name,
                                () => controller.store.selectBus(bus.id),
                                active: bus.active, size: 28, height: 56),
                        ])),
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
                    title('MASTERS', 31, size: 16),
                  ])))),
                  // Keep monitoring recovery reachable even when the provider
                  // exposes enough banks/buses to fill the scrolling rail.
                  if (controller.hasSolo)
                    button(
                        'Clear solos',
                        controller.store.canClearSolos
                            ? controller.store.clearSolos
                            : null,
                        size: 18,
                        color: ConsolePalette.of(context).solo),
                ]);
              })
            : SizedBox(
                height: 54,
                child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 8),
                    child: Row(children: [
                      Expanded(
                          // Long-press opens the custom layers editor.
                          child: GestureDetector(
                              onLongPress: () =>
                                  showMixerLayers(context, controller),
                              child: DropdownButtonHideUnderline(
                                  child: DropdownButton<String>(
                                      isExpanded: true,
                                      dropdownColor:
                                          ConsolePalette.of(context).chip,
                                      value: controller.isCustomLayerActive
                                          ? 'layer:${controller.activeLayer.id}'
                                          : banks.any((b) =>
                                                  b.key ==
                                                  controller.store.bank)
                                              ? controller.store.bank
                                              : null,
                                      hint: status,
                                      style: mixerType(context, 14),
                                      items: [
                                        for (final b in banks)
                                          DropdownMenuItem(
                                              value: b.key,
                                              child: Text(b.label,
                                                  overflow:
                                                      TextOverflow.ellipsis)),
                                        for (final layer in layers)
                                          DropdownMenuItem(
                                              value: 'layer:${layer.id}',
                                              child: Text(layer.name,
                                                  overflow:
                                                      TextOverflow.ellipsis))
                                      ],
                                      onChanged: (key) {
                                        if (key == null) return;
                                        if (key.startsWith('layer:')) {
                                          controller
                                              .setActiveLayer(key.substring(6));
                                        } else {
                                          controller.setBank(key);
                                        }
                                      })))),
                      SizedBox(width: 114, child: _pager(context)),
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
        sheetAnimationStyle: AnimationStyle.noAnimation,
        context: context,
        backgroundColor: ConsolePalette.of(context).background,
        showDragHandle: true,
        builder: (context) => SafeArea(
            child: ListenableBuilder(
                listenable: controller,
                builder: (context, _) => Padding(
                    padding: const EdgeInsets.all(16),
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      Text('Mute groups', style: mixerType(context, 20)),
                      TextButton(
                          onPressed: () =>
                              showMixerGroupMembership(context, controller),
                          child: const Text('Edit group members')),
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
                                  activeColor: ConsolePalette.of(context).mute,
                                  activeForeground:
                                      ConsolePalette.of(context).onDanger,
                                  onPressed: () =>
                                      controller.toggleMuteGroup(g)))
                      ]),
                    ])))));

Future<void> showMixerBanks(BuildContext context, MixerController controller) =>
    showModalBottomSheet<void>(
        sheetAnimationStyle: AnimationStyle.noAnimation,
        context: context,
        backgroundColor: ConsolePalette.of(context).background,
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
                                    style: mixerType(context, 20))),
                            IconButton(
                                tooltip: 'Previous channels',
                                onPressed: controller.page > 0
                                    ? () => controller.nextPage(-1)
                                    : null,
                                icon: const Icon(Icons.chevron_left)),
                            Text(
                                '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                                style: mixerType(context, 13)),
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
                                Text(bank.label, style: mixerType(context, 17)),
                            trailing: bank.key == controller.store.bank
                                ? Icon(Icons.check,
                                    color: ConsolePalette.of(context).lime)
                                : null,
                            onTap: () {
                              controller.setBank(bank.key);
                              Navigator.pop(context);
                            }),
                      Divider(color: ConsolePalette.of(context).line),
                      ListTile(
                          title: Text('Layers', style: mixerType(context, 17)),
                          subtitle: Text('Custom channel sets',
                              style: mixerType(context, 13,
                                  color: ConsolePalette.of(context).mutedText)),
                          onTap: () => showMixerLayers(context, controller)),
                    ]))));

Future<void> showCustomMixerViews(
        BuildContext context, MixerController controller) =>
    showModalBottomSheet<void>(
      sheetAnimationStyle: AnimationStyle.noAnimation,
      context: context,
      backgroundColor: ConsolePalette.of(context).background,
      showDragHandle: true,
      builder: (context) => SafeArea(
          child: ListenableBuilder(
        listenable: controller,
        builder: (context, _) => ListView(shrinkWrap: true, children: [
          ListTile(
              title: Text('Custom views',
                  style: mixerType(context, 20, weight: FontWeight.w600))),
          ListTile(
              leading: const Icon(Icons.add),
              title: const Text('Create view'),
              onTap: () => _editCustomView(context, controller)),
          for (final view in controller.store.customMixerViews)
            ListTile(
                title: Text(view.name),
                subtitle: Text('${view.stripKeys.length} channels'),
                trailing: const Icon(Icons.edit_outlined),
                onTap: () => _editCustomView(context, controller, view)),
        ]),
      )),
    );

Future<void> _editCustomView(BuildContext context, MixerController controller,
    [CustomMixerView? existing]) async {
  final engine = controller.store.engine;
  final scope = (engine.driver, engine.model, engine.linkIp);
  final strips = engine.strips.where((s) => s.key != 'main').toList();
  final name = TextEditingController(text: existing?.name ?? '');
  final selected = <String>{...?existing?.stripKeys};
  final route = DialogRoute<String>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => StatefulBuilder(
          builder: (context, setDialogState) => AlertDialog(
                backgroundColor: ConsolePalette.of(context).background,
                title: Text(existing == null ? 'Create view' : 'Edit view'),
                content: SizedBox(
                    width: 480,
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      TextField(
                          controller: name,
                          maxLength: 32,
                          decoration:
                              const InputDecoration(labelText: 'View name')),
                      Flexible(
                          child: ListView(shrinkWrap: true, children: [
                        for (final strip in strips)
                          CheckboxListTile(
                            title: Text(
                                strip.name.isEmpty ? strip.key : strip.name),
                            subtitle: Text(strip.key),
                            value: selected.contains(strip.key),
                            onChanged: (value) => setDialogState(() {
                              if (value == true) {
                                selected.add(strip.key);
                              } else {
                                selected.remove(strip.key);
                              }
                            }),
                          ),
                      ])),
                    ])),
                actions: [
                  if (existing != null)
                    TextButton(
                        onPressed: () => Navigator.pop(context, 'delete'),
                        child: const Text('Delete')),
                  TextButton(
                      onPressed: () => Navigator.pop(context),
                      child: const Text('Cancel')),
                  TextButton(
                      onPressed: () {
                        if (name.text.trim().isNotEmpty) {
                          Navigator.pop(context, 'save');
                        }
                      },
                      child: const Text('Save')),
                ],
              )));
  final result = await Navigator.of(context).push(route);
  final viewName = name.text.trim();
  await route.completed;
  name.dispose();
  if (result == null || !context.mounted) return;
  if ((engine.driver, engine.model, engine.linkIp) != scope) {
    ScaffoldMessenger.of(context).showSnackBar(const SnackBar(
        content: Text('Mixer changed. Open custom views again.')));
    return;
  }
  final views = controller.store.customMixerViews
      .where((v) => v.id != existing?.id)
      .toList();
  if (result == 'save') {
    final view = CustomMixerView(
        id: existing?.id ?? DateTime.now().microsecondsSinceEpoch.toString(),
        name: viewName,
        stripKeys: selected);
    if (existing == null) {
      views.add(view);
    } else {
      final index = controller.store.customMixerViews
          .indexWhere((v) => v.id == existing.id);
      views.insert(index.clamp(0, views.length), view);
    }
  }
  try {
    await controller.store.saveCustomMixerViews(views);
  } catch (error) {
    if (context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text('Could not save view: $error')));
    }
  }
}

Future<void> showMixerGroupMembership(
        BuildContext context, MixerController controller) =>
    showDialog<void>(
        animationStyle: AnimationStyle.noAnimation,
        context: context,
        builder: (_) => _GroupMembership(controller));

class _GroupMembership extends StatefulWidget {
  final MixerController controller;
  const _GroupMembership(this.controller);
  @override
  State<_GroupMembership> createState() => _GroupMembershipState();
}

class _GroupMembershipState extends State<_GroupMembership> {
  String? selected, status;
  bool pending = false;
  @override
  Widget build(BuildContext context) {
    final store = widget.controller.store;
    return ListenableBuilder(
        listenable: store,
        builder: (context, _) {
          final targets = [
            for (final strip in store.assignmentStrips)
              if (store.captureNameTarget(strip.ch) case final target?)
                (strip.name, target)
          ];
          final choices = <String, String>{};
          for (final (_, target) in targets) {
            for (final choice
                in store.assignmentsForChannel(target, includeUnknown: true)) {
              if (RegExp(r'^dca[0-9]+_assign$').hasMatch(choice.id) ||
                  choice.id.startsWith('mute_group:')) {
                choices.putIfAbsent(choice.id, () => choice.label);
              }
            }
          }
          for (var group = 0;
              group < widget.controller.muteGroupCount;
              group++) {
            choices.putIfAbsent(
                'mute_group:$group', () => 'Mute group ${group + 1}');
          }
          final id = choices.containsKey(selected)
              ? selected
              : choices.keys.firstOrNull;
          return PopScope(
              canPop: !pending,
              child: AlertDialog(
                backgroundColor: ConsolePalette.of(context).panel,
                title: const Text('Group members'),
                content: SizedBox(
                    width: 380,
                    child: Column(mainAxisSize: MainAxisSize.min, children: [
                      if (id == null)
                        Text(!store.engine.capsReady || targets.isEmpty
                            ? 'Loading group memberships…'
                            : 'No group assignments are advertised for these strips.')
                      else ...[
                        DropdownButton<String>(
                            value: id,
                            isExpanded: true,
                            items: [
                              for (final entry in choices.entries)
                                DropdownMenuItem(
                                    value: entry.key, child: Text(entry.value))
                            ],
                            onChanged: pending
                                ? null
                                : (value) => setState(() {
                                      selected = value;
                                      status = null;
                                    })),
                        Flexible(
                            child: SingleChildScrollView(
                                child: Column(children: [
                          for (final (name, target) in targets)
                            if (store
                                    .assignmentsForChannel(target,
                                        includeUnknown: true)
                                    .any((c) => c.id == id) ||
                                id.startsWith('mute_group:'))
                              Builder(builder: (context) {
                                final current =
                                    store.assignmentForChannel(target, id);
                                final error =
                                    store.assignmentEditError(target, id);
                                if (current == null) {
                                  return ListTile(
                                    title:
                                        Text(name.isEmpty ? target.key : name),
                                    subtitle: Text(
                                        '${target.key}\nMembership unavailable or still loading.'),
                                    trailing: const Text('Unavailable'),
                                  );
                                }
                                return SwitchListTile.adaptive(
                                    title:
                                        Text(name.isEmpty ? target.key : name),
                                    subtitle: Text(error == null
                                        ? target.key
                                        : '${target.key}\n$error'),
                                    value: current == true,
                                    onChanged: pending || error != null
                                        ? null
                                        : (value) async {
                                            setState(() {
                                              pending = true;
                                              status = null;
                                            });
                                            final result =
                                                await store.assignChannel(
                                                    target, id, value,
                                                    expected: current);
                                            if (!mounted) return;
                                            setState(() {
                                              pending = false;
                                              status = result.error ??
                                                  'Assignment state observed.';
                                            });
                                          });
                              }),
                        ]))),
                      ],
                      if (pending || status != null)
                        Text(pending ? 'Sending assignment…' : status!),
                    ])),
                actions: [
                  TextButton(
                      onPressed: pending ? null : () => Navigator.pop(context),
                      child: const Text('Close'))
                ],
              ));
        });
  }
}
