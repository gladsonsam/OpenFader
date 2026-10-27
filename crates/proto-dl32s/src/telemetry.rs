//! Push-decoded input/dynamics meters and passive shared-source RTA.
use crate::{Dl32sDriver, addrs::INPUT_COUNT};
use openfader_core::{AudioSpectrum, MeterFrame, StripId};
use openfader_transport::mackie::{dl32s_meters as layout, PushEvent, VALUE_TYPE_METERS, VALUE_TYPE_MUTE_GROUP};
use layout::{DYNAMICS_OFFSET, DYNAMICS_FRAME_LEN, OUTPUT_GR_FRAME_LEN, RTA_FRAME_LEN, RTA_OFFSET as RTA_BIN_OFFSET, RTA_BIN_COUNT};
use std::{sync::atomic::Ordering, time::Duration};

/// Input-meter push shape (type-1 `Values`): list ID `start == 1` with
/// exactly our explicit list length or the legacy 60 words. The first 32
/// words are the ordered input meters as
/// big-endian IEEE-754 dB. Rig-verified 2026-10-24 (CH7 A->B flip moved only
/// word 7 by >10 dB; word 7 restored with all 106 CH7 words): structurally
/// upstream, CH7 live-pinned. Words past the first 32 ride the rest of the
/// meter layout; the extended layout appends measured processor telemetry.
// Meter start is list ID 1, not native address 1.
const METER_START: u32 = 1;

/// Staleness bound for the input-meter cache: the desk pushes at ~11 Hz, so
/// 1 s is ~11 missed frames — long enough to ride out jitter, short enough
/// that a dead stream reads unavailable (empty) instead of frozen session data.
const METER_STALE_AFTER: Duration = Duration::from_secs(1);

/// Cached pre-fader input levels (linear amplitude, one entry per input).
/// Written only by the connection ingest task, read by `poll_meters`; the
/// generation ties each sample to its connection (a new connect or a
/// disconnect clears it, so stale data never survives a session change).
#[derive(Debug, Default)]
pub(super) struct MeterState {
    pub(super) generation: u64,
    pub(super) updated_at: Option<std::time::Instant>,
    pub(super) levels: Vec<f32>,
    pub(super) outputs: Vec<openfader_core::OutputMeterFrame>,
    pub(super) outputs_at: Option<std::time::Instant>,
    pub(super) dynamics_at: Option<std::time::Instant>,
    pub(super) dynamics: Vec<openfader_core::DynamicsMeterFrame>,
}

/// Decode one meter push (type-1 `Values`) into the 32 linear input levels,
/// or `None` when the shape is not the observed one (wrong list ID/length, malformed
/// block) or any of the first 32 words is non-finite dB. Conversion is
/// `10^(db/20)` amplitude: valid 0 dB reads back 1, and an exact `0.0` word
/// is 0 dB (unity) — never silence-mapped. Only the first-32 scope keeps
/// phantom aux slots out; nothing here multiplies by fader state, so no
/// post-fader level is ever inferred.
pub(super) fn decode_input_meters(push: &PushEvent) -> Option<Vec<f32>> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_METERS => (*start, raws),
        _ => return None,
    };
    if start != METER_START
        || !(layout::is_explicit_shape(raws.len()) || raws.len() == layout::LEGACY_FRAME_LEN)
    {
        return None;
    }
    let mut out = Vec::with_capacity(INPUT_COUNT as usize);
    for raw in raws.iter().take(INPUT_COUNT as usize) {
        let db = f32::from_bits(*raw);
        if !db.is_finite() {
            return None;
        }
        out.push(10f32.powf(db / 20.0).clamp(0.0, 1.0));
    }
    Some(out)
}

// Dynamics starts after the actual 62-strip + 31-RTA prefix. Each row is
// gate IN/positive GR/OUT then compressor IN/positive GR/OUT.
pub(super) fn decode_dynamics_meters(push: &PushEvent) -> Option<Vec<openfader_core::DynamicsMeterFrame>> {
    let PushEvent::Values { value_type: VALUE_TYPE_METERS, start: METER_START, raws } = push else { return None; };
    if !matches!(raws.len(), DYNAMICS_FRAME_LEN | OUTPUT_GR_FRAME_LEN) { return None; }
    let mut frames = Vec::with_capacity(layout::INPUT_COUNT);
    for (ch, row) in raws[DYNAMICS_OFFSET..DYNAMICS_FRAME_LEN].as_chunks::<{ layout::DYNAMICS_WIDTH }>().0.iter().enumerate() {
        let v: Vec<f32> = row.iter().map(|r| f32::from_bits(*r)).collect();
        if v.iter().any(|v| !v.is_finite()) || v[1] < -0.001 || v[4] < -0.001 { return None; }
        frames.push(openfader_core::DynamicsMeterFrame {
            ch: ch as u16, gate_in: v[0], gate_reduction: v[1].max(0.0), gate_out: v[2],
            comp_in: v[3], comp_reduction: v[4].max(0.0), comp_out: v[5],
        });
    }
    Some(frames)
}

/// Shared RTA selector (type-2) address: the desk's RTA source selector.
/// The input bank uses 4000 + zero-based channel index: live desk probes
/// verified CH1, CH3, CH7 and the CH32 endpoint. Return 1–2 uses
/// 5000. Other source families remain unavailable until identified.
/// The source is passive (selected on the desk; we only follow).
pub(super) const RTA_SELECTOR_ADDR: u32 = 22;
/// Confirmed shared source: Return 1–2 (global, `strip: None`).
const RTA_SOURCE_RETURN_12: u32 = 5000;
/// Shared input-source bank, attributed to the corresponding input strip.
const RTA_SOURCE_INPUT_FIRST: u32 = 4000;
const RTA_SOURCE_INPUT_LAST: u32 = 4031;

// RTA starts after the exact DL32S strip prefix, never the legacy family
// list. The 93/285/301-word explicit lists carry these bins.

/// Transition sentinel: the desk parks all RTA bins at −999 dB while the
/// shared source settles. An all-sentinel frame is dropped; finite quiet
/// bins (real low-level signal) are valid and kept.
const RTA_SENTINEL_DB: f32 = -999.0;

/// Source-settle window: after a selector/generation change, frames are
/// withheld this long so in-flight old-source words are never
/// misattributed. Enforced in [`Dl32sDriver::poll_spectrum`] off the
/// existing ingest/poll path — no extra timer.
const RTA_SETTLE: Duration = Duration::from_millis(500);

/// Nominal ISO 1/3-octave centers backing the 31-band RTA
/// (rig-corroborated: an independent two-tone 220 Hz peaked addr 13/14 and
/// 1 kHz addr 20, and the reference manual specifies 31 bands; the other 29
/// bands were not individually excited). Positions are trusted; exact band
/// shapes were not measured.
const RTA_CENTERS_HZ: [f32; RTA_BIN_COUNT] = [
    20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0,
    400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0,
    5000.0, 6300.0, 8000.0, 10000.0, 12500.0, 16000.0, 20000.0,
];

/// Nominal 32 band edges for [`RTA_CENTERS_HZ`]: geometric midpoints
/// between adjacent centers, ends geometrically extrapolated
/// (`edge0 = c0²/mid0`, `edge31 = c30²/mid29`). Nominal — not measured on
/// the desk; carried only because [`AudioSpectrum`] requires edges.
pub(super) fn rta_band_edges_hz() -> Vec<f32> {
    let c = RTA_CENTERS_HZ;
    let mut edges = Vec::with_capacity(RTA_BIN_COUNT + 1);
    let mid0 = (c[0] * c[1]).sqrt();
    edges.push(c[0] * c[0] / mid0);
    for w in c.windows(2) {
        edges.push((w[0] * w[1]).sqrt());
    }
    let mid_last = (c[RTA_BIN_COUNT - 2] * c[RTA_BIN_COUNT - 1]).sqrt();
    edges.push(c[RTA_BIN_COUNT - 1] * c[RTA_BIN_COUNT - 1] / mid_last);
    edges
}

/// Cached passive RTA frame (shared Return 1–2 source): written only by the
/// connection ingest task, read by `poll_spectrum`. `selector` is the last
/// type-2 addr-22 value; `epoch` is monotonic across source/generation
/// changes (`generation << 32` base, +1 per source change); `switched_at`
/// gates the [`RTA_SETTLE`] window; `updated_at`/`bins_db` are the last
/// decoded 31-bin frame. Stale (>1 s, same bound as [`METER_STALE_AFTER`])
/// reads unavailable (empty) instead of frozen session data.
#[derive(Debug, Default)]
pub(super) struct SpectrumState {
    pub(super) generation: u64,
    pub(super) selector: Option<u32>,
    pub(super) epoch: u64,
    pub(super) switched_at: Option<std::time::Instant>,
    pub(super) updated_at: Option<std::time::Instant>,
    pub(super) bins_db: Vec<f32>,
}

/// Decode the explicit list's RTA suffix (62..92, BE-f32 dB), or `None`
/// when the shape is not an exact RTA/dynamics list, any bin is non-finite, or
/// all bins sit at the [`RTA_SENTINEL_DB`] transition sentinel.
pub(super) fn decode_rta_bins(push: &PushEvent) -> Option<Vec<f32>> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_METERS => (*start, raws),
        _ => return None,
    };
    if start != METER_START || !matches!(raws.len(), RTA_FRAME_LEN | DYNAMICS_FRAME_LEN | OUTPUT_GR_FRAME_LEN) {
        return None;
    }
    let mut out = Vec::with_capacity(RTA_BIN_COUNT);
    for raw in raws.iter().skip(RTA_BIN_OFFSET).take(RTA_BIN_COUNT) {
        let db = f32::from_bits(*raw);
        if !db.is_finite() {
            return None;
        }
        out.push(db);
    }
    if out.len() != RTA_BIN_COUNT {
        return None;
    }
    if out.iter().all(|v| *v == RTA_SENTINEL_DB) {
        return None;
    }
    Some(out)
}

/// Extract the shared RTA selector (type-2 addr 22) from a push that covers
/// it. Mirrors the transport type-2 cache; the ingest task feeds this so
/// source changes clear the cache/epoch without a session lock on the poll
/// path.
pub(super) fn rta_selector_from_push(push: &PushEvent) -> Option<u32> {
    let (start, raws) = match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } if *value_type == VALUE_TYPE_MUTE_GROUP => (*start, raws),
        _ => return None,
    };
    let i = RTA_SELECTOR_ADDR.checked_sub(start)? as usize;
    raws.get(i).copied()
}

impl Dl32sDriver {
    /// Non-blocking input-meter sample: the cached linear levels (one per
    /// input strip), or empty when no fresh push backs this generation —
    /// empty means unavailable, never a fabricated silence row. The engine
    /// skips both the `meters` and the `in` tap frames on empty, so a dead
    /// stream goes quiet instead of frozen. No polling here: pushes arrive
    /// on the existing broadcast feed, and the shell's 80 ms ticker stays
    /// the only meter clock.
    pub(super) fn telemetry_poll_meters(&self) -> MeterFrame {
        let current = self.generation.load(Ordering::Acquire);
        let fresh = self.meters.lock().ok().and_then(|m| {
            if m.generation != current || m.levels.len() != INPUT_COUNT as usize {
                return None;
            }
            let at = m.updated_at?;
            if at.elapsed() > METER_STALE_AFTER {
                return None;
            }
            Some(m.levels.clone())
        });
        match fresh {
            Some(levels) => MeterFrame { levels },
            None => MeterFrame { levels: Vec::new() },
        }
    }

    pub(super) fn telemetry_poll_dynamics_meters(&self) -> Option<Vec<openfader_core::DynamicsMeterFrame>> {
        let current = self.generation.load(Ordering::Acquire);
        Some(self.meters.lock().ok().filter(|m| m.generation == current &&
            m.dynamics_at.is_some_and(|at| at.elapsed() <= METER_STALE_AFTER))
            .map(|m| m.dynamics.clone()).unwrap_or_default())
    }

    /// Passive shared RTA: the desk's source selected on the desk,
    /// followed read-only — no acquisition, no restore writes. Returns the
    /// global (`strip: None`) frame for the confirmed selector 5000
    /// (`DL32S Return 1–2 (shared)` / `shared`); input selectors 4000–4031
    /// attribute to the corresponding zero-based input [`StripId`].
    /// Unknown selectors (and the unsettled/stale windows) report empty —
    /// unavailable, never fabricated. Source changes clear the cache, bump
    /// the epoch (monotonic across source/generation changes) and withhold
    /// frames for [`RTA_SETTLE`] so in-flight old-source words are never
    /// misattributed. Stale (>1 s, same bound as input meters) reads
    /// empty. Band edges are nominal (see [`rta_band_edges_hz`]). No
    /// Return-strip id or param is invented here — return processing stays
    /// a later slice.
    pub(super) fn telemetry_poll_spectrum(&self) -> Vec<AudioSpectrum> {
        let current = self.generation.load(Ordering::Acquire);
        let (selector, epoch, bins_db) = match self.spectrum.lock() {
            Ok(s) => {
                if s.generation != current {
                    return Vec::new();
                }
                match s.selector {
                    Some(v)
                        if v == RTA_SOURCE_RETURN_12
                            || (RTA_SOURCE_INPUT_FIRST..=RTA_SOURCE_INPUT_LAST).contains(&v) => {}
                    _ => return Vec::new(),
                }
                if s.switched_at.is_some_and(|t| t.elapsed() < RTA_SETTLE) {
                    return Vec::new();
                }
                let at = match s.updated_at {
                    Some(t) => t,
                    None => return Vec::new(),
                };
                if at.elapsed() > METER_STALE_AFTER {
                    return Vec::new();
                }
                if s.bins_db.len() != RTA_BIN_COUNT {
                    return Vec::new();
                }
                (s.selector, s.epoch, s.bins_db.clone())
            }
            Err(_) => return Vec::new(),
        };
        let (strip, source, position) = match selector {
            Some(v) if v == RTA_SOURCE_RETURN_12 => (
                None,
                "DL32S Return 1–2 (shared)".to_string(),
                "shared".to_string(),
            ),
            Some(v) if (RTA_SOURCE_INPUT_FIRST..=RTA_SOURCE_INPUT_LAST).contains(&v) => (
                Some(StripId((v - RTA_SOURCE_INPUT_FIRST) as u16)),
                format!("DL32S CH{} (shared)", v - RTA_SOURCE_INPUT_FIRST + 1),
                "shared".to_string(),
            ),
            _ => return Vec::new(),
        };
        vec![AudioSpectrum {
            strip,
            bins_db,
            band_edges_hz: rta_band_edges_hz(),
            source,
            position,
            epoch,
        }]
    }
}
