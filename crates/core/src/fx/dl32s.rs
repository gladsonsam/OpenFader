//! DL32S FX engineering schemas, desk-verified.
//! Anchors: control reference guide 5.1 pp165–172.
//! Defaults seed simulation only; production never fabricates values.
use crate::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};

#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub word: usize,
    pub id: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub unit: &'static str,
    pub integer: bool,
    pub default: f32,
    pub tap: bool,
}
#[derive(Debug)]
pub struct Algorithm {
    pub id: u32,
    pub label: &'static str,
    pub fields: &'static [Field],
}

#[rustfmt::skip]
const A2: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 1.0, max: 3.0, unit: "s", integer: false, default: 2.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A3: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 0.1, max: 1.3, unit: "s", integer: false, default: 0.7, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A4: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 0.5, max: 2.0, unit: "s", integer: false, default: 1.25, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A5: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 1.0, max: 3.0, unit: "s", integer: false, default: 2.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A6: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 2.0, max: 6.0, unit: "s", integer: false, default: 4.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A7: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 2.0, max: 6.0, unit: "s", integer: false, default: 4.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A8: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 4.0, max: 10.0, unit: "s", integer: false, default: 7.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A9: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 2.0, max: 6.0, unit: "s", integer: false, default: 4.0, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
    Field { word: 14, id: "gate_release", label: "Gate release", min: 50.0, max: 3000.0, unit: "ms", integer: false, default: 250.0, tap: false },
];
#[rustfmt::skip]
const A10: &[Field] = &[
    Field { word: 2, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 3, id: "decay", label: "Decay", min: 0.1, max: 10.0, unit: "s", integer: false, default: 5.05, tap: false },
    Field { word: 4, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", integer: false, default: 5.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 6, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 9, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", integer: false, default: 0.0, tap: false },
    Field { word: 10, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 11, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 12, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 20.0, tap: false },
];
#[rustfmt::skip]
const A11: &[Field] = &[
    Field { word: 2, id: "time", label: "Delay time", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 4, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "", integer: false, default: 20.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A12: &[Field] = &[
    Field { word: 2, id: "time", label: "Delay time", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 4, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "", integer: false, default: 20.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A13: &[Field] = &[
    Field { word: 2, id: "time_l", label: "Delay L", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 3, id: "time_r", label: "Delay R", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 4, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "", integer: false, default: 20.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A14: &[Field] = &[
    Field { word: 2, id: "time_l", label: "Delay L", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 3, id: "time_r", label: "Delay R", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 4, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "", integer: false, default: 20.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A15: &[Field] = &[
    Field { word: 2, id: "time_1", label: "Delay 1", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 3, id: "time_2", label: "Delay 2", min: 10.0, max: 1600.0, unit: "ms", integer: false, default: 250.0, tap: true },
    Field { word: 4, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "", integer: false, default: 20.0, tap: false },
    Field { word: 5, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A16: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.02, max: 10.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 1.0, tap: false },
    Field { word: 3, id: "waveform", label: "Waveform", min: 0.0, max: 4.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 4, id: "notch_frequency", label: "Notch frequency", min: 500.0, max: 2000.0, unit: "Hz", integer: false, default: 500.0, tap: false },
    Field { word: 5, id: "filter_number", label: "Filter number", min: 2.0, max: 12.0, unit: "", integer: true, default: 12.0, tap: false },
    Field { word: 6, id: "feedback", label: "Feedback", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
    Field { word: 7, id: "width", label: "Width", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
];
#[rustfmt::skip]
const A17: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.02, max: 10.0, unit: "Hz", integer: false, default: 5.0, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.7, tap: false },
    Field { word: 3, id: "waveform", label: "Waveform", min: 0.0, max: 4.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 4, id: "width", label: "Width", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
];
#[rustfmt::skip]
const A18: &[Field] = &[
    Field { word: 1, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "ms", integer: false, default: 0.25, tap: false },
    Field { word: 2, id: "tremolo_depth", label: "Tremolo depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.7, tap: false },
    Field { word: 3, id: "acceleration", label: "Acceleration", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.25, tap: false },
    Field { word: 4, id: "rate_high", label: "Rate high", min: 0.1, max: 10.0, unit: "Hz", integer: false, default: 6.7, tap: false },
    Field { word: 5, id: "rate_low", label: "Rate low", min: 0.1, max: 10.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 6, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "speed", label: "Speed", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
    Field { word: 8, id: "width", label: "Width", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.7, tap: false },
];
#[rustfmt::skip]
const A19: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.05, max: 5.0, unit: "Hz", integer: false, default: 0.5, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 3, id: "delay_time", label: "Delay time", min: 0.0, max: 50.0, unit: "ms", integer: false, default: 10.0, tap: false },
    Field { word: 4, id: "waveform", label: "Waveform", min: 0.0, max: 5.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 5, id: "voices", label: "Voices", min: 0.0, max: 3.0, unit: "", integer: true, default: 2.0, tap: false },
    Field { word: 6, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A20: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.05, max: 5.0, unit: "Hz", integer: false, default: 0.5, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.5, tap: false },
    Field { word: 3, id: "delay_time", label: "Delay time", min: 0.0, max: 50.0, unit: "ms", integer: false, default: 10.0, tap: false },
    Field { word: 4, id: "waveform", label: "Waveform", min: 0.0, max: 5.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 5, id: "voices", label: "Voices", min: 0.0, max: 3.0, unit: "", integer: true, default: 3.0, tap: false },
    Field { word: 6, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
];
#[rustfmt::skip]
const A21: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.05, max: 5.0, unit: "Hz", integer: false, default: 0.2, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.2, tap: false },
    Field { word: 3, id: "delay_time", label: "Delay time", min: 0.0, max: 10.0, unit: "ms", integer: false, default: 0.0, tap: false },
    Field { word: 4, id: "waveform", label: "Waveform", min: 0.0, max: 4.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 5, id: "feedback", label: "Feedback", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.69, tap: false },
    Field { word: 6, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 20000.0, tap: false },
    Field { word: 7, id: "width", label: "Width", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
];
#[rustfmt::skip]
const A22: &[Field] = &[
    Field { word: 1, id: "rate", label: "Rate", min: 0.05, max: 5.0, unit: "Hz", integer: false, default: 0.5, tap: false },
    Field { word: 2, id: "depth", label: "Depth", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.1, tap: false },
    Field { word: 3, id: "delay_time", label: "Delay time", min: 60.0, max: 1600.0, unit: "ms", integer: false, default: 500.0, tap: true },
    Field { word: 4, id: "waveform", label: "Waveform", min: 0.0, max: 4.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 5, id: "feedback", label: "Feedback", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.2, tap: false },
    Field { word: 6, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", integer: false, default: 4000.0, tap: false },
    Field { word: 7, id: "width", label: "Width", min: -1.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
];
#[rustfmt::skip]
const A23: &[Field] = &[
    Field { word: 1, id: "enable", label: "Enable", min: 0.0, max: 1.0, unit: "", integer: true, default: 1.0, tap: false },
    Field { word: 3, id: "sweep_type", label: "Sweep type", min: 0.0, max: 12.0, unit: "", integer: true, default: 8.0, tap: false },
    Field { word: 4, id: "upper_frequency", label: "Upper frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 2700.0, tap: false },
    Field { word: 5, id: "lower_frequency", label: "Lower frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 200.0, tap: false },
    Field { word: 6, id: "upper_q", label: "Upper Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 4.0, tap: false },
    Field { word: 7, id: "lower_q", label: "Lower Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 1.0, tap: false },
    Field { word: 8, id: "upper_gain", label: "Upper gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 15.0, tap: false },
    Field { word: 9, id: "lower_gain", label: "Lower gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 8.0, tap: false },
    Field { word: 14, id: "attack", label: "Attack", min: 0.0, max: 1.0, unit: "", integer: false, default: 1.0, tap: false },
    Field { word: 15, id: "position", label: "Position", min: 0.0, max: 1.0, unit: "", integer: false, default: 0.0, tap: false },
];
#[rustfmt::skip]
const A24: &[Field] = &[
    Field { word: 1, id: "enable", label: "Enable", min: 0.0, max: 1.0, unit: "", integer: true, default: 1.0, tap: false },
    Field { word: 3, id: "sweep_type", label: "Sweep type", min: 0.0, max: 12.0, unit: "", integer: true, default: 8.0, tap: false },
    Field { word: 4, id: "upper_frequency", label: "Upper frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 2700.0, tap: false },
    Field { word: 5, id: "lower_frequency", label: "Lower frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 200.0, tap: false },
    Field { word: 6, id: "upper_q", label: "Upper Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 4.0, tap: false },
    Field { word: 7, id: "lower_q", label: "Lower Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 1.0, tap: false },
    Field { word: 8, id: "upper_gain", label: "Upper gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 15.0, tap: false },
    Field { word: 9, id: "lower_gain", label: "Lower gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 8.0, tap: false },
    Field { word: 10, id: "auto_upper_level", label: "Auto upper level", min: -80.0, max: 0.0, unit: "dB", integer: false, default: -21.0, tap: false },
    Field { word: 11, id: "auto_lower_level", label: "Auto lower level", min: -80.0, max: 0.0, unit: "dB", integer: false, default: -40.0, tap: false },
    Field { word: 14, id: "attack", label: "Attack", min: 0.0, max: 1.0, unit: "", integer: false, default: 1.0, tap: false },
];
#[rustfmt::skip]
const A25: &[Field] = &[
    Field { word: 1, id: "enable", label: "Enable", min: 0.0, max: 1.0, unit: "", integer: true, default: 1.0, tap: false },
    Field { word: 3, id: "sweep_type", label: "Sweep type", min: 0.0, max: 12.0, unit: "", integer: true, default: 8.0, tap: false },
    Field { word: 4, id: "upper_frequency", label: "Upper frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 2700.0, tap: false },
    Field { word: 5, id: "lower_frequency", label: "Lower frequency", min: 20.0, max: 20000.0, unit: "Hz", integer: false, default: 200.0, tap: false },
    Field { word: 6, id: "upper_q", label: "Upper Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 4.0, tap: false },
    Field { word: 7, id: "lower_q", label: "Lower Q", min: 0.1, max: 16.0, unit: "", integer: false, default: 1.0, tap: false },
    Field { word: 8, id: "upper_gain", label: "Upper gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 15.0, tap: false },
    Field { word: 9, id: "lower_gain", label: "Lower gain", min: -15.0, max: 15.0, unit: "dB", integer: false, default: 8.0, tap: false },
    Field { word: 12, id: "rate", label: "Rate", min: 0.02, max: 10.0, unit: "Hz", integer: false, default: 1.0, tap: false },
    Field { word: 13, id: "waveform", label: "Waveform", min: 0.0, max: 5.0, unit: "", integer: true, default: 0.0, tap: false },
    Field { word: 14, id: "attack", label: "Attack", min: 0.0, max: 1.0, unit: "", integer: false, default: 1.0, tap: false },
];
#[rustfmt::skip]
pub const ALGORITHMS: &[Algorithm] = &[
    Algorithm { id: 2, label: "Plate Reverb", fields: A2 },
    Algorithm { id: 3, label: "Ambience Reverb", fields: A3 },
    Algorithm { id: 4, label: "Small Room Reverb", fields: A4 },
    Algorithm { id: 5, label: "Medium Room Reverb", fields: A5 },
    Algorithm { id: 6, label: "Large Room Reverb", fields: A6 },
    Algorithm { id: 7, label: "Hall Reverb", fields: A7 },
    Algorithm { id: 8, label: "Cathedral Reverb", fields: A8 },
    Algorithm { id: 9, label: "Gated Reverb", fields: A9 },
    Algorithm { id: 10, label: "Spring Reverb", fields: A10 },
    Algorithm { id: 11, label: "Mono Delay", fields: A11 },
    Algorithm { id: 12, label: "Tape Echo Delay", fields: A12 },
    Algorithm { id: 13, label: "Stereo Delay", fields: A13 },
    Algorithm { id: 14, label: "Ping Pong Delay", fields: A14 },
    Algorithm { id: 15, label: "Multi Tap Delay", fields: A15 },
    Algorithm { id: 16, label: "Phaser", fields: A16 },
    Algorithm { id: 17, label: "Tremolo", fields: A17 },
    Algorithm { id: 18, label: "Rotary", fields: A18 },
    Algorithm { id: 19, label: "Mono Chorus", fields: A19 },
    Algorithm { id: 20, label: "Stereo Chorus", fields: A20 },
    Algorithm { id: 21, label: "Flanger", fields: A21 },
    Algorithm { id: 22, label: "Mod Delay", fields: A22 },
    Algorithm { id: 23, label: "Sweep Filter Manual", fields: A23 },
    Algorithm { id: 24, label: "Sweep Filter Auto", fields: A24 },
    Algorithm { id: 25, label: "Sweep Filter LFO", fields: A25 },
];

// Native selection popover includes None=0, skips internal Generic=1.
static NONE: Algorithm = Algorithm {
    id: 0,
    label: "None",
    fields: &[],
};
pub fn algorithm(id: u32) -> Option<&'static Algorithm> {
    if id == 0 {
        Some(&NONE)
    } else {
        ALGORITHMS.iter().find(|a| a.id == id)
    }
}
impl Field {
    pub fn decode(&self, raw: u32) -> Option<f32> {
        let value = if self.integer {
            raw as f32
        } else {
            f32::from_bits(raw)
        };
        (value.is_finite()
            && (self.min..=self.max).contains(&value)
            && (!self.integer || value.fract() == 0.0))
            .then_some(value)
    }
    pub fn encode(&self, value: f32) -> Option<u32> {
        let raw = if self.integer {
            value as u32
        } else {
            value.to_bits()
        };
        (self.decode(raw) == Some(value)).then_some(raw)
    }
    pub fn choices(&self, algorithm: u32) -> Vec<DestinationChoice> {
        let pairs: Vec<(f32, &str)> = match self.id {
            "waveform" => ["Sine", "Square", "Triangle", "Saw", "Reverse Saw", "Random"]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| *i as f32 <= self.max)
                .map(|(i, s)| (i as f32, s))
                .collect(),
            // DLEnums::LOUD_eq supplies exact raw identities. The native
            // formatter collapses several of these labels to “Shelf”.
            "sweep_type" => vec![
                (0.0, "LPF"),
                (1.0, "HPF"),
                (2.0, "PEQ"),
                (3.0, "Low shelf"),
                (4.0, "High shelf"),
                (5.0, "Low shelf, first order"),
                (6.0, "High shelf, first order"),
                (7.0, "Notch"),
                (8.0, "Bandpass"),
                (9.0, "LPF, first order"),
                (10.0, "HPF, first order"),
                (11.0, "Low shelf, variable Q"),
                (12.0, "High shelf, variable Q"),
            ],
            "filter_number" => (2..=12).map(|n| (n as f32, "")).collect(),
            "voices" if algorithm == 19 => vec![(0.0, "One"), (2.0, "Two")],
            "voices" => vec![(1.0, "One"), (3.0, "Two")],
            "width" => vec![(0.0, "Narrow"), (0.7, "Wide")],
            "acceleration" => vec![(0.25, "Slow"), (0.75, "Fast")],
            "speed" => vec![(0.0, "Low"), (1.0, "High")],
            "enable" => vec![(0.0, "Off"), (1.0, "On")],
            _ => Vec::new(),
        };
        pairs
            .into_iter()
            .map(|(value, label)| DestinationChoice {
                value,
                label: if label.is_empty() {
                    format!("{value:.0}")
                } else {
                    label.to_string()
                },
            })
            .collect()
    }
    pub fn control(
        &self,
        slot: u16,
        algorithm: &Algorithm,
        value: Option<f32>,
        revision: u64,
        generation: u64,
    ) -> DestinationControl {
        let mut row = selector(slot, Some(algorithm.id), revision, generation);
        row.id = self.id.to_string();
        row.label = self.label.to_string();
        row.description = format!("Changes {} for every route through FX {slot}. Mute, returns and routing are separate controls.", self.label.to_lowercase());
        row.choices = self.choices(algorithm.id);
        row.range = row.choices.is_empty().then(|| DestinationRange {
            min: self.min,
            max: self.max,
            unit: if self.id == "feedback" && (11..=15).contains(&algorithm.id) {
                "%"
            } else {
                self.unit
            }
            .to_string(),
            decimals: if self.integer || matches!(self.unit, "ms" | "Hz") {
                0
            } else {
                2
            },
            step: None,
        });
        row.tap_tempo = self.tap;
        row.current = value.map(|value| DestinationValue {
            value,
            verified: true,
        });
        row
    }
}
pub fn selector(
    slot: u16,
    current: Option<u32>,
    revision: u64,
    generation: u64,
) -> DestinationControl {
    DestinationControl {
        target: format!("fx/{slot:02}"), target_label: format!("FX {slot}"),
        target_name: current.and_then(algorithm).map(|a| a.label.to_string()),
        id: "algorithm".to_string(), label: "Algorithm".to_string(), surface: "fx".to_string(),
        description: format!("Changes the algorithm for every route through FX {slot}. The mixer may reset effect parameters. Fresh readback is required before editing."),
        choices: std::iter::once(&NONE).chain(ALGORITHMS.iter())
            .map(|a| DestinationChoice { value: a.id as f32, label: a.label.to_string() }).collect(),
        range: None, current: current.filter(|id| algorithm(*id).is_some()).map(|id| DestinationValue { value: id as f32, verified: true }),
        revision: Some(revision), generation: Some(generation), members: Vec::new(), send_slot: None,
        preset: false, tap_tempo: false, gesture: None,
    }
}
