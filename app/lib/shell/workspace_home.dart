import 'package:material_ui/material_ui.dart';

import '../mixer/controller.dart';
import '../ui/console.dart';

const _twoColumnWidth = 650.0;
const _threeColumnWidth = 1000.0;

/// Workspace tools have a stable home instead of competing with mixer controls.
class WorkspaceHome extends StatelessWidget {
  final VoidCallback onMixer;
  final VoidCallback onConnections;
  final VoidCallback onConsoleControls;
  final VoidCallback onLayers;
  final VoidCallback? onRecorder;
  final VoidCallback? onQuit;
  final VoidCallback? onLeaveServer;
  final String leaveServerLabel;
  final String serverError;
  final VoidCallback? onReloadServer;
  final ValueChanged<MixerDestination> onNavigate;
  final ValueChanged<BuildContext> onLayout;
  const WorkspaceHome(
      {super.key,
      required this.onMixer,
      required this.onConnections,
      required this.onConsoleControls,
      required this.onLayers,
      this.onRecorder,
      this.onQuit,
      this.onLeaveServer,
      this.leaveServerLabel = 'Disconnect server',
      this.serverError = '',
      this.onReloadServer,
      required this.onNavigate,
      required this.onLayout});

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    Widget tile(String title, String subtitle, IconData icon,
            VoidCallback action) =>
        ConsoleNavigationTile(
            label: title,
            detail: subtitle,
            leading: Icon(icon),
            showChevron: false,
            outlined: true,
            onTap: action);
    return LayoutBuilder(builder: (context, bounds) {
      final columns = bounds.maxWidth >= _threeColumnWidth
          ? 3
          : bounds.maxWidth >= _twoColumnWidth
              ? 2
              : 1;
      return SingleChildScrollView(
        padding: const EdgeInsets.all(AppDesign.spaceLg),
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
                            if (onRecorder != null)
                              tile(
                                  'Recorder & takes',
                                  'Recording and virtual soundcheck',
                                  Icons.fiber_manual_record,
                                  onRecorder!),
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
                                Icons.bookmarks_outlined,
                                () => onNavigate(MixerDestination.scenes)),
                            tile('Layers', 'Switch or edit mixer layers',
                                Icons.layers_outlined, onLayers),
                            Builder(
                                builder: (anchor) => tile(
                                    'Banks & groups',
                                    'Banks, groups and master bus',
                                    Icons.view_column_outlined,
                                    () => onLayout(anchor))),
                            tile(
                                'Console controls',
                                'Browse controls from the connected mixer',
                                Icons.tune_outlined,
                                onConsoleControls),
                            tile(
                                'Connections',
                                'Connect a mixer or work offline',
                                Icons.router_outlined,
                                onConnections),
                          ]),
                      const SizedBox(height: AppDesign.spaceLg),
                      if (serverError.isNotEmpty) ...[
                        Text('Server connection: $serverError',
                            style: TextStyle(color: palette.text)),
                        if (onReloadServer != null)
                          Align(
                              alignment: Alignment.centerLeft,
                              child: ConsoleButton(
                                  label: 'Reload / sign in',
                                  variant: ConsoleButtonVariant.quiet,
                                  onPressed: onReloadServer)),
                        const SizedBox(height: AppDesign.spaceSm),
                      ],
                      Row(children: [
                        Expanded(
                            child: Align(
                                alignment: Alignment.centerLeft,
                                child: ConsoleButton(
                                    label: 'Diagnostics',
                                    variant: ConsoleButtonVariant.quiet,
                                    accent: palette.mutedText,
                                    onPressed: () =>
                                        onNavigate(MixerDestination.debug)))),
                        if (onLeaveServer != null)
                          ConsoleButton(
                              label: leaveServerLabel,
                              variant: ConsoleButtonVariant.quiet,
                              onPressed: onLeaveServer),
                        if (onQuit != null)
                          ConsoleButton(
                              label: 'Quit OpenFader',
                              variant: ConsoleButtonVariant.quiet,
                              onPressed: onQuit),
                      ]),
                    ]))),
      );
    });
  }
}
