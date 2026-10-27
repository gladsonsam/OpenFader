//! Reverb parameter tables.
use super::*;
pub(super) static HALL: &[FxParamDef] = &[
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

pub(super) static AMBI: &[FxParamDef] = &[
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

pub(super) static RPLT: &[FxParamDef] = &[
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

pub(super) static ROOM: &[FxParamDef] = &[
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

pub(super) static CHAM: &[FxParamDef] = &[
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

pub(super) static PLAT: &[FxParamDef] = &[
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

pub(super) static VREV: &[FxParamDef] = &[
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

pub(super) static VRM: &[FxParamDef] = &[
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

pub(super) static GATE: &[FxParamDef] = &[
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

pub(super) static RVRS: &[FxParamDef] = &[
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
