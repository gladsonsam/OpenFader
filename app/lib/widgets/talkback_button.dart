import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart' show kPrimaryButton;
import 'package:flutter/services.dart';

import '../core/quick_access.dart';
import '../palette.dart';

/// Gestures are delegated to the shell store. This widget owns no enable state,
/// timer, command future, or transport queue.
class TalkbackButton extends StatefulWidget {
  final QuickAccessStore store;
  final bool currentSession;
  const TalkbackButton({
    super.key,
    required this.store,
    required this.currentSession,
  });
  @override
  State<TalkbackButton> createState() => _TalkbackButtonState();
}

class _TalkbackButtonState extends State<TalkbackButton> {
  final _focus = FocusNode(debugLabel: 'Talkback');
  int? _pointer;
  final _hitTarget = GlobalKey();
  LogicalKeyboardKey? _keyboardKey;

  void _cancel() {
    _pointer = null;
    _keyboardKey = null;
    widget.store.cancelTalkback();
  }

  bool _inside(Offset position) {
    final box = _hitTarget.currentContext?.findRenderObject() as RenderBox?;
    return box != null &&
        box.hasSize &&
        (Offset.zero & box.size).contains(box.globalToLocal(position));
  }

  void _release() {
    if (_pointer == null) return;
    _pointer = null;
    widget.store.releaseTalkback();
  }

  @override
  void didUpdateWidget(covariant TalkbackButton oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.store != widget.store) {
      oldWidget.store.cancelTalkback();
      _cancel();
    } else if (!widget.currentSession) {
      _cancel();
    }
  }

  @override
  void dispose() {
    widget.store.cancelTalkback();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final store = widget.store;
    final control = store.talkbackGestureControl;
    final canPress = widget.currentSession && store.canPressTalkback;
    final palette = ConsolePalette.of(context);
    return Focus(
      focusNode: _focus,
      onFocusChange: (focused) {
        if (!focused) _cancel();
      },
      onKeyEvent: (_, event) {
        if (event.logicalKey != LogicalKeyboardKey.space &&
            event.logicalKey != LogicalKeyboardKey.enter) {
          return KeyEventResult.ignored;
        }
        if (event is KeyDownEvent &&
            canPress &&
            _pointer == null &&
            _keyboardKey == null) {
          _keyboardKey = event.logicalKey;
          store.pressTalkback();
        }
        if (event is KeyUpEvent && _keyboardKey == event.logicalKey) {
          _keyboardKey = null;
          store.releaseTalkback();
        }
        return KeyEventResult.handled;
      },
      child: Semantics(
        button: true,
        enabled: canPress || store.talkbackHeld,
        label: 'Talkback ${store.talkbackModePreference.name}',
        value: control?.valueLabel ?? 'Unavailable',
        onTap: canPress ? store.tapTalkback : null,
        child: Listener(
          key: _hitTarget,
          onPointerDown: (event) {
            if (canPress &&
                _pointer == null &&
                _keyboardKey == null &&
                event.buttons == kPrimaryButton) {
              _pointer = event.pointer;
              _focus.requestFocus();
              // This fixed footer has no scrollable ancestor. Momentary and
              // Auto start at actual down; only Latch waits for accepted up.
              store.pressTalkback();
            }
          },
          onPointerMove: (event) {
            if (_pointer == event.pointer && !_inside(event.position)) {
              _cancel();
            }
          },
          onPointerUp: (event) {
            if (_pointer != event.pointer) return;
            if (!_inside(event.position)) {
              _cancel();
              return;
            }
            // Only an accepted tap-up may commit Latch/short Auto. Keep the
            // pending release installed while ON is awaiting its receipt.
            scheduleMicrotask(() {
              if (mounted && _pointer == event.pointer) _cancel();
            });
          },
          onPointerCancel: (event) {
            if (_pointer == event.pointer) _cancel();
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            excludeFromSemantics: true,
            // Keep recognizers installed while busy so release/cancel intent
            // survives the synchronous store notification from raw down.
            onTapUp: (details) {
              if (_pointer == null) return;
              if (_inside(details.globalPosition)) {
                _release();
              } else {
                _cancel();
              }
            },
            onTapCancel: () {
              if (_pointer != null) _cancel();
            },

            child: Container(
              constraints: const BoxConstraints(minHeight: 48),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                border: Border.all(
                  color: store.talkbackHeld ? palette.cyan : palette.text7,
                ),
                borderRadius: BorderRadius.circular(6),
              ),
              child: Text(
                'Talkback · ${control?.valueLabel ?? 'Unavailable'}'
                '${store.talkbackHeld ? ' · Held' : ''}'
                '${store.talkbackBusy ? ' · Confirming…' : ''}',
                style: TextStyle(
                  color: canPress || store.talkbackHeld
                      ? palette.text1
                      : palette.text7,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
