import 'package:flutter/material.dart';

import '../core/access_limits.dart';
import '../palette.dart';
import '../theme.dart';

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

  Future<void> _set(String key, bool blocked) async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.store.setAccessBlocked(key, blocked);
    } catch (error) {
      if (mounted) {
        setState(() => _error = 'Could not save access limit: $error');
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final palette = ConsolePalette.of(context);
    final options = widget.store.accessOptions;
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
            'Anyone with access to Settings can change these limits. Passcode locking is not available.',
          ),
          const SizedBox(height: 12),
          const Text(
            'Scene and show recalls, library changes, uploads and offline pushes are blocked while a limit is active.',
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
                      (!option.available &&
                          !widget.store.blockedAccess.contains(option.key))
                  ? null
                  : (value) => _set(option.key, value),
            ),
          const SizedBox(height: 20),
          Text(
            'Not covered in this release',
            style: TextStyle(color: palette.text1, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          Text(
            'Master routing, physical output patching, oscillator, presets as a separate family, shows as a separate family, quick access, mute/view groups, group assignment, Masters, Overview, Devices, Settings and I/O Patchbay. Input routing covers the input patches and source controls offered by the mixer. Mic preamps covers offered input gain and 48V controls; trim and polarity remain independent. Destination limits cover advertised mixes and groups, not physical output patches. Individual limits are unavailable for destinations the mixer does not identify. Stereo pairing uses the mixer’s reported channel pairs and current link state. Input faders and group actions can still affect limited outputs; this is not mix isolation.',
            style: TextStyle(color: palette.text7),
          ),
        ],
      ),
    );
  }
}
