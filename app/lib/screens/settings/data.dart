part of '../settings.dart';

class _DataDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function({
    required String title,
    required String body,
    required String action,
    required Future<void> Function() onConfirm,
  }) confirm;

  const _DataDetail({
    required this.store,
    required this.toast,
    required this.confirm,
  });

  static String _bytes(int b) =>
      b < 1024 ? '$b B' : '${(b / 1024).toStringAsFixed(1)} KB';

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(
          first: true,
          title: 'On this device',
          children: [
            _PrefRow(
              label: 'Local mixes',
              trailing: _ValueText('${store.mixes.mixes.length}'),
            ),
            _PrefRow(
              label: 'Saved mixers',
              trailing: _ValueText('${store.recents.items.length}'),
            ),
            _PrefRow(
              label: 'App storage',
              trailing: FutureBuilder<int>(
                future: store.appStorageBytes(),
                builder: (context, snap) => _ValueText(
                  snap.hasError
                      ? 'Unavailable'
                      : (snap.hasData ? _bytes(snap.data!) : 'Calculating'),
                ),
              ),
            ),
          ],
        ),
        _Section(
          title: 'Manage data',
          children: [
            _ActionRow(
              label: 'Clear cache',
              desc: 'Remove temporary app files. Your mixes stay saved.',
              onTap: () => confirm(
                title: 'Clear cache?',
                body:
                    'Temporary app files will be removed. Local mixes, scenes/shows, saved mixers and preferences will stay.',
                action: 'Clear cache',
                onConfirm: () async {
                  await store.clearCache();
                  toast('Cache cleared');
                },
              ),
            ),
            _ActionRow(
              label: 'Forget saved mixers',
              desc: 'Remove mixer names and addresses from this device.',
              onTap: () => confirm(
                title: 'Forget saved mixers?',
                body:
                    '${store.recents.items.length} saved mixer(s) will be removed from this device. Your mixes will stay.',
                action: 'Forget mixers',
                onConfirm: () async {
                  await store.forgetMixers();
                  toast('Saved mixers forgotten');
                },
              ),
            ),
            _ActionRow(
              label: 'Reset preferences',
              desc: 'Restore display and connection defaults.',
              onTap: () => confirm(
                title: 'Reset preferences?',
                body:
                    'Display, meter and connection preferences will return to their defaults. Saved mixes and mixer addresses will stay.',
                action: 'Reset preferences',
                onConfirm: () async {
                  await store.resetPrefs();
                  toast('Preferences reset');
                },
              ),
            ),
          ],
        ),
        _Section(
          title: 'Reset',
          children: [
            _ActionRow(
              label: 'Delete local mixes',
              desc:
                  'Remove local mixes and backups; keep scenes/shows and processor presets.',
              danger: true,
              onTap: () => confirm(
                title: 'Delete local mixes?',
                body:
                    '${store.mixes.mixes.length} local mix(es) and local backups will be removed. Scenes/shows and processor presets will stay. Export a backup first if you want to keep them.',
                action: 'Delete mixes',
                onConfirm: () async {
                  await store.deleteMixes();
                  toast('Local mixes deleted');
                },
              ),
            ),
            _ActionRow(
              label: 'Reset all app data',
              desc:
                  'Remove local mixes, scenes/shows, processor presets and preferences.',
              danger: true,
              onTap: () => confirm(
                title: 'Reset all app data?',
                body:
                    'Local scenes/shows will be reset through the engine first, then local mixes, backups, processor presets, saved mixers, cache and preferences will be removed. A connected mixer will not be reset.',
                action: 'Reset app data',
                onConfirm: () async {
                  await store.fullReset();
                  toast('App data reset');
                },
              ),
            ),
          ],
        ),
        const _Note(
          'These actions affect this device. They do not reset a connected mixer.',
        ),
      ],
    );
  }
}
