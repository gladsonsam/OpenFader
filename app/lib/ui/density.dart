import 'package:material_ui/material_ui.dart';

/// Immutable touch-target density for one app tree. Resolve in build and pass
/// the value into geometry and painters.
@immutable
class ConsoleDensity extends ThemeExtension<ConsoleDensity> {
  final bool largeTargets;
  const ConsoleDensity({this.largeTargets = true});

  static ConsoleDensity of(BuildContext context) =>
      Theme.of(context).extension<ConsoleDensity>() ?? const ConsoleDensity();

  @override
  ConsoleDensity copyWith({bool? largeTargets}) =>
      ConsoleDensity(largeTargets: largeTargets ?? this.largeTargets);

  @override
  ConsoleDensity lerp(covariant ConsoleDensity? other, double t) =>
      other == null || t < .5 ? this : other;

  @override
  bool operator ==(Object other) =>
      other is ConsoleDensity && other.largeTargets == largeTargets;

  @override
  int get hashCode => largeTargets.hashCode;
}
