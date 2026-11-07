import 'dart:math' as math;

import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Reserves the actual control height before sizing the interactive graph.
/// In a scroll viewport, minHeight is the available page height. Only controls
/// plus a usable minimum graph can require scrolling, regardless of width.
class ProcessingLayout extends MultiChildRenderObjectWidget {
  final double preferredGraphHeight;

  ProcessingLayout({
    super.key,
    required this.preferredGraphHeight,
    required Widget header,
    required Widget graph,
    required Widget controls,
  }) : super(children: [header, graph, controls]);

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _ProcessingLayout(preferredGraphHeight);

  @override
  void updateRenderObject(
      BuildContext context, covariant RenderObject renderObject) {
    (renderObject as _ProcessingLayout).preferredGraphHeight =
        preferredGraphHeight;
  }
}

class _ProcessingParentData extends ContainerBoxParentData<RenderBox> {}

class _ProcessingLayout extends RenderBox
    with
        ContainerRenderObjectMixin<RenderBox,
            ContainerBoxParentData<RenderBox>>,
        RenderBoxContainerDefaultsMixin<RenderBox,
            ContainerBoxParentData<RenderBox>> {
  _ProcessingLayout(this._preferredGraphHeight);
  double _preferredGraphHeight;

  set preferredGraphHeight(double value) {
    if (value == _preferredGraphHeight) return;
    _preferredGraphHeight = value;
    markNeedsLayout();
  }

  @override
  void setupParentData(RenderBox child) {
    if (child.parentData is! ContainerBoxParentData<RenderBox>) {
      child.parentData = _ProcessingParentData();
    }
  }

  @override
  void performLayout() {
    final header = firstChild!;
    final graph = childAfter(header)!;
    final controls = childAfter(graph)!;
    final natural = BoxConstraints.tightFor(width: constraints.maxWidth);
    header.layout(natural, parentUsesSize: true);
    controls.layout(natural, parentUsesSize: true);
    final remaining =
        constraints.minHeight - header.size.height - controls.size.height;
    final graphHeight =
        math.max(160.0, math.min(_preferredGraphHeight, remaining));
    graph.layout(
        BoxConstraints.tightFor(
            width: constraints.maxWidth, height: graphHeight),
        parentUsesSize: true);
    (graph.parentData! as ContainerBoxParentData<RenderBox>).offset =
        Offset(0, header.size.height);
    (controls.parentData! as ContainerBoxParentData<RenderBox>).offset =
        Offset(0, header.size.height + graphHeight);
    size = constraints.constrain(Size(constraints.maxWidth,
        header.size.height + graphHeight + controls.size.height));
  }

  @override
  void paint(PaintingContext context, Offset offset) =>
      defaultPaint(context, offset);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      defaultHitTestChildren(result, position: position);
}
