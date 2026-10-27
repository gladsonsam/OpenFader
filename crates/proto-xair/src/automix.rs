//! X-Air channel automix (`/ch/NN/automix`, `/ch/NN/automix/weight`).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`):
//!   the channel block carries an `automix/` node with `weight`.
//! - Community X-Air OSC table: `/ch/01/automix/weight` float `0.0..=1.0`.
//! - XR18 protocol notes from desk research (§7):
//!   `automix ,if` = (enable, weight).
//!
//! Owner-rig verified 2026-10-20 (XR18 FW 1.20, ch16 muted/unused,
//! restored): the `/ch/NN/automix` container answers `(int, float)` and
//! the pos0 int IS the enable — writing `(1, weight)` enables automix for
//! the channel, `(0, weight)` disables it (weight preserved, read back
//! bit-exact). There is no standalone `{root}/automix/on` leaf (silent),
//! so the driver reads the container and rewrites it with the current
//! weight to flip the enable without disturbing the mix.

/// Automix weight bounds (normalized wire float, unitless).
pub const AUTOMIX_WEIGHT_MIN: f32 = 0.0;
pub const AUTOMIX_WEIGHT_MAX: f32 = 1.0;

/// `(int, float)` = (enable, weight).
///
/// ```
/// # use openfader_proto_xair::automix::automix;
/// assert_eq!(automix("/ch/01"), "/ch/01/automix");
/// ```
pub fn automix(root: &str) -> String {
    format!("{root}/automix")
}

pub fn automix_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<(bool, f32)> {
    match msg.args.as_slice() {
        [rosc::OscType::Int(on), rosc::OscType::Float(w)] => Ok((*on != 0, wire_to_weight(*w)?)),
        other => anyhow::bail!("{}: automix needs (int, float), got {other:?}", msg.addr),
    }
}

/// Preserve the firmware's OFF / X / Y group, rather than collapsing it to a boolean.
pub fn automix_group_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<(i32, f32)> {
    match msg.args.as_slice() {
        [rosc::OscType::Int(group @ 0..=2), rosc::OscType::Float(w)] => {
            Ok((*group, wire_to_weight(*w)?))
        }
        other => anyhow::bail!(
            "{}: automix needs group 0..2 and weight, got {other:?}",
            msg.addr
        ),
    }
}

pub fn automix_to_args(on: bool, weight: f32) -> anyhow::Result<Vec<rosc::OscType>> {
    Ok(vec![
        rosc::OscType::Int(i32::from(on)),
        rosc::OscType::Float(weight_to_wire(weight)?),
    ])
}
/// ```
/// # use openfader_proto_xair::automix::automix_weight;
/// assert_eq!(automix_weight("/ch/01"), "/ch/01/automix/weight");
/// ```
pub fn automix_weight(root: &str) -> String {
    format!("{root}/automix/weight")
}

/// Identity over `0..1`.
pub fn weight_to_wire(v: f32) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(
        v,
        AUTOMIX_WEIGHT_MIN,
        AUTOMIX_WEIGHT_MAX,
        "automix",
        "weight",
    )
}

pub fn wire_to_weight(w: f32) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(
        w,
        AUTOMIX_WEIGHT_MIN,
        AUTOMIX_WEIGHT_MAX,
        "automix",
        "weight",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(automix_weight("/ch/01"), "/ch/01/automix/weight");
        assert_eq!(automix_weight("/ch/16"), "/ch/16/automix/weight");
    }

    #[test]
    fn weight_roundtrips_verbatim() {
        assert_eq!(weight_to_wire(0.0).unwrap(), 0.0);
        assert_eq!(weight_to_wire(1.0).unwrap(), 1.0);
        assert_eq!(weight_to_wire(0.5).unwrap(), 0.5);
        assert_eq!(wire_to_weight(0.5).unwrap(), 0.5);
        assert!(weight_to_wire(-0.1).is_err());
        assert!(weight_to_wire(1.1).is_err());
        assert!(weight_to_wire(f32::NAN).is_err());
        assert!(wire_to_weight(1.1).is_err());
    }

    #[test]
    fn container_roundtrips_enable_and_weight() {
        use rosc::OscMessage;
        let msg = |on: i32, w: f32| OscMessage {
            addr: "/ch/01/automix".to_string(),
            args: vec![rosc::OscType::Int(on), rosc::OscType::Float(w)],
        };
        assert_eq!(automix_from_msg(&msg(0, 0.5)).unwrap(), (false, 0.5));
        assert_eq!(automix_from_msg(&msg(1, 0.75)).unwrap(), (true, 0.75));
        assert!(automix_from_msg(&msg(1, 1.5)).is_err());
        // Nonzero ints enable (desk truth: the leaf is int-valued).
        assert_eq!(automix_from_msg(&msg(7, 0.5)).unwrap(), (true, 0.5));
        assert_eq!(
            automix_to_args(true, 0.5).unwrap(),
            vec![rosc::OscType::Int(1), rosc::OscType::Float(0.5)]
        );
        assert_eq!(
            automix_to_args(false, 0.75).unwrap(),
            vec![rosc::OscType::Int(0), rosc::OscType::Float(0.75)]
        );
        assert!(automix_to_args(true, 1.5).is_err());
        assert!(automix_to_args(true, f32::NAN).is_err());
    }
}
