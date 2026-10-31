import '../../palette.dart';
import '../../core/selected_control_readout.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../controller.dart';
import '../design.dart';
import '../../u4law.dart';
import '../../widgets/number_entry.dart';
import '../../widgets/design_icon.dart';
import 'banks.dart';

class MixerToolbar extends StatelessWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Back to the Connections start screen (top-left home button).
  final VoidCallback? onHome;
  final Widget? readoutContent;
  final bool compact, showBankControls;
  const MixerToolbar(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome,
      this.readoutContent,
      required this.compact,
      this.showBankControls = false});
  @override
  Widget build(BuildContext context) {
    final actions = <(String, MixerDestination)>[
      ('Effects', MixerDestination.fx),
      ('Settings', MixerDestination.settings),
      ('Routing', MixerDestination.routing),
      ('Meter bridge', MixerDestination.meters),
      ('Snapshots', MixerDestination.scenes),
      ('Connection', MixerDestination.connect),
      ('Debug', MixerDestination.debug),
    ];
    final readout = ValueListenableBuilder<MixerReadoutFocus>(
        valueListenable: controller.readoutFocus,
        builder: (context, focus, _) {
          final channel = controller.readoutChannel;
          return Container(
              padding: EdgeInsets.symmetric(horizontal: compact ? 6 : 10),
              height: 52,
              decoration: BoxDecoration(
                  color: ConsolePalette.of(context).panel,
                  borderRadius: MixerDesign.radius,
                  border: Border.all(color: ConsolePalette.of(context).line)),
              child: readoutContent ??
                  Row(children: [
                    // Equal constrained side lanes keep the numeric value centered
                    // in the box on both layouts. Long names stay in the left lane.
                    Expanded(
                        child: GestureDetector(
                            onTap: channel == null || channel.master
                                ? null
                                : () {
                                    controller.select(channel);
                                    onNavigate(MixerDestination.channel);
                                  },
                            behavior: HitTestBehavior.opaque,
                            child: Column(
                                mainAxisAlignment: MainAxisAlignment.center,
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text(
                                      channel == null
                                          ? controller.connected
                                              ? 'No channel selected'
                                              : 'Connecting…'
                                          : _targetLabel(channel),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: mixerType(
                                          context, compact ? 12 : 14,
                                          color: ConsolePalette.of(context)
                                              .mutedText)),
                                  Text(_controlLabel(focus.control),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: mixerType(
                                          context, compact ? 12 : 14,
                                          color: ConsolePalette.of(context)
                                              .mutedText)),
                                ]))),
                    Expanded(
                        flex: compact ? 2 : 1,
                        child: Center(
                            child: _SelectedLevel(
                                controller: controller,
                                channel: channel,
                                control: focus.control,
                                compact: compact))),
                    Expanded(
                        child: controller.connected && !controller.demo
                            ? Semantics(
                                button: true,
                                label:
                                    'Connected to ${controller.connectionDisplayName}',
                                child: Tooltip(
                                    message: controller.connectionDisplayName,
                                    child: InkWell(
                                        onTap: () => onNavigate(
                                            MixerDestination.connect),
                                        child: Align(
                                          alignment: Alignment.centerRight,
                                          child: FittedBox(
                                            fit: BoxFit.scaleDown,
                                            child: _MixerConnectionStatus(
                                                controller: controller,
                                                compact: compact),
                                          ),
                                        ))))
                            : const SizedBox.shrink()),
                  ]));
        });
    return Container(
        height: compact ? 64 : 68,
        padding:
            EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
        decoration: BoxDecoration(
            color: ConsolePalette.of(context).background,
            border: Border(
                bottom: BorderSide(color: ConsolePalette.of(context).line))),
        child: Row(children: [
          // Approved symbol returns to Connections on both layouts.
          _BrandButton(
              compact: compact,
              onHome: onHome,
              trailingGap: compact ? 6.0 : 0.0),
          Expanded(
              child: compact
                  ? readout
                  : Padding(
                      // Equal insets center the bounded box in the gap from
                      // the brand button's trailing edge to the FX button.
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      child:
                          Center(child: SizedBox(width: 892, child: readout)))),
          if (compact) const SizedBox(width: 4),
          if (!compact) ...[
            MixerToolButton(
                label: 'Effects',
                onPressed: () => onNavigate(MixerDestination.fx),
                child: Text('FX',
                    style: mixerType(context, 20,
                        color: ConsolePalette.of(context).mutedText,
                        weight: FontWeight.w600))),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Settings',
                onPressed: () => onNavigate(MixerDestination.settings),
                child: const DesignIcon(DesignSymbol.settings)),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Routing',
                onPressed: () => onNavigate(MixerDestination.routing),
                child: const DesignIcon(DesignSymbol.routing)),
            const SizedBox(width: 8),
            MixerToolButton(
                label: 'Layout',
                onPressed: () =>
                    showMixerLayout(context, controller, onNavigate),
                child: const DesignIcon(DesignSymbol.layout)),
            Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                child: SizedBox(
                    height: 44,
                    child: OutlinedButton(
                        style: OutlinedButton.styleFrom(
                            foregroundColor: ConsolePalette.of(context).lime,
                            backgroundColor: ConsolePalette.of(context).chip,
                            side: BorderSide(
                                color: ConsolePalette.of(context).lineCtl),
                            shape: const RoundedRectangleBorder(
                                borderRadius: MixerDesign.radius)),
                        onPressed: () => onNavigate(MixerDestination.scenes),
                        child: const Text('Shows')))),
          ],
          if (compact)
            Padding(
              padding: const EdgeInsets.only(right: 10),
              child: Builder(
                  builder: (menuAnchor) => _MenuButton(
                        onSelected: (action) => action(),
                        items: [
                          if (onHome != null)
                            PopupMenuItem(
                                value: onHome!,
                                child: Text('Connections',
                                    style: mixerType(context, 16))),
                          for (final a in actions)
                            PopupMenuItem(
                                value: () => onNavigate(a.$2),
                                child:
                                    Text(a.$1, style: mixerType(context, 16))),
                          if (showBankControls) ...[
                            const PopupMenuDivider(),
                            PopupMenuItem(
                                value: () =>
                                    showMixerBanks(context, controller),
                                child: Text('Channels & banks',
                                    style: mixerType(context, 16))),
                            PopupMenuItem(
                                value: () =>
                                    showMixerBuses(menuAnchor, controller),
                                child: Text('Master bus',
                                    style: mixerType(context, 16))),
                            PopupMenuItem(
                                value: () => showMixerGroupMembership(
                                    context, controller),
                                child: Text('Group members',
                                    style: mixerType(context, 16))),
                            if (controller.muteGroupCount > 0)
                              PopupMenuItem(
                                  value: () =>
                                      showMixerMuteGroups(context, controller),
                                  child: Text('Mute groups',
                                      style: mixerType(context, 16))),
                          ],
                        ],
                      )),
            ),
        ]));
  }
}

// Readiness listens directly to controls without enabling unverified values.
class _MixerConnectionStatus extends StatelessWidget {
  final MixerController controller;
  final bool compact;
  const _MixerConnectionStatus(
      {required this.controller, required this.compact});

  @override
  Widget build(BuildContext context) {
    final channels = controller.channels
        .where((c) => c.channel != null &&
            !controller.store.engine.locks.containsKey(c.channel) &&
            controller.store.accessTargetAllowed(c.key))
        .take(8)
        .toList();
    final notes = [
      for (final channel in channels)
        controller.store.controlReadyNote(channel.channel!)
    ];
    return ListenableBuilder(
      listenable: Listenable.merge(notes),
      builder: (context, _) {
        final ready = notes.where((note) => note.value & 3 == 3).length;
        final syncing = notes.isNotEmpty && ready < notes.length;
        return Text(
            syncing ? 'Syncing $ready/${notes.length}' : controller.connectionDisplayName,
            maxLines: 1,
            textAlign: TextAlign.right,
            style: mixerType(context, compact ? 11 : 13,
                color: syncing
                    ? ConsolePalette.of(context).mutedText
                    : ConsolePalette.of(context).text));
      });
  }
}

String _targetLabel(MixerChannel channel) =>
    channel.master && channel.name == 'Main' ? 'Main LR' : channel.name;

String _controlLabel(MixerReadoutControl control) => switch (control) {
      MixerReadoutControl.volume => 'Volume',
      MixerReadoutControl.pan => 'Pan',
      MixerReadoutControl.gain => 'Gain',
    };

/// Only this readout follows focus changes and live fader/pan writes.
class _SelectedLevel extends StatelessWidget {
  final MixerController controller;
  final MixerChannel? channel;
  final MixerReadoutControl control;
  final bool compact;
  const _SelectedLevel(
      {required this.controller,
      required this.channel,
      required this.control,
      required this.compact});

  @override
  Widget build(BuildContext context) {
    final ch = channel;
    if (ch == null) return _value(context, '—');
    if (control == MixerReadoutControl.gain) {
      if (controller.gainRangeOf(ch) == null) {
        return _value(context, '—', enabled: false);
      }
      final gain = (ch.params['gain'] as num?)?.toDouble() ?? 0;
      return _value(context, '${gain.toStringAsFixed(1)} dB');
    }
    if (ch.master && control == MixerReadoutControl.volume) {
      return ValueListenableBuilder<int>(
          valueListenable: controller.store.controlReadyNote(-1),
          builder: (context, ready, _) => ready & 1 == 0
              ? _value(context, '—', enabled: false)
              : _liveValue(context, ch));
    }
    return _liveValue(context, ch);
  }

  Widget _liveValue(BuildContext context, MixerChannel ch) =>
      ValueListenableBuilder<double>(
          valueListenable: control == MixerReadoutControl.pan
              ? controller.panNoteOf(ch.channel!)
              : controller.faderNoteOf(ch.channel, ch.master),
          builder: (context, value, _) => _value(
              context,
              control == MixerReadoutControl.pan
                  ? panText(value)
                  : '${fmtDb(value)} dB'));

  Widget _value(BuildContext context, String text, {bool enabled = true}) {
    final ch = channel;
    final onEdit = ch == null || !enabled
        ? null
        : () => _typeControl(context, controller, ch, control);
    return Semantics(
        button: onEdit != null,
        label: ch == null
            ? controller.connected
                ? 'No channel selected'
                : 'Connecting'
            : '${_targetLabel(ch)} ${_controlLabel(control)}, $text, activate to type a value',
        onTap: onEdit,
        child: GestureDetector(
            onTap: onEdit,
            behavior: HitTestBehavior.opaque,
            child: FittedBox(
                fit: BoxFit.scaleDown,
                child: Text(text,
                    maxLines: 1,
                    style: mixerType(context, compact ? 18 : 22,
                        color: ConsolePalette.of(context).lime,
                        weight: FontWeight.w700)))));
  }
}

/// Capture the target and control before opening entry, so acceptance writes
/// the value the dialog described even if focus changes while it is open.
Future<void> _typeControl(BuildContext context, MixerController controller,
    MixerChannel channel, MixerReadoutControl control) async {
  final double initial;
  switch (control) {
    case MixerReadoutControl.volume:
      initial = controller.faderNoteOf(channel.channel, channel.master).value;
    case MixerReadoutControl.pan:
      if (channel.master) return;
      initial = controller.panNoteOf(channel.channel!).value * 100;
    case MixerReadoutControl.gain:
      if (channel.master) return;
      initial = (channel.params['gain'] as num?)?.toDouble() ?? 0;
  }
  final volume = control == MixerReadoutControl.volume;
  final pan = control == MixerReadoutControl.pan;
  final gainRange = control == MixerReadoutControl.gain
      ? controller.gainRangeOf(channel)
      : null;
  if (control == MixerReadoutControl.gain && gainRange == null) return;
  final value = await showNumberEntry(
      context: context,
      title:
          '${_targetLabel(channel)} ${_controlLabel(control)}${pan ? ' (−100 L … +100 R)' : ''}',
      initial: volume ? fmtDb(initial) : initial.toStringAsFixed(1),
      min: volume ? -60 : (pan ? -100 : gainRange!.first),
      max: volume ? 10 : (pan ? 100 : gainRange!.last),
      allowNegInf: volume,
      negInfValue: double.negativeInfinity);
  if (value == null) return;
  if (channel.master && channel.name != controller.master.name) return;
  switch (control) {
    case MixerReadoutControl.volume:
      controller.setPosition(
          channel, value.isFinite ? MixerFaderScale.positionForDb(value) : 0);
    case MixerReadoutControl.pan:
      controller.setPan(channel, value / 100);
    case MixerReadoutControl.gain:
      controller.setGain(channel, value);
  }
}

/// Phone menu: explicit semantics button opening a menu sheet.
class _MenuButton extends StatelessWidget {
  final ValueChanged<VoidCallback> onSelected;
  final List<PopupMenuEntry<VoidCallback>> items;
  const _MenuButton({required this.onSelected, required this.items});

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
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).line)),
                      child: Icon(Icons.menu,
                          size: 22,
                          color: ConsolePalette.of(context).mutedText))))));

  Future<void> _open(BuildContext context) async {
    final box = context.findRenderObject() as RenderBox?;
    final overlay =
        Overlay.of(context).context.findRenderObject() as RenderBox?;
    if (box == null || overlay == null) return;
    final choice = await showMenu<VoidCallback>(
      popUpAnimationStyle: AnimationStyle.noAnimation,
      context: context,
      color: ConsolePalette.of(context).chip,
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

/// Approved 44px toolbar button: charcoal fill, hairline edge, 10px radius,
/// 22px icon or FX text in #b5b5b5.
class MixerToolButton extends StatelessWidget {
  final String label;
  final Widget child;
  final VoidCallback onPressed;
  const MixerToolButton(
      {super.key,
      required this.label,
      required this.child,
      required this.onPressed});
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
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).line)),
                      child: child)))));
}

/// Approved top-left logo symbol. Tapping returns to Connections.
class _BrandButton extends StatelessWidget {
  final bool compact;
  final VoidCallback? onHome;
  final double trailingGap;
  const _BrandButton(
      {required this.compact, required this.onHome, required this.trailingGap});
  @override
  Widget build(BuildContext context) {
    final logo = SizedBox(
        width: compact ? 44 : 52,
        height: compact ? 44 : 52,
        child: SvgPicture.asset('assets/brand/open-fader-logo.svg',
            semanticsLabel: 'OpenFader'));
    return Row(mainAxisSize: MainAxisSize.min, children: [
      Semantics(
          button: true,
          label: 'OpenFader — Connections',
          onTap: onHome,
          child: Tooltip(
              message: 'Connections',
              child: InkWell(
                  borderRadius: BorderRadius.circular(10),
                  onTap: onHome,
                  child: Padding(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 4, vertical: 4),
                      child: logo)))),
      SizedBox(width: trailingGap),
    ]);
  }
}

/// Selected controls use the same centered readout lanes on every mixer.
class SelectedControlReadout extends StatelessWidget {
  final SelectedControlReadoutData data;
  final bool compact;
  final String connectionName;
  final VoidCallback? onConnection;

  const SelectedControlReadout(
      {super.key,
      required this.data,
      required this.compact,
      this.connectionName = '',
      this.onConnection});

  @override
  Widget build(BuildContext context) => Row(children: [
        Expanded(
            child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
              Text(data.target,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: mixerType(context, compact ? 12 : 14,
                      color: ConsolePalette.of(context).mutedText)),
              Text(data.label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: mixerType(context, compact ? 12 : 14,
                      color: ConsolePalette.of(context).mutedText)),
            ])),
        Expanded(
            flex: compact ? 2 : 1,
            child: Semantics(
                button: data.onEdit != null,
                label: '${data.target}, ${data.label}, ${data.value}',
                child: InkWell(
                    onTap: data.onEdit,
                    child: Center(
                        child:
                            Column(mainAxisSize: MainAxisSize.min, children: [
                      FittedBox(
                          fit: BoxFit.scaleDown,
                          child: Text(data.value,
                              style: mixerType(context, compact ? 20 : 24,
                                  color: ConsolePalette.of(context).lime,
                                  weight: FontWeight.w700))),
                      if (data.unconfirmed)
                        Text('(unconfirmed)',
                            style: mixerType(context, 11,
                                color: ConsolePalette.of(context).mutedText)),
                    ]))))),
        Expanded(
            child: InkWell(
                onTap: onConnection,
                child: Align(
                    alignment: Alignment.centerRight,
                    child: FittedBox(
                        fit: BoxFit.scaleDown,
                        child: Text(connectionName,
                            maxLines: 1,
                            style: mixerType(context, compact ? 11 : 13,
                                color: ConsolePalette.of(context).text)))))),
      ]);
}
