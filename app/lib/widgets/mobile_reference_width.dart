import 'package:flutter/rendering.dart';
import 'package:flutter/widgets.dart';

/// Lays out phone artwork in its authored CSS width, then scales layout,
/// painting, semantics and pointer coordinates by the same factor. In a scroll
/// view the scaled natural height is reported, so lower controls remain reachable.
class MobileReferenceWidth extends SingleChildRenderObjectWidget {
  final double referenceWidth;
  const MobileReferenceWidth(
      {super.key, required this.referenceWidth, required super.child});
  @override
  RenderObject createRenderObject(BuildContext context) =>
      _ReferenceWidth(referenceWidth);
  @override
  void updateRenderObject(
      BuildContext context, covariant RenderObject renderObject) {
    (renderObject as _ReferenceWidth).referenceWidth = referenceWidth;
  }
}

class _ReferenceWidth extends RenderProxyBox {
  _ReferenceWidth(this._referenceWidth);
  double _referenceWidth;
  double _scale = 1;
  set referenceWidth(double value) {
    if (value == _referenceWidth) return;
    _referenceWidth = value;
    markNeedsLayout();
  }

  Matrix4 get _transform => Matrix4.diagonal3Values(_scale, _scale, 1);
  @override
  void performLayout() {
    final width = constraints.maxWidth;
    _scale = width / _referenceWidth;
    child!.layout(
        BoxConstraints(
            minWidth: _referenceWidth,
            maxWidth: _referenceWidth,
            minHeight: constraints.minHeight / _scale,
            maxHeight: constraints.maxHeight / _scale),
        parentUsesSize: true);
    size = constraints.constrain(Size(width, child!.size.height * _scale));
  }

  @override
  void paint(PaintingContext context, Offset offset) {
    context.pushTransform(needsCompositing, offset, _transform,
        (context, offset) => context.paintChild(child!, offset));
  }

  @override
  void applyPaintTransform(RenderBox child, Matrix4 transform) =>
      transform.multiply(_transform);
  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      result.addWithPaintTransform(
          transform: _transform,
          position: position,
          hitTest: (result, position) =>
              child!.hitTest(result, position: position));
}
