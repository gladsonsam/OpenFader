part of '../settings.dart';

class _Section extends StatelessWidget {
  final String title;
  final List<Widget> children;
  final bool first;
  const _Section({
    required this.title,
    required this.children,
    this.first = false,
  });

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(
            4,
            first
                ? (!wide && _SettingsLayout.isConnections(context) ? 20 : 0)
                : (wide ? 32 : _SettingsLayout.sectionGap(context)),
            4,
            wide ? 14 : 10,
          ),
          child: Text(
            title,
            style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 18 : 17,
              height: 1.3,
              fontWeight: FontWeight.w500,
              color: ConsolePalette.of(context).text7,
            ),
          ),
        ),
        ConsolePanel(
          color: ConsolePalette.of(context).chip,
          padding: EdgeInsets.zero,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: _divided(context, children),
          ),
        ),
      ],
    );
  }

  static List<Widget> _divided(BuildContext context, List<Widget> children) => [
        for (var i = 0; i < children.length; i++)
          DecoratedBox(
            decoration: BoxDecoration(
              border: i < children.length - 1
                  ? Border(
                      bottom:
                          BorderSide(color: ConsolePalette.of(context).line),
                    )
                  : null,
            ),
            child: children[i],
          ),
      ];
}

class _Note extends StatelessWidget {
  final String text;
  const _Note(this.text);

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Padding(
      padding: EdgeInsets.fromLTRB(4, wide ? 14 : 12, 4, 0),
      child: Text(
        text,
        style: TextStyle(
          fontFamily: U4.font,
          fontSize: wide ? 14 : 14,
          height: wide ? 1.5 : 22 / 15,
          color: ConsolePalette.of(context).panFill,
        ),
      ),
    );
  }
}

class _PrefRow extends StatelessWidget {
  final String label;
  final String? desc;
  final Widget trailing;
  const _PrefRow({required this.label, this.desc, required this.trailing});

  @override
  Widget build(BuildContext context) => ConsolePreferenceRow(
      label: label,
      description: desc,
      trailing: trailing,
      minHeight: MediaQuery.sizeOf(context).width <=
                  AppDesign.settingsSidebarBreakpoint &&
              _SettingsLayout.isConnections(context)
          ? AppDesign.compactSettingsRowHeight
          : AppDesign.settingsRowHeight);
}

class _ActionRow extends StatelessWidget {
  final String label;
  final String? desc;
  final bool danger;
  final FutureOr<void> Function() onTap;
  const _ActionRow({
    required this.label,
    this.desc,
    this.danger = false,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: label,
      onTap: () => _runSettingsAction(context, onTap),
      child: InkWell(
          onTap: () => _runSettingsAction(context, onTap),
          child: ConsolePreferenceRow(
              label: label,
              description: desc,
              foreground: danger ? ConsolePalette.of(context).mute : null,
              trailing: _SettingsIcon(Icons.chevron_right,
                  size: AppDesign.iconSize,
                  color: ConsolePalette.of(context).text7))));
}

class _ValueText extends StatelessWidget {
  final String text;
  const _ValueText(this.text);

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Text(
      text,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: TextStyle(
        fontFamily: U4.font,
        fontSize: wide ? 17 : 16,
        height: wide ? 1.3 : 24 / 18,
        color: ConsolePalette.of(context).text7,
        fontFeatures: const [FontFeature.tabularFigures()],
      ),
    );
  }
}

class _SettingsLayout extends InheritedWidget {
  final SettingsSection section;
  const _SettingsLayout({required this.section, required super.child});

  static SettingsSection? _section(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<_SettingsLayout>()?.section;

  static bool isConnections(BuildContext context) =>
      _section(context) == SettingsSection.connections;

  static double sectionGap(BuildContext context) => switch (_section(context)) {
        SettingsSection.connections => 20,
        SettingsSection.about => 20,
        _ => 26,
      };

  @override
  bool updateShouldNotify(_SettingsLayout oldWidget) =>
      section != oldWidget.section;
}
