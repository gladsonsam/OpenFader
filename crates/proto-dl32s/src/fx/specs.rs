//! Reviewed FX address table, engineering domains and wire encoding.
// Each listed parameter address was independently changed, observed in
// manufacturer mixer software and restored against the complete mixer bank. Algorithm
// Historic evidence covers only the observed slot/algorithm pairs.
// Keep these 55 scalar descriptors + four source actions as compatibility
// overrides; the complete native catalogue lives in core::fx::dl32s.
// Engineering domains: manufacturer mixer software 5.1 reference guide pp165,167.
// Decay bounds additionally verified at both native slider endpoints for
// each listed slot/algorithm: Plate1..3 s, Hall2..6 s, Spring0.1..10 s.
#[derive(Debug)]
pub(super) struct FxParameterSpec {
    pub(super) slot: u16,
    pub(super) algorithm_address: u32,
    pub(super) algorithm: u32,
    pub(super) algorithm_name: &'static str,
    pub(super) address: u32,
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) unit: &'static str,
    pub(super) decimals: u8,
}

#[rustfmt::skip]
pub(super) const FX_PARAMETERS: &[FxParameterSpec] = &[
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3631, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3632, id: "decay", label: "Decay", min: 1.0, max: 3.0, unit: "s", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3633, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3634, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3635, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3636, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3637, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3638, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    // Native slider endpoints independently verified as normalized0..1,
    // despite the reference guide describing modulation depth in percent.
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3639, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3640, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3641, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", decimals: 2 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3642, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", decimals: 1 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3662, id: "time", label: "Delay time", min: 10.0, max: 1600.0, unit: "ms", decimals: 0 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3664, id: "feedback", label: "Feedback", min: 0.0, max: 100.0, unit: "%", decimals: 0 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3665, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3693, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3694, id: "decay", label: "Decay", min: 2.0, max: 6.0, unit: "s", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3695, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3696, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3697, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3698, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3699, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3700, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3701, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3702, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3703, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", decimals: 2 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3704, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3724, id: "predelay", label: "Predelay", min: 0.0, max: 300.0, unit: "ms", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3725, id: "decay", label: "Decay", min: 0.1, max: 10.0, unit: "s", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3726, id: "diffusion", label: "Diffusion", min: 0.0, max: 10.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3727, id: "hf_damping", label: "HF Damping", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3728, id: "lr_rolloff", label: "LR Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3729, id: "lr_bass_boost", label: "LR Bass Boost", min: -15.0, max: 15.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3730, id: "er_level", label: "ER Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3731, id: "lr_level", label: "LR Level", min: -12.0, max: 12.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3732, id: "mod_depth", label: "Mod Depth", min: 0.0, max: 1.0, unit: "", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3733, id: "er_rolloff", label: "ER Rolloff", min: 500.0, max: 20000.0, unit: "Hz", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3734, id: "mod_rate", label: "Mod Rate", min: 0.0, max: 20.0, unit: "Hz", decimals: 2 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3735, id: "lf_damping", label: "LF Damping", min: 20.0, max: 20000.0, unit: "Hz", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3755, id: "mute", label: "Mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3827, id: "mute", label: "Mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3899, id: "mute", label: "Mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3971, id: "mute", label: "Mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3756, id: "main_return", label: "Main return", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3828, id: "main_return", label: "Main return", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3900, id: "main_return", label: "Main return", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3972, id: "main_return", label: "Main return", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3584, id: "send_master", label: "Send master", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3596, id: "send_master", label: "Send master", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3608, id: "send_master", label: "Send master", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3620, id: "send_master", label: "Send master", min: -120.0, max: 10.0, unit: "dB", decimals: 1 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3583, id: "send_mute", label: "Send mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3595, id: "send_mute", label: "Send mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3607, id: "send_mute", label: "Send mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3619, id: "send_mute", label: "Send mute", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 1, algorithm_address: 3629, algorithm: 2, algorithm_name: "Plate Reverb",
        address: 3585, id: "send_source", label: "Send source", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 2, algorithm_address: 3660, algorithm: 12, algorithm_name: "Tape Echo Delay",
        address: 3597, id: "send_source", label: "Send source", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 3, algorithm_address: 3691, algorithm: 7, algorithm_name: "Hall Reverb",
        address: 3609, id: "send_source", label: "Send source", min: 0.0, max: 1.0, unit: "", decimals: 0 },
    FxParameterSpec { slot: 4, algorithm_address: 3722, algorithm: 10, algorithm_name: "Spring Reverb",
        address: 3621, id: "send_source", label: "Send source", min: 0.0, max: 1.0, unit: "", decimals: 0 },
];

impl FxParameterSpec {
    pub(super) fn is_binary(&self) -> bool {
        matches!(self.id, "mute" | "send_mute" | "send_source")
    }

    pub(super) fn allows(&self, value: f32) -> bool {
        value.is_finite()
            && (self.min..=self.max).contains(&value)
            && (!self.is_binary() || matches!(value, 0.0 | 1.0))
    }

    pub(super) fn raw_value(&self, value: f32) -> u32 {
        if self.is_binary() {
            value as u32
        } else {
            value.to_bits()
        }
    }

    pub(super) fn read_value(
        &self,
        raw: u32,
    ) -> Option<openfader_core::destinations::DestinationValue> {
        let value = if self.is_binary() {
            raw as f32
        } else {
            f32::from_bits(raw)
        };
        self.allows(value)
            .then_some(openfader_core::destinations::DestinationValue {
                value,
                verified: true,
            })
    }
}
