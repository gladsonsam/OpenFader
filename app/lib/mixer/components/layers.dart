import '../../widgets/inline_feedback.dart';
import '../../ui/console.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../settings/prefs.dart';
import '../controller.dart';
import '../design.dart';
import '../mixer_layers.dart';
import 'banks.dart' show showCustomMixerViews;

/// Channel catalog for the layer editor, from live surface strips. The
/// master strip is never part of a layer - it stays fixed on the mixer page.
List<LayerChannelOption> layerChannelOptionsOf(MixerController controller) => [
      for (final s in controller.store.engine.strips)
        if (s.key != 'main')
          (
            key: s.key,
            name: s.name.isEmpty ? s.key : s.name,
          ),
    ];

String _optionName(List<LayerChannelOption> channels, String key) {
  for (final option in channels) {
    if (option.key == key) return option.name;
  }
  return key;
}

Future<void> saveMixerLayer(MixerLayer layer) async {
  final prefs = PrefsService.instance;
  final layers = [...prefs.mixerLayers];
  final index = layers.indexWhere((l) => l.id == layer.id);
  if (index >= 0) {
    layers[index] = layer;
  } else {
    layers.add(layer);
  }
  await prefs.setMixerLayers(layers);
}

Future<MixerLayer> duplicateMixerLayer(MixerLayer layer) async {
  final copy = MixerLayer(
    id: MixerLayer.newId(),
    name: '${layer.name} copy',
    channelKeys: [...layer.channelKeys],
    stripsPerPage: layer.stripsPerPage,
    deviceHint: layer.deviceHint,
  );
  await saveMixerLayer(copy);
  return copy;
}

/// Deletes a layer; when it was active, [activate] moves the view back to
/// the built-in All layer first so the mixer never points at a missing id.
Future<void> deleteMixerLayer(
  String id, {
  required Future<void> Function(String id) activate,
}) async {
  final prefs = PrefsService.instance;
  if (prefs.activeMixerLayerId == id) {
    await activate(MixerLayer.allLayerId);
  }
  await prefs.setMixerLayers([...prefs.mixerLayers.where((l) => l.id != id)]);
}

/// Copies the layers list JSON to the clipboard (export).
Future<void> shareMixerLayers(BuildContext context) async {
  final text = MixerLayer.encodeList(PrefsService.instance.mixerLayers);
  await Clipboard.setData(ClipboardData(text: text));
  if (context.mounted) {
    InlineFeedbackScope.show(context, 'Layers copied to clipboard');
  }
}

/// Pastes a layers JSON payload from a dialog text field, validates it via
/// [MixerLayer.tryDecodeList], and persists it on success. Invalid payloads
/// show inline error feedback and change nothing.
Future<void> showMixerLayerImport(BuildContext context) async {
  final editing = TextEditingController();
  final pasted = await showDialog<String>(
    animationStyle: AnimationStyle.noAnimation,
    context: context,
    builder: (context) => ConsoleAlertDialog(
      backgroundColor: ConsolePalette.of(context).chip,
      title: Text('Import layers', style: mixerType(context, 18)),
      content: TextField(
        controller: editing,
        maxLines: 6,
        minLines: 3,
        decoration: const InputDecoration(
          hintText: 'Paste layers JSON here',
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text('Cancel', style: mixerType(context, 16)),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, editing.text),
          child: Text('Import', style: mixerType(context, 16)),
        ),
      ],
    ),
  );
  editing.dispose();
  if (pasted == null || !context.mounted) return;
  final decoded = MixerLayer.tryDecodeList(pasted.trim());
  if (decoded == null) {
    InlineFeedbackScope.show(context, 'Invalid layers JSON - nothing imported');
    return;
  }
  await PrefsService.instance.setMixerLayers(decoded);
  if (context.mounted) {
    InlineFeedbackScope.show(context, 'Imported ${decoded.length} layers');
  }
}

Future<void> showMixerLayerRename(
    BuildContext context, MixerLayer layer) async {
  final editing = TextEditingController(text: layer.name);
  final name = await showDialog<String>(
    animationStyle: AnimationStyle.noAnimation,
    context: context,
    builder: (context) => ConsoleAlertDialog(
      backgroundColor: ConsolePalette.of(context).chip,
      title: Text('Rename layer', style: mixerType(context, 18)),
      content: TextField(
        controller: editing,
        autofocus: true,
        textCapitalization: TextCapitalization.words,
        decoration: const InputDecoration(hintText: 'Layer name'),
        onSubmitted: (value) => Navigator.pop(context, value),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text('Cancel', style: mixerType(context, 16)),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context, editing.text),
          child: Text('Save', style: mixerType(context, 16)),
        ),
      ],
    ),
  );
  editing.dispose();
  final trimmed = name?.trim() ?? '';
  if (trimmed.isNotEmpty && trimmed != layer.name) {
    await saveMixerLayer(layer.copyWith(name: trimmed));
  }
}

/// Layer list sheet (mixer entry point): pick the active layer, or
/// create/rename/duplicate/delete and open the per-layer editor.
Future<void> showMixerLayers(BuildContext context, MixerController controller) {
  final channels = layerChannelOptionsOf(controller);
  return showModalBottomSheet<void>(
      sheetAnimationStyle: AnimationStyle.noAnimation,
      context: context,
      backgroundColor: ConsolePalette.of(context).background,
      showDragHandle: true,
      builder: (context) => InlineFeedbackScope(
            expand: true,
            builder: (context) => SafeArea(
              child: ListenableBuilder(
                listenable: PrefsService.instance,
                builder: (context, _) {
                  final prefs = PrefsService.instance;
                  final state = prefs.mixerLayersState;
                  return ListView(
                    shrinkWrap: true,
                    children: [
                      Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 16),
                        child: Text('Layers',
                            style: mixerType(context, 20,
                                weight: FontWeight.w600)),
                      ),
                      ListTile(
                        title: Text('All', style: mixerType(context, 17)),
                        subtitle: Text('Follows bank paging',
                            style: mixerType(context, 13,
                                color: ConsolePalette.of(context).mutedText)),
                        trailing: state.activeLayer.isAll &&
                                controller.store.bank == 'view:all'
                            ? Icon(Icons.check,
                                color: ConsolePalette.of(context).lime)
                            : null,
                        onTap: () {
                          Navigator.pop(context);
                          controller.setBank('view:all');
                        },
                      ),
                      for (final view in controller.store.customMixerViews)
                        ListTile(
                          title: Text(view.name, style: mixerType(context, 17)),
                          subtitle: Text('${view.stripKeys.length} channels',
                              style: mixerType(context, 13,
                                  color: ConsolePalette.of(context).mutedText)),
                          trailing: state.activeLayer.isAll &&
                                  controller.store.bank == 'custom:${view.id}'
                              ? Icon(Icons.check,
                                  color: ConsolePalette.of(context).lime)
                              : null,
                          onTap: () {
                            Navigator.pop(context);
                            controller.setBank('custom:${view.id}');
                          },
                        ),
                      if (controller.store.customMixerViews.isNotEmpty)
                        ListTile(
                          title: Text('Edit existing views',
                              style: mixerType(context, 15)),
                          onTap: () {
                            Navigator.pop(context);
                            showCustomMixerViews(context, controller);
                          },
                        ),
                      for (final layer in state.layers)
                        ListTile(
                          title:
                              Text(layer.name, style: mixerType(context, 17)),
                          subtitle: Text(
                            '${layer.channelKeys.length} channels · '
                            '${layer.effectiveStripsPerPage} per page'
                            '${layer.deviceHint == null ? '' : ' · ${layer.deviceHint}'}',
                            style: mixerType(context, 13,
                                color: ConsolePalette.of(context).mutedText),
                          ),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              if (layer.id == state.activeLayerId)
                                Icon(Icons.check,
                                    color: ConsolePalette.of(context).lime),
                              PopupMenuButton<String>(
                                popUpAnimationStyle: AnimationStyle.noAnimation,
                                tooltip: 'Layer actions',
                                icon: Icon(Icons.more_vert,
                                    color:
                                        ConsolePalette.of(context).mutedText),
                                color: ConsolePalette.of(context).chip,
                                onSelected: (action) async {
                                  switch (action) {
                                    case 'edit':
                                      await showMixerLayerEditor(context,
                                          layer: layer, channels: channels);
                                    case 'rename':
                                      await showMixerLayerRename(
                                          context, layer);
                                    case 'duplicate':
                                      await duplicateMixerLayer(layer);
                                    case 'delete':
                                      await deleteMixerLayer(layer.id,
                                          activate: controller.setActiveLayer);
                                  }
                                },
                                itemBuilder: (context) => const [
                                  PopupMenuItem(
                                      value: 'edit', child: Text('Edit')),
                                  PopupMenuItem(
                                      value: 'rename', child: Text('Rename')),
                                  PopupMenuItem(
                                      value: 'duplicate',
                                      child: Text('Duplicate')),
                                  PopupMenuItem(
                                      value: 'delete', child: Text('Delete')),
                                ],
                              ),
                            ],
                          ),
                          onTap: () {
                            Navigator.pop(context);
                            controller.setActiveLayer(layer.id);
                          },
                        ),
                      Divider(color: ConsolePalette.of(context).line),
                      Padding(
                        padding: const EdgeInsets.fromLTRB(16, 4, 16, 0),
                        child: Text('New from template',
                            style: mixerType(context, 15,
                                color: ConsolePalette.of(context).mutedText)),
                      ),
                      for (final template in MixerLayerTemplate.all)
                        ListTile(
                          leading: Icon(Icons.auto_awesome,
                              color: ConsolePalette.of(context).mutedText),
                          title: Text(template.name,
                              style: mixerType(context, 17)),
                          subtitle: Text('Tap to create',
                              style: mixerType(context, 13,
                                  color: ConsolePalette.of(context).mutedText)),
                          onTap: () async {
                            final layer = template.resolve(channels);
                            await saveMixerLayer(layer);
                            if (context.mounted) {
                              await showMixerLayerEditor(context,
                                  layer: layer, channels: channels);
                            }
                          },
                        ),
                      Divider(color: ConsolePalette.of(context).line),
                      ListTile(
                        leading: Icon(Icons.ios_share,
                            color: ConsolePalette.of(context).mutedText),
                        title:
                            Text('Share layers', style: mixerType(context, 17)),
                        subtitle: Text('Copy layers JSON to clipboard',
                            style: mixerType(context, 13,
                                color: ConsolePalette.of(context).mutedText)),
                        onTap: () => shareMixerLayers(context),
                      ),
                      ListTile(
                        leading: Icon(Icons.file_download_outlined,
                            color: ConsolePalette.of(context).mutedText),
                        title: Text('Import layers',
                            style: mixerType(context, 17)),
                        subtitle: Text('Paste layers JSON',
                            style: mixerType(context, 13,
                                color: ConsolePalette.of(context).mutedText)),
                        onTap: () => showMixerLayerImport(context),
                      ),
                      Divider(color: ConsolePalette.of(context).line),
                      ListTile(
                        leading: Icon(Icons.add,
                            color: ConsolePalette.of(context).lime),
                        title: Text('New layer', style: mixerType(context, 17)),
                        onTap: () async {
                          final layer = MixerLayer(
                            id: MixerLayer.newId(),
                            name: 'Layer ${state.layers.length + 1}',
                            channelKeys: const [],
                          );
                          await saveMixerLayer(layer);
                          if (context.mounted) {
                            await showMixerLayerEditor(context,
                                layer: layer, channels: channels);
                          }
                        },
                      ),
                    ],
                  );
                },
              ),
            ),
          ));
}

/// Per-layer editor sheet: name, density stepper, channel membership, and
/// drag-reorder. Writes go straight to prefs so the mixer (and Settings)
/// pick them up live.
Future<void> showMixerLayerEditor(
  BuildContext context, {
  required MixerLayer layer,
  required List<LayerChannelOption> channels,
}) =>
    showModalBottomSheet<void>(
      sheetAnimationStyle: AnimationStyle.noAnimation,
      context: context,
      backgroundColor: ConsolePalette.of(context).background,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (context) => SafeArea(
        child: DraggableScrollableSheet(
          expand: false,
          initialChildSize: 0.85,
          minChildSize: 0.5,
          maxChildSize: 0.95,
          builder: (context, scroll) => _LayerEditor(
            layerId: layer.id,
            channels: channels,
            scroll: scroll,
          ),
        ),
      ),
    );

class _LayerEditor extends StatefulWidget {
  final String layerId;
  final List<LayerChannelOption> channels;
  final ScrollController scroll;
  const _LayerEditor(
      {required this.layerId, required this.channels, required this.scroll});

  @override
  State<_LayerEditor> createState() => _LayerEditorState();
}

class _LayerEditorState extends State<_LayerEditor> {
  late final TextEditingController _name;
  late final TextEditingController _deviceHint;

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(text: _layer?.name ?? '');
    _deviceHint = TextEditingController(text: _layer?.deviceHint ?? '');
  }

  @override
  void dispose() {
    _name.dispose();
    _deviceHint.dispose();
    super.dispose();
  }

  MixerLayer? get _layer {
    for (final layer in PrefsService.instance.mixerLayers) {
      if (layer.id == widget.layerId) return layer;
    }
    return null;
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
        listenable: PrefsService.instance,
        builder: (context, _) {
          final layer = _layer;
          if (layer == null) {
            return const Center(child: Text('Layer deleted'));
          }
          if (_name.text != layer.name && _name.selection.baseOffset < 0) {
            _name.text = layer.name;
          }
          final hintText = layer.deviceHint ?? '';
          if (_deviceHint.text != hintText &&
              _deviceHint.selection.baseOffset < 0) {
            _deviceHint.text = hintText;
          }
          final inLayer = layer.channelKeys.toSet();
          return ListView(
            controller: widget.scroll,
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: Text('Edit layer',
                    style: mixerType(context, 20, weight: FontWeight.w600)),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                child: TextField(
                  controller: _name,
                  textCapitalization: TextCapitalization.words,
                  decoration: const InputDecoration(labelText: 'Name'),
                  onChanged: (value) {
                    final trimmed = value.trim();
                    if (trimmed.isNotEmpty && trimmed != layer.name) {
                      saveMixerLayer(layer.copyWith(name: trimmed));
                    }
                  },
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
                child: TextField(
                  controller: _deviceHint,
                  textCapitalization: TextCapitalization.none,
                  decoration: const InputDecoration(
                    labelText: 'Device (optional)',
                    hintText: 'e.g. X32, DL32S - blank = all mixers',
                  ),
                  onChanged: (value) {
                    final trimmed = value.trim();
                    final next = trimmed.isEmpty ? null : trimmed;
                    if (next != layer.deviceHint) {
                      saveMixerLayer(layer.copyWith(
                        deviceHint: next,
                        clearDeviceHint: next == null,
                      ));
                    }
                  },
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 14, 16, 6),
                child: Text('Strips per page',
                    style: mixerType(context, 15,
                        color: ConsolePalette.of(context).mutedText)),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16),
                child: ConsoleActionGroup(
                  children: [
                    for (final option in MixerLayer.stripsPerPageOptions)
                      ChoiceChip(
                        label: Text('$option'),
                        selected: layer.effectiveStripsPerPage == option,
                        onSelected: (_) => saveMixerLayer(
                            layer.copyWith(stripsPerPage: option)),
                      ),
                  ],
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 18, 16, 6),
                child: Text(
                  'Channels (${inLayer.length} of ${widget.channels.length})',
                  style: mixerType(context, 15,
                      color: ConsolePalette.of(context).mutedText),
                ),
              ),
              for (final option in widget.channels)
                CheckboxListTile(
                  dense: true,
                  title: Text(option.name, style: mixerType(context, 16)),
                  subtitle: Text(option.key,
                      style: mixerType(context, 12,
                          color: ConsolePalette.of(context).mutedText)),
                  value: inLayer.contains(option.key),
                  onChanged: (on) async {
                    final keys = [...layer.channelKeys];
                    if (on == true) {
                      if (!keys.contains(option.key)) {
                        keys.add(option.key);
                      }
                    } else {
                      keys.remove(option.key);
                    }
                    await saveMixerLayer(layer.copyWith(channelKeys: keys));
                  },
                ),
              if (layer.channelKeys.isNotEmpty) ...[
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 18, 16, 6),
                  child: Text('Order (drag to reorder)',
                      style: mixerType(context, 15,
                          color: ConsolePalette.of(context).mutedText)),
                ),
                ReorderableListView.builder(
                  shrinkWrap: true,
                  physics: const NeverScrollableScrollPhysics(),
                  itemCount: layer.channelKeys.length,
                  itemBuilder: (context, i) {
                    final key = layer.channelKeys[i];
                    return ListTile(
                      key: ValueKey(key),
                      dense: true,
                      title: Text(_optionName(widget.channels, key),
                          style: mixerType(context, 16)),
                      trailing: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          IconButton(
                            tooltip: 'Remove from layer',
                            icon: const Icon(Icons.remove_circle_outline,
                                size: 20),
                            onPressed: () {
                              final keys = [...layer.channelKeys]..remove(key);
                              saveMixerLayer(layer.copyWith(channelKeys: keys));
                            },
                          ),
                          ReorderableDragStartListener(
                            index: i,
                            child: Icon(Icons.drag_handle,
                                color: ConsolePalette.of(context).mutedText),
                          ),
                        ],
                      ),
                    );
                  },
                  onReorderItem: (oldIndex, newIndex) async {
                    final keys = [...layer.channelKeys];
                    final key = keys.removeAt(oldIndex);
                    keys.insert(newIndex, key);
                    await saveMixerLayer(layer.copyWith(channelKeys: keys));
                  },
                ),
                const SizedBox(height: 16),
              ],
            ],
          );
        },
      );
}
