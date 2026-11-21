import '../../widgets/inline_feedback.dart';
import '../design.dart';
import '../../ui/console.dart';
import 'package:material_ui/material_ui.dart';
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
                  color: ConsolePalette.of(context).accent, size: 18),
          ])),
    const PopupMenuDivider(),
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
                  color: ConsolePalette.of(context).accent, size: 20),
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
            child: ConsoleIconButton(
                label: 'Previous channels',
                onPressed:
                    controller.page > 0 ? () => controller.nextPage(-1) : null,
                child: const Icon(Icons.chevron_left))),
        Text('${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
            style: mixerType(context, 11)),
        Expanded(
            child: ConsoleIconButton(
                label: 'Next channels',
                onPressed: controller.page + 1 < controller.pageCount
                    ? () => controller.nextPage(1)
                    : null,
                child: const Icon(Icons.chevron_right))),
      ]);
  @override
  Widget build(BuildContext context) {
    final banks = controller.store.banks;
    final layers = PrefsService.instance.mixerLayers;
    final count = controller.muteGroupCount;
    final status = controller.demo
        ? const SizedBox.shrink()
        : Text(controller.connectionLabel.toUpperCase(),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: mixerType(context, 10,
                color: controller.demo
                    ? ConsolePalette.of(context).amber
                    : ConsolePalette.of(context).accent,
                weight: FontWeight.w600));
    return Material(
        color: ConsolePalette.of(context).panel,
        child: vertical
            ? LayoutBuilder(builder: (context, box) {
                final palette = ConsolePalette.of(context);
                final outputs = controller.store.busRows;
                final hasOutputs = outputs.isNotEmpty;
                final hasGroups = count > 0;
                final groupRows = (count + 1) ~/ 2;
                final hasPages = controller.pageCount > 1;
                final hasSolo = controller.hasSolo;
                const rowHeight = AppDesign.touchTarget;
                const headingHeight = AppDesign.spaceXl;
                const outputPadding = AppDesign.spaceXs;
                final headingSpace = headingHeight *
                        (1 + (hasGroups ? 1 : 0) + (hasOutputs ? 1 : 0)) +
                    AppDesign.spaceLg;
                final fixedSpace = rowHeight +
                    (hasSolo ? rowHeight : 0) +
                    (hasPages ? rowHeight * 2 + AppDesign.spaceLg : 0) +
                    (hasOutputs ? outputPadding * 2 : 0);
                final compactRows = (hasGroups ? 1 : 0) + (hasOutputs ? 1 : 0);
                final showHeadings =
                    fixedSpace + headingSpace + compactRows * rowHeight <=
                        box.maxHeight;
                final availableRows = (box.maxHeight -
                        fixedSpace -
                        (showHeadings ? headingSpace : 0)) /
                    rowHeight;
                final groupSlots =
                    (availableRows.floor() - (hasOutputs ? 1 : 0))
                        .clamp(0, groupRows);
                final showMoreGroups = groupSlots < groupRows && groupSlots > 0;
                final visibleGroups =
                    ((groupSlots - (showMoreGroups ? 1 : 0)) * 2)
                        .clamp(0, count);
                final outputSlots = (availableRows.floor() - groupSlots)
                    .clamp(0, outputs.length);
                final visibleOutputs = outputSlots < outputs.length
                    ? (outputSlots - 1).clamp(0, outputs.length)
                    : outputs.length;

                Widget text(String label, double size,
                        {Color? color, FontWeight weight = FontWeight.w400}) =>
                    FittedBox(
                        fit: BoxFit.scaleDown,
                        child: Text(label,
                            maxLines: 1,
                            softWrap: false,
                            style: mixerType(context, size,
                                color: color, weight: weight)));

                Widget heading(String label) => SizedBox(
                    height: headingHeight,
                    child: Center(child: text(label, AppDesign.body)));

                Widget button(String label, VoidCallback? onTap,
                        {bool active = false,
                        double size = AppDesign.controlLabel,
                        Color? color,
                        FontWeight weight = FontWeight.w400}) =>
                    SizedBox(
                      height: rowHeight,
                      width: double.infinity,
                      child: Semantics(
                        selected: active,
                        child: TextButton(
                          onPressed: onTap,
                          style: TextButton.styleFrom(
                            backgroundColor: Colors.transparent,
                            shape: const RoundedRectangleBorder(),
                            padding: const EdgeInsets.symmetric(
                                horizontal: AppDesign.spaceXs),
                            minimumSize: const Size(rowHeight, rowHeight),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                            visualDensity: VisualDensity.standard,
                          ),
                          child: text(label, size,
                              color: active ? palette.accent
                                  : color ?? palette.mutedText,
                              weight: weight),
                        ),
                      ),
                    );

                return Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    if (groupSlots > 0) ...[
                      if (showHeadings) heading('MUTE'),
                      ColoredBox(
                        color: palette.background,
                        child: Column(children: [
                          for (var first = 0; first < visibleGroups; first += 2)
                            Row(children: [
                              for (var group = first; group < first + 2; group++)
                                Expanded(
                                    child: group >= visibleGroups
                                        ? const SizedBox.shrink()
                                        : MixerToggle(
                                            label: controller.muteGroupLabel(group),
                                            channelName: 'Mute group ${group + 1}',
                                            on: controller.store.engine
                                                    .muteGroups[group] ==
                                                true,
                                            visualHeight:
                                                AppDesign.actionDenseHeight,
                                            fontSize: AppDesign.smallLabel,
                                            horizontalInset: AppDesign.spaceXs,
                                            activeColor: palette.mute,
                                            activeForeground: palette.onDanger,
                                            onPressed: () => controller
                                                .toggleMuteGroup(group))),
                            ]),
                          if (showMoreGroups)
                            Builder(builder: (anchor) => button(
                                visibleGroups == 0 ? 'Mute groups' : 'More groups',
                                () => showMixerMuteGroups(anchor, controller),
                                color: controller.store.engine.muteGroups.values
                                        .any((on) => on)
                                    ? palette.mute
                                    : null,
                                size: AppDesign.smallLabel)),
                        ]),
                      ),
                    ],
                    if (showHeadings) heading('MIX'),
                    ColoredBox(
                      color: palette.background,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          if (showHeadings)
                            SizedBox(
                              height: AppDesign.spaceLg,
                              child: Padding(
                                padding: const EdgeInsets.symmetric(
                                    horizontal: AppDesign.spaceSm),
                                child: Align(
                                  alignment: Alignment.centerLeft,
                                  child: text(controller.master.number,
                                      AppDesign.smallLabel,
                                      color: controller.masterColor(palette),
                                      weight: FontWeight.w700),
                                ),
                              ),
                            ),
                          Builder(builder: (anchor) => button(
                              controller.master.name,
                              hasOutputs
                                  ? () => showMixerBuses(anchor, controller)
                                  : null,
                              color: palette.text,
                              size: AppDesign.sectionTitle,
                              weight: FontWeight.w600)),
                        ],
                      ),
                    ),
                    if (outputSlots > 0) ...[
                      if (showHeadings) heading('OUTPUTS'),
                      DecoratedBox(
                        decoration: BoxDecoration(
                          color: palette.background,
                          border: Border.all(color: palette.cap),
                          borderRadius: AppDesign.radius,
                        ),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                              vertical: outputPadding),
                          child: Column(children: [
                            for (final output in outputs.take(visibleOutputs))
                              button(output.name,
                                  () => controller.store.selectBus(output.id),
                                  active: output.active),
                            if (visibleOutputs < outputs.length)
                              Builder(builder: (anchor) => button(
                                  visibleOutputs == 0
                                      ? 'Select output'
                                      : 'More outputs',
                                  () => showMixerBuses(anchor, controller),
                                  size: AppDesign.smallLabel)),
                          ]),
                        ),
                      ),
                    ],
                    if (hasPages) ...[
                      Center(
                        child: ConsoleIconButton(
                          label: 'Previous channels',
                          onPressed: controller.page > 0
                              ? () => controller.nextPage(-1)
                              : null,
                          child: const Icon(Icons.chevron_left),
                        ),
                      ),
                      SizedBox(
                        height: AppDesign.spaceLg,
                        child: Center(child: text(
                            '${controller.page + 1}/${controller.pageCount}',
                            AppDesign.smallLabel)),
                      ),
                      Center(
                        child: ConsoleIconButton(
                          label: 'Next channels',
                          onPressed: controller.page + 1 < controller.pageCount
                              ? () => controller.nextPage(1)
                              : null,
                          child: const Icon(Icons.chevron_right),
                        ),
                      ),
                    ],
                    const Spacer(),
                    if (hasSolo)
                      button(
                        'Clear solos',
                        controller.store.canClearSolos
                            ? controller.store.clearSolos
                            : null,
                        size: AppDesign.smallLabel,
                        color: palette.solo,
                      ),
                  ],
                );
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
                        ConsoleIconButton(
                            label: 'Mute groups',
                            child:
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
                      ConsoleButton(
                          label: 'Edit group members',
                          variant: ConsoleButtonVariant.quiet,
                          onPressed: () =>
                              showMixerGroupMembership(context, controller)),
                      const SizedBox(height: 16),
                      ConsoleActionGroup(children: [
                        for (var g = 0; g < controller.muteGroupCount; g++)
                          SizedBox(
                              width: 140,
                              child: MixerToggle(
                                  fontSize: AppDesign.body,
                                  label: controller.muteGroupLabel(g),
                                  channelName: 'Mute group ${g + 1}',
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
                            ConsoleIconButton(
                                label: 'Previous channels',
                                onPressed: controller.page > 0
                                    ? () => controller.nextPage(-1)
                                    : null,
                                child: const Icon(Icons.chevron_left)),
                            Text(
                                '${controller.page + 1}/${controller.pageCount.clamp(1, 999)}',
                                style: mixerType(context, 13)),
                            ConsoleIconButton(
                                label: 'Next channels',
                                onPressed:
                                    controller.page + 1 < controller.pageCount
                                        ? () => controller.nextPage(1)
                                        : null,
                                child: const Icon(Icons.chevron_right)),
                          ])),
                      for (final bank in controller.store.banks)
                        ListTile(
                            title:
                                Text(bank.label, style: mixerType(context, 17)),
                            trailing: bank.key == controller.store.bank
                                ? Icon(Icons.check,
                                    color: ConsolePalette.of(context).accent)
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
        builder: (context) => InlineFeedbackScope(
              expand: true,
              builder: (context) => SafeArea(
                  child: ListenableBuilder(
                listenable: controller,
                builder: (context, _) => ListView(shrinkWrap: true, children: [
                  ListTile(
                      title: Text('Custom views',
                          style:
                              mixerType(context, 20, weight: FontWeight.w600))),
                  ListTile(
                      leading: const Icon(Icons.add),
                      title: const Text('Create view'),
                      onTap: () => _editCustomView(context, controller)),
                  for (final view in controller.store.customMixerViews)
                    ListTile(
                        title: Text(view.name),
                        subtitle: Text(
                            '${view.stripKeys.length} channels · saved order'),
                        trailing: const Icon(Icons.edit_outlined),
                        onTap: () =>
                            _editCustomView(context, controller, view)),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                    child: Text(
                        'Use Edit layout on the mixer to reorder a view.',
                        style: mixerType(context, 12,
                            color: ConsolePalette.of(context).mutedText)),
                  ),
                ]),
              )),
            ));

Future<void> _editCustomView(BuildContext context, MixerController controller,
    [CustomMixerView? existing]) async {
  final engine = controller.store.engine;
  final scope = controller.mixerScope;
  final strips = engine.strips.where((s) => s.key != 'main').toList();
  final name = TextEditingController(text: existing?.name ?? '');
  // Ordered membership: existing views keep their saved order, and new
  // selections append in the order the user ticks them.
  final selected = <String>[...?existing?.stripKeys];
  final route = DialogRoute<String>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => StatefulBuilder(
          builder: (context, setDialogState) => ConsoleDialog(
                title: existing == null ? 'Create view' : 'Edit view',
                scrollable: false,
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
                                if (!selected.contains(strip.key)) {
                                  selected.add(strip.key);
                                }
                              } else {
                                selected.remove(strip.key);
                              }
                            }),
                          ),
                      ])),
                    ])),
                actions: [
                  if (existing != null)
                    ConsoleButton(
                        label: 'Delete',
                        variant: ConsoleButtonVariant.danger,
                        size: ConsoleButtonSize.regular,
                        onPressed: () => Navigator.pop(context, 'delete')),
                  ConsoleButton(
                      label: 'Cancel',
                      variant: ConsoleButtonVariant.quiet,
                      size: ConsoleButtonSize.regular,
                      onPressed: () => Navigator.pop(context)),
                  ConsoleButton(
                      label: 'Save',
                      variant: ConsoleButtonVariant.primary,
                      size: ConsoleButtonSize.regular,
                      onPressed: () {
                        if (name.text.trim().isNotEmpty) {
                          Navigator.pop(context, 'save');
                        }
                      }),
                ],
              )));
  final result = await Navigator.of(context).push(route);
  final viewName = name.text.trim();
  await route.completed;
  name.dispose();
  if (result == null || !context.mounted) return;
  if (controller.mixerScope != scope) {
    InlineFeedbackScope.show(
        context, 'Mixer changed. Open custom views again.');
    return;
  }
  final views = controller.store.customMixerViews
      .where((v) => v.id != existing?.id)
      .toList();
  if (result == 'save') {
    final view = CustomMixerView(
        id: existing?.id ?? DateTime.now().microsecondsSinceEpoch.toString(),
        name: viewName,
        stripKeys: List.unmodifiable(selected));
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
      InlineFeedbackScope.show(context, 'Could not save view: $error');
    }
  }
}

Future<void> showMixerGroupMembership(
        BuildContext context, MixerController controller) =>
    showConsoleDialog<void>(context,
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
                'mute_group:$group',
                () => widget.controller.muteGroupLabel(group));
          }
          final id = choices.containsKey(selected)
              ? selected
              : choices.keys.firstOrNull;
          return PopScope(
              canPop: !pending,
              child: ConsoleDialog(
                title: 'Group members',
                scrollable: false,
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
                  ConsoleButton(
                      label: 'Close',
                      variant: ConsoleButtonVariant.quiet,
                      size: ConsoleButtonSize.regular,
                      onPressed: pending ? null : () => Navigator.pop(context))
                ],
              ));
        });
  }
}
