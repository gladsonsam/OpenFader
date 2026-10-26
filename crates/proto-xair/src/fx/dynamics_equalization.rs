//! Dynamics equalization parameter tables.
use super::*;
pub(super) static DES: &[FxParamDef] = &[
    p(1, "Low(M)", "", "0", "50"),
    p(2, "High(M)", "", "0", "50"),
    p(3, "Low S", "", "0", "50"),
    p(4, "High S", "", "0", "50"),
    e(5, "Male", DES_VOICE),
    e(6, "M-S", DES_MODE),
];

pub(super) static DES2: &[FxParamDef] = &[
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
pub(super) static P1A: &[FxParamDef] = &[
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

pub(super) static PQ5: &[FxParamDef] = &[
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

pub(super) static WAVD: &[FxParamDef] = &[
    p(1, "AttackA", "%", "-100", "+100"),
    p(2, "SustainA", "%", "-100", "+100"),
    p(3, "OutGainA", "dB", "-24", "+24"),
    p(4, "AttackB", "%", "-100", "+100"),
    p(5, "SustainB", "%", "-100", "+100"),
    p(6, "OutGainB", "dB", "-24", "+24"),
];

pub(super) static LIM: &[FxParamDef] = &[
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
pub(super) static CMB: &[FxParamDef] = &[
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
pub(super) static FAC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    // Range conflict: X-Air −20…0 vs X32 doc −20…+20; X-Air preferred.
    q(2, "InGain", "dB", "-20", "0"),
    p(3, "Threshold", "", "0", "10"),
    p(4, "Time", "", "1", "6"),
    p(5, "DCBias", "%", "0", "100"),
    p(6, "OutGain", "dB", "-18", "+6"),
    p(7, "Balance", "%", "-100", "+100"),
];

pub(super) static LEC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "Gain", "", "0", "100"),
    p(3, "Peak Red", "", "0", "100"),
    e(4, "Mode", LEC_MODE),
    p(5, "OutGain", "dB", "-18", "+6"),
];

pub(super) static ULC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "InGain", "dB", "-48", "0"),
    p(3, "OutGain", "dB", "-48", "0"),
    p(4, "Attack", "", "1", "7"),
    p(5, "Release", "", "1", "7"),
    e(6, "Ratio", ULC_RATIO),
];
