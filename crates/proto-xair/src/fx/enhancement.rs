//! Enhancement parameter tables.
use super::*;
pub(super) static IMG: &[FxParamDef] = &[
    p(1, "Balance", "%", "-100", "+100"),
    p(2, "MonoPan", "%", "-100", "+100"),
    p(3, "StereoPan", "%", "-100", "+100"),
    p(4, "ShvGain", "dB", "0", "12"),
    p(5, "ShvFreq", "Hz", "100", "1000"),
    p(6, "ShvQ", "", "1", "10"),
    p(7, "OutGain", "dB", "-12", "+12"),
];

pub(super) static EDI: &[FxParamDef] = &[
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
pub(super) static SON4: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    p(2, "LoContour", "", "0", "10"),
    q(3, "Process", "", "0", "10"),
    p(4, "Gain", "dB", "-12", "+12"),
];

pub(super) static EXC: &[FxParamDef] = &[
    p(1, "Tune", "Hz", "1k", "10k"),
    p(2, "Peak", "%", "0", "100"),
    p(3, "ZeroFill", "%", "0", "100"),
    p(4, "Timbre", "", "-50", "+50"),
    p(5, "Harmonics", "%", "0", "100"),
    p(6, "Mix", "%", "0", "100"),
    e(7, "Solo", OFF_ON),
];

pub(super) static ENH: &[FxParamDef] = &[
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
pub(super) static ENH2: &[FxParamDef] = &[
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
pub(super) static AMP: &[FxParamDef] = &[
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

pub(super) static DRV: &[FxParamDef] = &[
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
pub(super) static PIT2: &[FxParamDef] = &[
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

pub(super) static PIT: &[FxParamDef] = &[
    p(1, "Semi", "", "-12", "+12"),
    p(2, "Cent", "", "-50", "+50"),
    // Range mismatch: X-Air 1–500 ms vs X32 doc 1–100 — X-Air preferred.
    q(3, "Delay", "ms", "1", "500"),
    p(4, "LoCut", "Hz", "10", "500"),
    p(5, "HiCut", "Hz", "2k", "20k"),
    p(6, "Mix", "%", "0", "100"),
];
