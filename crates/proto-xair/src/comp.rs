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
/// Comp threshold upper bound (see [`COMP_THR_MIN_DB`]).
pub const COMP_THR_MAX_DB: f32 = 0.0;
/// Comp ratio step table (firmware map `ratio [0,11]`: the wire is the INT
/// index into this table, NOT the ratio number).
pub const COMP_RATIO_TABLE: [f32; 12] = [
    1.1, 1.3, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10.0, 20.0, 100.0,
];
/// Comp ratio index bounds (firmware `ratio [0,11]`).
pub const COMP_RATIO_MIN: i32 = 0;
/// Comp ratio index upper bound (see [`COMP_RATIO_MIN`]).
pub const COMP_RATIO_MAX: i32 = 11;
/// Comp knee bounds (firmware `lin(6) // 0..5` over wire `0..1`).
pub const COMP_KNEE_MIN: f32 = 0.0;
/// Comp knee upper bound (see [`COMP_KNEE_MIN`]).
pub const COMP_KNEE_MAX: f32 = 5.0;
/// Comp makeup-gain bounds in dB (firmware `lin(49) // 0..24`).
pub const COMP_MAKEUP_MIN_DB: f32 = 0.0;
/// Comp makeup-gain upper bound (see [`COMP_MAKEUP_MIN_DB`]).
pub const COMP_MAKEUP_MAX_DB: f32 = 24.0;
/// Comp attack bounds in ms (firmware `lin(121) // 0..120`).
pub const COMP_ATT_MIN_MS: f32 = 0.0;
/// Comp attack upper bound (see [`COMP_ATT_MIN_MS`]).
pub const COMP_ATT_MAX_MS: f32 = 120.0;
/// Comp hold bounds in ms (firmware `log(101) // 0.02..2000`).
pub const COMP_HOLD_MIN_MS: f32 = 0.02;
/// Comp hold upper bound (see [`COMP_HOLD_MIN_MS`]).
pub const COMP_HOLD_MAX_MS: f32 = 2000.0;
/// Comp release bounds in ms (firmware `log(101) // 5..4000`).
pub const COMP_REL_MIN_MS: f32 = 5.0;
/// Comp release upper bound (see [`COMP_REL_MIN_MS`]).
pub const COMP_REL_MAX_MS: f32 = 4000.0;
/// Comp mode bounds (assumed 0 = compressor, 1 = expander).
pub const COMP_MODE_MIN: i32 = 0;
/// Comp mode upper bound (see [`COMP_MODE_MIN`]).
pub const COMP_MODE_MAX: i32 = 1;
/// Detector bounds (assumed 0 = peak, 1 = RMS).
pub const COMP_DET_MIN: i32 = 0;
/// Detector upper bound (see [`COMP_DET_MIN`]).
pub const COMP_DET_MAX: i32 = 1;
/// Envelope bounds (assumed 0 = linear, 1 = logarithmic).
pub const COMP_ENV_MIN: i32 = 0;
/// Envelope upper bound (see [`COMP_ENV_MIN`]).
pub const COMP_ENV_MAX: i32 = 1;
/// Parallel-mix bounds in % (assumed linear wire `0..1` ↔ `0..=100`).
pub const COMP_MIX_MIN: f32 = 0.0;
/// Parallel-mix upper bound (see [`COMP_MIX_MIN`]).
pub const COMP_MIX_MAX: f32 = 100.0;

/// OSC root for one strip (`/ch/NN`, `/bus/N`, `/fxrtn/NN`, `/lr`).
fn leaf(root: &str, leaf: &str) -> String {
    format!("{root}/dyn/{leaf}")
}

/// Comp threshold address (dB, assumed linear wire).
pub fn comp_thr(root: &str) -> String {
    leaf(root, "thr")
}
/// Comp ratio address (ratio number, ASSUMED direct-float wire).
pub fn comp_ratio(root: &str) -> String {
    leaf(root, "ratio")
}
/// Comp knee address (ASSUMED direct float `0..=5`).
pub fn comp_knee(root: &str) -> String {
    leaf(root, "knee")
}
/// Comp makeup-gain address (dB, assumed linear wire).
pub fn comp_makeup(root: &str) -> String {
    leaf(root, "mgain")
}
/// Comp attack address (ms, assumed linear wire).
pub fn comp_attack(root: &str) -> String {
    leaf(root, "attack")
}
/// Comp hold address (ms, assumed linear wire).
pub fn comp_hold(root: &str) -> String {
    leaf(root, "hold")
}
/// Comp release address (ms, firmware `log(101) // 5..4000` wire).
pub fn comp_release(root: &str) -> String {
    leaf(root, "release")
}
/// Comp mode address (firmware `mode [0,1] // COMP, EXP`, int).
pub fn comp_mode(root: &str) -> String {
    leaf(root, "mode")
}
/// Detector address (firmware `det [0,1] // PEAK, RMS`, int).
pub fn comp_det(root: &str) -> String {
    leaf(root, "det")
}
/// Envelope address (firmware `env [0,1] // LIN, LOG`, int).
pub fn comp_env(root: &str) -> String {
    leaf(root, "env")
}
/// Parallel-mix address (firmware `mix lin(51) // 0..100`).
pub fn comp_mix(root: &str) -> String {
    leaf(root, "mix")
}
/// Auto-time address (firmware `auto [0,1] // OFF, ON`, int).
pub fn comp_auto(root: &str) -> String {
    leaf(root, "auto")
}
/// Comp key-source address (firmware `keysrc [0,22]`, int; ch+bus).
pub fn comp_key_src(root: &str) -> String {
    leaf(root, "keysrc")
}

/// Comp sidechain-filter on/off address (firmware `filter/on [0,1]`, int).
pub fn comp_filter_on(root: &str) -> String {
    format!("{root}/dyn/filter/on")
}

/// Comp sidechain-filter type address (firmware `filter/type [0,8]`, int).
pub fn comp_filter_type(root: &str) -> String {
    format!("{root}/dyn/filter/type")
}

/// Comp sidechain-filter corner address (firmware `filter/f`, log wire
/// `0..1` ↔ `20..20000` Hz).
pub fn comp_filter_freq(root: &str) -> String {
    format!("{root}/dyn/filter/f")
}

/// Map engineering dB/ms/% to a normalized wire float over `[min, max]`
/// (firmware `lin(n)` leaves). Rejects NaN/infinite and out-of-range values.
pub fn lin_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(v, min, max, "comp", what)
}

/// Map a normalized wire float back over `[min, max]` (firmware `lin(n)`).
pub fn wire_to_lin(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(w, min, max, "comp", what)
}

/// Map engineering ms to a normalized wire float over `[min, max]`
/// (firmware `log(n)` leaves). Rejects NaN/infinite and out-of-range values.
pub fn log_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::log_to_wire(v, min, max, "comp", what)
}

/// Map a normalized wire float back over `[min, max]` (firmware `log(n)`).
pub fn wire_to_log(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_log(w, min, max, "comp", what)
}

/// Encode a comp ratio as the firmware step-table index: the UI value must
/// be one of [`COMP_RATIO_TABLE`] (within 1e-6); the wire is the INT index.
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

/// Encode a comp ratio index (whole-number floats `0..=11`) to the wire int.
pub fn ratio_index_to_arg(v: f32) -> anyhow::Result<OscType> {
    int_leaf_to_arg(v, COMP_RATIO_MIN, COMP_RATIO_MAX, "ratio")
}

/// Decode a ratio-index reply (int kept, whole-number float accepted) to
/// the engineering ratio number.
pub fn wire_to_ratio(msg: &rosc::OscMessage) -> anyhow::Result<f32> {
    let i = int_leaf_from_msg(msg, COMP_RATIO_MIN, COMP_RATIO_MAX, "ratio")?;
    Ok(COMP_RATIO_TABLE[i as usize])
}

/// Encode an int leaf (mode/det/env/auto): whole-number floats.
pub fn int_leaf_to_arg(v: f32, min: i32, max: i32, what: &str) -> anyhow::Result<OscType> {
    crate::codec::int_to_arg(v, min, max, "comp", what)
}

/// Decode an int leaf reply (int kept, whole-number float accepted).
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
        // Firmware map: the wire is the INT index, not the ratio number.
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
        // Live desk ch01 reads Int(5) = 3.0 (was misreported as 5.0).
        assert_eq!(wire_to_ratio(&reply).unwrap(), 3.0);
    }
}
