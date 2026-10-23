// Area D: settings screen. Behaviour prefs (segmented/toggle rows), the offline queue
// (push/discard) and engine stats.
//
// Wiring: everything flows through [SettingsStore] (constructor injection
// — Area A implements it over the engine/prefs; this file only renders ui4
// tokens, see lib/theme.dart). No Material Card/AppBar/Switch/Slider;
// segmented options and toggles are plain text buttons.
import 'package:flutter/material.dart';

import '../theme.dart';
import '../widgets/console_button.dart';

/// One pickable value in a setting row (On/Off, 8/12/16, mix 1..16…).
class SettingOption {
  final String label;
  final String value;
  final bool pressed;

  const SettingOption({
    required this.label,
    required this.value,
    this.pressed = false,
  });
}

/// One descriptor-driven row (SETTING_DESCRIPTORS port: key/title/desc).
class SettingRowVm {
  final String key;
  final String label;
  final String desc;
  final bool isToggle; // 2-option Off/On rendered as one tap target
  final List<SettingOption> options;

  const SettingRowVm({
    required this.key,
    required this.label,
    required this.desc,
    this.isToggle = false,
    this.options = const [],
  });
}

class SettingsSectionVm {
  final String title;
  final List<SettingRowVm> rows;

  const SettingsSectionVm({required this.title, required this.rows});
}

/// One staged offline edit (engine `offline_diff` entry).
class OfflineChangeVm {
  final int strip;
  final String id;
  final String from;
  final String to;

  const OfflineChangeVm({
    required this.strip,
    required this.id,
    required this.from,
    required this.to,
  });
}

/// Minimal store contract the settings screen renders. Implemented by the
/// shell/owner over prefs + the live engine.
abstract class SettingsStore extends ChangeNotifier {
  List<SettingsSectionVm> get sections;
  List<OfflineChangeVm> get offlineChanges;
  String get stats; // engine `stats` text, '' = never fetched
  String? get notice; // last action feedback (push/discard/errors)

  void setSetting(String key, String value);
  void offlinePush(); // cmd offline_push
  void offlineDiscard(); // cmd offline_discard
  void requestStats(); // cmd stats
}

class U4SettingsPage extends StatelessWidget {
  final SettingsStore store;

  const U4SettingsPage({super.key, required this.store});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: store,
          builder: (context, _) {
            return SingleChildScrollView(
              padding: const EdgeInsets.fromLTRB(12, 10, 12, 16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  const Text('SETTINGS',
                      style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.6,
                          color: U4.text1)),
                  const SizedBox(height: 10),
                  for (final sec in store.sections) ...[
                    Text(sec.title,
                        style: const TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w700,
                            color: U4.text4)),
                    const SizedBox(height: 6),
                    Container(
                      margin: const EdgeInsets.only(bottom: 12),
                      decoration: BoxDecoration(
                        color: U4.panel,
                        border: Border.all(color: U4.linePanel),
                        borderRadius:
                            const BorderRadius.all(Radius.circular(10)),
                      ),
                      child: Column(
                        children: [
                          for (var i = 0; i < sec.rows.length; i++) ...[
                            _SettingRow(
                              row: sec.rows[i],
                              onPick: (v) =>
                                  store.setSetting(sec.rows[i].key, v),
                            ),
                            if (i != sec.rows.length - 1)
                              const Divider(color: U4.divider, height: 1),
                          ],
                        ],
                      ),
                    ),
                  ],
                  _OfflinePanel(store: store),
                  const SizedBox(height: 12),
                  _StatsPanel(store: store),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _SettingRow extends StatelessWidget {
  final SettingRowVm row;
  final ValueChanged<String> onPick;

  const _SettingRow({required this.row, required this.onPick});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(row.label,
                    style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                        color: U4.text1)),
                const SizedBox(height: 2),
                Text(row.desc,
                    style: const TextStyle(fontSize: 12, color: U4.text7)),
              ],
            ),
          ),
          const SizedBox(width: 10),
          if (row.options.isNotEmpty)
            Wrap(
              spacing: 4,
              children: [
                for (final o in row.options)
                  ConsoleButton(
                      label: o.label,
                      selected: o.pressed,
                      onPressed: () => onPick(o.value)),
              ],
            ),
        ],
      ),
    );
  }
}

class _OfflinePanel extends StatelessWidget {
  final SettingsStore store;
  const _OfflinePanel({required this.store});

  @override
  Widget build(BuildContext context) {
    final n = store.offlineChanges.length;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: U4.panel,
        border: Border.all(color: U4.linePanel),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            n == 0
                ? 'Offline queue · empty'
                : 'Offline queue · $n change${n == 1 ? '' : 's'}',
            style: const TextStyle(
                fontSize: 14, fontWeight: FontWeight.w600, color: U4.text1),
          ),
          const SizedBox(height: 2),
          const Text(
            'Edits made while offline are staged here, never sent until you push.',
            style: TextStyle(fontSize: 12, color: U4.text7),
          ),
          if (n > 0) ...[
            const SizedBox(height: 8),
            for (final c in store.offlineChanges.take(8))
              Padding(
                padding: const EdgeInsets.only(bottom: 2),
                child: Text(
                  'strip ${c.strip} ${c.id}: ${c.from} → ${c.to}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(fontSize: 11, color: U4.text4),
                ),
              ),
            if (n > 8)
              Text('…and ${n - 8} more',
                  style: const TextStyle(fontSize: 11, color: U4.text7)),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: GestureDetector(
                    onTap: store.offlinePush,
                    child: Container(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      alignment: Alignment.center,
                      decoration: const BoxDecoration(
                        color: U4.cyan,
                        borderRadius: U4.rBtn,
                      ),
                      child: const Text('Push to desk',
                          textAlign: TextAlign.center,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                              fontSize: 13,
                              fontWeight: FontWeight.w700,
                              color: U4.well)),
                    ),
                  ),
                ),
                const SizedBox(width: 6),
                GestureDetector(
                  onTap: store.offlineDiscard,
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                        horizontal: 14, vertical: 12),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: U4.well,
                      border: Border.all(color: U4.lineCtl),
                      borderRadius: U4.rBtn,
                    ),
                    child: const Text('Discard',
                        textAlign: TextAlign.center,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: U4.text1)),
                  ),
                ),
              ],
            ),
          ],
          if (store.notice != null) ...[
            const SizedBox(height: 6),
            Text(store.notice!,
                style: const TextStyle(fontSize: 12, color: U4.meterAmber)),
          ],
        ],
      ),
    );
  }
}

class _StatsPanel extends StatelessWidget {
  final SettingsStore store;
  const _StatsPanel({required this.store});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: U4.panel,
        border: Border.all(color: U4.linePanel),
        borderRadius: const BorderRadius.all(Radius.circular(10)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Expanded(
                child: Text('Engine stats',
                    style: TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w600,
                        color: U4.text1)),
              ),
              GestureDetector(
                onTap: store.requestStats,
                child: Container(
                  padding:
                      const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                  decoration: BoxDecoration(
                    color: U4.well,
                    border: Border.all(color: U4.lineCtl),
                    borderRadius: U4.rBtn,
                  ),
                  child: const Text('Refresh',
                      textAlign: TextAlign.center,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          color: U4.text1)),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            store.stats.isEmpty ? 'never fetched' : store.stats,
            style: const TextStyle(fontSize: 11, color: U4.text4),
          ),
        ],
      ),
    );
  }
}
