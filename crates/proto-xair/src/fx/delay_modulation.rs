//! Delay modulation parameter tables.
use super::*;
pub(super) static DLY: &[FxParamDef] = &[
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

pub(super) static TAP3: &[FxParamDef] = &[
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

pub(super) static TAP4: &[FxParamDef] = &[
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

pub(super) static CRS: &[FxParamDef] = &[
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

pub(super) static FLNG: &[FxParamDef] = &[
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

pub(super) static PHAS: &[FxParamDef] = &[
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

pub(super) static DIMC: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "M/St", DIMC_MODE),
    e(3, "Dry", OFF_ON),
    e(4, "Sw1", OFF_ON),
    e(5, "Sw2", OFF_ON),
    e(6, "Sw3", OFF_ON),
    e(7, "Sw4", OFF_ON),
];

pub(super) static FILT: &[FxParamDef] = &[
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

pub(super) static ROTA: &[FxParamDef] = &[
    p(1, "LoSpeed", "Hz", "0.1", "4"),
    p(2, "HiSpeed", "Hz", "2", "10"),
    p(3, "Accel", "%", "0", "100"),
    p(4, "Distance", "%", "0", "100"),
    p(5, "Balance", "%", "-100", "+100"),
    p(6, "Mix", "%", "0", "100"),
    e(7, "StopSW", OFF_ON),
    e(8, "SpeedSW", OFF_ON),
];

pub(super) static PAN: &[FxParamDef] = &[
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
pub(super) static SUB5: &[FxParamDef] = &[
    e(1, "On", OFF_ON),
    e(2, "Range", SUB_RANGE),
    p(3, "Dry", "%", "0", "100"),
    p(4, "Oct1", "%", "0", "100"),
    p(5, "Oct2", "%", "0", "100"),
];

pub(super) static DRV_COMBO: &[FxParamDef] = &[
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

pub(super) static CRR: &[FxParamDef] = &[
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

pub(super) static FLR: &[FxParamDef] = &[
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

pub(super) static DCR: &[FxParamDef] = &[
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

pub(super) static DFL: &[FxParamDef] = &[
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

pub(super) static MODD: &[FxParamDef] = &[
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
