import 'package:flutter/foundation.dart' show listEquals;

import '../engine.dart' show AudioSpectrumFrame;

/// Immutable measured-dB peak hold. Demo magnitudes never enter this snapshot.
/// The source frame supplies provenance and exact band geometry, not timing.
class AudioSpectrumPeaks {
  final AudioSpectrumFrame? _geometry;
  final List<double> binsDb;

  const AudioSpectrumPeaks.empty()
      : _geometry = null,
        binsDb = const [];

  AudioSpectrumPeaks._(AudioSpectrumFrame frame, List<double> peaks)
      : _geometry = frame,
        binsDb = List.unmodifiable(peaks);

  bool matches(AudioSpectrumFrame frame) {
    final geometry = _geometry;
    return geometry != null &&
        geometry.ch == frame.ch &&
        geometry.source == frame.source &&
        geometry.position == frame.position &&
        geometry.epoch == frame.epoch &&
        listEquals(geometry.bandEdgesHz, frame.bandEdgesHz);
  }

  /// Hold each band indefinitely until a higher measurement or a store reset.
  AudioSpectrumPeaks include(AudioSpectrumFrame frame) {
    if (!matches(frame)) return AudioSpectrumPeaks._(frame, frame.binsDb);
    var changed = false;
    final peaks = List<double>.of(binsDb);
    for (var i = 0; i < peaks.length; i++) {
      if (frame.binsDb[i] > peaks[i]) {
        peaks[i] = frame.binsDb[i];
        changed = true;
      }
    }
    return changed ? AudioSpectrumPeaks._(frame, peaks) : this;
  }
}
