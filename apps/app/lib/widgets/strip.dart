// Mixer strip: top→bottom MUTE → fader-area → NameBar (+ pan bar),
// matching ui/layouts/strip.default.json order (pan, mute, fader-area,
// name-wrap) with the phone-only preamp/proc sections omitted. Strip
// frame: panel bg, 3px padding/gap, 8px radius on touch builds.
import 'package:flutter/material.dart';

import '../engine.dart';
import '../theme.dart';
import '../u4law.dart';
import 'fader.dart';
import 'mute_button.dart';
import 'name_bar.dart';
import 'pan_bar.dart';

/// Touch-build control heights: the strip phone variant uses mute h48,
/// and the ui4 touch rules lift every pan bar to min 44px
/// (base JSON values are mute h40 / pan h30).
const kStripMuteH = 48.0;
const kStripPanH = 44.0;

class U4Strip extends StatelessWidget {
  final Strip strip;
  final int position;
  final double pan;
  final bool solo;
  final ValueNotifier<double> meter;
  final ValueChanged<double> onFader;
  final VoidCallback onMute;
  final ValueChanged<double> onPan;
  final VoidCallback? onSolo;

  const U4Strip({
    super.key,
    required this.strip,
    required this.position,
    required this.pan,
    this.solo = false,
    required this.meter,
    required this.onFader,
    required this.onMute,
    required this.onPan,
    this.onSolo,
  });

  @override
  Widget build(BuildContext context) {
    final tape = U4.tape[position % U4.tape.length];
    return Container(
      width: 128,
      padding: const EdgeInsets.all(3),
      decoration: const BoxDecoration(
        color: U4.panel,
        borderRadius: U4.rStrip,
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          U4PanBar(value: pan, height: kStripPanH, onChanged: onPan),
          const SizedBox(height: 3),
          U4MuteButton(
              muted: strip.muted, height: kStripMuteH, onPressed: onMute),
          if (onSolo != null) ...[
            const SizedBox(height: 3),
            U4SoloButton(solo: solo, height: 32, onPressed: onSolo),
          ],
          const SizedBox(height: 3),
          Expanded(
            child: Container(
              constraints: const BoxConstraints(minHeight: 180),
              decoration: const BoxDecoration(
                color: U4.well,
                borderRadius: U4.rWell,
              ),
              padding: const EdgeInsets.symmetric(horizontal: 4),
              child: U4Fader(
                value: strip.fader,
                dbText: fmtDb(strip.fader),
                meter: meter,
                onChanged: onFader,
              ),
            ),
          ),
          const SizedBox(height: 3),
          U4NameBar(
              name: strip.name.isEmpty ? strip.key.toUpperCase() : strip.name,
              color: tape[0],
              text: tape[1]),
        ],
      ),
    );
  }
}

/// Master strip (w112, gold cap line): the engine exposes no master
/// bus param, so fader/mute are local UI state (see main.dart).
class U4MasterStrip extends StatelessWidget {
  final double fader;
  final bool muted;
  final ValueNotifier<double> meter;
  final ValueChanged<double> onFader;
  final VoidCallback onMute;
  final String busName;
  final VoidCallback? onBusTap;

  const U4MasterStrip({
    super.key,
    required this.fader,
    required this.muted,
    required this.meter,
    required this.onFader,
    required this.onMute,
    this.busName = 'MAIN',
    this.onBusTap,
  });

  @override
  Widget build(BuildContext context) {
    final nameBar = U4NameBar(
      name: busName,
      color: U4.masterLine,
      text: const Color(0xFF241703), // --u4-amber-ink
    );
    return Container(
      width: 112,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: U4.panel,
        borderRadius: U4.rStrip,
        border: Border.all(color: U4.masterLine),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          U4MuteButton(muted: muted, height: 40, onPressed: onMute),
          const SizedBox(height: 3),
          Expanded(
            child: Container(
              constraints: const BoxConstraints(minHeight: 180),
              decoration: const BoxDecoration(
                color: U4.well,
                borderRadius: U4.rWell,
              ),
              padding: const EdgeInsets.symmetric(horizontal: 4),
              child: U4Fader(
                value: fader,
                dbText: fmtDb(fader),
                meter: meter,
                accent: FaderAccent.master,
                onChanged: onFader,
              ),
            ),
          ),
          const SizedBox(height: 3),
          if (onBusTap != null)
            GestureDetector(onTap: onBusTap, child: nameBar)
          else
            nameBar,
        ],
      ),
    );
  }
}

/// SOLO button (SOF cyan): off is a dark well with grey text, on is cyan
/// with dark ink. Same full-width touch shape as [U4MuteButton].
class U4SoloButton extends StatelessWidget {
  final bool solo;
  final double height;
  final VoidCallback? onPressed;

  const U4SoloButton({
    super.key,
    required this.solo,
    this.height = 32,
    this.onPressed,
  });

  @override
  Widget build(BuildContext context) {
    final enabled = onPressed != null;
    return GestureDetector(
      onTap: enabled ? onPressed : null,
      child: Opacity(
        opacity: enabled ? 1 : 0.45,
        child: Container(
          height: height,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: solo ? U4.cyan : U4.well,
            border: Border.all(color: solo ? U4.cyan : U4.railBorder),
            borderRadius: U4.rBtn,
          ),
          child: Text(
            'SOLO',
            textAlign: TextAlign.center,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w700,
              color: solo ? const Color(0xFF062A30) : U4.text7,
            ),
          ),
        ),
      ),
    );
  }
}
