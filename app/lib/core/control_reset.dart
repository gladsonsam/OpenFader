/// Reset only to a real value in the control's advertised native range.
double? numericResetValue(double min, double max, {Object? nativeDefault}) {
  if (!min.isFinite || !max.isFinite || min > max) return null;
  if (min <= 0 && max >= 0) return 0;
  if (nativeDefault is num &&
      nativeDefault.isFinite &&
      nativeDefault >= min &&
      nativeDefault <= max) {
    return nativeDefault.toDouble();
  }
  return null;
}

/// Unity is a convenience near unity, never a jump from silence or mute.
bool canResetLevelToUnity(double db,
        {required bool muted, required double min, required double max}) =>
    !muted && db.isFinite && db.abs() <= 12 && min <= 0 && max >= 0;
