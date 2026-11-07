// Channel toolbar: home returns to the mixer with state retained.
library;

import 'package:flutter/material.dart';

import '../theme.dart';
import '../ui/console.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_svg/flutter_svg.dart';
import '../u4law.dart';
import '../widgets/channel_name_editor.dart';
import 'channel.dart';

enum ChannelTool { fx, settings, routing, layout, scenes }

class ChannelToolbar extends StatelessWidget {
  final ChannelStore store;
  final bool compact;
  final bool embedded;
  final Widget? trailing;
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
      this.controlFocus,
      this.embedded = false,
      this.trailing});

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
      height: embedded
          ? 52
          : compact
              ? 64
              : 68,
      decoration: embedded
          ? null
          : BoxDecoration(
              color: ConsolePalette.of(context).page,
              border: Border(
                  bottom: BorderSide(
                      color: ConsolePalette.of(context).lineHeader))),
      child: LayoutBuilder(builder: (context, box) {
        final toolsWidth = compact ? 44.0 : 296.0;
        final toolsLeft = box.maxWidth - 16 - toolsWidth;
        final readoutWidth =
            compact ? box.maxWidth - 152 : (toolsLeft - 60) * .63;
        final identity = store.stripKey == 'main'
            ? 'Main LR'
            : store.stripKey.startsWith('bus/')
                ? 'Bus ${int.tryParse(store.stripKey.split('/').last) ?? store.channelNumber}'
                : 'channel ${store.channelNumber}';
        final readout = Container(
            padding: embedded
                ? EdgeInsets.zero
                : const EdgeInsets.symmetric(horizontal: 12),
            decoration: embedded
                ? null
                : BoxDecoration(
                    color: ConsolePalette.of(context).panel,
                    borderRadius: U4.rBtn,
                    border: Border.all(
                        color: ConsolePalette.of(context).linePanel)),
            child: Stack(alignment: Alignment.center, children: [
              if (trailing != null && !compact)
                Align(
                    alignment: Alignment.centerRight,
                    child: FractionallySizedBox(
                        widthFactor: .30, child: trailing)),
              Align(
                  alignment: Alignment.centerLeft,
                  child: FractionallySizedBox(
                      widthFactor: compact ? .45 : .30,
                      child: Tooltip(
                          message: 'Rename $identity',
                          child: Semantics(
                              button: true,
                              label: 'Rename $identity',
                              child: InkWell(
                                  onTap: store.captureNameTarget() == null
                                      ? null
                                      : () {
                                          final target =
                                              store.captureNameTarget();
                                          if (target != null) {
                                            showChannelNameEditor(
                                                context, store, target);
                                          }
                                        },
                                  child: SizedBox(
                                      height: AppDesign.touchTarget,
                                      child: Column(
                                          mainAxisAlignment:
                                              MainAxisAlignment.center,
                                          crossAxisAlignment:
                                              CrossAxisAlignment.start,
                                          children: [
                                            Text(
                                                store.stripKey == 'main'
                                                    ? 'Main LR'
                                                    : store.stripKey
                                                            .startsWith('bus/')
                                                        ? 'Bus ${int.tryParse(store.stripKey.split('/').last) ?? store.channelNumber} · ${store.displayName}'
                                                        : 'Ch ${store.channelNumber} · ${store.displayName}',
                                                maxLines: 1,
                                                overflow: TextOverflow.ellipsis,
                                                style: AppDesign.type(
                                                    compact ? 12 : 14,
                                                    color: ConsolePalette.of(
                                                            context)
                                                        .text7)),
                                            Text(
                                                focus == null
                                                    ? 'Fader'
                                                    : '${focus.title} · ${focus.label}',
                                                maxLines: 1,
                                                overflow: TextOverflow.ellipsis,
                                                style: AppDesign.type(
                                                    compact ? 12 : 14,
                                                    color: ConsolePalette.of(
                                                            context)
                                                        .text7)),
                                          ]))))))),
              Align(
                  alignment: compact ? Alignment.centerRight : Alignment.center,
                  child: FractionallySizedBox(
                      widthFactor: compact ? .50 : .40,
                      child: focus == null
                          ? _FaderValue(
                              key: ValueKey(
                                  (store.stripKey, store.connectionEpoch)),
                              store: store,
                              compact: compact)
                          : _FocusedValue(
                              focus: focus,
                              compact: compact,
                              isCurrent: () =>
                                  identical(controlFocus?.value, focus)))),
            ]));
        if (embedded) return readout;
        return Stack(alignment: Alignment.centerLeft, children: [
          Positioned(left: 16, child: _HomeButton(onHome: onHome)),
          Positioned(
              left: compact ? 76 : 60 + (toolsLeft - 60 - readoutWidth) / 2,
              width: readoutWidth,
              height: 52,
              child: readout),
          Positioned(
              left: toolsLeft,
              child: Row(children: [
                if (!compact) ...[
                  _ToolButton(
                      label: 'Effects',
                      onPressed: () => onTool(ChannelTool.fx),
                      child: Text('FX',
                          style: AppDesign.type(20,
                              color: ConsolePalette.of(context).text7,
                              weight: FontWeight.w600))),
                  const SizedBox(width: AppDesign.spaceSm),
                  _ToolButton(
                      label: 'Settings',
                      onPressed: () => onTool(ChannelTool.settings),
                      child: const DesignIcon(DesignSymbol.settings)),
                  const SizedBox(width: AppDesign.spaceSm),
                  _ToolButton(
                      label: 'Routing',
                      onPressed: () => onTool(ChannelTool.routing),
                      child: const DesignIcon(DesignSymbol.routing)),
                  const SizedBox(width: AppDesign.spaceSm),
                  _ToolButton(
                      label: 'Layout',
                      onPressed: () => onTool(ChannelTool.layout),
                      child: const DesignIcon(DesignSymbol.layout)),
                  const SizedBox(width: AppDesign.spaceSm),
                  SizedBox(
                      width: 88,
                      height: 44,
                      child: TextButton(
                          onPressed: () => onTool(ChannelTool.scenes),
                          style: TextButton.styleFrom(
                              backgroundColor: ConsolePalette.of(context).chip,
                              foregroundColor: ConsolePalette.of(context).text7,
                              disabledForegroundColor:
                                  ConsolePalette.of(context).text7),
                          child: Text('Shows',
                              style: AppDesign.type(18,
                                  color: ConsolePalette.of(context).text7)))),
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

class _FocusedValue extends StatefulWidget {
  final ChannelFocus focus;
  final bool compact;
  final bool Function() isCurrent;
  final bool allowNegativeInfinity;
  const _FocusedValue(
      {required this.focus,
      required this.compact,
      required this.isCurrent,
      this.allowNegativeInfinity = false});

  @override
  State<_FocusedValue> createState() => _FocusedValueState();
}

class _FocusedValueState extends State<_FocusedValue> {
  final _controller = TextEditingController();
  final _inputFocus = FocusNode();
  bool _editing = false;
  String? _error;

  @override
  void didUpdateWidget(covariant _FocusedValue oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.focus, widget.focus)) {
      _editing = false;
      _error = null;
      _inputFocus.unfocus();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _inputFocus.dispose();
    super.dispose();
  }

  void _start() {
    if (!widget.focus.enabled || !widget.isCurrent()) return;
    _controller.text = widget.focus.entryText?.call() ?? widget.focus.text();
    _controller.selection =
        TextSelection(baseOffset: 0, extentOffset: _controller.text.length);
    setState(() {
      _editing = true;
      _error = null;
    });
    _inputFocus.requestFocus();
  }

  void _submit() {
    final focus = widget.focus;
    if (!focus.enabled || !widget.isCurrent()) {
      setState(() => _editing = false);
      return;
    }
    var input = _controller.text.trim().toLowerCase().replaceAll(',', '.');
    final frequency = focus.label == 'Frequency';
    final kilo = frequency && (input.endsWith('k') || input.endsWith('khz'));
    if (frequency) input = input.replaceFirst(RegExp(r'\s*(khz|hz|k)$'), '');
    final negativeInfinity = widget.allowNegativeInfinity &&
        const ['-inf', '-infinity', '-∞'].contains(input.trim());
    final parsed = negativeInfinity
        ? double.negativeInfinity
        : double.tryParse(input.trim());
    final value = parsed == null ? null : parsed * (kilo ? 1000 : 1);
    if (value == null ||
        (!negativeInfinity &&
            (!value.isFinite || value < focus.min || value > focus.max))) {
      setState(() => _error =
          'Range ${focus.min.toStringAsFixed(1)}–${focus.max.toStringAsFixed(1)}');
      return;
    }
    focus.apply(value);
    if (mounted) {
      setState(() {
        _editing = false;
        _error = null;
      });
    }
    _inputFocus.unfocus();
  }

  @override
  Widget build(BuildContext context) {
    final live = widget.focus.valueListenable;
    if (live == null) return _value(context);
    return ListenableBuilder(
        listenable: live, builder: (context, child) => _value(context));
  }

  Widget _value(BuildContext context) {
    final focus = widget.focus;
    final style = AppDesign.type(
        widget.compact
            ? AppDesign.compactParameterValue
            : AppDesign.parameterValue,
        color: ConsolePalette.of(context).cyan,
        weight: FontWeight.w700);
    if (_editing && focus.enabled) {
      return TextField(
        controller: _controller,
        focusNode: _inputFocus,
        keyboardType:
            const TextInputType.numberWithOptions(signed: true, decimal: true),
        textInputAction: TextInputAction.done,
        textAlign: TextAlign.center,
        style: style,
        decoration: InputDecoration(
          isDense: true,
          contentPadding:
              const EdgeInsets.symmetric(horizontal: 4, vertical: 8),
          suffixText: focus.label == 'Frequency' ? 'Hz' : null,
          errorText: _error,
        ),
        onSubmitted: (_) => _submit(),
        onTapOutside: (_) {
          setState(() {
            _editing = false;
            _error = null;
          });
          _inputFocus.unfocus();
        },
      );
    }
    return Semantics(
      button: focus.enabled,
      label: '${focus.title} ${focus.label}, ${focus.text()}, edit value',
      child: InkWell(
        onTap: focus.enabled ? _start : null,
        child: SizedBox(
            height: AppDesign.touchTarget,
            child: Center(
                child: Text(focus.text(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: style))),
      ),
    );
  }
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
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).linePanel)),
                      child: const DesignIcon(DesignSymbol.menu))))));

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

String _toolLabel(ChannelTool t) => switch (t) {
      ChannelTool.fx => 'Effects',
      ChannelTool.settings => 'Settings',
      ChannelTool.routing => 'Routing',
      ChannelTool.layout => 'Layout',
      ChannelTool.scenes => 'Shows',
    };

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
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).linePanel)),
                      child: SvgPicture.string(
                          '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10 9-7 9 7M5 9v11h5v-6h4v6h5V9"/></svg>',
                          width: 22,
                          height: 22,
                          colorFilter: ColorFilter.mode(
                              ConsolePalette.of(context).text7,
                              BlendMode.srcIn)))))));
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
                          color: ConsolePalette.of(context).chip,
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                              color: ConsolePalette.of(context).linePanel)),
                      child: child)))));
}

class _FaderValue extends StatefulWidget {
  final ChannelStore store;
  final bool compact;
  const _FaderValue({super.key, required this.store, required this.compact});

  @override
  State<_FaderValue> createState() => _FaderValueState();
}

class _FaderValueState extends State<_FaderValue> {
  late final _target = widget.store.stripKey;
  late final _epoch = widget.store.connectionEpoch;
  bool _current() =>
      widget.store.stripKey == _target &&
      widget.store.connectionEpoch == _epoch;
  late final _focus = ChannelFocus(
      title: 'Channel fader',
      label: 'Level',
      text: () => '${fmtDb(widget.store.faderNote().value)} dB',
      entryText: () => fmtDb(widget.store.faderNote().value),
      valueListenable: widget.store.faderNote(),
      min: 0,
      max: 1,
      resolveRange: () => _current() && widget.store.faderNote().value.isFinite
          ? widget.store.faderEntryRange
          : null,
      apply: (value) {
        if (_current()) widget.store.setFaderDb(value.isFinite ? value : -90);
      });

  @override
  Widget build(BuildContext context) => _FocusedValue(
      focus: _focus,
      compact: widget.compact,
      isCurrent: _current,
      allowNegativeInfinity: true);
}
