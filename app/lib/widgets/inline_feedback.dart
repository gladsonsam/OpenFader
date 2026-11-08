import 'package:flutter/material.dart';
import '../palette.dart';

/// Persistent operation feedback, placed in the surface that owns the action.
class InlineFeedbackController extends ValueNotifier<String?> {
  InlineFeedbackController() : super(null);
  void show(String message) => value = message;
  void clear() => value = null;
}

class InlineFeedbackView extends StatelessWidget {
  const InlineFeedbackView(
      {super.key, required this.controller, this.safeBottom = false});
  final InlineFeedbackController controller;
  final bool safeBottom;

  @override
  Widget build(BuildContext context) => ValueListenableBuilder<String?>(
      valueListenable: controller,
      builder: (context, message, _) {
        if (message == null) return const SizedBox.shrink();
        return Material(
            type: MaterialType.transparency,
            child: SafeArea(
                top: false,
                bottom: safeBottom,
                child: Padding(
                    padding:
                        const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                    child: LayoutBuilder(builder: (context, constraints) {
                      final text = Semantics(
                          liveRegion: true,
                          child: Text(message,
                              style: TextStyle(
                                  color: ConsolePalette.of(context).text1)));
                      final dismiss = IconButton(
                          tooltip: 'Dismiss message',
                          onPressed: controller.clear,
                          icon: const Icon(Icons.close));
                      if (constraints.maxWidth < 240) {
                        return Column(
                            mainAxisSize: MainAxisSize.min,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [text, dismiss]);
                      }
                      return Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [Expanded(child: text), dismiss]);
                    }))));
      });
}

class InlineFeedbackScope extends StatefulWidget {
  const InlineFeedbackScope(
      {super.key, required this.builder, this.expand = false, this.controller});
  final WidgetBuilder builder;
  final bool expand;
  final InlineFeedbackController? controller;

  static void show(BuildContext context, String message) {
    final state = context.findAncestorStateOfType<_InlineFeedbackScopeState>();
    assert(state != null, 'Inline feedback requires a local scope');
    state?._controller.show(message);
  }

  @override
  State<InlineFeedbackScope> createState() => _InlineFeedbackScopeState();
}

class _InlineFeedbackScopeState extends State<InlineFeedbackScope> {
  late final _owned = InlineFeedbackController();
  InlineFeedbackController get _controller => widget.controller ?? _owned;

  @override
  void dispose() {
    _owned.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (!widget.expand) {
      return Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Builder(builder: widget.builder),
            InlineFeedbackView(controller: _controller),
          ]);
    }
    return LayoutBuilder(
        builder: (context, constraints) =>
            Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Expanded(child: Builder(builder: widget.builder)),
              ConstrainedBox(
                  constraints:
                      BoxConstraints(maxHeight: constraints.maxHeight * .3),
                  child: SingleChildScrollView(
                      child: InlineFeedbackView(
                          controller: _controller, safeBottom: true))),
            ]));
  }
}
