//! X-Air FX slot addressing and effect parameter definitions.
//! Slots and parameters are 1-based on the wire and 0-based in the engine.
//! Enums use integer indices; continuous parameters use normalized floats.
//! Unknown or padding parameters stay unavailable; inferred labels are marked.

use openfader_core::fx::{FX_PARAM_COUNT, FX_SLOT_MAX, FX_SLOT_MIN};
use rosc::OscType;

mod catalogue;
mod definitions;
mod delay_modulation;
mod display;
mod dynamics_equalization;
mod enhancement;
mod reverb;
mod tap;
mod wire;

pub use catalogue::{fx_effect_full_name, fx_effect_name, fx_param_count, fx_param_def};
use definitions::*;
pub use definitions::{FxParamDef, GEQ_BANDS};
use delay_modulation::*;
pub use display::{fx_position_def, FxPositionDef};
use dynamics_equalization::*;
use enhancement::*;
use reverb::*;
pub use tap::*;
pub use wire::*;

#[cfg(test)]
mod tests;
