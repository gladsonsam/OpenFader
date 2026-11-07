import 'package:flutter/material.dart';

import '../core/access_limits.dart';
import '../ui/console.dart';
import '../theme.dart';
import 'settings_access_passcode.dart';

/// Reads only the injected policy contract; no engine or AppStore dependency.
class AccessLimitsDetail extends StatefulWidget {
  const AccessLimitsDetail({super.key, required this.store});
  final AccessLimitsStore store;

  @override
  State<AccessLimitsDetail> createState() => _AccessLimitsDetailState();
}

class _AccessLimitsDetailState extends State<AccessLimitsDetail> {
  bool _saving = false;
  String? _error;

  Future<void> _run(Future<void> Function() action) async {
    if (_saving) return;
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await action();
    } catch (error) {
      if (mounted) {
        setState(() => _error = 'Could not save access policy: $error');
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  Future<void> _passcode(AccessPasscodeAction action) =>
      _run(() => editAccessPasscode(context, widget.store, action));

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final options = widget.store.accessOptions;
    final locked = widget.store.accessLocked;
    final blocked = widget.store.blockedAccess;
    final titleStyle = TextStyle(
        fontFamily: U4.font,
        fontSize: 18,
        fontWeight: FontWeight.w600,
        color: palette.text1);
    final detailStyle = TextStyle(
        fontFamily: U4.font, fontSize: 14, color: palette.text7, height: 1.4);
    return DefaultTextStyle(
      style: TextStyle(fontFamily: U4.font, color: palette.text1, fontSize: 16),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
        Text('Control what can be edited on this device.', style: detailStyle),
        const SizedBox(height: 24),
        Container(
          padding: const EdgeInsets.all(16),
          decoration: BoxDecoration(
              color: palette.panel,
              border: Border.all(color: palette.line),
              borderRadius: BorderRadius.circular(10)),
          child:
              Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            Row(children: [
              Icon(locked ? Icons.lock_outline : Icons.lock_open_outlined,
                  size: 20, color: palette.text7),
              const SizedBox(width: 10),
              Expanded(
                  child: Text(locked ? 'Limits locked' : 'Limits unlocked',
                      style: titleStyle)),
              OutlinedButton(
                onPressed: _saving
                    ? null
                    : () {
                        if (locked) {
                          _passcode(AccessPasscodeAction.unlock);
                        } else if (widget.store.accessHasPasscode) {
                          _run(widget.store.lockAccess);
                        } else {
                          _passcode(AccessPasscodeAction.set);
                        }
                      },
                child: Text(locked ? 'Unlock' : 'Lock'),
              ),
            ]),
            const SizedBox(height: 6),
            Text('Use a four-character passcode to protect these settings.',
                style: detailStyle),
            if (!locked && widget.store.accessHasPasscode) ...[
              const SizedBox(height: 8),
              ConsoleActionGroup(children: [
                TextButton(
                    onPressed: _saving
                        ? null
                        : () => _passcode(AccessPasscodeAction.change),
                    child: const Text('Change passcode')),
                TextButton(
                    onPressed: _saving
                        ? null
                        : () => _passcode(AccessPasscodeAction.remove),
                    child: const Text('Remove passcode')),
              ]),
            ],
          ]),
        ),
        const SizedBox(height: 28),
        Wrap(
            alignment: WrapAlignment.spaceBetween,
            crossAxisAlignment: WrapCrossAlignment.center,
            spacing: 12,
            runSpacing: 8,
            children: [
              Text('Restrict controls', style: titleStyle),
              ConsoleActionGroup(children: [
                OutlinedButton(
                    onPressed: _saving || locked
                        ? null
                        : () =>
                            _run(() => widget.store.setAllAccessBlocked(true)),
                    child: const Text('Restrict all')),
                OutlinedButton(
                    onPressed: _saving || locked
                        ? null
                        : () =>
                            _run(() => widget.store.setAllAccessBlocked(false)),
                    child: const Text('Clear all')),
              ]),
            ]),
        const SizedBox(height: 8),
        Text('Turn on a limit to block edits.', style: detailStyle),
        if (_error != null)
          Padding(
              padding: const EdgeInsets.only(top: 12),
              child: Text(_error!, style: TextStyle(color: palette.mute))),
        const SizedBox(height: 12),
        for (final option in options) ...[
          SwitchListTile.adaptive(
            contentPadding: const EdgeInsets.symmetric(vertical: 4),
            activeThumbColor: palette.mute,
            title: Text(option.label,
                style: TextStyle(
                    color: palette.text1,
                    fontFamily: U4.font,
                    fontWeight: FontWeight.w500)),
            subtitle: Text(
                '${option.coverage}${option.available ? '' : ' · Not currently available'}',
                style: detailStyle),
            value: blocked.contains(option.key),
            onChanged: _saving ||
                    locked ||
                    (!option.available && !blocked.contains(option.key))
                ? null
                : (value) => _run(
                    () => widget.store.setAccessBlocked(option.key, value)),
          ),
          Divider(height: 1, color: palette.line),
        ],
        const SizedBox(height: 20),
        ExpansionTile(
          tilePadding: EdgeInsets.zero,
          childrenPadding: const EdgeInsets.only(bottom: 16),
          title: Text('About access limits', style: titleStyle),
          children: [
            Text(
                'Limits apply only to this device and remain saved across mixers and offline sessions. The passcode protects these settings; it does not secure the mixer.\n\n'
                'Scene and show recalls, library changes, uploads and offline pushes are blocked while any limit is active. Restrict all and Clear all affect only controls available on the current mixer; other saved restrictions remain.\n\n'
                'If you forget the passcode, uninstall and reinstall the app to reset it. This can delete local mixes, presets and settings; export them first. Reset preferences and Reset all app data do not unlock it.\n\n'
                'Limits do not cover master routing beyond advertised output patches, oscillator, presets or shows as separate families, mute/view groups, group assignment, Masters, Overview, Devices, Settings or I/O Patchbay. Input routing covers advertised patches and source controls. Mic preamps cover input gain and 48V; trim and polarity are separate. Output patching covers advertised physical and USB send patches. Destination limits include advertised patch endpoints; individual limits require identified destinations. Stereo pairing follows the mixer’s reported pairs and link state. Input faders and group actions can still affect limited outputs; this is not mix isolation.',
                style: detailStyle),
          ],
        ),
      ]),
    );
  }
}
