import 'xair_fx_display.dart';

/// Discrete, provider-defined mapping from an FX value to its display units.
abstract interface class FxPositionScale {
  double get min;
  double get max;
  int get steps;
  String get unit;
  int position(double normalized);
  double display(int position);
  double entry(double value);
  String text(int position);
}

FxPositionScale? fxPositionScale(String driver, Map? metadata) =>
    switch (driver) {
      'xair' => XairHallScale.fromMetadata(metadata),
      _ => null,
    };
