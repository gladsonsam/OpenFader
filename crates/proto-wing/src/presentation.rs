//! Model selectors and conditional standard-plugin controls. Reference manual
//! pp. 42–56. Alternative plugins retain their identity; standard parameters
//! are hidden unless that model is positively selected.
use crate::mapping::{COMP_MODELS, EQ_MODELS, GATE_MODELS};
use openfader_core::{params::*, StripKind};
use std::sync::OnceLock;

pub const INPUTS: &[StripKind] = &[StripKind::In, StripKind::Ret];
pub const DSP: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Main,
    StripKind::Mtx,
];
pub const COMP: &[StripKind] = &[
    StripKind::In,
    StripKind::Bus,
    StripKind::Main,
    StripKind::Mtx,
];
pub const SENDS: &[StripKind] = &[StripKind::In, StripKind::Ret, StripKind::Bus];

pub fn defs() -> &'static [ParamDef] {
    static DEFS: OnceLock<Vec<ParamDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut defs = vec![
            ParamDef {
                id: "wing:send_tap",
                label: "Pre tap source",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: 0.,
                max: 7.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "solo",
                label: "Solo",
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: SCOPES_ALL,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            },
            ParamDef {
                id: "source_select",
                label: "Main/alternate input",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: INPUTS,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "source_a",
                label: "Main input source",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: INPUTS,
                min: 0.,
                max: 16064.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "source_b",
                label: "Alternate input source",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: INPUTS,
                min: 0.,
                max: 16064.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
            ParamDef {
                id: "eq_mode",
                label: "EQ model",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: DSP,
                min: 0.,
                max: 7.,
                unit: "",
                default: ParamDefault::Float(0.),
            },
        ];
        for (id, label, kind, max) in [
            ("delay_on", "Delay", ParamKind::Bool, 1.),
            ("wing:delay_unit", "Delay unit", ParamKind::Enum, 3.),
            ("wing:delay_value", "Unit-tagged delay", ParamKind::Text, 0.),
            ("wing:delay_state", "Live delay state", ParamKind::Text, 0.),
        ] {
            defs.push(ParamDef {
                id,
                label,
                kind,
                scope: ParamScope::Channel,
                scopes: DSP,
                min: 0.,
                max,
                unit: "",
                default: if kind == ParamKind::Bool {
                    ParamDefault::Bool(false)
                } else if kind == ParamKind::Text {
                    ParamDefault::Text("")
                } else {
                    ParamDefault::Float(0.)
                },
            });
        }
        for (id, label) in [
            ("_mg1", "Mute group 1"),
            ("_mg2", "Mute group 2"),
            ("_mg3", "Mute group 3"),
            ("_mg4", "Mute group 4"),
            ("_mg5", "Mute group 5"),
            ("_mg6", "Mute group 6"),
            ("_mg7", "Mute group 7"),
            ("_mg8", "Mute group 8"),
        ] {
            defs.push(ParamDef {
                id,
                label,
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: DSP,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
        }
        defs.push(ParamDef {
            id: "wing:eq_bands",
            label: "EQ band count",
            kind: ParamKind::Enum,
            scope: ParamScope::Channel,
            scopes: DSP,
            min: 6.,
            max: 8.,
            unit: "",
            default: ParamDefault::Float(6.),
        });
        for (id, label) in [
            ("dca5_assign", "DCA 5"),
            ("dca6_assign", "DCA 6"),
            ("dca7_assign", "DCA 7"),
            ("dca8_assign", "DCA 8"),
            ("dca9_assign", "DCA 9"),
            ("dca10_assign", "DCA 10"),
            ("dca11_assign", "DCA 11"),
            ("dca12_assign", "DCA 12"),
            ("dca13_assign", "DCA 13"),
            ("dca14_assign", "DCA 14"),
            ("dca15_assign", "DCA 15"),
            ("dca16_assign", "DCA 16"),
        ] {
            defs.push(ParamDef {
                id,
                label,
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: DSP,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
        }
        for (id, label, min, max, unit) in [
            ("eq7_gain", "EQ 7 gain", -15., 15., "dB"),
            ("eq7_freq", "EQ 7 frequency", 20., 20000., "Hz"),
            ("eq7_q", "EQ 7 Q", 0.442, 10., ""),
            ("eq7_type", "EQ 7 type", 0., 5., ""),
            ("eq8_gain", "EQ 8 gain", -15., 15., "dB"),
            ("eq8_freq", "EQ 8 frequency", 20., 20000., "Hz"),
            ("eq8_q", "EQ 8 Q", 0.442, 10., ""),
            ("eq8_type", "EQ 8 type", 0., 5., ""),
        ] {
            defs.push(ParamDef {
                id,
                label,
                kind: ParamKind::Float,
                scope: ParamScope::Bus,
                scopes: crate::mapping::OUTPUTS,
                min,
                max,
                unit,
                default: ParamDefault::Float(min),
            });
        }
        defs.extend_from_slice(crate::sends::definitions());
        defs.extend_from_slice(crate::filters::DEFS);
        defs.extend_from_slice(crate::source::DEFS);
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
fn choices(models: &'static [&'static str]) -> Vec<ParamChoice> {
    models
        .iter()
        .enumerate()
        .map(|(i, label)| ParamChoice {
            value: i as f32,
            label,
        })
        .collect()
}
pub fn presentations() -> &'static [ParamPresentation] {
    static GATE: OnceLock<Vec<ParamChoice>> = OnceLock::new();
    static COMP: OnceLock<Vec<ParamChoice>> = OnceLock::new();
    static EQ: OnceLock<Vec<ParamChoice>> = OnceLock::new();
    static ROWS: OnceLock<Vec<ParamPresentation>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let mut rows = vec![
            ParamPresentation {
                choices: &[
                    ParamChoice {
                        value: 0.,
                        label: "Metres",
                    },
                    ParamChoice {
                        value: 1.,
                        label: "Feet",
                    },
                    ParamChoice {
                        value: 2.,
                        label: "Milliseconds",
                    },
                    ParamChoice {
                        value: 3.,
                        label: "Samples",
                    },
                ],
                ..row("wing:delay_unit")
            },
            ParamPresentation {
                choices: &[
                    ParamChoice {
                        value: 0.,
                        label: "Input",
                    },
                    ParamChoice {
                        value: 1.,
                        label: "After filter",
                    },
                    ParamChoice {
                        value: 2.,
                        label: "Processing tap 3",
                    },
                    ParamChoice {
                        value: 3.,
                        label: "Processing tap 4",
                    },
                    ParamChoice {
                        value: 4.,
                        label: "Processing tap 5",
                    },
                    ParamChoice {
                        value: 5.,
                        label: "Pre-fader",
                    },
                    ParamChoice {
                        value: 6.,
                        label: "Post-fader",
                    },
                    ParamChoice {
                        value: 7.,
                        label: "After processing",
                    },
                ],
                ..row("wing:send_tap")
            },
            ParamPresentation {
                choices: GATE.get_or_init(|| choices(GATE_MODELS)),
                ..row("gate_mode")
            },
            ParamPresentation {
                choices: COMP.get_or_init(|| choices(COMP_MODELS)),
                ..row("comp_mode")
            },
            ParamPresentation {
                choices: EQ.get_or_init(|| choices(EQ_MODELS)),
                ..row("eq_mode")
            },
            ParamPresentation {
                choices: &[
                    ParamChoice {
                        value: 0.,
                        label: "Main",
                    },
                    ParamChoice {
                        value: 1.,
                        label: "Alternate",
                    },
                ],
                ..row("source_select")
            },
            ParamPresentation {
                choices: &[
                    ParamChoice {
                        value: 1.1,
                        label: "1.1:1",
                    },
                    ParamChoice {
                        value: 1.2,
                        label: "1.2:1",
                    },
                    ParamChoice {
                        value: 1.3,
                        label: "1.3:1",
                    },
                    ParamChoice {
                        value: 1.5,
                        label: "1.5:1",
                    },
                    ParamChoice {
                        value: 1.7,
                        label: "1.7:1",
                    },
                    ParamChoice {
                        value: 2.,
                        label: "2:1",
                    },
                    ParamChoice {
                        value: 2.5,
                        label: "2.5:1",
                    },
                    ParamChoice {
                        value: 3.,
                        label: "3:1",
                    },
                    ParamChoice {
                        value: 3.5,
                        label: "3.5:1",
                    },
                    ParamChoice {
                        value: 4.,
                        label: "4:1",
                    },
                    ParamChoice {
                        value: 5.,
                        label: "5:1",
                    },
                    ParamChoice {
                        value: 6.,
                        label: "6:1",
                    },
                    ParamChoice {
                        value: 8.,
                        label: "8:1",
                    },
                    ParamChoice {
                        value: 10.,
                        label: "10:1",
                    },
                    ParamChoice {
                        value: 20.,
                        label: "20:1",
                    },
                    ParamChoice {
                        value: 50.,
                        label: "50:1",
                    },
                    ParamChoice {
                        value: 100.,
                        label: "100:1",
                    },
                ],
                ..row("comp_ratio")
            },
        ];
        rows.extend_from_slice(crate::filters::PRESENTATIONS);
        rows.extend_from_slice(crate::source::PRESENTATIONS);
        for n in 1..=24 {
            let id = crate::capabilities()
                .iter()
                .copied()
                .find(|id| *id == format!("send{n}_tap"))
                .unwrap();
            rows.push(ParamPresentation {
                choices: &[
                    ParamChoice {
                        value: 3.,
                        label: "Pre tap",
                    },
                    ParamChoice {
                        value: 4.,
                        label: "Post-fader",
                    },
                    ParamChoice {
                        value: 5.,
                        label: "Subgroup",
                    },
                ],
                ..row(id)
            });
        }
        for id in ["gain", "phantom"] {
            rows.push(ParamPresentation {
                hidden: true,
                ..row(id)
            });
            rows.push(ParamPresentation {
                when: Some(ParamCondition::Any {
                    any: &[
                        ParamCondition::All {
                            all: &[
                                ParamCondition::Equals {
                                    id: "source_select",
                                    value: 0.,
                                },
                                ParamCondition::Any {
                                    any: &[
                                        ParamCondition::Range {
                                            id: "source_a",
                                            min: 1001.,
                                            max: 1024.,
                                        },
                                        ParamCondition::Range {
                                            id: "source_a",
                                            min: 3001.,
                                            max: 5048.,
                                        },
                                    ],
                                },
                            ],
                        },
                        ParamCondition::All {
                            all: &[
                                ParamCondition::Equals {
                                    id: "source_select",
                                    value: 1.,
                                },
                                ParamCondition::Any {
                                    any: &[
                                        ParamCondition::Range {
                                            id: "source_b",
                                            min: 1001.,
                                            max: 1024.,
                                        },
                                        ParamCondition::Range {
                                            id: "source_b",
                                            min: 3001.,
                                            max: 5048.,
                                        },
                                    ],
                                },
                            ],
                        },
                    ],
                }),
                ..row(id)
            });
        }
        for (mode, ids) in [
            (
                "gate_mode",
                &[
                    "gate_thr",
                    "gate_range",
                    "gate_att",
                    "gate_hold",
                    "gate_rel",
                ][..],
            ),
            (
                "comp_mode",
                &[
                    "comp_thr",
                    "comp_att",
                    "comp_hold",
                    "comp_rel",
                    "comp_knee",
                    "comp_auto",
                    "comp_ratio",
                    "comp_det",
                    "comp_env",
                ][..],
            ),
            (
                "eq_mode",
                &[
                    "eq1_gain", "eq1_freq", "eq1_q", "eq2_gain", "eq2_freq", "eq2_q", "eq3_gain",
                    "eq3_freq", "eq3_q", "eq4_gain", "eq4_freq", "eq4_q", "eq5_gain", "eq5_freq",
                    "eq5_q", "eq6_gain", "eq6_freq", "eq6_q", "eq7_gain", "eq7_freq", "eq7_q",
                    "eq8_gain", "eq8_freq", "eq8_q",
                ][..],
            ),
        ] {
            for &id in ids {
                rows.push(ParamPresentation {
                    hidden: true,
                    ..row(id)
                });
                let choices = if id == "comp_ratio" {
                    rows.iter()
                        .find(|r| r.id == id && !r.choices.is_empty())
                        .unwrap()
                        .choices
                } else {
                    &[]
                };
                rows.push(ParamPresentation {
                    when: Some(ParamCondition::Equals {
                        id: mode,
                        value: 0.,
                    }),
                    choices,
                    ..row(id)
                });
            }
        }
        for id in ["eq1_type", "eq6_type", "eq8_type"] {
            rows.push(ParamPresentation {
                hidden: true,
                ..row(id)
            });
            rows.push(ParamPresentation {
                when: Some(ParamCondition::Equals {
                    id: "eq_mode",
                    value: 0.,
                }),
                ..row(id)
            });
        }
        for id in ["eq2_type", "eq3_type", "eq4_type", "eq5_type", "eq7_type"] {
            rows.push(ParamPresentation {
                hidden: true,
                ..row(id)
            });
        }
        rows
    })
}

pub fn scopes() -> Vec<(&'static str, &'static [StripKind])> {
    crate::capabilities()
        .iter()
        .filter_map(|&id| {
            let kinds = if id.starts_with("hpf_")
                || id.starts_with("wing:hcf_")
                || id.starts_with("wing:filter_")
            {
                SCOPES_INPUT
            } else if (id.starts_with("dca") && id.ends_with("_assign"))
                || id == "delay_on"
                || id.starts_with("wing:delay_")
            {
                DSP
            } else if id.starts_with("eq7") || id.starts_with("eq8") {
                crate::mapping::OUTPUTS
            } else if id.starts_with("eq") || id == "comp_on" || id == "pan" {
                DSP
            } else if id.starts_with("comp_") {
                COMP
            } else if id.starts_with("send") {
                if crate::sends::definitions().iter().any(|def| def.id == id) {
                    return None;
                }
                if id.ends_with("_pan") || id.ends_with("_tap") {
                    INPUTS
                } else {
                    SENDS
                }
            } else if matches!(
                id,
                "trim"
                    | "polarity"
                    | "source"
                    | "source_select"
                    | "source_a"
                    | "source_b"
                    | "gain"
                    | "phantom"
                    | "hpf_on"
                    | "hpf_freq"
            ) {
                INPUTS
            } else {
                return None;
            };
            Some((id, kinds))
        })
        .collect()
}
