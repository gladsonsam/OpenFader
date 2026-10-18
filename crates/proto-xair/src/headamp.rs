//! X-Air channel preamp (`/headamp/NN/gain`, `/headamp/NN/phantom`).
//!
//! Verified against xair-api-python source (MIT, onyx-and-iris, dev branch,
//! fetched 2026-10-18):
//!
//! - `xair_api/headamp.py`: address `/headamp/{index+1:02d}` (1-based,
//!   zero-padded two-digit); `gain` getter `round(lin_get(-12, 60, wire), 1)`,
//!   setter `lin_set(-12, 60, db)`; `phantom` getter `wire == 1`, setter
//!   `1 if on else 0` (int).
//! - `xair_api/util.py`: `lin_get(min,max,v) = min+(max-min)*v`,
//!   `lin_set(min,max,val) = (val-min)/(max-min)` — the wire float `0..1`
//!   is linear in dB over `-12..=60`.
//! - `xair_api/xair.py` + `kinds.py`: one headamp per input strip
//!   (`range(kind.num_strip)`; XR18/XR16 = 16, XR12 = 12), so strip `i`
//!   (0-based) drives headamp `i + 1` — the same 1:1 mapping as `/ch/NN`.
//!
//! Owner-rig verified 2026-10-18 ~01:00 AWST (XR18 FW 1.20): gain + phantom
//! reads reply (/headamp/01/gain=0.17, /headamp/01/phantom=int 0,
//! /headamp/16/gain=0.17); same-value gain write ACKed (readback 0.1667,
//! i.e. write path + wire quantization confirmed).
//!
//! Open hardware questions (re-verify on the rig before trusting readbacks):
//! headamp gain wire quantization is uncharacterized (faders quantize to
//! ~10-bit steps, baseline T8 — headamps may quantize to 0.5 dB or similar),
//! and `/headamp/NN/phantom` has no documented readback shape beyond the
//! int the python setter writes.

/// Lowest headamp gain in dB (xair-api-python `HeadAmp.gain` lower bound).
pub const GAIN_MIN_DB: f32 = -12.0;
/// Highest headamp gain in dB (xair-api-python `HeadAmp.gain` upper bound).
pub const GAIN_MAX_DB: f32 = 60.0;

/// OSC address of one headamp gain: 1-based wire number, zero-padded
/// (`HeadAmp.address` = `/headamp/{index+1:02d}` + `/gain`).
///
/// ```
/// # use openmix_proto_xair::headamp::headamp_gain;
/// assert_eq!(headamp_gain(1), "/headamp/01/gain");
/// assert_eq!(headamp_gain(16), "/headamp/16/gain");
/// ```
pub fn headamp_gain(ch: u16) -> String {
    format!("/headamp/{ch:02}/gain")
}

/// OSC address of one headamp phantom switch (`…/phantom`, int `1`/`0`).
///
/// ```
/// # use openmix_proto_xair::headamp::headamp_phantom;
/// assert_eq!(headamp_phantom(1), "/headamp/01/phantom");
/// assert_eq!(headamp_phantom(16), "/headamp/16/phantom");
/// ```
pub fn headamp_phantom(ch: u16) -> String {
    format!("/headamp/{ch:02}/phantom")
}

/// Map a gain in dB to the normalized wire float (`lin_set(-12, 60, db)`).
/// Rejects NaN/infinite and out-of-range values before anything is sent.
pub fn db_to_wire(db: f32) -> anyhow::Result<f32> {
    if !db.is_finite() {
        anyhow::bail!("headamp gain must be finite, got {db}");
    }
    if !(GAIN_MIN_DB..=GAIN_MAX_DB).contains(&db) {
        anyhow::bail!("headamp gain {db} dB out of range {GAIN_MIN_DB}..={GAIN_MAX_DB}");
    }
    Ok((db - GAIN_MIN_DB) / (GAIN_MAX_DB - GAIN_MIN_DB))
}

/// Map a normalized wire float back to dB (`lin_get(-12, 60, v)`).
/// Rejects NaN and values outside `0..=1` — no clamping of desk replies.
pub fn wire_to_db(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("headamp gain wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("headamp gain wire value {v} outside 0..=1");
    }
    Ok(GAIN_MIN_DB + (GAIN_MAX_DB - GAIN_MIN_DB) * v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(headamp_gain(1), "/headamp/01/gain");
        assert_eq!(headamp_gain(9), "/headamp/09/gain");
        assert_eq!(headamp_gain(16), "/headamp/16/gain");
        assert_eq!(headamp_phantom(1), "/headamp/01/phantom");
        assert_eq!(headamp_phantom(12), "/headamp/12/phantom");
        assert_eq!(headamp_phantom(16), "/headamp/16/phantom");
    }

    #[test]
    fn gain_range_bounds_match_python_headamp() {
        // xair-api-python HeadAmp.gain: float, from -12.0 to 60.0.
        assert_eq!(GAIN_MIN_DB, -12.0);
        assert_eq!(GAIN_MAX_DB, 60.0);
    }

    #[test]
    fn db_wire_mapping_is_linear() {
        // lin_set/lin_get(-12, 60, …): endpoints + midpoint exact.
        assert_eq!(db_to_wire(-12.0).unwrap(), 0.0);
        assert_eq!(db_to_wire(60.0).unwrap(), 1.0);
        assert_eq!(db_to_wire(24.0).unwrap(), 0.5);
        assert_eq!(wire_to_db(0.0).unwrap(), -12.0);
        assert_eq!(wire_to_db(1.0).unwrap(), 60.0);
        assert_eq!(wire_to_db(0.5).unwrap(), 24.0);
        // Round-trip across the range within f32 error.
        for db in [-12.0, -6.5, 0.0, 12.0, 24.0, 48.0, 60.0] {
            let back = wire_to_db(db_to_wire(db).unwrap()).unwrap();
            assert!((back - db).abs() < 1e-5, "round-trip {db} -> {back}");
        }
    }

    #[test]
    fn mapping_rejects_bad_values() {
        assert!(db_to_wire(-12.1).is_err());
        assert!(db_to_wire(60.1).is_err());
        assert!(db_to_wire(f32::NAN).is_err());
        assert!(db_to_wire(f32::INFINITY).is_err());
        assert!(wire_to_db(-0.001).is_err());
        assert!(wire_to_db(1.001).is_err());
        assert!(wire_to_db(f32::NAN).is_err());
    }

    #[test]
    fn phantom_roundtrips_as_plain_bool_int() {
        // Phantom shares the single plain-bool wire codec: int `1`/`0`,
        // decoded with `osc::is_on` (`wire == 1` means on).
        use openmix_transport::osc;
        use rosc::{OscMessage, OscType};
        assert_eq!(crate::codec::bool_arg(true), OscType::Int(1));
        assert_eq!(crate::codec::bool_arg(false), OscType::Int(0));
        let on = OscMessage {
            addr: "/headamp/01/phantom".into(),
            args: vec![OscType::Int(1)],
        };
        let off = OscMessage {
            addr: "/headamp/01/phantom".into(),
            args: vec![OscType::Int(0)],
        };
        assert!(osc::is_on(&on).unwrap());
        assert!(!osc::is_on(&off).unwrap());
    }
}
