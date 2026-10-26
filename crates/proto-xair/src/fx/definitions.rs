//! Shared parameter descriptors and table constructors.
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
pub(super) const fn p(
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
pub(super) const fn q(
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
pub(super) const fn e(
    par: u8,
    label: &'static str,
    options: &'static [&'static str],
) -> FxParamDef {
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
pub(super) const fn ie(
    par: u8,
    label: &'static str,
    options: &'static [&'static str],
) -> FxParamDef {
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

pub(super) const OFF_ON: &[&str] = &["OFF", "ON"];
pub(super) const ZERO_ONE: &[&str] = &["0", "1"];
/// Stereo-delay note divisions (DLY FactorL/R, 3TAP/4TAP FactorN).
pub(super) const NOTE_DIV9: &[&str] = &["1/4", "3/8", "1/2", "2/3", "1", "4/3", "3/2", "2", "3"];
/// Combination-effect delay patterns (D/RV, D/CR, D/FL par 02).
pub(super) const PATTERN14: &[&str] = &[
    "1/4", "1/3", "3/8", "1/2", "2/3", "3/4", "1", "1/4X", "1/3X", "3/8X", "1/2X", "2/3X", "3/4X",
    "1X",
];
pub(super) const MODD_FACTOR: &[&str] = &["1", "1/2", "2/3", "3/2"];
pub(super) const MODD_SETUP: &[&str] = &["PAR", "SER"];
pub(super) const MODD_RTYPE: &[&str] = &["AMB", "CLUB", "HALL"];
pub(super) const VINTAGE_OUT: &[&str] = &["FRONT", "REAR"];
pub(super) const DLY_MODE: &[&str] = &["ST", "X", "M"];
pub(super) const FILT_MODE: &[&str] = &["LP", "HP", "BP", "NO"];
pub(super) const FILT_WAVE: &[&str] = &["TRI", "SIN", "SAW", "SAW-", "RMP", "SQU", "RND"];
pub(super) const SUB_RANGE: &[&str] = &["LO", "MID", "HI"];
pub(super) const P1A_LOFREQ: &[&str] = &["0", "30", "60", "100"];
pub(super) const P1A_HIFREQ: &[&str] = &["3k", "4k", "5k", "8k", "10k", "12k", "16k"];
pub(super) const P1A_ATTFR: &[&str] = &["5k", "10k", "20k"];
pub(super) const PQ5_LOFREQ: &[&str] = &["200", "300", "500", "700", "1000"];
pub(super) const PQ5_MIDFREQ: &[&str] = &[
    "200", "300", "500", "700", "1k", "1k5", "2k", "3k", "4k", "5k", "7k",
];
pub(super) const PQ5_HIFREQ: &[&str] = &["1k5", "2k", "3k", "4k", "5k"];
pub(super) const ULC_RATIO: &[&str] = &["4", "8", "12", "20", "ALL"];
pub(super) const CMB_SOLO: &[&str] = &["OFF", "Bd1", "Bd2", "Bd3", "Bd4", "Bd5"];
pub(super) const CMB_SLOPE: &[&str] = &["12", "48"];
pub(super) const CMB_RATIO: &[&str] = &[
    "1.1", "1.2", "1.3", "1.5", "1.7", "2", "2.5", "3", "3.5", "4", "5", "7", "10", "LIM",
];
pub(super) const CMB_METER: &[&str] = &["GR", "SBC", "PEAK"];
pub(super) const LEC_MODE: &[&str] = &["COMP", "LIM"];
pub(super) const DES_VOICE: &[&str] = &["FEM", "MALE"];
pub(super) const DES_MODE: &[&str] = &["ST", "M-S"];
pub(super) const DIMC_MODE: &[&str] = &["M", "ST"];

/// Graphic-EQ band centres, shared by GEQ/TEQ (+dual): 20 Hz … 20 kHz.
pub const GEQ_BANDS: [&str; 31] = [
    "20", "25", "31.5", "40", "50", "63", "80", "100", "125", "160", "200", "250", "315", "400",
    "500", "630", "800", "1k", "1k25", "1k6", "2k", "2k5", "3k15", "4k", "5k", "6k3", "8k", "10k",
    "12k5", "16k", "20k",
];
