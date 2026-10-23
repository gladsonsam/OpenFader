//! RTA/meter pipeline: FFT-backed per-channel spectrum for the UI.
//!
//! [`RtaProcessor`] accepts [`MeterFrame`]s (e.g. forwarded from the
//! transport [`MeterHub`](openfader_transport::MeterHub)) and exposes per-channel
//! peak/RMS, a latched clip flag, and a trailing 100 ms window of history.
//!
//! [`RtaProcessor::spectrum`] collects the per-channel level series from the
//! frame history, removes the mean (meter levels are unipolar envelopes, so
//! without this the DC bin would always dominate), zero-pads to
//! [`SPECTRUM_FFT_LEN`], and returns [`SPECTRUM_BINS`] normalized magnitudes
//! (peak = 1.0, silence = all zeros). With the `fft` cargo feature the
//! transform runs through `realfft`; without it a naive O(N^2) DFT produces
//! identical bins (fine at N = 128, only run on UI ticks).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::MeterFrame;

/// Trailing history window (100 ms ≈ 2 frames at the 50 ms meter tick).
pub const RTA_WINDOW: Duration = Duration::from_millis(100);

/// Linear level at/above which the clip flag latches.
pub const CLIP_LEVEL: f32 = 0.99;

/// Zero-padded FFT length used by [`RtaProcessor::spectrum`].
pub const SPECTRUM_FFT_LEN: usize = 128;

/// Number of magnitude bins returned by [`RtaProcessor::spectrum`].
pub const SPECTRUM_BINS: usize = 64;

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
            self.clips.extend(std::iter::repeat_n(false, extra));
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

    /// Per-channel magnitude spectrum (length [`SPECTRUM_BINS`]).
    ///
    /// Time series is the channel's levels across the frame history
    /// (oldest → newest, truncated to the latest [`SPECTRUM_FFT_LEN`]
    /// samples), mean-removed, zero-padded to [`SPECTRUM_FFT_LEN`], then
    /// transformed; magnitudes are normalized so the peak bin is 1.0.
    /// Out-of-range channels yield an empty vec; in-range channels with no
    /// history yield all zeros. With `fft` this uses `realfft`, otherwise
    /// an equivalent naive DFT.
    pub fn spectrum(&self, ch: usize) -> Vec<f32> {
        if ch >= self.channels {
            return Vec::new();
        }
        let mut series: Vec<f32> = self
            .frames
            .iter()
            .filter_map(|(_, levels)| levels.get(ch).copied())
            .collect();
        if series.is_empty() {
            return vec![0.0; SPECTRUM_BINS];
        }
        if series.len() > SPECTRUM_FFT_LEN {
            series.drain(..series.len() - SPECTRUM_FFT_LEN);
        }
        let mean = series.iter().sum::<f32>() / series.len() as f32;
        let mut input = vec![0.0f32; SPECTRUM_FFT_LEN];
        for (i, v) in series.iter().enumerate() {
            input[i] = v - mean;
        }
        let mags = self::fft_magnitudes(&input);
        let mut bins = vec![0.0f32; SPECTRUM_BINS];
        let n = mags.len().min(SPECTRUM_BINS);
        bins[..n].copy_from_slice(&mags[..n]);
        let peak = bins.iter().fold(0.0f32, |a, &b| a.max(b));
        if peak > 0.0 {
            for b in bins.iter_mut() {
                *b /= peak;
            }
        }
        bins
    }
}

/// Real-FFT magnitudes via `realfft` (first N/2+1 bins of a length-N input).
#[cfg(feature = "fft")]
fn fft_magnitudes(input: &[f32]) -> Vec<f32> {
    use realfft::RealFftPlanner;

    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(input.len());
    let mut spectrum = fft.make_output_vec();
    let mut scratch = fft.make_scratch_vec();
    let mut buf = input.to_vec();
    fft.process_with_scratch(&mut buf, &mut spectrum, &mut scratch)
        .expect("realfft forward failed");
    spectrum.iter().map(|c| c.norm()).collect()
}

/// Naive DFT magnitudes (same layout as [`fft_magnitudes`]): the no-`fft`
/// fallback so `spectrum` behaves identically without the dependency.
#[cfg(not(feature = "fft"))]
fn fft_magnitudes(input: &[f32]) -> Vec<f32> {
    use std::f32::consts::TAU;
    let n = input.len();
    (0..=n / 2)
        .map(|k| {
            let mut re = 0.0f64;
            let mut im = 0.0f64;
            for (i, &x) in input.iter().enumerate() {
                let phase = TAU as f64 * k as f64 * i as f64 / n as f64;
                re += x as f64 * phase.cos();
                im -= x as f64 * phase.sin();
            }
            (re * re + im * im).sqrt() as f32
        })
        .collect()
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

    /// 128 frames of a 4-cycles-per-window sine on ch0 (as unipolar meter
    /// levels, i.e. with a 0.5 DC offset the spectrum must remove), DC on
    /// ch1. Zero-padded FFT length is 128, so the tone lands on bin 4.
    fn tone_frames() -> Vec<MeterFrame> {
        (0..SPECTRUM_FFT_LEN)
            .map(|k| {
                let s = (2.0 * PI * 4.0 * k as f32 / SPECTRUM_FFT_LEN as f32).sin();
                MeterFrame {
                    levels: vec![0.5 + 0.5 * s, 0.25],
                }
            })
            .collect()
    }

    #[test]
    fn spectrum_sine_peak_at_expected_bin() {
        let mut rta = RtaProcessor::new(2);
        for f in tone_frames() {
            rta.push_frame(&f);
        }
        let bins = rta.spectrum(0);
        assert_eq!(bins.len(), SPECTRUM_BINS);
        let peak = bins
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        assert_eq!(peak, 4, "tone at 4 cycles/window must peak at bin 4");
        assert!((bins[4] - 1.0).abs() < 1e-5, "peak bin normalized to 1.0");
        // DC offset must be gone: bin 0 well below the tone.
        assert!(bins[0] < 0.1, "mean removal must suppress DC, got {}", bins[0]);

        // Flat channel: silent spectrum.
        let flat = rta.spectrum(1);
        assert_eq!(flat.len(), SPECTRUM_BINS);
        assert!(flat.iter().all(|&b| b == 0.0));
    }

    #[test]
    fn spectrum_empty_and_out_of_range() {
        let rta = RtaProcessor::new(1);
        assert_eq!(rta.spectrum(0), vec![0.0; SPECTRUM_BINS]);
        assert!(rta.spectrum(1).is_empty());
        assert!(rta.spectrum(99).is_empty());
    }
}
