//! X-Air channel gate (`/ch/NN/gate/…`, ch strips only).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`,
//!   extracted from Behringer-provided firmware): the gate block carries
//!   `on [0,1]`, `mode [0,4] // GATE, EXP2, EXP3, EXP4, DUCK`,
//!   `thr [0.0,1.0] lin(161) // -80..0`, `range [0.0,1.0] lin(58) // 3..60`,
//!   `attack [0.0,1.0] lin(121) // 0..120`,
//!   `hold [0.0,1.0] log(101) // 0.02..2000`,
//!   `release [0.0,1.0] log(101) // 5..4000`,
//!   `keysrc [0,22] // SELF, Ch01..Ch16, Bus1..Bus6` and a `filter/` node
//!   (`on [0,1]`, `type [0,8]`, `f [0.0,1.0] log(201) // 20..20000`).
//!   Availability column is ch-only: buses/LR/main carry dyn+EQ but no gate.
//! - protocol reference X32 OSC protocol documentation (`/tmp/x32.pdf`): backs
//!   the leaf-name vocabulary (`thr`, `range`, `attack`, `hold`, `release`,
//!   `mode`) shared across the X-Air/X32 gate blocks.
//! - xair-api-python `shared.py` (MIT, onyx-and-iris, dev branch, fetched
//!   2026-10-18): gate leaves under `{root}/gate` as int/float params.
//!
//! Owner-rig verified 2026-10-18 (XR18 FW 1.20, `/ch/01` reads):
//! `thr`/`range`/`attack`/`release` floats, `mode`/`keysrc` ints,
//! `filter/` node answering `[on, type, f]` (e.g. `[0, 6, 0.565]`) with each
//! sub-leaf individually queryable. Linear-vs-log curve tags come from the
//! firmware map above (the map gives ranges AND the lin/log tag).

use rosc::OscType;

/// Gate threshold bounds in dB (firmware map `lin(161) // -80..0`).
pub const GATE_THR_MIN_DB: f32 = -80.0;
/// Gate threshold upper bound (see [`GATE_THR_MIN_DB`]).
pub const GATE_THR_MAX_DB: f32 = 0.0;
/// Gate range (depth) bounds in dB (firmware map `lin(58) // 3..60`).
pub const GATE_RANGE_MIN_DB: f32 = 3.0;
/// Gate range upper bound (see [`GATE_RANGE_MIN_DB`]).
pub const GATE_RANGE_MAX_DB: f32 = 60.0;
/// Gate attack bounds in ms (firmware map `lin(121) // 0..120`).
pub const GATE_ATT_MIN_MS: f32 = 0.0;
/// Gate attack upper bound (see [`GATE_ATT_MIN_MS`]).
pub const GATE_ATT_MAX_MS: f32 = 120.0;
/// Gate hold bounds in ms (firmware map `log(101) // 0.02..2000`).
pub const GATE_HOLD_MIN_MS: f32 = 0.02;
/// Gate hold upper bound (see [`GATE_HOLD_MIN_MS`]).
pub const GATE_HOLD_MAX_MS: f32 = 2000.0;
/// Gate release bounds in ms (firmware map `log(101) // 5..4000`).
pub const GATE_REL_MIN_MS: f32 = 5.0;
/// Gate release upper bound (see [`GATE_REL_MIN_MS`]).
pub const GATE_REL_MAX_MS: f32 = 4000.0;
/// Gate mode bounds (firmware map `0..4 // GATE, EXP2, EXP3, EXP4, DUCK`).
pub const GATE_MODE_MIN: i32 = 0;
/// Gate mode upper bound (see [`GATE_MODE_MIN`]).
pub const GATE_MODE_MAX: i32 = 4;
/// Gate key-source bounds (firmware map `0..22 // SELF, Ch01..16, Bus1..6`).
pub const GATE_KEY_SRC_MIN: i32 = 0;
/// Gate key-source upper bound (see [`GATE_KEY_SRC_MIN`]).
pub const GATE_KEY_SRC_MAX: i32 = 22;
/// Gate key-filter on/off (firmware `filter/on [0,1]`).
pub const GATE_FILTER_ON_MIN: i32 = 0;
/// Gate key-filter on/off upper bound.
pub const GATE_FILTER_ON_MAX: i32 = 1;
/// Gate key-filter type bounds (firmware `filter/type [0,8] //
/// LC6, LC12, HC6, HC12, 1.0, 2.0, 3.0, 5.0, 10.0`).
pub const GATE_FILTER_TYPE_MIN: i32 = 0;
/// Gate key-filter type upper bound (see [`GATE_FILTER_TYPE_MIN`]).
pub const GATE_FILTER_TYPE_MAX: i32 = 8;
/// Gate key-filter corner bounds in Hz (firmware `filter/f log(201) //
/// 20..20000`, same shape as the EQ corner).
pub const GATE_FILTER_FREQ_MIN_HZ: f32 = 20.0;
/// Gate key-filter corner upper bound (see [`GATE_FILTER_FREQ_MIN_HZ`]).
pub const GATE_FILTER_FREQ_MAX_HZ: f32 = 20000.0;

/// OSC root for one strip (`/ch/NN`, `/bus/N`, `/fxrtn/NN`, `/lr`).
fn leaf(root: &str, leaf: &str) -> String {
    format!("{root}/gate/{leaf}")
}

/// Gate threshold address (dB, assumed linear wire).
pub fn gate_thr(root: &str) -> String {
    leaf(root, "thr")
}
/// Gate range (depth) address (dB, assumed linear wire).
pub fn gate_range(root: &str) -> String {
    leaf(root, "range")
}
/// Gate attack address (ms, assumed linear wire).
pub fn gate_attack(root: &str) -> String {
    leaf(root, "attack")
}
/// Gate hold address (ms, assumed linear wire).
pub fn gate_hold(root: &str) -> String {
    leaf(root, "hold")
}
/// Gate release address (ms, assumed linear wire).
pub fn gate_release(root: &str) -> String {
    leaf(root, "release")
}
/// Gate mode address (assumed int `0..=2`).
pub fn gate_mode(root: &str) -> String {
    leaf(root, "mode")
}
/// Gate key-source address (firmware `keysrc [0,22]`, int).
pub fn gate_key_src(root: &str) -> String {
    leaf(root, "keysrc")
}

/// Gate key-filter on/off address (firmware `filter/on [0,1]`, int).
pub fn gate_filter_on(root: &str) -> String {
    format!("{root}/gate/filter/on")
}

/// Gate key-filter type address (firmware `filter/type [0,8]`, int).
pub fn gate_filter_type(root: &str) -> String {
    format!("{root}/gate/filter/type")
}

/// Gate key-filter corner address (firmware `filter/f`, log wire
/// `0..1` ↔ `20..20000` Hz).
pub fn gate_filter_freq(root: &str) -> String {
    format!("{root}/gate/filter/f")
}

/// Map engineering dB/ms to a normalized wire float over `[min, max]`
/// (firmware `lin(n)` leaves). Rejects NaN/infinite and out-of-range values.
pub fn lin_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(v, min, max, "gate", what)
}

/// Map a normalized wire float back over `[min, max]` (firmware `lin(n)`).
pub fn wire_to_lin(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(w, min, max, "gate", what)
}

/// Map engineering ms to a normalized wire float over `[min, max]`
/// (firmware `log(n)` leaves: `value = min * exp(log(max/min) * wire)`).
/// Rejects NaN/infinite and out-of-range values.
pub fn log_to_wire(v: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::log_to_wire(v, min, max, "gate", what)
}

/// Map a normalized wire float back over `[min, max]` (firmware `log(n)`).
pub fn wire_to_log(w: f32, min: f32, max: f32, what: &str) -> anyhow::Result<f32> {
    crate::codec::wire_to_log(w, min, max, "gate", what)
}

/// Map a key-filter corner in Hz to the normalized wire float
/// (`log(201) // 20..20000`, same shape as the EQ corner).
pub fn filter_hz_to_wire(hz: f32) -> anyhow::Result<f32> {
    log_to_wire(hz, GATE_FILTER_FREQ_MIN_HZ, GATE_FILTER_FREQ_MAX_HZ, "filter freq")
}

/// Map a normalized wire float back to the key-filter corner in Hz.
pub fn wire_to_filter_hz(w: f32) -> anyhow::Result<f32> {
    wire_to_log(w, GATE_FILTER_FREQ_MIN_HZ, GATE_FILTER_FREQ_MAX_HZ, "filter freq")
}

/// Encode an int leaf (mode/keysrc/filter): whole-number floats.
pub fn int_leaf_to_arg(v: f32, min: i32, max: i32, what: &str) -> anyhow::Result<OscType> {
    crate::codec::int_to_arg(v, min, max, "gate", what)
}

/// Decode an int leaf reply (int kept, whole-number float accepted).
pub fn int_leaf_from_msg(msg: &rosc::OscMessage, min: i32, max: i32, what: &str) -> anyhow::Result<i32> {
    crate::codec::int_from_msg(msg, min, max, "gate", what)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(gate_thr("/ch/01"), "/ch/01/gate/thr");
        assert_eq!(gate_mode("/ch/01"), "/ch/01/gate/mode");
        assert_eq!(gate_key_src("/ch/16"), "/ch/16/gate/keysrc");
        assert_eq!(gate_filter_on("/ch/01"), "/ch/01/gate/filter/on");
        assert_eq!(gate_filter_type("/ch/01"), "/ch/01/gate/filter/type");
        assert_eq!(gate_filter_freq("/ch/01"), "/ch/01/gate/filter/f");
        assert_eq!(gate_release("/ch/01"), "/ch/01/gate/release");
    }

    #[test]
    fn lin_mapping_roundtrips() {
        assert_eq!(lin_to_wire(-80.0, GATE_THR_MIN_DB, GATE_THR_MAX_DB, "thr").unwrap(), 0.0);
        assert_eq!(lin_to_wire(0.0, GATE_THR_MIN_DB, GATE_THR_MAX_DB, "thr").unwrap(), 1.0);
        let back = wire_to_lin(0.375, GATE_THR_MIN_DB, GATE_THR_MAX_DB, "thr").unwrap();
        assert!((back + 50.0).abs() < 1e-5, "{back}");
        assert!(lin_to_wire(-80.1, GATE_THR_MIN_DB, GATE_THR_MAX_DB, "thr").is_err());
        assert!(wire_to_lin(1.1, GATE_THR_MIN_DB, GATE_THR_MAX_DB, "thr").is_err());
    }

    #[test]
    fn log_mapping_matches_firmware_tags() {
        // Firmware map: hold log(101) // 0.02..2000, release log(101) // 5..4000.
        assert_eq!(log_to_wire(0.02, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap(), 0.0);
        assert_eq!(log_to_wire(2000.0, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap(), 1.0);
        assert_eq!(wire_to_log(0.0, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap(), 0.02);
        // Live rig wire 0.76 -> ~126 ms.
        let live = wire_to_log(0.76, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap();
        assert!((live - 126.0).abs() < 3.0, "live hold {live}");
        for ms in [0.02, 1.0, 50.0, 500.0, 2000.0] {
            let back = wire_to_log(log_to_wire(ms, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap(), GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").unwrap();
            assert!((back - ms).abs() / ms < 1e-4, "round-trip {ms} -> {back}");
        }
        assert!(log_to_wire(0.019, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").is_err());
        assert!(wire_to_log(1.1, GATE_HOLD_MIN_MS, GATE_HOLD_MAX_MS, "hold").is_err());
    }

    #[test]
    fn int_codec_shapes() {
        assert_eq!(int_leaf_to_arg(4.0, GATE_MODE_MIN, GATE_MODE_MAX, "mode").unwrap(), OscType::Int(4));
        assert!(int_leaf_to_arg(5.0, GATE_MODE_MIN, GATE_MODE_MAX, "mode").is_err());
        assert!(int_leaf_to_arg(1.5, GATE_MODE_MIN, GATE_MODE_MAX, "mode").is_err());
        // Key source spans SELF + 16 channels + 6 buses (firmware 0..22).
        assert_eq!(int_leaf_to_arg(22.0, GATE_KEY_SRC_MIN, GATE_KEY_SRC_MAX, "keysrc").unwrap(), OscType::Int(22));
        assert!(int_leaf_to_arg(23.0, GATE_KEY_SRC_MIN, GATE_KEY_SRC_MAX, "keysrc").is_err());
    }
}
