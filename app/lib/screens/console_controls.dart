import 'package:material_ui/material_ui.dart';

import '../core/console_controls.dart';
import '../design.dart';
import '../palette.dart';
import '../ui/controls/console_button.dart';
import '../ui/overlays/console_dialogs.dart';

/// Browse only the controls and values reported by the connected provider.
class ConsoleControlsPage extends StatefulWidget {
  const ConsoleControlsPage({super.key, required this.store});
  final ConsoleControlsStore store;

  @override
  State<ConsoleControlsPage> createState() => _ConsoleControlsPageState();
}

class _ConsoleControlsPageState extends State<ConsoleControlsPage> {
  @override
  void initState() {
    super.initState();
    widget.store.open('/');
  }

  String _value(ConsoleControlItem item) {
    if (item.kind == ConsoleControlKind.action) return 'Action';
    if (!item.valueKnown) return 'Waiting for readback';
    if (item.kind == ConsoleControlKind.bitset && item.value is int) {
      final value = item.value! as int;
      final labels = item.choices
          .where(
            (choice) =>
                choice.value is int && (value & (choice.value as int)) != 0,
          )
          .map((choice) => choice.label)
          .join(', ');
      return labels.isNotEmpty
          ? labels
          : value == 0
          ? 'None'
          : 'Other selection';
    }
    if (item.kind == ConsoleControlKind.enumeration) {
      for (final choice in item.choices) {
        if (choice.value == item.value) return choice.label;
      }
      return 'Unknown value';
    }
    if (item.value is bool) return item.value == true ? 'On' : 'Off';
    final value = item.value;
    var display = '$value';
    if (value is num && value.isFinite) {
      display = value == value.roundToDouble()
          ? value.toStringAsFixed(0)
          : value.toStringAsPrecision(5);
      if (display.contains('.') && !display.contains('e')) {
        display = display
            .replaceFirst(RegExp(r'0+$'), '')
            .replaceFirst(RegExp(r'\.$'), '');
      }
    }
    return '$display${item.unit == null || item.unit!.isEmpty ? '' : ' ${item.unit}'}';
  }

  String? _reason(ConsoleControlItem item) {
    if (item.unavailableReason != null) return item.unavailableReason;
    if (item.readOnly) return 'Read only';
    if (item.revision == null) return 'Waiting for control revision';
    if (item.kind == ConsoleControlKind.action) {
      return item.choices.isEmpty ? 'Action unavailable' : null;
    }
    if (!item.valueKnown || item.value == null) return 'Waiting for readback';
    if ((item.kind == ConsoleControlKind.boolean && item.value is! bool) ||
        (item.kind == ConsoleControlKind.text && item.value is! String) ||
        ((item.kind == ConsoleControlKind.integer ||
                item.kind == ConsoleControlKind.number) &&
            (item.value is! num || !(item.value! as num).isFinite))) {
      return 'Current value unavailable';
    }
    if (item.kind == ConsoleControlKind.bitset &&
        (item.value is! int ||
            item.choices.isEmpty ||
            item.choices.any(
              (choice) => choice.value is! int || (choice.value as int) <= 0,
            ))) {
      return 'Bit choices unavailable';
    }
    if (item.kind == ConsoleControlKind.enumeration &&
        (!item.choices.any((choice) => choice.value == item.value) ||
            item.choices.isEmpty)) {
      return 'Current value or choices unavailable';
    }
    if ((item.kind == ConsoleControlKind.number ||
            item.kind == ConsoleControlKind.integer) &&
        (item.min == null ||
            item.max == null ||
            !item.min!.isFinite ||
            !item.max!.isFinite ||
            item.min! > item.max!)) {
      return 'Control range unavailable';
    }
    return null;
  }

  Future<void> _edit(ConsoleControlItem item) async {
    if (_reason(item) != null) return;
    Object? next;
    var selectedNullAction = false;
    if (item.kind == ConsoleControlKind.bitset) {
      var draft = item.value! as int;
      next = await showDialog<int>(
        context: context,
        builder: (context) => StatefulBuilder(
          builder: (context, setState) => ConsoleAlertDialog(
            backgroundColor: ConsolePalette.of(context).panel,
            title: Text(
              item.label,
              style: AppDesign.type(AppDesign.dialogTitle),
            ),
            scrollable: true,
            content: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final choice in item.choices)
                  CheckboxListTile(
                    title: Text(choice.label),
                    value: (draft & (choice.value as int)) != 0,
                    onChanged: (selected) => setState(() {
                      final mask = choice.value as int;
                      draft = selected == true ? draft | mask : draft & ~mask;
                    }),
                  ),
              ],
            ),
            actions: [
              ConsoleButton(
                label: 'Cancel',
                onPressed: () => Navigator.pop(context),
              ),
              ConsoleButton(
                label: 'Apply',
                primary: true,
                onPressed: () => Navigator.pop(context, draft),
              ),
            ],
          ),
        ),
      );
    } else if (item.kind == ConsoleControlKind.action &&
        item.choices.length == 1) {
      next = item.choices.single.value;
      selectedNullAction = next == null;
    } else if (item.kind == ConsoleControlKind.boolean && item.value is bool) {
      next = !(item.value! as bool);
    } else if (item.kind == ConsoleControlKind.enumeration ||
        item.kind == ConsoleControlKind.action) {
      final selected = await showDialog<ConsoleControlChoice>(
        context: context,
        builder: (context) => ConsoleAlertDialog(
          backgroundColor: ConsolePalette.of(context).panel,
          title: Text(item.label, style: AppDesign.type(AppDesign.dialogTitle)),
          scrollable: true,
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (final choice in item.choices)
                ListTile(
                  title: Text(choice.label),
                  selected: choice.value == item.value,
                  onTap: () => Navigator.pop(context, choice),
                ),
            ],
          ),
          actions: [
            ConsoleButton(
              label: 'Cancel',
              onPressed: () => Navigator.pop(context),
            ),
          ],
        ),
      );
      if (selected == null) return;
      next = selected.value;
      selectedNullAction =
          item.kind == ConsoleControlKind.action && next == null;
    } else if (item.kind == ConsoleControlKind.text ||
        item.kind == ConsoleControlKind.integer ||
        item.kind == ConsoleControlKind.number) {
      final draft = await showConsoleNameDialog(
        context,
        title: item.label,
        confirmLabel: 'Apply',
        initialValue: '${item.value}',
        fieldLabel: item.kind == ConsoleControlKind.text
            ? item.unit
            : '${item.min}–${item.max}${item.unit == null || item.unit!.isEmpty ? '' : ' ${item.unit}'}',
      );
      if (draft == null) return;
      if (item.kind == ConsoleControlKind.text) {
        next = draft;
      } else {
        final parsed = item.kind == ConsoleControlKind.integer
            ? int.tryParse(draft.trim())
            : double.tryParse(draft.trim());
        if (parsed == null ||
            !parsed.isFinite ||
            parsed < item.min! ||
            parsed > item.max!) {
          if (mounted) {
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(
                content: Text('Enter a value from ${item.min} to ${item.max}.'),
              ),
            );
          }
          return;
        }
        final step = item.step;
        if (step != null && step.isFinite && step > 0) {
          final offset = (parsed - item.min!) / step;
          if ((offset - offset.round()).abs() > 0.0001) {
            if (mounted) {
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(content: Text('Use increments of $step.')),
              );
            }
            return;
          }
        }
        next = parsed;
      }
    }
    if ((next == null && !selectedNullAction) ||
        (item.kind != ConsoleControlKind.action && next == item.value)) {
      return;
    }
    final result = await widget.store.write(item, next);
    if (mounted && result != null) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(result)));
    }
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.store,
      builder: (context, _) {
        final palette = ConsolePalette.of(context);
        final store = widget.store;
        return ColoredBox(
          color: palette.page,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: AppDesign.pagePadding,
                child: Row(
                  children: [
                    Expanded(
                      child: Wrap(
                        spacing: AppDesign.spaceXs,
                        children: [
                          for (final crumb in store.breadcrumbs)
                            TextButton(
                              onPressed: () => store.open(crumb.path),
                              child: Text(crumb.label),
                            ),
                        ],
                      ),
                    ),
                    IconButton(
                      tooltip: 'Refresh controls',
                      onPressed: store.loading ? null : store.refresh,
                      icon: const Icon(Icons.refresh),
                    ),
                  ],
                ),
              ),
              if (store.loading) const LinearProgressIndicator(),
              if (store.unavailableReason != null)
                Padding(
                  padding: AppDesign.pagePadding,
                  child: Text(
                    store.unavailableReason!,
                    style: AppDesign.type(
                      AppDesign.body,
                      color: palette.mutedText,
                    ),
                  ),
                ),
              if (store.receipt != null)
                Padding(
                  padding: AppDesign.pagePadding,
                  child: Text(
                    store.receipt!,
                    style: AppDesign.type(AppDesign.body, color: palette.text1),
                  ),
                ),
              Expanded(
                child:
                    store.items.isEmpty &&
                        !store.loading &&
                        store.unavailableReason == null
                    ? Center(
                        child: Text(
                          'No controls are available here.',
                          style: AppDesign.type(
                            AppDesign.body,
                            color: palette.mutedText,
                          ),
                        ),
                      )
                    : ListView.separated(
                        padding: AppDesign.pagePadding,
                        itemCount: store.items.length,
                        separatorBuilder: (_, _) =>
                            Divider(color: palette.linePanel, height: 1),
                        itemBuilder: (context, index) {
                          final item = store.items[index];
                          final branch = item.kind == ConsoleControlKind.branch;
                          final reason = branch
                              ? item.unavailableReason
                              : _reason(item);
                          return ListTile(
                            contentPadding: const EdgeInsets.symmetric(
                              horizontal: AppDesign.spaceSm,
                            ),
                            title: Text(
                              item.label,
                              style: AppDesign.type(
                                AppDesign.rowLabel,
                                color: palette.text1,
                              ),
                            ),
                            subtitle: branch
                                ? (reason == null ? null : Text(reason))
                                : Text(
                                    reason == null
                                        ? _value(item)
                                        : '${_value(item)} · $reason',
                                    style: AppDesign.type(
                                      AppDesign.body,
                                      color: palette.mutedText,
                                    ),
                                  ),
                            trailing: Icon(
                              branch
                                  ? Icons.chevron_right
                                  : reason != null
                                  ? Icons.lock_outline
                                  : item.kind == ConsoleControlKind.action
                                  ? Icons.play_arrow
                                  : Icons.edit_outlined,
                              color: palette.mutedText,
                            ),
                            onTap: reason != null
                                ? null
                                : branch
                                ? () => store.open(item.path)
                                : () => _edit(item),
                          );
                        },
                      ),
              ),
            ],
          ),
        );
      },
    );
  }
}
