import 'package:flutter/material.dart';

import '../core/access_limits.dart';
import '../palette.dart';
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
    return DefaultTextStyle(
      style: TextStyle(fontFamily: U4.font, color: palette.text1, fontSize: 17),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const Text(
            'Limits for this device',
            style: TextStyle(fontSize: 23, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 12),
          const Text(
            'Turn on a limit to block edits on this device. Limits are remembered across mixers and offline sessions.',
          ),
          const SizedBox(height: 12),
          const Text(
            'A four-character passcode locks changes to this policy. It does not authenticate to or secure the mixer.',
          ),
          const SizedBox(height: 12),
          const Text(
            'Scene and show recalls, library changes, uploads and offline pushes are blocked while a limit is active.',
          ),
          const SizedBox(height: 12),
          Text(locked ? 'Access policy locked' : 'Access policy unlocked'),
          Wrap(
            spacing: 12,
            runSpacing: 8,
            children: [
              if (locked)
                OutlinedButton(
                  onPressed: _saving
                      ? null
                      : () => _passcode(AccessPasscodeAction.unlock),
                  child: const Text('Unlock'),
                )
              else ...[
                OutlinedButton(
                  onPressed: _saving
                      ? null
                      : () => widget.store.accessHasPasscode
                          ? _run(widget.store.lockAccess)
                          : _passcode(AccessPasscodeAction.set),
                  child: const Text('Lock'),
                ),
                if (widget.store.accessHasPasscode) ...[
                  TextButton(
                    onPressed: _saving
                        ? null
                        : () => _passcode(AccessPasscodeAction.change),
                    child: const Text('Change passcode'),
                  ),
                  TextButton(
                    onPressed: _saving
                        ? null
                        : () => _passcode(AccessPasscodeAction.remove),
                    child: const Text('Remove passcode'),
                  ),
                ],
              ],
            ],
          ),
          const Text(
            'Forgotten passcode: uninstall and reinstall to reset this device’s app data. This can delete local mixes, presets and settings; export them first. Reset preferences and Reset all app data preserve access policy and cannot unlock it.',
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 12,
            children: [
              OutlinedButton(
                onPressed: _saving || locked
                    ? null
                    : () => _run(() => widget.store.setAllAccessBlocked(true)),
                child: const Text('All On'),
              ),
              OutlinedButton(
                onPressed: _saving || locked
                    ? null
                    : () => _run(() => widget.store.setAllAccessBlocked(false)),
                child: const Text('All Off'),
              ),
            ],
          ),
          const Text(
            'All On / All Off affect only currently advertised controls. Saved restrictions for other mixers are preserved.',
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 12),
              child: Text(_error!, style: TextStyle(color: palette.mute)),
            ),
          const SizedBox(height: 20),
          for (final option in options)
            SwitchListTile.adaptive(
              contentPadding: EdgeInsets.zero,
              activeThumbColor: palette.mute,
              title: Text(
                option.label,
                style: TextStyle(color: palette.text1, fontFamily: U4.font),
              ),
              subtitle: Text(
                '${option.coverage}${option.available ? '' : ' · Not currently advertised'}',
                style: TextStyle(color: palette.text7, fontFamily: U4.font),
              ),
              value: widget.store.blockedAccess.contains(option.key),
              onChanged: _saving ||
                      locked ||
                      (!option.available &&
                          !widget.store.blockedAccess.contains(option.key))
                  ? null
                  : (value) => _run(
                        () => widget.store.setAccessBlocked(option.key, value),
                      ),
            ),
          const SizedBox(height: 20),
          Text(
            'Not covered in this release',
            style: TextStyle(color: palette.text1, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          Text(
            'Master routing beyond advertised output patches, oscillator, presets as a separate family, shows as a separate family, quick access, mute/view groups, group assignment, Masters, Overview, Devices, Settings and I/O Patchbay. Input routing covers the input patches and source controls offered by the mixer. Mic preamps covers offered input gain and 48V controls; trim and polarity remain independent. Output patching covers advertised physical and USB send patches. Destination limits also cover advertised output patch endpoints. Individual limits are unavailable for destinations the mixer does not identify. Stereo pairing uses the mixer’s reported channel pairs and current link state. Input faders and group actions can still affect limited outputs; this is not mix isolation.',
            style: TextStyle(color: palette.text7),
          ),
        ],
      ),
    );
  }
}
