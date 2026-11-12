import '../../design.dart';
import 'dart:math' as math;
import 'package:material_ui/material_ui.dart';

import '../../palette.dart';

/// Keep option menus attached to the control that opened them. Flutter fits
/// the menu to the available space when its trigger is near a screen edge.
Future<T?> showAnchoredMenu<T>(
  BuildContext anchor,
  List<PopupMenuEntry<T>> items,
) async {
  if (!anchor.mounted || items.isEmpty) return null;
  final box = anchor.findRenderObject() as RenderBox?;
  final overlay = Overlay.of(anchor).context.findRenderObject() as RenderBox?;
  if (box == null || overlay == null || !box.attached) return null;
  final origin = box.localToGlobal(Offset.zero, ancestor: overlay);
  return showMenu<T>(
    popUpAnimationStyle: AnimationStyle.noAnimation,
    context: anchor,
    color: ConsolePalette.of(anchor).panel,
    constraints: AppDesign.menuConstraints,
    position: RelativeRect.fromRect(
      Rect.fromLTWH(origin.dx, origin.dy + box.size.height, box.size.width, 0),
      Offset.zero & overlay.size,
    ),
    items: items,
  );
}

/// An editable popup uses the same spatial convention as option menus, with
/// enough room for forms and a live viewport that excludes the keyboard.
Future<T?> showAnchoredPopover<T>(BuildContext anchor, WidgetBuilder builder,
    {Rect? anchorBounds, double width = 340}) {
  final box = anchor.findRenderObject() as RenderBox?;
  final navigator = Navigator.of(anchor);
  final overlay = navigator.overlay?.context.findRenderObject() as RenderBox?;
  if (box == null || overlay == null || !box.attached) {
    return Future.value();
  }
  final origin = box.localToGlobal(Offset.zero, ancestor: overlay);
  return navigator.push(_AnchoredPopoverRoute<T>(
      anchor: (anchorBounds ?? Offset.zero & box.size).shift(origin),
      width: width,
      themes: InheritedTheme.capture(from: anchor, to: navigator.context),
      reduceMotion: MediaQuery.disableAnimationsOf(anchor),
      dismissLabel: MaterialLocalizations.of(anchor).modalBarrierDismissLabel,
      builder: builder));
}

class _AnchoredPopoverRoute<T> extends PopupRoute<T> {
  final Rect anchor;
  final double width;
  final CapturedThemes themes;
  final String dismissLabel;
  final WidgetBuilder builder;
  final bool reduceMotion;
  _AnchoredPopoverRoute(
      {required this.anchor,
      required this.width,
      required this.themes,
      required this.reduceMotion,
      required this.dismissLabel,
      required this.builder});
  @override
  Color? get barrierColor => null;
  @override
  bool get barrierDismissible => true;
  @override
  String get barrierLabel => dismissLabel;
  @override
  Duration get transitionDuration =>
      reduceMotion ? Duration.zero : const Duration(milliseconds: 170);
  @override
  Duration get reverseTransitionDuration =>
      reduceMotion ? Duration.zero : const Duration(milliseconds: 130);
  @override
  Widget buildPage(BuildContext context, Animation<double> animation,
      Animation<double> secondaryAnimation) {
    final media = MediaQuery.of(context);
    return themes.wrap(CustomSingleChildLayout(
        delegate: _PopoverLayout(
            anchor,
            width,
            EdgeInsets.fromLTRB(
                media.padding.left,
                media.padding.top,
                media.padding.right,
                math.max(media.padding.bottom, media.viewInsets.bottom))),
        child: _PopoverEntrance(
            animation: animation,
            anchor: anchor,
            enabled: !reduceMotion && !media.disableAnimations,
            child: Material(
                color: ConsolePalette.of(context).panel,
                elevation: 12,
                shape: RoundedRectangleBorder(
                    borderRadius: AppDesign.radius,
                    side: BorderSide(color: ConsolePalette.of(context).line)),
                clipBehavior: Clip.antiAlias,
                child: Builder(builder: builder)))));
  }
}

/// Animate only the panel's paint, keeping its final layout and anchor stable.
class _PopoverEntrance extends StatelessWidget {
  final Animation<double> animation;
  final Rect anchor;
  final bool enabled;
  final Widget child;
  const _PopoverEntrance(
      {required this.animation,
      required this.anchor,
      required this.enabled,
      required this.child});

  @override
  Widget build(BuildContext context) {
    if (!enabled) return child;
    final media = MediaQuery.of(context);
    return LayoutBuilder(builder: (context, constraints) {
      final left = anchor.left.clamp(
          media.padding.left + AppDesign.spaceSm,
          media.size.width -
              media.padding.right -
              AppDesign.spaceSm -
              constraints.maxWidth);
      final originX =
          ((anchor.center.dx - left) / constraints.maxWidth).clamp(0.0, 1.0);
      final bottom = media.size.height -
          math.max(media.padding.bottom, media.viewInsets.bottom);
      final above = anchor.top - media.padding.top;
      final below = bottom - anchor.bottom;
      final opensAbove = above >= below;
      final alignment = Alignment(originX * 2 - 1, opensAbove ? 1 : -1);
      return AnimatedBuilder(
          animation: animation,
          child: child,
          builder: (context, panel) {
            final progress = (animation.status == AnimationStatus.reverse
                    ? Curves.easeInCubic
                    : Curves.easeOutCubic)
                .transform(animation.value);
            return Opacity(
                opacity: progress,
                child: Transform.translate(
                    offset: Offset(0, (opensAbove ? 8 : -8) * (1 - progress)),
                    child: Transform.scale(
                        scale: .96 + .04 * progress,
                        alignment: alignment,
                        child: panel)));
          });
    });
  }
}

class _PopoverLayout extends SingleChildLayoutDelegate {
  final Rect anchor;
  final double width;
  final EdgeInsets insets;
  const _PopoverLayout(this.anchor, this.width, this.insets);

  Rect _available(Size size) => Rect.fromLTRB(
      insets.left + AppDesign.spaceSm,
      insets.top + AppDesign.spaceSm,
      size.width - insets.right - AppDesign.spaceSm,
      size.height - insets.bottom - AppDesign.spaceSm);

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) {
    final area = _available(constraints.biggest);
    final above =
        (math.min(anchor.top, area.bottom) - AppDesign.spaceSm - area.top)
            .clamp(0.0, area.height);
    final below =
        (area.bottom - math.max(anchor.bottom, area.top) - AppDesign.spaceSm)
            .clamp(0.0, area.height);
    final popupWidth = math.min(width, area.width);
    return BoxConstraints(
        minWidth: popupWidth,
        maxWidth: popupWidth,
        maxHeight: math.max(above, below));
  }

  @override
  Offset getPositionForChild(Size size, Size childSize) {
    final area = _available(size);
    final above = math.min(anchor.top, area.bottom) - AppDesign.spaceSm;
    final top = above - childSize.height >= area.top
        ? above - childSize.height
        : math.max(anchor.bottom, area.top) + AppDesign.spaceSm;
    return Offset(anchor.left.clamp(area.left, area.right - childSize.width),
        top.clamp(area.top, area.bottom - childSize.height));
  }

  @override
  bool shouldRelayout(_PopoverLayout oldDelegate) =>
      oldDelegate.anchor != anchor ||
      oldDelegate.width != width ||
      oldDelegate.insets != insets;
}
