//! X-Air FX slot address mapping (`/fx/N/par/MM`).
//!
//! The desk exposes 4 stereo FX slots, each a flat array of 64 numbered
//! float params. The wire is 1-based on both axes with a zero-padded
//! two-digit param number: slot `N` in `1..=4`, param `MM` in `01..=64`
//! (hardware-verified read of `/fx/1/par/01` → `,f 0.1`, same-value
//! roundtrip write `0.5` → `0.5` bit-exact — baseline §6 row 5).
//!
//! [`openmix_core::fx`] owns the shape and validation (0-based slot/index
//! at the trait level); this module only adds 1 and formats the address.
//! FX params round-trip bit-exact on hardware (unlike faders, no ~10-bit
//! quantization — baseline T8), so readback asserts use exact equality.
//!
//! Web UI field naming (see `FxRack.svelte`): `fx{slot}_par{MM}` with the
//! same 1-based conventions, e.g. `fx1_par01` = slot 1, param index 0.
//! Passed through the existing `param` command as
//! `backend.setParam(slot - 1, 'fx{slot}_par{MM}', value01)` — the field
//! is authoritative, `ch` carries the 0-based slot so a future relay can
//! route without parsing the field.
use openmix_core::fx::{FX_PARAM_COUNT, FX_SLOT_MAX, FX_SLOT_MIN};

/// Validate a 1-based wire slot number (`1..=4`).
pub fn check_slot(slot: u8) -> anyhow::Result<u8> {
    openmix_core::fx::validate_slot(slot)
}

/// Validate a 0-based param index (`0..64`).
pub fn check_index(index: u8) -> anyhow::Result<u8> {
    openmix_core::fx::validate_param_index(index)
}

/// Validate a normalized param value (`0.0..=1.0`, finite).
pub fn check_value(value: f32) -> anyhow::Result<f32> {
    openmix_core::fx::validate_param_value(value)
}

/// OSC address for one FX param: 0-based `index` maps to 1-based `MM`.
/// Slot is the 1-based wire number (`1..=4`, same as the trait hooks).
///
/// ```
/// # use openmix_proto_xair::fx::fx_param;
/// assert_eq!(fx_param(1, 0).unwrap(), "/fx/1/par/01");
/// assert_eq!(fx_param(4, 63).unwrap(), "/fx/4/par/64");
/// ```
pub fn fx_param(slot: u8, index: u8) -> anyhow::Result<String> {
    check_slot(slot)?;
    check_index(index)?;
    Ok(format!("/fx/{slot}/par/{:02}", index + 1))
}

/// Web UI field for one FX param, e.g. `fx1_par01` (see module docs).
pub fn fx_field(slot: u8, index: u8) -> anyhow::Result<String> {
    check_slot(slot)?;
    check_index(index)?;
    Ok(format!("fx{slot}_par{:02}", index + 1))
}

/// Lowest/highest slot, re-exported for driver bounds checks.
pub use openmix_core::fx::FX_SLOT_MAX as SLOT_MAX;
pub use openmix_core::fx::FX_SLOT_MIN as SLOT_MIN;
/// Params per slot (0-based `0..FX_PARAM_COUNT`), re-exported for loops.
pub use openmix_core::fx::FX_PARAM_COUNT as PARAM_COUNT;

/// Assert the module stays in lockstep with the core shape (4 slots × 64).
pub(crate) fn _shape_check() {
    const _: () = assert!(FX_SLOT_MIN == 1 && FX_SLOT_MAX == 4);
    const _: () = assert!(FX_PARAM_COUNT == 64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(fx_param(1, 0).unwrap(), "/fx/1/par/01");
        assert_eq!(fx_param(1, 9).unwrap(), "/fx/1/par/10");
        assert_eq!(fx_param(4, 63).unwrap(), "/fx/4/par/64");
        assert_eq!(fx_field(1, 0).unwrap(), "fx1_par01");
        assert_eq!(fx_field(4, 63).unwrap(), "fx4_par64");
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(fx_param(0, 0).is_err());
        assert!(fx_param(5, 0).is_err());
        assert!(fx_param(1, 64).is_err());
        assert!(fx_field(0, 0).is_err());
        assert!(check_value(f32::NAN).is_err());
        assert!(check_value(1.5).is_err());
        assert_eq!(check_value(0.5).unwrap(), 0.5);
    }
}
