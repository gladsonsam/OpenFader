import '../design.dart';
import '../../core/selected_control_readout.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../controller.dart';
import '../../u4law.dart';
import '../../widgets/number_entry.dart';
import '../../ui/console.dart';

class MixerToolbar extends StatelessWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Persistent workspace home (top-left brand button).
  final VoidCallback? onHome;
  final VoidCallback? onOpenTools;
  final Widget? readoutContent;
  final String? pageTitle;
  final bool compact;
  const MixerToolbar(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome,
      this.onOpenTools,
      this.readoutContent,
      this.pageTitle,
      required this.compact});
  @override
  Widget build(BuildContext context) {
    final readout = ValueListenableBuilder<MixerReadoutFocus>(
        valueListenable: controller.readoutFocus,
        builder: (context, focus, _) {
          final channel = controller.readoutChannel;
          return Container(
              padding: EdgeInsets.symmetric(horizontal: compact ? 6 : 10),
              height: AppDesign.touchTarget,
              decoration: BoxDecoration(
                  color: ConsolePalette.of(context).panel,
                  borderRadius: MixerDesign.radius,
                  border: Border.all(color: ConsolePalette.of(context).line)),
              child: readoutContent ??
                  (pageTitle != null
                      ? Row(children: [
                          Expanded(
                              child: Align(
                                  alignment: Alignment.centerLeft,
                                  child: Text(pageTitle!,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: mixerType(
                                          context,
                                          compact
                                              ? AppDesign.compactParameterValue
                                              : AppDesign.parameterValue,
                                          weight: FontWeight.w600)))),
                          if (!compact)
                            Expanded(
                                child: Align(
                                    alignment: Alignment.centerRight,
                                    child: TextButton(
                                        style: TextButton.styleFrom(
                                            backgroundColor:
                                                Colors.transparent),
                                        onPressed: () => onNavigate(
                                            MixerDestination.connect),
                                        child: Text(
                                            controller.demo
                                                ? 'Demo desk'
                                                : controller.connected
                                                    ? controller
                                                        .connectionDisplayName
                                                    : 'Disconnected',
                                            maxLines: 1,
                                            overflow: TextOverflow.ellipsis,
                                            style: mixerType(
                                                context, compact ? 11 : 13,
                                                color:
                                                    ConsolePalette.of(context)
                                                        .mutedText))))),
                        ])
                      : Row(children: [
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
                                      mainAxisAlignment:
                                          MainAxisAlignment.center,
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
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
                                                color:
                                                    ConsolePalette.of(context)
                                                        .mutedText)),
                                        Text(
                                            _controlLabel(focus.control,
                                                sendMix: channel?.master ==
                                                        false
                                                    ? controller
                                                        .sendsOnFadersBusName
                                                    : null),
                                            maxLines: 1,
                                            overflow: TextOverflow.ellipsis,
                                            style: mixerType(
                                                context, compact ? 12 : 14,
                                                color:
                                                    ConsolePalette.of(context)
                                                        .mutedText)),
                                      ]))),
                          Expanded(
                              flex: 1,
                              child: Center(
                                  child: _SelectedLevel(
                                      controller: controller,
                                      channel: channel,
                                      control: focus.control,
                                      compact: compact))),
                          if (!compact)
                            Expanded(
                                child: controller.connected && !controller.demo
                                    ? Semantics(
                                        button: true,
                                        label:
                                            'Connected to ${controller.connectionDisplayName}',
                                        child: Tooltip(
                                            message: controller
                                                .connectionDisplayName,
                                            child: InkWell(
                                                onTap: () => onNavigate(
                                                    MixerDestination.connect),
                                                child: Align(
                                                  alignment:
                                                      Alignment.centerRight,
                                                  child: FittedBox(
                                                    fit: BoxFit.scaleDown,
                                                    child:
                                                        _MixerConnectionStatus(
                                                            controller:
                                                                controller,
                                                            compact: compact),
                                                  ),
                                                ))))
                                    : const SizedBox.shrink()),
                        ])));
        });
    return Container(
        height:
            compact ? AppDesign.toolbarCompactHeight : AppDesign.toolbarHeight,
        padding:
            EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
        decoration: BoxDecoration(
            color: ConsolePalette.of(context).background,
            border: Border(
                bottom: BorderSide(color: ConsolePalette.of(context).line))),
        child: Row(children: [
          // The brand button always returns to Mixer.
          _BrandButton(
              compact: compact,
              onHome: onHome,
              trailingGap: compact ? 6.0 : 0.0),
          Expanded(
              child: compact
                  ? readout
                  : Padding(
                      // Keep the readout aligned with the two navigation controls.
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      child: readout)),
          const SizedBox(width: AppDesign.spaceSm),
          MixerToolButton(
              label: 'Effects',
              active: pageTitle == 'FX',
              onPressed: () => onNavigate(MixerDestination.fx),
              child: Text('FX',
                  style: mixerType(context, 17,
                      color: ConsolePalette.of(context).mutedText,
                      weight: FontWeight.w600))),
          const SizedBox(width: AppDesign.spaceSm),
          MixerToolButton(
              label: 'Settings',
              active: pageTitle == 'Settings',
              onPressed: () => onNavigate(MixerDestination.settings),
              child: Icon(Icons.settings_outlined,
                  size: AppDesign.iconSize,
                  color: ConsolePalette.of(context).mutedText)),
          const SizedBox(width: AppDesign.spaceSm),
          SizedBox(
              width: compact ? 48 : null,
              height: AppDesign.touchTarget,
              child: OutlinedButton(
                  onPressed: pageTitle == 'Tools' ? onHome : onOpenTools,
                  style: OutlinedButton.styleFrom(
                      minimumSize: const Size(0, AppDesign.touchTarget),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                      padding:
                          compact ? EdgeInsets.zero : AppDesign.controlPadding),
                  child: Text(pageTitle == 'Tools' ? 'Mixer' : 'Tools',
                      style: compact ? const TextStyle(fontSize: 12) : null))),
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
        .where((c) =>
            c.channel != null &&
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
              syncing
                  ? 'Syncing $ready/${notes.length}'
                  : controller.connectionDisplayName,
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

String _controlLabel(MixerReadoutControl control, {String? sendMix}) =>
    switch (control) {
      MixerReadoutControl.volume =>
        sendMix == null ? 'Volume' : 'Send → $sendMix',
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
                  : '${fmtDb(value)} dB',
              enabled: control == MixerReadoutControl.pan || value.isFinite));

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
            : '${_targetLabel(ch)} ${_controlLabel(control, sendMix: ch.master ? null : controller.sendsOnFadersBusName)}, $text, activate to type a value',
        onTap: onEdit,
        child: GestureDetector(
            onTap: onEdit,
            behavior: HitTestBehavior.opaque,
            child: FittedBox(
                fit: BoxFit.scaleDown,
                child: Text(text,
                    maxLines: 1,
                    style: mixerType(
                        context,
                        compact
                            ? AppDesign.compactParameterValue
                            : AppDesign.parameterValue,
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
          '${_targetLabel(channel)} ${_controlLabel(control, sendMix: channel.master ? null : controller.sendsOnFadersBusName)}${pan ? ' (−100 L … +100 R)' : ''}',
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

/// Approved 44px toolbar button: charcoal fill, hairline edge, 10px radius,
/// 22px icon or FX text in #b5b5b5.
class MixerToolButton extends StatelessWidget {
  final String label;
  final Widget child;
  final VoidCallback onPressed;
  final bool active;
  const MixerToolButton(
      {super.key,
      required this.label,
      required this.child,
      required this.onPressed,
      this.active = false});
  @override
  Widget build(BuildContext context) => ConsoleIconButton(
      label: label,
      onPressed: onPressed,
      selected: active,
      filled: true,
      child: child);
}

/// Persistent workspace home: the logo always returns to Mixer.
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
          label: 'OpenFader — Mixer',
          onTap: onHome,
          child: Tooltip(
              message: 'Mixer',
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
            flex: 1,
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
                              style: mixerType(
                                  context,
                                  compact
                                      ? AppDesign.compactParameterValue
                                      : AppDesign.parameterValue,
                                  color: ConsolePalette.of(context).lime,
                                  weight: FontWeight.w700))),
                      if (data.unconfirmed)
                        Text('(unconfirmed)',
                            style: mixerType(context, 11,
                                color: ConsolePalette.of(context).mutedText)),
                    ]))))),
        if (!compact)
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
