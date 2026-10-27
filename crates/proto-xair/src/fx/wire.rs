//! FX addresses, wire validation, and argument decoding.
use super::*;
pub fn check_slot(slot: u8) -> anyhow::Result<u8> {
    openfader_core::fx::validate_slot(slot)
}

pub fn check_index(index: u8) -> anyhow::Result<u8> {
    openfader_core::fx::validate_param_index(index)
}

pub fn check_value(value: f32) -> anyhow::Result<f32> {
    openfader_core::fx::validate_param_value(value)
}

/// Slot is the 1-based wire number (`1..=4`, same as the trait hooks).
///
/// ```
/// # use openfader_proto_xair::fx::fx_param;
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

/// Carries the loaded algorithm as an int.
///
/// ```
/// # use openfader_proto_xair::fx::fx_type;
/// assert_eq!(fx_type(1).unwrap(), "/fx/1/type");
/// assert_eq!(fx_type(4).unwrap(), "/fx/4/type");
/// ```
pub fn fx_type(slot: u8) -> anyhow::Result<String> {
    check_slot(slot)?;
    Ok(format!("/fx/{slot}/type"))
}

/// OSC address of the FX slot insert switch (`/fx/N/insert`, int `1`/`0`).
///
/// Doc sources: the community X-Air OSC table gives `/fx/1/insert` int
/// `0..=1` (OFF/ON). Owner-rig verified 2026-10-19 (XR18 FW 1.20):
/// `/fx/4/insert` = 1 matches the `/fx/4 ,ii` = (type, insert) container,
/// slots 1–3 read 0.
///
/// ```
/// # use openfader_proto_xair::fx::fx_insert;
/// assert_eq!(fx_insert(1).unwrap(), "/fx/1/insert");
/// assert_eq!(fx_insert(4).unwrap(), "/fx/4/insert");
/// ```
pub fn fx_insert(slot: u8) -> anyhow::Result<String> {
    check_slot(slot)?;
    Ok(format!("/fx/{slot}/insert"))
}

/// Validated before any I/O so out-of-range never hits the desk.
pub fn fx_type_to_arg(effect: u8) -> anyhow::Result<rosc::OscType> {
    if !(FX_TYPE_MIN..=FX_TYPE_MAX).contains(&effect) {
        anyhow::bail!("fx type {effect} out of range {FX_TYPE_MIN}..={FX_TYPE_MAX}");
    }
    Ok(rosc::OscType::Int(effect as i32))
}

/// Re-exported for loops.
pub use openfader_core::fx::FX_PARAM_COUNT as PARAM_COUNT;
/// Lowest/highest slot, re-exported for driver bounds checks.
pub use openfader_core::fx::FX_SLOT_MAX as SLOT_MAX;
pub use openfader_core::fx::FX_SLOT_MIN as SLOT_MIN;

pub const FX_TYPE_MIN: u8 = 0;
/// 61 algorithms, `0..=60`.
pub const FX_TYPE_MAX: u8 = 60;

/// Parse a `/fx/N/type` reply into an effect index. Accepts the documented
/// int form and a normalized-float form (integral values only); anything
/// else bails — no guessing at the loaded effect.
pub fn parse_fx_type(args: &[OscType]) -> anyhow::Result<u8> {
    let index: i64 = match args.first() {
        Some(OscType::Int(i)) => *i as i64,
        Some(OscType::Long(l)) => *l,
        Some(OscType::Float(f)) => {
            if !f.is_finite() {
                anyhow::bail!("fx type must be finite, got {f}");
            }
            if (f.round() - f).abs() > 1e-6 {
                anyhow::bail!("fx type float {f} is not an integral effect index");
            }
            f.round() as i64
        }
        Some(OscType::Double(d)) => {
            if !d.is_finite() {
                anyhow::bail!("fx type must be finite, got {d}");
            }
            if (d.round() - d).abs() > 1e-9 {
                anyhow::bail!("fx type float {d} is not an integral effect index");
            }
            d.round() as i64
        }
        other => anyhow::bail!("fx type needs an int arg, got {other:?}"),
    };
    if !(FX_TYPE_MIN as i64..=FX_TYPE_MAX as i64).contains(&index) {
        anyhow::bail!("fx type {index} out of range 0..=60");
    }
    Ok(index as u8)
}

// ---------------------------------------------------------------------------
// Wire value codec (hardware-proven 2026-10-18, baseline "Live FX par shapes").
// ---------------------------------------------------------------------------

/// Decode one `/fx/N/par/MM` reply into normalized `0..1`.
///
/// `def` is the effect's own [`fx_param_def`] row for this par (`None` for
/// padding/unknown — decoded raw, still validated). Hardware facts (XR18
/// FW 1.20):
///
/// * Enum/switch pars reply as INT: MODD par 02/08/09 = `0`/`1`/`2`, VRM
///   par 13 (Freeze) = `0`. An int on an enum def is the option index,
///   mapped onto `0..1` steps (`index / (options - 1)`, so OFF/ON lands
///   exactly on `0.0`/`1.0`); an out-of-range index bails — no guessing.
///   An int on a continuous (or unknown) def is the raw value (`1` →
///   `1.0`), still range-checked.
/// * Pars past the effect's own count reply as float NaN (VRM/HALL 14+,
///   MODD 14+, PIT 7+). NaN means absent: this bails (like an unknown
///   par) and never surfaces NaN as a parameter value. Other non-finite
///   floats bail the same way; anything non-numeric bails too.
///
/// ```
/// # use openfader_proto_xair::fx::{fx_param_def, parse_fx_param};
/// # use rosc::OscType;
/// // VRM par 13 is Freeze (OFF/ON): int 0 → 0.0.
/// let def = fx_param_def(7, 13);
/// assert_eq!(parse_fx_param(&[OscType::Int(0)], def.as_ref()).unwrap(), 0.0);
/// // Absent pad par replies NaN → bail, never NaN.
/// assert!(parse_fx_param(&[OscType::Float(f32::NAN)], def.as_ref()).is_err());
/// ```
pub fn parse_fx_param(args: &[OscType], def: Option<&FxParamDef>) -> anyhow::Result<f32> {
    match args.first() {
        Some(OscType::Int(i)) => decode_fx_int(*i as i64, def),
        Some(OscType::Long(l)) => decode_fx_int(*l, def),
        Some(OscType::Float(f)) => {
            if !f.is_finite() {
                anyhow::bail!("fx par absent (non-finite wire float {f})");
            }
            check_value(*f)
        }
        Some(OscType::Double(d)) => {
            if !d.is_finite() {
                anyhow::bail!("fx par absent (non-finite wire double {d})");
            }
            check_value(*d as f32)
        }
        other => anyhow::bail!("fx par needs an int/float arg, got {other:?}"),
    }
}

/// Int arm of [`parse_fx_param`]: option index for enum defs, raw int otherwise.
fn decode_fx_int(i: i64, def: Option<&FxParamDef>) -> anyhow::Result<f32> {
    match def.and_then(|d| d.options) {
        Some(options) => {
            let n = options.len() as i64;
            if n < 2 {
                anyhow::bail!("fx enum par has {n} options, cannot map int {i}");
            }
            if !(0..n).contains(&i) {
                anyhow::bail!("fx enum index {i} out of range 0..{n}");
            }
            Ok(i as f32 / (n - 1) as f32)
        }
        None => check_value(i as f32),
    }
}
