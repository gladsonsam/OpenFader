// Shared console toolbar for secondary pages.
library;

import 'package:material_ui/material_ui.dart';
import '../palette.dart';

import '../theme.dart';
import '../mixer/design.dart';

// Console toolbar modes for the active tile state.
enum ShellMode { mixer, channel, scenes, fx, routing, connect, settings }

class U4ShellHeader extends StatelessWidget {
  final String title;
  final String pageTitle;
  final bool showBack;
  final String backLabel;
  final bool isChannel;
  final String channelName;
  final Color? channelColor;
  final bool locked;
  final Color? connDot;
  final String connAria;
  final ShellMode mode;
  final bool compact;
  final bool connected;
  final bool metersActive;
  final bool showMeters;
  final String? sceneNum;
  final String? sceneName;
  final String sceneAria;
  final bool scenesActive;

  final VoidCallback? onBack;
  final VoidCallback? onUnlock;
  final VoidCallback? onFx;
  final VoidCallback? onRouting;
  final VoidCallback? onConnect;
  final VoidCallback? onSettings;
  final VoidCallback? onMeters;
  final VoidCallback? onScene;
  final VoidCallback? onPrevChannel;
  final VoidCallback? onNextChannel;

  const U4ShellHeader({
    super.key,
    required this.title,
    this.pageTitle = '',
    this.showBack = false,
    this.backLabel = 'Back',
    this.isChannel = false,
    this.channelName = '',
    this.channelColor,
    this.locked = false,
    this.connDot,
    this.connAria = 'Disconnected',
    this.mode = ShellMode.mixer,
    this.compact = false,
    this.connected = false,
    this.metersActive = false,
    this.showMeters = true,
    this.sceneNum,
    this.sceneName,
    this.sceneAria = 'Open scene list',
    this.scenesActive = false,
    this.onBack,
    this.onUnlock,
    this.onFx,
    this.onRouting,
    this.onConnect,
    this.onSettings,
    this.onMeters,
    this.onScene,
    this.onPrevChannel,
    this.onNextChannel,
  });

  @override
  Widget build(BuildContext context) {
    final heading = isChannel
        ? channelName
        : pageTitle.isNotEmpty
            ? pageTitle
            : 'OpenFader';
    final actions = <(String, String, VoidCallback?, bool)>[
      ('Effects', 'FX', onFx, mode == ShellMode.fx),
      ('Routing', 'routing', onRouting, mode == ShellMode.routing),
      ('Settings', 'settings', onSettings, mode == ShellMode.settings),
      if (showMeters) ('Meters', 'meters', onMeters, metersActive),
      if (onScene != null) ('Scenes', 'snapshots', onScene, scenesActive),
    ];
    return Container(
      height: compact ? 64 : 68,
      padding: EdgeInsets.symmetric(horizontal: compact ? 6 : 10, vertical: 6),
      decoration: BoxDecoration(
          color: ConsolePalette.of(context).headerBg,
          border: Border(
              bottom:
                  BorderSide(color: ConsolePalette.of(context).lineHeader))),
      child: Row(children: [
        if (showBack) ...[
          IconButton(
              tooltip: 'Back to mixer',
              onPressed: onBack,
              icon: Icon(Icons.chevron_left,
                  color: ConsolePalette.of(context).text1)),
          const SizedBox(width: 6),
        ],
        Expanded(
            child: Container(
          height: 52,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          decoration: BoxDecoration(
              color: ConsolePalette.of(context).panel,
              borderRadius: U4.rBtn,
              border: Border.all(color: ConsolePalette.of(context).linePanel)),
          child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(heading,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: MixerDesign.type(17, weight: FontWeight.w600)),
                Text(title,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: MixerDesign.type(11,
                        color: ConsolePalette.of(context).text7)),
              ]),
        )),
        const SizedBox(width: 4),
        if (isChannel && onPrevChannel != null && onNextChannel != null) ...[
          IconButton(
              tooltip: 'Previous channel',
              onPressed: onPrevChannel,
              icon: const Icon(Icons.chevron_left)),
          IconButton(
              tooltip: 'Next channel',
              onPressed: onNextChannel,
              icon: const Icon(Icons.chevron_right)),
        ],
        if (locked)
          IconButton(
              tooltip: 'Unlock channel',
              onPressed: onUnlock,
              icon: Icon(Icons.lock_outline,
                  color: ConsolePalette.of(context).meterAmber)),
        _HeaderTool(
            label: connAria,
            artwork: 'connection',
            onTap: onConnect,
            active: mode == ShellMode.connect,
            dot: connDot ?? ConsolePalette.of(context).meterAmber),
        if (!compact)
          for (final action in actions)
            _HeaderTool(
                label: action.$1,
                artwork: action.$2,
                onTap: action.$3,
                active: action.$4),
        if (compact)
          PopupMenuButton<int>(
            popUpAnimationStyle: AnimationStyle.noAnimation,
            tooltip: 'More',
            icon:
                Icon(Icons.more_horiz, color: ConsolePalette.of(context).text7),
            onSelected: (index) => actions[index].$3?.call(),
            itemBuilder: (_) => [
              for (var i = 0; i < actions.length; i++)
                PopupMenuItem(
                    value: i,
                    enabled: actions[i].$3 != null,
                    child: Text(actions[i].$1)),
            ],
          ),
      ]),
    );
  }
}

class _HeaderTool extends StatelessWidget {
  final String label, artwork;
  final VoidCallback? onTap;
  final bool active;
  final Color? dot;
  const _HeaderTool(
      {required this.label,
      required this.artwork,
      required this.onTap,
      this.active = false,
      this.dot});
  @override
  Widget build(BuildContext context) => SizedBox(
      width: 44,
      height: 44,
      child: IconButton(
          tooltip: label,
          onPressed: onTap,
          style: IconButton.styleFrom(
              backgroundColor: active
                  ? ConsolePalette.of(context).control
                  : Colors.transparent,
              shape: const RoundedRectangleBorder(borderRadius: U4.rBtn)),
          icon: Stack(clipBehavior: Clip.none, children: [
            if (artwork == 'meters')
              Icon(Icons.graphic_eq, color: ConsolePalette.of(context).text7)
            else if (artwork == 'FX')
              Text('FX',
                  style: MixerDesign.type(17,
                      color: ConsolePalette.of(context).text7))
            else
              MixerArtwork(artwork, width: 28, height: 28),
            if (dot != null)
              Positioned(
                  right: -2,
                  bottom: -2,
                  child: Container(
                      width: 7,
                      height: 7,
                      decoration:
                          BoxDecoration(color: dot, shape: BoxShape.circle))),
          ])));
}
