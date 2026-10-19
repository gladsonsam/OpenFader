//! Shared X-Air wire codecs: the single implementation of the normalized
//! `0.0..=1.0` wire-float mappings every param block reuses.
//!
//! The desk exposes most engineering values (dB, ms, Hz, %) as normalized
//! floats on the wire; the firmware OSC map tags each leaf `lin(n)` or
//! `log(n)` (see `crates/proto-xair/src/gate.rs` for the gate block,
//! `comp.rs` for dynamics, `eq.rs` for EQ — all three share this math).
//! Int leaves (modes, sources, taps, filter types) travel as `Int` with
//! whole-number-float decode tolerance; plain on/off switches travel as int
//! `1`/`0` (the xair-api-python convention — NOT the inverted `/mix/on`
//! semantics of [`openmix_transport::osc::on_arg`]).
//!
//! Error strings keep the per-block `{tag} {what} …` shape the block
//! modules historically produced, so diagnostics read the same as before.

use rosc::OscType;

/// Map an engineering value to a normalized wire float over `[min, max]`
/// (firmware `lin(n)` leaves). Rejects NaN/infinite and out-of-range values.
pub fn lin_to_wire(v: f32, min: f32, max: f32, tag: &str, what: &str) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("{tag} {what} must be finite, got {v}");
    }
    if !(min..=max).contains(&v) {
        anyhow::bail!("{tag} {what} {v} out of range {min}..={max}");
    }
    Ok((v - min) / (max - min))
}

/// Map a normalized wire float back over `[min, max]` (firmware `lin(n)`).
pub fn wire_to_lin(w: f32, min: f32, max: f32, tag: &str, what: &str) -> anyhow::Result<f32> {
    if !w.is_finite() {
        anyhow::bail!("{tag} {what} wire value must be finite, got {w}");
    }
    if !(0.0..=1.0).contains(&w) {
        anyhow::bail!("{tag} {what} wire value {w} outside 0..=1");
    }
    Ok(min + (max - min) * w)
}

/// Map an engineering value to a normalized wire float over `[min, max]`
/// (firmware `log(n)` leaves: `value = min * exp(log(max/min) * wire)`).
/// Rejects NaN/infinite and out-of-range values.
pub fn log_to_wire(v: f32, min: f32, max: f32, tag: &str, what: &str) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("{tag} {what} must be finite, got {v}");
    }
    if !(min..=max).contains(&v) {
        anyhow::bail!("{tag} {what} {v} out of range {min}..={max}");
    }
    Ok((v / min).ln() / (max / min).ln())
}

/// Map a normalized wire float back over `[min, max]` (firmware `log(n)`).
pub fn wire_to_log(w: f32, min: f32, max: f32, tag: &str, what: &str) -> anyhow::Result<f32> {
    if !w.is_finite() {
        anyhow::bail!("{tag} {what} wire value must be finite, got {w}");
    }
    if !(0.0..=1.0).contains(&w) {
        anyhow::bail!("{tag} {what} wire value {w} outside 0..=1");
    }
    Ok(min * (w * (max / min).ln()).exp())
}

/// Encode an int leaf (modes, sources, taps, filter types, enums):
/// whole-number floats in `min..=max` become the wire int.
pub fn int_to_arg(v: f32, min: i32, max: i32, tag: &str, what: &str) -> anyhow::Result<OscType> {
    if !v.is_finite() || v.fract() != 0.0 {
        anyhow::bail!("{tag} {what} must be a whole number, got {v}");
    }
    let i = v as i32;
    if !(min..=max).contains(&i) {
        anyhow::bail!("{tag} {what} {i} out of range {min}..={max}");
    }
    Ok(OscType::Int(i))
}

/// Decode an int leaf reply (int kept, whole-number float accepted).
pub fn int_from_msg(
    msg: &rosc::OscMessage,
    min: i32,
    max: i32,
    tag: &str,
    what: &str,
) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(OscType::Int(i)) => {
            if !(min..=max).contains(i) {
                anyhow::bail!("{}: {tag} {what} {i} outside {min}..={max}", msg.addr);
            }
            Ok(*i)
        }
        Some(OscType::Float(f)) => {
            if !f.is_finite() || f.fract() != 0.0 {
                anyhow::bail!("{}: {tag} {what} must be a whole number, got {f}", msg.addr);
            }
            let i = *f as i32;
            if !(min..=max).contains(&i) {
                anyhow::bail!("{}: {tag} {what} {i} outside {min}..={max}", msg.addr);
            }
            Ok(i)
        }
        other => anyhow::bail!("{}: {tag} {what} needs an int arg, got {other:?}", msg.addr),
    }
}

/// Encode a plain (non-inverted) on/off switch the way xair-api writes it:
/// int `1`/`0`. (Unlike `/mix/on`, these are NOT inverted —
/// [`openmix_transport::osc::on_arg`] must not be used here.)
/// Decode with [`openmix_transport::osc::is_on`] (`wire == 1` means on).
pub fn bool_arg(on: bool) -> OscType {
    OscType::Int(if on { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lin_roundtrips_with_tagged_errors() {
        assert_eq!(lin_to_wire(-80.0, -80.0, 0.0, "gate", "thr").unwrap(), 0.0);
        assert_eq!(lin_to_wire(0.0, -80.0, 0.0, "gate", "thr").unwrap(), 1.0);
        let back = wire_to_lin(0.375, -80.0, 0.0, "gate", "thr").unwrap();
        assert!((back + 50.0).abs() < 1e-5, "{back}");
        let err = lin_to_wire(f32::NAN, -80.0, 0.0, "gate", "thr")
            .unwrap_err()
            .to_string();
        assert!(err.contains("gate thr must be finite"), "{err}");
        let err = lin_to_wire(1.0, -80.0, 0.0, "comp", "thr")
            .unwrap_err()
            .to_string();
        assert!(err.contains("comp thr 1 out of range -80..=0"), "{err}");
        assert!(wire_to_lin(1.1, -80.0, 0.0, "gate", "thr").is_err());
    }

    #[test]
    fn log_roundtrips_with_tagged_errors() {
        assert_eq!(log_to_wire(0.02, 0.02, 2000.0, "gate", "hold").unwrap(), 0.0);
        assert_eq!(log_to_wire(2000.0, 0.02, 2000.0, "gate", "hold").unwrap(), 1.0);
        assert_eq!(wire_to_log(0.0, 0.02, 2000.0, "gate", "hold").unwrap(), 0.02);
        // Live rig wire 0.76 -> ~126 ms.
        let live = wire_to_log(0.76, 0.02, 2000.0, "gate", "hold").unwrap();
        assert!((live - 126.0).abs() < 3.0, "live hold {live}");
        assert!(log_to_wire(0.019, 0.02, 2000.0, "gate", "hold").is_err());
        assert!(wire_to_log(1.1, 0.02, 2000.0, "gate", "hold").is_err());
    }

    #[test]
    fn int_codec_with_tagged_errors() {
        assert_eq!(int_to_arg(4.0, 0, 4, "gate", "mode").unwrap(), OscType::Int(4));
        assert!(int_to_arg(5.0, 0, 4, "gate", "mode").is_err());
        assert!(int_to_arg(1.5, 0, 4, "gate", "mode").is_err());
        let reply = rosc::OscMessage {
            addr: "/ch/01/gate/mode".into(),
            args: vec![OscType::Int(4)],
        };
        assert_eq!(int_from_msg(&reply, 0, 4, "gate", "mode").unwrap(), 4);
        let bad = rosc::OscMessage {
            addr: "/ch/01/gate/mode".into(),
            args: vec![OscType::Float(1.5)],
        };
        assert!(int_from_msg(&bad, 0, 4, "gate", "mode").is_err());
    }

    #[test]
    fn bool_codec_shapes() {
        assert_eq!(bool_arg(true), OscType::Int(1));
        assert_eq!(bool_arg(false), OscType::Int(0));
    }
}
