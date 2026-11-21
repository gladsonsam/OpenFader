part of '../settings.dart';

/// Custom mixer layers: pick the active layer and manage the user layers
/// (create/rename/duplicate/delete, density, channel membership and order).
/// Channel membership editing reuses the mixer's layer editor sheet; the
/// channel catalog comes from [SettingsStore.layerChannelOptions].
class _LayersSection extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  const _LayersSection({required this.store, required this.toast});

  @override
  Widget build(BuildContext context) {
    final prefs = store.prefs;
    return ListenableBuilder(
      listenable: prefs,
      builder: (context, _) {
        final state = prefs.mixerLayersState;
        final activeKey = store.mixerSelectionKey;
        // Settings lists layers only, so native and custom views have no row
        // here. Keep the list neutral for them instead of marking All.
        final allActive = activeKey == 'view:all';
        final activeLayerId = activeKey.startsWith('layer:')
            ? activeKey.substring('layer:'.length)
            : null;
        final channels = store.layerChannelOptions;
        Future<void> activate(String id) async {
          await store.activateMixerLayer(id);
          toast('Layer active');
        }

        return _Section(
          title: 'Layers',
          children: [
            ListTile(
              dense: true,
              title: Text(
                'All',
                style: TextStyle(
                  fontFamily: AppDesign.font,
                  fontSize: AppDesign.rowLabel,
                  color: ConsolePalette.of(context).text,
                ),
              ),
              subtitle: Text(
                'Follows bank paging',
                style: TextStyle(
                  fontFamily: AppDesign.font,
                  fontSize: AppDesign.body,
                  color: ConsolePalette.of(context).mutedText,
                ),
              ),
              trailing: allActive
                  ? Icon(Icons.check, color: ConsolePalette.of(context).accent)
                  : null,
              onTap: () => _runSettingsAction(context, () async {
                await activate(MixerLayer.allLayerId);
              }),
            ),
            for (final layer in state.layers)
              ListTile(
                dense: true,
                title: Text(
                  layer.name,
                  style: TextStyle(
                    fontFamily: AppDesign.font,
                    fontSize: AppDesign.rowLabel,
                    color: ConsolePalette.of(context).text,
                  ),
                ),
                subtitle: Text(
                  '${mixerChannelCount(layer.channelKeys.length)} · '
                  '${layer.effectiveStripsPerPage} per page'
                  '${layer.deviceHint == null ? '' : ' · ${layer.deviceHint}'}',
                  style: TextStyle(
                    fontFamily: AppDesign.font,
                    fontSize: AppDesign.body,
                    color: ConsolePalette.of(context).mutedText,
                  ),
                ),
                trailing: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (layer.id == activeLayerId)
                      Icon(Icons.check, color: ConsolePalette.of(context).accent),
                    PopupMenuButton<String>(
                      popUpAnimationStyle: AnimationStyle.noAnimation,
                      tooltip: 'Layer actions',
                      icon: Icon(Icons.more_vert,
                          color: ConsolePalette.of(context).mutedText),
                      onSelected: (action) =>
                          _runSettingsAction(context, () async {
                        switch (action) {
                          case 'edit':
                            await showMixerLayerEditor(context,
                                layer: layer, channels: channels);
                          case 'rename':
                            await showMixerLayerRename(context, layer);
                          case 'duplicate':
                            await duplicateMixerLayer(layer);
                            toast('Preference saved');
                          case 'delete':
                            await deleteMixerLayer(layer.id,
                                activeSelection: activeKey,
                                activate: activate);
                            toast('Preference saved');
                        }
                      }),
                      itemBuilder: (context) => const [
                        PopupMenuItem(value: 'edit', child: Text('Edit')),
                        PopupMenuItem(value: 'rename', child: Text('Rename')),
                        PopupMenuItem(
                            value: 'duplicate', child: Text('Duplicate')),
                        PopupMenuItem(value: 'delete', child: Text('Delete')),
                      ],
                    ),
                  ],
                ),
                onTap: () => _runSettingsAction(context, () async {
                  if (layer.id != state.activeLayerId) {
                    await activate(layer.id);
                  }
                }),
              ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
              child: ConsoleButton(
                icon: Icons.add,
                label: 'New layer',
                size: ConsoleButtonSize.regular,
                onPressed: () => _runSettingsAction(context, () async {
                  final layer = MixerLayer(
                    id: MixerLayer.newId(),
                    name: 'Layer ${state.layers.length + 1}',
                    channelKeys: const [],
                  );
                  await saveMixerLayer(layer);
                  toast('Preference saved');
                  if (context.mounted) {
                    await showMixerLayerEditor(context,
                        layer: layer, channels: channels);
                  }
                }),
              ),
            ),
          ],
        );
      },
    );
  }
}
