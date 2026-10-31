import 'package:flutter/material.dart';

import '../mixer/controller.dart';
import '../palette.dart';

/// Workspace tools have a stable home instead of competing with mixer controls.
class WorkspaceHome extends StatelessWidget {
  final VoidCallback onMixer;
  final ValueChanged<MixerDestination> onNavigate;
  final ValueChanged<BuildContext> onLayout;
  const WorkspaceHome(
      {super.key,
      required this.onMixer,
      required this.onNavigate,
      required this.onLayout});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    Widget tile(String title, String subtitle, IconData icon,
            VoidCallback action) =>
        OutlinedButton(
            onPressed: action,
            style: OutlinedButton.styleFrom(
                padding:
                    const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                alignment: Alignment.centerLeft,
                backgroundColor: palette.panel,
                foregroundColor: palette.text1,
                side: BorderSide(color: palette.linePanel),
                shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(10))),
            child: Row(children: [
              Icon(icon, size: 24, color: palette.mutedText),
              const SizedBox(width: 16),
              Expanded(
                  child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                    Text(title,
                        style: const TextStyle(
                            fontSize: 17, fontWeight: FontWeight.w600)),
                    const SizedBox(height: 4),
                    Text(subtitle,
                        style:
                            TextStyle(fontSize: 13, color: palette.mutedText)),
                  ])),
            ]));
    return LayoutBuilder(builder: (context, bounds) {
      final columns = bounds.maxWidth >= 1000
          ? 3
          : bounds.maxWidth >= 650
              ? 2
              : 1;
      return SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Center(
            child: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 1100),
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      GridView.count(
                          crossAxisCount: columns,
                          mainAxisSpacing: 12,
                          crossAxisSpacing: 12,
                          mainAxisExtent: (columns == 1 ? 84.0 : 96.0) *
                              MediaQuery.textScalerOf(context).scale(1),
                          shrinkWrap: true,
                          physics: const NeverScrollableScrollPhysics(),
                          children: [
                            tile('Mixer', 'Channels, faders and mixes',
                                Icons.tune, onMixer),
                            tile(
                                'Routing',
                                'Patch sources to destinations',
                                Icons.alt_route,
                                () => onNavigate(MixerDestination.routing)),
                            tile(
                                'Meters',
                                'Watch input and output signals',
                                Icons.graphic_eq,
                                () => onNavigate(MixerDestination.meters)),
                            tile(
                                'Effects',
                                'Edit effect processors',
                                Icons.multitrack_audio,
                                () => onNavigate(MixerDestination.fx)),
                            tile(
                                'Scenes',
                                'Save and recall mixes',
                                Icons.layers_outlined,
                                () => onNavigate(MixerDestination.scenes)),
                            Builder(
                                builder: (anchor) => tile(
                                    'Mixer layout',
                                    'Banks, layers and master bus',
                                    Icons.view_column_outlined,
                                    () => onLayout(anchor))),
                            tile(
                                'Connections',
                                'Connect a mixer or work offline',
                                Icons.router_outlined,
                                () => onNavigate(MixerDestination.connect)),
                            tile(
                                'Settings',
                                'Appearance and app preferences',
                                Icons.settings_outlined,
                                () => onNavigate(MixerDestination.settings)),
                          ]),
                      const SizedBox(height: 16),
                      Align(
                          alignment: Alignment.centerLeft,
                          child: TextButton(
                              onPressed: () =>
                                  onNavigate(MixerDestination.debug),
                              style: TextButton.styleFrom(
                                  backgroundColor: Colors.transparent,
                                  foregroundColor: palette.mutedText),
                              child: const Text('Diagnostics'))),
                    ]))),
      );
    });
  }
}
