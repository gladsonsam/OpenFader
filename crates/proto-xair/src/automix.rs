//! X-Air channel automix weight (`/ch/NN/automix/weight`, ch strips only).
//!
//! Doc sources (address shapes + wire codecs):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`):
//!   the channel block carries an `automix/` node with `weight`.
//! - Community X-Air OSC table: `/ch/01/automix/weight` float `0.0..=1.0`.
//! - X-Air Edit protocol map (`docs/xair-edit-protocol-map.md` §7):
//!   `automix ,if` = (?, weight) — the weight leaf is verified, the pos0
//!   int has no desk leaf.
//!
//! Owner-rig verified 2026-10-19 (XR18 FW 1.20): `/ch/01/automix/weight`
//! reads `0.5`. No engineering-unit mapping is verified (the community
//! table prints `-24..+24` beside the float without a curve tag), so the
//! driver carries the normalized wire float `0..1` verbatim and the
//! catalogue row is unitless — no invented dB law. The automix on-leaf has
//! no desk leaf (pos0 residue), so there is no `automix_on` id.

/// Automix weight bounds (normalized wire float, unitless).
pub const AUTOMIX_WEIGHT_MIN: f32 = 0.0;
/// Automix weight upper bound (see [`AUTOMIX_WEIGHT_MIN`]).
pub const AUTOMIX_WEIGHT_MAX: f32 = 1.0;

/// OSC address of one channel automix weight (`automix/weight`).
///
/// ```
/// # use openmix_proto_xair::automix::automix_weight;
/// assert_eq!(automix_weight("/ch/01"), "/ch/01/automix/weight");
/// ```
pub fn automix_weight(root: &str) -> String {
    format!("{root}/automix/weight")
}

/// Map a weight to the normalized wire float (identity over `0..1`;
/// rejects NaN/infinite and out-of-range values before anything is sent).
pub fn weight_to_wire(v: f32) -> anyhow::Result<f32> {
    crate::codec::lin_to_wire(v, AUTOMIX_WEIGHT_MIN, AUTOMIX_WEIGHT_MAX, "automix", "weight")
}

/// Map a normalized wire float back to the weight.
pub fn wire_to_weight(w: f32) -> anyhow::Result<f32> {
    crate::codec::wire_to_lin(w, AUTOMIX_WEIGHT_MIN, AUTOMIX_WEIGHT_MAX, "automix", "weight")
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
}
