//! XR18 labels for parameter values that are numeric on the wire.
use openfader_core::params::{ParamChoice, ParamPresentation};

const fn c(value: f32, label: &'static str) -> ParamChoice {
    ParamChoice { value, label }
}
const GATE_MODES: &[ParamChoice] = &[
    c(0., "Gate"),
    c(1., "Expander 2:1"),
    c(2., "Expander 3:1"),
    c(3., "Expander 4:1"),
    c(4., "Duck"),
];
const COMP_MODES: &[ParamChoice] = &[c(0., "Compressor"), c(1., "Expander")];
const DETECTORS: &[ParamChoice] = &[c(0., "Peak"), c(1., "RMS")];
const ENVELOPES: &[ParamChoice] = &[c(0., "Linear"), c(1., "Logarithmic")];
const FILTERS: &[ParamChoice] = &[
    c(0., "Low cut 6 dB/oct"),
    c(1., "Low cut 12 dB/oct"),
    c(2., "High cut 6 dB/oct"),
    c(3., "High cut 12 dB/oct"),
    c(4., "Band 1.0"),
    c(5., "Band 2.0"),
    c(6., "Band 3.0"),
    c(7., "Band 5.0"),
    c(8., "Band 10.0"),
];
const KEYS: &[ParamChoice] = &[
    c(0., "Self"),
    c(1., "Channel 1"),
    c(2., "Channel 2"),
    c(3., "Channel 3"),
    c(4., "Channel 4"),
    c(5., "Channel 5"),
    c(6., "Channel 6"),
    c(7., "Channel 7"),
    c(8., "Channel 8"),
    c(9., "Channel 9"),
    c(10., "Channel 10"),
    c(11., "Channel 11"),
    c(12., "Channel 12"),
    c(13., "Channel 13"),
    c(14., "Channel 14"),
    c(15., "Channel 15"),
    c(16., "Channel 16"),
    c(17., "Bus 1"),
    c(18., "Bus 2"),
    c(19., "Bus 3"),
    c(20., "Bus 4"),
    c(21., "Bus 5"),
    c(22., "Bus 6"),
];
const fn presentation(id: &'static str, choices: &'static [ParamChoice]) -> ParamPresentation {
    ParamPresentation {
        id,
        when: None,
        choices,
        value_labels: &[],
        limiter_value: None,
        hidden: false,
        curve: None,
    }
}

const fn eq_presentation(id: &'static str) -> ParamPresentation {
    ParamPresentation {
        // X AIR Edit 1.8.1 / XR18 FW 1.20, 2026-10-28: shelf
        // response is unchanged across Q .3, 3.1 and 10. Bell Q is retained.
        curve: Some("fixed_shelf"),
        ..presentation(id, &[])
    }
}
pub(super) const PRESENTATIONS: &[ParamPresentation] = &[
    eq_presentation("eq1_type"),
    eq_presentation("eq2_type"),
    eq_presentation("eq3_type"),
    eq_presentation("eq4_type"),
    eq_presentation("eq5_type"),
    eq_presentation("eq6_type"),
    presentation("gate_mode", GATE_MODES),
    presentation("gate_key_src", KEYS),
    presentation("gate_filter_type", FILTERS),
    presentation("comp_mode", COMP_MODES),
    presentation("comp_det", DETECTORS),
    presentation("comp_env", ENVELOPES),
    presentation("comp_key_src", KEYS),
    presentation("comp_filter_type", FILTERS),
];
