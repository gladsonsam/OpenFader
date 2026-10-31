part of '../settings.dart';

class _ConnectionsDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function({
    required String title,
    required String body,
    required String action,
    required Future<void> Function() onConfirm,
  }) confirm;

  const _ConnectionsDetail({
    required this.store,
    required this.toast,
    required this.confirm,
  });

  @override
  Widget build(BuildContext context) {
    final prefs = store.prefs;
    final recents = store.recents.items;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(
          first: true,
          title: 'Connection behaviour',
          children: [
            _PrefRow(
              label: 'Find mixers automatically',
              trailing: _Toggle(
                label: 'Find mixers automatically',
                value: prefs.autoFind,
                onChanged: (v) async {
                  await prefs.setAutoFind(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Reconnect to the last mixer',
              trailing: _Toggle(
                label: 'Reconnect to the last mixer',
                value: prefs.reconnectLast,
                onChanged: (v) async {
                  await prefs.setReconnectLast(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Remember recent mixers',
              trailing: _Toggle(
                label: 'Remember recent mixers',
                value: prefs.rememberRecents,
                onChanged: (v) async {
                  await prefs.setRemember(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Start on',
              trailing: _Select(
                label: 'Start on',
                value: prefs.startupDest,
                options: const [
                  ('connections', 'Connections'),
                  ('last', 'Last mixer'),
                ],
                onChanged: (v) async {
                  await prefs.setStartupDest(v);
                  toast('Preference saved');
                },
              ),
            ),
          ],
        ),
        _Section(
          title: 'Saved mixers',
          children: [
            if (recents.isEmpty)
              const _PrefRow(
                label: 'No saved mixers',
                desc: 'Mixers you connect to will appear here.',
                trailing: SizedBox.shrink(),
              ),
            for (final r in recents)
              _SavedMixerRow(
                name: r.name.isEmpty ? r.addr : r.name,
                addr: r.addr,
                onForget: () => confirm(
                  title: 'Forget ${r.name.isEmpty ? r.addr : r.name}?',
                  body:
                      'Remove this saved address from this device. Your mixes will stay.',
                  action: 'Forget mixer',
                  onConfirm: () async {
                    await store.recents.forget(r.addr);
                    toast('Mixer forgotten');
                  },
                ),
              ),
          ],
        ),
        const _Note('Your device and mixer must be on the same network.'),
        _Section(
          title: 'Supported mixers',
          children: const [
            _PrefRow(label: 'XR18', trailing: _ValueText('Behringer')),
            _PrefRow(label: 'DL32S', trailing: _ValueText('Mackie')),
          ],
        ),
      ],
    );
  }
}

class _SavedMixerRow extends StatelessWidget {
  final String name;
  final String addr;
  final FutureOr<void> Function() onForget;
  const _SavedMixerRow({
    required this.name,
    required this.addr,
    required this.onForget,
  });

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
        horizontal: 16,
        vertical: 12,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 17,
                    height: wide ? 1.25 : 24 / 18,
                    fontWeight: FontWeight.w500,
                    color: ConsolePalette.of(context).text1,
                  ),
                ),
                SizedBox(height: wide ? 7 : 4),
                Text(
                  addr,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 14,
                    height: wide ? 1.4 : 1.5,
                    color: ConsolePalette.of(context).text7,
                    fontFeatures: const [FontFeature.tabularFigures()],
                  ),
                ),
              ],
            ),
          ),
          _IconBtnInline(tooltip: 'Forget $name', onTap: onForget),
        ],
      ),
    );
  }
}
