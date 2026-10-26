//! WING send destinations and additional parameter IDs. Slots 1–16 are buses,
//! 17–24 are matrices, 25–28 are mains. See manual pp.44–54 and frozen 3.0.5
//! schema: input/aux sends have mode/pan; output sends have a boolean pre tap.
use openfader_core::{params::*, StripKind};
use std::sync::OnceLock;

pub const MATRIX_SOURCES: &[StripKind] = &[
    StripKind::In,
    StripKind::Ret,
    StripKind::Bus,
    StripKind::Main,
];
pub const OUTPUT_SEND_SOURCES: &[StripKind] = &[StripKind::Bus, StripKind::Main];
pub const BUS_SOURCE: &[StripKind] = &[StripKind::Bus];

#[derive(Clone, Copy)]
pub struct SendIds {
    pub slot: u16,
    pub level: &'static str,
    pub on: &'static str,
    pub pan: &'static str,
    pub tap: &'static str,
    pub pre: &'static str,
}
macro_rules! ids {
    ($n:literal) => {
        SendIds {
            slot: $n,
            level: concat!("send", stringify!($n), "_level"),
            on: concat!("send", stringify!($n), "_on"),
            pan: concat!("send", stringify!($n), "_pan"),
            tap: concat!("send", stringify!($n), "_tap"),
            pre: concat!("send", stringify!($n), "_pre"),
        }
    };
}
pub const IDS: &[SendIds] = &[
    ids!(1),
    ids!(2),
    ids!(3),
    ids!(4),
    ids!(5),
    ids!(6),
    ids!(7),
    ids!(8),
    ids!(9),
    ids!(10),
    ids!(11),
    ids!(12),
    ids!(13),
    ids!(14),
    ids!(15),
    ids!(16),
    ids!(17),
    ids!(18),
    ids!(19),
    ids!(20),
    ids!(21),
    ids!(22),
    ids!(23),
    ids!(24),
    ids!(25),
    ids!(26),
    ids!(27),
    ids!(28),
];

pub fn target(slot: u16) -> Option<(&'static str, usize, &'static str)> {
    match slot {
        1..=16 => Some(("bus", 48 + (slot - 1) as usize, "Bus")),
        17..=24 => Some(("mtx", 68 + (slot - 17) as usize, "Matrix")),
        25..=28 => Some(("main", 64 + (slot - 25) as usize, "Main")),
        _ => None,
    }
}

pub fn definitions() -> &'static [ParamDef] {
    static DEFS: OnceLock<Vec<ParamDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut defs = Vec::new();
        for id in IDS {
            let source = if id.slot <= 16 {
                BUS_SOURCE
            } else if id.slot <= 24 {
                OUTPUT_SEND_SOURCES
            } else {
                super::presentation::SENDS
            };
            defs.push(ParamDef {
                id: id.pre,
                label: "Pre-fader",
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: source,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
            if id.slot <= 16 {
                continue;
            }
            let source = if id.slot <= 24 {
                MATRIX_SOURCES
            } else {
                super::presentation::SENDS
            };
            defs.push(ParamDef {
                id: id.level,
                label: "Send level",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: source,
                min: -144.,
                max: 10.,
                unit: "dB",
                default: ParamDefault::Float(-144.),
            });
            defs.push(ParamDef {
                id: id.on,
                label: "Send on",
                kind: ParamKind::Bool,
                scope: ParamScope::Channel,
                scopes: source,
                min: 0.,
                max: 1.,
                unit: "",
                default: ParamDefault::Bool(false),
            });
            if id.slot <= 24 {
                defs.push(ParamDef {
                    id: id.pan,
                    label: "Send pan",
                    kind: ParamKind::Float,
                    scope: ParamScope::Channel,
                    scopes: super::presentation::INPUTS,
                    min: -100.,
                    max: 100.,
                    unit: "L/R",
                    default: ParamDefault::Float(0.),
                });
                defs.push(ParamDef {
                    id: id.tap,
                    label: "Send mode",
                    kind: ParamKind::Enum,
                    scope: ParamScope::Channel,
                    scopes: super::presentation::INPUTS,
                    min: 3.,
                    max: 5.,
                    unit: "",
                    default: ParamDefault::Float(4.),
                });
            }
        }
        defs
    })
}
