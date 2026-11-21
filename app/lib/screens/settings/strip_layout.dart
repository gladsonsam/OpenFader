part of '../settings.dart';

/// Channel-strip composition: presets, the ordered row list, and the layout
/// properties that are not rows. The list and the mixer strip share
/// [StripElement] metadata and [StripLayout] helpers, so hiding or moving a
/// row here has exactly one meaning in the running strip.
class _StripLayoutSection extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  const _StripLayoutSection({required this.store, required this.toast});

  @override
  Widget build(BuildContext context) {
    final prefs = store.prefs;
    return ListenableBuilder(
      listenable: prefs,
      builder: (context, _) {
        final layout = prefs.stripLayout;
        final palette = ConsolePalette.of(context);
        Future<void> apply(StripLayout next) async {
          await prefs.setStripLayout(next);
          toast('Preference saved');
        }

        final rows = layout.rows;
        return _Section(
          title: 'Channel strips',
          children: [
            Padding(
              padding: const EdgeInsets.symmetric(
                  horizontal: AppDesign.spaceLg, vertical: AppDesign.spaceMd),
              child: ConsoleActionGroup(
                children: [
                  for (final preset in const [
                    ('essential', 'Essential'),
                    ('full', 'Full'),
                    ('monitor', 'Monitor'),
                    ('phone', 'Phone'),
                  ])
                    _PresetChip(
                      label: preset.$2,
                      selected: layout ==
                          switch (preset.$1) {
                            'essential' => StripLayout.essential,
                            'full' => StripLayout.full,
                            'monitor' => StripLayout.monitor,
                            _ => StripLayout.phone,
                          },
                      onTap: () => apply(switch (preset.$1) {
                        'essential' => StripLayout.essential,
                        'full' => StripLayout.full,
                        'monitor' => StripLayout.monitor,
                        _ => StripLayout.phone,
                      }),
                    ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(AppDesign.spaceLg,
                  AppDesign.spaceMd, AppDesign.spaceLg, AppDesign.spaceXs),
              child: Text(
                'Rows (drag to reorder)',
                style: AppDesign.type(AppDesign.body, color: palette.mutedText),
              ),
            ),
            ReorderableListView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              buildDefaultDragHandles: false,
              itemCount: rows.length,
              itemBuilder: (context, index) {
                final element = rows[index];
                final shown = layout.shows(element);
                return DecoratedBox(
                  key: ValueKey(element.key),
                  decoration: BoxDecoration(
                    border: index < rows.length - 1
                        ? Border(bottom: BorderSide(color: palette.line))
                        : null,
                  ),
                  child: ConsolePreferenceRow(
                    label: element.label,
                    description: element.mandatory
                        ? 'Always shown'
                        : element.description,
                    foreground: shown ? null : palette.mutedText,
                    trailing: Row(mainAxisSize: MainAxisSize.min, children: [
                      if (!element.mandatory)
                        _Toggle(
                          label: '${element.label} visible',
                          value: shown,
                          onChanged: (v) =>
                              apply(layout.withVisibility(element, v)),
                        ),
                      const SizedBox(width: AppDesign.spaceSm),
                      ReorderableDragStartListener(
                        index: index,
                        child: Semantics(
                          label: 'Reorder ${element.label}',
                          onDecrease: index == 0
                              ? null
                              : () => apply(
                                  layout.withOrder(_moveRow(rows, index, -1))),
                          onIncrease: index == rows.length - 1
                              ? null
                              : () => apply(
                                  layout.withOrder(_moveRow(rows, index, 1))),
                          child: SizedBox(
                            width: AppDesign.touchTarget,
                            height: AppDesign.touchTarget,
                            child: Icon(Icons.drag_handle,
                                size: AppDesign.iconSize,
                                color: palette.mutedText),
                          ),
                        ),
                      ),
                    ]),
                  ),
                );
              },
              onReorderItem: (oldIndex, newIndex) async {
                final next = [...rows];
                final element = next.removeAt(oldIndex);
                next.insert(newIndex, element);
                await apply(layout.withOrder(next));
              },
            ),
            _PrefRow(
              label: 'Meter',
              desc: StripElement.meter.description,
              trailing: _Toggle(
                label: 'Meter visible',
                value: layout.shows(StripElement.meter),
                onChanged: (v) =>
                    apply(layout.withVisibility(StripElement.meter, v)),
              ),
            ),
            _PrefRow(
              label: 'Fader size',
              desc: layout.faderWeight.toStringAsFixed(1),
              trailing: SizedBox(
                width: AppDesign.compactSelectMaxWidth,
                child: Slider(
                  value: layout.faderWeight,
                  min: 1,
                  max: 6,
                  divisions: 10,
                  label: layout.faderWeight.toStringAsFixed(1),
                  onChanged: (v) => prefs
                      .setStripLayoutDebounced(layout.copyWith(faderWeight: v)),
                ),
              ),
            ),
            _PrefRow(
              label: 'Compact processing',
              desc: 'EQ-only preview on all strips',
              trailing: _Toggle(
                label: 'Compact processing',
                value: layout.compactProcessing,
                onChanged: (v) =>
                    apply(layout.copyWith(compactProcessing: v)),
              ),
            ),
            const SizedBox(height: AppDesign.spaceSm),
          ],
        );
      },
    );
  }
}

/// Move one row without letting a screen-reader action walk past the ends.
List<StripElement> _moveRow(List<StripElement> rows, int index, int delta) {
  final next = [...rows];
  final element = next.removeAt(index);
  next.insert((index + delta).clamp(0, next.length).toInt(), element);
  return next;
}

class _PresetChip extends StatelessWidget {
  final String label;
  final bool selected;
  final FutureOr<void> Function() onTap;
  const _PresetChip(
      {required this.label, required this.selected, required this.onTap});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: '$label preset${selected ? ', selected' : ''}',
        onTap: () => _runSettingsAction(context, onTap),
        child: GestureDetector(
          onTap: () => _runSettingsAction(context, onTap),
          behavior: HitTestBehavior.opaque,
          child: Container(
            constraints: const BoxConstraints(minWidth: 44, minHeight: 44),
            padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              color: selected
                  ? ConsolePalette.of(context).accent
                  : ConsolePalette.of(context).control,
              borderRadius: BorderRadius.circular(9),
              border: Border.all(
                color: selected
                    ? ConsolePalette.of(context).accent
                    : ConsolePalette.of(context).lineCtl,
              ),
            ),
            child: Text(
              label,
              style: TextStyle(
                fontFamily: AppDesign.font,
                fontSize: AppDesign.rowLabel,
                fontWeight: FontWeight.w500,
                color: selected
                    ? ConsolePalette.of(context).onAccent
                    : ConsolePalette.of(context).text,
              ),
            ),
          ),
        ),
      );
}
