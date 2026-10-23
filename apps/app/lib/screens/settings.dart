// Settings: six destinations with a tablet sidebar/detail split and phone
// overview-list + subpages. All preferences are real (PrefsService),
// mixes/backups are real files (LocalMixStore), discovery/help/links use
// the live services. Prototype copy (sample devices, demo toasts) is gone.
//
// Wiring: everything flows through [SettingsStore] (constructor injection —
// Area A implements it over prefs + recents + mixes + platform plugins).
library;

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:url_launcher/url_launcher.dart';

import '../settings/mixes.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../theme.dart';

enum SettingsSection {
  connections('Connections', Icons.wifi),
  mixes('Mixes & backups', Icons.library_music_outlined),
  data('App data', Icons.storage_outlined),
  appearance('Appearance & controls', Icons.palette_outlined),
  about('About & Support', Icons.info_outline);

  const SettingsSection(this.label, this.icon);
  final String label;
  final IconData icon;
}

/// Minimal store contract the settings UI renders. Implemented by the
/// shell/owner over prefs + recents + mixes + platform plugins.
abstract class SettingsStore extends ChangeNotifier {
  PrefsService get prefs;
  RecentMixers get recents;
  LocalMixStore get mixes;

  Future<String?> importMix();
  Future<void> exportMix(LocalMix mix);
  Future<void> deleteMix(LocalMix mix);
  Future<String> importBackup();
  Future<void> exportBackup();
  Future<void> clearCache();
  Future<void> forgetMixers();
  Future<void> resetPrefs();
  Future<void> deleteMixes();
  Future<void> fullReset();
  Future<void> exportDiagnostics();
}

class U4SettingsPage extends StatefulWidget {
  final SettingsStore store;
  final VoidCallback onClose;
  final VoidCallback onOpenMixer;

  const U4SettingsPage(
      {super.key,
      required this.store,
      required this.onClose,
      required this.onOpenMixer});

  @override
  State<U4SettingsPage> createState() => _U4SettingsPageState();
}

class _U4SettingsPageState extends State<U4SettingsPage> {
  /// Tablet detail selection (default: connections). Phone subpage
  /// (null = overview list).
  SettingsSection _sel = SettingsSection.connections;
  SettingsSection? _page;

  void _toast(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
        content: Text(msg,
            textAlign: TextAlign.center,
            style: const TextStyle(
                fontFamily: U4.font, fontSize: 17)),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3)));
  }

  Future<void> _confirm(
      {required String title,
      required String body,
      required String action,
      required Future<void> Function() onConfirm}) async {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: const Color(0xff262626),
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(14),
            side: const BorderSide(color: Color(0xff555555))),
        title: Text(title,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 30 : 23,
                fontWeight: FontWeight.w600,
                color: U4.text1)),
        content: Text(body,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 25 : 17,
                height: 1.5,
                color: const Color(0xffb5b5b5))),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text('Cancel',
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 22 : 17,
                      color: U4.text1))),
          TextButton(
              onPressed: () => Navigator.pop(context, true),
              style: TextButton.styleFrom(
                  backgroundColor: const Color(0xffd63a2c),
                  padding: EdgeInsets.symmetric(
                      horizontal: 22, vertical: wide ? 12 : 10),
                  shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(9))),
              child: Text(action,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 22 : 17,
                      fontWeight: FontWeight.w600,
                      color: Colors.white))),
        ],
      ),
    );
    if (ok == true) {
      await onConfirm();
      if (mounted) setState(() {});
    }
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Scaffold(
      backgroundColor: U4.page,
      body: SafeArea(
        child: ListenableBuilder(
          listenable: widget.store,
          builder: (context, _) {
            if (wide) return _tablet();
            return _page == null ? _overviewList() : _subpage(_page!);
          },
        ),
      ),
    );
  }

  // -------------------------------------------------------------------------
  // Chrome: topbar + sidebar + overview list.
  // -------------------------------------------------------------------------

  Widget _topbar({required String title, required bool showBack}) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Container(
      height: wide ? 104 : 88,
      padding: EdgeInsets.symmetric(horizontal: wide ? 32 : 16),
      decoration: const BoxDecoration(
          color: Color(0xff181818),
          border: Border(bottom: BorderSide(color: Color(0xff363636)))),
      child: Row(children: [
        if (!wide && showBack)
          _IconBtn(
              tooltip: 'Back to Settings',
              onTap: () => setState(() => _page = null),
              child: const Icon(Icons.arrow_back,
                  size: 24, color: Color(0xffb5b5b5))),
        if (!wide && showBack) const SizedBox(width: 14),
        Expanded(
            child: Text(title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 34 : 22,
                    fontWeight: FontWeight.w600,
                    color: U4.text1))),
        _IconBtn(
            tooltip: 'Close Settings',
            onTap: widget.onClose,
            child: const Icon(Icons.close,
                size: 24, color: Color(0xffb5b5b5))),
      ]),
    );
  }

  Widget _tablet() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _topbar(title: 'Settings', showBack: false),
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _Sidebar(
                  active: _sel,
                  onSelect: (s) => setState(() => _sel = s)),
              Expanded(
                child: SingleChildScrollView(
                  padding:
                      const EdgeInsets.fromLTRB(56, 44, 56, 60),
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 1200),
                    child: _detail(_sel),
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _overviewList() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _topbar(title: 'Settings', showBack: false),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 20, 16, 36),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _OverviewBrand(),
                const SizedBox(height: 24),
                for (final s in SettingsSection.values)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: _OverviewCard(
                        section: s,
                        onTap: () => setState(() => _page = s)),
                  ),
              ],
            ),
          ),
        ),
      ],
    );
  }

  Widget _subpage(SettingsSection s) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _topbar(title: s.label, showBack: true),
        Expanded(
          child: SingleChildScrollView(
            padding: const EdgeInsets.fromLTRB(16, 20, 16, 36),
            child: _detail(s),
          ),
        ),
      ],
    );
  }

  // -------------------------------------------------------------------------
  // Details per destination.
  // -------------------------------------------------------------------------

  Widget _detail(SettingsSection s) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (wide)
          Padding(
            padding: const EdgeInsets.only(bottom: 30),
            child: Text(s.label,
                style: const TextStyle(
                    fontFamily: U4.font,
                    fontSize: 36,
                    height: 1.22,
                    fontWeight: FontWeight.w600,
                    color: U4.text1)),
          ),
        switch (s) {
          SettingsSection.connections => _ConnectionsDetail(
              store: widget.store, toast: _toast, confirm: _confirm),
          SettingsSection.mixes => _MixesDetail(
              store: widget.store,
              toast: _toast,
              confirm: _confirm,
              onOpenMixer: widget.onOpenMixer),
          SettingsSection.data => _DataDetail(
              store: widget.store, toast: _toast, confirm: _confirm),
          SettingsSection.appearance => _AppearanceDetail(
              store: widget.store, toast: _toast),
          SettingsSection.about =>
            _AboutDetail(store: widget.store, toast: _toast),
        },
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Shared chrome + row widgets.
// ---------------------------------------------------------------------------

class _IconBtn extends StatelessWidget {
  final String tooltip;
  final VoidCallback onTap;
  final Widget child;
  const _IconBtn(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: tooltip,
      onTap: onTap,
      child: Tooltip(
          message: tooltip,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onTap,
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: const Color(0xff262626),
                          borderRadius: BorderRadius.circular(10),
                          border:
                              Border.all(color: const Color(0xff363636))),
                      child: child)))));
}

class _Sidebar extends StatelessWidget {
  final SettingsSection active;
  final ValueChanged<SettingsSection> onSelect;
  const _Sidebar({required this.active, required this.onSelect});

  @override
  Widget build(BuildContext context) {
    return Container(
      width: 400,
      padding: const EdgeInsets.fromLTRB(20, 28, 20, 20),
      decoration: const BoxDecoration(
          color: Color(0xff181818),
          border: Border(right: BorderSide(color: Color(0xff363636)))),
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
                        horizontal: 20, vertical: 18),
                    constraints: const BoxConstraints(minHeight: 88),
                    decoration: BoxDecoration(
                        color: s == active
                            ? const Color(0xff262b14)
                            : Colors.transparent,
                        borderRadius: BorderRadius.circular(12)),
                    child: Row(children: [
                      Icon(s.icon,
                          size: 26,
                          color: s == active
                              ? U4.cyan
                              : const Color(0xffb5b5b5)),
                      const SizedBox(width: 16),
                      Expanded(
                          child: Text(s.label,
                              style: TextStyle(
                                  fontFamily: U4.font,
                                  fontSize: 25,
                                  fontWeight: FontWeight.w500,
                                  color: s == active
                                      ? U4.cyan
                                      : const Color(0xffb5b5b5)))),
                    ]),
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
              child: Row(children: [
                SizedBox(
                    width: 44,
                    height: 44,
                    child: SvgPicture.asset(
                        'assets/brand/open-fader-logo.svg',
                        semanticsLabel: 'OpenFader')),
                const SizedBox(width: 14),
                const Text('OpenFader',
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 25,
                        fontWeight: FontWeight.w600,
                        color: U4.text1)),
              ]),
            ),
          ),
        ],
      ),
    );
  }
}

class _OverviewBrand extends StatelessWidget {
  @override
  Widget build(BuildContext context) => const Padding(
        padding: EdgeInsets.fromLTRB(4, 6, 4, 0),
        child: Row(children: [
          SizedBox(
              width: 54,
              height: 54,
              child: SvgPictureAssetLogo()),
          SizedBox(width: 14),
          Text('OpenFader',
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 25,
                  fontWeight: FontWeight.w600,
                  color: U4.text1)),
        ]),
      );
}

class SvgPictureAssetLogo extends StatelessWidget {
  const SvgPictureAssetLogo({super.key});
  @override
  Widget build(BuildContext context) => SvgPicture.asset(
      'assets/brand/open-fader-logo.svg',
      semanticsLabel: 'OpenFader');
}

class _OverviewCard extends StatelessWidget {
  final SettingsSection section;
  final VoidCallback onTap;
  const _OverviewCard({required this.section, required this.onTap});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: section.label,
      onTap: onTap,
      child: GestureDetector(
          onTap: onTap,
          behavior: HitTestBehavior.opaque,
          child: Container(
              padding: const EdgeInsets.symmetric(
                  horizontal: 16, vertical: 18),
              constraints: const BoxConstraints(minHeight: 90),
              decoration: BoxDecoration(
                  color: const Color(0xff262626),
                  borderRadius: BorderRadius.circular(12)),
              child: Row(children: [
                Icon(section.icon,
                    size: 24, color: const Color(0xffb5b5b5)),
                const SizedBox(width: 16),
                Expanded(
                    child: Text(section.label,
                        style: const TextStyle(
                            fontFamily: U4.font,
                            fontSize: 19,
                            fontWeight: FontWeight.w500,
                            color: U4.text1))),
                const Icon(Icons.chevron_right,
                    size: 24, color: Color(0xffb5b5b5)),
              ]))));
}

/// Section heading + card group.
class _Section extends StatelessWidget {
  final String title;
  final List<Widget> children;
  const _Section({required this.title, required this.children});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: EdgeInsets.fromLTRB(4, wide ? 32 : 20, 4, wide ? 14 : 10),
          child: Text(title,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 26 : 17,
                  height: wide ? 1.3 : 1.35,
                  fontWeight: FontWeight.w500,
                  color: const Color(0xffb5b5b5))),
        ),
        Container(
          decoration: BoxDecoration(
              color: const Color(0xff262626),
              borderRadius:
                  BorderRadius.circular(wide ? 14 : 12)),
          clipBehavior: Clip.antiAlias,
          child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: _divided(children)),
        ),
      ],
    );
  }

  static List<Widget> _divided(List<Widget> children) {
    final out = <Widget>[];
    for (var i = 0; i < children.length; i++) {
      out.add(children[i]);
      if (i < children.length - 1) {
        out.add(const Divider(
            color: Color(0xff363636), height: 1));
      }
    }
    return out;
  }
}

class _Note extends StatelessWidget {
  final String text;
  const _Note(this.text);

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.fromLTRB(4, 14, 4, 0),
      child: Text(text,
          style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 23 : 15,
              height: 1.5,
              color: const Color(0xffaaaaaa))),
    );
  }
}

/// Label/description/value row with a trailing control.
class _PrefRow extends StatelessWidget {
  final String label;
  final String? desc;
  final Widget trailing;
  const _PrefRow(
      {required this.label, this.desc, required this.trailing});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16,
          vertical: wide ? 22 : 16),
      child: Row(children: [
        Expanded(
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
              Text(label,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 27 : 18,
                      height: 1.25,
                      fontWeight: FontWeight.w500,
                      color: U4.text1)),
              if (desc != null) ...[
                const SizedBox(height: 7),
                Text(desc!,
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: wide ? 23 : 15,
                        height: 1.4,
                        color: const Color(0xffb5b5b5))),
              ],
            ])),
        const SizedBox(width: 24),
        trailing,
      ]),
    );
  }
}

class _Toggle extends StatelessWidget {
  final bool value;
  final ValueChanged<bool> onChanged;
  final String label;
  const _Toggle(
      {required this.value,
      required this.onChanged,
      required this.label});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final w = wide ? 60.0 : 50.0;
    final h = wide ? 36.0 : 30.0;
    final knob = wide ? 28.0 : 22.0;
    return Semantics(
      button: true,
      label: '$label ${value ? 'on' : 'off'}',
      onTap: () => onChanged(!value),
      child: GestureDetector(
        onTap: () => onChanged(!value),
        behavior: HitTestBehavior.opaque,
        child: SizedBox(
          width: w,
          height: 44,
          child: Center(
            child: Container(
              width: w,
              height: h,
              decoration: BoxDecoration(
                  color: value
                      ? U4.cyan
                      : const Color(0xff444444),
                  borderRadius: BorderRadius.circular(h / 2),
                  border: Border.all(
                      color: value
                          ? U4.cyan
                          : const Color(0xff626262))),
              child: Stack(children: [
                Positioned(
                    left: value ? w - knob - 4 : 4,
                    top: (h - knob) / 2,
                    child: Container(
                        width: knob,
                        height: knob,
                        decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color: value
                                ? const Color(0xff1c1c1c)
                                : const Color(0xffebebeb)))),
              ]),
            ),
          ),
        ),
      ),
    );
  }
}

class _Select extends StatelessWidget {
  final String value;
  final List<(String, String)> options;
  final ValueChanged<String> onChanged;
  final String label;
  const _Select(
      {required this.value,
      required this.options,
      required this.onChanged,
      required this.label});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Container(
      constraints: BoxConstraints(
          minHeight: wide ? 50 : 44,
          maxWidth: wide ? 360 : 174),
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 16 : 12),
      decoration: BoxDecoration(
          color: const Color(0xff343434),
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: const Color(0xff414141))),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<String>(
          value: value,
          dropdownColor: const Color(0xff262626),
          icon: const Icon(Icons.arrow_drop_down,
              size: 18, color: Color(0xffb5b5b5)),
          style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 22 : 17,
              color: U4.text1),
          items: [
            for (final o in options)
              DropdownMenuItem(
                  value: o.$1,
                  child: Text(o.$2,
                      overflow: TextOverflow.ellipsis)),
          ],
          onChanged: (v) {
            if (v != null) onChanged(v);
          },
        ),
      ),
    );
  }
}

/// Full-width chevron action row.
class _ActionRow extends StatelessWidget {
  final String label;
  final String? desc;
  final bool danger;
  final VoidCallback onTap;
  const _ActionRow(
      {required this.label,
      this.desc,
      this.danger = false,
      required this.onTap});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Padding(
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 26 : 16,
              vertical: wide ? 22 : 16),
          child: Row(children: [
            Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(label,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: wide ? 27 : 18,
                          height: 1.25,
                          fontWeight: FontWeight.w500,
                          color: danger
                              ? const Color(0xffff8a80)
                              : U4.text1)),
                  if (desc != null) ...[
                    const SizedBox(height: 7),
                    Text(desc!,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            height: 1.4,
                            color: const Color(0xffb5b5b5))),
                  ],
                ])),
            const SizedBox(width: 16),
            const Icon(Icons.chevron_right,
                size: 24, color: Color(0xffb5b5b5)),
          ]),
        ),
      ),
    );
  }
}

class _ValueText extends StatelessWidget {
  final String text;
  const _ValueText(this.text);

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Text(text,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: TextStyle(
            fontFamily: U4.font,
            fontSize: wide ? 27 : 18,
            color: const Color(0xffb5b5b5),
            fontFeatures: const [FontFeature.tabularFigures()]));
  }
}

// ---------------------------------------------------------------------------
// Connections detail.
// ---------------------------------------------------------------------------

class _ConnectionsDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function(
      {required String title,
      required String body,
      required String action,
      required Future<void> Function() onConfirm}) confirm;

  const _ConnectionsDetail(
      {required this.store, required this.toast, required this.confirm});

  @override
  Widget build(BuildContext context) {
    final prefs = store.prefs;
    final recents = store.recents.items;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(title: 'Connection behaviour', children: [
          _PrefRow(
              label: 'Find mixers automatically',
              trailing: _Toggle(
                  label: 'Find mixers automatically',
                  value: prefs.autoFind,
                  onChanged: (v) async {
                    await prefs.setAutoFind(v);
                    toast('Preference saved');
                  })),
          _PrefRow(
              label: 'Reconnect to the last mixer',
              trailing: _Toggle(
                  label: 'Reconnect to the last mixer',
                  value: prefs.reconnectLast,
                  onChanged: (v) async {
                    await prefs.setReconnectLast(v);
                    toast('Preference saved');
                  })),
          _PrefRow(
              label: 'Remember recent mixers',
              trailing: _Toggle(
                  label: 'Remember recent mixers',
                  value: prefs.rememberRecents,
                  onChanged: (v) async {
                    await prefs.setRemember(v);
                    toast('Preference saved');
                  })),
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
                  })),
        ]),
        _Section(title: 'Saved mixers', children: [
          if (recents.isEmpty)
            const _PrefRow(
                label: 'No saved mixers',
                desc: 'Mixers you connect to will appear here.',
                trailing: SizedBox.shrink()),
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
                    })),
        ]),
        const _Note('Your device and mixer must be on the same network.'),
        _Section(title: 'Supported mixers', children: const [
          _PrefRow(
              label: 'XR18',
              trailing: _ValueText('Behringer')),
          _PrefRow(
              label: 'DL32S',
              trailing: _ValueText('Mackie')),
        ]),
      ],
    );
  }
}

class _SavedMixerRow extends StatelessWidget {
  final String name;
  final String addr;
  final VoidCallback onForget;
  const _SavedMixerRow(
      {required this.name, required this.addr, required this.onForget});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16, vertical: wide ? 22 : 16),
      child: Row(children: [
        Expanded(
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
              Text(name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 27 : 18,
                      fontWeight: FontWeight.w500,
                      color: U4.text1)),
              const SizedBox(height: 7),
              Text(addr,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 23 : 15,
                      color: const Color(0xffb5b5b5),
                      fontFeatures: const [
                        FontFeature.tabularFigures()
                      ])),
            ])),
        _IconBtnInline(tooltip: 'Forget $name', onTap: onForget),
      ]),
    );
  }
}

class _IconBtnInline extends StatelessWidget {
  final String tooltip;
  final VoidCallback onTap;
  const _IconBtnInline({required this.tooltip, required this.onTap});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: tooltip,
      onTap: onTap,
      child: Tooltip(
          message: tooltip,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onTap,
                  child: const Icon(Icons.delete_outline,
                      size: 22, color: Color(0xffb5b5b5))))));
}

// ---------------------------------------------------------------------------
// Mixes detail.
// ---------------------------------------------------------------------------

class _MixesDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function(
      {required String title,
      required String body,
      required String action,
      required Future<void> Function() onConfirm}) confirm;
  final VoidCallback onOpenMixer;

  const _MixesDetail(
      {required this.store,
      required this.toast,
      required this.confirm,
      required this.onOpenMixer});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final mixes = store.mixes.mixes;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(title: 'Local mixes', children: [
          if (mixes.isEmpty)
            Padding(
              padding: EdgeInsets.symmetric(
                  horizontal: wide ? 26 : 16,
                  vertical: wide ? 32 : 22),
              child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('No mixes saved yet',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 27 : 18,
                            fontWeight: FontWeight.w500,
                            color: U4.text1)),
                    const SizedBox(height: 7),
                    Text('Import a mix, or open the offline mixer to prepare one.',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            color: const Color(0xffb5b5b5))),
                  ]),
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
                      })),
        ]),
        Padding(
          padding: EdgeInsets.only(top: wide ? 20 : 16),
          child: Wrap(spacing: 14, runSpacing: 10, children: [
            _PrimaryButton(
                label: 'Import mix',
                onTap: () async {
                  final err = await store.importMix();
                  if (err == null) {
                    toast('Mix imported');
                  } else if (err != 'cancelled') {
                    toast(err);
                  }
                }),
            _PlainButton(label: 'Open offline mixer', onTap: onOpenMixer),
          ]),
        ),
        _Section(title: 'Backups', children: [
          _PrefRow(
              label: 'Back up changes automatically',
              trailing: _Toggle(
                  label: 'Back up changes automatically',
                  value: store.prefs.autoBackup,
                  onChanged: (v) async {
                    await store.prefs.setAutoBackup(v);
                    toast('Preference saved');
                  })),
          _PrefRow(
              label: 'Last local backup',
              trailing: _ValueText(store.prefs.backupCreatedAt.isEmpty
                  ? 'Not created'
                  : 'Saved on this device')),
        ]),
        Padding(
          padding: EdgeInsets.only(top: wide ? 20 : 16),
          child: Wrap(spacing: 14, runSpacing: 10, children: [
            _PrimaryButton(
                label: 'Export backup',
                onTap: () async {
                  await store.exportBackup();
                  toast('Backup exported');
                }),
            _PlainButton(
                label: 'Import backup',
                onTap: () async {
                  final res = await store.importBackup();
                  if (res == 'cancelled') return;
                  final n = int.tryParse(res) ?? 0;
                  if (n == 0) {
                    toast('Nothing valid to import');
                  } else {
                    toast(
                        '$n mix${n == 1 ? '' : 'es'} imported');
                  }
                }),
          ]),
        ),
        const _Note(
            'Keep an exported backup somewhere safe before deleting local mixes or resetting app data.'),
      ],
    );
  }
}

class _MixRow extends StatelessWidget {
  final LocalMix mix;
  final VoidCallback onExport;
  final VoidCallback onDelete;
  const _MixRow(
      {required this.mix,
      required this.onExport,
      required this.onDelete});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16, vertical: wide ? 22 : 16),
      child: Row(children: [
        Expanded(
            child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
              Text(mix.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 27 : 18,
                      fontWeight: FontWeight.w500,
                      color: U4.text1)),
              const SizedBox(height: 7),
              Text(mix.mixerModel,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 23 : 14,
                      color: const Color(0xffb5b5b5))),
            ])),
        _IconBtnInline(
            tooltip: 'Export ${mix.name}', onTap: onExport),
        const SizedBox(width: 6),
        _IconBtnInline(
            tooltip: 'Delete ${mix.name}', onTap: onDelete),
      ]),
    );
  }
}

class _PrimaryButton extends StatelessWidget {
  final String label;
  final VoidCallback onTap;
  const _PrimaryButton({required this.label, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints:
              BoxConstraints(minHeight: wide ? 52 : 44),
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 22 : 14, vertical: 10),
          decoration: BoxDecoration(
              color: U4.cyan,
              borderRadius: BorderRadius.circular(9)),
          child: Text(label,
              textAlign: TextAlign.center,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 22 : 17,
                  fontWeight: FontWeight.w600,
                  color: const Color(0xff171717))),
        ),
      ),
    );
  }
}

class _PlainButton extends StatelessWidget {
  final String label;
  final VoidCallback onTap;
  const _PlainButton({required this.label, required this.onTap});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints:
              BoxConstraints(minHeight: wide ? 52 : 44),
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 22 : 14, vertical: 10),
          decoration: BoxDecoration(
              color: const Color(0xff343434),
              borderRadius: BorderRadius.circular(9),
              border: Border.all(color: const Color(0xff414141))),
          child: Text(label,
              textAlign: TextAlign.center,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 22 : 17,
                  fontWeight: FontWeight.w500,
                  color: U4.text1)),
        ),
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// App data detail.
// ---------------------------------------------------------------------------

class _DataDetail extends StatelessWidget {
  final SettingsStore store;
  final void Function(String) toast;
  final Future<void> Function(
      {required String title,
      required String body,
      required String action,
      required Future<void> Function() onConfirm}) confirm;

  const _DataDetail(
      {required this.store, required this.toast, required this.confirm});

  static String _bytes(int b) =>
      b < 1024 ? '$b B' : '${(b / 1024).toStringAsFixed(1)} KB';

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _Section(title: 'On this device', children: [
          _PrefRow(
              label: 'Local mixes',
              trailing:
                  _ValueText('${store.mixes.mixes.length}')),
          _PrefRow(
              label: 'Saved mixers',
              trailing:
                  _ValueText('${store.recents.items.length}')),
          _PrefRow(
              label: 'App storage',
              trailing: FutureBuilder<int>(
                  future: store.mixes.storageBytes(),
                  builder: (context, snap) => _ValueText(
                      snap.hasData ? _bytes(snap.data!) : 'Calculating'))),
        ]),
        _Section(title: 'Manage data', children: [
          _ActionRow(
              label: 'Clear cache',
              desc: 'Remove temporary app files. Your mixes stay saved.',
              onTap: () => confirm(
                  title: 'Clear cache?',
                  body:
                      'Temporary app files will be removed. Local mixes, saved mixers and preferences will stay.',
                  action: 'Clear cache',
                  onConfirm: () async {
                    await store.clearCache();
                    toast('Cache cleared');
                  })),
          _ActionRow(
              label: 'Forget saved mixers',
              desc: 'Remove mixer names and addresses from this device.',
              onTap: () => confirm(
                  title: 'Forget saved mixers?',
                  body:
                      '${store.recents.items.length} saved mixer(s) will be removed from this device. Your mixes will stay.',
                  action: 'Forget mixers',
                  onConfirm: () async {
                    await store.forgetMixers();
                    toast('Saved mixers forgotten');
                  })),
          _ActionRow(
              label: 'Reset preferences',
              desc: 'Restore display and connection defaults.',
              onTap: () => confirm(
                  title: 'Reset preferences?',
                  body:
                      'Display, meter and connection preferences will return to their defaults. Saved mixes and mixer addresses will stay.',
                  action: 'Reset preferences',
                  onConfirm: () async {
                    await store.resetPrefs();
                    toast('Preferences reset');
                  })),
        ]),
        _Section(title: 'Reset', children: [
          _ActionRow(
              label: 'Delete local mixes',
              desc: 'Remove all mixes and local backups from this device.',
              danger: true,
              onTap: () => confirm(
                  title: 'Delete local mixes?',
                  body:
                      '${store.mixes.mixes.length} local mix(es) and local backups will be removed. Export a backup first if you want to keep them.',
                  action: 'Delete mixes',
                  onConfirm: () async {
                    await store.deleteMixes();
                    toast('Local mixes deleted');
                  })),
          _ActionRow(
              label: 'Reset all app data',
              desc: 'Remove local mixes, saved mixers and preferences.',
              danger: true,
              onTap: () => confirm(
                  title: 'Reset all app data?',
                  body:
                      'Local mixes, backups, saved mixers, cache and preferences will be removed. A connected mixer will not be reset.',
                  action: 'Reset app data',
                  onConfirm: () async {
                    await store.fullReset();
                    toast('App data reset');
                  })),
        ]),
        const _Note(
            'These actions affect this device. They do not reset a connected mixer.'),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// Appearance detail.
// ---------------------------------------------------------------------------

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
        _Section(title: 'Display', children: [
          const _PrefRow(
              label: 'Appearance',
              trailing: _ValueText('Dark')),
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
                  })),
          _PrefRow(
              label: 'Keep screen awake',
              trailing: _Toggle(
                  label: 'Keep screen awake',
                  value: prefs.keepAwake,
                  onChanged: (v) async {
                    await prefs.setKeepAwake(v);
                    toast('Preference saved');
                  })),
        ]),
        _Section(title: 'Meters', children: [
          _PrefRow(
              label: 'Refresh rate',
              trailing: _Select(
                  label: 'Refresh rate',
                  value: '${prefs.meterRate}',
                  options: const [
                    ('30', '30 fps'),
                    ('60', '60 fps'),
                  ],
                  onChanged: (v) async {
                    await prefs.setMeterRate(int.parse(v));
                    toast('Preference saved');
                  })),
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
                  })),
          _PrefRow(
              label: 'Hold clip indicators',
              trailing: _Toggle(
                  label: 'Hold clip indicators',
                  value: prefs.holdClips,
                  onChanged: (v) async {
                    await prefs.setHoldClips(v);
                    toast('Preference saved');
                  })),
        ]),
        _Section(title: 'Touch controls', children: [
          _PrefRow(
              label: 'Larger fader touch targets',
              trailing: _Toggle(
                  label: 'Larger fader touch targets',
                  value: prefs.largeTargets,
                  onChanged: (v) async {
                    await prefs.setLargeTargets(v);
                    toast('Preference saved');
                  })),
        ]),
      ],
    );
  }
}

// ---------------------------------------------------------------------------
// About detail.
// ---------------------------------------------------------------------------

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
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!wide)
          const Padding(
            padding: EdgeInsets.fromLTRB(4, 2, 4, 24),
            child: Row(children: [
              SizedBox(
                  width: 64,
                  height: 64,
                  child: SvgPictureAssetLogo()),
              SizedBox(width: 18),
              Text('OpenFader',
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 26,
                      fontWeight: FontWeight.w600,
                      color: U4.text1)),
            ]),
          ),
        _Section(title: 'Support', children: [
          _LinkRow(
              label: 'Report a bug',
              onTap: () =>
                  _open('$_repo/issues/new?labels=bug')),
          _LinkRow(
              label: 'Request a feature',
              onTap: () =>
                  _open('$_repo/issues/new?labels=enhancement')),
          _LinkRow(
              label: 'Export diagnostics',
              desc:
                  'App settings and counts. No mixer addresses or mix contents.',
              icon: Icons.download_outlined,
              onTap: () async {
                await store.exportDiagnostics();
                toast('Diagnostics exported');
              }),
        ]),
        _Section(title: 'Connection help', children: [
          _HelpAccordion(
              title: 'My mixer does not appear',
              body:
                  'Connect this device and the mixer to the same network. Refresh recent mixers or use Discover on Connections.\n\nYou can also add a mixer manually with its IP address or hostname.'),
          _HelpAccordion(
              title: 'A mixer will not reconnect',
              body:
                  'Check the mixer is powered on and its network address has not changed. Try a fresh connection from the Connections screen.'),
        ]),
        _Section(title: 'Project', children: [
          _LinkRow(
              label: 'Source code',
              desc: 'github.com/gladsonsam/OpenFader',
              onTap: () => _open(_repo)),
          _LinkRow(
              label: 'Contributors',
              onTap: () => _open('$_repo/graphs/contributors')),
          _LinkRow(
              label: 'Licences & credits',
              onTap: () => showDialog<void>(
                  context: context,
                  builder: (context) => AlertDialog(
                        backgroundColor: const Color(0xff262626),
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(14),
                            side: const BorderSide(
                                color: Color(0xff555555))),
                        title: const Text('Licences & credits',
                            style: TextStyle(
                                fontFamily: U4.font,
                                fontSize: 23,
                                fontWeight: FontWeight.w600,
                                color: U4.text1)),
                        content: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment:
                              CrossAxisAlignment.start,
                          children: [
                            const Text(
                                'OpenFader is free and open source. The project repository contains the current licence and contributor information.',
                                style: TextStyle(
                                    fontFamily: U4.font,
                                    fontSize: 17,
                                    height: 1.5,
                                    color: Color(0xffb5b5b5))),
                            const SizedBox(height: 12),
                            const Text(
                                'Interface typeface: Barlow. Toolbar symbols and the OpenFader logo are vector artwork.',
                                style: TextStyle(
                                    fontFamily: U4.font,
                                    fontSize: 17,
                                    height: 1.5,
                                    color: Color(0xffb5b5b5))),
                            const SizedBox(height: 16),
                            GestureDetector(
                              onTap: () => _open(_repo),
                              child: const Text(
                                  'Open project repository',
                                  style: TextStyle(
                                      fontFamily: U4.font,
                                      fontSize: 17,
                                      fontWeight: FontWeight.w600,
                                      color: U4.cyan)),
                            ),
                          ],
                        ),
                        actions: [
                          TextButton(
                              onPressed: () =>
                                  Navigator.pop(context),
                              child: const Text('Close',
                                  style: TextStyle(
                                      fontFamily: U4.font,
                                      fontSize: 17,
                                      color: U4.text1))),
                        ],
                      ))),
        ]),
        const Padding(
          padding: EdgeInsets.only(top: 28, bottom: 12),
          child: Text('Made by Gladson Sam',
              textAlign: TextAlign.center,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 13,
                  color: Color(0xffaaaaaa))),
        ),
      ],
    );
  }
}

class _LinkRow extends StatelessWidget {
  final String label;
  final String? desc;
  final IconData icon;
  final VoidCallback onTap;
  const _LinkRow(
      {required this.label,
      this.desc,
      this.icon = Icons.open_in_new,
      required this.onTap});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: onTap,
      child: GestureDetector(
        onTap: onTap,
        behavior: HitTestBehavior.opaque,
        child: Padding(
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 26 : 16,
              vertical: wide ? 22 : 16),
          child: Row(children: [
            Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(label,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: wide ? 27 : 18,
                          height: 1.25,
                          fontWeight: FontWeight.w500,
                          color: U4.text1)),
                  if (desc != null) ...[
                    const SizedBox(height: 7),
                    Text(desc!,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            height: 1.4,
                            color: const Color(0xffb5b5b5))),
                  ],
                ])),
            const SizedBox(width: 16),
            Icon(icon, size: 22, color: const Color(0xffb5b5b5)),
          ]),
        ),
      ),
    );
  }
}

class _HelpAccordion extends StatelessWidget {
  final String title;
  final String body;
  const _HelpAccordion({required this.title, required this.body});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Theme(
      data: Theme.of(context)
          .copyWith(dividerColor: Colors.transparent),
      child: ExpansionTile(
        tilePadding: EdgeInsets.symmetric(
            horizontal: wide ? 26 : 16,
            vertical: wide ? 10 : 6),
        childrenPadding: EdgeInsets.fromLTRB(
            wide ? 26 : 16, 0, wide ? 26 : 16, wide ? 24 : 18),
        title: Text(title,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 27 : 18,
                fontWeight: FontWeight.w500,
                color: U4.text1)),
        iconColor: const Color(0xffb5b5b5),
        collapsedIconColor: const Color(0xffb5b5b5),
        children: [
          Align(
            alignment: Alignment.centerLeft,
            child: Text(body,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 23 : 15,
                    height: 1.5,
                    color: const Color(0xffb5b5b5))),
          ),
        ],
      ),
    );
  }
}
