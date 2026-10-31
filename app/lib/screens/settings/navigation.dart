part of '../settings.dart';

class _Sidebar extends StatelessWidget {
  final SettingsSection active;
  final ValueChanged<SettingsSection> onSelect;
  final VoidCallback onHome;
  const _Sidebar(
      {required this.active, required this.onSelect, required this.onHome});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 260,
      padding: const EdgeInsets.all(AppDesign.spaceMd),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).headerBg,
        border: Border(
          right: BorderSide(color: ConsolePalette.of(context).line),
        ),
      ),
      child: SingleChildScrollView(
          child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (final s in SettingsSection.values)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Semantics(
                button: true,
                selected: s == active,
                label: s.label,
                onTap: () => onSelect(s),
                child: GestureDetector(
                  onTap: () => onSelect(s),
                  behavior: HitTestBehavior.opaque,
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 12,
                    ),
                    constraints: const BoxConstraints(minHeight: 56),
                    decoration: BoxDecoration(
                      color: s == active
                          ? ConsolePalette.of(context).selectionFill
                          : Colors.transparent,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Row(
                      children: [
                        _SettingsIcon(
                          s.icon,
                          size: 22,
                          color: s == active
                              ? ConsolePalette.of(context).cyan
                              : ConsolePalette.of(context).text7,
                        ),
                        const SizedBox(width: AppDesign.spaceLg),
                        Expanded(
                          child: Text(
                            s.label,
                            style: TextStyle(
                              fontFamily: U4.font,
                              fontSize: AppDesign.controlLabel,
                              fontWeight: FontWeight.w500,
                              color: s == active
                                  ? ConsolePalette.of(context).cyan
                                  : ConsolePalette.of(context).text7,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
        ],
      )),
    );
  }
}

class _OverviewBrand extends StatelessWidget {
  final VoidCallback onHome;
  const _OverviewBrand({required this.onHome});
  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: 'Open Connections home',
      child: GestureDetector(
          onTap: onHome,
          behavior: HitTestBehavior.opaque,
          child: Padding(
            padding: const EdgeInsets.fromLTRB(4, 6, 4, 0),
            child: Row(
              children: [
                SizedBox(width: 54, height: 54, child: SvgPictureAssetLogo()),
                SizedBox(width: 14),
                Text(
                  'OpenFader',
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: AppDesign.controlLabel,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).text1,
                  ),
                ),
              ],
            ),
          )));
}

class SvgPictureAssetLogo extends StatelessWidget {
  const SvgPictureAssetLogo({super.key});
  @override
  Widget build(BuildContext context) => SvgPicture.asset(
        'assets/brand/open-fader-logo.svg',
        semanticsLabel: 'OpenFader',
      );
}

class _OverviewCard extends StatelessWidget {
  final SettingsSection section;
  final FutureOr<void> Function() onTap;
  const _OverviewCard({required this.section, required this.onTap});

  @override
  Widget build(BuildContext context) => Semantics(
        button: true,
        label: section.label,
        onTap: () => _runSettingsAction(context, onTap),
        child: GestureDetector(
          onTap: () => _runSettingsAction(context, onTap),
          behavior: HitTestBehavior.opaque,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            constraints: const BoxConstraints(minHeight: 64),
            decoration: BoxDecoration(
              color: ConsolePalette.of(context).chip,
              borderRadius: BorderRadius.circular(12),
            ),
            child: Row(
              children: [
                _SettingsIcon(
                  section.icon,
                  size: 24,
                  color: ConsolePalette.of(context).text7,
                ),
                const SizedBox(width: AppDesign.spaceLg),
                Expanded(
                  child: Text(
                    section.label,
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: AppDesign.rowLabel,
                      fontWeight: FontWeight.w500,
                      color: ConsolePalette.of(context).text1,
                    ),
                  ),
                ),
                _SettingsIcon(
                  Icons.chevron_right,
                  size: 24,
                  color: ConsolePalette.of(context).text7,
                ),
              ],
            ),
          ),
        ),
      );
}
