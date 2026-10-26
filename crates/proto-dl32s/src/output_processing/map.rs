//! Inferred output address geometry; no hardware verification is claimed.
use openfader_core::destinations::DestinationValue;
use std::sync::OnceLock;

#[derive(Debug)]
pub(super) struct Spec {
    pub(super) target: String,
    pub(super) target_label: String,
    pub(super) name_slot: Option<u32>,
    pub(super) address: u32,
    pub(super) mode: Option<u32>,
    pub(super) id: String,
    pub(super) label: String,
    pub(super) surface: &'static str,
    pub(super) min: f32,
    pub(super) max: f32,
    pub(super) unit: &'static str,
    pub(super) choices: Vec<(u32, &'static str)>,
}

impl Spec {
    pub(super) fn allows(&self, value: f32) -> bool {
        value.is_finite()
            && if self.choices.is_empty() {
                (self.min..=self.max).contains(&value)
            } else {
                self.choices.iter().any(|(v, _)| *v as f32 == value)
            }
    }

    pub(super) fn raw(&self, value: f32) -> u32 {
        if self.choices.is_empty() {
            value.to_bits()
        } else {
            value as u32
        }
    }

    pub(super) fn decode(&self, raw: u32) -> Option<DestinationValue> {
        let value = if self.choices.is_empty() {
            f32::from_bits(raw)
        } else {
            raw as f32
        };
        self.allows(value).then_some(DestinationValue {
            value,
            verified: true,
        })
    }
}

pub(super) fn specs() -> &'static [Spec] {
    static SPECS: OnceLock<Vec<Spec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        let mut specs = Vec::new();
        let targets = std::iter::once(("main".to_string(), "Main".to_string(), 4347, false, None))
            .chain((1..=8).map(|n| {
                (
                    format!("bus/{n:02}"),
                    format!("Aux {n}"),
                    4434 + (n - 1) * 90,
                    false,
                    Some(49 + n),
                )
            }))
            .chain((1..=6).map(|n| {
                (
                    format!("subgroup/{n:02}"),
                    format!("Subgroup {n}"),
                    4041 + (n - 1) * 51,
                    true,
                    None,
                )
            }));
        for (target, target_label, base, subgroup, name_slot) in targets {
            let comp = if subgroup {
                7
            } else if target == "main" {
                5
            } else {
                8
            };
            let eq = if subgroup {
                15
            } else if target == "main" {
                55
            } else {
                58
            };
            let mut add =
                |offset, id: String, label: String, surface, min, max, unit, choices, mode| {
                    specs.push(Spec {
                        target: target.clone(),
                        target_label: target_label.clone(),
                        name_slot,
                        address: base + offset,
                        mode,
                        id,
                        label,
                        surface,
                        min,
                        max,
                        unit,
                        choices,
                    });
                };
            let binary = || vec![(0, "Off"), (1, "On")];
            add(
                comp + 1,
                "comp_on".into(),
                "Compressor".into(),
                "output_comp",
                0.,
                1.,
                "",
                binary(),
                None,
            );
            for (offset, id, label, min, max, unit) in [
                (2, "comp_makeup", "Makeup", 0., 20., "dB"),
                (3, "comp_thr", "Threshold", -80., 0., "dB"),
                (4, "comp_att", "Attack", 0.1, 300., "ms"),
                (5, "comp_rel", "Release", 50., 3000., "ms"),
                (6, "comp_ratio", "Ratio", 1., 101., ":1"),
            ] {
                add(
                    comp + offset,
                    id.into(),
                    label.into(),
                    "output_comp",
                    min,
                    max,
                    unit,
                    vec![],
                    Some(base + comp),
                );
            }
            add(
                comp + 7,
                "comp_knee".into(),
                "Knee".into(),
                "output_comp",
                0.,
                1.,
                "",
                vec![(0, "Soft"), (1, "Hard")],
                Some(base + comp),
            );
            add(
                eq + 1,
                "eq_on".into(),
                "Parametric EQ".into(),
                "output_peq",
                0.,
                1.,
                "",
                binary(),
                None,
            );
            for band in 1..=4 {
                let first = eq + 2 + (4 - band) * 4;
                let choices = match band {
                    1 => vec![(2, "Bell"), (3, "Low shelf")],
                    4 => vec![(2, "Bell"), (4, "High shelf")],
                    _ => vec![],
                };
                if !choices.is_empty() {
                    add(
                        first,
                        format!("eq{band}_type"),
                        format!("Band {band} shape"),
                        "output_peq",
                        0.,
                        0.,
                        "",
                        choices,
                        Some(base + eq),
                    );
                }
                for (offset, suffix, label, min, max, unit) in [
                    (1, "gain", "gain", -15., 15., "dB"),
                    (2, "freq", "frequency", 20., 20000., "Hz"),
                    (3, "q", "Q", 0.5, 16., ""),
                ] {
                    add(
                        first + offset,
                        format!("eq{band}_{suffix}"),
                        format!("Band {band} {label}"),
                        "output_peq",
                        min,
                        max,
                        unit,
                        vec![],
                        Some(base + eq),
                    );
                }
            }
            if subgroup {
                continue;
            }
            let geq = comp + 8;
            add(
                geq,
                "geq_master".into(),
                "GEQ master".into(),
                "output_geq",
                -12.,
                12.,
                "dB",
                vec![],
                None,
            );
            add(
                geq + 1,
                "geq_on".into(),
                "Graphic EQ".into(),
                "output_geq",
                0.,
                1.,
                "",
                binary(),
                None,
            );
            for (index, label) in [
                "20", "25", "31.5", "40", "50", "63", "80", "100", "125", "160", "200", "250",
                "315", "400", "500", "630", "800", "1k", "1.25k", "1.6k", "2k", "2.5k", "3.15k",
                "4k", "5k", "6.3k", "8k", "10k", "12.5k", "16k", "20k",
            ]
            .iter()
            .enumerate()
            {
                add(
                    geq + 2 + index as u32,
                    format!("geq{}_gain", index + 1),
                    format!("{label} Hz"),
                    "output_geq",
                    -12.,
                    12.,
                    "dB",
                    vec![],
                    None,
                );
            }
            for (first, id, label) in [
                (geq + 33, "hpf", "High-pass"),
                (geq + 37, "lpf", "Low-pass"),
            ] {
                // Neighboring zero words could be enable/mode or an unused
                // filter-record gain. Native movement must identify them.
                add(
                    first + 2,
                    format!("{id}_freq"),
                    format!("{label} frequency"),
                    "output_peq",
                    20.,
                    20000.,
                    "Hz",
                    vec![],
                    None,
                );
            }
        }
        specs
    })
}
