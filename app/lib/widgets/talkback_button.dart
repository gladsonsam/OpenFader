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
  const TalkbackButton(
      {super.key, required this.store, required this.currentSession});
  @override
  State<TalkbackButton> createState() => _TalkbackButtonState();
}

class _TalkbackButtonState extends State<TalkbackButton> {
  final _focus = FocusNode(debugLabel: 'Talkback');
  int? _pointer;
  LogicalKeyboardKey? _keyboardKey;

  void _cancel() {
    _pointer = null;
    _keyboardKey = null;
    widget.store.cancelTalkback();
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
          onPointerDown: (event) {
            if (canPress &&
                _pointer == null &&
                _keyboardKey == null &&
                event.buttons == kPrimaryButton) {
              _pointer = event.pointer;
              _focus.requestFocus();
              store.pressTalkback();
            }
          },
          onPointerCancel: (event) {
            if (_pointer == event.pointer) _cancel();
          },
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            excludeFromSemantics: true,
            // Raw pointer events establish the actual 500ms boundary. The tap
            // recognizer supplies accepted release or scroll/arena cancellation,
            // without its delayed onTapDown shifting the Auto hold duration.
            // A cancelled arena can never commit a short tap as latch. Keep
            // callbacks installed while busy so rebuilds cannot swallow release.
            onTapUp: (_) {
              if (_pointer != null) {
                _pointer = null;
                store.releaseTalkback();
              }
            },
            onTapCancel: _cancel,
            child: Container(
              constraints: const BoxConstraints(minHeight: 48),
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                border: Border.all(
                    color: store.talkbackHeld ? palette.cyan : palette.text7),
                borderRadius: BorderRadius.circular(6),
              ),
              child: Text(
                'Talkback · ${control?.valueLabel ?? 'Unavailable'}'
                '${store.talkbackHeld ? ' · Held' : ''}'
                '${store.talkbackBusy ? ' · Confirming…' : ''}',
                style: TextStyle(
                    color: canPress || store.talkbackHeld
                        ? palette.text1
                        : palette.text7),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
