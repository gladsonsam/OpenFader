//! RTA/meter pipeline: FFT-ready ring consumer for UI spectrum later.
//!
//! [`RtaProcessor`] accepts [`MeterFrame`]s (e.g. forwarded from the
//! transport [`MeterHub`](openmix_transport::MeterHub)) and exposes per-channel
//! peak/RMS, a latched clip flag, and a trailing 100 ms window of history.
//!
//! No FFT dependency yet: enable the `fft` cargo feature for the
//! [`RtaProcessor::spectrum`] hook.
//! TODO(fft): wire a real spectrum (recommended: `realfft`, a safe Rust
//! wrapper over `rustfft`, fed from accumulated meter frames or a tap
//! stream) and fill in `spectrum` behind the `fft` feature.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::MeterFrame;

/// Trailing history window (100 ms ≈ 2 frames at the 50 ms meter tick).
pub const RTA_WINDOW: Duration = Duration::from_millis(100);

/// Linear level at/above which the clip flag latches.
pub const CLIP_LEVEL: f32 = 0.99;

/// Per-channel stats over the current window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelStats {
    pub peak: f32,
    pub rms: f32,
    /// Latched: set on any sample >= [`CLIP_LEVEL`], cleared by
    /// [`RtaProcessor::clear_clips`].
    pub clip: bool,
}

/// FFT-ready meter consumer. Cheap enough to run per meter tick.
pub struct RtaProcessor {
    channels: usize,
    window: Duration,
    frames: VecDeque<(Instant, Vec<f32>)>,
    clips: Vec<bool>,
}

impl RtaProcessor {
    /// Processor with the default 100 ms history window.
    pub fn new(channels: usize) -> Self {
        Self::with_window(channels, RTA_WINDOW)
    }

    pub fn with_window(channels: usize, window: Duration) -> Self {
        Self {
            channels,
            window,
            frames: VecDeque::new(),
            clips: vec![false; channels],
        }
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    /// Frames currently inside the window.
    pub fn history_len(&self) -> usize {
        self.frames.len()
    }

    /// Accept one meter frame. Wider frames grow the channel count.
    pub fn push_frame(&mut self, frame: &MeterFrame) {
        let now = Instant::now();
        if frame.levels.len() > self.channels {
            let extra = frame.levels.len() - self.channels;
            self.channels += extra;
            self.clips.extend(std::iter::repeat(false).take(extra));
        }
        for (i, level) in frame.levels.iter().enumerate() {
            if *level >= CLIP_LEVEL {
                self.clips[i] = true;
            }
        }
        self.frames.push_back((now, frame.levels.clone()));
        self.prune(now);
    }

    fn prune(&mut self, now: Instant) {
        while let Some((t, _)) = self.frames.front() {
            if now.duration_since(*t) <= self.window {
                break;
            }
            self.frames.pop_front();
        }
    }

    /// Peak/RMS over the window for channel `ch`; `None` when out of range.
    /// With no frames in the window, peak/RMS are 0 but latched clips persist.
    pub fn channel_stats(&self, ch: usize) -> Option<ChannelStats> {
        if ch >= self.channels {
            return None;
        }
        let mut peak = 0.0f32;
        let mut sum_sq = 0.0f64;
        let mut n = 0u64;
        for (_, levels) in &self.frames {
            if let Some(&v) = levels.get(ch) {
                peak = peak.max(v);
                sum_sq += (v as f64) * (v as f64);
                n += 1;
            }
        }
        let rms = if n > 0 {
            (sum_sq / n as f64).sqrt() as f32
        } else {
            0.0
        };
        Some(ChannelStats {
            peak,
            rms,
            clip: self.clips[ch],
        })
    }

    /// Clear all latched clip flags.
    pub fn clear_clips(&mut self) {
        for c in self.clips.iter_mut() {
            *c = false;
        }
    }

    /// Future spectrum hook (only with the `fft` feature).
    ///
    /// TODO(fft): compute a real per-channel magnitude spectrum from the
    /// frame history using `realfft`; currently a placeholder error.
    #[cfg(feature = "fft")]
    pub fn spectrum(&self, _ch: usize) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("fft feature: spectrum not yet implemented (see rta.rs TODO)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    /// 8 synthetic sine frames: ch0 full-scale sine, ch1 quiet DC.
    fn sine_frames() -> Vec<MeterFrame> {
        (0..8)
            .map(|k| {
                let s = (2.0 * PI * k as f32 / 8.0).sin();
                MeterFrame {
                    levels: vec![0.5 + 0.5 * s, 0.25],
                }
            })
            .collect()
    }

    #[test]
    fn sine_peak_rms_and_clip() {
        let mut rta = RtaProcessor::new(2);
        for f in sine_frames() {
            rta.push_frame(&f);
        }
        assert_eq!(rta.history_len(), 8);
        let ch0 = rta.channel_stats(0).unwrap();
        assert!((ch0.peak - 1.0).abs() < 1e-6);
        // E[(0.5+0.5s)^2] = 0.25 + 0.125 = 0.375 -> rms ~= 0.6124.
        assert!((ch0.rms - 0.6124).abs() < 0.01, "rms={}", ch0.rms);
        assert!(ch0.clip, "full-scale sine must latch clip");

        let ch1 = rta.channel_stats(1).unwrap();
        assert!((ch1.peak - 0.25).abs() < 1e-6);
        assert!((ch1.rms - 0.25).abs() < 1e-6);
        assert!(!ch1.clip);

        rta.clear_clips();
        assert!(!rta.channel_stats(0).unwrap().clip);
    }

    #[test]
    fn out_of_range_channel_is_none() {
        let rta = RtaProcessor::new(2);
        assert!(rta.channel_stats(2).is_none());
    }

    #[test]
    fn empty_window_gives_zeros() {
        let rta = RtaProcessor::new(1);
        let stats = rta.channel_stats(0).unwrap();
        assert_eq!(stats.peak, 0.0);
        assert_eq!(stats.rms, 0.0);
        assert!(!stats.clip);
    }
}
