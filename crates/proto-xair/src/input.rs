//! X-Air input routing and preamp codecs.
//! Leaf shapes follow the XR18 firmware OSC map and xair-api-python (MIT).
//! Firmware behavior varies: source-select reads are not exposed as writes.

use openfader_transport::osc;
use rosc::{OscMessage, OscType};

/// USB-return source index bounds (firmware map: `rtnsrc [0,17] // U01..U18`).
pub const RTNSRC_MIN: i32 = 0;
pub const RTNSRC_MAX: i32 = 17;

/// USB-return trim bounds in dB (firmware map + xair-api `usbtrim`).
pub const RTNTRIM_MIN_DB: f32 = -18.0;
pub const RTNTRIM_MAX_DB: f32 = 18.0;

/// HPF corner bounds in Hz (firmware map + xair-api `highpassfilter`).
pub const HPF_MIN_HZ: f32 = 20.0;
pub const HPF_MAX_HZ: f32 = 400.0;

/// Firmware `rtnsrc`, xair-api `Config.usbreturn`.
///
/// ```
/// # use openfader_proto_xair::input::config_rtnsrc;
/// assert_eq!(config_rtnsrc(1), "/ch/01/config/rtnsrc");
/// assert_eq!(config_rtnsrc(16), "/ch/16/config/rtnsrc");
/// ```
pub fn config_rtnsrc(ch: u16) -> String {
    format!("/ch/{ch:02}/config/rtnsrc")
}

/// Firmware `rtntrim`, xair-api `Preamp.usbtrim`.
///
/// ```
/// # use openfader_proto_xair::input::preamp_rtntrim;
/// assert_eq!(preamp_rtntrim(1), "/ch/01/preamp/rtntrim");
/// ```
pub fn preamp_rtntrim(ch: u16) -> String {
    format!("/ch/{ch:02}/preamp/rtntrim")
}

/// Firmware `invert`, xair-api `Preamp.invert`.
///
/// ```
/// # use openfader_proto_xair::input::preamp_invert;
/// assert_eq!(preamp_invert(16), "/ch/16/preamp/invert");
/// ```
pub fn preamp_invert(ch: u16) -> String {
    format!("/ch/{ch:02}/preamp/invert")
}

/// Firmware `hpon`, xair-api `Preamp.highpasson`.
///
/// ```
/// # use openfader_proto_xair::input::preamp_hpf_on;
/// assert_eq!(preamp_hpf_on(1), "/ch/01/preamp/hpon");
/// ```
pub fn preamp_hpf_on(ch: u16) -> String {
    format!("/ch/{ch:02}/preamp/hpon")
}

/// Firmware `hpf`, xair-api `Preamp.highpassfilter`.
///
/// ```
/// # use openfader_proto_xair::input::preamp_hpf;
/// assert_eq!(preamp_hpf(16), "/ch/16/preamp/hpf");
/// ```
pub fn preamp_hpf(ch: u16) -> String {
    format!("/ch/{ch:02}/preamp/hpf")
}

/// xair-api `lin_set(-18, 18, db)`.
pub fn rtntrim_db_to_wire(db: f32) -> anyhow::Result<f32> {
    if !db.is_finite() {
        anyhow::bail!("usb trim must be finite, got {db}");
    }
    if !(RTNTRIM_MIN_DB..=RTNTRIM_MAX_DB).contains(&db) {
        anyhow::bail!("usb trim {db} dB out of range {RTNTRIM_MIN_DB}..={RTNTRIM_MAX_DB}");
    }
    Ok((db - RTNTRIM_MIN_DB) / (RTNTRIM_MAX_DB - RTNTRIM_MIN_DB))
}

/// xair-api `lin_get(-18, 18, v)`.
pub fn rtntrim_wire_to_db(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("usb trim wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("usb trim wire value {v} outside 0..=1");
    }
    Ok(RTNTRIM_MIN_DB + (RTNTRIM_MAX_DB - RTNTRIM_MIN_DB) * v)
}

/// xair-api `log_set(20, 400, hz)`.
pub fn hpf_hz_to_wire(hz: f32) -> anyhow::Result<f32> {
    if !hz.is_finite() {
        anyhow::bail!("hpf corner must be finite, got {hz}");
    }
    if !(HPF_MIN_HZ..=HPF_MAX_HZ).contains(&hz) {
        anyhow::bail!("hpf corner {hz} Hz out of range {HPF_MIN_HZ}..={HPF_MAX_HZ}");
    }
    Ok((hz / HPF_MIN_HZ).ln() / (HPF_MAX_HZ / HPF_MIN_HZ).ln())
}

/// xair-api `log_get(20, 400, v)`, rounded to whole Hz like xair-api (`int(...)`).
pub fn hpf_wire_to_hz(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("hpf wire value must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("hpf wire value {v} outside 0..=1");
    }
    Ok((HPF_MIN_HZ * (v * (HPF_MAX_HZ / HPF_MIN_HZ).ln()).exp()).round())
}

/// Whole-number floats `0..=17` become the wire int (`U01 + index`).
pub fn usbreturn_to_arg(v: f32) -> anyhow::Result<OscType> {
    if !v.is_finite() || v.fract() != 0.0 {
        anyhow::bail!("usb return source must be a whole number, got {v}");
    }
    let i = v as i32;
    if !(RTNSRC_MIN..=RTNSRC_MAX).contains(&i) {
        anyhow::bail!("usb return source {i} out of range {RTNSRC_MIN}..={RTNSRC_MAX}");
    }
    Ok(OscType::Int(i))
}

/// `,i` reply; firmware-doc range enforced, never clamped.
pub fn usbreturn_from_msg(msg: &OscMessage) -> anyhow::Result<i32> {
    let i = osc::int_arg(msg)?;
    if !(RTNSRC_MIN..=RTNSRC_MAX).contains(&i) {
        anyhow::bail!(
            "{}: usb return source {i} outside {RTNSRC_MIN}..={RTNSRC_MAX}",
            msg.addr
        );
    }
    Ok(i)
}

/// Strict integer decoding for the active-source selector and its source index.
pub(super) fn source_selector(msg: &OscMessage, max: i32) -> anyhow::Result<i32> {
    match msg.args.as_slice() {
        [OscType::Int(value)] if (0..=max).contains(value) => Ok(*value),
        _ => anyhow::bail!("{}: invalid source selector", msg.addr),
    }
}

/// The physical and USB source banks are zero-based on the mixer.
pub(super) fn source_label(mode: i32, source: i32) -> String {
    match (mode, source) {
        (0, n @ 0..=15) => format!("Mic {}", n + 1),
        (0, 16) => "Aux L".into(),
        (0, 17) => "Aux R".into(),
        (0, 18) => "Off".into(),
        (1, n @ 0..=17) => format!("USB {}", n + 1),
        _ => unreachable!("validated source selector"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(config_rtnsrc(1), "/ch/01/config/rtnsrc");
        assert_eq!(config_rtnsrc(16), "/ch/16/config/rtnsrc");
        assert_eq!(preamp_rtntrim(1), "/ch/01/preamp/rtntrim");
        assert_eq!(preamp_rtntrim(16), "/ch/16/preamp/rtntrim");
        assert_eq!(preamp_invert(1), "/ch/01/preamp/invert");
        assert_eq!(preamp_invert(16), "/ch/16/preamp/invert");
        assert_eq!(preamp_hpf_on(1), "/ch/01/preamp/hpon");
        assert_eq!(preamp_hpf_on(9), "/ch/09/preamp/hpon");
        assert_eq!(preamp_hpf(1), "/ch/01/preamp/hpf");
        assert_eq!(preamp_hpf(16), "/ch/16/preamp/hpf");
    }

    #[test]
    fn rtntrim_range_matches_firmware_map() {
        assert_eq!(RTNTRIM_MIN_DB, -18.0);
        assert_eq!(RTNTRIM_MAX_DB, 18.0);
        assert_eq!(rtntrim_db_to_wire(-18.0).unwrap(), 0.0);
        assert_eq!(rtntrim_db_to_wire(18.0).unwrap(), 1.0);
        assert_eq!(rtntrim_db_to_wire(0.0).unwrap(), 0.5);
        assert_eq!(rtntrim_wire_to_db(0.0).unwrap(), -18.0);
        assert_eq!(rtntrim_wire_to_db(1.0).unwrap(), 18.0);
        assert_eq!(rtntrim_wire_to_db(0.5).unwrap(), 0.0);
        assert_eq!(rtntrim_wire_to_db(0.5).unwrap(), 0.0);
        for db in [-18.0, -6.0, 0.0, 6.0, 18.0] {
            let back = rtntrim_wire_to_db(rtntrim_db_to_wire(db).unwrap()).unwrap();
            assert!((back - db).abs() < 1e-5, "round-trip {db} -> {back}");
        }
    }

    #[test]
    fn rtntrim_rejects_bad_values() {
        assert!(rtntrim_db_to_wire(-18.1).is_err());
        assert!(rtntrim_db_to_wire(18.1).is_err());
        assert!(rtntrim_db_to_wire(f32::NAN).is_err());
        assert!(rtntrim_db_to_wire(f32::INFINITY).is_err());
        assert!(rtntrim_wire_to_db(-0.001).is_err());
        assert!(rtntrim_wire_to_db(1.001).is_err());
        assert!(rtntrim_wire_to_db(f32::NAN).is_err());
    }

    #[test]
    fn hpf_range_matches_firmware_map() {
        assert_eq!(hpf_hz_to_wire(20.0).unwrap(), 0.0);
        assert_eq!(hpf_hz_to_wire(400.0).unwrap(), 1.0);
        assert_eq!(hpf_wire_to_hz(0.0).unwrap(), 20.0);
        assert_eq!(hpf_wire_to_hz(1.0).unwrap(), 400.0);
        // Log midpoint: 20*sqrt(20) ≈ 89.4 Hz.
        let mid = hpf_wire_to_hz(0.5).unwrap();
        assert!((mid - 89.0).abs() < 1.0, "log midpoint {mid}");
        // Round-trip across the range within 1 Hz (desk quantizes to 101
        // log steps anyway, so the driver never asserts exact Hz).
        for hz in [20.0, 40.0, 80.0, 120.0, 200.0, 400.0] {
            let back = hpf_wire_to_hz(hpf_hz_to_wire(hz).unwrap()).unwrap();
            assert!((back - hz).abs() < 1.0, "round-trip {hz} -> {back}");
        }
    }

    #[test]
    fn hpf_rejects_bad_values() {
        assert!(hpf_hz_to_wire(19.9).is_err());
        assert!(hpf_hz_to_wire(400.1).is_err());
        assert!(hpf_hz_to_wire(f32::NAN).is_err());
        assert!(hpf_hz_to_wire(f32::INFINITY).is_err());
        assert!(hpf_wire_to_hz(-0.001).is_err());
        assert!(hpf_wire_to_hz(1.001).is_err());
        assert!(hpf_wire_to_hz(f32::NAN).is_err());
    }

    #[test]
    fn usbreturn_codec_shapes() {
        assert_eq!(RTNSRC_MIN, 0);
        assert_eq!(RTNSRC_MAX, 17);
        assert_eq!(usbreturn_to_arg(0.0).unwrap(), OscType::Int(0));
        assert_eq!(usbreturn_to_arg(17.0).unwrap(), OscType::Int(17));
        assert!(usbreturn_to_arg(-1.0).is_err());
        assert!(usbreturn_to_arg(18.0).is_err());
        assert!(usbreturn_to_arg(3.5).is_err());
        assert!(usbreturn_to_arg(f32::NAN).is_err());
        let reply = OscMessage {
            addr: "/ch/16/config/rtnsrc".into(),
            args: vec![OscType::Int(17)],
        };
        assert_eq!(usbreturn_from_msg(&reply).unwrap(), 17);
        let out_of_range = OscMessage {
            addr: "/ch/16/config/rtnsrc".into(),
            args: vec![OscType::Int(18)],
        };
        assert!(usbreturn_from_msg(&out_of_range).is_err());
        let wrong_kind = OscMessage {
            addr: "/ch/16/config/rtnsrc".into(),
            args: vec![OscType::String("U18".into())],
        };
        assert!(usbreturn_from_msg(&wrong_kind).is_err());
    }
}
