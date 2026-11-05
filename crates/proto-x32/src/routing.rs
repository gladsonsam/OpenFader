//! X32/M32 routing and output patching (protocol reference v4.09-02).
//!
//! Three independent layers, all provider-owned:
//!
//! 1. **Input source selection** — `/ch/NN/config/source` and
//!    `/auxin/NN/config/source`, wire int `0..64`
//!    (`OFF`, `In01..32`, `Aux 1..6`, `USB L`, `USB R`, `Fx 1L..4R`,
//!    `Bus 01..16`). FX returns carry no source leaf. A source write
//!    re-resolves the shared headamp (see `crate::preamp`) and the push
//!    plane already invalidates displayed preamp values on
//!    `/config/source` traffic.
//! 2. **Routing banks** — `/config/routing/IN|PLAY/AUX` (input/aux bank
//!    feeds), `/config/routing/AES50A|AES50B|CARD` (digital port feeds),
//!    `/config/routing/OUT` (physical output bank feeds) and the
//!    `/config/routing/routswitch` REC/PLAY selector. Option tables below
//!    follow the protocol document exactly; the independent emulator only
//!    implements the pre-UIN subset (20 IN / 26 AES / 26 OUT options), so
//!    indices above the emulator envelope are document-defined and flagged
//!    as emulator-unconfirmed in the coverage report.
//! 3. **Output patch** — `/outputs/main|aux|p16|aes|rec/NN/src|pos|invert`
//!    plus `delay/on|time` on `main` only. Source wire int `0..76`
//!    (`OFF`, `Main L`, `Main R`, `M/C`, `MixBus 01..16`, `Matrix 1..6`,
//!    direct outs, `Monitor L/R`, `Talkback`); tap wire enum `0..8`
//!    (`crate::addrs::OUT_POSITIONS`). There is no documented `card` or
//!    `user` *output* plane: card/user routing lives in layer 2 and
//!    `/config/userrout/*`.
//!
//! Live emulator evidence (pinned `mixer-simulator@unrecorded`,
//! fresh state, 127.0.0.4, this slice): fresh reads succeed on all 161
//! advertised output/bank/user/FX destinations; guarded writes round-trip
//! with fresh readback for `outputs/aux/01` (`src`, `pos` 0->1->0,
//! `invert` 0->1->0 via both the patch leaf and the routing control),
//! `outputs/main/01` (`src`, `delay/on` 0->1->0, `delay/time` within
//! 0.101 ms over 0.3..500 ms), `fx/source/01/l` (`Source 1`->`Source 2`->
//! `Source 1`), `/config/routing/IN/1-8`, `/config/routing/routswitch`
//! (both modes), `/config/userrout/out/01` (incl. `Monitor 1`/`Monitor 2`)
//! and `/ch/01/config/source` (cleared->`In 1`->cleared). Stale guards,
//! A->B->A repaints and post-reconnect generations all fail closed.
//! `p16`/`aes`/`rec` planes answer fresh reads here; `rec` has no
//! documented invert leaf and is reported honestly where its invert read
//! is unavailable. `/bus/*/mix/*/level`, `/main/*/mix/*/type` and
//! `/main/m/mix/*/level` are implemented per document and reported as
//! emulator-unconfirmed, never widened to fit the emulator.
use crate::{
    addrs,
    mapping::{Mapping, WireKind},
    X32Driver,
};
use openfader_core::{
    destinations::{DestinationChoice, DestinationControl, DestinationValue},
    routing::{OutputPlane, OutputTapState, RouteState},
    StripId,
};
use rosc::OscType;

/// Input-bank feeds (wire int `0..23`). Document v4.09-02 §routing.
pub const ROUTING_IN: &[&str] = &[
    "AN1-8",
    "AN9-16",
    "AN17-24",
    "AN25-32",
    "A1-8",
    "A9-16",
    "A17-24",
    "A25-32",
    "A33-40",
    "A41-48",
    "B1-8",
    "B9-16",
    "B17-24",
    "B25-32",
    "B33-40",
    "B41-48",
    "CARD1-8",
    "CARD9-16",
    "CARD17-24",
    "CARD25-32",
    "UIN1-8",
    "UIN9-16",
    "UIN17-24",
    "UIN25-32",
];
/// Aux-bank feeds (wire int `0..15`). Index 0 stays `AUX1-4` for backward
/// compatibility even though it feeds AUX 1..6 (document footnote).
pub const ROUTING_IN_AUX: &[&str] = &[
    "AUX1-4", "AN1-2", "AN1-4", "AN1-6", "A1-2", "A1-4", "A1-6", "B1-2", "B1-4", "B1-6", "CARD1-2",
    "CARD1-4", "CARD1-6", "UIN1-2", "UIN1-4", "UIN1-6",
];
/// AES50 port feeds (wire int `0..35`). Document v4.09-02 §routing.
pub const ROUTING_AES: &[&str] = &[
    "AN1-8",
    "AN9-16",
    "AN17-24",
    "AN25-32",
    "A1-8",
    "A9-16",
    "A17-24",
    "A25-32",
    "A33-40",
    "A41-48",
    "B1-8",
    "B9-16",
    "B17-24",
    "B25-32",
    "B33-40",
    "B41-48",
    "CARD1-8",
    "CARD9-16",
    "CARD17-24",
    "CARD25-32",
    "OUT1-8",
    "OUT9-16",
    "P161-8",
    "P169-16",
    "AUX1-6/Mon",
    "AuxIN1-6/TB",
    "UOUT1-8",
    "UOUT9-16",
    "UOUT17-24",
    "UOUT25-32",
    "UOUT33-40",
    "UOUT41-48",
    "UIN1-8",
    "UIN9-16",
    "UIN17-24",
    "UIN25-32",
];
/// Physical-output bank feeds, low nibbles (wire int `0..35`).
pub const ROUTING_OUT_A: &[&str] = &[
    "AN1-4",
    "AN9-12",
    "AN17-20",
    "AN25-28",
    "A1-4",
    "A9-12",
    "A17-20",
    "A25-28",
    "A33-36",
    "A41-44",
    "B1-4",
    "B9-12",
    "B17-20",
    "B25-28",
    "B33-36",
    "B41-44",
    "CARD1-4",
    "CARD9-12",
    "CARD17-20",
    "CARD25-28",
    "OUT1-4",
    "OUT9-12",
    "P161-4",
    "P169-12",
    "AUX/CR",
    "AUX/TB",
    "UOUT1-4",
    "UOUT9-12",
    "UOUT17-20",
    "UOUT25-28",
    "UOUT33-36",
    "UOUT41-44",
    "UIN1-4",
    "UIN9-12",
    "UIN17-20",
    "UIN25-28",
];
/// Physical-output bank feeds, high nibbles (wire int `0..35`).
pub const ROUTING_OUT_B: &[&str] = &[
    "AN5-8",
    "AN13-16",
    "AN21-24",
    "AN29-32",
    "A5-8",
    "A13-16",
    "A21-24",
    "A29-32",
    "A37-40",
    "A45-48",
    "B5-8",
    "B13-16",
    "B21-24",
    "B29-32",
    "B37-40",
    "B45-48",
    "CARD5-8",
    "CARD13-16",
    "CARD21-24",
    "CARD29-32",
    "OUT5-8",
    "OUT13-16",
    "P165-8",
    "P1613-16",
    "AUX/CR",
    "AUX/TB",
    "UOUT5-8",
    "UOUT13-16",
    "UOUT21-24",
    "UOUT29-32",
    "UOUT37-40",
    "UOUT45-48",
    "UIN5-8",
    "UIN13-16",
    "UIN21-24",
    "UIN29-32",
];

/// REC/PLAY bank selector (wire int `0..1`).
pub const ROUTSWITCH: &[&str] = &["REC", "PLAY"];

/// Input source selector groups behind the routing plane
/// (`(key, label, 1-based socket count)`; wire int is the running sum with
/// `0` = cleared). Canonical patch labels are `{Group} {index}`.
pub const INPUT_SOURCE_GROUPS: &[(&str, &str, usize)] = &[
    ("In", "Channels", 32),
    ("Aux", "Aux inputs", 6),
    ("USB", "USB returns", 2),
    ("Fx", "FX returns", 8),
    ("Bus", "Mix buses", 16),
];

/// Canonical label for an input-source wire int (`0` = cleared).
pub fn input_source_label(index: i32) -> anyhow::Result<String> {
    if index == 0 {
        return Ok("—".to_string());
    }
    let mut raw = index;
    for (key, _, count) in INPUT_SOURCE_GROUPS {
        if raw <= *count as i32 {
            return Ok(format!("{key} {raw}"));
        }
        raw -= *count as i32;
    }
    anyhow::bail!("x32: input source {index} out of range 0..64")
}

/// Wire int for a canonical input-source patch (`group` + 1-based `index`).
pub fn input_source_index(group: &str, index: u8) -> anyhow::Result<i32> {
    let mut base = 0;
    for (key, _, count) in INPUT_SOURCE_GROUPS {
        if *key == group {
            anyhow::ensure!(
                (1..=*count as u8).contains(&index),
                "x32: source index out of range"
            );
            return Ok(base + i32::from(index));
        }
        base += *count as i32;
    }
    anyhow::bail!("x32: unknown source group {group:?}")
}

/// `/config/userrout/out/NN` label (wire int `0..208`).
pub fn userrout_out_label(index: i32) -> anyhow::Result<String> {
    Ok(match index {
        0 => "Off 1".to_string(),
        1..=32 => format!("Local {}", index),
        33..=80 => format!("AesA {}", index - 32),
        81..=128 => format!("AesB {}", index - 80),
        129..=160 => format!("Card {}", index - 128),
        161..=166 => format!("AuxIn {}", index - 160),
        167 => "Talkback 1".to_string(),
        168 => "Talkback 2".to_string(),
        169..=184 => format!("Output {}", index - 168),
        185..=200 => format!("P16 {}", index - 184),
        201..=206 => format!("AuxOut {}", index - 200),
        // Canonical numeric identities: the shared capture/recall path
        // parses every output source as `{Group} {index}`.
        207..=208 => format!("Monitor {}", index - 206),
        _ => anyhow::bail!("x32: userrout out {index} out of range 0..208"),
    })
}

/// `/config/userrout/in/NN` label (wire int `0..168`).
pub fn userrout_in_label(index: i32) -> anyhow::Result<String> {
    Ok(match index {
        0 => "Off 1".to_string(),
        1..=32 => format!("Local {}", index),
        33..=80 => format!("AesA {}", index - 32),
        81..=128 => format!("AesB {}", index - 80),
        129..=160 => format!("Card {}", index - 128),
        161..=166 => format!("AuxIn {}", index - 160),
        167 => "Talkback 1".to_string(),
        168 => "Talkback 2".to_string(),
        _ => anyhow::bail!("x32: userrout in {index} out of range 0..168"),
    })
}

/// Source groups for the `/outputs/*` patch planes (counts sum to 77).
pub const OUTPUT_SOURCE_GROUPS: &[(&str, &str, usize)] = &[
    ("Off", "Off", 1),
    ("Main", "Main L/R/M-C", 3),
    ("MixBus", "Mix buses", 16),
    ("Matrix", "Matrices", 6),
    ("DirCh", "Direct out channels", 32),
    ("DirAux", "Direct out aux", 8),
    ("DirFx", "Direct out FX", 8),
    ("Monitor", "Monitor", 2),
    ("Talkback", "Talkback", 1),
];

/// Source groups for `/config/userrout/out/*` (counts sum to 209).
pub const USERROUT_OUT_GROUPS: &[(&str, &str, usize)] = &[
    ("Off", "Off", 1),
    ("Local", "Local inputs", 32),
    ("AesA", "AES50-A", 48),
    ("AesB", "AES50-B", 48),
    ("Card", "Card inputs", 32),
    ("AuxIn", "Aux inputs", 6),
    ("Talkback", "Talkback", 2),
    ("Output", "Outputs", 16),
    ("P16", "P16", 16),
    ("AuxOut", "Aux outputs", 6),
    ("Monitor", "Monitor", 2),
];

/// Source groups for `/config/userrout/in/*` (counts sum to 169).
pub const USERROUT_IN_GROUPS: &[(&str, &str, usize)] = &[
    ("Off", "Off", 1),
    ("Local", "Local inputs", 32),
    ("AesA", "AES50-A", 48),
    ("AesB", "AES50-B", 48),
    ("Card", "Card inputs", 32),
    ("AuxIn", "Aux inputs", 6),
    ("Talkback", "Talkback", 2),
];

/// Source groups for the `/fx/N/source/l|r` insert-source selectors
/// (slots 1..4 × sides L/R = 8 routes). Canonical labels are 1-based
/// `{Source} {index}` (`Source 1` = native `INS` = wire `0`); the
/// `options` list carries the native desk-facing labels
/// (`crate::addrs::FX_SOURCES`) for display.
pub const FX_SOURCE_GROUPS: &[(&str, &str, usize)] =
    &[("Source", "FX source", addrs::FX_SOURCES.len())];

/// Every FX insert-source route id (`fx/source/NN/l|r`, 8 total).
pub fn fx_source_ids() -> Vec<String> {
    let mut ids = Vec::with_capacity(8);
    for slot in 1..=addrs::FX_SOURCE_SLOTS {
        for side in ["l", "r"] {
            ids.push(format!("fx/source/{slot:02}/{side}"));
        }
    }
    ids
}

fn group_sources(
    groups: &'static [(&'static str, &'static str, usize)],
) -> Vec<(&'static str, &'static str, usize)> {
    // The tables above are `const` so they can be shared; the plane
    // contract wants owned statics, which these are.
    groups.to_vec()
}

/// Output-patch + user planes for the engine `output_planes` helper.
/// Routing *bank* planes (`routing/*`) are described separately by
/// [`BANK_PLANES`]: their sockets take one bank enum each, so the engine
/// registry builds their caps by hand (with explicit option labels)
/// instead of the numbered-socket helper.
pub fn planes() -> Vec<OutputPlane> {
    let mut planes: Vec<OutputPlane> = addrs::OUTPUT_PLANES
        .iter()
        .map(|(plane, label, count, _)| OutputPlane {
            key: match *plane {
                "main" => "outputs/main",
                "aux" => "outputs/aux",
                "p16" => "outputs/p16",
                "aes" => "outputs/aes",
                _ => "outputs/rec",
            },
            label,
            count: *count,
            clearable: false,
            sources: group_sources(OUTPUT_SOURCE_GROUPS),
        })
        .collect();
    planes.extend(
        [
            ("userrout/out", "User outputs", 48, USERROUT_OUT_GROUPS),
            ("userrout/in", "User inputs", 32, USERROUT_IN_GROUPS),
        ]
        .into_iter()
        .map(|(key, label, count, groups)| OutputPlane {
            key,
            label,
            count,
            clearable: false,
            sources: group_sources(groups),
        }),
    );
    planes
}

/// One routing-bank destination: the engine-facing socket identity is
/// `Bank {n}` (1-based into `options`); `options` carries the desk-facing
/// bank labels for display.
pub struct BankDest {
    pub id: &'static str,
    pub label: &'static str,
    pub target: &'static str,
    pub options: &'static [&'static str],
}

/// One routing-bank plane descriptor for hand-built registry caps.
pub struct BankPlane {
    pub key: &'static str,
    pub label: &'static str,
    pub destinations: &'static [BankDest],
}

/// Routing-bank planes: input/playback bank feeds, digital-port feeds,
/// physical-output bank feeds and the REC/PLAY selector. Slot→bank maps:
/// `routing/in|play/01..04` = banks `1-8..25-32`; `routing/out-a/01|02`
/// = `OUT/1-4|9-12`; `routing/out-b/01|02` = `OUT/5-8|13-16`.
pub const BANK_PLANES: &[BankPlane] = &[
    BankPlane {
        key: "routing/in",
        label: "Input banks",
        destinations: &[
            BankDest {
                id: "routing/in/01",
                label: "Inputs 1-8",
                target: "in",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/in/02",
                label: "Inputs 9-16",
                target: "in",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/in/03",
                label: "Inputs 17-24",
                target: "in",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/in/04",
                label: "Inputs 25-32",
                target: "in",
                options: ROUTING_IN,
            },
        ],
    },
    BankPlane {
        key: "routing/play",
        label: "Playback banks",
        destinations: &[
            BankDest {
                id: "routing/play/01",
                label: "Playback 1-8",
                target: "play",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/play/02",
                label: "Playback 9-16",
                target: "play",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/play/03",
                label: "Playback 17-24",
                target: "play",
                options: ROUTING_IN,
            },
            BankDest {
                id: "routing/play/04",
                label: "Playback 25-32",
                target: "play",
                options: ROUTING_IN,
            },
        ],
    },
    BankPlane {
        key: "routing/inaux",
        label: "Aux input bank",
        destinations: &[BankDest {
            id: "routing/inaux/01",
            label: "Aux inputs",
            target: "inaux",
            options: ROUTING_IN_AUX,
        }],
    },
    BankPlane {
        key: "routing/playaux",
        label: "Playback aux bank",
        destinations: &[BankDest {
            id: "routing/playaux/01",
            label: "Playback aux",
            target: "playaux",
            options: ROUTING_IN_AUX,
        }],
    },
    BankPlane {
        key: "routing/aes50a",
        label: "AES50-A banks",
        destinations: &[
            BankDest {
                id: "routing/aes50a/01",
                label: "AES50-A 1-8",
                target: "aes50a",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50a/02",
                label: "AES50-A 9-16",
                target: "aes50a",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50a/03",
                label: "AES50-A 17-24",
                target: "aes50a",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50a/04",
                label: "AES50-A 25-32",
                target: "aes50a",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50a/05",
                label: "AES50-A 33-40",
                target: "aes50a",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50a/06",
                label: "AES50-A 41-48",
                target: "aes50a",
                options: ROUTING_AES,
            },
        ],
    },
    BankPlane {
        key: "routing/aes50b",
        label: "AES50-B banks",
        destinations: &[
            BankDest {
                id: "routing/aes50b/01",
                label: "AES50-B 1-8",
                target: "aes50b",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50b/02",
                label: "AES50-B 9-16",
                target: "aes50b",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50b/03",
                label: "AES50-B 17-24",
                target: "aes50b",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50b/04",
                label: "AES50-B 25-32",
                target: "aes50b",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50b/05",
                label: "AES50-B 33-40",
                target: "aes50b",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/aes50b/06",
                label: "AES50-B 41-48",
                target: "aes50b",
                options: ROUTING_AES,
            },
        ],
    },
    BankPlane {
        key: "routing/card",
        label: "Card banks",
        destinations: &[
            BankDest {
                id: "routing/card/01",
                label: "Card 1-8",
                target: "card",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/card/02",
                label: "Card 9-16",
                target: "card",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/card/03",
                label: "Card 17-24",
                target: "card",
                options: ROUTING_AES,
            },
            BankDest {
                id: "routing/card/04",
                label: "Card 25-32",
                target: "card",
                options: ROUTING_AES,
            },
        ],
    },
    BankPlane {
        key: "routing/out-a",
        label: "Output banks 1-4/9-12",
        destinations: &[
            BankDest {
                id: "routing/out-a/01",
                label: "Outputs 1-4",
                target: "out-a",
                options: ROUTING_OUT_A,
            },
            BankDest {
                id: "routing/out-a/02",
                label: "Outputs 9-12",
                target: "out-a",
                options: ROUTING_OUT_A,
            },
        ],
    },
    BankPlane {
        key: "routing/out-b",
        label: "Output banks 5-8/13-16",
        destinations: &[
            BankDest {
                id: "routing/out-b/01",
                label: "Outputs 5-8",
                target: "out-b",
                options: ROUTING_OUT_B,
            },
            BankDest {
                id: "routing/out-b/02",
                label: "Outputs 13-16",
                target: "out-b",
                options: ROUTING_OUT_B,
            },
        ],
    },
    BankPlane {
        key: "routing/mode",
        label: "Record/play selector",
        destinations: &[BankDest {
            id: "routing/mode/01",
            label: "Routing mode",
            target: "mode",
            options: ROUTSWITCH,
        }],
    },
];

/// Display label for an output/bank plane destination id
/// (`outputs/aux/01` → `Aux outputs 1`; `routing/in/02` → bank label;
/// `fx/source/01/l` → `FX 1 L source`).
pub fn route_dest_label(id: &str) -> anyhow::Result<String> {
    let addr = route_addr(id)?;
    match addr {
        RouteAddr::FxSource { slot, ref side } => {
            let side_label = if side == "l" { "L" } else { "R" };
            Ok(format!("FX {slot} {side_label} source"))
        }
        RouteAddr::OutputPatch { plane, slot } => {
            let label = addrs::OUTPUT_PLANES
                .iter()
                .find(|(key, _, _, _)| *key == plane)
                .map(|(_, label, _, _)| *label)
                .ok_or_else(|| anyhow::anyhow!("x32: unknown output plane"))?;
            Ok(format!("{label} {slot}"))
        }
        RouteAddr::Bank { .. } | RouteAddr::Mode => {
            for plane in BANK_PLANES {
                if let Some(dest) = plane.destinations.iter().find(|d| d.id == id) {
                    return Ok(dest.label.to_string());
                }
            }
            anyhow::bail!("x32: unknown route {id:?}")
        }
        RouteAddr::User { out: true, slot } => Ok(format!("User output {slot}")),
        RouteAddr::User { out: false, slot } => Ok(format!("User input {slot}")),
    }
}

/// Illuminated tap control row behind an `/outputs/*` destination, for the
/// telemetry owner and scene-recall results. Sync `destination_controls`
/// rows are built from the same read-fed cache this constructor feeds, so
/// cached and recalled rows cannot drift apart.
pub fn tap_control_row(
    id: &str,
    detail: &OutputPatchDetail,
    revision: u64,
    generation: u64,
) -> anyhow::Result<DestinationControl> {
    Ok(DestinationControl {
        revision: Some(revision),
        generation: Some(generation),
        preset: false,
        members: vec![id.to_string()],
        target: id.to_string(),
        target_label: route_dest_label(id)?,
        target_name: None,
        send_slot: None,
        surface: "routing".into(),
        description: "Select the signal tap for this physical output. The captured source must still be live.".into(),
        id: "tap".into(),
        label: "Tap position".into(),
        tap_tempo: false,
        gesture: None,
        choices: addrs::OUT_POSITIONS
            .iter()
            .enumerate()
            .map(|(i, label)| DestinationChoice {
                value: i as f32,
                label: (*label).into(),
            })
            .collect(),
        range: None,
        current: Some(DestinationValue {
            value: f32::from(detail.tap),
            verified: true,
        }),
    })
}
/// Phase-invert control row behind an `/outputs/*` destination.
/// Documented on every tap-carrying plane except `rec` (whose invert
/// leaf is undocumented and honestly reports unavailable): choices are
/// `{OFF, ON}` as `0`/`1`, surface `routing`, never a local-processor
/// preset member.
pub fn invert_control_row(
    id: &str,
    invert: bool,
    revision: u64,
    generation: u64,
) -> anyhow::Result<DestinationControl> {
    Ok(DestinationControl {
        revision: Some(revision),
        generation: Some(generation),
        preset: false,
        members: vec![id.to_string()],
        target: id.to_string(),
        target_label: route_dest_label(id)?,
        target_name: None,
        send_slot: None,
        surface: "routing".into(),
        description:
            "Invert the polarity of this physical output. The captured source must still be live."
                .into(),
        id: "invert".into(),
        label: "Invert".into(),
        tap_tempo: false,
        gesture: None,
        choices: ["OFF", "ON"]
            .iter()
            .enumerate()
            .map(|(i, label)| DestinationChoice {
                value: i as f32,
                label: (*label).into(),
            })
            .collect(),
        range: None,
        current: Some(DestinationValue {
            value: if invert { 1.0 } else { 0.0 },
            verified: true,
        }),
    })
}

/// Delay-enable control row behind a `/outputs/main/*` destination
/// (main outputs only). Choices `{OFF, ON}`, surface `routing`.
pub fn delay_on_control_row(
    id: &str,
    on: bool,
    revision: u64,
    generation: u64,
) -> anyhow::Result<DestinationControl> {
    Ok(DestinationControl {
        revision: Some(revision),
        generation: Some(generation),
        preset: false,
        members: vec![id.to_string()],
        target: id.to_string(),
        target_label: route_dest_label(id)?,
        target_name: None,
        send_slot: None,
        surface: "routing".into(),
        description:
            "Enable the output delay for this main output. The captured source must still be live."
                .into(),
        id: "delay_on".into(),
        label: "Delay enable".into(),
        tap_tempo: false,
        gesture: None,
        choices: ["OFF", "ON"]
            .iter()
            .enumerate()
            .map(|(i, label)| DestinationChoice {
                value: i as f32,
                label: (*label).into(),
            })
            .collect(),
        range: None,
        current: Some(DestinationValue {
            value: if on { 1.0 } else { 0.0 },
            verified: true,
        }),
    })
}

/// Delay-time control row behind a `/outputs/main/*` destination
/// (main outputs only). Engineering-unit range
/// `[0.3, 500.0]` ms, `0.1` ms desk step; surface `routing`.
pub fn delay_time_control_row(
    id: &str,
    ms: f32,
    revision: u64,
    generation: u64,
) -> anyhow::Result<DestinationControl> {
    Ok(DestinationControl {
        revision: Some(revision),
        generation: Some(generation),
        preset: false,
        members: vec![id.to_string()],
        target: id.to_string(),
        target_label: route_dest_label(id)?,
        target_name: None,
        send_slot: None,
        surface: "routing".into(),
        description: "Output delay time for this main output in milliseconds. The captured source must still be live.".into(),
        id: "delay_time".into(),
        label: "Delay time".into(),
        tap_tempo: false,
        gesture: None,
        choices: Vec::new(),
        range: Some(openfader_core::destinations::DestinationRange {
            min: addrs::OUTPUT_DELAY_MIN_MS,
            max: addrs::OUTPUT_DELAY_MAX_MS,
            unit: "ms".to_string(),
            decimals: 1,
            step: Some(addrs::OUTPUT_DELAY_STEP_MS),
        }),
        current: Some(DestinationValue {
            value: ms,
            verified: true,
        }),
    })
}
/// True for addresses whose desk-side change invalidates cached routing or
/// source-bound preamp state. The push plane already reroutes
/// `/headamp/`, `/-ha/` and `/config/source` into `_preamp_dirty`; the
/// telemetry owner should additionally treat these bank/patch leaves as
/// routing-frame invalidation.
pub fn is_shared_route_addr(addr: &str) -> bool {
    addr.starts_with("/outputs/")
        || addr.starts_with("/config/routing/")
        || addr.starts_with("/config/userrout/")
        || addr.starts_with("/fx/")
}

/// Route-target address behind a plane destination id.
#[derive(Clone, Debug)]
enum RouteAddr {
    OutputPatch {
        plane: String,
        slot: u16,
    },
    Bank {
        addr: String,
        options: &'static [&'static str],
    },
    User {
        out: bool,
        slot: u16,
    },
    Mode,
    FxSource {
        slot: u8,
        side: String,
    },
}

fn bank_name<'a>(names: &[&'a str], slot: u16) -> anyhow::Result<&'a str> {
    let index = usize::from(
        slot.checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("x32: routing slot out of range"))?,
    );
    names
        .get(index)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("x32: routing slot out of range"))
}

fn bank_addr(bank: &str, slot: u16) -> anyhow::Result<(String, &'static [&'static str])> {
    let (addr, options) = match bank {
        "in" => (
            format!(
                "/config/routing/IN/{}",
                bank_name(&["1-8", "9-16", "17-24", "25-32"], slot)?
            ),
            ROUTING_IN,
        ),
        "play" => (
            format!(
                "/config/routing/PLAY/{}",
                bank_name(&["1-8", "9-16", "17-24", "25-32"], slot)?
            ),
            ROUTING_IN,
        ),
        "inaux" => ("/config/routing/IN/AUX".to_string(), ROUTING_IN_AUX),
        "playaux" => ("/config/routing/PLAY/AUX".to_string(), ROUTING_IN_AUX),
        "aes50a" => (
            format!(
                "/config/routing/AES50A/{}",
                bank_name(&["1-8", "9-16", "17-24", "25-32", "33-40", "41-48"], slot)?
            ),
            ROUTING_AES,
        ),
        "aes50b" => (
            format!(
                "/config/routing/AES50B/{}",
                bank_name(&["1-8", "9-16", "17-24", "25-32", "33-40", "41-48"], slot)?
            ),
            ROUTING_AES,
        ),
        // The document lists CARD bank addresses without an option table;
        // the independent emulator serves the AES table there, so the AES
        // table applies pending a hardware check (see coverage report).
        "card" => (
            format!(
                "/config/routing/CARD/{}",
                bank_name(&["1-8", "9-16", "17-24", "25-32"], slot)?
            ),
            ROUTING_AES,
        ),
        "out-a" => (
            format!("/config/routing/OUT/{}", bank_name(&["1-4", "9-12"], slot)?),
            ROUTING_OUT_A,
        ),
        "out-b" => (
            format!(
                "/config/routing/OUT/{}",
                bank_name(&["5-8", "13-16"], slot)?
            ),
            ROUTING_OUT_B,
        ),
        _ => anyhow::bail!("x32: unknown routing bank {bank:?}"),
    };
    // Single-destination banks only serve slot 1.
    if matches!(bank, "inaux" | "playaux") {
        anyhow::ensure!(slot == 1, "x32: routing slot out of range");
    }
    Ok((addr, options))
}

fn route_addr(id: &str) -> anyhow::Result<RouteAddr> {
    if id == "routing/mode/01" {
        return Ok(RouteAddr::Mode);
    }
    if let Some(rest) = id.strip_prefix("fx/source/") {
        let (slot, side) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("x32: invalid FX source route {id:?}"))?;
        let slot: u8 = slot
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: invalid FX source route {id:?}"))?;
        addrs::fx_source_addr(slot, side)?;
        return Ok(RouteAddr::FxSource {
            slot,
            side: side.to_string(),
        });
    }
    if let Some(rest) = id.strip_prefix("outputs/") {
        let (plane, slot) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("x32: invalid output route {id:?}"))?;
        let slot: u16 = slot
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: invalid output route {id:?}"))?;
        addrs::output_addr(plane, slot, "src")?;
        return Ok(RouteAddr::OutputPatch {
            plane: plane.to_string(),
            slot,
        });
    }
    if let Some(rest) = id.strip_prefix("routing/") {
        let (bank, slot) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("x32: invalid routing id {id:?}"))?;
        let slot: u16 = slot
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: invalid routing id {id:?}"))?;
        let (addr, options) = bank_addr(bank, slot)?;
        return Ok(RouteAddr::Bank { addr, options });
    }
    if let Some(rest) = id.strip_prefix("userrout/") {
        let (dir, slot) = rest
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("x32: invalid userrout id {id:?}"))?;
        let slot: u16 = slot
            .parse()
            .map_err(|_| anyhow::anyhow!("x32: invalid userrout id {id:?}"))?;
        match dir {
            "out" => {
                anyhow::ensure!((1..=48).contains(&slot), "x32: userrout slot out of range");
                return Ok(RouteAddr::User { out: true, slot });
            }
            "in" => {
                anyhow::ensure!((1..=32).contains(&slot), "x32: userrout slot out of range");
                return Ok(RouteAddr::User { out: false, slot });
            }
            _ => anyhow::bail!("x32: invalid userrout id {id:?}"),
        }
    }
    anyhow::bail!("x32: unknown route {id:?}")
}

/// Canonical `{Group} {index}` source label behind a plane destination.
/// Bank sockets identify by position (`Bank {n}`, 1-based into the bank
/// option table); the desk-facing bank labels ride the caps `options`
/// list, matching the engine `{Group} {index}` write contract.
fn route_source_label(addr: &RouteAddr, raw: i32) -> anyhow::Result<String> {
    match addr {
        RouteAddr::OutputPatch { .. } => addrs::output_source_label(raw),
        RouteAddr::Bank { options, .. } => {
            let option = options
                .get(raw as usize)
                .ok_or_else(|| anyhow::anyhow!("x32: bank source {raw} out of range"))?;
            let _ = option;
            Ok(format!("Bank {}", raw + 1))
        }
        RouteAddr::User { out: true, .. } => userrout_out_label(raw),
        RouteAddr::User { out: false, .. } => userrout_in_label(raw),
        // Canonical `Mode {n}` identity (the desk `REC`/`PLAY` labels ride
        // the caps `options` list): the shared capture/recall path parses
        // every output source as `{Group} {index}`.
        RouteAddr::Mode => {
            anyhow::ensure!(
                (0..ROUTSWITCH.len() as i32).contains(&raw),
                "x32: routswitch {raw} out of range"
            );
            Ok(format!("Mode {}", raw + 1))
        }
        // Canonical `Source {n}` identity for the FX insert-source
        // selectors (native `0` = `Source 1` = `INS`); the native
        // desk-facing labels ride the caps `options` list.
        RouteAddr::FxSource { .. } => addrs::fx_source_label(raw),
    }
}

/// Wire int behind a canonical source label (round-trips
/// [`route_source_label`]).
fn route_source_index(addr: &RouteAddr, source: &str) -> anyhow::Result<i32> {
    match addr {
        RouteAddr::OutputPatch { .. } => {
            let (group, index) = addrs::output_source_split(source)?;
            let mut base = 0;
            for (key, _, count) in OUTPUT_SOURCE_GROUPS {
                if *key == group {
                    anyhow::ensure!(
                        (1..=*count).contains(&index),
                        "x32: output source is not advertised"
                    );
                    return Ok(base + index as i32 - 1);
                }
                base += *count as i32;
            }
            anyhow::bail!("x32: output source is not advertised: {source:?}")
        }
        RouteAddr::Bank { options, .. } => {
            let (group, index) = addrs::output_source_split(source)?;
            anyhow::ensure!(
                group == "Bank" && index > 0 && index <= options.len(),
                "x32: bank source is not advertised: {source:?}"
            );
            Ok(index as i32 - 1)
        }
        RouteAddr::User { out: true, .. } => {
            let (group, index) = addrs::output_source_split(source)?;
            let mut base = 0;
            for (key, _, count) in USERROUT_OUT_GROUPS {
                if *key == group {
                    anyhow::ensure!(
                        (1..=*count).contains(&index),
                        "x32: userrout source is not advertised"
                    );
                    return Ok(base + index as i32 - 1);
                }
                base += *count as i32;
            }
            // `Talkback 1|2` (internal/external) parse through the group table.
            anyhow::bail!("x32: userrout source is not advertised: {source:?}")
        }
        RouteAddr::User { out: false, .. } => {
            let (group, index) = addrs::output_source_split(source)?;
            let mut base = 0;
            for (key, _, count) in USERROUT_IN_GROUPS {
                if *key == group {
                    anyhow::ensure!(
                        (1..=*count).contains(&index),
                        "x32: userrout source is not advertised"
                    );
                    return Ok(base + index as i32 - 1);
                }
                base += *count as i32;
            }
            anyhow::bail!("x32: userrout source is not advertised: {source:?}")
        }
        RouteAddr::Mode => {
            let (group, index) = addrs::output_source_split(source)?;
            anyhow::ensure!(
                group == "Mode" && (1..=ROUTSWITCH.len()).contains(&index),
                "x32: routswitch source is not advertised: {source:?}"
            );
            Ok(index as i32 - 1)
        }
        RouteAddr::FxSource { .. } => addrs::fx_source_index(source),
    }
}

fn route_src_addr(addr: &RouteAddr) -> String {
    match addr {
        RouteAddr::OutputPatch { plane, slot } => {
            format!("/outputs/{plane}/{slot:02}/src")
        }
        RouteAddr::Bank { addr, .. } => addr.clone(),
        RouteAddr::User { out: true, slot } => format!("/config/userrout/out/{slot:02}"),
        RouteAddr::User { out: false, slot } => format!("/config/userrout/in/{slot:02}"),
        RouteAddr::Mode => "/config/routing/routswitch".to_string(),
        RouteAddr::FxSource { slot, side } => format!("/fx/{slot}/source/{side}"),
    }
}

fn revision_of(src: i32, tap: i32, invert: i32) -> u64 {
    // Content signature only. Public revisions are monotonic counters below,
    // so an observed A -> B -> A transition cannot resurrect a stale guard.
    let mut hash = 0xcbf29ce484222325u64;
    for word in [src as u64, tap as u64, invert as u64] {
        for byte in word.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

/// Recently verified route states, keyed by `(generation, plane id)`. The
/// sync engine contract (`output_route`) cannot do network I/O: the shell
/// cadence fingerprint polls every output destination on every tick, so a
/// blocking read there would stall the mixer surface. Async plane reads
/// and writes refresh this cache; sync readers only ever see
/// independently-read values younger than the TTL, with the session
/// generation rechecked on every hit. Anything older or from a previous
/// session honestly reports unavailable (never a guess, never an
/// own-write echo: entries are stored only after independent readback).
/// Raw `(src, tap, invert)` triples ride alongside so guarded tap writes
/// can compare integers, not labels.
struct CachedRoute {
    state: RouteState,
    signature: u64,
    src: i32,
    tap: i32,
    invert: i32,
    at: std::time::Instant,
}

static ROUTE_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<(u64, String), CachedRoute>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

type RouteSessions =
    std::collections::HashMap<u64, std::sync::Weak<openfader_transport::OscSession>>;
static ROUTE_SESSIONS: std::sync::LazyLock<std::sync::Mutex<RouteSessions>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(RouteSessions::new()));
static NEXT_ROUTE_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
static NEXT_ROUTE_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Independently observed extra-control values (`invert`, `delay_on`,
/// `delay_time`) behind `/outputs/*` destinations, keyed by
/// `(generation, destination id, control id)`. Same freshness contract
/// as the route cache: sync descriptor readers only ever see
/// independently-read values younger than the TTL on the live session,
/// with the session generation rechecked on every hit. Stored only
/// after independent readback, never as an own-write echo. Each
/// control carries its own monotonic observed revision: any observed
/// value change (including a desk-side or guarded delay write) bumps
/// only that control's revision and invalidates stale expected values
/// for exactly that control — delay changes never ride the
/// source/tap guard and source/tap moves never validate a delay write.
struct CachedOutputControl {
    value: f32,
    revision: u64,
    generation: u64,
    at: std::time::Instant,
}

/// Cache key for one extra-control observation.
type OutputCtlKey = (u64, String, String);

static OUTPUT_CTL_CACHE: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<OutputCtlKey, CachedOutputControl>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

fn remember_output_control(
    id: &str,
    control: &str,
    generation: u64,
    value: f32,
) -> anyhow::Result<u64> {
    let mut cache = OUTPUT_CTL_CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: output control cache unavailable"))?;
    if cache.len() > 512 {
        cache.retain(|_, entry| entry.at.elapsed() < ROUTE_CACHE_TTL);
    }
    let key = (generation, id.to_string(), control.to_string());
    let revision = match cache.get(&key) {
        // Float-bit equality: delay readback agrees within a step, but the
        // cached descriptor value is the exact observed float, so only a
        // bitwise-identical observation keeps the revision.
        Some(previous)
            if previous.value.to_bits() == value.to_bits() && previous.generation == generation =>
        {
            previous.revision
        }
        _ => next_counter(&NEXT_ROUTE_REVISION)?,
    };
    cache.insert(
        key,
        CachedOutputControl {
            value,
            revision,
            generation,
            at: std::time::Instant::now(),
        },
    );
    Ok(revision)
}

fn cached_output_control_value(generation: u64, id: &str, control: &str) -> Option<(f32, u64)> {
    OUTPUT_CTL_CACHE.lock().ok().and_then(|cache| {
        cache
            .get(&(generation, id.to_string(), control.to_string()))
            .filter(|hit| hit.generation == generation && hit.at.elapsed() < ROUTE_CACHE_TTL)
            .map(|hit| (hit.value, hit.revision))
    })
}

/// Drop every extra-control row for one destination (route pushes,
/// delay pushes and guarded writes all converge here so no stale
/// descriptor survives its leaf move).
pub(crate) fn invalidate_output_controls(generation: u64, id: &str) {
    if let Ok(mut cache) = OUTPUT_CTL_CACHE.lock() {
        cache.retain(|(gen, cached_id, _), _| *gen != generation || cached_id != id);
    }
}

fn next_counter(counter: &std::sync::atomic::AtomicU64) -> anyhow::Result<u64> {
    // `compare_exchange` loop: `fetch_update` fits but reads less
    // obviously under the workspace's deprecation lint, and the retry
    // budget here is one increment per session or revision.
    let mut current = counter.load(std::sync::atomic::Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("x32: route identity counter exhausted"))?;
        match counter.compare_exchange_weak(
            current,
            next,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        ) {
            Ok(_) => return Ok(next),
            Err(actual) => current = actual,
        }
    }
}

/// How long a verified route state stays servable to sync readers.
pub const ROUTE_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(5);

/// Renewal cadence for the session-owned maintenance task: entries older
/// than [`ROUTE_RENEW_AFTER`] are re-read every [`ROUTE_RENEW_EVERY`], so
/// the oldest servable entry stays at most `RENEW_AFTER` plus one bounded
/// pass duration old — always under [`ROUTE_CACHE_TTL`] while connected.
/// No page timers: the shell's 80 ms poll stays the only meter clock.
const ROUTE_RENEW_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
const ROUTE_RENEW_AFTER: std::time::Duration = std::time::Duration::from_secs(3);

/// Bounded fanout for route hydration/renewal queries (concurrent lanes;
/// same-address queries still serialize in the transport shard guard).
const ROUTE_FANOUT: usize = 8;

/// Demand signal from the push plane to the session-owned maintenance
/// task (see [`route_maintenance_task`]).
#[derive(Debug)]
pub(crate) enum RouteRefresh {
    /// One desk leaf moved: re-read its full triple independently.
    One(String),
    /// Pushes were missed/lagged: drop the session view, re-read all.
    All,
}

fn remember_route(
    id: &str,
    generation: u64,
    src: i32,
    tap: i32,
    invert: i32,
    source: &str,
) -> anyhow::Result<RouteState> {
    let mut cache = ROUTE_CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: route cache unavailable"))?;
    if cache.len() > 512 {
        cache.retain(|_, entry| entry.at.elapsed() < ROUTE_CACHE_TTL);
    }
    let key = (generation, id.to_string());
    let signature = revision_of(src, tap, invert);
    let revision = match cache.get(&key) {
        Some(previous) if previous.signature == signature => previous.state.revision,
        _ => next_counter(&NEXT_ROUTE_REVISION)?,
    };
    let state = RouteState {
        source: source.to_string(),
        revision,
        generation,
    };
    cache.insert(
        key,
        CachedRoute {
            state: state.clone(),
            signature,
            src,
            tap,
            invert,
            at: std::time::Instant::now(),
        },
    );
    Ok(state)
}

/// Sync cache read for the engine `output_route` contract. Returns a hit
/// only for a fresh independently-verified state on the live session.
pub(crate) fn cached_route_state(session_ptr: Option<u64>, id: &str) -> anyhow::Result<RouteState> {
    let generation = session_ptr.ok_or_else(|| anyhow::anyhow!("x32: not connected"))?;
    let cache = ROUTE_CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: route cache unavailable"))?;
    let hit = cache
        .get(&(generation, id.to_string()))
        .ok_or_else(|| anyhow::anyhow!("x32: waiting for independent route readback at {id}"))?;
    anyhow::ensure!(
        hit.state.generation == generation && hit.at.elapsed() < ROUTE_CACHE_TTL,
        "x32: waiting for independent route readback at {id}"
    );
    Ok(hit.state.clone())
}

/// Raw triple behind a fresh cached route state, for exact integer guard
/// comparison (labels alone cannot distinguish wire values).
pub(crate) fn cached_route_raws(
    session_ptr: Option<u64>,
    id: &str,
) -> anyhow::Result<(i32, i32, i32)> {
    let generation = session_ptr.ok_or_else(|| anyhow::anyhow!("x32: not connected"))?;
    let cache = ROUTE_CACHE
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: route cache unavailable"))?;
    let hit = cache
        .get(&(generation, id.to_string()))
        .ok_or_else(|| anyhow::anyhow!("x32: waiting for independent route readback at {id}"))?;
    anyhow::ensure!(
        hit.state.generation == generation && hit.at.elapsed() < ROUTE_CACHE_TTL,
        "x32: waiting for independent route readback at {id}"
    );
    Ok((hit.src, hit.tap, hit.invert))
}

/// Telemetry/push integration hook: record an independently observed route
/// triple (source, tap, invert raws) for a plane destination id. The push
/// owner (lifecycle/telemetry) calls this when desk-originated traffic for
/// `/outputs/*`, `/config/routing/*` or `/config/userrout/*` arrives, so
/// sync readers converge without polling. `generation` must be the live
/// session identity; stale generations are ignored, never stored.
/// Undetectable on-wire transitions stay undetectable: this hook only
/// records what was actually observed, and every guarded write still
/// re-reads before dispatch.
pub fn note_route_observed(id: &str, generation: u64, src: i32, tap: i32, invert: i32) {
    if !ROUTE_SESSIONS
        .lock()
        .ok()
        .and_then(|sessions| sessions.get(&generation).and_then(std::sync::Weak::upgrade))
        .is_some()
    {
        return;
    }
    if !(0..=8).contains(&tap) || !(0..=1).contains(&invert) {
        return;
    }
    let addr = match route_addr(id) {
        Ok(addr) => addr,
        Err(_) => return,
    };
    let Ok(source) = route_source_label(&addr, src) else {
        return;
    };
    let _ = remember_route(id, generation, src, tap, invert, &source);
}

/// Every advertised route destination id: routing-bank rows, output-patch
/// destinations, user planes, the REC/PLAY selector and the 8 FX
/// insert-source selectors (161 total).
pub fn all_route_ids() -> Vec<String> {
    let mut ids = Vec::with_capacity(161);
    for plane in BANK_PLANES {
        ids.extend(plane.destinations.iter().map(|d| d.id.to_string()));
    }
    ids.extend(output_tap_ids());
    for slot in 1..=48u16 {
        ids.push(format!("userrout/out/{slot:02}"));
    }
    for slot in 1..=32u16 {
        ids.push(format!("userrout/in/{slot:02}"));
    }
    ids.extend(fx_source_ids());
    ids
}

/// Destination ids behind one desk-originated address. Output `src`/`pos`
/// /`invert` leaves map to their shared destination id (the refresh
/// re-reads the full triple); output `delay/*` leaves map to the same
/// destination id for control-cache invalidation (delay carries no route
/// state, so only the extra-control cache is dropped); FX
/// `/fx/N/source/l|r` leaves map to their `fx/source/NN/l|r` id.
/// Unknown addresses map to nothing, never a guess.
pub(crate) fn route_ids_for_addr(addr: &str) -> Vec<String> {
    if let Some(rest) = addr.strip_prefix("/outputs/") {
        let mut parts = rest.split('/');
        let (plane, slot, leaf, sub) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next(),
        );
        // Delay leaves are two segments (`delay/on`, `delay/time`).
        if leaf == "delay" {
            let Some(sub) = sub else {
                return Vec::new();
            };
            if !matches!(sub, "on" | "time") || parts.next().is_some() {
                return Vec::new();
            }
            let Ok(slot) = slot.parse::<u16>() else {
                return Vec::new();
            };
            if addrs::output_addr(plane, slot, &format!("delay/{sub}")).is_err() {
                return Vec::new();
            }
            return vec![format!("outputs/{plane}/{slot:02}")];
        }
        if sub.is_some() {
            return Vec::new();
        }
        if !matches!(leaf, "src" | "pos" | "invert") {
            return Vec::new();
        }
        let Ok(slot) = slot.parse::<u16>() else {
            return Vec::new();
        };
        if addrs::output_addr(plane, slot, leaf).is_err() {
            return Vec::new();
        }
        return vec![format!("outputs/{plane}/{slot:02}")];
    }
    if let Some(rest) = addr.strip_prefix("/fx/") {
        let mut parts = rest.split('/');
        let (slot, leaf, side) = (
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
            parts.next().unwrap_or(""),
        );
        if parts.next().is_some() {
            return Vec::new();
        }
        if leaf != "source" || !(side == "l" || side == "r") {
            return Vec::new();
        }
        let Ok(slot) = slot.parse::<u8>() else {
            return Vec::new();
        };
        if addrs::fx_source_addr(slot, side).is_err() {
            return Vec::new();
        }
        return vec![format!("fx/source/{slot:02}/{side}")];
    }
    if addr == "/config/routing/routswitch" {
        return vec!["routing/mode/01".to_string()];
    }
    if addr.starts_with("/config/routing/") {
        let mut out = Vec::new();
        for plane in BANK_PLANES {
            for dest in plane.destinations {
                if let Ok(RouteAddr::Bank {
                    addr: bank_addr, ..
                }) = route_addr(dest.id)
                {
                    if bank_addr == addr {
                        out.push(dest.id.to_string());
                    }
                }
            }
        }
        return out;
    }
    if let Some(rest) = addr.strip_prefix("/config/userrout/out/") {
        if let Ok(slot) = rest.parse::<u16>() {
            if (1..=48).contains(&slot) {
                return vec![format!("userrout/out/{slot:02}")];
            }
        }
        return Vec::new();
    }
    if let Some(rest) = addr.strip_prefix("/config/userrout/in/") {
        if let Ok(slot) = rest.parse::<u16>() {
            if (1..=32).contains(&slot) {
                return vec![format!("userrout/in/{slot:02}")];
            }
        }
        return Vec::new();
    }
    Vec::new()
}

/// True for bank/user/mode/FX destinations, whose single source leaf is
/// the whole route state (`tap`/`invert` companions are fixed `0`).
fn is_single_leaf_id(id: &str) -> bool {
    matches!(
        route_addr(id),
        Ok(RouteAddr::Bank { .. }
            | RouteAddr::User { .. }
            | RouteAddr::Mode
            | RouteAddr::FxSource { .. })
    )
}

/// Push-plane entry: record or invalidate one desk-originated route leaf.
/// Bank/user/mode/FX leaves carry the whole state, so a well-formed int is
/// recorded directly via [`note_route_observed`]; output leaves only
/// invalidate their destination (the maintenance task re-reads the full
/// independent triple, plus the extra-control leaves). Malformed leaves
/// invalidate too — never stored. Stale generations are ignored, never
/// stored.
pub(crate) fn note_route_push(
    msg: &rosc::OscMessage,
    generation: u64,
    dirty: &tokio::sync::mpsc::UnboundedSender<RouteRefresh>,
) {
    let live = ROUTE_SESSIONS
        .lock()
        .map(|sessions| sessions.contains_key(&generation))
        .unwrap_or(false);
    if !live {
        return;
    }
    for id in route_ids_for_addr(&msg.addr) {
        if is_single_leaf_id(&id) {
            if let Ok(src) = crate::mapping::strict_int(msg) {
                note_route_observed(&id, generation, src, 0, 0);
                continue;
            }
        }
        invalidate_route(generation, &id);
        invalidate_output_controls(generation, &id);
        let _ = dirty.send(RouteRefresh::One(id));
    }
}

/// Drop one cached route state (single-leaf desk change invalidates the
/// whole triple; the refresh re-reads it independently). Extra-control
/// rows for the same destination are dropped alongside so delay/invert
/// pushes cannot serve a stale descriptor.
pub(crate) fn invalidate_route(generation: u64, id: &str) {
    if let Ok(mut cache) = ROUTE_CACHE.lock() {
        cache.remove(&(generation, id.to_string()));
    }
    invalidate_output_controls(generation, id);
}

/// Drop every cached route state for a session (missed/lagged pushes make
/// the whole view suspect; the maintenance task re-reads all of it).
/// Extra-control rows go with the route rows: a lagged push makes both
/// suspect, never half-fresh.
pub(crate) fn invalidate_all_routes(generation: u64) {
    if let Ok(mut cache) = ROUTE_CACHE.lock() {
        cache.retain(|(gen, _), _| *gen != generation);
    }
    if let Ok(mut cache) = OUTPUT_CTL_CACHE.lock() {
        cache.retain(|(gen, _, _), _| *gen != generation);
    }
}

/// Cached ids older than `older_than` on one session (TTL renewal set).
pub(crate) fn stale_route_ids(generation: u64, older_than: std::time::Duration) -> Vec<String> {
    ROUTE_CACHE
        .lock()
        .map(|cache| {
            cache
                .iter()
                .filter(|((gen, _), entry)| *gen == generation && entry.at.elapsed() > older_than)
                .map(|((_, id), _)| id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Generation bound to one exact session [`Arc`], without borrowing the
/// driver: session-owned worker tasks pin this identity across every read
/// so a reconnect can never mix generations or sessions inside one pass.
/// Lock order is always sessions-then-cache; nothing takes them reversed.
pub(crate) fn generation_for_session(
    session: &std::sync::Arc<openfader_transport::OscSession>,
) -> anyhow::Result<u64> {
    let mut sessions = ROUTE_SESSIONS
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: route session registry unavailable"))?;
    sessions.retain(|_, value| value.strong_count() != 0);
    for (generation, weak) in sessions.iter() {
        if weak
            .upgrade()
            .is_some_and(|live| std::sync::Arc::ptr_eq(&live, session))
        {
            return Ok(*generation);
        }
    }
    let generation = next_counter(&NEXT_ROUTE_GENERATION)?;
    sessions.insert(generation, std::sync::Arc::downgrade(session));
    // Drop cache rows no live session can serve anymore (lock order is
    // always sessions-then-cache; nothing takes them reversed).
    if let Ok(mut cache) = ROUTE_CACHE.lock() {
        cache.retain(|(gen, _), entry| {
            sessions.contains_key(gen) || entry.at.elapsed() < ROUTE_CACHE_TTL
        });
    }
    Ok(generation)
}

/// Full triple read on a pinned session (no driver borrow, so worker
/// tasks need no `&self`). One [`Arc`] serves every leaf, so no reconnect
/// can mix sessions inside one triple. Fresh readback only; failures are
/// per-destination errors, never cached.
pub(crate) async fn refresh_route_on(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    id: &str,
) -> anyhow::Result<RouteState> {
    let addr = route_addr(id)?;
    anyhow::ensure!(
        generation_for_session(session)? == generation,
        "x32: output session changed during route refresh"
    );
    let src_path = route_src_addr(&addr);
    let src = read_route_int(session, &src_path).await?;
    let (tap, invert) = match &addr {
        RouteAddr::OutputPatch { plane, slot } => {
            let tap = read_route_int(session, &format!("/outputs/{plane}/{slot:02}/pos")).await?;
            let invert = read_output_invert(session, plane, *slot).await?;
            anyhow::ensure!((0..=8).contains(&tap), "x32: output tap not advertised");
            anyhow::ensure!((0..=1).contains(&invert), "x32: output invert invalid");
            (tap, invert)
        }
        RouteAddr::FxSource { .. } => {
            anyhow::ensure!(
                (0..addrs::FX_SOURCE_COUNT).contains(&src),
                "x32: FX source out of range"
            );
            (0, 0)
        }
        _ => (0, 0),
    };
    anyhow::ensure!(
        generation_for_session(session)? == generation,
        "x32: output session changed during route refresh"
    );
    let source = route_source_label(&addr, src)?;
    remember_route(id, generation, src, tap, invert, &source)
}

/// Re-read a small id set on the pinned session with bounded fanout.
/// Per-destination failures are reported and skipped (never cached, never
/// fatal to the pass); renewal or the next desk push retries them.
/// Output destinations refresh both the route triple and the
/// extra-control leaves (`invert`, `delay/on|time` on `main`); a failed
/// extra leaf only drops its own control row, never the route.
async fn refresh_ids(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    ids: &[String],
) {
    let mut set = tokio::task::JoinSet::new();
    for chunk in ids.chunks(ids.len().div_ceil(ROUTE_FANOUT).max(1)) {
        let session = std::sync::Arc::clone(session);
        let chunk = chunk.to_vec();
        set.spawn(async move {
            for id in &chunk {
                if let Err(error) = refresh_route_on(&session, generation, id).await {
                    eprintln!("x32: route refresh {id} unavailable (retry on renewal or desk push): {error:#}");
                    continue;
                }
                if let Err(error) = refresh_output_controls_on(&session, generation, id).await {
                    eprintln!("x32: output control refresh {id} unavailable (retry on renewal or desk push): {error:#}");
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
}

/// Independently re-read the extra-control leaves behind one
/// `/outputs/*` destination id and refresh the per-control cache.
/// Non-output destinations are a no-op. Per-leaf honesty: a failed
/// leaf drops only its own control row (via the missing cache entry),
/// never the route triple and never a sibling control. `rec` has no
/// documented invert leaf and is never probed for that control.
pub(crate) async fn refresh_output_controls_on(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    id: &str,
) -> anyhow::Result<()> {
    let addr = route_addr(id)?;
    let RouteAddr::OutputPatch { plane, slot } = &addr else {
        return Ok(());
    };
    anyhow::ensure!(
        generation_for_session(session)? == generation,
        "x32: output session changed during control refresh"
    );
    // The recorder plane has source/tap only. Never probe or advertise
    // an undocumented leaf even if an emulator happens to answer it.
    if *plane == "rec" {
        return Ok(());
    }
    let invert = read_output_invert(session, plane, *slot).await?;
    anyhow::ensure!((0..=1).contains(&invert), "x32: output invert invalid");
    let _ = remember_output_control(id, "invert", generation, invert as f32)?;
    if *plane == "main" {
        let on = read_route_int(session, &format!("/outputs/{plane}/{slot:02}/delay/on")).await?;
        anyhow::ensure!((0..=1).contains(&on), "x32: output delay switch invalid");
        let _ = remember_output_control(id, "delay_on", generation, on as f32)?;
        let mapping = Mapping {
            path: format!("/outputs/{plane}/{slot:02}/delay/time"),
            kind: WireKind::Linf(addrs::OUTPUT_DELAY_MIN_MS, addrs::OUTPUT_DELAY_MAX_MS),
        };
        let reply = session
            .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
            .await?;
        anyhow::ensure!(
            generation_for_session(session)? == generation,
            "x32: output session changed during control refresh"
        );
        let value = mapping.decode(&reply)?;
        let openfader_core::ParamValue::Float(ms) = value else {
            anyhow::bail!("x32: invalid delay readback")
        };
        let _ = remember_output_control(id, "delay_time", generation, ms)?;
    }
    Ok(())
}

/// Re-read every advertised route on the pinned session. Same per-id
/// honesty as [`refresh_ids`]: an unreadable destination stays
/// unavailable instead of failing (or faking) the whole plane.
pub(crate) async fn hydrate_all_routes(
    session: std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
) {
    refresh_ids(&session, generation, &all_route_ids()).await;
}

/// Session-owned route maintenance: initial full hydration of all 161
/// destinations plus the 42 tap rows and the invert/delay control rows,
/// then desk-push refresh plus TTL renewal. Aborted with the link (see
/// `Link::Drop`); the fanout [`tokio::task::JoinSet`]s drop their query
/// children with the task, so no query task outlives the connection.
pub(crate) async fn route_maintenance_task(
    session: std::sync::Arc<openfader_transport::OscSession>,
    generation: u64,
    mut dirty: tokio::sync::mpsc::UnboundedReceiver<RouteRefresh>,
) {
    hydrate_all_routes(session.clone(), generation).await;
    let mut ticker = tokio::time::interval(ROUTE_RENEW_EVERY);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the immediate tick: hydration just stamped every row.
    ticker.tick().await;
    loop {
        tokio::select! {
            msg = dirty.recv() => {
                let Some(first) = msg else { break };
                // Coalesce bursts: drain what's already queued.
                let mut ids = Vec::new();
                let mut full = matches!(first, RouteRefresh::All);
                if let RouteRefresh::One(id) = first {
                    ids.push(id);
                }
                while let Ok(msg) = dirty.try_recv() {
                    match msg {
                        RouteRefresh::All => {
                            full = true;
                            break;
                        }
                        RouteRefresh::One(id) => ids.push(id),
                    }
                }
                if full {
                    invalidate_all_routes(generation);
                    hydrate_all_routes(session.clone(), generation).await;
                } else {
                    ids.sort();
                    ids.dedup();
                    // A storm of single leaves is cheaper as one pass.
                    if ids.len() > 40 {
                        hydrate_all_routes(session.clone(), generation).await;
                    } else {
                        refresh_ids(&session, generation, &ids).await;
                    }
                }
            }
            _ = ticker.tick() => {
                let stale = stale_route_ids(generation, ROUTE_RENEW_AFTER);
                if !stale.is_empty() {
                    refresh_ids(&session, generation, &stale).await;
                }
            }
        }
    }
}

/// Full live patch detail behind an `/outputs/*` destination id.
#[derive(Clone, Debug)]
pub struct OutputPatchDetail {
    pub source: String,
    pub tap: u8,
    /// Absent for recorder outputs, which have no documented invert leaf.
    pub invert: Option<bool>,
    pub delay_on: Option<bool>,
    pub delay_ms: Option<f32>,
}

/// Independent int read against an explicit session (routing/patch plane).
/// Fresh readback, never a cache: a successful write is not confirmation.
pub(crate) async fn read_route_int(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    path: &str,
) -> anyhow::Result<i32> {
    let reply = session.query_timeout(path, crate::QUERY_TIMEOUT).await?;
    crate::mapping::strict_int(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))
}

// An absent recorder invert leaf contributes a fixed sentinel to the
// internal route signature. It is never returned as an observed control.
async fn read_output_invert(
    session: &std::sync::Arc<openfader_transport::OscSession>,
    plane: &str,
    slot: u16,
) -> anyhow::Result<i32> {
    if plane == "rec" {
        return Ok(0);
    }
    read_route_int(session, &addrs::output_addr(plane, slot, "invert")?).await
}

/// Every tap-carrying output destination id (`outputs/*` planes only:
/// routing banks and user planes carry no tap leaf).
pub(crate) fn output_tap_ids() -> Vec<String> {
    let mut ids = Vec::new();
    for (plane, _, count, _) in addrs::OUTPUT_PLANES {
        let prefix = match *plane {
            "main" => "outputs/main",
            "aux" => "outputs/aux",
            "p16" => "outputs/p16",
            "aes" => "outputs/aes",
            _ => "outputs/rec",
        };
        for slot in 1..=*count {
            ids.push(format!("{prefix}/{slot:02}"));
        }
    }
    ids
}

impl X32Driver {
    /// Generation bound to one exact session [`Arc`]. Every async route
    /// flow pins its session first and derives the generation from that
    /// same [`Arc`], so a reconnect between reads can never mix
    /// generations or sessions across one operation.
    pub(crate) fn route_generation_for(
        &self,
        session: &std::sync::Arc<openfader_transport::OscSession>,
    ) -> anyhow::Result<u64> {
        generation_for_session(session)
    }

    pub(crate) fn route_generation(&self) -> anyhow::Result<u64> {
        let session = self.session()?;
        self.route_generation_for(&session)
    }

    async fn read_route_triple(
        &self,
        session: &std::sync::Arc<openfader_transport::OscSession>,
        addr: &RouteAddr,
    ) -> anyhow::Result<(i32, i32, i32)> {
        let src_path = route_src_addr(addr);
        let src = read_route_int(session, &src_path).await?;
        self.ensure_current(session)?;
        let (tap, invert) = match addr {
            RouteAddr::OutputPatch { plane, slot } => {
                let tap_path = format!("/outputs/{plane}/{slot:02}/pos");
                let tap = read_route_int(session, &tap_path).await?;
                self.ensure_current(session)?;
                let invert = read_output_invert(session, plane, *slot).await?;
                self.ensure_current(session)?;
                anyhow::ensure!((0..=8).contains(&tap), "x32: output tap not advertised");
                anyhow::ensure!((0..=1).contains(&invert), "x32: output invert invalid");
                (tap, invert)
            }
            RouteAddr::FxSource { .. } => {
                anyhow::ensure!(
                    (0..addrs::FX_SOURCE_COUNT).contains(&src),
                    "x32: FX source out of range"
                );
                (0, 0)
            }
            _ => (0, 0),
        };
        Ok((src, tap, invert))
    }

    /// Live route state behind a plane destination id (fresh reads, never
    /// a cache read: a successful write is not confirmation, an independent
    /// readback is). Verified states refresh the sync-serving cache. The
    /// session is pinned first and the generation derives from that same
    /// session, so no reconnect can mix generations across these reads.
    pub async fn output_route_state(&self, id: &str) -> anyhow::Result<RouteState> {
        let addr = route_addr(id)?;
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        let (src, tap, invert) = self.read_route_triple(&session, &addr).await?;
        let source = route_source_label(&addr, src)?;
        remember_route(id, generation, src, tap, invert, &source)
    }

    /// Sync read behind the engine `output_route` contract: serves only
    /// fresh independently-verified states (see the cache contract above).
    pub fn output_route_sync(&self, id: &str) -> anyhow::Result<RouteState> {
        // `route_addr` validation first so unknown ids never hit the cache.
        let _ = route_addr(id)?;
        let generation = self.route_generation().ok();
        cached_route_state(generation, id)
    }

    /// Guarded route write: the caller's `expected` state must still be
    /// live (same generation, source and companion triple), then exactly
    /// one int leaf is written and confirmed by fresh readback with
    /// companions rechecked. The lane pins one session [`Arc`] before the
    /// write lock and every step re-validates that same session, so a
    /// reconnect mid-write fails closed instead of dispatching or
    /// reading back across sessions. OSC has no transaction identity: a
    /// desk-side A -> B -> A repaint across this write is undetectable on
    /// the wire and is never claimed as detected.
    pub async fn set_output_route_guarded(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        let addr = route_addr(id)?;
        let value = route_source_index(&addr, source)?;
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            generation == expected.generation
                && self.route_generation_for(&session)? == expected.generation,
            "x32: output session changed"
        );
        let (src, tap, invert) = self.read_route_triple(&session, &addr).await?;
        let live = remember_route(
            id,
            generation,
            src,
            tap,
            invert,
            &route_source_label(&addr, src)?,
        )?;
        anyhow::ensure!(
            live == *expected,
            "x32: output source changed; reopen the control"
        );
        let path = route_src_addr(&addr);
        self.ensure_current(&session)?;
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        session.send(&path, vec![OscType::Int(value)]).await?;
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let read = crate::mapping::strict_int(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        anyhow::ensure!(
            read == value,
            "x32: output readback did not match the requested write at {path}"
        );
        let (src2, tap2, invert2) = self.read_route_triple(&session, &addr).await?;
        anyhow::ensure!(
            src2 == value && tap2 == tap && invert2 == invert,
            "x32: output companion changed during write; reconnect to refresh"
        );
        remember_route(id, generation, src2, tap2, invert2, source)?;
        Ok(())
    }

    /// Live output-patch detail (source, tap, invert, delay) behind an
    /// `/outputs/*` destination id, plus the verified route state behind
    /// it. The session is pinned first; every read re-validates that same
    /// session, so no reconnect can mix sessions across this detail.
    async fn output_patch_detail_on(
        &self,
        session: &std::sync::Arc<openfader_transport::OscSession>,
        generation: u64,
        id: &str,
    ) -> anyhow::Result<(OutputPatchDetail, RouteState)> {
        let addr = route_addr(id)?;
        let RouteAddr::OutputPatch { plane, slot } = &addr else {
            anyhow::bail!("x32: patch detail is outputs/* only")
        };
        let src = read_route_int(session, &format!("/outputs/{plane}/{slot:02}/src")).await?;
        self.ensure_current(session)?;
        let tap = read_route_int(session, &format!("/outputs/{plane}/{slot:02}/pos")).await?;
        self.ensure_current(session)?;
        let invert = read_output_invert(session, plane, *slot).await?;
        self.ensure_current(session)?;
        let (delay_on, delay_ms) = if *plane == "main" {
            let on =
                read_route_int(session, &format!("/outputs/{plane}/{slot:02}/delay/on")).await?;
            self.ensure_current(session)?;
            let mapping = Mapping {
                path: format!("/outputs/{plane}/{slot:02}/delay/time"),
                kind: WireKind::Linf(0.3, 500.0),
            };
            let reply = session
                .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
                .await?;
            self.ensure_current(session)?;
            let value = mapping.decode(&reply)?;
            let openfader_core::ParamValue::Float(ms) = value else {
                anyhow::bail!("x32: invalid delay readback")
            };
            (Some(on != 0), Some(ms))
        } else {
            (None, None)
        };
        anyhow::ensure!((0..=8).contains(&tap), "x32: output tap not advertised");
        anyhow::ensure!((0..=1).contains(&invert), "x32: output invert invalid");
        let source = addrs::output_source_label(src)?;
        let state = remember_route(id, generation, src, tap, invert, &source)?;
        Ok((
            OutputPatchDetail {
                source,
                tap: tap as u8,
                invert: (*plane != "rec").then_some(invert != 0),
                delay_on,
                delay_ms,
            },
            state,
        ))
    }

    /// Live output-patch detail (source, tap, invert, delay) behind an
    /// `/outputs/*` destination id.
    pub async fn output_patch_detail(&self, id: &str) -> anyhow::Result<OutputPatchDetail> {
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        Ok(self
            .output_patch_detail_on(&session, generation, id)
            .await?
            .0)
    }

    /// Guarded tap/invert/delay write behind an `/outputs/*` destination id.
    /// `leaf` is `pos`, `invert`, `delay/on` (main only) or `delay/time`
    /// (main only, ms `0.3..500`). `guard` binds the write to one verified
    /// route state: same generation, same revision, same source. The lane
    /// pins one session [`Arc`] before the write lock and re-resolves the
    /// source under that lock, so a desk-side repatch between the first
    /// read and the dispatch fails closed instead of binding this leaf to
    /// the new socket. Readback agreement is exact for integral leaves
    /// (`pos`, `invert`, `delay/on`: wire ints); `delay/time` rides a
    /// normalized wire float and agrees within one assumed 0.1 ms desk
    /// step plus f32 error. Returns the post-write detail and its verified
    /// route state.
    pub async fn set_output_patch_leaf(
        &self,
        id: &str,
        leaf: &str,
        value: openfader_core::ParamValue,
        guard: &RouteState,
        expected: &openfader_core::ParamValue,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<(OutputPatchDetail, RouteState)> {
        let addr = route_addr(id)?;
        let RouteAddr::OutputPatch { plane, slot } = &addr else {
            anyhow::bail!("x32: patch detail is outputs/* only")
        };
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        anyhow::ensure!(
            generation == guard.generation,
            "x32: output session changed"
        );
        let (_, before_state) = self
            .output_patch_detail_on(&session, generation, id)
            .await?;
        anyhow::ensure!(
            before_state == *guard,
            "x32: output source changed; reopen the control"
        );
        let mapping = match leaf {
            "pos" => Mapping {
                path: format!("/outputs/{plane}/{slot:02}/pos"),
                kind: WireKind::Enum(addrs::OUT_POSITIONS),
            },
            "invert" if *plane != "rec" => Mapping {
                path: format!("/outputs/{plane}/{slot:02}/invert"),
                kind: WireKind::Bool,
            },
            "delay/on" => Mapping {
                path: addrs::output_addr(plane, *slot, "delay/on")?,
                kind: WireKind::Bool,
            },
            "delay/time" => Mapping {
                path: addrs::output_addr(plane, *slot, "delay/time")?,
                kind: WireKind::Linf(0.3, 500.0),
            },
            _ => anyhow::bail!("x32: unknown output leaf {leaf:?}"),
        };
        let arg = mapping.encode(value.clone())?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            self.route_generation_for(&session)? == guard.generation,
            "x32: output session changed"
        );
        // Re-resolve the source under the write lane: a desk-side repatch
        // between the first read and the dispatch must not silently bind
        // this leaf to the new socket.
        let live_src = read_route_int(&session, &format!("/outputs/{plane}/{slot:02}/src")).await?;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            addrs::output_source_label(live_src).ok().as_deref() == Some(guard.source.as_str()),
            "x32: output source changed; reopen the control"
        );
        let reply = session
            .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
            .await?;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            mapping.decode(&reply)? == *expected,
            "x32: output value changed; reopen the control"
        );
        self.ensure_current(&session)?;
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        session.send(&mapping.path, vec![arg]).await?;
        let reply = session
            .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
            .await?;
        self.ensure_current(&session)?;
        let read = mapping.decode(&reply)?;
        // Integral leaves decode to exact integral floats (one wire int =
        // one step, so any mismatch is a real mismatch, never rounding).
        // `delay/time` is a normalized wire float: agree within one
        // assumed 0.1 ms desk step plus f32 round-trip error.
        let agrees = match (&value, &read) {
            (openfader_core::ParamValue::Bool(a), openfader_core::ParamValue::Bool(b)) => a == b,
            (openfader_core::ParamValue::Float(a), openfader_core::ParamValue::Float(b)) => {
                if !a.is_finite() || !b.is_finite() {
                    false
                } else if leaf == "delay/time" {
                    (a - b).abs() <= 0.101
                } else {
                    a == b
                }
            }
            _ => false,
        };
        anyhow::ensure!(agrees, "x32: output readback did not match requested value");
        let (after, after_state) = self
            .output_patch_detail_on(&session, generation, id)
            .await?;
        anyhow::ensure!(
            after.source == guard.source,
            "x32: output source changed during write; reconnect to refresh"
        );
        Ok((after, after_state))
    }

    /// Sync tap control rows behind every tap-carrying output, for the
    /// neutral `destination_controls` contract. Serves only fresh
    /// independently-verified cache entries on the live session (same
    /// rule as `output_route_sync`); anything cold or stale is omitted,
    /// never synthesized.
    pub(crate) fn tap_control_rows(&self) -> Vec<DestinationControl> {
        let Ok(session) = self.session() else {
            return Vec::new();
        };
        let Ok(generation) = self.route_generation_for(&session) else {
            return Vec::new();
        };
        let Ok(cache) = ROUTE_CACHE.lock() else {
            return Vec::new();
        };
        output_tap_ids()
            .into_iter()
            .filter_map(|id| {
                let hit = cache.get(&(generation, id.clone()))?;
                if hit.state.generation != generation || hit.at.elapsed() >= ROUTE_CACHE_TTL {
                    return None;
                }
                let tap = u8::try_from(hit.tap).ok()?;
                if !(0..=8).contains(&hit.tap) || !(0..=1).contains(&hit.invert) {
                    return None;
                }
                tap_control_row(
                    &id,
                    &OutputPatchDetail {
                        source: hit.state.source.clone(),
                        tap,
                        invert: (!id.starts_with("outputs/rec/")).then_some(hit.invert != 0),
                        delay_on: None,
                        delay_ms: None,
                    },
                    hit.state.revision,
                    generation,
                )
                .ok()
            })
            .collect()
    }

    /// Sync extra-control rows (`invert`, `delay_on`, `delay_time`)
    /// behind every tap-carrying output, for the neutral
    /// `destination_controls` contract. Serves only fresh
    /// independently-verified per-control cache entries on the live
    /// session (same TTL rule as `output_route_sync`); anything cold or
    /// stale is omitted, never synthesized. `rec` has no documented
    /// invert leaf, so no invert row is exposed for that plane.
    pub(crate) fn extra_control_rows(&self) -> Vec<DestinationControl> {
        let Ok(session) = self.session() else {
            return Vec::new();
        };
        let Ok(generation) = self.route_generation_for(&session) else {
            return Vec::new();
        };
        let Ok(cache) = OUTPUT_CTL_CACHE.lock() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for id in output_tap_ids() {
            if id.starts_with("outputs/rec/") {
                continue;
            }
            let fresh = |control: &str| -> Option<(f32, u64)> {
                let hit = cache.get(&(generation, id.clone(), control.to_string()))?;
                if hit.generation != generation || hit.at.elapsed() >= ROUTE_CACHE_TTL {
                    return None;
                }
                Some((hit.value, hit.revision))
            };
            if let Some((value, revision)) = fresh("invert") {
                if !(0.0..=1.0).contains(&value) || value.fract() != 0.0 {
                    continue;
                }
                if let Ok(row) = invert_control_row(id.as_str(), value != 0.0, revision, generation)
                {
                    out.push(row);
                }
            }
            if id.starts_with("outputs/main/") {
                if let Some((value, revision)) = fresh("delay_on") {
                    if (0.0..=1.0).contains(&value) && value.fract() == 0.0 {
                        if let Ok(row) =
                            delay_on_control_row(id.as_str(), value != 0.0, revision, generation)
                        {
                            out.push(row);
                        }
                    }
                }
                if let Some((value, revision)) = fresh("delay_time") {
                    if value.is_finite()
                        && (addrs::OUTPUT_DELAY_MIN_MS..=addrs::OUTPUT_DELAY_MAX_MS)
                            .contains(&value)
                    {
                        if let Ok(row) =
                            delay_time_control_row(id.as_str(), value, revision, generation)
                        {
                            out.push(row);
                        }
                    }
                }
            }
        }
        out
    }

    /// Guarded extra-control write (`invert`, `delay_on`, `delay_time`)
    /// behind an `/outputs/*` destination. The caller's descriptor row
    /// must still be the cached row (same control revision, generation
    /// and verified value); the write lane then re-reads the live leaf
    /// and requires exact agreement before dispatch, and exact readback
    /// (0.101 ms tolerance for the normalized `delay/time` float) plus an
    /// unchanged source companion after. Delay writes never validate
    /// against the source/tap guard: each control carries its own
    /// revision, and a delay move bumps only its own row. Returns the
    /// fresh control row; a missing row after the write fails the result
    /// as uncertain rather than echoing the request.
    pub(crate) async fn write_output_extra_control(
        &self,
        id: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let addr = route_addr(id)?;
        let RouteAddr::OutputPatch { plane, slot } = &addr else {
            anyhow::bail!("x32: output control is outputs/* only");
        };
        anyhow::ensure!(
            control != "invert" || *plane != "rec",
            "x32: recorder output invert is undocumented"
        );
        let kind: WireKind = match control {
            "invert" | "delay_on" => WireKind::Bool,
            "delay_time" => WireKind::Linf(addrs::OUTPUT_DELAY_MIN_MS, addrs::OUTPUT_DELAY_MAX_MS),
            _ => anyhow::bail!("x32: unknown output control {control:?}"),
        };
        if matches!(control, "delay_on" | "delay_time") && *plane != "main" {
            anyhow::bail!("x32: output delay is main outputs only");
        }
        match control {
            "invert" | "delay_on" => {
                anyhow::ensure!(
                    value.is_finite() && value.fract() == 0.0 && (0.0..=1.0).contains(&value),
                    "x32: output {control} is not advertised"
                );
                anyhow::ensure!(
                    expected.is_finite()
                        && expected.fract() == 0.0
                        && (0.0..=1.0).contains(&expected),
                    "x32: output {control} is not advertised"
                );
            }
            _ => {
                anyhow::ensure!(
                    value.is_finite()
                        && (addrs::OUTPUT_DELAY_MIN_MS..=addrs::OUTPUT_DELAY_MAX_MS)
                            .contains(&value),
                    "x32: output delay time out of range 0.3..500 ms"
                );
                anyhow::ensure!(
                    expected.is_finite()
                        && (addrs::OUTPUT_DELAY_MIN_MS..=addrs::OUTPUT_DELAY_MAX_MS)
                            .contains(&expected),
                    "x32: output delay time out of range 0.3..500 ms"
                );
            }
        }
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        let mut before = self
            .extra_control_rows()
            .into_iter()
            .find(|row| row.target == id && row.id == control);
        if before.is_none() {
            // Cold cache after (re)connect: one fresh control read seeds
            // the row from independent readback, never from the request.
            // A failed leaf read leaves the row missing (honestly
            // unavailable) instead of failing the whole write path here;
            // the missing row below reports exactly that.
            let _ = refresh_output_controls_on(&session, generation, id).await;
            before = self
                .extra_control_rows()
                .into_iter()
                .find(|row| row.target == id && row.id == control);
        }
        let before = before.ok_or_else(|| anyhow::anyhow!("x32: {control} unavailable at {id}"))?;
        // The engine passes the whole `routing`-surface block for this
        // target (tap + invert + delay rows); the caller's block must
        // still contain exactly this verified row. A desk-side move (even
        // an A -> B -> A repaint, which carries a new revision) fails
        // closed here.
        anyhow::ensure!(
            snapshot.contains(&before),
            "x32: output {control} changed; reopen the control"
        );
        anyhow::ensure!(
            before
                .current
                .as_ref()
                .is_some_and(|current| current.verified
                    && (current.value == expected
                        || (control == "delay_time"
                            && current.value.is_finite()
                            && expected.is_finite()
                            && (current.value - expected).abs() <= 0.101))),
            "x32: output {control} changed; reopen the control"
        );
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        anyhow::ensure!(
            before.generation == Some(generation),
            "x32: output session changed"
        );
        // Source companion: the extra leaf binds to its socket; a
        // desk-side repatch across this write fails closed.
        let route = cached_route_state(Some(generation), id)?;
        anyhow::ensure!(
            route.generation == generation,
            "x32: output session changed"
        );
        let bound_source = route.source.clone();
        let mapping = Mapping {
            path: match control {
                "invert" => format!("/outputs/{plane}/{slot:02}/invert"),
                "delay_on" => addrs::output_addr(plane, *slot, "delay/on")?,
                _ => addrs::output_addr(plane, *slot, "delay/time")?,
            },
            kind,
        };
        let want = match control {
            "delay_time" => openfader_core::ParamValue::Float(value),
            _ => openfader_core::ParamValue::Bool(value != 0.0),
        };
        let want_expected = match control {
            "delay_time" => openfader_core::ParamValue::Float(expected),
            _ => openfader_core::ParamValue::Bool(expected != 0.0),
        };
        let arg = mapping.encode(want.clone())?;
        let _lane = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            self.route_generation_for(&session)? == generation,
            "x32: output session changed"
        );
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        // Exact agreement with the triple the descriptor was built from,
        // plus the sibling extra leaves: any desk-side move (including an
        // A -> B -> A repaint, which carries a new revision) fails closed
        // here. Delay writes never validate against the tap guard value —
        // each control validates against its own cached leaf — but a moved
        // tap/invert companion still fails the write.
        let (exp_src, exp_tap, exp_inv) = cached_route_raws(Some(generation), id)?;
        let (src2, tap2, invert2) = self.read_route_triple(&session, &addr).await?;
        anyhow::ensure!(
            (src2, tap2, invert2) == (exp_src, exp_tap, exp_inv)
                && route_source_label(&addr, src2).ok().as_deref() == Some(bound_source.as_str()),
            "x32: output source or tap changed; reopen the control"
        );
        if *plane == "main" {
            let on =
                read_route_int(&session, &format!("/outputs/{plane}/{slot:02}/delay/on")).await?;
            self.ensure_current(&session)?;
            if let Some((cached_on, _)) = cached_output_control_value(generation, id, "delay_on") {
                anyhow::ensure!(
                    (on as f32) == cached_on || control == "delay_on",
                    "x32: output delay changed; reopen the control"
                );
            }
            if control != "delay_time" {
                if let Some((cached_ms, _)) =
                    cached_output_control_value(generation, id, "delay_time")
                {
                    let time_mapping = Mapping {
                        path: format!("/outputs/{plane}/{slot:02}/delay/time"),
                        kind: WireKind::Linf(
                            addrs::OUTPUT_DELAY_MIN_MS,
                            addrs::OUTPUT_DELAY_MAX_MS,
                        ),
                    };
                    let reply = session
                        .query_timeout(&time_mapping.path, crate::QUERY_TIMEOUT)
                        .await?;
                    self.ensure_current(&session)?;
                    let live_ms = time_mapping.decode(&reply)?;
                    anyhow::ensure!(
                        live_ms == openfader_core::ParamValue::Float(cached_ms),
                        "x32: output delay changed; reopen the control"
                    );
                }
            }
        }
        let live_read = {
            let reply = session
                .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
                .await?;
            self.ensure_current(&session)?;
            mapping.decode(&reply)?
        };
        // Bool leaves agree exactly; `delay/time` is a normalized wire
        // float, so the live leaf agrees within one assumed 0.1 ms desk
        // step plus f32 round-trip error. A real desk move still fails
        // closed through the revision net above.
        let live_agrees = match (&want_expected, &live_read) {
            (openfader_core::ParamValue::Bool(a), openfader_core::ParamValue::Bool(b)) => a == b,
            (openfader_core::ParamValue::Float(a), openfader_core::ParamValue::Float(b)) => {
                a.is_finite() && b.is_finite() && control == "delay_time" && (a - b).abs() <= 0.101
            }
            _ => false,
        };
        anyhow::ensure!(
            live_agrees,
            "x32: output {control} changed; reopen the control"
        );
        // Re-resolve the source under the write lane: a desk-side repatch
        // between the descriptor read and the dispatch must not silently
        // bind this leaf to the new socket.
        let live_src = read_route_int(&session, &format!("/outputs/{plane}/{slot:02}/src")).await?;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            addrs::output_source_label(live_src).ok().as_deref() == Some(bound_source.as_str()),
            "x32: output source changed; reopen the control"
        );
        session.send(&mapping.path, vec![arg]).await?;
        let reply = session
            .query_timeout(&mapping.path, crate::QUERY_TIMEOUT)
            .await?;
        self.ensure_current(&session)?;
        let read = mapping.decode(&reply)?;
        let agrees = match (&want, &read) {
            (openfader_core::ParamValue::Bool(a), openfader_core::ParamValue::Bool(b)) => a == b,
            (openfader_core::ParamValue::Float(a), openfader_core::ParamValue::Float(b)) => {
                a.is_finite() && b.is_finite() && control == "delay_time" && (a - b).abs() <= 0.101
            }
            _ => false,
        };
        anyhow::ensure!(agrees, "x32: output readback did not match requested value");
        // Post-write verification: source companion unchanged, control
        // value re-observed (never echoed). Companion leaves are
        // re-cached alongside so no stale sibling survives this write.
        let (after_detail, _after_state) = self
            .output_patch_detail_on(&session, generation, id)
            .await?;
        anyhow::ensure!(
            after_detail.source == bound_source,
            "x32: output source changed during write; reconnect to refresh"
        );
        if let Some(invert) = after_detail.invert {
            let _ =
                remember_output_control(id, "invert", generation, if invert { 1.0 } else { 0.0 })?;
        }
        if let (Some(on), Some(ms)) = (after_detail.delay_on, after_detail.delay_ms) {
            let _ =
                remember_output_control(id, "delay_on", generation, if on { 1.0 } else { 0.0 })?;
            let _ = remember_output_control(id, "delay_time", generation, ms)?;
        }
        let row = self
            .extra_control_rows()
            .into_iter()
            .find(|row| row.target == id && row.id == control)
            .ok_or_else(|| {
                anyhow::anyhow!("x32: {control} write result uncertain; reconnect to refresh")
            })?;
        anyhow::ensure!(
            row.current.as_ref().is_some_and(|current| {
                current.verified
                    && match control {
                        "delay_time" => (current.value - value).abs() <= 0.101,
                        _ => current.value == value,
                    }
            }),
            "x32: {control} write result uncertain; reconnect to refresh"
        );
        Ok(row)
    }

    /// Neutral tap-write core behind `set_output_tap_guarded` (source-bound
    /// scene recall) and `set_routing_control_guarded` (live-source control
    /// write). `source` is the bound socket: the captured one for recall,
    /// or the just-read live one for control writes. The descriptor row
    /// the caller validated must still be the cached row (same revision,
    /// generation and verified tap value); the write lane then re-reads
    /// the raw triple and requires exact integer agreement before
    /// dispatch, and exact readback plus unchanged companions after.
    /// Returns the fresh control row; a missing row after the write fails
    /// the result as uncertain rather than echoing the request.
    // Eight arguments: the neutral contract tuple (id, control, value,
    // expected, snapshot, source, check) arrives as one; bundling it
    // would hide the contract mapping.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn write_output_tap_control(
        &self,
        id: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        source: Option<&str>,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        anyhow::ensure!(control == "tap", "x32: unknown output control {control:?}");
        anyhow::ensure!(
            value.is_finite() && value.fract() == 0.0 && (0.0..=8.0).contains(&value),
            "x32: output tap is not advertised"
        );
        anyhow::ensure!(
            expected.is_finite() && expected.fract() == 0.0 && (0.0..=8.0).contains(&expected),
            "x32: output tap is not advertised"
        );
        let tap = value as u8;
        let mut before = self
            .tap_control_rows()
            .into_iter()
            .find(|row| row.target == id);
        if before.is_none() {
            // Cold cache after (re)connect: one fresh detail read seeds the
            // row from independent readback, never from the request.
            let _ = self.output_patch_detail(id).await?;
            before = self
                .tap_control_rows()
                .into_iter()
                .find(|row| row.target == id);
        }
        let before = before.ok_or_else(|| anyhow::anyhow!("x32: tap unavailable at {id}"))?;
        // The engine passes the whole `routing`-surface block for this
        // target (tap plus the invert/delay rows); the caller's block must
        // still contain exactly this verified tap row.
        anyhow::ensure!(
            snapshot.contains(&before),
            "x32: output tap changed; reopen the control"
        );
        anyhow::ensure!(
            before
                .current
                .as_ref()
                .is_some_and(|current| current.verified && current.value == expected),
            "x32: output tap changed; reopen the control"
        );
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        let session = self.session()?;
        let generation = self.route_generation_for(&session)?;
        anyhow::ensure!(
            before.generation == Some(generation),
            "x32: output session changed"
        );
        let route = cached_route_state(Some(generation), id)?;
        anyhow::ensure!(
            Some(route.revision) == before.revision && route.generation == generation,
            "x32: output source or tap changed; reopen the control"
        );
        let bound = match source {
            Some(source) => {
                anyhow::ensure!(
                    route.source == source,
                    "x32: output source changed; reopen the control"
                );
                source.to_string()
            }
            None => route.source.clone(),
        };
        let (expected_raw, tap_raw, invert_raw) = cached_route_raws(Some(generation), id)?;
        let _lane = self.writes.lock().await;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            self.route_generation_for(&session)? == generation,
            "x32: output session changed"
        );
        anyhow::ensure!(check(), "x32: output engine session or policy changed");
        // Exact integer agreement with the triple the descriptor was built
        // from: any desk-side move (including an A -> B -> A repaint,
        // which carries a new revision) fails closed here.
        let addr = route_addr(id)?;
        let (src2, tap2, invert2) = self.read_route_triple(&session, &addr).await?;
        anyhow::ensure!(
            (src2, tap2, invert2) == (expected_raw, tap_raw, invert_raw)
                && route_source_label(&addr, src2).ok().as_deref() == Some(bound.as_str())
                && tap2 as f32 == expected,
            "x32: output source or tap changed; reopen the control"
        );
        let (plane, slot) = match &addr {
            RouteAddr::OutputPatch { plane, slot } => (plane.clone(), *slot),
            _ => anyhow::bail!("x32: tap is outputs/* only"),
        };
        let path = format!("/outputs/{plane}/{slot:02}/pos");
        session
            .send(&path, vec![OscType::Int(i32::from(tap))])
            .await?;
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let read = crate::mapping::strict_int(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        anyhow::ensure!(
            read == i32::from(tap),
            "x32: output readback did not match the requested write at {path}"
        );
        let (src3, tap3, invert3) = self.read_route_triple(&session, &addr).await?;
        anyhow::ensure!(
            src3 == src2 && tap3 == i32::from(tap) && invert3 == invert2,
            "x32: output companion changed during write; reconnect to refresh"
        );
        let state = remember_route(id, generation, src3, tap3, invert3, &bound)?;
        let detail = OutputPatchDetail {
            source: bound,
            tap,
            invert: (plane != "rec").then_some(invert3 != 0),
            delay_on: None,
            delay_ms: None,
        };
        let row = tap_control_row(id, &detail, state.revision, state.generation)?;
        // The row must come from the post-write verification above; a
        // missing row means the cache did not converge, not a success.
        anyhow::ensure!(
            self.tap_control_rows()
                .into_iter()
                .any(|cached| cached == row),
            "x32: tap write result uncertain; reconnect to refresh"
        );
        Ok(row)
    }

    /// Two-pass tap snapshot over every tap-carrying output (source-bound,
    /// for scene recall). Both passes must agree; anything unreadable or
    /// disagreement fails the whole capture rather than mixing revisions.
    pub async fn read_output_taps_snapshot(&self) -> anyhow::Result<Vec<OutputTapState>> {
        let ids = output_tap_ids();
        let mut first = Vec::with_capacity(ids.len());
        for id in &ids {
            let detail = self.output_patch_detail(id).await?;
            first.push((id.clone(), detail.source, detail.tap));
        }
        let mut out = Vec::with_capacity(ids.len());
        for (id, source, tap) in &first {
            let detail = self.output_patch_detail(id).await?;
            anyhow::ensure!(
                &detail.source == source && &detail.tap == tap,
                "x32: output routing changed during tap capture at {id}"
            );
            out.push(OutputTapState {
                target: id.clone(),
                source: source.clone(),
                tap: *tap,
            });
        }
        Ok(out)
    }

    /// Source-bound tap write for scene recall: the live source must still
    /// match the captured one before the tap moves. Returns the
    /// post-write tap state (source, tap) from independent readback.
    pub async fn set_output_tap_bound(
        &self,
        desired: &OutputTapState,
        tap: u8,
        expected: f32,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<OutputTapState> {
        anyhow::ensure!((0..=8).contains(&tap), "x32: tap out of range 0..8");
        let row = self
            .write_output_tap_control(
                &desired.target,
                "tap",
                f32::from(tap),
                expected,
                &self.tap_control_rows_plus(&desired.target).await?,
                Some(&desired.source),
                check,
            )
            .await?;
        let tap = row
            .current
            .as_ref()
            .map(|current| current.value as u8)
            .ok_or_else(|| anyhow::anyhow!("x32: tap write result uncertain"))?;
        Ok(OutputTapState {
            target: desired.target.clone(),
            source: desired.source.clone(),
            tap,
        })
    }

    /// Current descriptor rows plus one fresh detail read for `target`,
    /// so a cold cache seeds from independent readback (never the
    /// request) before the snapshot comparison in the tap-write core.
    async fn tap_control_rows_plus(&self, target: &str) -> anyhow::Result<Vec<DestinationControl>> {
        let mut rows = self.tap_control_rows();
        if !rows.iter().any(|row| row.target == target) {
            let _ = self.output_patch_detail(target).await?;
            rows = self.tap_control_rows();
        }
        Ok(rows
            .into_iter()
            .filter(|row| row.target == target)
            .collect())
    }

    /// Current input-source label for a channel/aux strip (`—` = cleared).
    pub async fn input_source_routing(&self, strip: StripId) -> anyhow::Result<String> {
        let session = self.session()?;
        self.input_source_routing_on(&session, strip).await
    }

    async fn input_source_routing_on(
        &self,
        session: &std::sync::Arc<openfader_transport::OscSession>,
        strip: StripId,
    ) -> anyhow::Result<String> {
        let (root, fam) = crate::mapping::fam_of(strip)?;
        anyhow::ensure!(
            fam.is_ch || fam.is_aux,
            "x32: input source is channel/aux only"
        );
        let index = read_route_int(session, &format!("{root}/config/source")).await?;
        self.ensure_current(session)?;
        input_source_label(index)
    }

    /// Guarded input-source write. `expected` carries the caller's captured
    /// `source` label; the write lands only while it is still live, and the
    /// fresh readback must decode to the requested socket.
    pub async fn patch_input_source_guarded(
        &self,
        strip: StripId,
        group: Option<(&str, u8)>,
        expected: &str,
        check: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<String> {
        let (root, fam) = crate::mapping::fam_of(strip)?;
        anyhow::ensure!(
            fam.is_ch || fam.is_aux,
            "x32: input source is channel/aux only"
        );
        anyhow::ensure!(check(), "x32: routing session or policy changed");
        // Pin one session for the whole guarded write: the pre-read above
        // ran on a possibly older session, so the lane re-resolves and
        // only that lane's session is ever written or read back.
        let session = self.session()?;
        let live = self.input_source_routing_on(&session, strip).await?;
        anyhow::ensure!(
            live == expected,
            "x32: input source changed; reopen the control"
        );
        let value = match group {
            Some((group, index)) => input_source_index(group, index)?,
            None => 0,
        };
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        let path = format!("{root}/config/source");
        let live_again = read_route_int(&session, &path).await?;
        self.ensure_current(&session)?;
        anyhow::ensure!(
            input_source_label(live_again).ok().as_deref() == Some(expected),
            "x32: input source changed; reopen the control"
        );
        session.send(&path, vec![OscType::Int(value)]).await?;
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let read = crate::mapping::strict_int(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        anyhow::ensure!(
            read == value,
            "x32: source readback did not match the requested write at {path}"
        );
        input_source_label(read)
    }
}
