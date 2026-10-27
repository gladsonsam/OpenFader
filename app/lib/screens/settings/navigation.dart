part of '../settings.dart';

class _Sidebar extends StatelessWidget {
  final SettingsSection active;
  final ValueChanged<SettingsSection> onSelect;
  const _Sidebar({required this.active, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 400,
      padding: const EdgeInsets.fromLTRB(20, 28, 20, 20),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).headerBg,
        border: Border(
          right: BorderSide(color: ConsolePalette.of(context).line),
        ),
      ),
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
                      horizontal: 20,
                      vertical: 18,
                    ),
                    constraints: const BoxConstraints(minHeight: 88),
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
                          size: 26,
                          color: s == active
                              ? ConsolePalette.of(context).cyan
                              : ConsolePalette.of(context).text7,
                        ),
                        const SizedBox(width: 16),
                        Expanded(
                          child: Text(
                            s.label,
                            style: TextStyle(
                              fontFamily: U4.font,
                              fontSize: 25,
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
          const Spacer(),
          GestureDetector(
            onTap: () => onSelect(SettingsSection.connections),
            behavior: HitTestBehavior.opaque,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(20, 24, 20, 6),
              child: Row(
                children: [
                  SizedBox(
                    width: 44,
                    height: 44,
                    child: SvgPicture.asset(
                      'assets/brand/open-fader-logo.svg',
                      semanticsLabel: 'OpenFader',
                    ),
                  ),
                  const SizedBox(width: 14),
                  Text(
                    'OpenFader',
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 25,
                      fontWeight: FontWeight.w600,
                      color: ConsolePalette.of(context).text1,
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _OverviewBrand extends StatelessWidget {
  @override
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(4, 6, 4, 0),
        child: Row(
          children: [
            SizedBox(width: 54, height: 54, child: SvgPictureAssetLogo()),
            SizedBox(width: 14),
            Text(
              'OpenFader',
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: 25,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).text1,
              ),
            ),
          ],
        ),
      );
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
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 18),
            constraints: const BoxConstraints(minHeight: 90),
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
                const SizedBox(width: 16),
                Expanded(
                  child: Text(
                    section.label,
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 19,
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
