// Channel toolbar: approved 44px home button (returns to Mixer, state
// retained) + shortened session readout (Ch / name / live fader dB, tap
// to type) + the four FX/Settings/Routing/Layout tools on tablet, Menu
// on phone. Mirrors the mixer toolbar geometry.
library;

import 'package:flutter/material.dart';

import '../theme.dart';
import '../u4law.dart';
import '../widgets/number_entry.dart';
import 'channel.dart';

enum ChannelTool { fx, settings, routing, layout }

class ChannelToolbar extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  final VoidCallback onHome;
  final ValueChanged<ChannelTool> onTool;
  final VoidCallback? onLock;

  const ChannelToolbar(
      {super.key,
      required this.store,
      required this.compact,
      required this.onHome,
      required this.onTool,
      this.onLock});

  @override
  Widget build(BuildContext context) {
    return Container(
        height: compact ? 64 : 68,
        padding:
            EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
        decoration: const BoxDecoration(
            color: U4.page,
            border: Border(bottom: BorderSide(color: U4.lineHeader))),
        child: Row(children: [
          _HomeButton(onHome: onHome),
          if (!compact) const SizedBox(width: 12),
          Expanded(
              child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  height: 52,
                  decoration: BoxDecoration(
                      color: U4.well,
                      borderRadius: U4.rBtn,
                      border: Border.all(color: U4.linePanel)),
                  child: Row(children: [
                    Expanded(
                        child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                  compact
                                      ? 'Ch ${store.channelNumber} · ${store.displayName}'
                                      : 'Ch ${store.channelNumber}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: TextStyle(
                                      fontFamily: U4.font,
                                      fontSize: compact ? 12 : 14,
                                      color: U4.text7)),
                              if (!compact)
                                Text(store.displayName,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: TextStyle(
                                        fontFamily: U4.font,
                                        fontSize: 14,
                                        color: U4.text7)),
                            ])),
                    Expanded(
                        child: Center(
                            child: _FaderValue(
                                store: store, compact: compact))),
                    if (!compact)
                      const Padding(
                          padding: EdgeInsets.only(left: 12),
                          child: Text('Fader',
                              style: TextStyle(
                                  fontFamily: U4.font,
                                  fontSize: 22,
                                  color: U4.text7))),
                  ]))),
          const SizedBox(width: 4),
          if (!compact) ...[
            const SizedBox(width: 4),
            _ToolButton(
                label: 'Effects',
                onPressed: () => onTool(ChannelTool.fx),
                child: const Text('FX',
                    style: TextStyle(
                        fontFamily: U4.font,
                        fontSize: 20,
                        fontWeight: FontWeight.w600,
                        color: Color(0xffb5b5b5)))),
            const SizedBox(width: 8),
            _ToolButton(
                label: 'Settings',
                onPressed: () => onTool(ChannelTool.settings),
                child: const Icon(Icons.settings_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
            const SizedBox(width: 8),
            _ToolButton(
                label: 'Routing',
                onPressed: () => onTool(ChannelTool.routing),
                child: const Icon(Icons.account_tree_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
            const SizedBox(width: 8),
            _ToolButton(
                label: 'Layout',
                onPressed: () => onTool(ChannelTool.layout),
                child: const Icon(Icons.view_column_outlined,
                    size: 22, color: Color(0xffb5b5b5))),
          ] else
            Padding(
              padding: const EdgeInsets.only(right: 10),
              child: PopupMenuButton<VoidCallback>(
                tooltip: 'Menu',
                color: const Color(0xff262626),
                onSelected: (action) => action(),
                itemBuilder: (_) => [
                  for (final t in ChannelTool.values)
                    PopupMenuItem(
                        value: () => onTool(t),
                        child: Text(_toolLabel(t),
                            style: const TextStyle(
                                fontFamily: U4.font,
                                fontSize: 16,
                                color: U4.text1))),
                  if (onLock != null)
                    PopupMenuItem(
                        value: onLock!,
                        child: Text(
                            store.locked ? 'Unlock channel' : 'Lock channel',
                            style: const TextStyle(
                                fontFamily: U4.font,
                                fontSize: 16,
                                color: U4.text1))),
                ],
                child: Container(
                  width: 44,
                  height: 44,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                      color: const Color(0xff262626),
                      borderRadius: BorderRadius.circular(10),
                      border: Border.all(color: const Color(0xff363636))),
                  child: const Icon(Icons.menu,
                      size: 22, color: Color(0xffb5b5b5)),
                ),
              ),
            ),
        ]));
  }
}

String _toolLabel(ChannelTool t) => switch (t) {
      ChannelTool.fx => 'Effects',
      ChannelTool.settings => 'Settings',
      ChannelTool.routing => 'Routing',
      ChannelTool.layout => 'Layout',
    };

/// Approved 44px home button: charcoal, hairline edge, 10px radius, 22px
/// house icon. Returns to the mixer with selection/bank/scroll retained.
class _HomeButton extends StatelessWidget {
  final VoidCallback onHome;
  const _HomeButton({required this.onHome});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: 'Return to Mixer',
      onTap: onHome,
      child: Tooltip(
          message: 'Mixer',
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onHome,
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: const Color(0xff262626),
                          borderRadius: BorderRadius.circular(10),
                          border:
                              Border.all(color: const Color(0xff363636))),
                      child: const Icon(Icons.home_outlined,
                          size: 22, color: Color(0xffb5b5b5)))))));
}

class _ToolButton extends StatelessWidget {
  final String label;
  final Widget child;
  final VoidCallback onPressed;
  const _ToolButton(
      {required this.label, required this.child, required this.onPressed});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: label,
      child: Tooltip(
          message: label,
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onPressed,
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: const Color(0xff262626),
                          borderRadius: BorderRadius.circular(10),
                          border:
                              Border.all(color: const Color(0xff363636))),
                      child: child)))));
}

/// Live fader readout with tap-to-type entry (dB, -inf allowed).
class _FaderValue extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  const _FaderValue({required this.store, required this.compact});

  @override
  Widget build(BuildContext context) {
    return ValueListenableBuilder<double>(
        valueListenable: store.faderNote(),
        builder: (context, fader, _) {
          final text = fmtDb(fader);
          return Semantics(
              button: true,
              label: 'Channel fader, $text dB, activate to type a value',
              onTap: () => _type(context),
              child: GestureDetector(
                  onTap: () => _type(context),
                  behavior: HitTestBehavior.opaque,
                  child: Text('$text dB',
                      maxLines: 1,
                      style: TextStyle(
                          fontFamily: U4.font,
                          fontSize: compact ? 18 : 22,
                          fontWeight: FontWeight.w700,
                          color: U4.cyan,
                          fontFeatures: const [
                            FontFeature.tabularFigures()
                          ]))));
        });
  }

  Future<void> _type(BuildContext context) async {
    final v = await showNumberEntry(
        context: context,
        title: 'Ch ${store.channelNumber} fader',
        initial: fmtDb(store.faderNote().value),
        min: -60,
        max: 10,
        allowNegInf: true,
        negInfValue: double.negativeInfinity);
    if (v == null) return;
    store.setFaderDb(v.isFinite ? v : -90);
  }
}
