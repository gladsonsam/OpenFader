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

/// Validate a 1-based wire slot number (`1..=4`).
pub fn check_slot(slot: u8) -> anyhow::Result<u8> {
    openfader_core::fx::validate_slot(slot)
}

/// Validate a 0-based param index (`0..64`).
pub fn check_index(index: u8) -> anyhow::Result<u8> {
    openfader_core::fx::validate_param_index(index)
}

/// Validate a normalized param value (`0.0..=1.0`, finite).
pub fn check_value(value: f32) -> anyhow::Result<f32> {
    openfader_core::fx::validate_param_value(value)
}

/// OSC address for one FX param: 0-based `index` maps to 1-based `MM`.
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

/// OSC address of the effect-type selector for one slot: `/fx/N/type`
/// carries the loaded algorithm as an int `0..=60` (see module docs).
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

/// Encode an effect-type index (`0..=60`, [`FX_TYPE_MIN`]..=[`FX_TYPE_MAX`])
/// to the wire int. Validated BEFORE any I/O so out-of-range never hits
/// the desk.
pub fn fx_type_to_arg(effect: u8) -> anyhow::Result<rosc::OscType> {
    if !(FX_TYPE_MIN..=FX_TYPE_MAX).contains(&effect) {
        anyhow::bail!("fx type {effect} out of range {FX_TYPE_MIN}..={FX_TYPE_MAX}");
    }
    Ok(rosc::OscType::Int(effect as i32))
}

/// Lowest/highest slot, re-exported for driver bounds checks.
pub use openfader_core::fx::FX_SLOT_MAX as SLOT_MAX;
pub use openfader_core::fx::FX_SLOT_MIN as SLOT_MIN;
/// Params per slot (0-based `0..FX_PARAM_COUNT`), re-exported for loops.
pub use openfader_core::fx::FX_PARAM_COUNT as PARAM_COUNT;

/// Lowest effect-type index on `/fx/N/type`.
pub const FX_TYPE_MIN: u8 = 0;
/// Highest effect-type index on `/fx/N/type` (61 algorithms, `0..=60`).
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
/// * Enum/switch pars reply as INT: D/FL par 02/08/09 = `0`/`1`/`1`, VRM
///   par 13 (Freeze) = `0`. An int on an enum def is the option index,
///   mapped onto `0..1` steps (`index / (options - 1)`, so OFF/ON lands
///   exactly on `0.0`/`1.0`); an out-of-range index bails — no guessing.
///   An int on a continuous (or unknown) def is the raw value (`1` →
///   `1.0`), still range-checked.
/// * Pars past the effect's own count reply as float NaN (VRM/HALL 14+,
///   D/FL 14+, PIT 7+). NaN means absent: this bails (like an unknown
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

// ---------------------------------------------------------------------------
// Effect-type label table (research_notes/FX param labels/fx-labels.md).
// ---------------------------------------------------------------------------

/// One labeled FX param: 1-based `par`, X-Air display `label`, display
/// `unit` (`""` when unitless), display range `min`/`max` (`""` for enums),
/// discrete `options` (`Some` = render a select, not a slider),
/// `inferred` (low-confidence row — UI renders `?` + header note), and
/// `bank` (`Some("A"/"B")` for dual-block effects, `None` otherwise).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxParamDef {
    pub par: u8,
    pub label: &'static str,
    pub unit: &'static str,
    pub min: &'static str,
    pub max: &'static str,
    pub options: Option<&'static [&'static str]>,
    pub inferred: bool,
    pub bank: Option<&'static str>,
}

/// Documented continuous param.
const fn p(
    par: u8,
    label: &'static str,
    unit: &'static str,
    min: &'static str,
    max: &'static str,
) -> FxParamDef {
    FxParamDef {
        par,
        label,
        unit,
        min,
        max,
        options: None,
        inferred: false,
        bank: None,
    }
}

/// Low-confidence continuous param (notes flag an order/range conflict).
const fn q(
    par: u8,
    label: &'static str,
    unit: &'static str,
    min: &'static str,
    max: &'static str,
) -> FxParamDef {
    FxParamDef {
        par,
        label,
        unit,
        min,
        max,
        options: None,
        inferred: true,
        bank: None,
    }
}

/// Documented enum param (discrete option list).
const fn e(par: u8, label: &'static str, options: &'static [&'static str]) -> FxParamDef {
    FxParamDef {
        par,
        label,
        unit: "",
        min: "",
        max: "",
        options: Some(options),
        inferred: false,
        bank: None,
    }
}

/// Low-confidence enum param (whole effect or row flagged in the notes).
const fn ie(par: u8, label: &'static str, options: &'static [&'static str]) -> FxParamDef {
    FxParamDef {
        par,
        label,
        unit: "",
        min: "",
        max: "",
        options: Some(options),
        inferred: true,
        bank: None,
    }
}

const OFF_ON: &[&str] = &["OFF", "ON"];
const ZERO_ONE: &[&str] = &["0", "1"];
/// Stereo-delay note divisions (DLY FactorL/R, 3TAP/4TAP FactorN).
const NOTE_DIV9: &[&str] = &["1/4", "3/8", "1/2", "2/3", "1", "4/3", "3/2", "2", "3"];
/// Combination-effect delay patterns (D/RV, D/CR, D/FL par 02).
const PATTERN14: &[&str] = &[
    "1/4", "1/3", "3/8", "1/2", "2/3", "3/4", "1", "1/4X", "1/3X", "3/8X", "1/2X", "2/3X",
    "3/4X", "1X",
];
const MODD_FACTOR: &[&str] = &["1", "1/2", "2/3", "3/2"];
const MODD_SETUP: &[&str] = &["PAR", "SER"];
const MODD_RTYPE: &[&str] = &["AMB", "CLUB", "HALL"];
const VINTAGE_OUT: &[&str] = &["FRONT", "REAR"];
const DLY_MODE: &[&str] = &["ST", "X", "M"];
const FILT_MODE: &[&str] = &["LP", "HP", "BP", "NO"];
const FILT_WAVE: &[&str] = &["TRI", "SIN", "SAW", "SAW-", "RMP", "SQU", "RND"];
const SUB_RANGE: &[&str] = &["LO", "MID", "HI"];
const P1A_LOFREQ: &[&str] = &["0", "30", "60", "100"];
const P1A_HIFREQ: &[&str] = &["3k", "4k", "5k", "8k", "10k", "12k", "16k"];
const P1A_ATTFR: &[&str] = &["5k", "10k", "20k"];
const PQ5_LOFREQ: &[&str] = &["200", "300", "500", "700", "1000"];
const PQ5_MIDFREQ: &[&str] = &[
    "200", "300", "500", "700", "1k", "1k5", "2k", "3k", "4k", "5k", "7k",
];
const PQ5_HIFREQ: &[&str] = &["1k5", "2k", "3k", "4k", "5k"];
const ULC_RATIO: &[&str] = &["4", "8", "12", "20", "ALL"];
const CMB_SOLO: &[&str] = &["OFF", "Bd1", "Bd2", "Bd3", "Bd4", "Bd5"];
const CMB_SLOPE: &[&str] = &["12", "48"];
const CMB_RATIO: &[&str] = &[
    "1.1", "1.2", "1.3", "1.5", "1.7", "2", "2.5", "3", "3.5", "4", "5", "7", "10", "LIM",
];
const CMB_METER: &[&str] = &["GR", "SBC", "PEAK"];
const LEC_MODE: &[&str] = &["COMP", "LIM"];
const DES_VOICE: &[&str] = &["FEM", "MALE"];
const DES_MODE: &[&str] = &["ST", "M-S"];
const DIMC_MODE: &[&str] = &["M", "ST"];

/// Graphic-EQ band centres, shared by GEQ/TEQ (+dual): 20 Hz … 20 kHz.
pub const GEQ_BANDS: [&str; 31] = [
    "20", "25", "31.5", "40", "50", "63", "80", "100", "125", "160", "200", "250", "315",
    "400", "500", "630", "800", "1k", "1k25", "1k6", "2k", "2k5", "3k15", "4k", "5k",
    "6k3", "8k", "10k", "12k5", "16k", "20k",
];

static HALL: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.2", "5"),
    p(3, "Size", "", "2", "100"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "", "1", "30"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "BassMult", "", "0.5", "2"),
    p(10, "Spread", "", "0", "50"),
    p(11, "Shape", "", "0", "250"),
    p(12, "ModSpeed", "", "0", "100"),
];

static AMBI: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.2", "7.3"),
    p(3, "Size", "", "2", "100"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "", "1", "30"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "Modulate", "%", "0", "100"),
    p(10, "TailGain", "%", "0", "100"),
];

static RPLT: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.3", "29"),
    p(3, "Size", "m", "4", "39"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "%", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "BassMult", "", "0.25", "4"),
    p(10, "Spread", "", "0", "50"),
    p(11, "Attack", "", "0", "100"),
    p(12, "ModSpin", "%", "0", "100"),
    p(13, "EchoLeft", "ms", "0", "1200"),
    p(14, "EchoRight", "ms", "0", "1200"),
    p(15, "EchoFbL", "%", "-100", "+100"),
    p(16, "EchoFbR", "%", "-100", "+100"),
];

static ROOM: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.3", "29"),
    p(3, "Size", "m", "4", "76"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "%", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "BassMult", "", "0.25", "4"),
    p(10, "Spread", "", "0", "50"),
    p(11, "Shape", "", "0", "250"),
    p(12, "ModSpin", "%", "0", "100"),
    p(13, "EchoLeft", "ms", "0", "1200"),
    p(14, "EchoRight", "ms", "0", "1200"),
    p(15, "EchoFbL", "%", "-100", "+100"),
    p(16, "EchoFbR", "%", "-100", "+100"),
];

static CHAM: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.3", "29"),
    p(3, "Size", "m", "4", "76"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "%", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "BassMult", "", "0.25", "4"),
    p(10, "Spread", "", "0", "50"),
    p(11, "Shape", "", "0", "250"),
    p(12, "ModSpin", "%", "0", "100"),
    p(13, "ERLeft", "ms", "0", "500"),
    p(14, "ERRight", "ms", "0", "500"),
    p(15, "ERLevelL", "%", "0", "100"),
    // X32 doc prints "Reflection Gain L" twice; cheat-sheet disambiguates
    // L/R — assignment inferred (notes).
    q(16, "ERLevelR", "%", "0", "100"),
];

static PLAT: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.5", "10"),
    p(3, "Size", "", "2", "100"),
    p(4, "Damping", "Hz", "1k", "20k"),
    p(5, "Diffusion", "", "1", "30"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "BassMult", "", "0.5", "2"),
    p(10, "Xover", "Hz", "10", "500"),
    p(11, "ModDepth", "", "1", "50"),
    p(12, "ModSpeed", "", "0", "100"),
];

static VREV: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "120"),
    p(2, "Decay", "s", "0.4", "4.5"),
    p(3, "Modulate", "%", "0", "100"),
    e(4, "Vintage", OFF_ON),
    e(5, "Output", VINTAGE_OUT),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "10k", "20k"),
    p(9, "LoMult", "", "0.5", "2"),
    p(10, "HiMult", "", "0.25", "1"),
];

static VRM: &[FxParamDef] = &[
    p(1, "RevDelay", "ms", "0", "200"),
    p(2, "Decay", "s", "0.1", "20"),
    p(3, "Size", "", "2", "100"),
    p(4, "Density", "", "1", "30"),
    p(5, "ERLevel", "%", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoMult", "", "0.1", "10"),
    p(8, "HiMult", "", "0.1", "10"),
    p(9, "LoCut", "Hz", "10", "500"),
    p(10, "HiCut", "Hz", "200", "20k"),
    p(11, "ERLeft", "ms", "0", "200"),
    p(12, "ERRight", "ms", "0", "200"),
    e(13, "Freeze", OFF_ON),
];

static GATE: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "ms", "140", "1000"),
    p(3, "Attack", "", "0", "30"),
    // Range conflict: cheat-sheet 1–50 vs X32 doc 1–30; X-Air preferred.
    q(4, "Density", "", "1", "50"),
    p(5, "Spread", "", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiSvFreq", "Hz", "200", "20k"),
    p(9, "HiSvGain", "dB", "-30", "0"),
    p(10, "Diffusion", "", "1", "30"),
];

static RVRS: &[FxParamDef] = &[
    p(1, "PreDelay", "ms", "0", "200"),
    p(2, "Decay", "ms", "140", "1000"),
    p(3, "Rise", "", "0", "50"),
    p(4, "Diffusion", "", "1", "30"),
    p(5, "Spread", "", "0", "100"),
    p(6, "Level", "dB", "-12", "+12"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiSvFreq", "Hz", "200", "20k"),
    p(9, "HiSvGain", "dB", "-30", "0"),
];

static DLY: &[FxParamDef] = &[
    p(1, "Mix", "%", "0", "100"),
    p(2, "Time", "ms", "1", "3000"),
    e(3, "Mode", DLY_MODE),
    e(4, "FactorL", NOTE_DIV9),
    e(5, "FactorR", NOTE_DIV9),
    p(6, "Offset", "ms", "-100", "+100"),
    p(7, "LC", "Hz", "10", "500"),
    p(8, "HC", "Hz", "200", "20k"),
    p(9, "FeedLC", "Hz", "10", "500"),
    p(10, "FeedL", "%", "0", "100"),
    p(11, "FeedR", "%", "0", "100"),
    p(12, "FeedHC", "Hz", "200", "20k"),
];

static TAP3: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    p(2, "GainT", "%", "0", "100"),
    p(3, "PanT", "%", "-100", "+100"),
    p(4, "Feedback", "%", "0", "100"),
    p(5, "LC", "Hz", "10", "500"),
    p(6, "HC", "Hz", "200", "20k"),
    e(7, "FactorA", NOTE_DIV9),
    p(8, "GainA", "%", "0", "100"),
    p(9, "PanA", "%", "-100", "+100"),
    e(10, "FactorB", NOTE_DIV9),
    p(11, "GainB", "%", "0", "100"),
    p(12, "PanB", "%", "-100", "+100"),
    e(13, "X-Feed", OFF_ON),
    e(14, "Mono", OFF_ON),
    e(15, "Dry", OFF_ON),
];

static TAP4: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    p(2, "Gain", "%", "0", "100"),
    p(3, "Feedback", "%", "0", "100"),
    p(4, "LC", "Hz", "10", "500"),
    p(5, "HC", "Hz", "200", "20k"),
    p(6, "Spread", "", "0", "6"),
    e(7, "FactorA", NOTE_DIV9),
    p(8, "GainA", "%", "0", "100"),
    e(9, "FactorB", NOTE_DIV9),
    p(10, "GainB", "%", "0", "100"),
    e(11, "FactorC", NOTE_DIV9),
    p(12, "GainC", "%", "0", "100"),
    e(13, "X-Feed", OFF_ON),
    e(14, "Mono", OFF_ON),
    e(15, "Dry", OFF_ON),
];

static CRS: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "5"),
    p(2, "DepthL", "%", "0", "100"),
    p(3, "DepthR", "%", "0", "100"),
    p(4, "DelayL", "ms", "0.5", "50"),
    p(5, "DelayR", "ms", "0.5", "50"),
    p(6, "Mix", "%", "0", "100"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "Phase", "", "0", "180"),
    p(10, "Wave", "%", "0", "100"),
    p(11, "Spread", "%", "0", "100"),
];

static FLNG: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "5"),
    p(2, "DepthL", "%", "0", "100"),
    p(3, "DepthR", "%", "0", "100"),
    p(4, "DelayL", "ms", "0.5", "20"),
    p(5, "DelayR", "ms", "0.5", "20"),
    p(6, "Mix", "%", "0", "100"),
    p(7, "LoCut", "Hz", "10", "500"),
    p(8, "HiCut", "Hz", "200", "20k"),
    p(9, "Phase", "", "0", "180"),
    p(10, "FeedLC", "Hz", "10", "500"),
    p(11, "FeedHC", "Hz", "200", "20k"),
    p(12, "Feed", "%", "-90", "+90"),
];

static PHAS: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "5"),
    p(2, "Depth", "%", "0", "100"),
    p(3, "Reso", "%", "0", "80"),
    p(4, "Base", "", "0", "50"),
    p(5, "Stages", "", "2", "12"),
    p(6, "Mix", "%", "0", "70"),
    p(7, "Wave", "", "-50", "+50"),
    p(8, "Phase", "", "0", "180"),
    p(9, "EnvMod", "%", "-100", "+100"),
    p(10, "Attack", "ms", "10", "1000"),
    p(11, "Hold", "ms", "1", "2000"),
    p(12, "Release", "ms", "10", "1000"),
];

static DIMC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "M/St", DIMC_MODE),
    e(3, "Dry", OFF_ON),
    e(4, "Sw1", OFF_ON),
    e(5, "Sw2", OFF_ON),
    e(6, "Sw3", OFF_ON),
    e(7, "Sw4", OFF_ON),
];

static FILT: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "20"),
    p(2, "Depth", "%", "0", "100"),
    p(3, "Reso", "%", "0", "100"),
    p(4, "Base", "Hz", "20", "15k"),
    e(5, "Mode", FILT_MODE),
    p(6, "Mix", "%", "0", "100"),
    e(7, "Wave", FILT_WAVE),
    p(8, "Phase", "", "0", "180"),
    p(9, "EnvMod", "%", "-100", "+100"),
    p(10, "Attack", "ms", "10", "250"),
    p(11, "Release", "ms", "10", "500"),
    p(12, "Drive", "%", "0", "100"),
    e(13, "4pole", OFF_ON),
    e(14, "SideChain", OFF_ON),
];

static ROTA: &[FxParamDef] = &[
    p(1, "LoSpeed", "Hz", "0.1", "4"),
    p(2, "HiSpeed", "Hz", "2", "10"),
    p(3, "Accel", "%", "0", "100"),
    p(4, "Distance", "%", "0", "100"),
    p(5, "Balance", "%", "-100", "+100"),
    p(6, "Mix", "%", "0", "100"),
    e(7, "StopSW", OFF_ON),
    e(8, "SpeedSW", OFF_ON),
];

static PAN: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "5"),
    p(2, "Phase", "", "0", "180"),
    p(3, "Wave", "", "-50", "+50"),
    p(4, "Depth", "%", "0", "100"),
    p(5, "EnvSpd", "%", "0", "100"),
    p(6, "EnvDepth", "%", "0", "100"),
    p(7, "Attack", "ms", "10", "1000"),
    p(8, "Hold", "ms", "1", "2000"),
    p(9, "Release", "ms", "10", "1000"),
];

/// SUB A/B half-block (dual() banks it).
static SUB5: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "Range", SUB_RANGE),
    p(3, "Dry", "%", "0", "100"),
    p(4, "Oct1", "%", "0", "100"),
    p(5, "Oct2", "%", "0", "100"),
];

static DRV_COMBO: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    e(2, "Pattern", PATTERN14),
    p(3, "FeedHC", "Hz", "1k", "20k"),
    p(4, "Feedback", "%", "0", "100"),
    p(5, "X-Feed", "%", "0", "100"),
    p(6, "Balance", "", "-100", "+100"),
    p(7, "PreDelay", "ms", "0", "200"),
    p(8, "Decay", "s", "0.1", "5"),
    p(9, "Size", "", "2", "100"),
    p(10, "Damp", "Hz", "1k", "20k"),
    p(11, "LoCut", "Hz", "10", "500"),
    p(12, "Mix", "%", "0", "100"),
];

static CRR: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "4"),
    p(2, "Depth", "%", "0", "100"),
    p(3, "Delay", "ms", "0.5", "50"),
    p(4, "Phase", "", "0", "180"),
    p(5, "Wave", "%", "0", "100"),
    p(6, "Balance", "", "-100", "+100"),
    p(7, "PreDelay", "ms", "0", "200"),
    p(8, "Decay", "s", "0.1", "5"),
    p(9, "Size", "", "2", "100"),
    p(10, "Damp", "Hz", "1k", "20k"),
    p(11, "LoCut", "Hz", "10", "500"),
    p(12, "Mix", "%", "0", "100"),
];

static FLR: &[FxParamDef] = &[
    p(1, "Speed", "Hz", "0.05", "4"),
    p(2, "Depth", "%", "0", "100"),
    p(3, "Delay", "ms", "0.5", "20"),
    p(4, "Phase", "", "0", "180"),
    p(5, "Feed", "%", "-90", "+90"),
    p(6, "Balance", "", "-100", "+100"),
    p(7, "PreDelay", "ms", "0", "200"),
    p(8, "Decay", "s", "0.1", "5"),
    p(9, "Size", "", "2", "100"),
    p(10, "Damp", "Hz", "1k", "20k"),
    p(11, "LoCut", "Hz", "10", "500"),
    p(12, "Mix", "%", "0", "100"),
];

static DCR: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    e(2, "Pattern", PATTERN14),
    p(3, "FeedHC", "Hz", "1k", "20k"),
    p(4, "Feedback", "%", "0", "100"),
    p(5, "X-Feed", "%", "0", "100"),
    p(6, "Balance", "", "-100", "+100"),
    p(7, "Speed", "Hz", "0.05", "4"),
    p(8, "Depth", "%", "0", "100"),
    p(9, "Delay", "ms", "0.5", "50"),
    p(10, "Phase", "", "0", "180"),
    p(11, "Wave", "%", "0", "100"),
    p(12, "Mix", "%", "0", "100"),
];

static DFL: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    e(2, "Pattern", PATTERN14),
    p(3, "FeedHC", "Hz", "1k", "20k"),
    p(4, "Feedback", "%", "0", "100"),
    p(5, "X-Feed", "%", "0", "100"),
    p(6, "Balance", "", "-100", "+100"),
    p(7, "Speed", "Hz", "0.05", "4"),
    p(8, "Depth", "%", "0", "100"),
    p(9, "Delay", "ms", "0.5", "20"),
    p(10, "Phase", "", "0", "180"),
    p(11, "Feed", "%", "-90", "+90"),
    p(12, "Mix", "%", "0", "100"),
];

static MODD: &[FxParamDef] = &[
    p(1, "Time", "ms", "1", "3000"),
    e(2, "Factor", MODD_FACTOR),
    p(3, "Feedback", "%", "0", "100"),
    p(4, "FeedLC", "Hz", "10", "500"),
    p(5, "FeedHC", "Hz", "1k", "20k"),
    p(6, "Depth", "%", "0", "100"),
    p(7, "Speed", "Hz", "0.05", "10"),
    e(8, "Serial", MODD_SETUP),
    e(9, "Rtype", MODD_RTYPE),
    p(10, "Decay", "", "1", "10"),
    p(11, "Damp", "Hz", "1k", "20k"),
    p(12, "Balance", "", "-100", "+100"),
    p(13, "Mix", "%", "0", "100"),
];

static DES: &[FxParamDef] = &[
    p(1, "Low(M)", "", "0", "50"),
    p(2, "High(M)", "", "0", "50"),
    p(3, "Low S", "", "0", "50"),
    p(4, "High S", "", "0", "50"),
    e(5, "Male", DES_VOICE),
    e(6, "M-S", DES_MODE),
];

static DES2: &[FxParamDef] = &[
    p(1, "Low A", "", "0", "50"),
    p(2, "High A", "", "0", "50"),
    p(3, "Low B", "", "0", "50"),
    p(4, "High B", "", "0", "50"),
    e(5, "Male A", DES_VOICE),
    e(6, "Male B", DES_VOICE),
];

/// P1A single block (11 pars, whole order flagged — X-Air splits the X32
/// doc's width/attenuation rows into Lo/Hi pairs; follow X-Air, all rows
/// inferred). P1A2 banks this via dual().
static P1A: &[FxParamDef] = &[
    ie(1, "On", OFF_ON),
    q(2, "Gain", "dB", "-12", "+12"),
    q(3, "Lo Boost", "", "0", "10"),
    ie(4, "Lo Freq", P1A_LOFREQ),
    q(5, "Lo Atten", "", "0", "10"),
    q(6, "Hi Width", "", "0", "10"),
    q(7, "Hi Boost", "", "0", "10"),
    ie(8, "Hi Freq", P1A_HIFREQ),
    q(9, "Hi Atten", "", "0", "10"),
    ie(10, "Atten Fr", P1A_ATTFR),
    ie(11, "Transf", OFF_ON),
];

static PQ5: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "Gain", "dB", "-12", "+12"),
    e(3, "Lo Freq", PQ5_LOFREQ),
    p(4, "Lo Boost", "", "0", "10"),
    e(5, "Mid Freq", PQ5_MIDFREQ),
    p(6, "Mid Cut", "", "0", "10"),
    e(7, "Hi Freq", PQ5_HIFREQ),
    p(8, "Hi Boost", "", "0", "10"),
    e(9, "Transf", OFF_ON),
];

static WAVD: &[FxParamDef] = &[
    p(1, "AttackA", "%", "-100", "+100"),
    p(2, "SustainA", "%", "-100", "+100"),
    p(3, "OutGainA", "dB", "-24", "+24"),
    p(4, "AttackB", "%", "-100", "+100"),
    p(5, "SustainB", "%", "-100", "+100"),
    p(6, "OutGainB", "dB", "-24", "+24"),
];

static LIM: &[FxParamDef] = &[
    p(1, "InGain", "dB", "0", "18"),
    p(2, "OutGain", "dB", "-18", "0"),
    p(3, "Squeeze", "%", "0", "100"),
    p(4, "Knee", "dB", "0", "10"),
    p(5, "Attack", "ms", "0.05", "1"),
    p(6, "Release", "ms", "20", "2000"),
    e(7, "STLink", OFF_ON),
    e(8, "AutoGain", OFF_ON),
];

/// Combinator block (29 pars); CMB2 banks this via dual().
static CMB: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "SoloBd", CMB_SOLO),
    p(3, "Mix", "%", "0", "100"),
    p(4, "Attack", "", "0", "19"),
    p(5, "Release", "ms", "20", "3000"),
    e(6, "AutoRel", OFF_ON),
    p(7, "SBC", "", "1", "10"),
    e(8, "SBCOn", OFF_ON),
    p(9, "XOSpc", "", "-50", "+50"),
    e(10, "Slope", CMB_SLOPE),
    e(11, "Ratio", CMB_RATIO),
    p(12, "Thresh", "dB", "-40", "0"),
    p(13, "Gain", "dB", "-10", "+10"),
    p(14, "Bd1 Thr", "dB", "-10", "+10"),
    p(15, "Bd1 Gain", "dB", "-10", "+10"),
    e(16, "Bd1 Lock", ZERO_ONE),
    p(17, "Bd2 Thr", "dB", "-10", "+10"),
    p(18, "Bd2 Gain", "dB", "-10", "+10"),
    e(19, "Bd2 Lock", ZERO_ONE),
    p(20, "Bd3 Thr", "dB", "-10", "+10"),
    p(21, "Bd3 Gain", "dB", "-10", "+10"),
    e(22, "Bd3 Lock", ZERO_ONE),
    p(23, "Bd4 Thr", "dB", "-10", "+10"),
    p(24, "Bd4 Gain", "dB", "-10", "+10"),
    e(25, "Bd4 Lock", ZERO_ONE),
    p(26, "Bd5 Thr", "dB", "-10", "+10"),
    p(27, "Bd5 Gain", "dB", "-10", "+10"),
    e(28, "Bd5 Lock", ZERO_ONE),
    e(29, "MeterSel", CMB_METER),
];

/// FAC single block; FAC2 banks this via dual().
static FAC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    // Range conflict: X-Air −20…0 vs X32 doc −20…+20; X-Air preferred.
    q(2, "InGain", "dB", "-20", "0"),
    p(3, "Threshold", "", "0", "10"),
    p(4, "Time", "", "1", "6"),
    p(5, "DCBias", "%", "0", "100"),
    p(6, "OutGain", "dB", "-18", "+6"),
    p(7, "Balance", "%", "-100", "+100"),
];

static LEC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "Gain", "", "0", "100"),
    p(3, "Peak Red", "", "0", "100"),
    e(4, "Mode", LEC_MODE),
    p(5, "OutGain", "dB", "-18", "+6"),
];

static ULC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "InGain", "dB", "-48", "0"),
    p(3, "OutGain", "dB", "-48", "0"),
    p(4, "Attack", "", "1", "7"),
    p(5, "Release", "", "1", "7"),
    e(6, "Ratio", ULC_RATIO),
];

static IMG: &[FxParamDef] = &[
    p(1, "Balance", "%", "-100", "+100"),
    p(2, "MonoPan", "%", "-100", "+100"),
    p(3, "StereoPan", "%", "-100", "+100"),
    p(4, "ShvGain", "dB", "0", "12"),
    p(5, "ShvFreq", "Hz", "100", "1000"),
    p(6, "ShvQ", "", "1", "10"),
    p(7, "OutGain", "dB", "-12", "+12"),
];

static EDI: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "In ST-MS", DES_MODE),
    e(3, "Out ST-MS", DES_MODE),
    p(4, "StSpread", "", "-50", "+50"),
    p(5, "LMFSpread", "", "-50", "+50"),
    p(6, "Balance", "", "-50", "+50"),
    p(7, "CntrDist", "", "-50", "+50"),
    p(8, "Gain", "dB", "-12", "+12"),
];

/// SON half-block (dual() banks it); Process scale conflicts between the
/// docs (0–100 vs 0–10) — that row alone is inferred.
static SON4: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "LoContour", "", "0", "10"),
    q(3, "Process", "", "0", "10"),
    p(4, "Gain", "dB", "-12", "+12"),
];

static EXC: &[FxParamDef] = &[
    p(1, "Tune", "Hz", "1k", "10k"),
    p(2, "Peak", "%", "0", "100"),
    p(3, "ZeroFill", "%", "0", "100"),
    p(4, "Timbre", "", "-50", "+50"),
    p(5, "Harmonics", "%", "0", "100"),
    p(6, "Mix", "%", "0", "100"),
    e(7, "Solo", OFF_ON),
];

static ENH: &[FxParamDef] = &[
    p(1, "OutGain", "dB", "-12", "+12"),
    // X-Air "Spread"/"MidQ" vs X32 doc "Speed"/"Mid Freq" — same position
    // and range, X-Air names preferred, rows inferred.
    q(2, "Spread", "%", "0", "100"),
    p(3, "BassGain", "%", "0", "100"),
    p(4, "BassFreq", "", "1", "50"),
    p(5, "MidGain", "%", "0", "100"),
    q(6, "MidQ", "", "1", "50"),
    p(7, "HiGain", "%", "0", "100"),
    p(8, "HiFreq", "", "1", "50"),
    e(9, "Solo", OFF_ON),
];

/// ENH2: X-Air lists 16 pars as two 8-par side blocks (no per-side Speed),
/// differing from the X32 doc's 9+9 — follow X-Air, whole effect inferred.
static ENH2: &[FxParamDef] = &[
    q(1, "OutGain", "dB", "-12", "+12"),
    q(2, "BassGain", "%", "0", "100"),
    q(3, "BassFreq", "", "1", "50"),
    q(4, "MidGain", "%", "0", "100"),
    q(5, "MidQ", "", "1", "50"),
    q(6, "HiGain", "%", "0", "100"),
    q(7, "HiFreq", "", "1", "50"),
    ie(8, "Solo", OFF_ON),
    q(9, "OutGain", "dB", "-12", "+12"),
    q(10, "BassGain", "%", "0", "100"),
    q(11, "BassFreq", "", "1", "50"),
    q(12, "MidGain", "%", "0", "100"),
    q(13, "MidQ", "", "1", "50"),
    q(14, "HiGain", "%", "0", "100"),
    q(15, "HiFreq", "", "1", "50"),
    ie(16, "Solo", OFF_ON),
];

/// AMP single block (X-Air order PreAmp/Buzz/Punch/Crunch/Drive/Level/Low/
/// High; the X32 doc swaps Level/Low — pars 06–08 inferred). AMP2 banks
/// this via dual().
static AMP: &[FxParamDef] = &[
    p(1, "PreAmp", "", "0", "10"),
    p(2, "Buzz", "", "0", "10"),
    p(3, "Punch", "", "0", "10"),
    p(4, "Crunch", "", "0", "10"),
    p(5, "Drive", "", "0", "10"),
    q(6, "Level", "", "0", "10"),
    q(7, "Low", "", "0", "10"),
    q(8, "High", "", "0", "10"),
    e(9, "Cabinet", OFF_ON),
];

static DRV: &[FxParamDef] = &[
    p(1, "Drive", "%", "0", "100"),
    p(2, "Even", "%", "0", "50"),
    p(3, "Odd", "%", "0", "50"),
    p(4, "Gain", "dB", "-12", "+12"),
    p(5, "LoCut", "Hz", "20", "200"),
    p(6, "HiCut", "Hz", "4k", "20k"),
    p(7, "LoGain", "dB", "-12", "+12"),
    p(8, "LoFreq", "Hz", "50", "400"),
    p(9, "HiGain", "dB", "-12", "+12"),
    p(10, "HiFreq", "Hz", "1k", "10k"),
];

/// PIT2: X-Air carries Gain/Pan per side where the X32 doc lists LoCut/Mix
/// per side — follow the X-Air order above; whole effect flagged.
static PIT2: &[FxParamDef] = &[
    q(1, "Semi1", "", "-12", "+12"),
    q(2, "Cent1", "", "-50", "+50"),
    q(3, "Delay1", "ms", "1", "500"),
    q(4, "Gain1", "%", "0", "100"),
    q(5, "Pan1", "%", "-100", "+100"),
    q(6, "Mix", "%", "0", "100"),
    q(7, "Semi2", "", "-12", "+12"),
    q(8, "Cent2", "", "-50", "+50"),
    q(9, "Delay2", "ms", "1", "500"),
    q(10, "Gain2", "%", "0", "100"),
    q(11, "Pan2", "%", "-100", "+100"),
    q(12, "HiCut", "Hz", "2k", "20k"),
];

static PIT: &[FxParamDef] = &[
    p(1, "Semi", "", "-12", "+12"),
    p(2, "Cent", "", "-50", "+50"),
    // Range mismatch: X-Air 1–500 ms vs X32 doc 1–100 — X-Air preferred.
    q(3, "Delay", "ms", "1", "500"),
    p(4, "LoCut", "Hz", "10", "500"),
    p(5, "HiCut", "Hz", "2k", "20k"),
    p(6, "Mix", "%", "0", "100"),
];

/// Split a single-block base table into A/B halves: pars `1..=len` bank
/// `"A"`, `len+1..=2*len` bank `"B"`. Anything else is padding (`None`).
fn dual(base: &'static [FxParamDef], par: u8) -> Option<FxParamDef> {
    let half = base.len() as u8;
    if half == 0 || par == 0 {
        return None;
    }
    let (idx, bank) = if par <= half {
        (par - 1, "A")
    } else if par <= half * 2 {
        (par - 1 - half, "B")
    } else {
        return None;
    };
    base.get(idx as usize).map(|d| FxParamDef {
        par,
        bank: Some(bank),
        ..*d
    })
}

/// Graphic-EQ def (GEQ/TEQ): band centres `1..=31`, master `Lvl` at 32.
fn geq_def(par: u8, bank: Option<&'static str>) -> Option<FxParamDef> {
    if (1..=31).contains(&par) {
        Some(FxParamDef {
            par,
            label: GEQ_BANDS[(par - 1) as usize],
            unit: "Hz",
            min: "-15",
            max: "+15",
            options: None,
            inferred: false,
            bank,
        })
    } else if par == 32 {
        Some(FxParamDef {
            par,
            label: "Lvl",
            unit: "dB",
            min: "-15",
            max: "+15",
            options: None,
            inferred: false,
            bank,
        })
    } else {
        None
    }
}

/// Dual graphic-EQ def (GEQ2/TEQ2): A block `1..=32`, B block `33..=64`.
fn geq2_def(par: u8) -> Option<FxParamDef> {
    if (1..=32).contains(&par) {
        geq_def(par, Some("A"))
    } else if (33..=64).contains(&par) {
        geq_def(par - 32, Some("B")).map(|d| FxParamDef { par, ..d })
    } else {
        None
    }
}

/// Short display name for an effect-type index (`None` = undocumented
/// index — index 15 is a gap in the wire order; treat like unknown).
/// This is the desk code (`HALL`, `DLY`, …): stable wire key, used as the
/// `caps.fx` type `id`. Human-facing UI shows [`fx_effect_full_name`].
///
/// ```
/// # use openfader_proto_xair::fx::fx_effect_name;
/// assert_eq!(fx_effect_name(0), Some("HALL"));
/// assert_eq!(fx_effect_name(60), Some("PIT"));
/// assert_eq!(fx_effect_name(15), None);
/// assert_eq!(fx_effect_name(61), None);
/// ```
pub fn fx_effect_name(index: u8) -> Option<&'static str> {
    let (name, _, _) = fx_effect_meta(index)?;
    Some(name)
}

/// Full display name for an effect-type index (`None` = undocumented
/// index, same gap rule as [`fx_effect_name`]).
///
/// Provenance: 58 of the 60 names are the X-AIR-Edit 1.8.1 rack-select
/// strings (see X-Air Edit 1.8.1 app structure capture §4 —
/// our own prior strings inventory, names only, no artwork): matched to
/// wire indices by family/param semantics (the rack-select order is
/// grouped by category, NOT wire order). Two are clean-room inferences,
/// marked below: `PLAT` has no entry in the captured strings
/// ("Plate Reverb" from the code + plate param family) and `ULC2` takes
/// "Dual Ultimo Compressor" (the captured "Stereo Ultimo Compressor"
/// has no corresponding wire index in the 0..=60 order — possibly a
/// single-channel variant outside this table; never asserted).
///
/// ```
/// # use openfader_proto_xair::fx::fx_effect_full_name;
/// assert_eq!(fx_effect_full_name(0), Some("Hall Reverb"));
/// assert_eq!(fx_effect_full_name(10), Some("Stereo Delay"));
/// assert_eq!(fx_effect_full_name(60), Some("Stereo Pitch Shifter"));
/// assert_eq!(fx_effect_full_name(15), None);
/// ```
pub fn fx_effect_full_name(index: u8) -> Option<&'static str> {
    let (_, full, _) = fx_effect_meta(index)?;
    Some(full)
}

/// Number of meaningful (non-padding) pars for an effect-type index
/// (`None` = undocumented index).
///
/// ```
/// # use openfader_proto_xair::fx::fx_param_count;
/// assert_eq!(fx_param_count(0), Some(12));
/// assert_eq!(fx_param_count(28), Some(64));
/// assert_eq!(fx_param_count(15), None);
/// ```
pub fn fx_param_count(index: u8) -> Option<u8> {
    let (_, _, used) = fx_effect_meta(index)?;
    Some(used)
}

fn fx_effect_meta(index: u8) -> Option<(&'static str, &'static str, u8)> {
    match index {
        0 => Some(("HALL", "Hall Reverb", 12)),
        1 => Some(("AMBI", "Ambience", 10)),
        2 => Some(("RPLT", "Rich Plate Reverb", 16)),
        3 => Some(("ROOM", "Room Reverb", 16)),
        4 => Some(("CHAM", "Chamber Reverb", 16)),
        // Inferred: no "Plate Reverb" in the captured rack-select strings.
        5 => Some(("PLAT", "Plate Reverb", 12)),
        6 => Some(("VREV", "Vintage Reverb", 10)),
        7 => Some(("VRM", "Vintage Room", 13)),
        8 => Some(("GATE", "Gated Reverb", 10)),
        9 => Some(("RVRS", "Reverse Reverb", 9)),
        10 => Some(("DLY", "Stereo Delay", 12)),
        11 => Some(("3TAP", "3-Tap Delay", 15)),
        12 => Some(("4TAP", "Rhythm Delay", 15)),
        13 => Some(("CRS", "Stereo Chorus", 11)),
        14 => Some(("FLNG", "Stereo Flanger", 12)),
        // 15: undocumented gap (FLNG 14 → PHAS 16) — generic fallback.
        16 => Some(("PHAS", "Stereo Phaser", 12)),
        17 => Some(("DIMC", "Dimensional Chorus", 7)),
        18 => Some(("FILT", "Mood Filter", 14)),
        19 => Some(("ROTA", "Rotary Speaker", 8)),
        20 => Some(("PAN", "Tremolo / Panner", 9)),
        21 => Some(("SUB", "Suboctaver", 10)),
        22 => Some(("D/RV", "Delay + Chamber", 12)),
        23 => Some(("CR/R", "Chorus + Chamber", 12)),
        24 => Some(("FL/R", "Flanger + Chamber", 12)),
        25 => Some(("D/CR", "Delay + Chorus", 12)),
        26 => Some(("D/FL", "Delay + Flanger", 12)),
        27 => Some(("MODD", "Modulation Delay", 13)),
        28 => Some(("GEQ2", "Dual Graphic EQ", 64)),
        29 => Some(("GEQ", "Stereo Graphic EQ", 32)),
        30 => Some(("TEQ2", "Dual TruEQ", 64)),
        31 => Some(("TEQ", "Stereo TruEQ", 32)),
        32 => Some(("DES2", "Dual DeEsser", 6)),
        33 => Some(("DES", "Stereo DeEsser", 6)),
        34 => Some(("P1A", "Stereo Xtec EQ1", 11)),
        35 => Some(("P1A2", "Dual Xtec EQ1", 22)),
        36 => Some(("PQ5", "Stereo Xtec EQ5", 9)),
        37 => Some(("PQ5S", "Dual Xtec EQ5", 18)),
        38 => Some(("WAVD", "Wave Designer", 6)),
        39 => Some(("LIM", "Precision Limiter", 8)),
        40 => Some(("CMB", "Stereo Combinator", 29)),
        41 => Some(("CMB2", "Dual Combinator", 58)),
        42 => Some(("FAC", "Stereo Fair Compressor", 7)),
        43 => Some(("FAC1M", "M/S Fair Compressor", 14)),
        44 => Some(("FAC2", "Dual Fair Compressor", 14)),
        45 => Some(("LEC", "Stereo Leisure Compressor", 5)),
        46 => Some(("LEC2", "Dual Leisure Compressor", 10)),
        // 47/48 are contested in the notes: the wire-order grid (stated
        // doubly-sourced, count-checked) says 47=ULC2/48=ENH2, while the
        // section headers claim ULC(47), ULC2(48) AND ENH2(48) — 48 is
        // double-claimed, so at least one section number is a typo. The
        // grid wins for names; every row of 47 is forced inferred (see
        // fx_param_def) and 48 is wholly inferred already (flagged layout).
        // "Dual Ultimo Compressor": the captured strings also list a
        // "Stereo Ultimo Compressor" with no wire index here (see
        // fx_effect_full_name docs) — never asserted.
        47 => Some(("ULC2", "Dual Ultimo Compressor", 12)),
        48 => Some(("ENH2", "Dual Enhancer", 16)),
        49 => Some(("ENH", "Stereo Enhancer", 9)),
        50 => Some(("EXC2", "Dual Exciter", 14)),
        51 => Some(("EXC", "Stereo Exciter", 7)),
        52 => Some(("IMG", "Stereo Imager", 7)),
        53 => Some(("EDI", "Edison EX1", 8)),
        54 => Some(("SON", "Sound Maxer", 8)),
        55 => Some(("AMP2", "Dual Guitar Amp", 18)),
        56 => Some(("AMP", "Stereo Guitar Amp", 9)),
        57 => Some(("DRV2", "Dual Tube Stage", 20)),
        58 => Some(("DRV", "Stereo Tube Stage", 10)),
        59 => Some(("PIT2", "Dual Pitch Shifter", 12)),
        60 => Some(("PIT", "Stereo Pitch Shifter", 6)),
        _ => None,
    }
}

/// Labeled def for 1-based `par` of effect-type `index` (`None` = padding
/// par or unknown effect — the UI hides padding and falls back to generic
/// numbered sliders for unknown types).
///
/// ```
/// # use openfader_proto_xair::fx::fx_param_def;
/// let d = fx_param_def(0, 1).unwrap();
/// assert_eq!((d.label, d.unit), ("PreDelay", "ms"));
/// assert!(fx_param_def(0, 13).is_none()); // HALL padding
/// assert!(fx_param_def(99, 1).is_none()); // unknown effect
/// ```
pub fn fx_param_def(index: u8, par: u8) -> Option<FxParamDef> {
    if par == 0 || par > PARAM_COUNT as u8 {
        return None;
    }
    // NOTE: indices 47–60 follow the wire-order grid (doubly-sourced),
    // NOT the research file's section order — e.g. 47 is ULC2 (not ULC),
    // 48 is ENH2, 50 is EXC2, 59 is PIT2 and 60 is PIT. The meta table
    // above is the check.
    match index {
        0 => HALL.get((par - 1) as usize).copied(),
        1 => AMBI.get((par - 1) as usize).copied(),
        2 => RPLT.get((par - 1) as usize).copied(),
        3 => ROOM.get((par - 1) as usize).copied(),
        4 => CHAM.get((par - 1) as usize).copied(),
        5 => PLAT.get((par - 1) as usize).copied(),
        6 => VREV.get((par - 1) as usize).copied(),
        7 => VRM.get((par - 1) as usize).copied(),
        8 => GATE.get((par - 1) as usize).copied(),
        9 => RVRS.get((par - 1) as usize).copied(),
        10 => DLY.get((par - 1) as usize).copied(),
        11 => TAP3.get((par - 1) as usize).copied(),
        12 => TAP4.get((par - 1) as usize).copied(),
        13 => CRS.get((par - 1) as usize).copied(),
        14 => FLNG.get((par - 1) as usize).copied(),
        16 => PHAS.get((par - 1) as usize).copied(),
        17 => DIMC.get((par - 1) as usize).copied(),
        18 => FILT.get((par - 1) as usize).copied(),
        19 => ROTA.get((par - 1) as usize).copied(),
        20 => PAN.get((par - 1) as usize).copied(),
        21 => dual(SUB5, par),
        22 => DRV_COMBO.get((par - 1) as usize).copied(),
        23 => CRR.get((par - 1) as usize).copied(),
        24 => FLR.get((par - 1) as usize).copied(),
        25 => DCR.get((par - 1) as usize).copied(),
        26 => DFL.get((par - 1) as usize).copied(),
        27 => MODD.get((par - 1) as usize).copied(),
        28 => geq2_def(par),
        29 => geq_def(par, None),
        30 => geq2_def(par),
        31 => geq_def(par, None),
        32 => DES2.get((par - 1) as usize).copied(),
        33 => DES.get((par - 1) as usize).copied(),
        34 => P1A.get((par - 1) as usize).copied(),
        35 => dual(P1A, par),
        36 => PQ5.get((par - 1) as usize).copied(),
        37 => dual(PQ5, par),
        38 => WAVD.get((par - 1) as usize).copied(),
        39 => LIM.get((par - 1) as usize).copied(),
        40 => CMB.get((par - 1) as usize).copied(),
        41 => dual(CMB, par),
        42 => FAC.get((par - 1) as usize).copied(),
        // FAC1M header is inferred from block position (notes); same layout
        // as FAC2 but every row low-confidence.
        43 => dual(FAC, par).map(|d| FxParamDef {
            inferred: true,
            ..d
        }),
        44 => dual(FAC, par),
        45 => LEC.get((par - 1) as usize).copied(),
        46 => dual(LEC, par),
        // Contested identity (see meta): grid says ULC2, one section header
        // says ULC. The ULC2 A-block coincides with the ULC table either
        // way, so pars 1–6 are safe; every row is still forced inferred.
        47 => dual(ULC, par).map(|d| FxParamDef {
            inferred: true,
            ..d
        }),
        48 => ENH2.get((par - 1) as usize).copied(),
        49 => ENH.get((par - 1) as usize).copied(),
        50 => dual(EXC, par),
        51 => EXC.get((par - 1) as usize).copied(),
        52 => IMG.get((par - 1) as usize).copied(),
        53 => EDI.get((par - 1) as usize).copied(),
        54 => dual(SON4, par),
        55 => dual(AMP, par),
        56 => AMP.get((par - 1) as usize).copied(),
        57 => dual(DRV, par),
        58 => DRV.get((par - 1) as usize).copied(),
        59 => PIT2.get((par - 1) as usize).copied(),
        60 => PIT.get((par - 1) as usize).copied(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tap-tempo mapping (delay-type effects with a confident Time par only).
// ---------------------------------------------------------------------------

/// Tap-tempo subdivision: quarter-note or dotted-eighth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapSubdiv {
    Quarter,
    Dotted8th,
}

/// Tap-tempo target for an effect-type index: 1-based `par` carrying the
/// delay time, its display range in ms, and whether the effect offers a
/// documented note-division enum (Factor/Pattern) alongside it.
///
/// Only delay-type effects whose Time row is documented (non-`inferred`)
/// in the notes are wired: DLY (10, par 2), 3TAP (11), 4TAP (12),
/// D/RV (22), D/CR (25), D/FL (26) and MODD (27) — all par 1, all
/// `1..=3000` ms. Every one of these also carries a documented
/// Factor/Pattern division enum, so `subdiv` is always true here and the
/// UI offers quarter + dotted-eighth. All other effects — including PIT
/// (60) / PIT2 (59), whose Delay rows are range-inferred — return `None`
/// and get no TAP button (no guessing).
///
/// ```
/// # use openfader_proto_xair::fx::{fx_tap_target, TapSubdiv, tap_delay_ms};
/// let t = fx_tap_target(10).unwrap();
/// assert_eq!((t.par, t.min_ms, t.max_ms), (2, 1.0, 3000.0));
/// assert!(t.subdiv);
/// assert!(fx_tap_target(0).is_none()); // HALL: reverb, no tap
/// assert!(fx_tap_target(60).is_none()); // PIT Delay row is inferred
/// assert!((tap_delay_ms(120.0, TapSubdiv::Quarter) - 500.0).abs() < 1e-9);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTapTarget {
    pub par: u8,
    pub min_ms: f64,
    pub max_ms: f64,
    pub subdiv: bool,
}

pub fn fx_tap_target(index: u8) -> Option<FxTapTarget> {
    match index {
        10 => Some(FxTapTarget {
            par: 2,
            min_ms: 1.0,
            max_ms: 3000.0,
            subdiv: true,
        }),
        11 | 12 | 22 | 25 | 26 | 27 => Some(FxTapTarget {
            par: 1,
            min_ms: 1.0,
            max_ms: 3000.0,
            subdiv: true,
        }),
        _ => None,
    }
}

/// BPM from tap intervals: needs 3+ intervals (4+ taps); `None` when too
/// few, non-finite/non-positive, or outside a sane 20..=300 BPM window.
///
/// ```
/// # use openfader_proto_xair::fx::tap_bpm_from_intervals_ms;
/// assert!((tap_bpm_from_intervals_ms(&[500.0, 500.0, 500.0]).unwrap() - 120.0).abs() < 1e-9);
/// assert!(tap_bpm_from_intervals_ms(&[500.0, 500.0]).is_none()); // only 3 taps
/// ```
pub fn tap_bpm_from_intervals_ms(intervals_ms: &[f64]) -> Option<f64> {
    if intervals_ms.len() < 3 {
        return None;
    }
    if intervals_ms.iter().any(|i| !i.is_finite() || *i <= 0.0) {
        return None;
    }
    let mean = intervals_ms.iter().sum::<f64>() / intervals_ms.len() as f64;
    if !mean.is_finite() || mean <= 0.0 {
        return None;
    }
    let bpm = 60_000.0 / mean;
    if !(20.0..=300.0).contains(&bpm) {
        return None;
    }
    Some(bpm)
}

/// Delay time in ms for a BPM: quarter = one beat (`60000/bpm`),
/// dotted-eighth = three-quarters of a beat (`45000/bpm`).
pub fn tap_delay_ms(bpm: f64, subdiv: TapSubdiv) -> f64 {
    match subdiv {
        TapSubdiv::Quarter => 60_000.0 / bpm,
        TapSubdiv::Dotted8th => 45_000.0 / bpm,
    }
}

/// Linear map of an ms delay time onto the `0..1` wire value over the
/// par's display range, clamped. The notes pin no fader law for FX time
/// pars, so this is explicitly assumed-linear (same standing as the UI's
/// assumed discrete enum steps) — documented, never asserted on hardware.
pub fn tap_ms_to_normalized(ms: f64, min_ms: f64, max_ms: f64) -> f32 {
    if !ms.is_finite() || !min_ms.is_finite() || !max_ms.is_finite() || max_ms <= min_ms {
        return 0.0;
    }
    ((ms - min_ms) / (max_ms - min_ms)).clamp(0.0, 1.0) as f32
}

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
        assert_eq!(fx_type(1).unwrap(), "/fx/1/type");
        assert_eq!(fx_type(4).unwrap(), "/fx/4/type");
        assert_eq!(fx_insert(1).unwrap(), "/fx/1/insert");
        assert_eq!(fx_insert(4).unwrap(), "/fx/4/insert");
        assert!(fx_insert(0).is_err());
        assert!(fx_insert(5).is_err());
        assert_eq!(fx_type_to_arg(0).unwrap(), OscType::Int(0));
        assert_eq!(fx_type_to_arg(60).unwrap(), OscType::Int(60));
        assert!(fx_type_to_arg(61).is_err());
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(fx_param(0, 0).is_err());
        assert!(fx_param(5, 0).is_err());
        assert!(fx_param(1, 64).is_err());
        assert!(fx_field(0, 0).is_err());
        assert!(fx_type(0).is_err());
        assert!(fx_type(5).is_err());
        assert!(check_value(f32::NAN).is_err());
        assert!(check_value(1.5).is_err());
        assert_eq!(check_value(0.5).unwrap(), 0.5);
    }

    #[test]
    fn parse_type_shapes() {
        assert_eq!(parse_fx_type(&[OscType::Int(10)]).unwrap(), 10);
        assert_eq!(parse_fx_type(&[OscType::Int(0)]).unwrap(), 0);
        assert_eq!(parse_fx_type(&[OscType::Int(60)]).unwrap(), 60);
        assert_eq!(parse_fx_type(&[OscType::Float(10.0)]).unwrap(), 10);
        assert_eq!(parse_fx_type(&[OscType::Double(60.0)]).unwrap(), 60);
        assert!(parse_fx_type(&[]).is_err());
        assert!(parse_fx_type(&[OscType::Int(61)]).is_err());
        assert!(parse_fx_type(&[OscType::Int(-1)]).is_err());
        assert!(parse_fx_type(&[OscType::Float(10.5)]).is_err());
        assert!(parse_fx_type(&[OscType::Float(f32::NAN)]).is_err());
        assert!(parse_fx_type(&[OscType::String("HALL".into())]).is_err());
    }

    #[test]
    fn int_pars_decode_per_def_and_nan_is_absent() {
        // VRM par 13 is Freeze (OFF/ON): hardware replies INT 0.
        let freeze = fx_param_def(7, 13);
        assert_eq!(freeze.unwrap().options, Some(OFF_ON));
        assert_eq!(
            parse_fx_param(&[OscType::Int(0)], freeze.as_ref()).unwrap(),
            0.0
        );
        assert_eq!(
            parse_fx_param(&[OscType::Int(1)], freeze.as_ref()).unwrap(),
            1.0
        );
        assert_eq!(
            parse_fx_param(&[OscType::Long(1)], freeze.as_ref()).unwrap(),
            1.0
        );
        assert!(parse_fx_param(&[OscType::Int(2)], freeze.as_ref()).is_err());
        assert!(parse_fx_param(&[OscType::Int(-1)], freeze.as_ref()).is_err());

        // D/FL par 02 is Pattern (14 options): int is the option index.
        let pattern = fx_param_def(26, 2);
        assert_eq!(pattern.unwrap().options, Some(PATTERN14));
        assert_eq!(
            parse_fx_param(&[OscType::Int(0)], pattern.as_ref()).unwrap(),
            0.0
        );
        assert_eq!(
            parse_fx_param(&[OscType::Int(13)], pattern.as_ref()).unwrap(),
            1.0
        );
        assert!(
            (parse_fx_param(&[OscType::Int(1)], pattern.as_ref()).unwrap() - 1.0 / 13.0).abs()
                < 1e-6
        );
        assert!(parse_fx_param(&[OscType::Int(14)], pattern.as_ref()).is_err());

        // Continuous def with an int wire value (D/FL par 08/09 reply INT
        // 1): raw int, still range-checked.
        let depth = fx_param_def(26, 8);
        assert!(depth.unwrap().options.is_none());
        assert_eq!(
            parse_fx_param(&[OscType::Int(1)], depth.as_ref()).unwrap(),
            1.0
        );
        assert_eq!(
            parse_fx_param(&[OscType::Int(0)], depth.as_ref()).unwrap(),
            0.0
        );
        assert!(parse_fx_param(&[OscType::Int(2)], depth.as_ref()).is_err());

        // Floats pass through validated; unknown def decodes raw too.
        let hall = fx_param_def(0, 1);
        assert_eq!(
            parse_fx_param(&[OscType::Float(0.5)], hall.as_ref()).unwrap(),
            0.5
        );
        assert_eq!(
            parse_fx_param(&[OscType::Double(0.25)], hall.as_ref()).unwrap(),
            0.25
        );
        assert_eq!(parse_fx_param(&[OscType::Float(0.25)], None).unwrap(), 0.25);
        assert_eq!(parse_fx_param(&[OscType::Int(1)], None).unwrap(), 1.0);
        assert!(parse_fx_param(&[OscType::Float(1.5)], hall.as_ref()).is_err());

        // NaN (float or double, known def or not) means absent: bail with
        // an "absent" error, never surface NaN as a value.
        for args in [
            vec![OscType::Float(f32::NAN)],
            vec![OscType::Double(f64::NAN)],
        ] {
            for def in [hall.as_ref(), None] {
                let err = parse_fx_param(&args, def).unwrap_err();
                assert!(err.to_string().contains("absent"), "unexpected: {err}");
            }
        }
        assert!(parse_fx_param(&[OscType::Float(f32::INFINITY)], hall.as_ref()).is_err());
        assert!(parse_fx_param(&[], hall.as_ref()).is_err());
        assert!(parse_fx_param(&[OscType::String("x".into())], hall.as_ref()).is_err());
    }

    #[test]
    fn effect_names_cover_wire_order() {
        // 61 slots 0..=60, one undocumented gap at 15 → 60 names.
        let named: Vec<u8> = (0..=60).filter(|i| fx_effect_name(*i).is_some()).collect();
        assert_eq!(named.len(), 60);
        assert!(!named.contains(&15));
        assert_eq!(fx_effect_name(0), Some("HALL"));
        assert_eq!(fx_effect_name(10), Some("DLY"));
        assert_eq!(fx_effect_name(16), Some("PHAS"));
        assert_eq!(fx_effect_name(28), Some("GEQ2"));
        assert_eq!(fx_effect_name(43), Some("FAC1M"));
        assert_eq!(fx_effect_name(47), Some("ULC2"));
        assert_eq!(fx_effect_name(48), Some("ENH2"));
        assert_eq!(fx_effect_name(59), Some("PIT2"));
        assert_eq!(fx_effect_name(60), Some("PIT"));
        assert_eq!(fx_effect_name(61), None);
        assert_eq!(fx_effect_name(255), None);
    }

    #[test]
    fn full_names_cover_wire_order() {
        // Every documented index has a full display name; the gap (15)
        // and out-of-range indices have none. Spot-check the mapping.
        let named: Vec<u8> = (0..=60).filter(|i| fx_effect_full_name(*i).is_some()).collect();
        assert_eq!(named.len(), 60);
        assert!(!named.contains(&15));
        for i in named {
            let full = fx_effect_full_name(i).unwrap();
            assert!(!full.is_empty(), "index {i}");
            assert_ne!(full, fx_effect_name(i).unwrap(), "index {i} full == code");
        }
        assert_eq!(fx_effect_full_name(0), Some("Hall Reverb"));
        assert_eq!(fx_effect_full_name(2), Some("Rich Plate Reverb"));
        assert_eq!(fx_effect_full_name(10), Some("Stereo Delay"));
        assert_eq!(fx_effect_full_name(11), Some("3-Tap Delay"));
        assert_eq!(fx_effect_full_name(12), Some("Rhythm Delay"));
        assert_eq!(fx_effect_full_name(20), Some("Tremolo / Panner"));
        assert_eq!(fx_effect_full_name(21), Some("Suboctaver"));
        assert_eq!(fx_effect_full_name(22), Some("Delay + Chamber"));
        assert_eq!(fx_effect_full_name(27), Some("Modulation Delay"));
        assert_eq!(fx_effect_full_name(43), Some("M/S Fair Compressor"));
        assert_eq!(fx_effect_full_name(47), Some("Dual Ultimo Compressor"));
        assert_eq!(fx_effect_full_name(53), Some("Edison EX1"));
        assert_eq!(fx_effect_full_name(54), Some("Sound Maxer"));
        assert_eq!(fx_effect_full_name(60), Some("Stereo Pitch Shifter"));
        assert_eq!(fx_effect_full_name(15), None);
        assert_eq!(fx_effect_full_name(61), None);
    }

    #[test]
    fn used_counts_match_notes() {
        let cases: &[(u8, u8)] = &[
            (0, 12),
            (1, 10),
            (2, 16),
            (7, 13),
            (9, 9),
            (11, 15),
            (13, 11),
            (14, 12),
            (16, 12),
            (17, 7),
            (19, 8),
            (21, 10),
            (27, 13),
            (28, 64),
            (29, 32),
            (32, 6),
            (34, 11),
            (35, 22),
            (36, 9),
            (37, 18),
            (39, 8),
            (40, 29),
            (41, 58),
            (42, 7),
            (43, 14),
            (45, 5),
            (46, 10),
            (47, 12),
            (48, 16),
            (49, 9),
            (50, 14),
            (51, 7),
            (52, 7),
            (53, 8),
            (54, 8),
            (55, 18),
            (56, 9),
            (57, 20),
            (58, 10),
            (59, 12),
            (60, 6),
        ];
        for (index, used) in cases {
            assert_eq!(fx_param_count(*index), Some(*used), "index {index}");
        }
        assert_eq!(fx_param_count(15), None);
        assert_eq!(fx_param_count(99), None);
    }

    #[test]
    fn padding_and_unknown_yield_none() {
        assert!(fx_param_def(0, 13).is_none()); // HALL uses 01–12
        assert!(fx_param_def(17, 8).is_none()); // DIMC uses 01–07
        assert!(fx_param_def(45, 6).is_none()); // LEC uses 01–05
        assert!(fx_param_def(60, 7).is_none()); // PIT uses 01–06
        assert!(fx_param_def(29, 33).is_none()); // GEQ uses 01–32
        assert!(fx_param_def(15, 1).is_none()); // undocumented gap
        assert!(fx_param_def(99, 1).is_none()); // unknown effect
        assert!(fx_param_def(0, 0).is_none());
        assert!(fx_param_def(0, 65).is_none());
    }

    #[test]
    fn spot_labels_units_ranges() {
        let d = fx_param_def(0, 1).unwrap(); // HALL PreDelay
        assert_eq!((d.label, d.unit, d.min, d.max), ("PreDelay", "ms", "0", "200"));
        assert!(!d.inferred && d.options.is_none() && d.bank.is_none());

        let d = fx_param_def(10, 3).unwrap(); // DLY Mode
        assert_eq!(d.label, "Mode");
        assert_eq!(d.options, Some(DLY_MODE));

        let d = fx_param_def(29, 1).unwrap(); // GEQ 20 Hz band
        assert_eq!((d.label, d.unit, d.min, d.max), ("20", "Hz", "-15", "+15"));
        let d = fx_param_def(29, 32).unwrap(); // GEQ master
        assert_eq!(d.label, "Lvl");
        let d = fx_param_def(28, 64).unwrap(); // GEQ2 B master
        assert_eq!((d.label, d.bank), ("Lvl", Some("B")));

        let d = fx_param_def(40, 11).unwrap(); // CMB ratio
        assert_eq!(d.options, Some(CMB_RATIO));
        assert_eq!(d.options.unwrap().len(), 14);

        let d = fx_param_def(6, 5).unwrap(); // VREV Output
        assert_eq!(d.options, Some(VINTAGE_OUT));

        let d = fx_param_def(33, 5).unwrap(); // DES Voice
        assert_eq!(d.options, Some(DES_VOICE));
    }

    #[test]
    fn inferred_flags_match_notes() {
        // Documented rows stay clean.
        assert!(!fx_param_def(0, 1).unwrap().inferred);
        assert!(!fx_param_def(49, 3).unwrap().inferred);
        assert!(!fx_param_def(56, 1).unwrap().inferred);
        // Flagged rows carry the marker.
        assert!(fx_param_def(4, 16).unwrap().inferred); // CHAM ERLevelR
        assert!(fx_param_def(8, 4).unwrap().inferred); // GATE Density range
        assert!(fx_param_def(60, 3).unwrap().inferred); // PIT Delay range
        assert!(fx_param_def(42, 2).unwrap().inferred); // FAC InGain range
        assert!(!fx_param_def(42, 3).unwrap().inferred);
        assert!(fx_param_def(49, 2).unwrap().inferred); // ENH Spread name
        assert!(fx_param_def(49, 6).unwrap().inferred); // ENH MidQ name
        assert!(fx_param_def(56, 6).unwrap().inferred); // AMP Level order
        assert!(!fx_param_def(56, 9).unwrap().inferred); // AMP Cabinet ok
        assert!(fx_param_def(54, 3).unwrap().inferred); // SON Process scale
        assert!(!fx_param_def(54, 1).unwrap().inferred);
        // Contested 47 (grid ULC2 vs section ULC): every row inferred.
        for par in 1..=12 {
            assert!(fx_param_def(47, par).unwrap().inferred, "ULC2 par {par}");
        }
        // Wholly flagged effects: every row inferred.
        for par in 1..=12 {
            assert!(fx_param_def(59, par).unwrap().inferred, "PIT2 par {par}");
        }
        for par in 1..=11 {
            assert!(fx_param_def(34, par).unwrap().inferred, "P1A par {par}");
        }
        for par in 1..=16 {
            assert!(fx_param_def(48, par).unwrap().inferred, "ENH2 par {par}");
        }
        for par in 1..=14 {
            assert!(fx_param_def(43, par).unwrap().inferred, "FAC1M par {par}");
        }
        assert!(!fx_param_def(44, 1).unwrap().inferred); // FAC2 same layout, clean
    }

    #[test]
    fn dual_banks_split_cleanly() {
        let a = fx_param_def(46, 2).unwrap(); // LEC2 Gain A
        assert_eq!((a.label, a.bank), ("Gain", Some("A")));
        let b = fx_param_def(46, 7).unwrap(); // LEC2 Gain B
        assert_eq!((b.label, b.bank, b.par), ("Gain", Some("B"), 7));
        assert!(fx_param_def(46, 11).is_none());

        let b = fx_param_def(21, 6).unwrap(); // SUB On B
        assert_eq!((b.label, b.bank), ("On", Some("B")));

        let b = fx_param_def(41, 30).unwrap(); // CMB2 On B
        assert_eq!((b.label, b.bank), ("On", Some("B")));
        let lock = fx_param_def(41, 57).unwrap(); // CMB2 Bd5 Lock B
        assert_eq!((lock.label, lock.bank), ("Bd5 Lock", Some("B")));
        assert_eq!(lock.options, Some(ZERO_ONE));
        assert!(fx_param_def(41, 59).is_none());

        let b = fx_param_def(55, 18).unwrap(); // AMP2 Cabinet B
        assert_eq!((b.label, b.bank), ("Cabinet", Some("B")));
        assert!(fx_param_def(55, 6).unwrap().inferred); // order flag survives banking
    }

    /// Loopback: a fake desk serving `/fx/N/type` (int) + `/fx/N/par/MM`
    /// (floats) proves the read path — query type, map to the effect, then
    /// drive that effect's labeled pars bit-exact.
    #[tokio::test]
    async fn loopback_type_then_labeled_pars() {
        use openfader_transport::{osc, OscSession};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex as StdMutex};
        use tokio::net::UdpSocket;

        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let types: Arc<StdMutex<HashMap<u8, i32>>> =
            Arc::new(StdMutex::new(HashMap::from([(1u8, 10i32), (2u8, 60i32)])));
        let pars: Arc<StdMutex<HashMap<String, f32>>> = Arc::new(StdMutex::new(HashMap::new()));
        let worker = socket.clone();
        let wtypes = types.clone();
        let wpars = pars.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    let parts: Vec<&str> = msg.addr.split('/').collect();
                    // `/fx/N/type`
                    if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                        let Ok(slot) = parts[2].parse::<u8>() else {
                            continue;
                        };
                        match msg.args.first() {
                            // Store type selects verbatim (int form).
                            Some(OscType::Int(i)) => {
                                wtypes.lock().unwrap().insert(slot, *i);
                            }
                            Some(OscType::Float(f)) => {
                                wtypes.lock().unwrap().insert(slot, *f as i32);
                            }
                            _ => {
                                let t =
                                    wtypes.lock().unwrap().get(&slot).copied().unwrap_or(0);
                                let bytes = osc::encode_message(
                                    &msg.addr,
                                    vec![OscType::Int(t)],
                                )
                                .unwrap();
                                let _ = worker.send_to(&bytes, peer).await;
                            }
                        }
                        continue;
                    }
                    // `/fx/N/par/MM`
                    if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                        match msg.args.first() {
                            Some(OscType::Float(f)) => {
                                wpars.lock().unwrap().insert(msg.addr.clone(), *f);
                            }
                            _ => {
                                let v = wpars
                                    .lock()
                                    .unwrap()
                                    .get(&msg.addr)
                                    .copied()
                                    .unwrap_or(0.0);
                                let bytes = osc::encode_message(
                                    &msg.addr,
                                    vec![OscType::Float(v)],
                                )
                                .unwrap();
                                let _ = worker.send_to(&bytes, peer).await;
                            }
                        }
                    }
                }
            }
        });

        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(addr).await.unwrap();
        let session = OscSession::from_connected_socket(sock, None);

        // Slot 1 loads DLY (10): type query → effect → labeled par defs.
        let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
        let loaded = parse_fx_type(&reply.args).unwrap();
        assert_eq!(loaded, 10);
        assert_eq!(fx_effect_name(loaded), Some("DLY"));
        let mode = fx_param_def(loaded, 3).unwrap();
        assert_eq!(mode.label, "Mode");
        assert_eq!(mode.options, Some(DLY_MODE));

        // Slot 2 loads PIT (60): 6 meaningful pars, rest padding.
        let reply = session.query(&fx_type(2).unwrap()).await.unwrap();
        let loaded = parse_fx_type(&reply.args).unwrap();
        assert_eq!(loaded, 60);
        assert_eq!(fx_param_count(loaded), Some(6));
        assert!(fx_param_def(loaded, 7).is_none());

        // Labeled pars round-trip bit-exact like generic pars (baseline T8).
        session
            .send(&fx_param(1, 0).unwrap(), vec![OscType::Float(0.5)])
            .await
            .unwrap();
        let back = session.query(&fx_param(1, 0).unwrap()).await.unwrap();
        assert_eq!(osc::fader_value(&back).unwrap(), 0.5);
        session
            .send(&fx_param(2, 2).unwrap(), vec![OscType::Float(0.25)])
            .await
            .unwrap();
        let back = session.query(&fx_param(2, 2).unwrap()).await.unwrap();
        assert_eq!(osc::fader_value(&back).unwrap(), 0.25);
        // Untouched labeled pars read back zero.
        let back = session.query(&fx_param(1, 5).unwrap()).await.unwrap();
        assert_eq!(osc::fader_value(&back).unwrap(), 0.0);
    }

    #[test]
    fn tap_targets_cover_confident_delays_only() {
        // Wired: DLY par 2, combos + taps + MODD par 1, all 1..3000 ms.
        let dly = fx_tap_target(10).unwrap();
        assert_eq!(
            (dly.par, dly.min_ms, dly.max_ms, dly.subdiv),
            (2, 1.0, 3000.0, true)
        );
        for index in [11u8, 12, 22, 25, 26, 27] {
            let t = fx_tap_target(index).unwrap();
            assert_eq!((t.par, t.min_ms, t.max_ms, t.subdiv), (1, 1.0, 3000.0, true));
        }
        // No tap: reverbs, modulation, and inferred-range pitch delays.
        for index in [0u8, 1, 7, 13, 14, 16, 34, 59, 60, 15, 99] {
            assert!(fx_tap_target(index).is_none(), "index {index}");
        }
    }

    #[test]
    fn tap_bpm_needs_four_taps_in_window() {
        assert!((tap_bpm_from_intervals_ms(&[500.0, 500.0, 500.0]).unwrap() - 120.0).abs() < 1e-9);
        assert!((tap_bpm_from_intervals_ms(&[250.0, 250.0, 250.0]).unwrap() - 240.0).abs() < 1e-9);
        assert!(tap_bpm_from_intervals_ms(&[500.0, 500.0]).is_none());
        assert!(tap_bpm_from_intervals_ms(&[]).is_none());
        assert!(tap_bpm_from_intervals_ms(&[0.0, 500.0, 500.0]).is_none());
        assert!(tap_bpm_from_intervals_ms(&[50.0, 50.0, 50.0]).is_none()); // >300 BPM
        assert!(tap_bpm_from_intervals_ms(&[5000.0, 5000.0, 5000.0]).is_none()); // <20 BPM
        // Quarter vs dotted-eighth at 120 BPM: 500 ms vs 375 ms.
        assert!((tap_delay_ms(120.0, TapSubdiv::Quarter) - 500.0).abs() < 1e-9);
        assert!((tap_delay_ms(120.0, TapSubdiv::Dotted8th) - 375.0).abs() < 1e-9);
        // Linear ms→01 over 1..3000, clamped; degenerate ranges stay safe.
        assert!((tap_ms_to_normalized(500.0, 1.0, 3000.0) - 499.0 / 2999.0) < 1e-6);
        assert_eq!(tap_ms_to_normalized(1.0, 1.0, 3000.0), 0.0);
        assert_eq!(tap_ms_to_normalized(3000.0, 1.0, 3000.0), 1.0);
        assert_eq!(tap_ms_to_normalized(9999.0, 1.0, 3000.0), 1.0);
        assert_eq!(tap_ms_to_normalized(0.5, 1.0, 3000.0), 0.0);
        assert_eq!(tap_ms_to_normalized(f64::NAN, 1.0, 3000.0), 0.0);
    }

    /// Loopback: a fake serving a delay type + time par proves the tap path
    /// — tap target par writes round-trip bit-exact via `set_fx_param`.
    #[tokio::test]
    async fn loopback_tap_target_par_roundtrip() {
        use openfader_transport::{osc, OscSession};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex as StdMutex};
        use tokio::net::UdpSocket;

        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        let pars: Arc<StdMutex<HashMap<String, f32>>> = Arc::new(StdMutex::new(HashMap::new()));
        let worker = socket.clone();
        let wpars = pars.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    let parts: Vec<&str> = msg.addr.split('/').collect();
                    if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                        // Fake serves DLY (10) on every slot.
                        if msg.args.is_empty() {
                            let bytes =
                                osc::encode_message(&msg.addr, vec![OscType::Int(10)]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                        continue;
                    }
                    if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                        match msg.args.first() {
                            Some(OscType::Float(f)) => {
                                wpars.lock().unwrap().insert(msg.addr.clone(), *f);
                            }
                            _ => {
                                let v = wpars
                                    .lock()
                                    .unwrap()
                                    .get(&msg.addr)
                                    .copied()
                                    .unwrap_or(0.0);
                                let bytes = osc::encode_message(
                                    &msg.addr,
                                    vec![OscType::Float(v)],
                                )
                                .unwrap();
                                let _ = worker.send_to(&bytes, peer).await;
                            }
                        }
                    }
                }
            }
        });

        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(addr).await.unwrap();
        let session = OscSession::from_connected_socket(sock, None);

        // Fake type query → DLY (10) → tap target par 2 (0-based index 1).
        let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
        let loaded = parse_fx_type(&reply.args).unwrap();
        assert_eq!(loaded, 10);
        let target = fx_tap_target(loaded).unwrap();
        assert_eq!(target.par, 2);
        // 120 BPM quarter → 500 ms → linear 01 → bit-exact roundtrip.
        let ms = tap_delay_ms(120.0, TapSubdiv::Quarter);
        assert!((ms - 500.0).abs() < 1e-9);
        let v01 = tap_ms_to_normalized(ms, target.min_ms, target.max_ms);
        let idx = target.par - 1; // wire par is 1-based; driver index 0-based
        session
            .send(&fx_param(1, idx).unwrap(), vec![OscType::Float(v01)])
            .await
            .unwrap();
        let back = session.query(&fx_param(1, idx).unwrap()).await.unwrap();
        assert_eq!(osc::fader_value(&back).unwrap(), v01);
        // Dotted-eighth lands lower on the same fader, still exact.
        let vdot = tap_ms_to_normalized(
            tap_delay_ms(120.0, TapSubdiv::Dotted8th),
            target.min_ms,
            target.max_ms,
        );
        assert!(vdot < v01);
        session
            .send(&fx_param(1, idx).unwrap(), vec![OscType::Float(vdot)])
            .await
            .unwrap();
        let back = session.query(&fx_param(1, idx).unwrap()).await.unwrap();
        assert_eq!(osc::fader_value(&back).unwrap(), vdot);
    }

    /// Loopback: a fake serving the hardware-proven mix — INT enum/switch
    /// pars (D/FL par 02/08/09 = 0/1/1, VRM par 13 = 0), float continuous
    /// pars, and float-NaN padding past each effect's own count — proves
    /// the read path: int decode per def, NaN rejection, and bit-exact
    /// float roundtrips.
    #[tokio::test]
    async fn loopback_mixed_int_float_pars_with_nan_padding() {
        use openfader_transport::{osc, OscSession};
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex as StdMutex};
        use tokio::net::UdpSocket;

        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let addr = socket.local_addr().unwrap();
        // Verbatim wire storage so ints stay ints and NaN stays NaN.
        let pars: Arc<StdMutex<HashMap<String, OscType>>> =
            Arc::new(StdMutex::new(HashMap::from([
                // Slot 3 loads D/FL (26): live shape par 02/08/09 = INT 0/1/1.
                ("/fx/3/par/01".to_string(), OscType::Float(0.5)),
                ("/fx/3/par/02".to_string(), OscType::Int(0)),
                ("/fx/3/par/08".to_string(), OscType::Int(1)),
                ("/fx/3/par/09".to_string(), OscType::Int(1)),
                ("/fx/3/par/13".to_string(), OscType::Float(0.75)),
                ("/fx/3/par/14".to_string(), OscType::Float(f32::NAN)),
                // Slot 1 loads VRM (7): par 13 Freeze = INT 0, par 14+ NaN.
                ("/fx/1/par/13".to_string(), OscType::Int(0)),
                ("/fx/1/par/14".to_string(), OscType::Float(f32::NAN)),
            ])));
        let types: Arc<StdMutex<HashMap<u8, i32>>> =
            Arc::new(StdMutex::new(HashMap::from([(1u8, 7i32), (3u8, 26i32)])));
        let worker = socket.clone();
        let wpars = pars.clone();
        let wtypes = types.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    let parts: Vec<&str> = msg.addr.split('/').collect();
                    if parts.len() == 4 && parts[1] == "fx" && parts[3] == "type" {
                        if msg.args.is_empty() {
                            let t = wtypes
                                .lock()
                                .unwrap()
                                .get(&parts[2].parse::<u8>().unwrap_or(0))
                                .copied()
                                .unwrap_or(0);
                            let bytes =
                                osc::encode_message(&msg.addr, vec![OscType::Int(t)]).unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                        continue;
                    }
                    if parts.len() == 5 && parts[1] == "fx" && parts[3] == "par" {
                        match msg.args.first() {
                            // Store sends verbatim (int or float) — no coercion.
                            Some(arg @ (OscType::Float(_) | OscType::Int(_))) => {
                                wpars.lock().unwrap().insert(msg.addr.clone(), arg.clone());
                            }
                            // Queries answer the stored wire value verbatim;
                            // untouched pars read back NaN, like live padding.
                            _ => {
                                let v = wpars
                                    .lock()
                                    .unwrap()
                                    .get(&msg.addr)
                                    .cloned()
                                    .unwrap_or(OscType::Float(f32::NAN));
                                let bytes = osc::encode_message(&msg.addr, vec![v]).unwrap();
                                let _ = worker.send_to(&bytes, peer).await;
                            }
                        }
                    }
                }
            }
        });

        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(addr).await.unwrap();
        let session = OscSession::from_connected_socket(sock, None);

        // Slot 3 loads D/FL (26): int pars decode per def.
        let reply = session.query(&fx_type(3).unwrap()).await.unwrap();
        let loaded = parse_fx_type(&reply.args).unwrap();
        assert_eq!(loaded, 26);
        let def = |par| fx_param_def(loaded, par);
        let back = session.query(&fx_param(3, 1).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, def(2).as_ref()).unwrap(), 0.0);
        let back = session.query(&fx_param(3, 7).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, def(8).as_ref()).unwrap(), 1.0);
        let back = session.query(&fx_param(3, 8).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, def(9).as_ref()).unwrap(), 1.0);
        let back = session.query(&fx_param(3, 0).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, def(1).as_ref()).unwrap(), 0.5);
        // Padding past the effect's own count replies NaN → absent, never a value.
        let back = session.query(&fx_param(3, 13).unwrap()).await.unwrap();
        let err = parse_fx_param(&back.args, def(14).as_ref()).unwrap_err();
        assert!(err.to_string().contains("absent"), "unexpected: {err}");

        // Slot 1 loads VRM (7): Freeze int 0 → 0.0, par 14 NaN → absent.
        let reply = session.query(&fx_type(1).unwrap()).await.unwrap();
        let loaded = parse_fx_type(&reply.args).unwrap();
        assert_eq!(loaded, 7);
        let back = session.query(&fx_param(1, 12).unwrap()).await.unwrap();
        let freeze = fx_param_def(loaded, 13);
        assert_eq!(parse_fx_param(&back.args, freeze.as_ref()).unwrap(), 0.0);
        let back = session.query(&fx_param(1, 13).unwrap()).await.unwrap();
        let err = parse_fx_param(&back.args, fx_param_def(loaded, 14).as_ref()).unwrap_err();
        assert!(err.to_string().contains("absent"), "unexpected: {err}");

        // Bit-exact roundtrips: float par stores verbatim, int switch too.
        session
            .send(&fx_param(3, 0).unwrap(), vec![OscType::Float(0.5)])
            .await
            .unwrap();
        let back = session.query(&fx_param(3, 0).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, def(1).as_ref()).unwrap(), 0.5);
        session
            .send(&fx_param(1, 12).unwrap(), vec![OscType::Int(1)])
            .await
            .unwrap();
        let back = session.query(&fx_param(1, 12).unwrap()).await.unwrap();
        assert_eq!(parse_fx_param(&back.args, freeze.as_ref()).unwrap(), 1.0);
    }
}
