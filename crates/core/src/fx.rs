//! FX framework: generic numbered-param slots (X-Air-style), types + validation.
//!
//! Model: 4 FX slots ([`FX_SLOT_MIN`]..=[`FX_SLOT_MAX`]), each a flat array
//! of [`FX_PARAM_COUNT`] normalized params (0-based here; drivers add 1 on
//! 1-based wires, the same rule as [`StripId`](crate::StripId)).
//!
//! This crate only owns the shape and the validation. Per-mixer address
//! mapping (X-Air `/fx/N/par/MM`, DL32S equivalents) comes later in the
//! driver crates; [`MixerDriver`](crate::MixerDriver) already exposes
//! defaulted `set_fx_param`/`get_fx_param` hooks for that.

/// FX slot count (X-Air: 4 stereo slots).
pub const FX_SLOT_COUNT: usize = 4;
/// First slot number (wire convention is 1-based).
pub const FX_SLOT_MIN: u8 = 1;
/// Last slot number.
pub const FX_SLOT_MAX: u8 = 4;
/// Params per slot, 0-based indices `0..FX_PARAM_COUNT` (wire: +1).
pub const FX_PARAM_COUNT: usize = 64;

/// Validated FX slot number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxSlot(u8);

impl FxSlot {
    pub fn new(n: u8) -> anyhow::Result<Self> {
        validate_slot(n)?;
        Ok(Self(n))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

/// Validated (slot, param-index) address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxParamAddr {
    pub slot: FxSlot,
    pub index: u8,
}

impl FxParamAddr {
    pub fn new(slot: u8, index: u8) -> anyhow::Result<Self> {
        Ok(Self {
            slot: FxSlot::new(slot)?,
            index: validate_param_index(index)?,
        })
    }
}

/// Slot number must be 1..=4.
pub fn validate_slot(slot: u8) -> anyhow::Result<u8> {
    if !(FX_SLOT_MIN..=FX_SLOT_MAX).contains(&slot) {
        anyhow::bail!("fx slot {slot} out of range {FX_SLOT_MIN}..={FX_SLOT_MAX}");
    }
    Ok(slot)
}

/// Param index must be 0..64.
pub fn validate_param_index(index: u8) -> anyhow::Result<u8> {
    if index as usize >= FX_PARAM_COUNT {
        anyhow::bail!("fx param index {index} out of range 0..{FX_PARAM_COUNT}");
    }
    Ok(index)
}

/// Param values are normalized 0.0..=1.0.
pub fn validate_param_value(value: f32) -> anyhow::Result<f32> {
    if !value.is_finite() {
        anyhow::bail!("fx param must be finite, got {value}");
    }
    if !(0.0..=1.0).contains(&value) {
        anyhow::bail!("fx param {value} out of range 0.0..=1.0");
    }
    Ok(value)
}

/// In-memory rack: 4 × 64 normalized params. Drivers persist this to the
/// mixer once address mapping lands.
#[derive(Clone, Debug)]
pub struct FxRack {
    values: [[f32; FX_PARAM_COUNT]; FX_SLOT_COUNT],
}

impl FxRack {
    pub fn new() -> Self {
        Self {
            values: [[0.0; FX_PARAM_COUNT]; FX_SLOT_COUNT],
        }
    }

    pub fn set(&mut self, slot: u8, index: u8, value: f32) -> anyhow::Result<()> {
        let slot = validate_slot(slot)?;
        let index = validate_param_index(index)?;
        let value = validate_param_value(value)?;
        self.values[(slot - FX_SLOT_MIN) as usize][index as usize] = value;
        Ok(())
    }

    pub fn get(&self, slot: u8, index: u8) -> anyhow::Result<f32> {
        let slot = validate_slot(slot)?;
        let index = validate_param_index(index)?;
        Ok(self.values[(slot - FX_SLOT_MIN) as usize][index as usize])
    }
}

impl Default for FxRack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip() {
        let mut rack = FxRack::new();
        rack.set(1, 0, 0.75).unwrap();
        rack.set(4, 63, 1.0).unwrap();
        assert_eq!(rack.get(1, 0).unwrap(), 0.75);
        assert_eq!(rack.get(4, 63).unwrap(), 1.0);
        // Untouched params read back zero.
        assert_eq!(rack.get(2, 10).unwrap(), 0.0);
    }

    #[test]
    fn bad_slot_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(0, 0, 0.5).is_err());
        assert!(rack.set(5, 0, 0.5).is_err());
        assert!(rack.get(0, 0).is_err());
        assert!(FxSlot::new(4).is_ok());
    }

    #[test]
    fn bad_index_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(1, 64, 0.5).is_err());
        assert!(rack.get(1, 255).is_err());
        assert!(FxParamAddr::new(2, 63).is_ok());
        assert!(FxParamAddr::new(2, 64).is_err());
    }

    #[test]
    fn bad_value_rejected() {
        let mut rack = FxRack::new();
        assert!(rack.set(1, 0, f32::NAN).is_err());
        assert!(rack.set(1, 0, -0.1).is_err());
        assert!(rack.set(1, 0, 1.1).is_err());
    }
}
