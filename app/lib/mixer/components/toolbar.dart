import '../design.dart';
import '../../core/selected_control_readout.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../controller.dart';
import '../../u4law.dart';
import 'package:flutter/services.dart';
import '../../ui/console.dart';

class MixerToolbar extends StatelessWidget {
  final MixerController controller;
  final ValueChanged<MixerDestination> onNavigate;

  /// Back navigation through the top-left brand button.
  final VoidCallback? onHome;
  final String homeLabel;
  final VoidCallback? onOpenTools;
  final Widget? readoutContent;
  final String? pageTitle;
  final bool compact;
  const MixerToolbar(
      {super.key,
      required this.controller,
      required this.onNavigate,
      this.onHome,
      this.homeLabel = 'Mixer',
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
                          if (!compact && !controller.demo)
                            Expanded(
                                child: Align(
                                    alignment: Alignment.centerRight,
                                    child: TextButton(
                                        style: TextButton.styleFrom(
                                            alignment: Alignment.centerRight,
                                            padding: EdgeInsets.zero,
                                            backgroundColor:
                                                Colors.transparent),
                                        onPressed: () => onNavigate(
                                            MixerDestination.connect),
                                        child: Text(
                                            controller.connected
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
          // The shell supplies the next destination in its navigation chain.
          _BrandButton(
              compact: compact,
              onHome: onHome,
              label: homeLabel,
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
              width: compact ? 48 : 80,
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
        sendMix == null ? 'Volume' : 'Send to $sendMix',
      MixerReadoutControl.pan => 'Pan',
      MixerReadoutControl.gain => 'Gain',
    };

/// Only this readout follows focus changes and live fader/pan writes.
class _SelectedLevel extends StatefulWidget {
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
  State<_SelectedLevel> createState() => _SelectedLevelState();
}

class _SelectedLevelState extends State<_SelectedLevel> {
  final _text = TextEditingController();
  final _focus = FocusNode();
  bool _editing = false;
  int? _epoch;
  String? _mixTarget;
  String? _editKey;
  MixerReadoutControl? _editControl;
  String? _error;
  MixerController get controller => widget.controller;
  MixerChannel? get channel => widget.channel;
  MixerReadoutControl get control => widget.control;
  bool get compact => widget.compact;

  @override
  void didUpdateWidget(covariant _SelectedLevel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.channel?.key != channel?.key ||
        oldWidget.control != control) {
      _editing = false;
      _focus.unfocus();
    }
  }

  @override
  void dispose() {
    _text.dispose();
    _focus.dispose();
    super.dispose();
  }

  void _begin() {
    final ch = channel;
    if (ch == null) return;
    final initial = switch (control) {
      MixerReadoutControl.volume =>
        posToDb(controller.faderNoteOf(ch.channel, ch.master).value),
      MixerReadoutControl.pan => controller.panNoteOf(ch.channel!).value * 100,
      MixerReadoutControl.gain => (ch.params['gain'] as num?)?.toDouble() ?? 0,
    };
    _epoch = controller.store.connectionEpoch;
    _mixTarget = controller.store.masterProcessingTarget;
    _editKey = ch.key;
    _editControl = control;
    _text.text = initial.isFinite ? initial.toStringAsFixed(1) : '-inf';
    _text.selection =
        TextSelection(baseOffset: 0, extentOffset: _text.text.length);
    setState(() {
      _editing = true;
      _error = null;
    });
    _focus.requestFocus();
  }

  void _cancel() {
    setState(() => _editing = false);
    _focus.unfocus();
  }

  void _submit() {
    final ch = channel;
    if (ch == null ||
        ch.key != _editKey ||
        control != _editControl ||
        controller.readoutChannel?.key != _editKey ||
        controller.readoutFocus.value.control != control ||
        controller.store.connectionEpoch != _epoch ||
        controller.store.masterProcessingTarget != _mixTarget) {
      _cancel();
      return;
    }
    final ready =
        controller.store.controlReadyNote(ch.master ? -1 : ch.channel!).value;
    if ((control == MixerReadoutControl.volume && ready & 1 == 0) ||
        (control == MixerReadoutControl.pan && ready & 4 == 0)) {
      _cancel();
      return;
    }
    final raw = _text.text
        .trim()
        .toLowerCase()
        .replaceAll(',', '.')
        .replaceAll('−', '-');
    final volume = control == MixerReadoutControl.volume;
    final silence = volume && (raw == '-inf' || raw == '-∞');
    final parsed = double.tryParse(raw);
    if (!silence && (parsed == null || !parsed.isFinite)) {
      setState(() => _error = 'Enter a number');
      return;
    }
    final range = switch (control) {
      MixerReadoutControl.volume => controller.store.faderEntryRange,
      MixerReadoutControl.gain => controller.gainRangeOf(ch),
      MixerReadoutControl.pan => null,
    };
    if (control != MixerReadoutControl.pan && range == null) {
      _cancel();
      return;
    }
    final value = silence ? double.negativeInfinity : parsed!;
    final rounded = value.isFinite ? (value * 10).roundToDouble() / 10 : value;
    switch (control) {
      case MixerReadoutControl.volume:
        controller.setPosition(
            ch,
            silence
                ? 0
                : MixerFaderScale.positionForDb(
                    rounded.clamp(range!.first, range.last)));
      case MixerReadoutControl.pan:
        controller.setPan(ch, rounded.clamp(-100, 100) / 100);
      case MixerReadoutControl.gain:
        controller.setGain(ch, rounded.clamp(range!.first, range.last));
    }
    _cancel();
  }

  @override
  Widget build(BuildContext context) {
    final ch = channel;
    if (ch == null) return _value(context, '-');
    if (control == MixerReadoutControl.volume &&
        controller.store.faderEntryRange == null) {
      return _value(context, '-', enabled: false);
    }
    if (control == MixerReadoutControl.gain) {
      if (controller.gainRangeOf(ch) == null) {
        return _value(context, '-', enabled: false);
      }
      final gain = (ch.params['gain'] as num?)?.toDouble() ?? 0;
      return _value(context, '${gain.toStringAsFixed(1)} dB');
    }
    if (ch.master && control == MixerReadoutControl.volume) {
      return ValueListenableBuilder<int>(
          valueListenable: controller.store.controlReadyNote(-1),
          builder: (context, ready, _) => ready & 1 == 0
              ? _value(context, '-', enabled: false)
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
    final onEdit = ch == null || !enabled ? null : _begin;
    if (_editing && onEdit != null) {
      return ConstrainedBox(
          constraints: const BoxConstraints(
              minWidth: AppDesign.touchTarget,
              minHeight: AppDesign.touchTarget),
          child: TextField(
              controller: _text,
              focusNode: _focus,
              autofocus: true,
              textAlign: TextAlign.center,
              keyboardType: const TextInputType.numberWithOptions(
                  signed: true, decimal: true),
              textInputAction: TextInputAction.done,
              inputFormatters: [
                FilteringTextInputFormatter.allow(RegExp(r'[-+0-9.,infIN∞−]'))
              ],
              style: mixerType(
                  context,
                  compact
                      ? AppDesign.compactParameterValue
                      : AppDesign.parameterValue,
                  color: ConsolePalette.of(context).lime,
                  weight: FontWeight.w700),
              decoration: InputDecoration(
                  isDense: true,
                  contentPadding:
                      const EdgeInsets.symmetric(vertical: 8, horizontal: 4),
                  hintText: text,
                  errorText: _error),
              onChanged: (_) {
                if (_error != null) setState(() => _error = null);
              },
              onSubmitted: (_) => _submit(),
              onTapOutside: (_) => _cancel()));
    }
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
            child: ConstrainedBox(
                constraints: const BoxConstraints(
                    minWidth: AppDesign.touchTarget,
                    minHeight: AppDesign.touchTarget),
                child: Center(
                    widthFactor: 1,
                    heightFactor: 1,
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
                                weight: FontWeight.w700)))))));
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

/// Persistent logo navigation using the shell-provided destination.
class _BrandButton extends StatelessWidget {
  final bool compact;
  final VoidCallback? onHome;
  final double trailingGap;
  final String label;
  const _BrandButton(
      {required this.compact,
      required this.onHome,
      required this.trailingGap,
      required this.label});
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
          label: 'OpenFader - $label',
          onTap: onHome,
          child: Tooltip(
              message: label,
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
        if (!compact && connectionName.isNotEmpty)
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
