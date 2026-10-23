// Channel toolbar: approved 44px home button (returns to Mixer, state
// retained) + shortened session readout (Ch / name / live fader dB, tap
// to type) + the four FX/Settings/Routing/Layout tools on tablet, Menu
// on phone. Mirrors the mixer toolbar geometry.
library;

import 'package:flutter/material.dart';

import '../theme.dart';
import '../design.dart';
import '../widgets/design_icon.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../u4law.dart';
import '../widgets/number_entry.dart';
import 'channel.dart';

enum ChannelTool { fx, settings, routing, layout, scenes }

class ChannelToolbar extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  final VoidCallback onHome;
  final ValueChanged<ChannelTool> onTool;
  final VoidCallback? onLock;
  final ValueListenable<ChannelFocus?>? controlFocus;

  const ChannelToolbar(
      {super.key,
      required this.store,
      required this.compact,
      required this.onHome,
      required this.onTool,
      this.onLock,
      this.controlFocus});

  @override
  Widget build(BuildContext context) {
    if (controlFocus != null) {
      return ValueListenableBuilder<ChannelFocus?>(
          valueListenable: controlFocus!,
          builder: (context, focus, _) => _toolbar(context, focus));
    }
    return _toolbar(context, null);
  }

  Widget _toolbar(BuildContext context, ChannelFocus? focus) => Container(
      height: compact ? 64 : 68,
      decoration: const BoxDecoration(
          color: U4.page,
          border: Border(bottom: BorderSide(color: U4.lineHeader))),
      child: LayoutBuilder(builder: (context, box) {
        final toolsWidth = compact ? 44.0 : 296.0;
        final toolsLeft = box.maxWidth - 16 - toolsWidth;
        final readoutWidth =
            compact ? box.maxWidth - 152 : (toolsLeft - 60) * .63;
        return Stack(alignment: Alignment.centerLeft, children: [
          Positioned(left: 16, child: _HomeButton(onHome: onHome)),
          Positioned(
              left: compact ? 76 : 60 + (toolsLeft - 60 - readoutWidth) / 2,
              width: readoutWidth,
              height: 52,
              child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  decoration: BoxDecoration(
                      color: AppDesign.panel,
                      borderRadius: U4.rBtn,
                      border: Border.all(color: U4.linePanel)),
                  child: Stack(alignment: Alignment.center, children: [
                    Align(
                        alignment: Alignment.centerLeft,
                        child: FractionallySizedBox(
                            widthFactor: .30,
                            child: Column(
                                mainAxisAlignment: MainAxisAlignment.center,
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                      'Ch ${store.channelNumber} · ${store.displayName}',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: AppDesign.type(compact ? 12 : 14,
                                          color: U4.text7)),
                                  Text(
                                      focus == null
                                          ? 'Fader'
                                          : '${focus.title} · ${focus.label}',
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: AppDesign.type(compact ? 12 : 14,
                                          color: U4.text7)),
                                ]))),
                    // Numeric center stays at the exact midpoint of the readout.
                    Center(
                        child: FractionallySizedBox(
                            widthFactor: .40,
                            child: focus == null
                                ? _FaderValue(store: store, compact: compact)
                                : _FocusedValue(
                                    focus: focus, compact: compact))),
                  ]))),
          Positioned(
              left: toolsLeft,
              child: Row(children: [
                if (!compact) ...[
                  _ToolButton(
                      label: 'Effects',
                      onPressed: () => onTool(ChannelTool.fx),
                      child: Text('FX',
                          style: AppDesign.type(20,
                              color: U4.text7, weight: FontWeight.w600))),
                  const SizedBox(width: 8),
                  _ToolButton(
                      label: 'Settings',
                      onPressed: () => onTool(ChannelTool.settings),
                      child: const DesignIcon(DesignSymbol.settings)),
                  const SizedBox(width: 8),
                  _ToolButton(
                      label: 'Routing',
                      onPressed: () => onTool(ChannelTool.routing),
                      child: const DesignIcon(DesignSymbol.routing)),
                  const SizedBox(width: 8),
                  _ToolButton(
                      label: 'Layout',
                      onPressed: () => onTool(ChannelTool.layout),
                      child: const DesignIcon(DesignSymbol.layout)),
                  const SizedBox(width: 8),
                  SizedBox(
                      width: 88,
                      height: 44,
                      child: TextButton(
                          onPressed: () => onTool(ChannelTool.scenes),
                          style: TextButton.styleFrom(
                              backgroundColor: AppDesign.chip,
                              foregroundColor: U4.text7,
                              disabledForegroundColor: U4.text7),
                          child: Text('Shows',
                              style: AppDesign.type(18, color: U4.text7)))),
                ] else
                  _ChannelMenuButton(onSelected: (action) => action(), items: [
                    for (final t in ChannelTool.values)
                      PopupMenuItem(
                          value: () => onTool(t),
                          child:
                              Text(_toolLabel(t), style: AppDesign.type(16))),
                    if (onLock != null)
                      PopupMenuItem(
                          value: onLock!,
                          child: Text(
                              store.locked ? 'Unlock channel' : 'Lock channel',
                              style: AppDesign.type(16))),
                  ]),
              ])),
        ]);
      }));
}

class _FocusedValue extends StatelessWidget {
  final ChannelFocus focus;
  final bool compact;
  const _FocusedValue({required this.focus, required this.compact});
  @override
  Widget build(BuildContext context) => InkWell(
      onTap: () async {
        final v = await showNumberEntry(
            context: context,
            title: '${focus.title} · ${focus.label}',
            initial: focus.text(),
            min: focus.min,
            max: focus.max);
        if (v != null) focus.apply(v);
      },
      child: SizedBox(
          height: 44,
          child: Center(
              widthFactor: 1,
              child: Text(focus.text(),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: AppDesign.type(compact ? 18 : 22,
                      color: U4.cyan, weight: FontWeight.w700)))));
}

class _ChannelMenuButton extends StatelessWidget {
  final ValueChanged<VoidCallback> onSelected;
  final List<PopupMenuEntry<VoidCallback>> items;
  const _ChannelMenuButton({required this.onSelected, required this.items});

  @override
  Widget build(BuildContext context) => Semantics(
      button: true,
      label: 'Menu',
      onTap: () => _open(context),
      child: Tooltip(
          message: 'Menu',
          child: SizedBox(
              width: 44,
              height: 44,
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: () => _open(context),
                  child: Container(
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                          color: AppDesign.chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: U4.linePanel)),
                      child: const DesignIcon(DesignSymbol.menu))))));

  Future<void> _open(BuildContext context) async {
    final box = context.findRenderObject() as RenderBox?;
    final overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox?;
    if (box == null || overlay == null) return;
    final choice = await showMenu<VoidCallback>(
      context: context,
      color: AppDesign.chip,
      position: RelativeRect.fromRect(
          Rect.fromPoints(
              box.localToGlobal(Offset.zero, ancestor: overlay),
              box.localToGlobal(box.size.bottomRight(Offset.zero),
                  ancestor: overlay)),
          Offset.zero & overlay.size),
      items: items,
    );
    if (choice != null) onSelected(choice);
  }
}

String _toolLabel(ChannelTool t) => switch (t) {
      ChannelTool.fx => 'Effects',
      ChannelTool.settings => 'Settings',
      ChannelTool.routing => 'Routing',
      ChannelTool.layout => 'Layout',
      ChannelTool.scenes => 'Shows',
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
                          color: AppDesign.chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: U4.linePanel)),
                      child: SvgPicture.string(
                          '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10 9-7 9 7M5 9v11h5v-6h4v6h5V9"/></svg>',
                          width: 22,
                          height: 22,
                          colorFilter: const ColorFilter.mode(
                              U4.text7, BlendMode.srcIn)))))));
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
                          color: AppDesign.chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(color: U4.linePanel)),
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
                  child: SizedBox(
                      height: 44,
                      child: Center(
                          widthFactor: 1,
                          child: Text('$text dB',
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                  fontFamily: U4.font,
                                  fontSize: compact ? 18 : 22,
                                  fontWeight: FontWeight.w700,
                                  color: U4.cyan,
                                  fontFeatures: const [
                                    FontFeature.tabularFigures()
                                  ]))))));
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
