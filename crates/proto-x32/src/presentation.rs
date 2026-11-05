//! Provider-owned capabilities, engineering ranges, contributed
//! param definitions, presentations and scope restrictions.
//!
//! Ranges follow protocol reference v4.09-02 exactly; nothing is copied from
//! X-Air defaults. Anything without a confirmed mapping stays out of
//! `capabilities()` so the UI hides it.
use crate::addrs;
use openfader_core::{
    params::{
        ParamChoice, ParamCondition, ParamDef, ParamDefault, ParamKind, ParamPresentation,
        ParamScope, ParamSwatch, SCOPES_INPUT,
    },
    StripKind,
};
use std::sync::OnceLock;

const CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "color",
    "icon",
    "pan",
    "lr_assign",
    "x32:mono_assign",
    "x32:mono_level",
    "source",
    "x32:preamp_state",
    "x32:eq_bands",
    "x32:eq_type_count",
    "x32:automix_gain",
    "gain",
    "phantom",
    "trim",
    "polarity",
    "delay_on",
    "delay_ms",
    "hpf_on",
    "hpf_freq",
    "hpf_slope",
    "dca1_assign",
    "dca2_assign",
    "dca3_assign",
    "dca4_assign",
    "dca5_assign",
    "dca6_assign",
    "dca7_assign",
    "dca8_assign",
    "_mg1",
    "_mg2",
    "_mg3",
    "_mg4",
    "_mg5",
    "_mg6",
    "automix_group",
    "automix_weight",
    "insert_on",
    "insert_sel",
    "x32:insert_pos",
    "gate_gr",
    "gate_on",
    "gate_mode",
    "gate_thr",
    "gate_range",
    "gate_att",
    "gate_hold",
    "gate_rel",
    "gate_key_src",
    "gate_filter_on",
    "gate_filter_type",
    "gate_filter_freq",
    "comp_gr",
    "comp_on",
    "comp_mode",
    "comp_det",
    "comp_env",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_makeup",
    "comp_att",
    "comp_hold",
    "comp_rel",
    "comp_mix",
    "comp_auto",
    "comp_key_src",
    "comp_filter_on",
    "comp_filter_type",
    "comp_filter_freq",
    "eq_on",
    "eq1_gain",
    "eq1_freq",
    "eq1_q",
    "eq1_type",
    "eq2_gain",
    "eq2_freq",
    "eq2_q",
    "eq2_type",
    "eq3_gain",
    "eq3_freq",
    "eq3_q",
    "eq3_type",
    "eq4_gain",
    "eq4_freq",
    "eq4_q",
    "eq4_type",
    "eq5_gain",
    "eq5_freq",
    "eq5_q",
    "eq5_type",
    "eq6_gain",
    "eq6_freq",
    "eq6_q",
    "eq6_type",
    "send1_level",
    "send1_on",
    "send1_pan",
    "send1_tap",
    "send2_level",
    "send2_on",
    "send3_level",
    "send3_on",
    "send3_pan",
    "send3_tap",
    "send4_level",
    "send4_on",
    "send5_level",
    "send5_on",
    "send5_pan",
    "send5_tap",
    "send6_level",
    "send6_on",
    "send7_level",
    "send7_on",
    "send7_pan",
    "send7_tap",
    "send8_level",
    "send8_on",
    "send9_level",
    "send9_on",
    "send9_pan",
    "send9_tap",
    "send10_level",
    "send10_on",
    "send11_level",
    "send11_on",
    "send11_pan",
    "send11_tap",
    "send12_level",
    "send12_on",
    "send13_level",
    "send13_on",
    "send13_pan",
    "send13_tap",
    "send14_level",
    "send14_on",
    "send15_level",
    "send15_on",
    "send15_pan",
    "send15_tap",
    "send16_level",
    "send16_on",
    "send17_level",
    "send17_on",
    "send17_pan",
    "send17_tap",
    "send18_level",
    "send18_on",
    "send19_level",
    "send19_on",
    "send19_pan",
    "send19_tap",
    "send20_level",
    "send20_on",
    "send21_level",
    "send21_on",
    "send21_pan",
    "send21_tap",
    "send22_level",
    "send22_on",
];

pub(crate) const RANGES: &[(&str, f32, f32)] = &[
    ("icon", 1.0, 74.0),
    ("insert_sel", 0.0, 22.0),
    ("delay_ms", 0.3, 500.0),
    ("automix_weight", -12.0, 12.0),
    ("gate_key_src", 0.0, 64.0),
    ("comp_key_src", 0.0, 64.0),
    ("eq1_type", 0.0, 13.0),
    ("eq2_type", 0.0, 13.0),
    ("eq3_type", 0.0, 13.0),
    ("eq4_type", 0.0, 13.0),
    ("eq5_type", 0.0, 13.0),
    ("eq6_type", 0.0, 13.0),
    ("x32:mono_level", -90.0, 10.0),
];

pub(crate) fn capabilities() -> &'static [&'static str] {
    CAPABILITIES
}

const INPUT_DSP: &[StripKind] = &[StripKind::In, StripKind::Ret];
const MIXABLE: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Mtx,
    StripKind::Main,
];
const DYN: &[StripKind] = &[
    StripKind::In,
    StripKind::Bus,
    StripKind::Mtx,
    StripKind::Main,
];
const EQ: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Mtx,
    StripKind::Main,
];
const EQ56: &[StripKind] = &[StripKind::Bus, StripKind::Mtx, StripKind::Main];
const SENDS: &[StripKind] = &[StripKind::In, StripKind::Ret];
/// Bus/main → matrix sends (`send17..22`, canonical slots 17..22 so one
/// global destination identity never means bus and matrix at once).
/// Matrix strips carry no sends; DCA strips carry none either.
const SENDS_MATRIX: &[StripKind] = &[StripKind::Bus, StripKind::Main];
/// DCA assignment rides every mixable family except DCA masters.
const DCA_ASSIGN: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Mtx,
    StripKind::Main,
];

pub fn defs() -> &'static [ParamDef] {
    static DEFS: OnceLock<Vec<ParamDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut defs = vec![
            ParamDef {
                id: "x32:eq_bands",
                label: "EQ band count",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: EQ,
                min: 4.,
                max: 6.,
                unit: "",
                default: ParamDefault::Float(4.),
            },
            ParamDef {
                id: "x32:eq_type_count",
                label: "Native EQ type count",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: EQ,
                min: 6.,
                max: 14.,
                unit: "",
                default: ParamDefault::Float(6.),
            },
            // Read-only meter range: the upper bound is the finite log
            // floor used for zero linear gain, not a writable DSP range.
            ParamDef {
                id: "gate_gr",
                label: "Gate gain reduction",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: 0.,
                max: 758.5956,
                unit: "dB",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "comp_gr",
                label: "Dynamics gain reduction",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: DYN,
                min: 0.,
                max: 758.5956,
                unit: "dB",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "x32:mono_assign",
                label: "Mono assign",
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: &[StripKind::In, StripKind::Ret, StripKind::Bus],
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            },
            ParamDef {
                id: "x32:mono_level",
                label: "Mono level",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: &[StripKind::In, StripKind::Ret, StripKind::Bus],
                min: -90.,
                max: 10.,
                unit: "dB",
                default: ParamDefault::Float(-90.),
            },
            ParamDef {
                id: "x32:insert_pos",
                label: "Insert position",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: &[
                    StripKind::In,
                    StripKind::Bus,
                    StripKind::Mtx,
                    StripKind::Main,
                ],
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "automix_group",
                label: "Automix group",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: 0.,
                max: 2.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "x32:preamp_state",
                label: "Active preamp availability",
                kind: ParamKind::Text,
                scope: ParamScope::Channel,
                scopes: INPUT_DSP,
                min: 0.,
                max: 0.,
                unit: "",
                default: ParamDefault::Text(""),
            },
            // Live automix gain per /meters/16 (channels 01..08). The
            // meter cache serves only fresh received samples; the
            // generic `provider:automix_gain` alias resolves.
            ParamDef {
                id: "x32:automix_gain",
                label: "Automix gain",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: -128.,
                max: 0.,
                unit: "dB",
                default: ParamDefault::Float(0.),
            },
        ];
        for n in 5..=8 {
            defs.push(ParamDef {
                id: match n {
                    5 => "dca5_assign",
                    6 => "dca6_assign",
                    7 => "dca7_assign",
                    _ => "dca8_assign",
                },
                label: match n {
                    5 => "DCA 5",
                    6 => "DCA 6",
                    7 => "DCA 7",
                    _ => "DCA 8",
                },
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: &[StripKind::In, StripKind::Ret, StripKind::Bus],
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
        }
        for (id, label) in [
            ("_mg1", "Mute group 1"),
            ("_mg2", "Mute group 2"),
            ("_mg3", "Mute group 3"),
            ("_mg4", "Mute group 4"),
            ("_mg5", "Mute group 5"),
            ("_mg6", "Mute group 6"),
        ] {
            defs.push(ParamDef {
                id,
                label,
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: MIXABLE,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
        }
        // Canonical sends stop at slot 16. Matrix destinations use their
        // own provider slots, so register their definitions as well as
        // advertising capabilities/scopes; otherwise caps and the poller
        // silently omit these eighteen supported controls.
        for (id, scope) in scopes() {
            let Some((number, field)) = id
                .strip_prefix("send")
                .and_then(|rest| rest.split_once('_'))
            else {
                continue;
            };
            if !number.parse::<u8>().is_ok_and(|n| (17..=22).contains(&n)) {
                continue;
            }
            let (kind, min, max, unit, default, label) = match field {
                "level" => (
                    ParamKind::Float,
                    -90.,
                    10.,
                    "dB",
                    ParamDefault::Float(-90.),
                    "Matrix send level",
                ),
                "on" => (
                    ParamKind::Bool,
                    0.,
                    1.,
                    "",
                    ParamDefault::Bool(true),
                    "Matrix send on",
                ),
                "pan" => (
                    ParamKind::Float,
                    -100.,
                    100.,
                    "L/R",
                    ParamDefault::Float(0.),
                    "Matrix send pan",
                ),
                "tap" => (
                    ParamKind::Enum,
                    0.,
                    4.,
                    "",
                    ParamDefault::Float(1.),
                    "Matrix send tap",
                ),
                _ => continue,
            };
            defs.push(ParamDef {
                id,
                label,
                kind,
                scope: ParamScope::Channel,
                scopes: scope,
                min,
                max,
                unit,
                default,
            });
        }
        defs
    })
}

fn row(id: &'static str) -> ParamPresentation {
    ParamPresentation {
        id,
        when: None,
        choices: &[],
        value_labels: &[],
        limiter_value: None,
        hidden: false,
        curve: None,
    }
}

fn enum_choices(values: &'static [&'static str]) -> Vec<ParamChoice> {
    values
        .iter()
        .enumerate()
        .map(|(i, label)| ParamChoice {
            value: i as f32,
            label,
        })
        .collect()
}

pub fn presentations() -> &'static [ParamPresentation] {
    static ROWS: OnceLock<Vec<ParamPresentation>> = OnceLock::new();
    ROWS.get_or_init(|| {
        static GATE: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static RATIO: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static KEYFILT: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static EQ4: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static EQ6: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static SENDTAP: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static MTXTAP: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static HPF: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static COLOR: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static AMIX: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static INSPOS: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static INSERT: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static KEYSRC: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        static KNEE: OnceLock<Vec<ParamChoice>> = OnceLock::new();
        let mut rows = vec![
            ParamPresentation {
                choices: GATE.get_or_init(|| {
                    addrs::GATE_MODES
                        .iter()
                        .enumerate()
                        .map(|(i, label)| ParamChoice {
                            value: i as f32,
                            label,
                        })
                        .collect()
                }),
                ..row("gate_mode")
            },
            ParamPresentation {
                choices: RATIO.get_or_init(|| {
                    addrs::DYN_RATIOS
                        .iter()
                        .map(|r| {
                            let label = match *r {
                                1.1 => "1.1:1",
                                1.3 => "1.3:1",
                                1.5 => "1.5:1",
                                2.0 => "2:1",
                                2.5 => "2.5:1",
                                3.0 => "3:1",
                                4.0 => "4:1",
                                5.0 => "5:1",
                                7.0 => "7:1",
                                10.0 => "10:1",
                                20.0 => "20:1",
                                _ => "100:1",
                            };
                            ParamChoice { value: *r, label }
                        })
                        .collect()
                }),
                ..row("comp_ratio")
            },
            ParamPresentation {
                choices: KEYFILT.get_or_init(|| enum_choices(addrs::KEY_FILTER_TYPES)),
                ..row("gate_filter_type")
            },
            ParamPresentation {
                choices: KEYFILT.get_or_init(|| enum_choices(addrs::KEY_FILTER_TYPES)),
                ..row("comp_filter_type")
            },
            ParamPresentation {
                choices: SENDTAP.get_or_init(|| enum_choices(addrs::SEND_TYPES)),
                ..row("send1_tap")
            },
            ParamPresentation {
                choices: HPF.get_or_init(|| enum_choices(addrs::HPF_SLOPES)),
                ..row("hpf_slope")
            },
            ParamPresentation {
                choices: COLOR.get_or_init(|| enum_choices(addrs::COLORS)),
                ..row("color")
            },
            ParamPresentation {
                choices: AMIX.get_or_init(|| {
                    ["OFF", "X", "Y"]
                        .iter()
                        .enumerate()
                        .map(|(i, label)| ParamChoice {
                            value: i as f32,
                            label,
                        })
                        .collect()
                }),
                ..row("automix_group")
            },
            ParamPresentation {
                choices: INSPOS.get_or_init(|| enum_choices(&["PRE", "POST"])),
                ..row("x32:insert_pos")
            },
        ];
        // Native source selectors: protocol reference pp25-26 and output strip
        // tables. Labels stay with the driver; consumers keep opaque IDs.
        // Native dynamics knee is six discrete values, not a binary switch
        // (protocol reference channel/bus/matrix/main dyn/knee: [0, 5, 1]).
        rows.push(ParamPresentation {
            choices: KNEE.get_or_init(|| enum_choices(&["0", "1", "2", "3", "4", "5"])),
            ..row("comp_knee")
        });
        for id in ["gate_key_src", "comp_key_src"] {
            rows.push(ParamPresentation {
                choices: KEYSRC.get_or_init(|| {
                    (0..=64)
                        .map(|index| ParamChoice {
                            value: index as f32,
                            label: Box::leak(
                                addrs::keysrc_label(index)
                                    .expect("documented key-source index")
                                    .into_boxed_str(),
                            ),
                        })
                        .collect()
                }),
                ..row(id)
            });
        }
        rows.push(ParamPresentation {
            choices: INSERT.get_or_init(|| {
                (0..=22)
                    .map(|index| ParamChoice {
                        value: index as f32,
                        label: Box::leak(
                            addrs::insert_label(index)
                                .expect("documented insert index")
                                .into_boxed_str(),
                        ),
                    })
                    .collect()
            }),
            ..row("insert_sel")
        });
        // All bands use the strip family's native type table. Band count
        // alone cannot select it: buses have six bands but six type options;
        // matrices and mains have fourteen options on all six bands.
        for id in [
            "eq1_type", "eq2_type", "eq3_type", "eq4_type", "eq5_type", "eq6_type",
        ] {
            rows.push(ParamPresentation {
                when: Some(ParamCondition::Equals {
                    id: "x32:eq_type_count",
                    value: 6.,
                }),
                choices: EQ4.get_or_init(|| enum_choices(addrs::EQ_TYPES_4BAND)),
                ..row(id)
            });
            rows.push(ParamPresentation {
                when: Some(ParamCondition::Equals {
                    id: "x32:eq_type_count",
                    value: 14.,
                }),
                choices: EQ6.get_or_init(|| enum_choices(addrs::EQ_TYPES_6BAND)),
                ..row(id)
            });
        }
        // The same tap-type choices serve every odd send slot; matrix
        // sends (17/19/21) offer the five documented bus/main options
        // (no GRP) while input sends offer six.
        for id in [
            "send3_tap",
            "send5_tap",
            "send7_tap",
            "send9_tap",
            "send11_tap",
            "send13_tap",
            "send15_tap",
        ] {
            rows.push(ParamPresentation {
                choices: SENDTAP.get_or_init(|| enum_choices(addrs::SEND_TYPES)),
                ..row(id)
            });
        }
        for id in ["send17_tap", "send19_tap", "send21_tap"] {
            rows.push(ParamPresentation {
                choices: MTXTAP.get_or_init(|| enum_choices(addrs::MATRIX_SEND_TYPES)),
                ..row(id)
            });
        }
        // gain/phantom stay statically visible: routed-headamp availability
        // is strip-dependent (`/-ha/[00..39]/index`, -1 = internal source
        // with no headamp; protocol reference v4.09-02 Headamp chapter), so no static
        // presentation can decide it. The engine evaluates
        // `capture_param_applicable` per strip and the frontend additionally
        // gates on the shared `x32:preamp_state` readout, reporting
        // "Unavailable for this source" where no headamp is routed.
        rows
    })
}

pub fn swatches() -> &'static [ParamSwatch] {
    static SW: OnceLock<Vec<ParamSwatch>> = OnceLock::new();
    SW.get_or_init(|| {
        addrs::COLORS
            .iter()
            .enumerate()
            .map(|(i, label)| ParamSwatch {
                value: i as f32,
                label,
                rgb: None,
            })
            .collect()
    })
}

/// Scope restrictions over the canonical catalogue (`restrict_scopes`
/// replaces the static scopes, so both narrowing and widening land here).
pub fn scopes() -> Vec<(&'static str, &'static [StripKind])> {
    let mut out: Vec<(&'static str, &'static [StripKind])> = vec![
        ("gain", INPUT_DSP),
        ("phantom", INPUT_DSP),
        ("trim", INPUT_DSP),
        ("polarity", INPUT_DSP),
        ("source", INPUT_DSP),
        (
            "pan",
            &[
                StripKind::In,
                StripKind::Ret,
                StripKind::Bus,
                StripKind::Main,
            ],
        ),
        ("eq_on", EQ),
        ("eq1_gain", EQ),
        ("eq1_freq", EQ),
        ("eq1_q", EQ),
        ("eq1_type", EQ),
        ("eq2_gain", EQ),
        ("eq2_freq", EQ),
        ("eq2_q", EQ),
        ("eq2_type", EQ),
        ("eq3_gain", EQ),
        ("eq3_freq", EQ),
        ("eq3_q", EQ),
        ("eq3_type", EQ),
        ("eq4_gain", EQ),
        ("eq4_freq", EQ),
        ("eq4_q", EQ),
        ("eq4_type", EQ),
        ("eq5_gain", EQ56),
        ("eq5_freq", EQ56),
        ("eq5_q", EQ56),
        ("eq5_type", EQ56),
        ("eq6_gain", EQ56),
        ("eq6_freq", EQ56),
        ("eq6_q", EQ56),
        ("eq6_type", EQ56),
        ("comp_on", DYN),
        ("comp_mode", DYN),
        ("comp_det", DYN),
        ("comp_env", DYN),
        ("comp_thr", DYN),
        ("comp_ratio", DYN),
        ("comp_knee", DYN),
        ("comp_makeup", DYN),
        ("comp_att", DYN),
        ("comp_hold", DYN),
        ("comp_rel", DYN),
        ("comp_mix", DYN),
        ("comp_auto", DYN),
        ("comp_key_src", &[StripKind::In, StripKind::Bus]),
        ("comp_filter_on", DYN),
        ("comp_filter_type", DYN),
        ("comp_filter_freq", DYN),
        ("dca1_assign", DCA_ASSIGN),
        ("dca2_assign", DCA_ASSIGN),
        ("dca3_assign", DCA_ASSIGN),
        ("dca4_assign", DCA_ASSIGN),
        ("dca5_assign", DCA_ASSIGN),
        ("dca6_assign", DCA_ASSIGN),
        ("dca7_assign", DCA_ASSIGN),
        ("dca8_assign", DCA_ASSIGN),
        (
            "insert_on",
            &[
                StripKind::In,
                StripKind::Bus,
                StripKind::Mtx,
                StripKind::Main,
            ],
        ),
        (
            "insert_sel",
            &[
                StripKind::In,
                StripKind::Bus,
                StripKind::Mtx,
                StripKind::Main,
            ],
        ),
    ];
    for n in 1..=16 {
        let (id_level, id_on, id_pan, id_tap) = (
            match n {
                1 => "send1_level",
                2 => "send2_level",
                3 => "send3_level",
                4 => "send4_level",
                5 => "send5_level",
                6 => "send6_level",
                7 => "send7_level",
                8 => "send8_level",
                9 => "send9_level",
                10 => "send10_level",
                11 => "send11_level",
                12 => "send12_level",
                13 => "send13_level",
                14 => "send14_level",
                15 => "send15_level",
                _ => "send16_level",
            },
            match n {
                1 => "send1_on",
                2 => "send2_on",
                3 => "send3_on",
                4 => "send4_on",
                5 => "send5_on",
                6 => "send6_on",
                7 => "send7_on",
                8 => "send8_on",
                9 => "send9_on",
                10 => "send10_on",
                11 => "send11_on",
                12 => "send12_on",
                13 => "send13_on",
                14 => "send14_on",
                15 => "send15_on",
                _ => "send16_on",
            },
            match n {
                1 => Some("send1_pan"),
                3 => Some("send3_pan"),
                5 => Some("send5_pan"),
                7 => Some("send7_pan"),
                9 => Some("send9_pan"),
                11 => Some("send11_pan"),
                13 => Some("send13_pan"),
                15 => Some("send15_pan"),
                _ => None,
            },
            match n {
                1 => Some("send1_tap"),
                3 => Some("send3_tap"),
                5 => Some("send5_tap"),
                7 => Some("send7_tap"),
                9 => Some("send9_tap"),
                11 => Some("send11_tap"),
                13 => Some("send13_tap"),
                15 => Some("send15_tap"),
                _ => None,
            },
        );
        // Input-side sends (ch/aux/fxrtn → bus 1..16). Bus/main →
        // matrix sends live on the distinct canonical slots 17..22
        // (bus/main → mtx 1..6); the mapping layer rejects 1..16 on
        // bus/main and 17..22 on inputs.
        out.push((id_level, SENDS));
        out.push((id_on, SENDS));
        if let Some(id) = id_pan {
            out.push((id, SENDS));
        }
        if let Some(id) = id_tap {
            out.push((id, SENDS));
        }
    }
    for n in 17..=22 {
        let (id_level, id_on, id_pan, id_tap) = match n {
            17 => (
                "send17_level",
                "send17_on",
                Some("send17_pan"),
                Some("send17_tap"),
            ),
            18 => ("send18_level", "send18_on", None, None),
            19 => (
                "send19_level",
                "send19_on",
                Some("send19_pan"),
                Some("send19_tap"),
            ),
            20 => ("send20_level", "send20_on", None, None),
            21 => (
                "send21_level",
                "send21_on",
                Some("send21_pan"),
                Some("send21_tap"),
            ),
            _ => ("send22_level", "send22_on", None, None),
        };
        out.push((id_level, SENDS_MATRIX));
        out.push((id_on, SENDS_MATRIX));
        if let Some(id) = id_pan {
            out.push((id, SENDS_MATRIX));
        }
        if let Some(id) = id_tap {
            out.push((id, SENDS_MATRIX));
        }
    }
    out
}
