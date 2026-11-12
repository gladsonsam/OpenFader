import 'package:material_ui/material_ui.dart';

import '../../design.dart';

/// Button and choice groups keep the same gutters when they wrap to new rows.
/// Dense data grids and compound segmented controls own their own geometry.
class ConsoleActionGroup extends Wrap {
  const ConsoleActionGroup({
    super.key,
    super.direction,
    super.alignment,
    super.spacing = AppDesign.spaceSm,
    super.runAlignment,
    super.runSpacing = AppDesign.spaceSm,
    super.crossAxisAlignment = WrapCrossAlignment.center,
    super.textDirection,
    super.verticalDirection,
    super.clipBehavior,
    super.children,
  });
}
