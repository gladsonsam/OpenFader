part of '../settings.dart';

class _AppearanceDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  const _AppearanceDetail({required this.store, required this.toast});

  @override
  Widget build(BuildContext context) {
    final prefs = store.prefs;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(
          first: true,
          title: 'Display',
          children: [
            _PrefRow(
              label: 'Appearance',
              trailing: _Select(
                label: 'Appearance',
                value: prefs.appearance,
                options: const [
                  ('dark', 'Dark'),
                  ('light', 'Light'),
                  ('system', 'System'),
                ],
                onChanged: (v) async {
                  await prefs.setAppearance(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Accent color',
              trailing: _Select(
                label: 'Accent color',
                value: prefs.accent,
                options: const [
                  ('lime', 'Lime'),
                  ('ocean', 'Ocean'),
                  ('violet', 'Violet'),
                  ('amber', 'Amber'),
                ],
                onChanged: (v) async {
                  await prefs.setAccent(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Interface size',
              trailing: _Select(
                label: 'Interface size',
                value: prefs.interfaceSize,
                options: const [
                  ('comfortable', 'Comfortable'),
                  ('compact', 'Compact'),
                  ('large', 'Large'),
                ],
                onChanged: (v) async {
                  await prefs.setInterfaceSize(v);
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Keep screen awake',
              trailing: _Toggle(
                label: 'Keep screen awake',
                value: prefs.keepAwake,
                onChanged: (v) async {
                  await prefs.setKeepAwake(v);
                  toast('Preference saved');
                },
              ),
            ),
          ],
        ),
        _Section(
          title: 'Meters',
          children: [
            _PrefRow(
              label: 'Refresh rate',
              trailing: _Select(
                label: 'Refresh rate',
                value: '${prefs.meterRate}',
                options: const [('30', '30 fps'), ('60', '60 fps')],
                onChanged: (v) async {
                  await prefs.setMeterRate(int.parse(v));
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Peak hold',
              trailing: _Select(
                label: 'Peak hold',
                value: '${prefs.peakHold}',
                options: const [
                  ('1', '1 second'),
                  ('3', '3 seconds'),
                  ('5', '5 seconds'),
                ],
                onChanged: (v) async {
                  await prefs.setPeakHold(int.parse(v));
                  toast('Preference saved');
                },
              ),
            ),
            _PrefRow(
              label: 'Hold clip indicators',
              trailing: _Toggle(
                label: 'Hold clip indicators',
                value: prefs.holdClips,
                onChanged: (v) async {
                  await prefs.setHoldClips(v);
                  toast('Preference saved');
                },
              ),
            ),
          ],
        ),
        _Section(
          title: 'Touch controls',
          children: [
            _PrefRow(
              label: 'Larger fader touch targets',
              trailing: _Toggle(
                label: 'Larger fader touch targets',
                value: prefs.largeTargets,
                onChanged: (v) async {
                  await prefs.setLargeTargets(v);
                  toast('Preference saved');
                },
              ),
            ),
          ],
        ),
        _StripLayoutSection(store: store, toast: toast),
        _LayersSection(store: store, toast: toast),
      ],
    );
  }
}
