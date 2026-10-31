part of '../settings.dart';

class _IconBtn extends StatelessWidget {
  final String tooltip;
  final FutureOr<void> Function() onTap;
  final Widget child;
  const _IconBtn({
    required this.tooltip,
    required this.onTap,
    required this.child,
  });

  @override
  Widget build(BuildContext context) => ConsoleIconButton(
      label: tooltip,
      filled: true,
      onPressed: () => _runSettingsAction(context, onTap),
      child: child);
}

class _Toggle extends StatelessWidget {
  final bool value;
  final Future<void> Function(bool) onChanged;
  final String label;
  const _Toggle({
    required this.value,
    required this.onChanged,
    required this.label,
  });

  @override
  Widget build(BuildContext context) => ConsoleToggle(
      label: label,
      value: value,
      onChanged: (value) =>
          _runSettingsAction(context, () => onChanged(value)));
}

class _Select extends StatelessWidget {
  final String value;
  final List<(String, String)> options;
  final Future<void> Function(String) onChanged;
  final String label;
  const _Select({
    required this.value,
    required this.options,
    required this.onChanged,
    required this.label,
  });

  @override
  Widget build(BuildContext context) => ConsoleSelect<String>(
      label: label,
      value: value,
      options: options,
      maxWidth:
          MediaQuery.sizeOf(context).width > AppDesign.settingsSidebarBreakpoint
              ? AppDesign.selectMaxWidth
              : AppDesign.compactSelectMaxWidth,
      onChanged: (value) =>
          _runSettingsAction(context, () => onChanged(value)));
}

class _IconBtnInline extends StatelessWidget {
  final String tooltip;
  final FutureOr<void> Function() onTap;
  final IconData icon;
  const _IconBtnInline({
    required this.tooltip,
    required this.onTap,
    this.icon = Icons.close,
  });

  @override
  Widget build(BuildContext context) => ConsoleIconButton(
      label: tooltip,
      onPressed: () => _runSettingsAction(context, onTap),
      child: Icon(icon,
          size: icon == Icons.close
              ? AppDesign.removeIconSize
              : AppDesign.iconSize,
          color: ConsolePalette.of(context).text7));
}

class _PrimaryButton extends StatelessWidget {
  final String label;
  final FutureOr<void> Function() onTap;
  final IconData? icon;
  const _PrimaryButton({required this.label, required this.onTap, this.icon});

  @override
  Widget build(BuildContext context) => ConsoleButton(
      label: label,
      icon: icon,
      primary: true,
      fontSize: AppDesign.rowLabel,
      onPressed: () => _runSettingsAction(context, onTap));
}

class _PlainButton extends StatelessWidget {
  final String label;
  final FutureOr<void> Function() onTap;
  final IconData? icon;
  const _PlainButton({required this.label, required this.onTap, this.icon});

  @override
  Widget build(BuildContext context) => ConsoleButton(
      label: label,
      icon: icon,
      fontSize: AppDesign.rowLabel,
      onPressed: () => _runSettingsAction(context, onTap));
}

// Awaitable wrapper so gesture and semantics taps can call async actions.
Future<void> _runSettingsAction(
  BuildContext context,
  FutureOr<void> Function() action,
) async {
  try {
    await action();
  } on MixExportCancelled {
    return;
  } catch (error) {
    if (!context.mounted) return;
    final messenger = ScaffoldMessenger.of(context);
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      SnackBar(
        content: Text(
          'Could not complete the action: $error',
          textAlign: TextAlign.center,
          style: const TextStyle(fontFamily: U4.font, fontSize: 17),
        ),
        behavior: SnackBarBehavior.floating,
        duration: const Duration(seconds: 3),
      ),
    );
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
      colorFilter: ColorFilter.mode(color, BlendMode.srcIn),
    );
  }
}
