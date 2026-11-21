import '../../widgets/inline_feedback.dart';
import '../../ui/console.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import '../../settings/prefs.dart';
import '../controller.dart';
import '../design.dart';
import '../mixer_layers.dart';
import '../store.dart' show CustomMixerView;

/// Channel catalog for the layer editor, from live surface strips. The
/// master strip is never part of a layer - it stays fixed on the mixer page.
List<LayerChannelOption> layerChannelOptionsOf(MixerController controller) =>
    controller.layerChannels;

/// Shared spacing for the mixer layers surface and its persistent bar. Visual
/// padding stays inside these insets while every control keeps its 44-point
/// touch target.
abstract final class MixerLayerMetrics {
  static const double inset = AppDesign.spaceLg;
  static const double gap = AppDesign.spaceSm;
  static const double tightGap = AppDesign.spaceXs;
  static const double barHeight = AppDesign.touchTarget + gap;
  static const EdgeInsets barPadding =
      EdgeInsets.symmetric(horizontal: inset, vertical: tightGap);
  static const EdgeInsets headingPadding =
      EdgeInsets.fromLTRB(inset, AppDesign.spaceLg, inset, tightGap);
  static const EdgeInsets rowPadding =
      EdgeInsets.symmetric(horizontal: inset, vertical: tightGap);
}

String _optionName(List<LayerChannelOption> channels, String key) {
  for (final option in channels) {
    if (option.key == key) return option.name;
  }
  return key;
}

/// Count wording for layer rows. One strip reads as "1 channel", not "1
/// channels"; the same phrase labels layers, views and drafts.
String mixerChannelCount(int count) =>
    count == 1 ? '1 channel' : '$count channels';

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

/// Deletes a layer; when it was active for [scope], [activate] moves the view
/// back to the built-in All layer first so the mixer never points at a missing
/// id. Pass the mixer scope so another mixer's selection is never touched.
Future<void> deleteMixerLayer(
  String id, {
  required Future<void> Function(String id) activate,
  String? scope,
  String? activeSelection,
}) async {
  final prefs = PrefsService.instance;
  final active = activeSelection ??
      (scope == null
          ? prefs.activeMixerLayerId
          : prefs.mixerSelection(scope));
  if (active == 'layer:$id' || active == id) {
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
  final pasted = await showConsoleDialog<String>(
    context,
    builder: (context) => ConsoleDialog(
      title: 'Import layers',
      content: TextField(
        controller: editing,
        maxLines: 6,
        minLines: 3,
        decoration: const InputDecoration(
          hintText: 'Paste layers JSON here',
        ),
      ),
      actions: [
        ConsoleButton(
          label: 'Cancel',
          variant: ConsoleButtonVariant.quiet,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context),
        ),
        ConsoleButton(
          label: 'Import',
          variant: ConsoleButtonVariant.primary,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context, editing.text),
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
  final name = await showConsoleDialog<String>(
    context,
    builder: (context) => ConsoleDialog(
      title: 'Rename layer',
      content: TextField(
        controller: editing,
        autofocus: true,
        textCapitalization: TextCapitalization.words,
        decoration: const InputDecoration(hintText: 'Layer name'),
        onSubmitted: (value) => Navigator.pop(context, value),
      ),
      actions: [
        ConsoleButton(
          label: 'Cancel',
          variant: ConsoleButtonVariant.quiet,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context),
        ),
        ConsoleButton(
          label: 'Save',
          variant: ConsoleButtonVariant.primary,
          size: ConsoleButtonSize.regular,
          onPressed: () => Navigator.pop(context, editing.text),
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

/// Layers workspace (mixer entry point). One adaptive surface replaces the
/// old stacked sheets: a right-hand panel on wide windows and a bottom sheet
/// on narrow ones. Switching, creating and editing all happen in place.
Future<void> showMixerLayers(
  BuildContext context,
  MixerController controller, {
  bool createLayer = false,
  bool editActive = false,
}) {
  final wide = MediaQuery.sizeOf(context).width >=
      AppDesign.compactToolbarBreakpoint;
  final palette = ConsolePalette.of(context);
  return showGeneralDialog<void>(
    context: context,
    barrierDismissible: true,
    barrierLabel: 'Close layers',
    barrierColor: palette.background.withValues(alpha: .72),
    transitionDuration: Duration.zero,
    pageBuilder: (context, _, _) => SafeArea(
      child: Align(
        alignment: wide ? Alignment.centerRight : Alignment.bottomCenter,
        child: Material(
          color: palette.background,
          elevation: 16,
          shape: RoundedRectangleBorder(
              borderRadius: wide
                  ? BorderRadius.horizontal(
                      left: Radius.circular(AppDesign.panelRadius))
                  : BorderRadius.vertical(
                      top: Radius.circular(AppDesign.panelRadius))),
          clipBehavior: Clip.antiAlias,
          child: SizedBox(
            width: wide ? 440 : double.infinity,
            height: wide
                ? double.infinity
                : MediaQuery.sizeOf(context).height * .9,
            child: _MixerLayersWorkspace(
              controller: controller,
              startCreateLayer: createLayer,
              startEditActive: editActive,
            ),
          ),
        ),
      ),
    ),
  );
}

/// Anchored add panel used by mixer Edit layout mode. It is a single surface
/// attached to the Add strip tile, not a second stacked sheet. The tile itself
/// fills the strip column, so callers pass [anchorBounds] for its visible top
/// edge; anchoring to the whole tile would cap the panel at the few points
/// above it. [included] is read again after every add so a channel never
/// reappears in the list once it is in the draft.
Future<void> showMixerLayerAddPanel(
  BuildContext anchor,
  MixerController controller, {
  required List<String> Function() included,
  required ValueChanged<String> onAdd,
  Rect? anchorBounds,
}) =>
    showAnchoredPopover<void>(
      anchor,
      (context) => StatefulBuilder(
        builder: (context, setPanel) => Padding(
          padding: const EdgeInsets.all(AppDesign.spaceMd),
          child: _ChannelChoiceList(
            options: controller.layerChannels,
            included: included(),
            onAdd: (key) {
              onAdd(key);
              setPanel(() {});
            },
            onDone: () => Navigator.pop(context),
            fillResults: true,
          ),
        ),
      ),
      anchorBounds: anchorBounds,
      width: 320,
    );

enum _LayerMode { browse, layer, view }

class _MixerLayersWorkspace extends StatefulWidget {
  final MixerController controller;
  final bool startCreateLayer;
  final bool startEditActive;
  const _MixerLayersWorkspace(
      {required this.controller,
      this.startCreateLayer = false,
      this.startEditActive = false});
  @override
  State<_MixerLayersWorkspace> createState() => _MixerLayersWorkspaceState();
}

class _MixerLayersWorkspaceState extends State<_MixerLayersWorkspace> {
  final _name = TextEditingController();
  final _device = TextEditingController();
  _LayerMode _mode = _LayerMode.browse;
  MixerLayer? _layer;
  CustomMixerView? _view;
  List<String> _draft = const [];
  int _density = MixerLayer.defaultStripsPerPage;
  bool _adding = false;
  bool _busy = false;
  bool _confirmDelete = false;
  String? _feedback;

  MixerController get _controller => widget.controller;

  @override
  void initState() {
    super.initState();
    if (widget.startCreateLayer) {
      _beginNewLayer();
    } else if (widget.startEditActive) {
      _beginEditActive();
    }
  }

  @override
  void dispose() {
    _name.dispose();
    _device.dispose();
    super.dispose();
  }

  void _beginNewLayer() {
    final name = 'Layer ${PrefsService.instance.mixerLayers.length + 1}';
    _layer = MixerLayer(
        id: MixerLayer.newId(), name: name, channelKeys: const []);
    _view = null;
    _draft = const [];
    _name.text = name;
    _device.text = '';
    _density = MixerLayer.defaultStripsPerPage;
    _mode = _LayerMode.layer;
    _adding = false;
    _confirmDelete = false;
    _feedback = null;
  }

  void _beginNewView() {
    final name = 'View ${_controller.store.customMixerViews.length + 1}';
    _view = CustomMixerView(
        id: DateTime.now().microsecondsSinceEpoch.toString(),
        name: name,
        stripKeys: const []);
    _layer = null;
    _draft = const [];
    _name.text = name;
    _device.text = '';
    _mode = _LayerMode.view;
    _adding = false;
    _confirmDelete = false;
    _feedback = null;
  }

  void _beginEditLayer(MixerLayer layer) {
    _layer = layer;
    _view = null;
    _draft = [...layer.channelKeys];
    _name.text = layer.name;
    _device.text = layer.deviceHint ?? '';
    _density = layer.effectiveStripsPerPage;
    _mode = _LayerMode.layer;
    _adding = false;
    _confirmDelete = false;
    _feedback = null;
  }

  void _beginEditView(CustomMixerView view) {
    _view = view;
    _layer = null;
    _draft = [...view.stripKeys];
    _name.text = view.name;
    _device.text = '';
    _mode = _LayerMode.view;
    _adding = false;
    _confirmDelete = false;
    _feedback = null;
  }

  void _beginEditActive() {
    if (!_controller.activeLayer.isAll) {
      _beginEditLayer(_controller.activeLayer);
    } else if (_controller.activeCustomView != null) {
      _beginEditView(_controller.activeCustomView!);
    }
  }

  List<LayerChannelOption> get _draftOptions => [
        for (final key in _draft) ?_controller.channelOptionFor(key),
      ];

  List<String> get _draftVisibleKeys =>
      _draftOptions.map((option) => option.key).toList();

  void _addDraft(String key) {
    if (_draft.contains(key)) return;
    setState(() => _draft = [..._draft, key]);
  }

  void _removeDraft(String key) {
    if (!_draft.contains(key)) return;
    setState(() => _draft = [..._draft]..remove(key));
  }

  void _moveDraft(String key, int delta) {
    final visible = _draftVisibleKeys;
    final from = visible.indexOf(key);
    if (from < 0) return;
    final to = (from + delta).clamp(0, visible.length - 1);
    if (to == from) return;
    visible.insert(to, visible.removeAt(from));
    setState(
        () => _draft = _controller.mergeVisibleOrder(_draft, visible));
  }

  void _reorderDraft(int oldIndex, int newIndex) {
    final visible = _draftVisibleKeys;
    if (oldIndex < 0 || oldIndex >= visible.length) return;
    final moved = visible.removeAt(oldIndex);
    visible.insert(newIndex.clamp(0, visible.length), moved);
    setState(
        () => _draft = _controller.mergeVisibleOrder(_draft, visible));
  }

  void _backToBrowse() {
    setState(() {
      _mode = _LayerMode.browse;
      _layer = null;
      _view = null;
      _draft = const [];
      _adding = false;
      _confirmDelete = false;
      _feedback = null;
    });
  }

  Future<void> _saveAndUse() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _feedback = null;
    });
    try {
      final name = _name.text.trim();
      if (_mode == _LayerMode.layer) {
        final layer = _layer!;
        final device = _device.text.trim();
        final next = layer.copyWith(
          name: name.isEmpty ? layer.name : name,
          channelKeys: List.unmodifiable(_draft),
          stripsPerPage: _density,
          deviceHint: device.isEmpty ? null : device,
          clearDeviceHint: device.isEmpty,
        );
        await saveMixerLayer(next);
        await _controller.setActiveLayer(next.id);
      } else {
        final view = _view!;
        final next = CustomMixerView(
            id: view.id,
            name: name.isEmpty ? view.name : name,
            stripKeys: List.unmodifiable(_draft));
        final views = [..._controller.store.customMixerViews];
        final index = views.indexWhere((candidate) => candidate.id == next.id);
        if (index >= 0) {
          views[index] = next;
        } else {
          views.add(next);
        }
        await _controller.store.saveCustomMixerViews(views);
        await _controller.setBank('custom:${next.id}');
      }
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (mounted) {
        setState(() {
          _busy = false;
          _feedback = 'Could not save: $error';
        });
      }
    }
  }

  Future<void> _delete() async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      if (_mode == _LayerMode.layer) {
        final layer = _layer!;
        await deleteMixerLayer(layer.id,
            scope: _controller.mixerScope,
            activate: _controller.setActiveLayer);
      } else {
        final view = _view!;
        final views = [
          for (final existing in _controller.store.customMixerViews)
            if (existing.id != view.id) existing,
        ];
        await _controller.store.saveCustomMixerViews(views);
      }
      if (mounted) Navigator.pop(context);
    } catch (error) {
      if (mounted) {
        setState(() {
          _busy = false;
          _confirmDelete = false;
          _feedback = 'Could not delete: $error';
        });
      }
    }
  }

  Future<void> _selectBank(String key) async {
    Navigator.pop(context);
    await _controller.setBank(key);
  }

  Future<void> _selectLayer(MixerLayer layer) async {
    Navigator.pop(context);
    await _controller.setActiveLayer(layer.id);
  }

  Future<void> _selectView(CustomMixerView view) async {
    Navigator.pop(context);
    await _controller.setBank('custom:${view.id}');
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final title = switch (_mode) {
      _LayerMode.browse => 'Layers',
      _LayerMode.layer => PrefsService.instance.mixerLayers
              .any((saved) => saved.id == _layer!.id)
          ? 'Edit layer'
          : 'New layer',
      _LayerMode.view => _controller.store.customMixerViews
              .any((saved) => saved.id == _view!.id)
          ? 'Edit view'
          : 'New view',
    };
    return Column(children: [
      Container(
        padding: const EdgeInsets.fromLTRB(16, 12, 8, 8),
        decoration:
            BoxDecoration(border: Border(bottom: BorderSide(color: palette.line))),
        child: Row(children: [
          if (_mode != _LayerMode.browse)
            ConsoleIconButton(
                label: 'Back to layers',
                onPressed: _busy ? null : _backToBrowse,
                child: Icon(Icons.arrow_back, color: palette.mutedText)),
          Expanded(
              child: Text(title,
                  style: mixerType(context, 20, weight: FontWeight.w600))),
          ConsoleIconButton(
              label: 'Close layers',
              onPressed: _busy ? null : () => Navigator.pop(context),
              child: Icon(Icons.close, color: palette.mutedText)),
        ]),
      ),
      Expanded(
          child: _mode == _LayerMode.browse
              ? _browseBody(context)
              : _editorBody(context)),
      if (_mode != _LayerMode.browse) _editorActions(context),
    ]);
  }

  Widget _browseBody(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final prefs = PrefsService.instance;
    final layers = prefs.mixerLayers;
    final views = _controller.store.customMixerViews;
    final banks = _controller.store.banks;
    final selection = _controller.selectionKey;
    final activeLayer = _controller.activeLayer;
    final nativeViews = [
      for (final bank in banks)
        if (bank.key != 'view:all' && !bank.key.startsWith('custom:')) bank,
    ];
    Widget heading(String title, String detail) => Padding(
          padding: MixerLayerMetrics.headingPadding,
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Text(title,
                style: mixerType(context, 16, weight: FontWeight.w600)),
            Text(detail,
                style: mixerType(context, 12, color: palette.mutedText)),
          ]),
        );
    Widget note(String text) => Padding(
          padding: const EdgeInsets.fromLTRB(
              MixerLayerMetrics.inset, 0, MixerLayerMetrics.inset, MixerLayerMetrics.gap),
          child:
              Text(text, style: mixerType(context, 13, color: palette.mutedText)),
        );
    Widget action(String label, VoidCallback onPressed) => ListTile(
          leading: Icon(Icons.add, color: palette.accent),
          title: Text(label, style: mixerType(context, 17)),
          onTap: onPressed,
        );
    return ListView(
        padding: const EdgeInsets.only(bottom: MixerLayerMetrics.inset),
        children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 12,
            MixerLayerMetrics.inset, MixerLayerMetrics.tightGap),
        child: Text('Current: ${_controller.selectionLabel}',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: mixerType(context, 13, color: palette.accent)),
      ),
      ListTile(
        title: Text('All', style: mixerType(context, 17)),
        subtitle: Text('All channels',
            style: mixerType(context, 13, color: palette.mutedText)),
        trailing: selection == 'view:all'
            ? Icon(Icons.check, color: palette.accent)
            : null,
        onTap: () => _selectBank('view:all'),
      ),
      heading('On this mixer', 'Native views'),
      if (nativeViews.isEmpty)
        note('This mixer does not advertise additional views.')
      else
        for (final bank in nativeViews)
          ListTile(
            title: Text(bank.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: mixerType(context, 17)),
            trailing: !_controller.isCustomLayerActive &&
                    _controller.store.bank == bank.key
                ? Icon(Icons.check, color: palette.accent)
                : null,
            onTap: () => _selectBank(bank.key),
          ),
      heading('Saved in OpenFader', 'Layers and custom views'),
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
            MixerLayerMetrics.gap, MixerLayerMetrics.inset, 0),
        child: Text('Custom views', style: mixerType(context, 15)),
      ),
      action('New view', () => setState(_beginNewView)),
      if (views.isEmpty)
        note('No custom views yet.')
      else
        for (final view in views)
          ListTile(
            title: Text(view.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: mixerType(context, 17)),
            subtitle: Text(mixerChannelCount(view.stripKeys.length),
                style: mixerType(context, 13, color: palette.mutedText)),
            trailing: Row(mainAxisSize: MainAxisSize.min, children: [
              if (selection == 'custom:${view.id}')
                Icon(Icons.check, color: palette.accent),
              ConsoleIconButton(
                  label: 'Edit ${view.name}',
                  onPressed: () => setState(() => _beginEditView(view)),
                  child: Icon(Icons.edit_outlined, color: palette.mutedText)),
            ]),
            onTap: () => _selectView(view),
          ),
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
            MixerLayerMetrics.gap, MixerLayerMetrics.inset, 0),
        child: Text('Layers', style: mixerType(context, 15)),
      ),
      action('New layer', () => setState(_beginNewLayer)),
      if (layers.isEmpty)
        note('No layers yet.')
      else
        for (final layer in layers)
          ListTile(
            title: Text(layer.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: mixerType(context, 17)),
            subtitle: Text(
                '${mixerChannelCount(layer.channelKeys.length)} · '
                '${layer.effectiveStripsPerPage} per page'
                '${layer.deviceHint == null ? '' : ' · ${layer.deviceHint}'}',
                style: mixerType(context, 13, color: palette.mutedText)),
            trailing: Row(mainAxisSize: MainAxisSize.min, children: [
              if (activeLayer.id == layer.id)
                Icon(Icons.check, color: palette.accent),
              ConsoleIconButton(
                  label: 'Edit ${layer.name}',
                  onPressed: () => setState(() => _beginEditLayer(layer)),
                  child: Icon(Icons.edit_outlined, color: palette.mutedText)),
            ]),
            onTap: () => _selectLayer(layer),
          ),
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 8,
            MixerLayerMetrics.inset, MixerLayerMetrics.tightGap),
        child: Text('Templates',
            style: mixerType(context, 13, color: palette.mutedText)),
      ),
      for (final template in MixerLayerTemplate.all)
        ListTile(
          leading: Icon(Icons.auto_awesome, color: palette.mutedText),
          title: Text(template.name, style: mixerType(context, 17)),
          subtitle: Text('From channel names',
              style: mixerType(context, 13, color: palette.mutedText)),
          onTap: () {
            final layer = template.resolve(_controller.layerChannels);
            setState(() => _beginEditLayer(layer));
          },
        ),
      Divider(color: palette.line),
      ListTile(
        leading: Icon(Icons.ios_share, color: palette.mutedText),
        title: Text('Share layers', style: mixerType(context, 17)),
        subtitle: Text('Copy layers JSON to clipboard',
            style: mixerType(context, 13, color: palette.mutedText)),
        onTap: () => shareMixerLayers(context),
      ),
      ListTile(
        leading: Icon(Icons.file_download_outlined, color: palette.mutedText),
        title: Text('Import layers', style: mixerType(context, 17)),
        subtitle: Text('Paste layers JSON',
            style: mixerType(context, 13, color: palette.mutedText)),
        onTap: () => showMixerLayerImport(context),
      ),
    ]);
  }

  Widget _editorBody(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final options = _draftOptions;
    return ReorderableListView.builder(
      buildDefaultDragHandles: false,
      padding: const EdgeInsets.only(bottom: 8),
      itemCount: options.length,
      onReorderItem: _reorderDraft,
      header: _editorHeader(context),
      footer: _addSection(context),
      itemBuilder: (context, index) {
        final option = options[index];
        final canLeft = index > 0;
        final canRight = index < options.length - 1;
        return Padding(
          key: ValueKey('layer-draft:${option.key}'),
          padding: const EdgeInsets.symmetric(
              horizontal: MixerLayerMetrics.tightGap),
          child: Container(
            constraints: const BoxConstraints(
                minHeight: AppDesign.touchTarget + AppDesign.spaceMd),
            padding: const EdgeInsets.symmetric(
                vertical: MixerLayerMetrics.tightGap),
            color: index.isEven ? palette.panel : palette.background,
            child: Row(children: [
            ReorderableDragStartListener(
                index: index,
                child: Semantics(
                    label: 'Reorder ${option.name}',
                    child: SizedBox.square(
                        dimension: AppDesign.touchTarget,
                        child: Icon(Icons.drag_indicator,
                            color: palette.mutedText)))),
            Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                  Text(option.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: mixerType(context, 16)),
                  Text(option.key,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: mixerType(context, 12, color: palette.mutedText)),
                ])),
            PopupMenuButton<String>(
              popUpAnimationStyle: AnimationStyle.noAnimation,
              tooltip: 'Channel options',
              icon: Icon(Icons.more_horiz, color: palette.mutedText),
              color: palette.chip,
              onSelected: (value) {
                switch (value) {
                  case 'left':
                    _moveDraft(option.key, -1);
                  case 'right':
                    _moveDraft(option.key, 1);
                  case 'remove':
                    _removeDraft(option.key);
                }
              },
              itemBuilder: (context) => [
                PopupMenuItem(
                    value: 'left',
                    enabled: canLeft,
                    child: const Text('Move left')),
                PopupMenuItem(
                    value: 'right',
                    enabled: canRight,
                    child: const Text('Move right')),
                const PopupMenuItem(
                    value: 'remove', child: Text('Remove')),
              ],
            ),
          ]),
          ),
        );
      },
    );
  }

  Widget _editorHeader(BuildContext context) {
    final palette = ConsolePalette.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 12,
            MixerLayerMetrics.inset, MixerLayerMetrics.tightGap),
        child: TextField(
          controller: _name,
          textCapitalization: TextCapitalization.words,
          decoration: const InputDecoration(labelText: 'Name'),
        ),
      ),
      if (_mode == _LayerMode.layer) ...[
        Padding(
          padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
              MixerLayerMetrics.tightGap, MixerLayerMetrics.inset, 0),
          child: TextField(
            controller: _device,
            decoration: const InputDecoration(
              labelText: 'Device tag',
              hintText: 'Optional, e.g. X32',
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 12,
              MixerLayerMetrics.inset, MixerLayerMetrics.tightGap),
          child: Text('Strips per page',
              style: mixerType(context, 13, color: palette.mutedText)),
        ),
        Padding(
          padding: const EdgeInsets.symmetric(
              horizontal: MixerLayerMetrics.inset),
          child: ConsoleActionGroup(children: [
            for (final option in MixerLayer.stripsPerPageOptions)
              ChoiceChip(
                label: Text('$option'),
                selected: _density == option,
                onSelected: (_) => setState(() => _density = option),
              ),
          ]),
        ),
      ],
      Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 16,
            MixerLayerMetrics.inset, MixerLayerMetrics.tightGap),
        child: Text(
            '${mixerChannelCount(_draftOptions.length)} · drag to reorder',
            style: mixerType(context, 13, color: palette.mutedText)),
      ),
    ]);
  }

  Widget _addSection(BuildContext context) {
    final palette = ConsolePalette.of(context);
    if (!_adding) {
      return Padding(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset, 12,
            MixerLayerMetrics.inset, MixerLayerMetrics.gap),
        child: Align(
          alignment: Alignment.centerLeft,
          child: ConsoleButton(
            label: 'Add channel',
            icon: Icons.add,
            variant: ConsoleButtonVariant.quiet,
            size: ConsoleButtonSize.compact,
            onPressed: () => setState(() => _adding = true),
          ),
        ),
      );
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Row(children: [
        const SizedBox(width: MixerLayerMetrics.inset),
        Expanded(
            child: Text('Add channel',
                style: mixerType(context, 15, weight: FontWeight.w600))),
        ConsoleIconButton(
            label: 'Close add channel',
            onPressed: () => setState(() => _adding = false),
            child: Icon(Icons.close, color: palette.mutedText)),
      ]),
      _ChannelChoiceList(
        options: _controller.layerChannels,
        included: _draft,
        onAdd: _addDraft,
      ),
      const SizedBox(height: MixerLayerMetrics.gap),
    ]);
  }

  Widget _editorActions(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final existing = _mode == _LayerMode.layer
        ? PrefsService.instance.mixerLayers.any((l) => l.id == _layer!.id)
        : _controller.store.customMixerViews.any((v) => v.id == _view!.id);
    if (_confirmDelete) {
      return Container(
        padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
            MixerLayerMetrics.gap, MixerLayerMetrics.inset, 12),
        decoration:
            BoxDecoration(border: Border(top: BorderSide(color: palette.line))),
        child: Row(children: [
          Expanded(
              child: Text(
                  'Delete this ${_mode == _LayerMode.layer ? 'layer' : 'view'}?',
                  style: mixerType(context, 14))),
          ConsoleButton(
              label: 'Keep',
              variant: ConsoleButtonVariant.quiet,
              size: ConsoleButtonSize.compact,
              onPressed: _busy
                  ? null
                  : () => setState(() => _confirmDelete = false)),
          const SizedBox(width: MixerLayerMetrics.gap),
          ConsoleButton(
              label: 'Delete',
              variant: ConsoleButtonVariant.danger,
              size: ConsoleButtonSize.compact,
              onPressed: _busy ? null : _delete),
        ]),
      );
    }
    return Container(
      padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
          MixerLayerMetrics.gap, MixerLayerMetrics.inset, 12),
      decoration:
          BoxDecoration(border: Border(top: BorderSide(color: palette.line))),
      child: Column(mainAxisSize: MainAxisSize.min, children: [
        if (_feedback != null)
          Padding(
            padding: const EdgeInsets.only(bottom: MixerLayerMetrics.gap),
            child: Align(
                alignment: Alignment.centerLeft,
                child: Text(_feedback!,
                    style: mixerType(context, 12, color: palette.amber))),
          ),
        Row(children: [
          if (existing)
            ConsoleButton(
                label: 'Delete',
                variant: ConsoleButtonVariant.quiet,
                size: ConsoleButtonSize.compact,
                onPressed:
                    _busy ? null : () => setState(() => _confirmDelete = true)),
          const Spacer(),
          ConsoleButton(
              label: 'Cancel',
              variant: ConsoleButtonVariant.quiet,
              size: ConsoleButtonSize.compact,
              onPressed: _busy ? null : () => Navigator.pop(context)),
          const SizedBox(width: MixerLayerMetrics.gap),
          ConsoleButton(
              label: _busy ? 'Saving…' : 'Save & use',
              variant: ConsoleButtonVariant.primary,
              size: ConsoleButtonSize.compact,
              onPressed: _busy ? null : _saveAndUse),
        ]),
      ]),
    );
  }
}

/// Inline searchable channel list. Used inside the layers workspace and inside
/// the anchored add panel, so add and order never split into two screens.
/// [fillResults] makes the result area take the space its panel offers
/// instead of a fixed height, so a short or narrow window scrolls the list
/// rather than overflowing the surface.
class _ChannelChoiceList extends StatefulWidget {
  final List<LayerChannelOption> options;
  final List<String> included;
  final ValueChanged<String> onAdd;
  final VoidCallback? onDone;
  final bool fillResults;
  const _ChannelChoiceList(
      {required this.options,
      required this.included,
      required this.onAdd,
      this.onDone,
      this.fillResults = false});
  @override
  State<_ChannelChoiceList> createState() => _ChannelChoiceListState();
}

class _ChannelChoiceListState extends State<_ChannelChoiceList> {
  final _search = TextEditingController();
  final _added = <String>{};

  @override
  void didUpdateWidget(_ChannelChoiceList oldWidget) {
    super.didUpdateWidget(oldWidget);
    _added.removeWhere((key) => !widget.included.contains(key));
  }

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  void _add(String key) {
    setState(() => _added.add(key));
    widget.onAdd(key);
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final present = {...widget.included, ..._added};
    final query = _search.text.trim().toLowerCase();
    final choices = [
      for (final option in widget.options)
        if (!present.contains(option.key) &&
            (query.isEmpty ||
                option.name.toLowerCase().contains(query) ||
                option.key.toLowerCase().contains(query)))
          option,
    ];
    Widget results() => SizedBox(
        height: 220,
        child: choices.isEmpty
            ? Center(
                child: Text(
                    present.isEmpty
                        ? 'No channels are available to add.'
                        : 'All available channels are already in this layout.',
                    style: mixerType(context, 13, color: palette.mutedText)))
            : ListView.builder(
                shrinkWrap: true,
                itemCount: choices.length,
                itemBuilder: (context, index) {
                  final option = choices[index];
                  return ListTile(
                    dense: true,
                    title: Text(option.name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: mixerType(context, 16)),
                    subtitle: Text(option.key,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: mixerType(context, 12, color: palette.mutedText)),
                    trailing: Icon(Icons.add, color: palette.accent),
                    onTap: () => _add(option.key),
                  );
                },
              ));
    return Column(mainAxisSize: MainAxisSize.min, children: [
      Padding(
        padding: const EdgeInsets.symmetric(
            horizontal: MixerLayerMetrics.inset),
        child: TextField(
          controller: _search,
          onChanged: (_) => setState(() {}),
          decoration: const InputDecoration(
            hintText: 'Search channels',
            prefixIcon: Icon(Icons.search),
          ),
        ),
      ),
      const SizedBox(height: MixerLayerMetrics.gap),
      if (widget.fillResults)
        Flexible(child: results())
      else
        results(),
      if (widget.onDone != null)
        Padding(
          padding: const EdgeInsets.fromLTRB(MixerLayerMetrics.inset,
              MixerLayerMetrics.gap, MixerLayerMetrics.inset, 0),
          child: Align(
            alignment: Alignment.centerRight,
            child: ConsoleButton(
                label: 'Done',
                variant: ConsoleButtonVariant.quiet,
                size: ConsoleButtonSize.compact,
                onPressed: widget.onDone),
          ),
        ),
    ]);
  }
}

/// Per-layer editor sheet: name, density stepper, channel membership, and
/// drag-reorder. Writes go straight to prefs so the mixer (and Settings)
/// pick them up live; Save & use also makes this layer the active selection.
Future<void> showMixerLayerEditor(
  BuildContext context, {
  required MixerLayer layer,
  required List<LayerChannelOption> channels,
  Future<void> Function(MixerLayer layer)? onSaveAndUse,
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
            onSaveAndUse: onSaveAndUse,
          ),
        ),
      ),
    );

class _LayerEditor extends StatefulWidget {
  final String layerId;
  final List<LayerChannelOption> channels;
  final ScrollController scroll;
  final Future<void> Function(MixerLayer layer)? onSaveAndUse;
  const _LayerEditor(
      {required this.layerId,
      required this.channels,
      required this.scroll,
      this.onSaveAndUse});

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

  /// Commit the fields that are only written on submit (name, device hint),
  /// then either use the layer or close the editor.
  Future<void> _save({required bool use}) async {
    final layer = _layer;
    if (layer == null) return;
    var next = layer;
    final name = _name.text.trim();
    if (name.isNotEmpty && name != layer.name) {
      next = next.copyWith(name: name);
    }
    final hint = _deviceHint.text.trim();
    final deviceHint = hint.isEmpty ? null : hint;
    if (deviceHint != layer.deviceHint) {
      next = next.copyWith(
        deviceHint: deviceHint,
        clearDeviceHint: deviceHint == null,
      );
    }
    await saveMixerLayer(next);
    if (use && widget.onSaveAndUse != null) {
      await widget.onSaveAndUse!(next);
    }
    if (mounted) Navigator.pop(context);
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
                    labelText: 'Device tag',
                    hintText: 'Optional, e.g. X32',
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
                          ConsoleIconButton(
                            label: 'Remove from layer',
                            child: const Icon(Icons.remove_circle_outline,
                                size: 20),
                            onPressed: () {
                              final keys = [...layer.channelKeys]..remove(key);
                              saveMixerLayer(layer.copyWith(channelKeys: keys));
                            },
                          ),
                          ReorderableDragStartListener(
                            index: i,
                            child: Semantics(
                                label: 'Reorder ${_optionName(widget.channels, key)}',
                                child: Icon(Icons.drag_handle,
                                    color:
                                        ConsolePalette.of(context).mutedText)),
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
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 8, 16, 24),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    ConsoleButton(
                      label: 'Close',
                      variant: ConsoleButtonVariant.quiet,
                      size: ConsoleButtonSize.regular,
                      onPressed: () => Navigator.pop(context),
                    ),
                    const SizedBox(width: AppDesign.spaceSm),
                    ConsoleButton(
                      label:
                          widget.onSaveAndUse == null ? 'Save' : 'Save & use',
                      variant: ConsoleButtonVariant.primary,
                      size: ConsoleButtonSize.regular,
                      onPressed: () => _save(use: widget.onSaveAndUse != null),
                    ),
                  ],
                ),
              ),
            ],
          );
        },
      );
}
