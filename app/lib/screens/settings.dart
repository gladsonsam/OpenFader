// Settings: tablet sidebar and detail split with a phone overview list and subpages.
// All preferences and mixes use the real services.
// Screens stay constructor-injected so Area A can implement the store.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:url_launcher/url_launcher.dart';

import '../settings/mixes.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../theme.dart';
import '../palette.dart';

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

// Implemented by the shell over prefs, recents, mixes, and platform plugins.
abstract class SettingsStore extends ChangeNotifier {
  PrefsService get prefs;
  RecentMixers get recents;
  LocalMixStore get mixes;
  Future<int> appStorageBytes();

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
  final SettingsSection? initialSection;
  final VoidCallback onClose;
  final VoidCallback onOpenMixer;

  const U4SettingsPage(
      {super.key,
      this.initialSection,
      required this.store,
      required this.onClose,
      required this.onOpenMixer});

  @override
  State<U4SettingsPage> createState() => _U4SettingsPageState();
}

class _U4SettingsPageState extends State<U4SettingsPage> {
  SettingsSection _sel = SettingsSection.connections;
  SettingsSection? _page;

  @override
  void initState() {
    super.initState();
    _page = widget.initialSection;
    _sel = widget.initialSection ?? SettingsSection.connections;
  }

  void _toast(String msg) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).hideCurrentSnackBar();
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(
        content: Text(msg,
            textAlign: TextAlign.center,
            style: const TextStyle(fontFamily: U4.font, fontSize: 17)),
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
        backgroundColor: ConsolePalette.of(context).chip,
        shape: RoundedRectangleBorder(
            borderRadius: BorderRadius.circular(14),
            side: BorderSide(color: ConsolePalette.of(context).lineCtl)),
        title: Text(title,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 30 : 23,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).text1)),
        content: Text(body,
            style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 25 : 17,
                height: 1.5,
                color: ConsolePalette.of(context).text7)),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text('Cancel',
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 22 : 17,
                      color: ConsolePalette.of(context).text1))),
          TextButton(
              onPressed: () => Navigator.pop(context, true),
              style: TextButton.styleFrom(
                  backgroundColor: ConsolePalette.of(context).mute,
                  padding: EdgeInsets.symmetric(
                      horizontal: 22, vertical: wide ? 12 : 10),
                  shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(9))),
              child: Text(action,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 22 : 17,
                      fontWeight: FontWeight.w600,
                      color: ConsolePalette.of(context).onDanger))),
        ],
      ),
    );
    if (ok == true) {
      try {
        await onConfirm();
        if (mounted) setState(() {});
      } catch (error) {
        _toast('Could not complete the action: $error');
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Scaffold(
      backgroundColor: ConsolePalette.of(context).page,
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

  Widget _topbar({required String title, required bool showBack}) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Container(
      height: wide ? 104 : 88,
      padding: wide
          ? const EdgeInsets.symmetric(horizontal: 32)
          : const EdgeInsets.fromLTRB(16, 22, 16, 12),
      decoration: BoxDecoration(
          color: ConsolePalette.of(context).headerBg,
          border: Border(
              bottom: BorderSide(color: ConsolePalette.of(context).line))),
      child: Row(children: [
        if (!wide && showBack)
          _IconBtn(
              tooltip: _page == null
                  ? 'Back to previous screen'
                  : 'Back to Settings',
              onTap: _page == null
                  ? widget.onClose
                  : () => setState(() => _page = null),
              child: _SettingsIcon(Icons.arrow_back,
                  size: 22, color: ConsolePalette.of(context).text7)),
        if (!wide && showBack) const SizedBox(width: 14),
        Expanded(
            child: Text(title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 34 : 22,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).text1))),
        _IconBtn(
            tooltip: 'Close Settings',
            onTap: widget.onClose,
            child: _SettingsIcon(Icons.close,
                size: 22, color: ConsolePalette.of(context).text7)),
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
              _Sidebar(active: _sel, onSelect: (s) => setState(() => _sel = s)),
              Expanded(child: _detailScroll(_sel, wide: true)),
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
        _topbar(title: 'Settings', showBack: true),
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
                        section: s, onTap: () => setState(() => _page = s)),
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
          child: _detailScroll(s, wide: false),
        ),
      ],
    );
  }

  Widget _detailScroll(SettingsSection section, {required bool wide}) {
    final about = section == SettingsSection.about;
    final padding = EdgeInsets.fromLTRB(wide ? 56 : 16, wide ? 44 : 20,
        wide ? 56 : 16, wide ? 60 : (about ? 12 : 36));
    return LayoutBuilder(builder: (context, viewport) {
      final detail = ConstrainedBox(
        constraints: BoxConstraints(
            maxWidth: wide ? 1200 : double.infinity,
            minHeight: about
                ? (viewport.maxHeight - padding.vertical)
                    .clamp(0.0, double.infinity)
                : 0),
        child:
            about ? IntrinsicHeight(child: _detail(section)) : _detail(section),
      );
      return SingleChildScrollView(
          padding: padding,
          child: Align(alignment: Alignment.topLeft, child: detail));
    });
  }

  Widget _detail(SettingsSection s) {
    return _SettingsLayout(section: s, child: _detailBody(s));
  }

  Widget _detailBody(SettingsSection s) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (wide)
          Padding(
            padding: const EdgeInsets.only(bottom: 30),
            child: Text(s.label,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: 36,
                    height: 1.22,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).text1)),
          ),
        switch (s) {
          SettingsSection.connections => _ConnectionsDetail(
              store: widget.store, toast: _toast, confirm: _confirm),
          SettingsSection.mixes => _MixesDetail(
              store: widget.store,
              toast: _toast,
              confirm: _confirm,
              onOpenMixer: widget.onOpenMixer),
          SettingsSection.data =>
            _DataDetail(store: widget.store, toast: _toast, confirm: _confirm),
          SettingsSection.appearance =>
            _AppearanceDetail(store: widget.store, toast: _toast),
          SettingsSection.about =>
            Expanded(child: _AboutDetail(store: widget.store, toast: _toast)),
        },
      ],
    );
  }
}

class _IconBtn extends StatelessWidget {
  final String tooltip;
  final FutureOr<void> Function() onTap;
  final Widget child;
  const _IconBtn(
      {required this.tooltip, required this.onTap, required this.child});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: tooltip,
      onTap: () => _runSettingsAction(context, onTap),
      child: Tooltip(
          message: tooltip,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: () => _runSettingsAction(context, onTap),
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).line)),
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
      decoration: BoxDecoration(
          color: ConsolePalette.of(context).headerBg,
          border: Border(
              right: BorderSide(color: ConsolePalette.of(context).line))),
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
                            ? ConsolePalette.of(context).selectionFill
                            : Colors.transparent,
                        borderRadius: BorderRadius.circular(12)),
                    child: Row(children: [
                      _SettingsIcon(s.icon,
                          size: 26,
                          color: s == active
                              ? ConsolePalette.of(context).cyan
                              : ConsolePalette.of(context).text7),
                      const SizedBox(width: 16),
                      Expanded(
                          child: Text(s.label,
                              style: TextStyle(
                                  fontFamily: U4.font,
                                  fontSize: 25,
                                  fontWeight: FontWeight.w500,
                                  color: s == active
                                      ? ConsolePalette.of(context).cyan
                                      : ConsolePalette.of(context).text7))),
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
                    child: SvgPicture.asset('assets/brand/open-fader-logo.svg',
                        semanticsLabel: 'OpenFader')),
                const SizedBox(width: 14),
                Text('OpenFader',
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 25,
                        fontWeight: FontWeight.w600,
                        color: ConsolePalette.of(context).text1)),
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
  Widget build(BuildContext context) => Padding(
        padding: EdgeInsets.fromLTRB(4, 6, 4, 0),
        child: Row(children: [
          SizedBox(width: 54, height: 54, child: SvgPictureAssetLogo()),
          SizedBox(width: 14),
          Text('OpenFader',
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: 25,
                  fontWeight: FontWeight.w600,
                  color: ConsolePalette.of(context).text1)),
        ]),
      );
}

class SvgPictureAssetLogo extends StatelessWidget {
  const SvgPictureAssetLogo({super.key});
  @override
  Widget build(BuildContext context) =>
      SvgPicture.asset('assets/brand/open-fader-logo.svg',
          semanticsLabel: 'OpenFader');
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
                  borderRadius: BorderRadius.circular(12)),
              child: Row(children: [
                _SettingsIcon(section.icon,
                    size: 24, color: ConsolePalette.of(context).text7),
                const SizedBox(width: 16),
                Expanded(
                    child: Text(section.label,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: 19,
                            fontWeight: FontWeight.w500,
                            color: ConsolePalette.of(context).text1))),
                _SettingsIcon(Icons.chevron_right,
                    size: 24, color: ConsolePalette.of(context).text7),
              ]))));
}

class _Section extends StatelessWidget {
  final String title;
  final List<Widget> children;
  final bool first;
  const _Section(
      {required this.title, required this.children, this.first = false});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
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
              wide ? 14 : 10),
          child: Text(title,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 26 : 17,
                  height: wide ? 34 / 26 : 23 / 17,
                  fontWeight: FontWeight.w500,
                  color: ConsolePalette.of(context).text7)),
        ),
        Container(
          decoration: BoxDecoration(
              color: ConsolePalette.of(context).chip,
              borderRadius: BorderRadius.circular(wide ? 14 : 12)),
          clipBehavior: Clip.antiAlias,
          child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: _divided(context, children)),
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
                          bottom: BorderSide(
                              color: ConsolePalette.of(context).line))
                      : null),
              child: children[i]),
      ];
}

class _Note extends StatelessWidget {
  final String text;
  const _Note(this.text);

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.fromLTRB(4, wide ? 14 : 12, 4, 0),
      child: Text(text,
          style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 23 : 15,
              height: wide ? 1.5 : 22 / 15,
              color: ConsolePalette.of(context).panFill)),
    );
  }
}

class _PrefRow extends StatelessWidget {
  final String label;
  final String? desc;
  final Widget trailing;
  const _PrefRow({required this.label, this.desc, required this.trailing});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final compact = !wide && _SettingsLayout.isConnections(context);
    return Container(
      constraints: BoxConstraints(minHeight: wide ? 96 : (compact ? 66 : 76)),
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16,
          vertical: wide ? 22 : (compact ? 12 : 16)),
      child: Row(children: [
        Expanded(
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(label,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 27 : 18,
                  height: wide ? 1.25 : 24 / 18,
                  fontWeight: FontWeight.w500,
                  color: ConsolePalette.of(context).text1)),
          if (desc != null) ...[
            SizedBox(height: wide ? 7 : 4),
            Text(desc!,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 23 : 15,
                    height: wide ? 1.4 : 21 / 15,
                    color: ConsolePalette.of(context).text7)),
          ],
        ])),
        SizedBox(width: wide ? 24 : 16),
        trailing,
      ]),
    );
  }
}

class _Toggle extends StatelessWidget {
  final bool value;
  final Future<void> Function(bool) onChanged;
  final String label;
  const _Toggle(
      {required this.value, required this.onChanged, required this.label});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    final w = wide ? 60.0 : 50.0;
    final h = wide ? 36.0 : 30.0;
    final knob = wide ? 28.0 : 22.0;
    return Semantics(
      button: true,
      label: '$label ${value ? 'on' : 'off'}',
      onTap: () => _runSettingsAction(context, () => onChanged(!value)),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, () => onChanged(!value)),
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
                      ? ConsolePalette.of(context).cyan
                      : ConsolePalette.of(context).control,
                  borderRadius: BorderRadius.circular(h / 2),
                  border: Border.all(
                      color: value
                          ? ConsolePalette.of(context).cyan
                          : ConsolePalette.of(context).lineCtl)),
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
                                ? ConsolePalette.of(context).onAccent
                                : ConsolePalette.of(context).text))),
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
  final Future<void> Function(String) onChanged;
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
          maxHeight: wide ? 60 : 44,
          maxWidth: wide ? 360 : 174),
      padding: EdgeInsets.symmetric(horizontal: wide ? 16 : 12),
      decoration: BoxDecoration(
          color: ConsolePalette.of(context).control,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: ConsolePalette.of(context).lineCtl)),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<String>(
          isDense: true,
          value: value,
          dropdownColor: ConsolePalette.of(context).chip,
          icon: _SettingsIcon(Icons.arrow_drop_down,
              size: 18, color: ConsolePalette.of(context).text7),
          style: TextStyle(
              fontFamily: U4.font,
              fontSize: wide ? 27 : 17,
              color: ConsolePalette.of(context).text1),
          items: [
            for (final o in options)
              DropdownMenuItem(
                  value: o.$1,
                  child: Text(o.$2, overflow: TextOverflow.ellipsis)),
          ],
          onChanged: (v) {
            if (v != null) {
              _runSettingsAction(context, () => onChanged(v));
            }
          },
        ),
      ),
    );
  }
}

class _ActionRow extends StatelessWidget {
  final String label;
  final String? desc;
  final bool danger;
  final FutureOr<void> Function() onTap;
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
      onTap: () => _runSettingsAction(context, onTap),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, onTap),
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints(minHeight: wide ? 96 : 76),
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 26 : 16, vertical: wide ? 22 : 16),
          child: Row(children: [
            Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(label,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: wide ? 27 : 18,
                          height: wide ? 1.25 : 24 / 18,
                          fontWeight: FontWeight.w500,
                          color: danger
                              ? ConsolePalette.of(context).mute
                              : ConsolePalette.of(context).text1)),
                  if (desc != null) ...[
                    SizedBox(height: wide ? 7 : 4),
                    Text(desc!,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            height: wide ? 1.4 : 21 / 15,
                            color: ConsolePalette.of(context).text7)),
                  ],
                ])),
            const SizedBox(width: 16),
            _SettingsIcon(Icons.chevron_right,
                size: 24, color: ConsolePalette.of(context).text7),
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
            height: wide ? 1.3 : 24 / 18,
            color: ConsolePalette.of(context).text7,
            fontFeatures: const [FontFeature.tabularFigures()]));
  }
}

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
        _Section(first: true, title: 'Connection behaviour', children: [
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
          _PrefRow(label: 'XR18', trailing: _ValueText('Behringer')),
          _PrefRow(label: 'DL32S', trailing: _ValueText('Mackie')),
        ]),
      ],
    );
  }
}

class _SavedMixerRow extends StatelessWidget {
  final String name;
  final String addr;
  final FutureOr<void> Function() onForget;
  const _SavedMixerRow(
      {required this.name, required this.addr, required this.onForget});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16, vertical: wide ? 22 : 12),
      child: Row(children: [
        Expanded(
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 27 : 18,
                  height: wide ? 1.25 : 24 / 18,
                  fontWeight: FontWeight.w500,
                  color: ConsolePalette.of(context).text1)),
          SizedBox(height: wide ? 7 : 4),
          Text(addr,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 23 : 14,
                  height: wide ? 1.4 : 1.5,
                  color: ConsolePalette.of(context).text7,
                  fontFeatures: const [FontFeature.tabularFigures()])),
        ])),
        _IconBtnInline(tooltip: 'Forget $name', onTap: onForget),
      ]),
    );
  }
}

class _IconBtnInline extends StatelessWidget {
  final String tooltip;
  final FutureOr<void> Function() onTap;
  final IconData icon;
  const _IconBtnInline(
      {required this.tooltip,
      required this.onTap,
      this.icon = Icons.delete_outline});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: tooltip,
      onTap: () => _runSettingsAction(context, onTap),
      child: Tooltip(
          message: tooltip,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: () => _runSettingsAction(context, onTap),
                  child: _SettingsIcon(icon,
                      size: 22, color: ConsolePalette.of(context).text7)))));
}

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
        _Section(first: true, title: 'Local mixes', children: [
          if (mixes.isEmpty)
            Padding(
              padding: EdgeInsets.symmetric(
                  horizontal: wide ? 26 : 16, vertical: wide ? 32 : 22),
              child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('No mixes saved yet',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 27 : 18,
                            height: wide ? 1.25 : 24 / 18,
                            fontWeight: FontWeight.w500,
                            color: ConsolePalette.of(context).text1)),
                    SizedBox(height: wide ? 7 : 4),
                    Text(
                        'Import a mix, or open the offline mixer to prepare one.',
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            height: wide ? 1.4 : 21 / 15,
                            color: ConsolePalette.of(context).text7)),
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
          child: Wrap(
              spacing: wide ? 14 : 10,
              runSpacing: wide ? 14 : 10,
              children: [
                _PrimaryButton(
                    label: 'Import mix',
                    icon: Icons.upload_outlined,
                    onTap: () async {
                      try {
                        final err = await store.importMix();
                        if (err == null) {
                          toast('Mix imported');
                        } else if (err != 'cancelled') {
                          toast(err);
                        }
                      } catch (error) {
                        toast('$error');
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
                    if (v) {
                      await store.mixes.snapshotBackup();
                      await store.prefs
                          .setBackupCreatedAt(DateTime.now().toIso8601String());
                    }
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
          child: Wrap(
              spacing: wide ? 14 : 10,
              runSpacing: wide ? 14 : 10,
              children: [
                _PlainButton(
                    label: 'Export backup',
                    icon: Icons.download_outlined,
                    onTap: () async {
                      await store.exportBackup();
                      toast('Backup exported');
                    }),
                _PlainButton(
                    label: 'Import backup',
                    icon: Icons.upload_outlined,
                    onTap: () async {
                      final res = await store.importBackup();
                      if (res == 'cancelled') return;
                      final n = int.tryParse(res);
                      if (n == null) {
                        toast(res);
                      } else if (n == 0) {
                        toast('Nothing valid to import');
                      } else {
                        toast('$n mix${n == 1 ? '' : 'es'} imported');
                      }
                    }),
              ]),
        ),
        const _Note(
            'Backups contain local mixes only. Processor presets and local scenes/shows are not included. Keep an exported backup somewhere safe before deleting mixes or resetting app data.'),
      ],
    );
  }
}

class _MixRow extends StatelessWidget {
  final LocalMix mix;
  final FutureOr<void> Function() onExport;
  final FutureOr<void> Function() onDelete;
  const _MixRow(
      {required this.mix, required this.onExport, required this.onDelete});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Padding(
      padding: EdgeInsets.symmetric(
          horizontal: wide ? 26 : 16, vertical: wide ? 22 : 16),
      child: Row(children: [
        Expanded(
            child:
                Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
          Text(mix.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 27 : 18,
                  fontWeight: FontWeight.w500,
                  color: ConsolePalette.of(context).text1)),
          SizedBox(height: wide ? 7 : 4),
          Text(mix.mixerModel,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 23 : 14,
                  color: ConsolePalette.of(context).text7)),
        ])),
        _IconBtnInline(
            tooltip: 'Export ${mix.name}',
            onTap: onExport,
            icon: Icons.download_outlined),
        const SizedBox(width: 6),
        _IconBtnInline(tooltip: 'Delete ${mix.name}', onTap: onDelete),
      ]),
    );
  }
}

class _PrimaryButton extends StatelessWidget {
  final String label;
  final FutureOr<void> Function() onTap;
  final IconData? icon;
  const _PrimaryButton({required this.label, required this.onTap, this.icon});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: () => _runSettingsAction(context, onTap),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, onTap),
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints(minHeight: wide ? 62 : 44),
          padding:
              EdgeInsets.symmetric(horizontal: wide ? 22 : 14, vertical: 10),
          decoration: BoxDecoration(
              color: ConsolePalette.of(context).cyan,
              borderRadius: BorderRadius.circular(9)),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            if (icon != null) ...[
              _SettingsIcon(icon!,
                  size: wide ? 22 : 18,
                  color: ConsolePalette.of(context).onAccent),
              SizedBox(width: wide ? 12 : 8),
            ],
            Text(label,
                textAlign: TextAlign.center,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 27 : 17,
                    fontWeight: FontWeight.w600,
                    color: ConsolePalette.of(context).onAccent)),
          ]),
        ),
      ),
    );
  }
}

class _PlainButton extends StatelessWidget {
  final String label;
  final FutureOr<void> Function() onTap;
  final IconData? icon;
  const _PlainButton({required this.label, required this.onTap, this.icon});

  @override
  Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Semantics(
      button: true,
      label: label,
      onTap: () => _runSettingsAction(context, onTap),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, onTap),
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints(minHeight: wide ? 62 : 44),
          padding:
              EdgeInsets.symmetric(horizontal: wide ? 22 : 14, vertical: 10),
          decoration: BoxDecoration(
              color: ConsolePalette.of(context).control,
              borderRadius: BorderRadius.circular(9),
              border: Border.all(color: ConsolePalette.of(context).lineCtl)),
          child: Row(mainAxisSize: MainAxisSize.min, children: [
            if (icon != null) ...[
              _SettingsIcon(icon!,
                  size: wide ? 22 : 18,
                  color: ConsolePalette.of(context).text1),
              SizedBox(width: wide ? 12 : 8),
            ],
            Text(label,
                textAlign: TextAlign.center,
                style: TextStyle(
                    fontFamily: U4.font,
                    fontSize: wide ? 27 : 17,
                    fontWeight: FontWeight.w500,
                    color: ConsolePalette.of(context).text1)),
          ]),
        ),
      ),
    );
  }
}

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
        _Section(first: true, title: 'On this device', children: [
          _PrefRow(
              label: 'Local mixes',
              trailing: _ValueText('${store.mixes.mixes.length}')),
          _PrefRow(
              label: 'Saved mixers',
              trailing: _ValueText('${store.recents.items.length}')),
          _PrefRow(
              label: 'App storage',
              trailing: FutureBuilder<int>(
                  future: store.appStorageBytes(),
                  builder: (context, snap) => _ValueText(snap.hasError
                      ? 'Unavailable'
                      : (snap.hasData ? _bytes(snap.data!) : 'Calculating')))),
        ]),
        _Section(title: 'Manage data', children: [
          _ActionRow(
              label: 'Clear cache',
              desc: 'Remove temporary app files. Your mixes stay saved.',
              onTap: () => confirm(
                  title: 'Clear cache?',
                  body:
                      'Temporary app files will be removed. Local mixes, scenes/shows, saved mixers and preferences will stay.',
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
              desc:
                  'Remove local mixes and backups; keep scenes/shows and processor presets.',
              danger: true,
              onTap: () => confirm(
                  title: 'Delete local mixes?',
                  body:
                      '${store.mixes.mixes.length} local mix(es) and local backups will be removed. Scenes/shows and processor presets will stay. Export a backup first if you want to keep them.',
                  action: 'Delete mixes',
                  onConfirm: () async {
                    await store.deleteMixes();
                    toast('Local mixes deleted');
                  })),
          _ActionRow(
              label: 'Reset all app data',
              desc:
                  'Remove local mixes, scenes/shows, processor presets and preferences.',
              danger: true,
              onTap: () => confirm(
                  title: 'Reset all app data?',
                  body:
                      'Local scenes/shows will be reset through the engine first, then local mixes, backups, processor presets, saved mixers, cache and preferences will be removed. A connected mixer will not be reset.',
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
        _Section(first: true, title: 'Display', children: [
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
                  })),
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
                  })),
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
          Padding(
            padding: EdgeInsets.fromLTRB(4, 2, 4, 12),
            child: Row(children: [
              SizedBox(width: 64, height: 64, child: SvgPictureAssetLogo()),
              SizedBox(width: 18),
              Text('OpenFader',
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: 26,
                      fontWeight: FontWeight.w600,
                      color: ConsolePalette.of(context).text1)),
            ]),
          ),
        _Section(first: true, title: 'Support', children: [
          _LinkRow(
              label: 'Report a bug',
              onTap: () => _open('$_repo/issues/new?labels=bug')),
          _LinkRow(
              label: 'Request a feature',
              onTap: () => _open('$_repo/issues/new?labels=enhancement')),
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
              icon: Icons.chevron_right,
              onTap: () => showDialog<void>(
                  context: context,
                  builder: (context) => AlertDialog(
                        backgroundColor: ConsolePalette.of(context).chip,
                        shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(14),
                            side: BorderSide(
                                color: ConsolePalette.of(context).lineCtl)),
                        title: Text('Licences & credits',
                            style: TextStyle(
                                fontFamily: U4.font,
                                fontSize: 23,
                                fontWeight: FontWeight.w600,
                                color: ConsolePalette.of(context).text1)),
                        content: Column(
                          mainAxisSize: MainAxisSize.min,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                                'OpenFader is free and open source. The project repository contains the current licence and contributor information.',
                                style: TextStyle(
                                    fontFamily: U4.font,
                                    fontSize: 17,
                                    height: 1.5,
                                    color: ConsolePalette.of(context).text7)),
                            const SizedBox(height: 12),
                            Text(
                                'Interface typeface: Barlow. Toolbar symbols and the OpenFader logo are vector artwork.',
                                style: TextStyle(
                                    fontFamily: U4.font,
                                    fontSize: 17,
                                    height: 1.5,
                                    color: ConsolePalette.of(context).text7)),
                            const SizedBox(height: 16),
                            GestureDetector(
                              onTap: () => _runSettingsAction(
                                  context, () => _open(_repo)),
                              child: Text('Open project repository',
                                  style: TextStyle(
                                      fontFamily: U4.font,
                                      fontSize: 17,
                                      fontWeight: FontWeight.w600,
                                      color: ConsolePalette.of(context).cyan)),
                            ),
                          ],
                        ),
                        actions: [
                          TextButton(
                              onPressed: () => Navigator.pop(context),
                              child: Text('Close',
                                  style: TextStyle(
                                      fontFamily: U4.font,
                                      fontSize: 17,
                                      color:
                                          ConsolePalette.of(context).text1))),
                        ],
                      ))),
        ]),
        const Spacer(),
        Padding(
          padding: EdgeInsets.only(top: wide ? 18 : 16),
          child: Text('Made by Gladson Sam',
              textAlign: TextAlign.center,
              style: TextStyle(
                  fontFamily: U4.font,
                  fontSize: wide ? 18 : 13,
                  height: wide ? 24 / 18 : 18 / 13,
                  color: ConsolePalette.of(context).panFill)),
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
      onTap: () => _runSettingsAction(context, onTap),
      child: GestureDetector(
        onTap: () => _runSettingsAction(context, onTap),
        behavior: HitTestBehavior.opaque,
        child: Container(
          constraints: BoxConstraints(minHeight: wide ? 96 : 76),
          padding: EdgeInsets.symmetric(
              horizontal: wide ? 26 : 16, vertical: wide ? 22 : 16),
          child: Row(children: [
            Expanded(
                child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                  Text(label,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: wide ? 27 : 18,
                          height: wide ? 1.25 : 24 / 18,
                          fontWeight: FontWeight.w500,
                          color: ConsolePalette.of(context).text1)),
                  if (desc != null) ...[
                    SizedBox(height: wide ? 7 : 4),
                    Text(desc!,
                        style: TextStyle(
                            fontFamily: U4.font,
                            fontSize: wide ? 23 : 15,
                            height: wide ? 1.4 : 21 / 15,
                            color: ConsolePalette.of(context).text7)),
                  ],
                ])),
            const SizedBox(width: 16),
            _SettingsIcon(icon,
                size: 22, color: ConsolePalette.of(context).text7),
          ]),
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
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
      Semantics(
        button: true,
        expanded: _expanded,
        label: widget.title,
        child: InkWell(
          onTap: () => setState(() => _expanded = !_expanded),
          child: Container(
            constraints: BoxConstraints(minHeight: wide ? 88 : 70),
            padding: EdgeInsets.symmetric(
                horizontal: wide ? 26 : 16, vertical: wide ? 24 : 18),
            child: Row(children: [
              Expanded(
                  child: Text(widget.title,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: wide ? 27 : 18,
                          height: 1.2,
                          fontWeight: FontWeight.w500,
                          color: ConsolePalette.of(context).text1))),
              const SizedBox(width: 20),
              RotatedBox(
                  quarterTurns: _expanded ? 1 : 0,
                  child: const _SettingsIcon(Icons.chevron_right)),
            ]),
          ),
        ),
      ),
      if (_expanded)
        Padding(
          padding: EdgeInsets.fromLTRB(
              wide ? 26 : 16, 0, wide ? 26 : 16, wide ? 24 : 18),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            for (final (index, paragraph)
                in widget.body.split('\n\n').indexed) ...[
              if (index > 0) const SizedBox(height: 12),
              Text(paragraph,
                  style: TextStyle(
                      fontFamily: U4.font,
                      fontSize: wide ? 23 : 15,
                      height: wide ? 1.5 : 22 / 15,
                      color: ConsolePalette.of(context).text7)),
            ],
          ]),
        ),
    ]);
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

// Awaitable wrapper so gesture and semantics taps can call async actions.
Future<void> _runSettingsAction(
    BuildContext context, FutureOr<void> Function() action) async {
  try {
    await action();
  } on MixExportCancelled {
    return;
  } catch (error) {
    if (!context.mounted) return;
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(SnackBar(
        content: Text('Could not complete the action: $error',
            textAlign: TextAlign.center,
            style: const TextStyle(fontFamily: U4.font, fontSize: 17)),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3)));
  }
}

// Outline symbols match the handoff artwork instead of filled platform glyphs.
class _SettingsIcon extends StatelessWidget {
  final IconData icon;
  final double size;
  final Color? color;
  const _SettingsIcon(this.icon, {this.size = 22, this.color});

  @override
  Widget build(BuildContext context) {
    final color = this.color ?? ConsolePalette.of(context).text7;
    final body = switch (icon) {
      Icons.wifi =>
        '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M7 9h10M7 13h4M7 16h10"/>',
      Icons.library_music_outlined =>
        '<path d="M3 7h7l2 2h9v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7ZM3 7V5a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v2"/>',
      Icons.storage_outlined =>
        '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v14c0 4 16 4 16 0V5M4 12c0 4 16 4 16 0"/>',
      Icons.palette_outlined =>
        '<rect x="3" y="3" width="18" height="14" rx="2"/><path d="M8 21h8M12 17v4M7 8h10M7 12h6"/>',
      Icons.info_outline =>
        '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>',
      Icons.arrow_back => '<path d="m14 6-6 6 6 6M8 12h12"/>',
      Icons.close => '<path d="m6 6 12 12M18 6 6 18"/>',
      Icons.chevron_right => '<path d="m9 5 7 7-7 7"/>',
      Icons.arrow_drop_down => '<path d="m7 10 5 5 5-5"/>',
      Icons.open_in_new =>
        '<path d="M14 3h7v7M21 3 11 13M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5"/>',
      Icons.download_outlined =>
        '<path d="M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5"/>',
      Icons.upload_outlined => '<path d="M12 16V4m-5 5 5-5 5 5M4 16v5h16v-5"/>',
      Icons.delete_outline =>
        '<path d="M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7"/>',
      _ => null,
    };
    if (body == null) return Icon(icon, size: size, color: color);
    return SvgPicture.string(
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">$body</svg>',
        width: size,
        height: size,
        colorFilter: ColorFilter.mode(color, BlendMode.srcIn));
  }
}
