import 'package:flutter/material.dart';

/// Short visual responses never delay the action or change its hit area.
abstract final class ConsoleMotion {
  static const press = Duration(milliseconds: 90);
  static const state = Duration(milliseconds: 110);
  static const page = Duration(milliseconds: 160);
}

class ConsolePressFeedback extends StatefulWidget {
  final Widget child;
  final bool enabled;
  const ConsolePressFeedback(
      {super.key, required this.child, this.enabled = true});

  @override
  State<ConsolePressFeedback> createState() => _ConsolePressFeedbackState();
}

class _ConsolePressFeedbackState extends State<ConsolePressFeedback> {
  final Set<int> _pointers = {};

  void _pointer(int pointer, bool down) {
    setState(() {
      if (down) {
        _pointers.add(pointer);
      } else {
        _pointers.remove(pointer);
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final reduced = MediaQuery.disableAnimationsOf(context);
    return Listener(
        onPointerDown: widget.enabled ? (e) => _pointer(e.pointer, true) : null,
        onPointerUp: (e) => _pointer(e.pointer, false),
        onPointerCancel: (e) => _pointer(e.pointer, false),
        child: TweenAnimationBuilder<double>(
            tween: Tween<double>(
                end: !reduced && widget.enabled && _pointers.isNotEmpty
                    ? .975
                    : 1),
            duration: reduced ? Duration.zero : ConsoleMotion.press,
            curve: Curves.easeOutCubic,
            builder: (context, scale, child) => Transform.scale(
                scale: scale, transformHitTests: false, child: child),
            child: widget.child));
  }
}

/// Reveals the new page immediately; outgoing controls cannot remain interactive.
/// The token changes only on navigation, never on live control updates.
class ConsolePageReveal extends StatefulWidget {
  final Object token;
  final Widget child;
  const ConsolePageReveal(
      {super.key, required this.token, required this.child});

  @override
  State<ConsolePageReveal> createState() => _ConsolePageRevealState();
}

class _ConsolePageRevealState extends State<ConsolePageReveal>
    with SingleTickerProviderStateMixin {
  late final _controller =
      AnimationController(vsync: this, duration: ConsoleMotion.page, value: 1);
  late final _curve =
      CurvedAnimation(parent: _controller, curve: Curves.easeOutCubic);

  @override
  void didUpdateWidget(ConsolePageReveal oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.token != widget.token) {
      if (MediaQuery.disableAnimationsOf(context)) {
        _controller.value = 1;
      } else {
        _controller.forward(from: 0);
      }
    }
  }

  @override
  void dispose() {
    _curve.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
      animation: _curve,
      child: widget.child,
      builder: (context, child) {
        final progress =
            MediaQuery.disableAnimationsOf(context) ? 1.0 : _curve.value;
        return ClipRect(
            child: Transform.translate(
                offset: Offset(0, 8 * (1 - progress)),
                child: Opacity(opacity: .88 + .12 * progress, child: child)));
      });
}
