//! Channel input filters (manual p.41 and pinned libwing /ch/1/flt schema).
//! Aux strips have no flt node. Tool-model segments in the frozen schema are
//! synthetic: real OSC addresses are flt/tilt, flt/low, flt/proc, flt/f, flt/q.
use crate::mapping::{Mapping, WireKind};
use openfader_core::params::*;

pub const MODELS: &[&str] = &["TILT", "MAX", "AP1", "AP2"];
pub const LOW_SLOPES: &[(&str, f32)] = &[("6", 0.), ("12", 1.), ("18", 2.), ("24", 3.)];
pub const HIGH_SLOPES: &[(&str, f32)] = &[("6", 6.), ("12", 12.)];

pub fn mapping(root: &str, param: &str) -> Option<Mapping> {
    let (leaf, kind, model) = match param {
        "hpf_slope" => ("lcs", WireKind::Choices(LOW_SLOPES), None),
        "wing:hcf_on" => ("hc", WireKind::Bool, None),
        "wing:hcf_freq" => ("hcf", WireKind::Float(50., 20000.), None),
        "wing:hcf_slope" => ("hcs", WireKind::Choices(HIGH_SLOPES), None),
        "wing:filter_on" => ("tf", WireKind::Bool, None),
        "wing:filter_model" => ("mdl", WireKind::Enum(MODELS), None),
        "wing:filter_tilt" => ("tilt", WireKind::Float(-6., 6.), Some("TILT")),
        "wing:filter_low" => ("low", WireKind::Float(0., 100.), Some("MAX")),
        "wing:filter_high" => ("proc", WireKind::Float(0., 100.), Some("MAX")),
        "wing:filter_ap1_freq" => ("f", WireKind::Float(100., 10000.), Some("AP1")),
        "wing:filter_ap2_freq" => ("f", WireKind::Float(100., 10000.), Some("AP2")),
        "wing:filter_ap2_q" => ("q", WireKind::Float(0.5, 5.), Some("AP2")),
        _ => return None,
    };
    Some(Mapping {
        path: format!("{root}/flt/{leaf}"),
        kind,
        model: model.map(|name| ("flt", name)),
    })
}

pub const DEFS: &[ParamDef] = &[
    def("hpf_slope", "Low cut slope", ParamKind::Enum, 0., 3., ""),
    def("wing:hcf_on", "High cut", ParamKind::Bool, 0., 1., ""),
    def(
        "wing:hcf_freq",
        "High cut frequency",
        ParamKind::Float,
        50.,
        20000.,
        "Hz",
    ),
    def(
        "wing:hcf_slope",
        "High cut slope",
        ParamKind::Enum,
        6.,
        12.,
        "dB/oct",
    ),
    def("wing:filter_on", "Tool filter", ParamKind::Bool, 0., 1., ""),
    def(
        "wing:filter_model",
        "Tool filter model",
        ParamKind::Enum,
        0.,
        3.,
        "",
    ),
    def("wing:filter_tilt", "Tilt", ParamKind::Float, -6., 6., "dB"),
    def(
        "wing:filter_low",
        "Low contour",
        ParamKind::Float,
        0.,
        100.,
        "%",
    ),
    def(
        "wing:filter_high",
        "High process",
        ParamKind::Float,
        0.,
        100.,
        "%",
    ),
    def(
        "wing:filter_ap1_freq",
        "All-pass frequency",
        ParamKind::Float,
        100.,
        10000.,
        "Hz",
    ),
    def(
        "wing:filter_ap2_freq",
        "All-pass frequency",
        ParamKind::Float,
        100.,
        10000.,
        "Hz",
    ),
    def(
        "wing:filter_ap2_q",
        "All-pass Q",
        ParamKind::Float,
        0.5,
        5.,
        "",
    ),
];
const fn def(
    id: &'static str,
    label: &'static str,
    kind: ParamKind,
    min: f32,
    max: f32,
    unit: &'static str,
) -> ParamDef {
    ParamDef {
        id,
        label,
        kind,
        scope: ParamScope::Channel,
        scopes: SCOPES_INPUT,
        min,
        max,
        unit,
        default: match kind {
            ParamKind::Bool => ParamDefault::Bool(false),
            _ => ParamDefault::Float(min),
        },
    }
}

pub const PRESENTATIONS: &[ParamPresentation] = &[
    presentation(
        "hpf_slope",
        None,
        &[
            ParamChoice {
                value: 0.,
                label: "6 dB/oct",
            },
            ParamChoice {
                value: 1.,
                label: "12 dB/oct",
            },
            ParamChoice {
                value: 2.,
                label: "18 dB/oct",
            },
            ParamChoice {
                value: 3.,
                label: "24 dB/oct",
            },
        ],
        false,
    ),
    presentation(
        "wing:hcf_slope",
        None,
        &[
            ParamChoice {
                value: 6.,
                label: "6 dB/oct",
            },
            ParamChoice {
                value: 12.,
                label: "12 dB/oct",
            },
        ],
        false,
    ),
    presentation(
        "wing:filter_model",
        None,
        &[
            ParamChoice {
                value: 0.,
                label: "Tilt EQ",
            },
            ParamChoice {
                value: 1.,
                label: "Maxer",
            },
            ParamChoice {
                value: 2.,
                label: "All-pass 90°",
            },
            ParamChoice {
                value: 3.,
                label: "All-pass 180°",
            },
        ],
        false,
    ),
    presentation("wing:filter_tilt", None, &[], true),
    presentation("wing:filter_tilt", Some(0.), &[], false),
    presentation("wing:filter_low", None, &[], true),
    presentation("wing:filter_low", Some(1.), &[], false),
    presentation("wing:filter_high", None, &[], true),
    presentation("wing:filter_high", Some(1.), &[], false),
    presentation("wing:filter_ap1_freq", None, &[], true),
    presentation("wing:filter_ap1_freq", Some(2.), &[], false),
    presentation("wing:filter_ap2_freq", None, &[], true),
    presentation("wing:filter_ap2_freq", Some(3.), &[], false),
    presentation("wing:filter_ap2_q", None, &[], true),
    presentation("wing:filter_ap2_q", Some(3.), &[], false),
];
const fn presentation(
    id: &'static str,
    model: Option<f32>,
    choices: &'static [ParamChoice],
    hidden: bool,
) -> ParamPresentation {
    ParamPresentation {
        id,
        when: match model {
            Some(value) => Some(ParamCondition::Equals {
                id: "wing:filter_model",
                value,
            }),
            None => None,
        },
        choices,
        value_labels: &[],
        limiter_value: None,
        hidden,
        curve: None,
    }
}
