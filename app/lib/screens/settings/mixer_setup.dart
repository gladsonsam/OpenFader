part of '../settings.dart';

class _MixerSetupDetail extends StatelessWidget {
  final SettingsStore store;
  const _MixerSetupDetail({required this.store});

  @override
  Widget build(BuildContext context) {
    if (!store.mixerConnected) {
      return const _Note('Connect to a mixer to view its setup.');
    }
    final controls = store.mixerSetupControls;
    if (controls.isEmpty) {
      return const _Note('No setup controls are available for this mixer.');
    }
    final surfaces = controls.map((row) => row.surface).toSet();
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      const _Note('These settings apply to the connected mixer.'),
      for (final surface in surfaces) ...[
        _Section(
            title: switch (surface) {
              'bus_link' || 'bus_link_state' => 'Bus links',
              'link_policy_state' || 'link_policy' => 'Link preferences',
              'usb_interface' => 'Audio / USB',
              'safe_levels' => 'Safe Levels',
              'clock_rate' => 'Clock',
              'midi_setup' => 'MIDI',
              'mute_behavior' => 'Mute behavior',
              _ => 'Automix',
            },
            children: [
              for (final control
                  in controls.where((row) => row.surface == surface))
                _PrefRow(
                    label: control.label,
                    trailing: surface == 'bus_link_state' ||
                            surface == 'link_policy_state'
                        ? _ValueText(control.valueLabel ?? 'Unavailable')
                        : OutlinedButton(
                            onPressed: store.mixerSetupControlIsCurrent(control)
                                ? () => showDialog<void>(
                                    animationStyle: AnimationStyle.noAnimation,
                                    context: context,
                                    builder: (_) => DestinationControlEditor(
                                        changes: store,
                                        accessEpoch: () =>
                                            store.controlAccessEpoch,
                                        isCurrent:
                                            store.mixerSetupControlIsCurrent,
                                        apply: store.setMixerSetupControl,
                                        control: control))
                                : null,
                            child: Text(control.valueLabel ?? 'Unavailable'))),
            ]),
        if (surface == 'clock_rate')
          const _Note(
              'Selects the mixer sample rate. Mute Main LR before changing it.'),
        if (surface == 'safe_levels')
          const _Note('Mutes outputs when the mixer powers on.'),
        if (surface == 'usb_interface')
          const _Note(
              'Changes the connected computer’s USB audio channel count.'),
        if (surface == 'bus_link_state')
          const _Note('Bus link status is read-only.'),
        if (surface == 'link_policy_state')
          const _Note('Link preference status is read-only.'),
      ],
    ]);
  }
}
