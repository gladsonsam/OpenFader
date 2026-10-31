import '../design.dart';
import 'package:flutter/material.dart';

import '../palette.dart';

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
