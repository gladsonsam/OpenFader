//! X-Air dynamics (`/ch/NN/dyn/…`, ch/bus/LR roots).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`,
//!   extracted from Behringer-provided firmware): the dyn block carries
//!   `on [0,1]`, `mode [0,1] // COMP, EXP`, `det [0,1] // PEAK, RMS`,
//!   `env [0,1] // LIN, LOG`, `thr [0.0,1.0] lin(121) // -60..0`,
//!   `ratio [0,11] // 1.1, 1.3, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10, 20, 100`
//!   (an INT step index, not a direct float),
//!   `knee [0.0,1.0] lin(6) // 0..5`, `mgain [0.0,1.0] lin(49) // 0..24`,
//!   `attack [0.0,1.0] lin(121) // 0..120`,
//!   `hold [0.0,1.0] log(101) // 0.02..2000`,
//!   `release [0.0,1.0] log(101) // 5..4000`,
//!   `mix [0.0,1.0] lin(51) // 0..100`, `keysrc [0,22]`,
//!   `auto [0,1]` plus a `filter/` node (`on`, `type [0,8]`, `f log // 20..20000`).
//!   Availability column is ch/bus/LR (no gate-style filter on returns).
//! - protocol reference X32 OSC protocol documentation (`/tmp/x32.pdf`): backs
//!   the leaf-name vocabulary (`thr`, `ratio`, `attack`, `hold`, `release`)
//!   shared across the X-Air/X32 dyn blocks.
//! - xair-api-python `shared.py` + `util.py` (MIT, onyx-and-iris, dev branch,
//!   fetched 2026-10-18): dyn leaves under `{root}/dyn`; `lin_get/lin_set`
//!   for dB/ms leaves, int `1`/`0` for switches.
//!
//! Owner-rig verified 2026-10-18 (XR18 FW 1.20): `/ch/01/dyn/ratio` answers
//! `Int(5)` — the step-table index (5 → 3.0), proving the direct-float wire
//! is wrong; `/ch/01/dyn/keysrc` and `/ch/01/dyn/filter/on` answer `Int(0)`.

use rosc::OscType;

/// Comp threshold bounds in dB (firmware map `lin(121) // -60..0`).
pub const COMP_THR_MIN_DB: f32 = -60.0;
pub const COMP_THR_MAX_DB: f32 = 0.0;
/// Comp ratio step table (firmware map `ratio [0,11]`: the wire is the INT
/// index into this table, NOT the ratio number).
pub const COMP_RATIO_TABLE: [f32; 12] = [
    1.1, 1.3, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10.0, 20.0, 100.0,
];
/// Comp ratio index bounds (firmware `ratio [0,11]`).
pub const COMP_RATIO_MIN: i32 = 0;
pub const COMP_RATIO_MAX: i32 = 11;
/// Comp knee bounds (firmware `lin(6) // 0..5` over wire `0..1`).
pub const COMP_KNEE_MIN: f32 = 0.0;
pub const COMP_KNEE_MAX: f32 = 5.0;
/// Comp makeup-gain bounds in dB (firmware `lin(49) // 0..24`).
pub const COMP_MAKEUP_MIN_DB: f32 = 0.0;
pub const COMP_MAKEUP_MAX_DB: f32 = 24.0;
/// Comp attack bounds in ms (firmware `lin(121) // 0..120`).
pub const COMP_ATT_MIN_MS: f32 = 0.0;
pub const COMP_ATT_MAX_MS: f32 = 120.0;
/// Comp hold bounds in ms (firmware `log(101) // 0.02..2000`).
pub const COMP_HOLD_MIN_MS: f32 = 0.02;
pub const COMP_HOLD_MAX_MS: f32 = 2000.0;
/// Comp release bounds in ms (firmware `log(101) // 5..4000`).
pub const COMP_REL_MIN_MS: f32 = 5.0;
pub const COMP_REL_MAX_MS: f32 = 4000.0;
/// Comp mode bounds (assumed 0 = compressor, 1 = expander).
pub const COMP_MODE_MIN: i32 = 0;
pub const COMP_MODE_MAX: i32 = 1;
/// Detector bounds (assumed 0 = peak, 1 = RMS).
pub const COMP_DET_MIN: i32 = 0;
pub const COMP_DET_MAX: i32 = 1;
/// Envelope bounds (assumed 0 = linear, 1 = logarithmic).
pub const COMP_ENV_MIN: i32 = 0;
pub const COMP_ENV_MAX: i32 = 1;
/// Parallel-mix bounds in % (assumed linear wire `0..1` ↔ `0..=100`).
pub const COMP_MIX_MIN: f32 = 0.0;
pub const COMP_MIX_MAX: f32 = 100.0;

fn leaf(root: &str, leaf: &str) -> String {
    format!("{root}/dyn/{leaf}")
}

pub fn comp_thr(root: &str) -> String {
    leaf(root, "thr")
}
/// Wire is the INT step-table index, not the ratio number (see [`ratio_to_index`]).
pub fn comp_ratio(root: &str) -> String {
    leaf(root, "ratio")
}
pub fn comp_knee(root: &str) -> String {
    leaf(root, "knee")
}
pub fn comp_makeup(root: &str) -> String {
    leaf(root, "mgain")
}
pub fn comp_attack(root: &str) -> String {
    leaf(root, "attack")
}
pub fn comp_hold(root: &str) -> String {
    leaf(root, "hold")
}
pub fn comp_release(root: &str) -> String {
    leaf(root, "release")
}
pub fn comp_mode(root: &str) -> String {
    leaf(root, "mode")
}
pub fn comp_det(root: &str) -> String {
    leaf(root, "det")
}
pub fn comp_env(root: &str) -> String {
    leaf(root, "env")
}
pub fn comp_mix(root: &str) -> String {
    leaf(root, "mix")
}
pub fn comp_auto(root: &str) -> String {
    leaf(root, "auto")
}
pub fn comp_key_src(root: &str) -> String {
    leaf(root, "keysrc")
}

pub fn comp_filter_on(root: &str) -> String {
    format!("{root}/dyn/filter/on")
}

pub fn comp_filter_type(root: &str) -> String {
    format!("{root}/dyn/filter/type")
}

pub fn comp_filter_freq(root: &str) -> String {
    format!("{root}/dyn/filter/f")
}

/// Firmware `lin(n)` leaves.
pub fn lin_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(v, min, max, "comp", what)
}

/// Firmware `lin(n)` leaves (decode direction).
pub fn wire_to_lin(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(w, min, max, "comp", what)
}

/// Firmware `log(n)` leaves.
pub fn log_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::log_to_wire(v, min, max, "comp", what)
}

/// Firmware `log(n)` leaves (decode direction).
pub fn wire_to_log(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_log(w, min, max, "comp", what)
}

/// UI value must be an exact table entry; the wire is the INT index.
pub fn ratio_to_index(v: f32) -> anyhow::Result<i32> {
    if !v.is_finite() {
        anyhow::bail!("comp ratio must be finite, got {v}");
    }
    COMP_RATIO_TABLE
        .iter()
        .position(|t| (t - v).abs() < 1e-6)
        .map(|i| i as i32)
        .ok_or_else(|| {
            anyhow::anyhow!("comp ratio {v} is not a desk step; pick one of {COMP_RATIO_TABLE:?}")
        })
}

/// Ratio-index encode (whole-number floats `0..=11`).
pub fn ratio_index_to_arg(v: f32) -> anyhow::Result<OscType> {
    int_leaf_to_arg(v, COMP_RATIO_MIN, COMP_RATIO_MAX, "ratio")
}

/// Ratio-index reply decode to the engineering ratio number.
pub fn wire_to_ratio(msg: &rosc::OscMessage) -> anyhow::Result<f32> {
    let i = int_leaf_from_msg(msg, COMP_RATIO_MIN, COMP_RATIO_MAX, "ratio")?;
    Ok(COMP_RATIO_TABLE[i as usize])
}

pub fn int_leaf_to_arg(v: f32, min: i32, max: i32, what: &str) -> anyhow::Result<OscType> {
    crate::codec::int_to_arg(v, min, max, "comp", what)
}

pub fn int_leaf_from_msg(msg: &rosc::OscMessage, min: i32, max: i32, what: &str) -> anyhow::Result<i32> {
    crate::codec::int_from_msg(msg, min, max, "comp", what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(comp_thr("/ch/01"), "/ch/01/dyn/thr");
        assert_eq!(comp_ratio("/ch/16"), "/ch/16/dyn/ratio");
        assert_eq!(comp_auto("/bus/1"), "/bus/1/dyn/auto");
        assert_eq!(comp_mix("/lr"), "/lr/dyn/mix");
        assert_eq!(comp_key_src("/ch/01"), "/ch/01/dyn/keysrc");
        assert_eq!(comp_filter_on("/ch/01"), "/ch/01/dyn/filter/on");
        assert_eq!(comp_filter_type("/bus/1"), "/bus/1/dyn/filter/type");
        assert_eq!(comp_filter_freq("/lr"), "/lr/dyn/filter/f");
    }

    #[test]
    fn lin_mapping_roundtrips() {
        assert_eq!(lin_to_wire(-60.0, COMP_THR_MIN_DB, COMP_THR_MAX_DB, "thr").unwrap(), 0.0);
        assert_eq!(lin_to_wire(0.0, COMP_THR_MIN_DB, COMP_THR_MAX_DB, "thr").unwrap(), 1.0);
        assert!(lin_to_wire(0.1, COMP_THR_MIN_DB, COMP_THR_MAX_DB, "thr").is_err());
    }

    #[test]
    fn ratio_step_table_codec() {
        assert_eq!(COMP_RATIO_TABLE[5], 3.0);
        assert_eq!(ratio_to_index(3.0).unwrap(), 5);
        assert_eq!(ratio_to_index(100.0).unwrap(), 11);
        assert!(ratio_to_index(3.5).is_err());
        assert_eq!(ratio_index_to_arg(5.0).unwrap(), OscType::Int(5));
        assert!(ratio_index_to_arg(12.0).is_err());
        let reply = rosc::OscMessage {
            addr: "/ch/01/dyn/ratio".into(),
            args: vec![OscType::Int(5)],
        };
        assert_eq!(wire_to_ratio(&reply).unwrap(), 3.0);
    }
}
