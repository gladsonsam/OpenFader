//! X32 FX effect catalogue: 61 documented algorithms with per-parameter
//! metadata (Maillot v4.09-02, Effects Parameters pp88-118; enum appendix
//! p166 for the slot domain tables, which live in `super::FX_TYPES_1_4` /
//! `FX_TYPES_5_8` and are preserved verbatim).
//!
//! Wire types come from the doc's own `Parameters` column (`f` = normalized
//! float `0..1`, `i` = OSC int enum/switch). Ranges and option lists are the
//! doc's `Type & Range` column; `linf`/`logf` select the documented fader
//! law. Units are absent from the doc, so none are advertised. Effect full
//! names are the appendix preset names.
//!
//! Known doc gaps, transcribed honestly rather than filled in: RPLT/ROOM
//! par 15+16 both read "Echo Feed L", CHAM par 15+16 both read
//! "Reflection Gain L", LEC/LEC2 repeat the "Gain" label, and FAC1M/FAC2
//! par 08 is an int with no documented label (placeholder "Par 08").
//! Canonical id for the wave designer is WAVD (both type tables); the
//! parameter page heads it WAV. TEQ's appendix name has a doc typo
//! ("Strereo"); the catalogue uses "Stereo TrueEQ".
//!
//! Independent-emulator conflicts (kept as evidence, never patched around):
//! the old emulator swaps 16 type-table entries (GEQ/TEQ2 etc., see
//! `fx_type_index` docs) and models ENH2 with 18 params (extra Spread A/B)
//! and PIT2 with 12 (no Hi Cut) where the doc lists 16 and 13 consecutive
//! rows. Catalogue counts follow the doc rows.

/// Documented fader law / wire kind of one FX parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxParamKind {
    /// `linf`: continuous, OSC float, normalized `0..1`.
    Lin,
    /// `logf`: continuous, OSC float, normalized `0..1`.
    Log,
    /// `enum`: discrete, OSC int option index.
    Enum,
}

/// One documented FX parameter: 1-based `par`, display `label`, wire
/// `kind`, engineering `min`/`max` (display only; the wire stays
/// normalized, discrete options as `options`).
#[derive(Clone, Copy, Debug)]
pub struct FxParamDef {
    pub par: u8,
    pub label: &'static str,
    pub kind: FxParamKind,
    pub min: f32,
    pub max: f32,
    pub options: Option<&'static [&'static str]>,
}

pub(super) const fn lin(par: u8, label: &'static str, min: f32, max: f32) -> FxParamDef {
    FxParamDef {
        par,
        label,
        kind: FxParamKind::Lin,
        min,
        max,
        options: None,
    }
}

pub(super) const fn log(par: u8, label: &'static str, min: f32, max: f32) -> FxParamDef {
    FxParamDef {
        par,
        label,
        kind: FxParamKind::Log,
        min,
        max,
        options: None,
    }
}

pub(super) const fn en(
    par: u8,
    label: &'static str,
    options: &'static [&'static str],
) -> FxParamDef {
    FxParamDef {
        par,
        label,
        kind: FxParamKind::Enum,
        min: 0.0,
        max: (options.len() - 1) as f32,
        options: Some(options),
    }
}

pub const OFF_ON: &[&str] = &["OFF", "ON"];
pub const FACTOR9: &[&str] = &["1/4", "3/8", "1/2", "2/3", "1", "4/3", "3/2", "2", "3"];
pub const PATTERN14: &[&str] = &[
    "1/4", "1/3", "3/8", "1/2", "2/3", "3/4", "1", "1/4X", "1/3X", "3/8X", "1/2X", "2/3X", "3/4X",
    "1X",
];
pub const FRONT_REAR: &[&str] = &["FRONT", "REAR"];
pub const M_ST: &[&str] = &["M", "ST"];
pub const LP_HP_BP_NO: &[&str] = &["LP", "HP", "BP", "NO"];
pub const WAVE7: &[&str] = &["TRI", "SIN", "SAW", "SAW-", "RMP", "SQU", "RND"];
pub const POLE24: &[&str] = &["2POL", "4POL"];
pub const RUN_STOP: &[&str] = &["RUN", "STOP"];
pub const SLOW_FAST: &[&str] = &["SLOW", "FAST"];
pub const LO_MID_HI: &[&str] = &["LO", "MID", "HI"];
pub const FEM_MALE: &[&str] = &["FEM", "MALE"];
pub const ST_MS: &[&str] = &["ST", "MS"];
pub const ST_MS_SLASH: &[&str] = &["ST", "M/S"];
pub const MS_DIV: &[&str] = &["1", "1/2", "2/3", "3/2"];
pub const PAR_SER: &[&str] = &["PAR", "SER"];
pub const AMB_CLUB_HALL: &[&str] = &["AMB", "CLUB", "HALL"];
pub const BAND_SOLO: &[&str] = &["OFF", "Bd1", "Bd2", "Bd3", "Bd4", "Bd5"];
pub const XOVER_SLOPE: &[&str] = &["12", "48"];
pub const RATIO14: &[&str] = &[
    "1.1", "1.2", "1.3", "1.5", "1.7", "2", "2.5", "3", "3.5", "4", "5", "7", "10", "LIM",
];
pub const LOCK01: &[&str] = &["0", "1"];
pub const GR_SBC_PEAK: &[&str] = &["GR", "SBC", "PEAK"];
pub const LO_FREQ4: &[&str] = &["20", "30", "60", "100"];
pub const HI_FREQ7: &[&str] = &["3k", "4k", "5k", "8k", "10k", "12k", "16k"];
pub const HI_FREQ3: &[&str] = &["5k", "10k", "20k"];
pub const PQ_LO5: &[&str] = &["200", "300", "500", "700", "1000"];
pub const PQ_MID11: &[&str] = &[
    "200", "300", "500", "700", "1k", "1k5", "2k", "3k", "4k", "5k", "7k",
];
pub const PQ_HI5: &[&str] = &["1k5", "2k", "3k", "4k", "5k"];
pub const COMP_LIM: &[&str] = &["COMP", "LIM"];
pub const RATIO5: &[&str] = &["4", "8", "12", "20", "ALL"];
pub const ST_X_M: &[&str] = &["ST", "X", "M"];

/// Parameter storage behind one catalogue effect: an explicit doc table,
/// or the graphic-EQ band rule (31 documented `linf [-15..+15]` bands plus
/// a master per block; dual blocks for GEQ2/TEQ2).
#[derive(Clone, Copy, Debug)]
pub enum EffectParams {
    Table(&'static [FxParamDef]),
    GraphicEq { dual: bool },
}

/// One catalogue effect: stable global `code`, appendix `full` name, params.
#[derive(Clone, Copy, Debug)]
pub struct FxEffectDef {
    pub code: &'static str,
    pub full: &'static str,
    pub params: EffectParams,
}

const HALL: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.2, 5.0),
    lin(3, "Size", 2.0, 100.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 1.0, 30.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Bass Multi", 0.5, 2.0),
    lin(10, "Spread", 0.0, 50.0),
    lin(11, "Shape", 0.0, 250.0),
    lin(12, "Mod Speed", 0.0, 100.0),
];
const AMBI: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.2, 7.3),
    lin(3, "Size", 2.0, 100.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 1.0, 30.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    lin(9, "Modulate", 0.0, 100.0),
    lin(10, "Tail Gain", 0.0, 100.0),
];
const RPLT: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.3, 29.0),
    lin(3, "Size", 4.0, 39.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Bass Multi", 0.25, 4.0),
    lin(10, "Spread", 0.0, 50.0),
    lin(11, "Attack", 0.0, 100.0),
    lin(12, "Spin", 0.0, 100.0),
    lin(13, "Echo L", 0.0, 1200.0),
    lin(14, "Echo R", 0.0, 1200.0),
    lin(15, "Echo Feed L", -100.0, 100.0),
    lin(16, "Echo Feed L", -100.0, 100.0),
];
const ROOM: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.3, 29.0),
    lin(3, "Size", 4.0, 76.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Bass Multi", 0.25, 4.0),
    lin(10, "Spread", 0.0, 50.0),
    lin(11, "Shape", 0.0, 250.0),
    lin(12, "Spin", 0.0, 100.0),
    lin(13, "Echo L", 0.0, 1200.0),
    lin(14, "Echo R", 0.0, 1200.0),
    lin(15, "Echo Feed L", -100.0, 100.0),
    lin(16, "Echo Feed L", -100.0, 100.0),
];
const CHAM: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.3, 29.0),
    lin(3, "Size", 4.0, 72.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Bass Multi", 0.25, 4.0),
    lin(10, "Spread", 0.0, 50.0),
    lin(11, "Shape", 0.0, 250.0),
    lin(12, "Spin", 0.0, 100.0),
    lin(13, "Reflection L", 0.0, 500.0),
    lin(14, "Reflection R", 0.0, 500.0),
    lin(15, "Reflection Gain L", 0.0, 100.0),
    lin(16, "Reflection Gain L", 0.0, 100.0),
];
const PLAT: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 0.5, 10.0),
    lin(3, "Size", 2.0, 100.0),
    log(4, "Damping", 1000.0, 20000.0),
    lin(5, "Diffuse", 1.0, 30.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Bass Multi", 0.5, 2.0),
    log(10, "Xover", 10.0, 500.0),
    lin(11, "Mod Depth", 1.0, 50.0),
    lin(12, "Mod Speed", 0.0, 100.0),
];
const VREV: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 120.0),
    lin(2, "Decay", 0.4, 4.5),
    lin(3, "Modulate", 0.0, 100.0),
    en(4, "Vintage", OFF_ON),
    en(5, "Position", FRONT_REAR),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 10000.0, 20000.0),
    log(9, "Lo Multiply", 0.5, 2.0),
    log(10, "Hi Multiply", 0.25, 1.0),
];
const VRM: &[FxParamDef] = &[
    lin(1, "Reverb Delay", 0.0, 200.0),
    log(2, "Decay", 0.1, 20.0),
    lin(3, "Size", 2.0, 100.0),
    lin(4, "Density", 1.0, 30.0),
    lin(5, "ER Level", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Multiply", 0.1, 10.0),
    log(8, "Hi Multiply", 0.1, 10.0),
    log(9, "Lo Cut", 10.0, 500.0),
    log(10, "Hi Cut", 200.0, 20000.0),
    lin(11, "ER Left", 0.0, 200.0),
    lin(12, "ER Right", 0.0, 200.0),
    en(13, "Freeze", OFF_ON),
];
const GATE: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    lin(2, "Decay", 140.0, 1000.0),
    lin(3, "Attack", 0.0, 30.0),
    lin(4, "Density", 1.0, 50.0),
    lin(5, "Spread", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Shv Freq", 200.0, 20000.0),
    lin(9, "Hi Shv Gain", -30.0, 0.0),
    lin(10, "Diffuse", 1.0, 30.0),
];
const RVRS: &[FxParamDef] = &[
    lin(1, "Pre Delay", 0.0, 200.0),
    log(2, "Decay", 140.0, 1000.0),
    lin(3, "Rise", 0.0, 50.0),
    lin(4, "Diffuse", 1.0, 30.0),
    lin(5, "Spread", 0.0, 100.0),
    lin(6, "Level", -12.0, 12.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Shv Freq", 200.0, 20000.0),
    lin(9, "Hi Shv Gain", -30.0, 0.0),
];
const DLY: &[FxParamDef] = &[
    lin(1, "Mix", 0.0, 100.0),
    lin(2, "Time", 1.0, 3000.0),
    en(3, "Mode", ST_X_M),
    en(4, "Factor L", FACTOR9),
    en(5, "Factor R", FACTOR9),
    lin(6, "Offset L/R", -100.0, 100.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    log(9, "Feed Lo Cut", 10.0, 500.0),
    lin(10, "Feed Left", 1.0, 100.0),
    lin(11, "Feed Right", 1.0, 100.0),
    log(12, "Feed Hi Cut", 200.0, 20000.0),
];
const TAP3: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    lin(2, "Gain Base", 0.0, 100.0),
    lin(3, "Pan Base", -100.0, 100.0),
    lin(4, "Feedback", 0.0, 100.0),
    log(5, "Lo Cut", 10.0, 500.0),
    log(6, "Hi Cut", 200.0, 20000.0),
    en(7, "Factor A", FACTOR9),
    lin(8, "Gain A", 0.0, 100.0),
    lin(9, "Pan A", -100.0, 100.0),
    en(10, "Factor B", FACTOR9),
    lin(11, "Gain B", 0.0, 100.0),
    lin(12, "Pan B", -100.0, 100.0),
    en(13, "Cross Feed", OFF_ON),
    en(14, "Mono", OFF_ON),
    en(15, "Dry", OFF_ON),
];
const TAP4: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    lin(2, "Gain Base", 0.0, 100.0),
    lin(3, "Feedback", 0.0, 100.0),
    log(4, "Lo Cut", 10.0, 500.0),
    log(5, "Hi Cut", 200.0, 20000.0),
    lin(6, "Spread", 0.0, 6.0),
    en(7, "Factor A", FACTOR9),
    lin(8, "Gain A", 0.0, 100.0),
    en(9, "Factor B", FACTOR9),
    lin(10, "Gain B", 0.0, 100.0),
    en(11, "Factor C", FACTOR9),
    lin(12, "Gain C", 0.0, 100.0),
    en(13, "Cross Feed", OFF_ON),
    en(14, "Mono", OFF_ON),
    en(15, "Dry", OFF_ON),
];
const CRS: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 5.0),
    lin(2, "Depth L", 0.0, 100.0),
    lin(3, "Depth R", 0.0, 100.0),
    log(4, "Delay L", 0.5, 50.0),
    log(5, "Delay R", 0.5, 50.0),
    lin(6, "Mix", 0.0, 100.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    lin(9, "Phase", 0.0, 180.0),
    lin(10, "Wave", 0.0, 100.0),
    lin(11, "Spread", 0.0, 100.0),
];
const FLNG: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 5.0),
    lin(2, "Depth L", 0.0, 100.0),
    lin(3, "Depth R", 0.0, 100.0),
    log(4, "Delay L", 0.5, 20.0),
    log(5, "Delay R", 0.5, 20.0),
    lin(6, "Mix", 0.0, 100.0),
    log(7, "Lo Cut", 10.0, 500.0),
    log(8, "Hi Cut", 200.0, 20000.0),
    lin(9, "Phase", 0.0, 180.0),
    log(10, "Feed Lo Cut", 10.0, 500.0),
    log(11, "Feed Hi Cut", 200.0, 20000.0),
    lin(12, "Feed", -90.0, 90.0),
];
const PHAS: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 5.0),
    lin(2, "Depth", 0.0, 100.0),
    lin(3, "Resonance", 0.0, 80.0),
    lin(4, "Base", 0.0, 50.0),
    lin(5, "Stages", 2.0, 12.0),
    lin(6, "Mix", 0.0, 100.0),
    lin(7, "Wave", -50.0, 50.0),
    lin(8, "Phase", 0.0, 180.0),
    lin(9, "Env. Modulation", -100.0, 100.0),
    log(10, "Attack", 10.0, 1000.0),
    log(11, "Hold", 1.0, 2000.0),
    log(12, "Release", 10.0, 1000.0),
];
const DIMC: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    en(2, "Mode", M_ST),
    en(3, "Dry", OFF_ON),
    en(4, "Mode 1", OFF_ON),
    en(5, "Mode 2", OFF_ON),
    en(6, "Mode 3", OFF_ON),
    en(7, "Mode 4", OFF_ON),
];
const FILT: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 20.0),
    lin(2, "Depth", 0.0, 100.0),
    lin(3, "Resonance", 0.0, 100.0),
    log(4, "Base", 20.0, 15000.0),
    en(5, "Mode", LP_HP_BP_NO),
    lin(6, "Mix", 0.0, 100.0),
    en(7, "Wave", WAVE7),
    lin(8, "Phase", 0.0, 180.0),
    lin(9, "Env. Modulation", -100.0, 100.0),
    log(10, "Attack", 10.0, 250.0),
    log(11, "Release", 10.0, 500.0),
    lin(12, "Drive", 0.0, 100.0),
    en(13, "4 Pole", POLE24),
    en(14, "Side Chain", OFF_ON),
];
const ROTA: &[FxParamDef] = &[
    log(1, "Lo Speed", 0.1, 4.0),
    log(2, "Hi Speed", 2.0, 10.0),
    lin(3, "Accelerate", 0.0, 100.0),
    lin(4, "Distance", 0.0, 100.0),
    lin(5, "Balance", -100.0, 100.0),
    lin(6, "Mix", 0.0, 100.0),
    en(7, "Stop", RUN_STOP),
    en(8, "Slow", SLOW_FAST),
];
const PAN: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 5.0),
    lin(2, "Phase", 0.0, 180.0),
    lin(3, "Wave", -50.0, 50.0),
    lin(4, "Depth", 0.0, 100.0),
    lin(5, "Env. Speed", 0.0, 100.0),
    lin(6, "Env. Depth", 0.0, 100.0),
    log(7, "Attack", 10.0, 1000.0),
    log(8, "Hold", 1.0, 2000.0),
    log(9, "Release", 10.0, 1000.0),
];
const SUB: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    en(2, "Range", LO_MID_HI),
    lin(3, "Dry", 0.0, 100.0),
    lin(4, "Octave -1", 0.0, 100.0),
    lin(5, "Octave -2", 0.0, 100.0),
    en(6, "Active", OFF_ON),
    en(7, "Range", LO_MID_HI),
    lin(8, "Dry", 0.0, 100.0),
    lin(9, "Octave -1", 0.0, 100.0),
    lin(10, "Octave -2", 0.0, 100.0),
];
const DRV_RV: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    en(2, "Pattern", PATTERN14),
    log(3, "Feed Hi Cut", 1000.0, 20000.0),
    lin(4, "Feedback", 0.0, 100.0),
    lin(5, "Cross Feed", 0.0, 100.0),
    lin(6, "Balance", -100.0, 100.0),
    lin(7, "Pre Delay", 0.0, 200.0),
    log(8, "Decay", 0.1, 5.0),
    lin(9, "Size", 2.0, 100.0),
    log(10, "Damping", 1000.0, 20000.0),
    log(11, "Lo Cut", 10.0, 500.0),
    lin(12, "Mix", 0.0, 100.0),
];
const CR_R: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 4.0),
    lin(2, "Depth", 0.0, 100.0),
    log(3, "Delay", 0.5, 50.0),
    lin(4, "Phase", 0.0, 180.0),
    lin(5, "Wave", 0.0, 100.0),
    lin(6, "Balance", -100.0, 100.0),
    lin(7, "Pre Delay", 0.0, 200.0),
    log(8, "Decay", 0.1, 5.0),
    lin(9, "Size", 2.0, 100.0),
    log(10, "Damping", 1000.0, 20000.0),
    log(11, "Lo Cut", 10.0, 500.0),
    lin(12, "Mix", 0.0, 100.0),
];
const FL_R: &[FxParamDef] = &[
    log(1, "Speed", 0.05, 4.0),
    lin(2, "Depth", 0.0, 100.0),
    log(3, "Delay", 0.5, 20.0),
    lin(4, "Phase", 0.0, 180.0),
    lin(5, "Feed", -90.0, 90.0),
    lin(6, "Balance", -100.0, 100.0),
    lin(7, "Pre Delay", 0.0, 200.0),
    log(8, "Decay", 0.1, 5.0),
    lin(9, "Size", 2.0, 100.0),
    log(10, "Damping", 1000.0, 20000.0),
    log(11, "Lo Cut", 10.0, 500.0),
    lin(12, "Mix", 0.0, 100.0),
];
const D_CR: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    en(2, "Pattern", PATTERN14),
    log(3, "Feed Hi Cut", 1000.0, 20000.0),
    lin(4, "Feedback", 0.0, 100.0),
    lin(5, "Cross Feed", 0.0, 100.0),
    lin(6, "Balance", -100.0, 100.0),
    log(7, "Speed", 0.05, 4.0),
    lin(8, "Depth", 0.0, 100.0),
    log(9, "Delay", 0.5, 50.0),
    lin(10, "Phase", 0.0, 180.0),
    lin(11, "Wave", 0.0, 100.0),
    lin(12, "Mix", 0.0, 100.0),
];
const D_FL: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    en(2, "Pattern", PATTERN14),
    log(3, "Feed Hi Cut", 1000.0, 20000.0),
    lin(4, "Feedback", 0.0, 100.0),
    lin(5, "Cross Feed", 0.0, 100.0),
    lin(6, "Balance", -100.0, 100.0),
    log(7, "Speed", 0.05, 4.0),
    lin(8, "Depth", 0.0, 100.0),
    log(9, "Delay", 0.5, 20.0),
    lin(10, "Phase", 0.0, 180.0),
    lin(11, "Feed", -90.0, 90.0),
    lin(12, "Mix", 0.0, 100.0),
];
const MODD: &[FxParamDef] = &[
    lin(1, "Time", 1.0, 3000.0),
    en(2, "Delay", MS_DIV),
    lin(3, "Feed", 0.0, 100.0),
    log(4, "Lo Cut", 10.0, 500.0),
    log(5, "Hi Cut", 200.0, 20000.0),
    lin(6, "Depth", 0.0, 100.0),
    log(7, "Rate", 0.05, 10.0),
    en(8, "Setup", PAR_SER),
    en(9, "Type", AMB_CLUB_HALL),
    lin(10, "Decay", 1.0, 10.0),
    log(11, "Damping", 1000.0, 20000.0),
    lin(12, "Balance", -100.0, 100.0),
    lin(13, "Mix", 0.0, 100.0),
];
const DES2: &[FxParamDef] = &[
    lin(1, "Lo Band A", 0.0, 50.0),
    lin(2, "Hi Band A", 0.0, 50.0),
    lin(3, "Lo Band B", 0.0, 50.0),
    lin(4, "Hi Band B", 0.0, 50.0),
    en(5, "Voice A", FEM_MALE),
    en(6, "Voice B", FEM_MALE),
];
const DES: &[FxParamDef] = &[
    lin(1, "Lo Band L", 0.0, 50.0),
    lin(2, "Hi Band L", 0.0, 50.0),
    lin(3, "Lo Band R", 0.0, 50.0),
    lin(4, "Hi Band R", 0.0, 50.0),
    en(5, "Voice", FEM_MALE),
    en(6, "Mode", ST_MS),
];
const P1A: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Gain", -12.0, 12.0),
    lin(3, "Lo Boost", 0.0, 10.0),
    en(4, "Lo Freq", LO_FREQ4),
    lin(5, "Lo Att", 0.0, 10.0),
    lin(6, "Hi Width", 0.0, 10.0),
    lin(7, "Hi Boost", 0.0, 10.0),
    en(8, "Hi Freq", HI_FREQ7),
    lin(9, "Alt Sel", 0.0, 10.0),
    en(10, "Hi Freq", HI_FREQ3),
    en(11, "Transformer", OFF_ON),
];
const P1A2: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    lin(2, "Gain A", -12.0, 12.0),
    lin(3, "Lo Boost A", 0.0, 10.0),
    en(4, "Lo Freq A", LO_FREQ4),
    lin(5, "Lo Att A", 0.0, 10.0),
    lin(6, "Hi Width A", 0.0, 10.0),
    lin(7, "Hi Boost A", 0.0, 10.0),
    en(8, "Hi Freq A", HI_FREQ7),
    lin(9, "Alt Sel A", 0.0, 10.0),
    en(10, "Hi Freq A", HI_FREQ3),
    en(11, "Transformer A", OFF_ON),
    en(12, "Active B", OFF_ON),
    lin(13, "Gain B", -12.0, 12.0),
    lin(14, "Lo Boost B", 0.0, 10.0),
    en(15, "Lo Freq B", LO_FREQ4),
    lin(16, "Lo Att B", 0.0, 10.0),
    lin(17, "Hi Width B", 0.0, 10.0),
    lin(18, "Hi Boost B", 0.0, 10.0),
    en(19, "Hi Freq B", HI_FREQ7),
    lin(20, "Alt Sel B", 0.0, 10.0),
    en(21, "Hi Freq B", HI_FREQ3),
    en(22, "Transformer B", OFF_ON),
];
const PQ5: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Gain", -12.0, 12.0),
    en(3, "Lo Freq", PQ_LO5),
    lin(4, "Lo Boost", 0.0, 10.0),
    en(5, "Mid Freq", PQ_MID11),
    lin(6, "Mid Boost", 0.0, 10.0),
    en(7, "Hi Freq", PQ_HI5),
    lin(8, "Hi Boost", 0.0, 10.0),
    en(9, "Transformer", OFF_ON),
];
const PQ5S: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    lin(2, "Gain A", -12.0, 12.0),
    en(3, "Lo Freq A", PQ_LO5),
    lin(4, "Lo Boost A", 0.0, 10.0),
    en(5, "Mid Freq A", PQ_MID11),
    lin(6, "Mid Boost A", 0.0, 10.0),
    en(7, "Hi Freq A", PQ_HI5),
    lin(8, "Hi Boost A", 0.0, 10.0),
    en(9, "Transformer A", OFF_ON),
    en(10, "Active B", OFF_ON),
    lin(11, "Gain B", -12.0, 12.0),
    en(12, "Lo Freq B", PQ_LO5),
    lin(13, "Lo Boost B", 0.0, 10.0),
    en(14, "Mid Freq B", PQ_MID11),
    lin(15, "Mid Boost B", 0.0, 10.0),
    en(16, "Hi Freq B", PQ_HI5),
    lin(17, "Hi Boost B", 0.0, 10.0),
    en(18, "Transformer B", OFF_ON),
];
const WAVD: &[FxParamDef] = &[
    lin(1, "Attack A", -100.0, 100.0),
    lin(2, "Sustain A", -100.0, 100.0),
    lin(3, "Gain A", -24.0, 24.0),
    lin(4, "Attack B", -100.0, 100.0),
    lin(5, "Sustain B", -100.0, 100.0),
    lin(6, "Gain B", -24.0, 24.0),
];
const LIM: &[FxParamDef] = &[
    lin(1, "Input Gain", 0.0, 18.0),
    lin(2, "Out Gain", -18.0, 0.0),
    lin(3, "Squeeze", 0.0, 100.0),
    lin(4, "Knee", 0.0, 10.0),
    log(5, "Attack", 0.05, 1.0),
    log(6, "Release", 20.0, 2000.0),
    en(7, "Stereo Link", OFF_ON),
    en(8, "Auto Gain", OFF_ON),
];
const CMB: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    en(2, "Band Solo", BAND_SOLO),
    lin(3, "Mix", 0.0, 100.0),
    lin(4, "Attack", 0.0, 19.0),
    log(5, "Release", 20.0, 3000.0),
    en(6, "Autorelease", OFF_ON),
    lin(7, "SBC speed", 0.0, 10.0),
    en(8, "SBC ON", OFF_ON),
    lin(9, "Xover", -50.0, 50.0),
    en(10, "Xover Slope", XOVER_SLOPE),
    en(11, "Ratio", RATIO14),
    lin(12, "Threshold", -40.0, 0.0),
    lin(13, "Gain", -10.0, 10.0),
    lin(14, "Band 1 Threshold", -10.0, 10.0),
    lin(15, "Band 1 Gain", -10.0, 10.0),
    en(16, "Band 1 Lock", LOCK01),
    lin(17, "Band 2 Threshold", -10.0, 10.0),
    lin(18, "Band 2 Gain", -10.0, 10.0),
    en(19, "Band 2 Lock", LOCK01),
    lin(20, "Band 3 Threshold", -10.0, 10.0),
    lin(21, "Band 3 Gain", -10.0, 10.0),
    en(22, "Band 3 Lock", LOCK01),
    lin(23, "Band 4 Threshold", -10.0, 10.0),
    lin(24, "Band 4 Gain", -10.0, 10.0),
    en(25, "Band 4 Lock", LOCK01),
    lin(26, "Band 5 Threshold", -10.0, 10.0),
    lin(27, "Band 5 Gain", -10.0, 10.0),
    en(28, "Band 5 Lock", LOCK01),
    en(29, "Meter Mode", GR_SBC_PEAK),
];
const CMB2: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    en(2, "Band Solo A", BAND_SOLO),
    lin(3, "Mix A", 0.0, 100.0),
    lin(4, "Attack A", 0.0, 19.0),
    log(5, "Release A", 20.0, 3000.0),
    en(6, "Autorelease A", OFF_ON),
    lin(7, "SBC speed A", 0.0, 10.0),
    en(8, "SBC ON A", OFF_ON),
    lin(9, "Xover A", -50.0, 50.0),
    en(10, "Xover Slope A", XOVER_SLOPE),
    en(11, "Ratio A", RATIO14),
    lin(12, "Threshold A", -40.0, 0.0),
    lin(13, "Gain A", -10.0, 10.0),
    lin(14, "Band 1 Threshold A", -10.0, 10.0),
    lin(15, "Band 1 Gain A", -10.0, 10.0),
    en(16, "Band 1 Lock A", LOCK01),
    lin(17, "Band 2 Threshold A", -10.0, 10.0),
    lin(18, "Band 2 Gain A", -10.0, 10.0),
    en(19, "Band 2 Lock A", LOCK01),
    lin(20, "Band 3 Threshold A", -10.0, 10.0),
    lin(21, "Band 3 Gain A", -10.0, 10.0),
    en(22, "Band 3 Lock A", LOCK01),
    lin(23, "Band 4 Threshold A", -10.0, 10.0),
    lin(24, "Band 4 Gain A", -10.0, 10.0),
    en(25, "Band 4 Lock A", LOCK01),
    lin(26, "Band 5 Threshold A", -10.0, 10.0),
    lin(27, "Band 5 Gain A", -10.0, 10.0),
    en(28, "Band 5 Lock A", LOCK01),
    en(29, "Meter Mode A", GR_SBC_PEAK),
    en(30, "Active B", OFF_ON),
    en(31, "Band Solo B", BAND_SOLO),
    lin(32, "Mix B", 0.0, 100.0),
    lin(33, "Attack B", 0.0, 19.0),
    log(34, "Release B", 20.0, 3000.0),
    en(35, "Autorelease B", OFF_ON),
    lin(36, "SBC speed B", 0.0, 10.0),
    en(37, "SBC ON B", OFF_ON),
    lin(38, "Xover B", -50.0, 50.0),
    en(39, "Xover Slope B", XOVER_SLOPE),
    en(40, "Ratio B", RATIO14),
    lin(41, "Threshold B", -40.0, 0.0),
    lin(42, "Gain B", -10.0, 10.0),
    lin(43, "Band 1 Threshold B", -10.0, 10.0),
    lin(44, "Band 1 Gain B", -10.0, 10.0),
    en(45, "Band 1 Lock B", LOCK01),
    lin(46, "Band 2 Threshold B", -10.0, 10.0),
    lin(47, "Band 2 Gain B", -10.0, 10.0),
    en(48, "Band 2 Lock B", LOCK01),
    lin(49, "Band 3 Threshold B", -10.0, 10.0),
    lin(50, "Band 3 Gain B", -10.0, 10.0),
    en(51, "Band 3 Lock B", LOCK01),
    lin(52, "Band 4 Threshold B", -10.0, 10.0),
    lin(53, "Band 4 Gain B", -10.0, 10.0),
    en(54, "Band 4 Lock B", LOCK01),
    lin(55, "Band 5 Threshold B", -10.0, 10.0),
    lin(56, "Band 5 Gain B", -10.0, 10.0),
    en(57, "Band 5 Lock B", LOCK01),
    en(58, "Meter Mode B", GR_SBC_PEAK),
];
const FAC: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Input Gain", -20.0, 0.0),
    lin(3, "Threshold", 0.0, 10.0),
    lin(4, "Time", 1.0, 6.0),
    lin(5, "Bias", 0.0, 100.0),
    lin(6, "Gain", -18.0, 6.0),
    lin(7, "Balance", -100.0, 100.0),
];
const FAC_DUAL: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Input Gain", -20.0, 0.0),
    lin(3, "Threshold", 0.0, 10.0),
    lin(4, "Time", 1.0, 6.0),
    lin(5, "Bias", 0.0, 100.0),
    lin(6, "Gain", -18.0, 6.0),
    lin(7, "Balance", -100.0, 100.0),
    en(8, "Par 08", OFF_ON),
    lin(9, "Input Gain", -20.0, 0.0),
    lin(10, "Threshold", 0.0, 10.0),
    lin(11, "Time", 1.0, 6.0),
    lin(12, "Bias", 0.0, 100.0),
    lin(13, "Gain", -18.0, 6.0),
    lin(14, "Balance", -100.0, 100.0),
];
const LEC: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Gain", 0.0, 100.0),
    lin(3, "Peak", 0.0, 100.0),
    en(4, "Mode", COMP_LIM),
    lin(5, "Gain", -18.0, 6.0),
];
const LEC2: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    lin(2, "Gain A", 0.0, 100.0),
    lin(3, "Peak A", 0.0, 100.0),
    en(4, "Mode A", COMP_LIM),
    lin(5, "Gain A", -18.0, 6.0),
    en(6, "Active B", OFF_ON),
    lin(7, "Gain B", 0.0, 100.0),
    lin(8, "Peak B", 0.0, 100.0),
    en(9, "Mode B", COMP_LIM),
    lin(10, "Gain B", -18.0, 6.0),
];
const ULC: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    lin(2, "Input Gain", -48.0, 0.0),
    lin(3, "Out Gain", -48.0, 0.0),
    lin(4, "Attack", 1.0, 7.0),
    lin(5, "Release", 1.0, 7.0),
    en(6, "Ratio", RATIO5),
];
const ULC2: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    lin(2, "Input Gain A", -48.0, 0.0),
    lin(3, "Out Gain A", -48.0, 0.0),
    lin(4, "Attack A", 1.0, 7.0),
    lin(5, "Release A", 1.0, 7.0),
    en(6, "Ratio A", RATIO5),
    en(7, "Active B", OFF_ON),
    lin(8, "Input Gain B", -48.0, 0.0),
    lin(9, "Out Gain B", -48.0, 0.0),
    lin(10, "Attack B", 1.0, 7.0),
    lin(11, "Release B", 1.0, 7.0),
    en(12, "Ratio B", RATIO5),
];
const ENH2: &[FxParamDef] = &[
    lin(1, "Out Gain A", -12.0, 12.0),
    lin(2, "Bass Gain A", 0.0, 100.0),
    lin(3, "Bass Freq A", 1.0, 50.0),
    lin(4, "Mid Gain A", 0.0, 100.0),
    lin(5, "Mid Q A", 1.0, 50.0),
    lin(6, "Hi Gain A", 0.0, 100.0),
    lin(7, "Hi Freq A", 1.0, 50.0),
    en(8, "Solo A", OFF_ON),
    lin(9, "Out Gain B", -12.0, 12.0),
    lin(10, "Bass Gain B", 0.0, 100.0),
    lin(11, "Bass Freq B", 1.0, 50.0),
    lin(12, "Mid Gain B", 0.0, 100.0),
    lin(13, "Mid Q B", 1.0, 50.0),
    lin(14, "Hi Gain B", 0.0, 100.0),
    lin(15, "Hi Freq B", 1.0, 50.0),
    en(16, "Solo B", OFF_ON),
];
const ENH: &[FxParamDef] = &[
    lin(1, "Out Gain", -12.0, 12.0),
    lin(2, "Spread", 0.0, 100.0),
    lin(3, "Bass Gain", 0.0, 100.0),
    lin(4, "Bass Freq", 1.0, 50.0),
    lin(5, "Mid Gain", 0.0, 100.0),
    lin(6, "Mid Q", 1.0, 50.0),
    lin(7, "Hi Gain", 0.0, 100.0),
    lin(8, "Hi Freq", 1.0, 50.0),
    en(9, "Solo", OFF_ON),
];
const EXC2: &[FxParamDef] = &[
    log(1, "Tune A", 1000.0, 10000.0),
    lin(2, "Peak A", 0.0, 100.0),
    lin(3, "Zero Fill A", 0.0, 100.0),
    lin(4, "Timbre A", -50.0, 50.0),
    lin(5, "Harmonics A", 0.0, 100.0),
    lin(6, "Mix A", 0.0, 100.0),
    en(7, "Solo A", OFF_ON),
    log(8, "Tune B", 1000.0, 10000.0),
    lin(9, "Peak B", 0.0, 100.0),
    lin(10, "Zero Fill B", 0.0, 100.0),
    lin(11, "Timbre B", -50.0, 50.0),
    lin(12, "Harmonics B", 0.0, 100.0),
    lin(13, "Mix B", 0.0, 100.0),
    en(14, "Solo B", OFF_ON),
];
const EXC: &[FxParamDef] = &[
    log(1, "Tune", 1000.0, 10000.0),
    lin(2, "Peak", 0.0, 100.0),
    lin(3, "Zero Fill", 0.0, 100.0),
    lin(4, "Timbre", -50.0, 50.0),
    lin(5, "Harmonics", 0.0, 100.0),
    lin(6, "Mix", 0.0, 100.0),
    en(7, "Solo", OFF_ON),
];
const IMG: &[FxParamDef] = &[
    lin(1, "Balance", -100.0, 100.0),
    lin(2, "Mono Pan", -100.0, 100.0),
    lin(3, "Stereo Pan", -100.0, 100.0),
    lin(4, "Shv Gain", 0.0, 12.0),
    log(5, "Shv Freq", 100.0, 1000.0),
    log(6, "Shv Q", 1.0, 10.0),
    lin(7, "Out Gain", -12.0, 12.0),
];
const EDI: &[FxParamDef] = &[
    en(1, "Active", OFF_ON),
    en(2, "Stereo Input", ST_MS_SLASH),
    en(3, "Stereo Output", ST_MS_SLASH),
    lin(4, "ST Spread", -50.0, 50.0),
    lin(5, "LMF Spread", -50.0, 50.0),
    lin(6, "Balance", -50.0, 50.0),
    lin(7, "Center Distance", -50.0, 50.0),
    lin(8, "Out Gain", -12.0, 12.0),
];
const SON: &[FxParamDef] = &[
    en(1, "Active A", OFF_ON),
    lin(2, "Lo Contour A", 0.0, 10.0),
    lin(3, "Process A", 0.0, 10.0),
    lin(4, "Out Gain A", -12.0, 12.0),
    en(5, "Active B", OFF_ON),
    lin(6, "Lo Contour B", 0.0, 10.0),
    lin(7, "Process B", 0.0, 10.0),
    lin(8, "Out Gain B", -12.0, 12.0),
];
const AMP2: &[FxParamDef] = &[
    lin(1, "Preamp A", 0.0, 10.0),
    lin(2, "Buzz A", 0.0, 10.0),
    lin(3, "Punch A", 0.0, 10.0),
    lin(4, "Crunch A", 0.0, 10.0),
    lin(5, "Drive A", 0.0, 10.0),
    lin(6, "Level A", 0.0, 10.0),
    lin(7, "Low A", 0.0, 10.0),
    lin(8, "High A", 0.0, 10.0),
    en(9, "Cabinet A", OFF_ON),
    lin(10, "Preamp B", 0.0, 10.0),
    lin(11, "Buzz B", 0.0, 10.0),
    lin(12, "Punch B", 0.0, 10.0),
    lin(13, "Crunch B", 0.0, 10.0),
    lin(14, "Drive B", 0.0, 10.0),
    lin(15, "Level B", 0.0, 10.0),
    lin(16, "Low B", 0.0, 10.0),
    lin(17, "High B", 0.0, 10.0),
    en(18, "Cabinet B", OFF_ON),
];
const AMP: &[FxParamDef] = &[
    lin(1, "Preamp", 0.0, 10.0),
    lin(2, "Buzz", 0.0, 10.0),
    lin(3, "Punch", 0.0, 10.0),
    lin(4, "Crunch", 0.0, 10.0),
    lin(5, "Drive", 0.0, 10.0),
    lin(6, "Level", 0.0, 10.0),
    lin(7, "Low", 0.0, 10.0),
    lin(8, "High", 0.0, 10.0),
    en(9, "Cabinet", OFF_ON),
];
const DRV2: &[FxParamDef] = &[
    lin(1, "Drive A", 0.0, 100.0),
    lin(2, "Even Har A", 0.0, 50.0),
    lin(3, "Odd Har A", 0.0, 50.0),
    lin(4, "Gain A", -12.0, 12.0),
    log(5, "Lo Cut A", 20.0, 200.0),
    log(6, "Hi Cut A", 4000.0, 20000.0),
    lin(7, "Lo Gain A", -12.0, 12.0),
    log(8, "Lo Freq A", 50.0, 400.0),
    lin(9, "Hi Gain A", -12.0, 12.0),
    log(10, "Hi Freq A", 1000.0, 10000.0),
    lin(11, "Drive B", 0.0, 100.0),
    lin(12, "Even Har B", 0.0, 50.0),
    lin(13, "Odd Har B", 0.0, 50.0),
    lin(14, "Gain B", -12.0, 12.0),
    log(15, "Lo Cut B", 20.0, 200.0),
    log(16, "Hi Cut B", 4000.0, 20000.0),
    lin(17, "Lo Gain B", -12.0, 12.0),
    log(18, "Lo Freq B", 50.0, 400.0),
    lin(19, "Hi Gain B", -12.0, 12.0),
    log(20, "Hi Freq B", 1000.0, 10000.0),
];
const DRV: &[FxParamDef] = &[
    lin(1, "Drive", 0.0, 100.0),
    lin(2, "Even Har", 0.0, 50.0),
    lin(3, "Odd Har", 0.0, 50.0),
    lin(4, "Gain", -12.0, 12.0),
    log(5, "Lo Cut", 20.0, 200.0),
    log(6, "Hi Cut", 4000.0, 20000.0),
    lin(7, "Lo Gain", -12.0, 12.0),
    log(8, "Lo Freq", 50.0, 400.0),
    lin(9, "Hi Gain", -12.0, 12.0),
    log(10, "Hi Freq", 1000.0, 10000.0),
];
const PIT2: &[FxParamDef] = &[
    lin(1, "Semitone 1", -12.0, 12.0),
    lin(2, "Cent 1", -50.0, 50.0),
    log(3, "Delay 1", 1.0, 500.0),
    lin(4, "Gain 1", 0.0, 100.0),
    lin(5, "Pan 1", -100.0, 100.0),
    lin(6, "Mix 1", 0.0, 100.0),
    lin(7, "Semitone 2", -12.0, 12.0),
    lin(8, "Cent 2", -50.0, 50.0),
    log(9, "Delay 2", 1.0, 500.0),
    lin(10, "Gain 2", 0.0, 100.0),
    lin(11, "Pan 2", -100.0, 100.0),
    lin(12, "Mix 2", 0.0, 100.0),
    log(13, "Hi Cut", 2000.0, 20000.0),
];
const PIT: &[FxParamDef] = &[
    lin(1, "Semitone", -12.0, 12.0),
    lin(2, "Cent", -50.0, 50.0),
    log(3, "Delay", 1.0, 500.0),
    lin(4, "Gain", 0.0, 100.0),
    lin(5, "Pan", -100.0, 100.0),
    lin(6, "Mix", 0.0, 100.0),
];
/// Full catalogue in slots-1..4 wire order (documented appendix order).
pub const EFFECTS: &[FxEffectDef] = &[
    FxEffectDef {
        code: "HALL",
        full: "Hall Reverb",
        params: EffectParams::Table(HALL),
    },
    FxEffectDef {
        code: "AMBI",
        full: "Ambiance",
        params: EffectParams::Table(AMBI),
    },
    FxEffectDef {
        code: "RPLT",
        full: "Rich Plate Reverb",
        params: EffectParams::Table(RPLT),
    },
    FxEffectDef {
        code: "ROOM",
        full: "Room Reverb",
        params: EffectParams::Table(ROOM),
    },
    FxEffectDef {
        code: "CHAM",
        full: "Chamber Reverb",
        params: EffectParams::Table(CHAM),
    },
    FxEffectDef {
        code: "PLAT",
        full: "Plate Reverb",
        params: EffectParams::Table(PLAT),
    },
    FxEffectDef {
        code: "VREV",
        full: "Vintage Reverb",
        params: EffectParams::Table(VREV),
    },
    FxEffectDef {
        code: "VRM",
        full: "Vintage room",
        params: EffectParams::Table(VRM),
    },
    FxEffectDef {
        code: "GATE",
        full: "Gated Reverb",
        params: EffectParams::Table(GATE),
    },
    FxEffectDef {
        code: "RVRS",
        full: "Reverse Reverb",
        params: EffectParams::Table(RVRS),
    },
    FxEffectDef {
        code: "DLY",
        full: "Stereo Delay",
        params: EffectParams::Table(DLY),
    },
    FxEffectDef {
        code: "3TAP",
        full: "3-Tap Delay",
        params: EffectParams::Table(TAP3),
    },
    FxEffectDef {
        code: "4TAP",
        full: "Rhythm Delay",
        params: EffectParams::Table(TAP4),
    },
    FxEffectDef {
        code: "CRS",
        full: "Stereo Chorus",
        params: EffectParams::Table(CRS),
    },
    FxEffectDef {
        code: "FLNG",
        full: "Stereo Flanger",
        params: EffectParams::Table(FLNG),
    },
    FxEffectDef {
        code: "PHAS",
        full: "Stereo Phaser",
        params: EffectParams::Table(PHAS),
    },
    FxEffectDef {
        code: "DIMC",
        full: "Dimension-C",
        params: EffectParams::Table(DIMC),
    },
    FxEffectDef {
        code: "FILT",
        full: "Mood Filter",
        params: EffectParams::Table(FILT),
    },
    FxEffectDef {
        code: "ROTA",
        full: "Rotary Speaker",
        params: EffectParams::Table(ROTA),
    },
    FxEffectDef {
        code: "PAN",
        full: "Tremolo/Panner",
        params: EffectParams::Table(PAN),
    },
    FxEffectDef {
        code: "SUB",
        full: "Suboctaver",
        params: EffectParams::Table(SUB),
    },
    FxEffectDef {
        code: "D/RV",
        full: "Delay+Chamber",
        params: EffectParams::Table(DRV_RV),
    },
    FxEffectDef {
        code: "CR/R",
        full: "Chorus+Chamber",
        params: EffectParams::Table(CR_R),
    },
    FxEffectDef {
        code: "FL/R",
        full: "Flanger+Chamber",
        params: EffectParams::Table(FL_R),
    },
    FxEffectDef {
        code: "D/CR",
        full: "Delay+Chorus",
        params: EffectParams::Table(D_CR),
    },
    FxEffectDef {
        code: "D/FL",
        full: "Delay+Flanger",
        params: EffectParams::Table(D_FL),
    },
    FxEffectDef {
        code: "MODD",
        full: "Modulation Delay",
        params: EffectParams::Table(MODD),
    },
    FxEffectDef {
        code: "GEQ2",
        full: "Dual Graphic EQ",
        params: EffectParams::GraphicEq { dual: true },
    },
    FxEffectDef {
        code: "GEQ",
        full: "Stereo Graphic EQ",
        params: EffectParams::GraphicEq { dual: false },
    },
    FxEffectDef {
        code: "TEQ2",
        full: "Dual TrueEQ",
        params: EffectParams::GraphicEq { dual: true },
    },
    FxEffectDef {
        code: "TEQ",
        full: "Stereo TrueEQ",
        params: EffectParams::GraphicEq { dual: false },
    },
    FxEffectDef {
        code: "DES2",
        full: "Dual DeEsser",
        params: EffectParams::Table(DES2),
    },
    FxEffectDef {
        code: "DES",
        full: "Stereo DeEsser",
        params: EffectParams::Table(DES),
    },
    FxEffectDef {
        code: "P1A",
        full: "Stereo Xtec EQ1",
        params: EffectParams::Table(P1A),
    },
    FxEffectDef {
        code: "P1A2",
        full: "Dual Xtec EQ1",
        params: EffectParams::Table(P1A2),
    },
    FxEffectDef {
        code: "PQ5",
        full: "Stereo Xtec EQ5",
        params: EffectParams::Table(PQ5),
    },
    FxEffectDef {
        code: "PQ5S",
        full: "Dual Xtec EQ5",
        params: EffectParams::Table(PQ5S),
    },
    FxEffectDef {
        code: "WAVD",
        full: "Wave Designer",
        params: EffectParams::Table(WAVD),
    },
    FxEffectDef {
        code: "LIM",
        full: "Precision Limiter",
        params: EffectParams::Table(LIM),
    },
    FxEffectDef {
        code: "CMB",
        full: "Combinator",
        params: EffectParams::Table(CMB),
    },
    FxEffectDef {
        code: "CMB2",
        full: "Dual Combinator",
        params: EffectParams::Table(CMB2),
    },
    FxEffectDef {
        code: "FAC",
        full: "Fair Comp",
        params: EffectParams::Table(FAC),
    },
    FxEffectDef {
        code: "FAC1M",
        full: "M/S Fair Comp",
        params: EffectParams::Table(FAC_DUAL),
    },
    FxEffectDef {
        code: "FAC2",
        full: "Dual Fair Comp",
        params: EffectParams::Table(FAC_DUAL),
    },
    FxEffectDef {
        code: "LEC",
        full: "Leisure Comp",
        params: EffectParams::Table(LEC),
    },
    FxEffectDef {
        code: "LEC2",
        full: "Dual Leisure Comp",
        params: EffectParams::Table(LEC2),
    },
    FxEffectDef {
        code: "ULC",
        full: "Ultimo Comp",
        params: EffectParams::Table(ULC),
    },
    FxEffectDef {
        code: "ULC2",
        full: "Dual Ultimo Comp",
        params: EffectParams::Table(ULC2),
    },
    FxEffectDef {
        code: "ENH2",
        full: "Dual Enhancer",
        params: EffectParams::Table(ENH2),
    },
    FxEffectDef {
        code: "ENH",
        full: "Stereo Enhancer",
        params: EffectParams::Table(ENH),
    },
    FxEffectDef {
        code: "EXC2",
        full: "Dual Exciter",
        params: EffectParams::Table(EXC2),
    },
    FxEffectDef {
        code: "EXC",
        full: "Stereo Exciter",
        params: EffectParams::Table(EXC),
    },
    FxEffectDef {
        code: "IMG",
        full: "Stereo Imager",
        params: EffectParams::Table(IMG),
    },
    FxEffectDef {
        code: "EDI",
        full: "Edison EX1",
        params: EffectParams::Table(EDI),
    },
    FxEffectDef {
        code: "SON",
        full: "Sound Maxer",
        params: EffectParams::Table(SON),
    },
    FxEffectDef {
        code: "AMP2",
        full: "Dual Guitar Amp",
        params: EffectParams::Table(AMP2),
    },
    FxEffectDef {
        code: "AMP",
        full: "Stereo Guitar Amp",
        params: EffectParams::Table(AMP),
    },
    FxEffectDef {
        code: "DRV2",
        full: "Dual Tube Stage",
        params: EffectParams::Table(DRV2),
    },
    FxEffectDef {
        code: "DRV",
        full: "Stereo Tube Stage",
        params: EffectParams::Table(DRV),
    },
    FxEffectDef {
        code: "PIT2",
        full: "Dual Pitch Shifter",
        params: EffectParams::Table(PIT2),
    },
    FxEffectDef {
        code: "PIT",
        full: "Stereo Pitch",
        params: EffectParams::Table(PIT),
    },
];

/// Look up a catalogue effect by stable global code.
pub fn effect_by_code(code: &str) -> Option<&'static FxEffectDef> {
    EFFECTS.iter().find(|e| e.code == code)
}

/// Documented parameter count for one catalogue effect.
pub fn param_count(code: &str) -> Option<usize> {
    let effect = effect_by_code(code)?;
    Some(match effect.params {
        EffectParams::Table(rows) => rows.len(),
        EffectParams::GraphicEq { dual } => {
            if dual {
                64
            } else {
                32
            }
        }
    })
}

/// Resolved parameter view: table rows borrow their static label, graphic-EQ
/// bands generate the documented `Eq Level` / `Master Level` labels.
#[derive(Clone, Debug)]
pub struct FxParamInfo {
    pub label: String,
    pub kind: FxParamKind,
    pub min: f32,
    pub max: f32,
    pub options: Option<&'static [&'static str]>,
}

fn eq_label(dual: bool, par: u8) -> Option<String> {
    let (block, band) = if dual {
        match par {
            1..=31 => ("A", par),
            32 => return Some("Master Level A".to_string()),
            33..=63 => ("B", par - 32),
            64 => return Some("Master Level B".to_string()),
            _ => return None,
        }
    } else {
        match par {
            1..=31 => ("L/R", par),
            32 => return Some("Master Level L/R".to_string()),
            _ => return None,
        }
    };
    Some(format!("Eq Level {block} {band}"))
}

/// Resolve one documented parameter (1-based `par`) of a catalogue effect.
pub fn param_info(code: &str, par: u8) -> Option<FxParamInfo> {
    let effect = effect_by_code(code)?;
    match effect.params {
        EffectParams::Table(rows) => {
            rows.iter()
                .find(|row| row.par == par)
                .map(|row| FxParamInfo {
                    label: row.label.to_string(),
                    kind: row.kind,
                    min: row.min,
                    max: row.max,
                    options: row.options,
                })
        }
        EffectParams::GraphicEq { dual } => eq_label(dual, par).map(|label| FxParamInfo {
            label,
            kind: FxParamKind::Lin,
            min: -15.0,
            max: 15.0,
            options: None,
        }),
    }
}
