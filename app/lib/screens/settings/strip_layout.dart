part of '../settings.dart';

/// Channel-strip composition: presets, per-element visibility, fader size,
/// order, and compact processing. Self-listens on [PrefsService] so edits
/// apply here immediately; the mixer page's own prefs listener applies them
/// to live strips.
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
        Future<void> apply(StripLayout next) async {
          await prefs.setStripLayout(next);
          toast('Preference saved');
        }

        return _Section(
          title: 'Channel strips',
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
              child: Wrap(
                spacing: 8,
                runSpacing: 8,
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
            for (final el in StripElement.values)
              if (el == StripElement.fader)
                _PrefRow(
                  label: el.label,
                  desc: 'Always on',
                  trailing: _ValueText('On'),
                )
              else
                _PrefRow(
                  label: el == StripElement.meter
                      ? 'Meter (in fader)'
                      : el.label,
                  trailing: _Toggle(
                    label: '${el.label} visible',
                    value: layout.visible.contains(el),
                    onChanged: (v) async {
                      final visible = {...layout.visible};
                      if (v) {
                        visible.add(el);
                      } else {
                        visible.remove(el);
                      }
                      await apply(layout.copyWith(visible: visible));
                    },
                  ),
                ),
            _PrefRow(
              label: 'Fader size',
              desc: layout.faderWeight.toStringAsFixed(1),
              trailing: SizedBox(
                width: 174,
                child: Slider(
                  value: layout.faderWeight,
                  min: 1,
                  max: 6,
                  divisions: 10,
                  label: layout.faderWeight.toStringAsFixed(1),
                  onChanged: (v) => prefs.setStripLayoutDebounced(
                      layout.copyWith(faderWeight: v)),
                ),
              ),
            ),
            _PrefRow(
              label: 'Compact processing',
              desc: 'EQ-only preview on all strips',
              trailing: _Toggle(
                label: 'Compact processing',
                value: layout.compactProcessing,
                onChanged: (v) async => apply(
                    layout.copyWith(compactProcessing: v)),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 14, 16, 6),
              child: Text(
                'Strip order (top to bottom)',
                style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 15,
                  color: ConsolePalette.of(context).text7,
                ),
              ),
            ),
            ReorderableListView.builder(
              shrinkWrap: true,
              physics: const NeverScrollableScrollPhysics(),
              itemCount:
                  layout.order.where((el) => el != StripElement.meter).length,
              itemBuilder: (context, i) {
                final order =
                    layout.order.where((el) => el != StripElement.meter).toList();
                final el = order[i];
                return ListTile(
                  key: ValueKey(el.key),
                  dense: true,
                  title: Text(
                    el.label,
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 17,
                      color: layout.visible.contains(el) ||
                              el == StripElement.fader
                          ? ConsolePalette.of(context).text1
                          : ConsolePalette.of(context).text7,
                    ),
                  ),
                  trailing: ReorderableDragStartListener(
                    index: i,
                    child: Icon(
                      Icons.drag_handle,
                      color: ConsolePalette.of(context).text7,
                    ),
                  ),
                );
              },
              onReorderItem: (oldIndex, newIndex) async {
                final order =
                    layout.order.where((el) => el != StripElement.meter).toList();
                final el = order.removeAt(oldIndex);
                order.insert(newIndex, el);
                // Meter stays fused to the fader; keep it out of the row order.
                await apply(layout.copyWith(order: order));
              },
            ),
            const SizedBox(height: 8),
          ],
        );
      },
    );
  }
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
            padding:
                const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
            decoration: BoxDecoration(
              color: selected
                  ? ConsolePalette.of(context).cyan
                  : ConsolePalette.of(context).control,
              borderRadius: BorderRadius.circular(9),
              border: Border.all(
                color: selected
                    ? ConsolePalette.of(context).cyan
                    : ConsolePalette.of(context).lineCtl,
              ),
            ),
            child: Text(
              label,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: 17,
                fontWeight: FontWeight.w500,
                color: selected
                    ? ConsolePalette.of(context).onAccent
                    : ConsolePalette.of(context).text1,
              ),
            ),
          ),
        ),
      );
}
