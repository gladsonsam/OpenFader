//! X-Air solo monitoring globals (`/config/solo/*`, not strip params).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Community X-Air OSC table: `/config/solo/dim` + `/config/solo/mute`
//!   int `0..=1` (OFF/ON), `/config/solo/level` float `0.0..=1.0`
//!   (headphone volume), `/config/solo/source` int `0..=14` (OFF, LR,
//!   LRPFL, LRAFL, AUX, U1718, Bus1-6, Bus12, Bus34, Bus56).
//! - X-Air Edit protocol map (X-Air Edit protocol map §7):
//!   `/config/solo ,fifiifiiii` = (level, source=3, …) — flip-verified
//!   2026-10-19: `dim 0→1` moves container pos6 only, `mute 0→1` moves pos8
//!   only, `level` moves pos0; leaf reads match (source reads 3).
//!
//! Owner-rig verified 2026-10-19 (XR18 FW 1.20, state-changing session,
//! restored): dim/mute/level flips round-trip and restore bit-exact.
//! Monitoring-side only — safe to drive + restore. These are GLOBAL driver
//! methods (modelled on [`crate::XAirDriver::get_fx_type`]), not strip
//! params: the catalogue rows are nominally main-scoped and out of strip
//! caps, so the poller never queries them. No engineering-unit mapping is
//! verified for `level`, so the driver carries the wire float verbatim.

use rosc::OscType;

/// Solo dim switch (`/config/solo/dim`, int `1` = dimmed).
pub const SOLO_DIM: &str = "/config/solo/dim";
/// Solo mute switch (`/config/solo/mute`, int `1` = muted).
pub const SOLO_MUTE: &str = "/config/solo/mute";
/// Solo headphone level (`/config/solo/level`, float `0..1`).
pub const SOLO_LEVEL: &str = "/config/solo/level";
/// Solo headphone source (`/config/solo/source`, int `0..=14`).
pub const SOLO_SOURCE: &str = "/config/solo/source";

/// Solo source bounds (community table: OFF, LR, LRPFL, LRAFL, AUX, U1718,
/// Bus1-6, Bus12, Bus34, Bus56; rig reads 3).
pub const SOLO_SOURCE_MIN: u8 = 0;
pub const SOLO_SOURCE_MAX: u8 = 14;

/// Solo level bounds (normalized wire float, unitless — no verified curve).
pub const SOLO_LEVEL_MIN: f32 = 0.0;
pub const SOLO_LEVEL_MAX: f32 = 1.0;

pub fn source_to_arg(v: u8) -> anyhow::Result<OscType> {
    if !(SOLO_SOURCE_MIN..=SOLO_SOURCE_MAX).contains(&v) {
        anyhow::bail!("solo source {v} out of range {SOLO_SOURCE_MIN}..={SOLO_SOURCE_MAX}");
    }
    Ok(OscType::Int(v as i32))
}

pub fn source_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<u8> {
    let i = crate::codec::int_from_msg(
        msg,
        SOLO_SOURCE_MIN as i32,
        SOLO_SOURCE_MAX as i32,
        "solo",
        "source",
    )?;
    Ok(i as u8)
}

/// Identity over `0..1` (no verified engineering curve).
pub fn level_to_wire(v: f32) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(v, SOLO_LEVEL_MIN, SOLO_LEVEL_MAX, "solo", "level")
}

pub fn wire_to_level(w: f32) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(w, SOLO_LEVEL_MIN, SOLO_LEVEL_MAX, "solo", "level")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(SOLO_DIM, "/config/solo/dim");
        assert_eq!(SOLO_MUTE, "/config/solo/mute");
        assert_eq!(SOLO_LEVEL, "/config/solo/level");
        assert_eq!(SOLO_SOURCE, "/config/solo/source");
        assert_eq!((SOLO_SOURCE_MIN, SOLO_SOURCE_MAX), (0, 14));
    }

    #[test]
    fn source_codec_shapes() {
        assert_eq!(source_to_arg(0).unwrap(), OscType::Int(0));
        assert_eq!(source_to_arg(14).unwrap(), OscType::Int(14));
        assert!(source_to_arg(15).is_err());
        let reply = rosc::OscMessage {
            addr: SOLO_SOURCE.into(),
            args: vec![OscType::Int(3)],
        };
        assert_eq!(source_from_msg(&reply).unwrap(), 3);
        let bad = rosc::OscMessage {
            addr: SOLO_SOURCE.into(),
            args: vec![OscType::Int(15)],
        };
        assert!(source_from_msg(&bad).is_err());
    }

    #[test]
    fn level_roundtrips_verbatim() {
        assert_eq!(level_to_wire(0.0).unwrap(), 0.0);
        assert_eq!(level_to_wire(0.75).unwrap(), 0.75);
        assert_eq!(wire_to_level(0.75).unwrap(), 0.75);
        assert!(level_to_wire(1.1).is_err());
        assert!(level_to_wire(f32::NAN).is_err());
        assert!(wire_to_level(1.1).is_err());
    }
}
