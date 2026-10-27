//! Delay tap-tempo targets and conversions.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapSubdiv {
    Quarter,
    Dotted8th,
}

/// Tap-tempo target for an effect-type index: 1-based `par` carrying the
/// delay time, its display range in ms, and whether the effect offers a
/// documented note-division enum (Factor/Pattern) alongside it.
///
/// Only delay-type effects whose Time row is documented (non-`inferred`)
/// in the notes are wired: DLY (10, par 2), 3TAP (11), 4TAP (12),
/// D/RV (21), D/CR (24), D/FL (25) and MODD (26) — all par 1, all
/// `1..=3000` ms. Every one of these also carries a documented
/// Factor/Pattern division enum, so `subdiv` is always true here and the
/// UI offers quarter + dotted-eighth. All other effects — including PIT
/// (60) / PIT2 (59), whose Delay rows are range-inferred — return `None`
/// and get no TAP button (no guessing).
///
/// ```
/// # use openfader_proto_xair::fx::{fx_tap_target, TapSubdiv, tap_delay_ms};
/// let t = fx_tap_target(10).unwrap();
/// assert_eq!((t.par, t.min_ms, t.max_ms), (2, 1.0, 3000.0));
/// assert!(t.subdiv);
/// assert!(fx_tap_target(0).is_none()); // HALL: reverb, no tap
/// assert!(fx_tap_target(60).is_none()); // PIT Delay row is inferred
/// assert!((tap_delay_ms(120.0, TapSubdiv::Quarter) - 500.0).abs() < 1e-9);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTapTarget {
    pub par: u8,
    pub min_ms: f64,
    pub max_ms: f64,
    pub subdiv: bool,
}

pub fn fx_tap_target(index: u8) -> Option<FxTapTarget> {
    match index {
        10 => Some(FxTapTarget {
            par: 2,
            min_ms: 1.0,
            max_ms: 3000.0,
            subdiv: true,
        }),
        11 | 12 | 21 | 24 | 25 | 26 => Some(FxTapTarget {
            par: 1,
            min_ms: 1.0,
            max_ms: 3000.0,
            subdiv: true,
        }),
        _ => None,
    }
}

/// BPM from tap intervals: needs 3+ intervals (4+ taps); `None` when too
/// few, non-finite/non-positive, or outside a sane 20..=300 BPM window.
///
/// ```
/// # use openfader_proto_xair::fx::tap_bpm_from_intervals_ms;
/// assert!((tap_bpm_from_intervals_ms(&[500.0, 500.0, 500.0]).unwrap() - 120.0).abs() < 1e-9);
/// assert!(tap_bpm_from_intervals_ms(&[500.0, 500.0]).is_none()); // only 3 taps
/// ```
pub fn tap_bpm_from_intervals_ms(intervals_ms: &[f64]) -> Option<f64> {
    if intervals_ms.len() < 3 {
        return None;
    }
    if intervals_ms.iter().any(|i| !i.is_finite() || *i <= 0.0) {
        return None;
    }
    let mean = intervals_ms.iter().sum::<f64>() / intervals_ms.len() as f64;
    if !mean.is_finite() || mean <= 0.0 {
        return None;
    }
    let bpm = 60_000.0 / mean;
    if !(20.0..=300.0).contains(&bpm) {
        return None;
    }
    Some(bpm)
}

/// Delay time in ms for a BPM: quarter = one beat (`60000/bpm`),
/// dotted-eighth = three-quarters of a beat (`45000/bpm`).
pub fn tap_delay_ms(bpm: f64, subdiv: TapSubdiv) -> f64 {
    match subdiv {
        TapSubdiv::Quarter => 60_000.0 / bpm,
        TapSubdiv::Dotted8th => 45_000.0 / bpm,
    }
}

/// Linear map of an ms delay time onto the `0..1` wire value over the
/// par's display range, clamped. The notes pin no fader law for FX time
/// pars, so this is explicitly assumed-linear (same standing as the UI's
/// assumed discrete enum steps) — documented, never asserted on hardware.
pub fn tap_ms_to_normalized(ms: f64, min_ms: f64, max_ms: f64) -> f32 {
    if !ms.is_finite() || !min_ms.is_finite() || !max_ms.is_finite() || max_ms <= min_ms {
        return 0.0;
    }
    ((ms - min_ms) / (max_ms - min_ms)).clamp(0.0, 1.0) as f32
}

/// Assert the module stays in lockstep with the core shape (4 slots × 64).
pub(crate) fn _shape_check() {
    const _: () = assert!(FX_SLOT_MIN == 1 && FX_SLOT_MAX == 4);
    const _: () = assert!(FX_PARAM_COUNT == 64);
}
