part of '../settings.dart';

class _AboutDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  const _AboutDetail({required this.store, required this.toast});

  static const _repo = 'https://github.com/gladsonsam/OpenFader';

  Future<void> _open(String url) async {
    final uri = Uri.parse(url);
    if (!await launchUrl(uri, mode: LaunchMode.externalApplication)) {
      throw 'Could not open $url';
    }
  }

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!wide)
          Padding(
            padding: EdgeInsets.fromLTRB(4, 2, 4, 12),
            child: Row(
              children: [
                SizedBox(width: 64, height: 64, child: SvgPictureAssetLogo()),
                SizedBox(width: 18),
                Text(
                  'OpenFader',
                  style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: AppDesign.pageTitle,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).text1,
                  ),
                ),
              ],
            ),
          ),
        _Section(
          first: true,
          title: 'Support',
          children: [
            _LinkRow(
              label: 'Report a bug',
              onTap: () => _open('$_repo/issues/new?labels=bug'),
            ),
            _LinkRow(
              label: 'Request a feature',
              onTap: () => _open('$_repo/issues/new?labels=enhancement'),
            ),
            _LinkRow(
              label: 'Export diagnostics',
              desc:
                  'App settings and counts. No mixer addresses or mix contents.',
              icon: Icons.download_outlined,
              onTap: () async {
                await store.exportDiagnostics();
                toast('Diagnostics exported');
              },
            ),
          ],
        ),
        _Section(
          title: 'Connection help',
          children: [
            _HelpAccordion(
              title: 'My mixer does not appear',
              body:
                  'Connect this device and the mixer to the same network. Refresh recent mixers or use Discover on Connections.\n\nYou can also add a mixer manually with its IP address or hostname.',
            ),
            _HelpAccordion(
              title: 'A mixer will not reconnect',
              body:
                  'Check the mixer is powered on and its network address has not changed. Try a fresh connection from the Connections screen.',
            ),
          ],
        ),
        _Section(
          title: 'Project',
          children: [
            _LinkRow(
              label: 'Source code',
              desc: 'github.com/gladsonsam/OpenFader',
              onTap: () => _open(_repo),
            ),
            _LinkRow(
              label: 'Contributors',
              onTap: () => _open('$_repo/graphs/contributors'),
            ),
            _LinkRow(
              label: 'Licences & credits',
              icon: Icons.chevron_right,
              onTap: () => showDialog<void>(
                animationStyle: AnimationStyle.noAnimation,
                context: context,
                builder: (context) => AlertDialog(
                  backgroundColor: ConsolePalette.of(context).chip,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(14),
                    side: BorderSide(color: ConsolePalette.of(context).lineCtl),
                  ),
                  title: Text(
                    'Licences & credits',
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 23,
                      fontWeight: FontWeight.w600,
                      color: ConsolePalette.of(context).text1,
                    ),
                  ),
                  content: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'OpenFader is free and open source. The project repository contains the current licence and contributor information.',
                        style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: AppDesign.rowLabel,
                          height: 1.5,
                          color: ConsolePalette.of(context).text7,
                        ),
                      ),
                      const SizedBox(height: AppDesign.spaceMd),
                      Text(
                        'Interface typeface: Barlow. Toolbar symbols and the OpenFader logo are vector artwork.',
                        style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: AppDesign.rowLabel,
                          height: 1.5,
                          color: ConsolePalette.of(context).text7,
                        ),
                      ),
                      const SizedBox(height: AppDesign.spaceLg),
                      GestureDetector(
                        onTap: () =>
                            _runSettingsAction(context, () => _open(_repo)),
                        child: Text(
                          'Open project repository',
                          style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: AppDesign.rowLabel,
                            fontWeight: FontWeight.w600,
                            color: ConsolePalette.of(context).cyan,
                          ),
                        ),
                      ),
                    ],
                  ),
                  actions: [
                    TextButton(
                      onPressed: () => Navigator.pop(context),
                      child: Text(
                        'Close',
                        style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: AppDesign.rowLabel,
                          color: ConsolePalette.of(context).text1,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
        const Spacer(),
        Padding(
          padding: EdgeInsets.only(top: wide ? 18 : 16),
          child: Text(
            'Made by Gladson Sam',
            textAlign: TextAlign.center,
            style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 18 : 13,
              height: wide ? 24 / 18 : 18 / 13,
              color: ConsolePalette.of(context).panFill,
            ),
          ),
        ),
      ],
    );
  }
}

class _LinkRow extends StatelessWidget {
  final String label;
  final String? desc;
  final IconData icon;
  final FutureOr<void> Function() onTap;
  const _LinkRow({
    required this.label,
    this.desc,
    this.icon = Icons.open_in_new,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Semantics(
      button: true,
      label: label,
      onTap: () => _runSettingsAction(context, onTap),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, onTap),
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints(minHeight: wide ? 96 : 76),
          padding: EdgeInsets.symmetric(
            horizontal: 16,
            vertical: wide ? 22 : 16,
          ),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      label,
                      style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: AppDesign.rowLabel,
                        height: wide ? 1.25 : 24 / 18,
                        fontWeight: FontWeight.w500,
                        color: ConsolePalette.of(context).text1,
                      ),
                    ),
                    if (desc != null) ...[
                      SizedBox(height: wide ? 7 : 4),
                      Text(
                        desc!,
                        style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: AppDesign.body,
                          height: wide ? 1.4 : 21 / 15,
                          color: ConsolePalette.of(context).text7,
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: AppDesign.spaceLg),
              _SettingsIcon(
                icon,
                size: 22,
                color: ConsolePalette.of(context).text7,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _HelpAccordion extends StatefulWidget {
  final String title;
  final String body;
  const _HelpAccordion({required this.title, required this.body});

  @override
  State<_HelpAccordion> createState() => _HelpAccordionState();
}

class _HelpAccordionState extends State<_HelpAccordion> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final wide =
        MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Semantics(
          button: true,
          expanded: _expanded,
          label: widget.title,
          child: InkWell(
            onTap: () => setState(() => _expanded = !_expanded),
            child: Container(
              constraints: BoxConstraints(minHeight: wide ? 88 : 70),
              padding: EdgeInsets.symmetric(
                horizontal: 16,
                vertical: wide ? 24 : 18,
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      widget.title,
                      style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: AppDesign.rowLabel,
                        height: 1.2,
                        fontWeight: FontWeight.w500,
                        color: ConsolePalette.of(context).text1,
                      ),
                    ),
                  ),
                  const SizedBox(width: 20),
                  RotatedBox(
                    quarterTurns: _expanded ? 1 : 0,
                    child: const _SettingsIcon(Icons.chevron_right),
                  ),
                ],
              ),
            ),
          ),
        ),
        if (_expanded)
          Padding(
            padding: EdgeInsets.fromLTRB(
              wide ? 26 : 16,
              0,
              wide ? 26 : 16,
              wide ? 24 : 18,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final (index, paragraph)
                    in widget.body.split('\n\n').indexed) ...[
                  if (index > 0) const SizedBox(height: AppDesign.spaceMd),
                  Text(
                    paragraph,
                    style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: AppDesign.body,
                      height: wide ? 1.5 : 22 / 15,
                      color: ConsolePalette.of(context).text7,
                    ),
                  ),
                ],
              ],
            ),
          ),
      ],
    );
  }
}
