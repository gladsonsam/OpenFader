// Settings: tablet sidebar and detail split with a phone overview list and subpages.
// All preferences and mixes use the real services.
// Screens stay constructor-injected so Area A can implement the store.
library;

import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:url_launcher/url_launcher.dart';

import '../core/access_limits.dart';
import '../core/destination_controls.dart';
import '../widgets/destination_control_editor.dart';
import 'settings_access.dart';

import '../settings/mixes.dart';
import '../settings/prefs.dart';
import '../settings/recents.dart';
import '../mixer/components/layers.dart';
import '../mixer/mixer_layers.dart';
import '../mixer/strip_layout.dart';
import '../theme.dart';
import '../palette.dart';

part 'settings/navigation.dart';
part 'settings/section_widgets.dart';
part 'settings/controls.dart';
part 'settings/connections.dart';
part 'settings/mixer_setup.dart';
part 'settings/mixes.dart';
part 'settings/data.dart';
part 'settings/appearance.dart';
part 'settings/strip_layout.dart';
part 'settings/layers.dart';
part 'settings/about.dart';

enum SettingsSection {
  connections('Connections', Icons.wifi),
  mixer('Mixer setup', Icons.tune),
  mixes('Mixes & backups', Icons.library_music_outlined),
  access('Access limits', Icons.lock_outline),
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
  AccessLimitsStore get accessLimits;
  RecentMixers get recents;
  LocalMixStore get mixes;
  bool get mixerConnected;
  List<DestinationControlVm> get mixerSetupControls;
  int get controlAccessEpoch;
  bool mixerSetupControlIsCurrent(DestinationControlVm control);
  Future<String?> setMixerSetupControl(
      DestinationControlVm control, double value);

  /// Live channel catalog for the custom-layers editor (master excluded).
  List<({String key, String name})> get layerChannelOptions;
  Future<void> activateMixerLayer(String id);
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
  final bool embedded;
  final VoidCallback onOpenMixer;
  final VoidCallback onHome;

  const U4SettingsPage({
    super.key,
    this.initialSection,
    required this.store,
    required this.onClose,
    required this.onOpenMixer,
    required this.onHome,
    this.embedded = false,
  });

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
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          msg,
          textAlign: TextAlign.center,
          style: const TextStyle(fontFamily: U4.font, fontSize: 17),
        ),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3),
      ),
    );
  }

  Future<void> _confirm({
    required String title,
    required String body,
    required String action,
    required Future<void> Function() onConfirm,
  }) async {
    final accessEpoch = widget.store.accessLimits.controlAccessEpoch;
    final wide = MediaQuery.sizeOf(context).width > 900;
    final ok = await showDialog<bool>(
      animationStyle: AnimationStyle.noAnimation,
      context: context,
      builder: (context) => AlertDialog(
        backgroundColor: ConsolePalette.of(context).chip,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(14),
          side: BorderSide(color: ConsolePalette.of(context).lineCtl),
        ),
        title: Text(
          title,
          style: TextStyle(
            fontFamily: U4.font,
            fontSize: 22,
            fontWeight: FontWeight.w600,
            color: ConsolePalette.of(context).text1,
          ),
        ),
        content: Text(
          body,
          style: TextStyle(
            fontFamily: U4.font,
            fontSize: 16,
            height: 1.5,
            color: ConsolePalette.of(context).text7,
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(
              'Cancel',
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: 16,
                color: ConsolePalette.of(context).text1,
              ),
            ),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            style: TextButton.styleFrom(
              backgroundColor: ConsolePalette.of(context).mute,
              padding: EdgeInsets.symmetric(
                horizontal: 22,
                vertical: wide ? 12 : 10,
              ),
              shape: RoundedRectangleBorder(
                borderRadius: BorderRadius.circular(9),
              ),
            ),
            child: Text(
              action,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: 16,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).onDanger,
              ),
            ),
          ),
        ],
      ),
    );
    if (ok == true) {
      try {
        if (widget.store.accessLimits.controlAccessEpoch != accessEpoch) {
          throw StateError('Access policy changed. Reopen this confirmation.');
        }
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
      // Dialogs handle their own keyboard insets; keep navigation at full height.
      resizeToAvoidBottomInset: false,
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
    if (widget.embedded) {
      if (title == 'Settings') return const SizedBox.shrink();
      return SizedBox(
          height: 52,
          child: Row(children: [
            IconButton(
                tooltip: 'Settings',
                onPressed: () => setState(() => _page = null),
                icon: const Icon(Icons.chevron_left)),
            Expanded(
                child: Text(title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                        fontSize: 18, fontWeight: FontWeight.w600))),
          ]));
    }
    final wide = MediaQuery.sizeOf(context).width > 900;
    return Container(
      height: wide ? 104 : 88,
      padding: wide
          ? const EdgeInsets.symmetric(horizontal: 32)
          : const EdgeInsets.fromLTRB(16, 22, 16, 12),
      decoration: BoxDecoration(
        color: ConsolePalette.of(context).headerBg,
        border: Border(
          bottom: BorderSide(color: ConsolePalette.of(context).line),
        ),
      ),
      child: Row(
        children: [
          if (!wide && showBack)
            _IconBtn(
              tooltip: _page == null
                  ? 'Back to previous screen'
                  : 'Back to Settings',
              onTap: _page == null
                  ? widget.onClose
                  : () => setState(() => _page = null),
              child: _SettingsIcon(
                Icons.arrow_back,
                size: 22,
                color: ConsolePalette.of(context).text7,
              ),
            ),
          if (!wide && showBack) const SizedBox(width: 14),
          Expanded(
            child: Text(
              title,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: wide ? 34 : 22,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).text1,
              ),
            ),
          ),
          _IconBtn(
            tooltip: 'Close Settings',
            onTap: widget.onClose,
            child: _SettingsIcon(
              Icons.close,
              size: 22,
              color: ConsolePalette.of(context).text7,
            ),
          ),
        ],
      ),
    );
  }

  Widget _tablet() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _topbar(title: 'Settings', showBack: false),
        const SizedBox(height: 16),
        Expanded(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _Sidebar(
                  active: _sel,
                  onSelect: (s) => setState(() => _sel = s),
                  onHome: widget.onHome),
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
                if (!widget.embedded) ...[
                  _OverviewBrand(onHome: widget.onHome),
                  const SizedBox(height: 24),
                ],
                for (final s in SettingsSection.values)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 10),
                    child: _OverviewCard(
                      section: s,
                      onTap: () => setState(() => _page = s),
                    ),
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
        Expanded(child: _detailScroll(s, wide: false)),
      ],
    );
  }

  Widget _detailScroll(SettingsSection section, {required bool wide}) {
    final about = section == SettingsSection.about;
    final padding = EdgeInsets.fromLTRB(
      wide ? 24 : 16,
      wide ? 20 : 16,
      wide ? 24 : 16,
      wide ? 24 : (about ? 12 : 24),
    );
    return LayoutBuilder(
      builder: (context, viewport) {
        final detail = ConstrainedBox(
          constraints: BoxConstraints(
            maxWidth: wide ? 1200 : double.infinity,
            minHeight: about
                ? (viewport.maxHeight - padding.vertical).clamp(
                    0.0,
                    double.infinity,
                  )
                : 0,
          ),
          child: about
              ? IntrinsicHeight(child: _detail(section))
              : _detail(section),
        );
        return SingleChildScrollView(
          padding: padding,
          child: Align(alignment: Alignment.topLeft, child: detail),
        );
      },
    );
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
            padding: const EdgeInsets.only(bottom: 24),
            child: Text(
              s.label,
              style: TextStyle(
                fontFamily: U4.font,
                fontSize: 26,
                height: 1.22,
                fontWeight: FontWeight.w600,
                color: ConsolePalette.of(context).text1,
              ),
            ),
          ),
        switch (s) {
          SettingsSection.mixer => _MixerSetupDetail(store: widget.store),
          SettingsSection.connections => _ConnectionsDetail(
              store: widget.store,
              toast: _toast,
              confirm: _confirm,
            ),
          SettingsSection.mixes => _MixesDetail(
              store: widget.store,
              toast: _toast,
              confirm: _confirm,
              onOpenMixer: widget.onOpenMixer,
            ),
          SettingsSection.access => AccessLimitsDetail(
              store: widget.store.accessLimits,
            ),
          SettingsSection.data => _DataDetail(
              store: widget.store,
              toast: _toast,
              confirm: _confirm,
            ),
          SettingsSection.appearance => _AppearanceDetail(
              store: widget.store,
              toast: _toast,
            ),
          SettingsSection.about => Expanded(
              child: _AboutDetail(store: widget.store, toast: _toast),
            ),
        },
      ],
    );
  }
}
