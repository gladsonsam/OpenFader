//! X-Air FX slot address mapping (`/fx/N/par/MM`) + effect-type label table.
//!
//! The desk exposes 4 stereo FX slots, each a flat array of 64 numbered
//! float params. The wire is 1-based on both axes with a zero-padded
//! two-digit param number: slot `N` in `1..=4`, param `MM` in `01..=64`
//! (hardware-verified read of `/fx/1/par/01` → `,f 0.1`, same-value
//! roundtrip write `0.5` → `0.5` bit-exact — baseline §6 row 5).
//!
//! [`openfader_core::fx`] owns the shape and validation (0-based slot/index
//! at the trait level); this module adds 1 on the wire and owns the
//! per-effect-type label table below. FX params round-trip bit-exact on
//! hardware (unlike faders, no ~10-bit quantization — baseline T8), so
//! readback asserts use exact equality.
//!
//! ## Effect-type table
//!
//! `/fx/N/type` carries the loaded algorithm as an int `0..=60`
//! ([`fx_type`], [`parse_fx_type`]). Index → short display name follows the
//! X-Air cheat-sheet / Maillot doc order ([`fx_effect_name`]); per-effect
//! par labels, units, display ranges and enum options come from
//! `research_notes/FX param labels/fx-labels.md` (61 effect types, par
//! labels/ranges/enums with confidence flags).
//!
//! Table rules, all deliberate:
//!
//! * Labels are the X-Air display names (first name where the notes list an
//!   `X-Air/X32` pair, e.g. `ModSpin` for `ModSpin/Spin`). Ranges are
//!   display strings only — the notes do not pin down every lin/log fader
//!   law, so this module never maps `0..1` to engineering units.
//! * `None` from [`fx_param_def`] means padding (pars above the effect's own
//!   count carry no meaning) or an unknown effect index — the UI hides
//!   padding and falls back to generic numbered sliders for unknown types.
//! * Rows the notes flag as `inferred` (contested order or range conflicts
//!   (CHAM par 16, GATE Density range, PIT Delay range, all of PIT2/P1A/
//!   P1A2/ENH2/FAC1M, ENH Spread+MidQ names, AMP Level/Low/High order, FAC
//!   InGain range, SON Process scale) carry `inferred: true`. Index 47 is
//!   additionally contested between parts of the notes (wire-order grid
//!   says ULC2, one section header says ULC), so all of its rows are
//!   forced inferred too. The UI must
//!   render those with a `?` suffix plus a header note, never assert them.
//! * Index 15 has no documented effect (the wire order jumps FLNG 14 →
//!   PHAS 16); [`fx_effect_name`] returns `None` there so it exercises the
//!   same generic fallback as any out-of-range index.
//! * Enum/switch pars reply as INT on hardware (D/FL par 02/08/09 =
//!   `0`/`1`/`1`, VRM par 13 = `0`; baseline "Live FX par shapes") while
//!   continuous pars reply as float — so [`parse_fx_param`] maps int wire
//!   values through the effect's own def (option index onto `0..1` steps
//!   for enums, raw int for continuous). Pars past the effect's own count
//!   reply as float NaN (VRM/HALL 14+, D/FL 14+, PIT 7+) — NaN means
//!   absent and [`parse_fx_param`] bails on it like an unknown par,
//!   never surfacing NaN as a value.
//!
//! Mixer UI field naming: `fx{slot}_par{MM}` with the
//! same 1-based conventions, e.g. `fx1_par01` = slot 1, param index 0.
//! Passed through the existing `param` command as
//! `backend.setParam(slot - 1, 'fx{slot}_par{MM}', value01)` — the field
//! is authoritative, `ch` carries the 0-based slot so a future relay can
//! route without parsing the field.
use openfader_core::fx::{FX_PARAM_COUNT, FX_SLOT_MAX, FX_SLOT_MIN};
use rosc::OscType;

mod catalogue;
mod definitions;
mod delay_modulation;
mod dynamics_equalization;
mod enhancement;
mod reverb;
mod tap;
mod wire;

pub use catalogue::{fx_effect_full_name, fx_effect_name, fx_param_count, fx_param_def};
use definitions::*;
pub use definitions::{FxParamDef, GEQ_BANDS};
use delay_modulation::*;
use dynamics_equalization::*;
use enhancement::*;
use reverb::*;
pub use tap::*;
pub use wire::*;

#[cfg(test)]
mod tests;
