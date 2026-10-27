part of '../settings.dart';

class _MixesDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function({
    required String title,
    required String body,
    required String action,
    required Future<void> Function() onConfirm,
  }) confirm;
  final VoidCallback onOpenMixer;

  const _MixesDetail({
    required this.store,
    required this.toast,
    required this.confirm,
    required this.onOpenMixer,
  });

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final mixes = store.mixes.mixes;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(
          first: true,
          title: 'Local mixes',
          children: [
            if (mixes.isEmpty)
              Padding(
                padding: EdgeInsets.symmetric(
                  horizontal: wide ? 26 : 16,
                  vertical: wide ? 32 : 22,
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'No mixes saved yet',
                      style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: wide ? 27 : 18,
                        height: wide ? 1.25 : 24 / 18,
                        fontWeight: FontWeight.w500,
                        color: ConsolePalette.of(context).text1,
                      ),
                    ),
                    SizedBox(height: wide ? 7 : 4),
                    Text(
                      'Import a mix, or open the offline mixer to prepare one.',
                      style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: wide ? 23 : 15,
                        height: wide ? 1.4 : 21 / 15,
                        color: ConsolePalette.of(context).text7,
                      ),
                    ),
                  ],
                ),
              )
            else
              for (final m in mixes)
                _MixRow(
                  mix: m,
                  onExport: () async {
                    await store.exportMix(m);
                    toast('Mix exported');
                  },
                  onDelete: () => confirm(
                    title: 'Delete ${m.name}?',
                    body:
                        'This local mix will be removed from this device. Your connected mixer will not change.',
                    action: 'Delete mix',
                    onConfirm: () async {
                      await store.deleteMix(m);
                      toast('Mix deleted');
                    },
                  ),
                ),
          ],
        ),
        Padding(
          padding: EdgeInsets.only(top: wide ? 20 : 16),
          child: Wrap(
            spacing: wide ? 14 : 10,
            runSpacing: wide ? 14 : 10,
            children: [
              _PrimaryButton(
                label: 'Import mix',
                icon: Icons.upload_outlined,
                onTap: () async {
                  try {
                    final err = await store.importMix();
                    if (err == null) {
                      toast('Mix imported');
                    } else if (err != 'cancelled') {
                      toast(err);
                    }
                  } catch (error) {
                    toast('$error');
                  }
                },
              ),
              _PlainButton(label: 'Open offline mixer', onTap: onOpenMixer),
            ],
          ),
        ),
        _Section(
          title: 'Backups',
          children: [
            _PrefRow(
              label: 'Back up changes automatically',
              trailing: _Toggle(
                label: 'Back up changes automatically',
                value: store.prefs.autoBackup,
                onChanged: (v) async {
                  await store.prefs.setAutoBackup(v);
                  if (v) {
                    await store.mixes.snapshotBackup();
                    await store.prefs.setBackupCreatedAt(
                      DateTime.now().toIso8601String(),
                    );
                  }
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Last local backup',
              trailing: _ValueText(
                store.prefs.backupCreatedAt.isEmpty
                    ? 'Not created'
                    : 'Saved on this device',
              ),
            ),
          ],
        ),
        Padding(
          padding: EdgeInsets.only(top: wide ? 20 : 16),
          child: Wrap(
            spacing: wide ? 14 : 10,
            runSpacing: wide ? 14 : 10,
            children: [
              _PlainButton(
                label: 'Export backup',
                icon: Icons.download_outlined,
                onTap: () async {
                  await store.exportBackup();
                  toast('Backup exported');
                },
              ),
              _PlainButton(
                label: 'Import backup',
                icon: Icons.upload_outlined,
                onTap: () async {
                  final res = await store.importBackup();
                  if (res == 'cancelled') return;
                  final n = int.tryParse(res);
                  if (n == null) {
                    toast(res);
                  } else if (n == 0) {
                    toast('Nothing valid to import');
                  } else {
                    toast('$n mix${n == 1 ? '' : 'es'} imported');
                  }
                },
              ),
            ],
          ),
        ),
        const _Note(
          'Backups contain local mixes only. Processor presets and local scenes/shows are not included. Keep an exported backup somewhere safe before deleting mixes or resetting app data.',
        ),
      ],
    );
  }
}

class _MixRow extends StatelessWidget {
  final LocalMix mix;
  final FutureOr<void> Function() onExport;
  final FutureOr<void> Function() onDelete;
  const _MixRow({
    required this.mix,
    required this.onExport,
    required this.onDelete,
  });

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
        horizontal: wide ? 26 : 16,
        vertical: wide ? 22 : 16,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  mix.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 27 : 18,
                    fontWeight: FontWeight.w500,
                    color: ConsolePalette.of(context).text1,
                  ),
                ),
                SizedBox(height: wide ? 7 : 4),
                Text(
                  mix.mixerModel,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 23 : 14,
                    color: ConsolePalette.of(context).text7,
                  ),
                ),
              ],
            ),
          ),
          _IconBtnInline(
            tooltip: 'Export ${mix.name}',
            onTap: onExport,
            icon: Icons.download_outlined,
          ),
          const SizedBox(width: 6),
          _IconBtnInline(tooltip: 'Delete ${mix.name}', onTap: onDelete),
        ],
      ),
    );
  }
}
