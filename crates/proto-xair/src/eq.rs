//! X-Air channel EQ (`/ch/NN/eq/…`, 4 bands; bus/main roots carry 6).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`,
//!   extracted from Behringer-provided firmware — the fullest X-Air table):
//!   `eq/ on [0,1]`, bands `1..4` (ch, rtn) / `1..6` (bus, lr):
//!   `type [0,5]` (LCut, LShv, PEQ, VEQ, HShv, HCut), `f [0.0,1.0] log(201)`
//!   (20..20000 Hz), `g [0.0,1.0] lin(121)` (−15..+15 dB),
//!   `q [0.0,1.0] log(72)` (10..0.3 — note the REVERSED direction: wire 0
//!   is Q 10, wire 1 is Q 0.3).
//! - xair-api-python `shared.py` (MIT, onyx-and-iris, dev branch, fetched
//!   2026-10-18): `EQBand` at `{root}/eq/{i}` (1-based) with `type` (raw
//!   int), `frequency` via `log_get/log_set(20, 20000)`, `gain` via
//!   `lin_get/lin_set(-15, 15)`, `quality` via `log_get/log_set(0.3, 10)`
//!   on the INVERTED wire (`1.0 - wire`); `EQ.on` int `1`/`0`. The `q`
//!   inversion agrees with the firmware map's `10..0.3` direction.
//! - protocol reference X32 OSC protocol documentation (`/tmp/x32.pdf`): backs
//!   only the leaf names and the six filter-type order
//!   (LCut, LShv, PEQ, VEQ, HShv, HCut = `0..=5`); X32 Q runs `10..0.3`
//!   too, so the reversed log direction is corroborated, not assumed.
//!
//! Owner-rig verified 2026-10-18 (XR18 FW 1.20): bus bands 5+6 answer
//! (`/bus/1/eq/5/g`, `/bus/1/eq/6/g` → `0.5`); channel bands 1..4 verified
//! through the driver readback path.

use rosc::{OscMessage, OscType};

/// EQ gain bounds in dB (firmware map `g lin(121) // -15..15`).
pub const EQ_GAIN_MIN_DB: f32 = -15.0;
pub const EQ_GAIN_MAX_DB: f32 = 15.0;

/// EQ corner bounds in Hz (firmware map `f log(201) // 20..20000`).
pub const EQ_FREQ_MIN_HZ: f32 = 20.0;
pub const EQ_FREQ_MAX_HZ: f32 = 20000.0;

/// EQ Q bounds (firmware map `q log(72) // 10..0.3`; xair-api validates
/// `0.3..=10`). Engineering direction is low→high; the WIRE runs high→low
/// (wire 0 = Q 10), handled inside [`q_to_wire`]/[`wire_to_q`].
pub const EQ_Q_MIN: f32 = 0.3;
pub const EQ_Q_MAX: f32 = 10.0;

/// EQ filter-type bounds (firmware map `type [0,5]`; protocol reference X32 doc order
/// LCut, LShv, PEQ, VEQ, HShv, HCut).
pub const EQ_TYPE_MIN: i32 = 0;
pub const EQ_TYPE_MAX: i32 = 5;

/// Lowest EQ band (firmware map: ch/rtn bands `1..4`, bus/lr `1..6` —
/// enforced per kind by [`check_band_wide`] plus the driver scope check).
pub const EQ_BAND_MIN: u8 = 1;

/// Root-parameterized EQ addresses for bus/main roots (`/bus/N`, `/lr`):
/// same leaves, same codecs, bands `1..=6` (firmware map).
pub fn eq_on_r(root: &str) -> String {
    format!("{root}/eq/on")
}
pub fn eq_gain_r(root: &str, band: u8) -> anyhow::Result<String> {
    check_band_wide(band)?;
    Ok(format!("{root}/eq/{band}/g"))
}
pub fn eq_freq_r(root: &str, band: u8) -> anyhow::Result<String> {
    check_band_wide(band)?;
    Ok(format!("{root}/eq/{band}/f"))
}
pub fn eq_q_r(root: &str, band: u8) -> anyhow::Result<String> {
    check_band_wide(band)?;
    Ok(format!("{root}/eq/{band}/q"))
}
pub fn eq_type_r(root: &str, band: u8) -> anyhow::Result<String> {
    check_band_wide(band)?;
    Ok(format!("{root}/eq/{band}/type"))
}

/// Highest EQ band on bus/main strips (firmware map: bus 6, lr 6).
pub const EQ_BAND_WIDE_MAX: u8 = 6;

/// Validate a 1-based EQ band on a bus/main root (`1..=6`).
pub fn check_band_wide(band: u8) -> anyhow::Result<u8> {
    if !(EQ_BAND_MIN..=EQ_BAND_WIDE_MAX).contains(&band) {
        anyhow::bail!("eq band {band} out of range {EQ_BAND_MIN}..={EQ_BAND_WIDE_MAX}");
    }
    Ok(band)
}

/// xair-api `lin_set(-15, 15, db)`.
pub fn gain_db_to_wire(db: f32) -> anyhow::Result<f32> {
    if !db.is_finite() {
        anyhow::bail!("eq gain must be finite, got {db}");
    }
    if !(EQ_GAIN_MIN_DB..=EQ_GAIN_MAX_DB).contains(&db) {
        anyhow::bail!("eq gain {db} dB out of range {EQ_GAIN_MIN_DB}..={EQ_GAIN_MAX_DB}");
    }
    Ok((db - EQ_GAIN_MIN_DB) / (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB))
}

/// xair-api `lin_get(-15, 15, v)`.
pub fn wire_to_gain_db(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("eq gain wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("eq gain wire value {v} outside 0..=1");
    }
    Ok(EQ_GAIN_MIN_DB + (EQ_GAIN_MAX_DB - EQ_GAIN_MIN_DB) * v)
}

/// xair-api `log_set(20, 20000, hz)`.
pub fn freq_hz_to_wire(hz: f32) -> anyhow::Result<f32> {
    if !hz.is_finite() {
        anyhow::bail!("eq corner must be finite, got {hz}");
    }
    if !(EQ_FREQ_MIN_HZ..=EQ_FREQ_MAX_HZ).contains(&hz) {
        anyhow::bail!("eq corner {hz} Hz out of range {EQ_FREQ_MIN_HZ}..={EQ_FREQ_MAX_HZ}");
    }
    Ok((hz / EQ_FREQ_MIN_HZ).ln() / (EQ_FREQ_MAX_HZ / EQ_FREQ_MIN_HZ).ln())
}

/// xair-api `log_get(20, 20000, v)`, rounded to 0.1 Hz.
pub fn wire_to_freq_hz(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("eq freq wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("eq freq wire value {v} outside 0..=1");
    }
    Ok((EQ_FREQ_MIN_HZ * (v * (EQ_FREQ_MAX_HZ / EQ_FREQ_MIN_HZ).ln()).exp() * 10.0).round() / 10.0)
}

/// Wire runs INVERTED: xair-api `1.0 - log_set(0.3, 10, q)`, firmware `q log(72) // 10..0.3`.
pub fn q_to_wire(q: f32) -> anyhow::Result<f32> {
    if !q.is_finite() {
        anyhow::bail!("eq Q must be finite, got {q}");
    }
    if !(EQ_Q_MIN..=EQ_Q_MAX).contains(&q) {
        anyhow::bail!("eq Q {q} out of range {EQ_Q_MIN}..={EQ_Q_MAX}");
    }
    Ok(1.0 - (q / EQ_Q_MIN).ln() / (EQ_Q_MAX / EQ_Q_MIN).ln())
}

/// xair-api `log_get(0.3, 10, 1 - v)`, rounded to 0.1.
pub fn wire_to_q(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("eq Q wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("eq Q wire value {v} outside 0..=1");
    }
    Ok((EQ_Q_MIN * ((1.0 - v) * (EQ_Q_MAX / EQ_Q_MIN).ln()).exp() * 10.0).round() / 10.0)
}

/// Whole-number floats `0..=5` become the wire int.
pub fn eqtype_to_arg(v: f32) -> anyhow::Result<OscType> {
    if !v.is_finite() || v.fract() != 0.0 {
        anyhow::bail!("eq type must be a whole number, got {v}");
    }
    let i = v as i32;
    if !(EQ_TYPE_MIN..=EQ_TYPE_MAX).contains(&i) {
        anyhow::bail!("eq type {i} out of range {EQ_TYPE_MIN}..={EQ_TYPE_MAX}");
    }
    Ok(OscType::Int(i))
}

/// Int `0..=5`; whole-number floats accepted like [`crate::fx::parse_fx_param`].
pub fn eqtype_from_msg(msg: &OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => {
            if !(EQ_TYPE_MIN..=EQ_TYPE_MAX).contains(i) {
                anyhow::bail!(
                    "{}: eq type {i} outside {EQ_TYPE_MIN}..={EQ_TYPE_MAX}",
                    msg.addr
                );
            }
            Ok(*i)
        }
        Some(OscType::Float(f)) => {
            if !f.is_finite() || f.fract() != 0.0 {
                anyhow::bail!("{}: eq type must be a whole number, got {f}", msg.addr);
            }
            let i = *f as i32;
            if !(EQ_TYPE_MIN..=EQ_TYPE_MAX).contains(&i) {
                anyhow::bail!(
                    "{}: eq type {i} outside {EQ_TYPE_MIN}..={EQ_TYPE_MAX}",
                    msg.addr
                );
            }
            Ok(i)
        }
        other => anyhow::bail!("{}: eq type needs an int arg, got {other:?}", msg.addr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(eq_on_r("/ch/01"), "/ch/01/eq/on");
        assert_eq!(eq_gain_r("/ch/01", 1).unwrap(), "/ch/01/eq/1/g");
        assert_eq!(eq_freq_r("/ch/16", 4).unwrap(), "/ch/16/eq/4/f");
        assert_eq!(eq_q_r("/ch/09", 2).unwrap(), "/ch/09/eq/2/q");
        assert_eq!(eq_type_r("/ch/03", 4).unwrap(), "/ch/03/eq/4/type");
        assert_eq!(eq_gain_r("/bus/1", 6).unwrap(), "/bus/1/eq/6/g");
        assert_eq!(eq_freq_r("/lr", 5).unwrap(), "/lr/eq/5/f");
        assert!(eq_gain_r("/ch/01", 0).is_err());
        assert!(eq_gain_r("/ch/01", 7).is_err());
    }

    #[test]
    fn gain_range_matches_firmware_map() {
        assert_eq!((EQ_GAIN_MIN_DB, EQ_GAIN_MAX_DB), (-15.0, 15.0));
        assert_eq!(gain_db_to_wire(-15.0).unwrap(), 0.0);
        assert_eq!(gain_db_to_wire(15.0).unwrap(), 1.0);
        assert_eq!(gain_db_to_wire(0.0).unwrap(), 0.5);
        assert_eq!(wire_to_gain_db(0.0).unwrap(), -15.0);
        assert_eq!(wire_to_gain_db(1.0).unwrap(), 15.0);
        for db in [-15.0, -7.5, 0.0, 7.5, 15.0] {
            let back = wire_to_gain_db(gain_db_to_wire(db).unwrap()).unwrap();
            assert!((back - db).abs() < 1e-5, "round-trip {db} -> {back}");
        }
        assert!(gain_db_to_wire(15.1).is_err());
        assert!(wire_to_gain_db(1.1).is_err());
    }

    #[test]
    fn freq_range_matches_firmware_map() {
        assert_eq!(freq_hz_to_wire(20.0).unwrap(), 0.0);
        assert_eq!(freq_hz_to_wire(20000.0).unwrap(), 1.0);
        assert_eq!(wire_to_freq_hz(0.0).unwrap(), 20.0);
        assert_eq!(wire_to_freq_hz(1.0).unwrap(), 20000.0);
        for hz in [20.0, 100.0, 1000.0, 5000.0, 20000.0] {
            let back = wire_to_freq_hz(freq_hz_to_wire(hz).unwrap()).unwrap();
            assert!((back - hz).abs() / hz < 1e-3, "round-trip {hz} -> {back}");
        }
        assert!(freq_hz_to_wire(19.9).is_err());
        assert!(wire_to_freq_hz(-0.1).is_err());
    }

    #[test]
    fn q_wire_runs_inverted_like_xair_api() {
        // Wire 0 is Q 10, wire 1 is Q 0.3 (firmware `10..0.3`,
        // xair-api `1.0 - log_set(0.3, 10, q)`).
        assert_eq!(q_to_wire(10.0).unwrap(), 0.0);
        assert_eq!(q_to_wire(0.3).unwrap(), 1.0);
        assert_eq!(wire_to_q(0.0).unwrap(), 10.0);
        assert_eq!(wire_to_q(1.0).unwrap(), 0.3);
        for q in [0.3, 0.7, 1.0, 2.5, 10.0] {
            let back = wire_to_q(q_to_wire(q).unwrap()).unwrap();
            assert!((back - q).abs() / q < 0.06, "round-trip {q} -> {back}");
        }
        assert!(q_to_wire(0.29).is_err());
        assert!(q_to_wire(10.1).is_err());
    }

    #[test]
    fn eqtype_codec_shapes() {
        assert_eq!(eqtype_to_arg(0.0).unwrap(), OscType::Int(0));
        assert_eq!(eqtype_to_arg(5.0).unwrap(), OscType::Int(5));
        assert!(eqtype_to_arg(6.0).is_err());
        assert!(eqtype_to_arg(2.5).is_err());
        let reply = OscMessage {
            addr: "/ch/01/eq/2/type".into(),
            args: vec![OscType::Int(2)],
        };
        assert_eq!(eqtype_from_msg(&reply).unwrap(), 2);
        let bad = OscMessage {
            addr: "/ch/01/eq/2/type".into(),
            args: vec![OscType::Int(6)],
        };
        assert!(eqtype_from_msg(&bad).is_err());
    }
}
