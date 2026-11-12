import 'package:material_ui/material_ui.dart';

import 'package:flutter/gestures.dart';

/// Graph touches belong to the graph from pointer-down, so an enclosing
/// scroll view cannot win the gesture arena halfway through a node edit.
class U4GraphTouchSurface extends StatelessWidget {
  final PointerDownEventListener onDown;
  final PointerMoveEventListener onMove;
  final PointerUpEventListener onUp;
  final PointerCancelEventListener onCancel;
  final Widget child;

  const U4GraphTouchSurface({
    super.key,
    required this.onDown,
    required this.onMove,
    required this.onUp,
    required this.onCancel,
    required this.child,
  });

  @override
  Widget build(BuildContext context) => RawGestureDetector(
        behavior: HitTestBehavior.opaque,
        gestures: {
          EagerGestureRecognizer:
              GestureRecognizerFactoryWithHandlers<EagerGestureRecognizer>(
            EagerGestureRecognizer.new,
            (_) {},
          ),
        },
        child: Listener(
          behavior: HitTestBehavior.opaque,
          onPointerDown: onDown,
          onPointerMove: onMove,
          onPointerUp: onUp,
          onPointerCancel: onCancel,
          child: child,
        ),
      );
}
