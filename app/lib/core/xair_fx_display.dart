import 'dart:typed_data';

// X AIR Edit's de-esser display uses stored positions, not a DSP unit.
// Preserve the float32 receive arithmetic before truncating the position.
double xairDeesserPosition(double normalized) {
  final value = Float32List(1)..[0] = normalized;
  value[0] = value[0] * 50.5;
  return value[0].truncate().clamp(0, 50).toDouble();
}

// Display-space numeric entry rounds to the nearest stored position.
// Rounding occurs after conversion to the vendor's float32 input domain.
double xairDeesserEntry(double display) {
  final value = Float32List(1)..[0] = display;
  value[0] = value[0] + 0.5;
  return value[0].truncate().clamp(0, 50) / 50;
}
