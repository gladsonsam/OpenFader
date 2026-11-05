//! X32/M32 address table and topology.
//!
//! Wire roots (protocol reference v4.09-02): `/ch/NN` (01..32), `/auxin/NN`
//! (01..08), `/fxrtn/NN` (01..08), `/bus/NN` (01..16, zero-padded —
//! unlike X-Air's single-digit buses), `/mtx/NN` (01..06), `/main/st`,
//! `/main/m`, `/dca/N` (1..8, single digit). `/info` is `[ver, name,
//! model, fw]`; X32 models are `X32`, `X32RACK`, `X32C`, `X32P`,
//! `X32CORE`, `M32`, `M32C`, `M32R`.
use openfader_core::{StripFamily, StripId, StripKind};

/// OSC control port for the whole X32/M32 family.
pub const PORT: u16 = 10023;
pub const XREMOTE: &str = "/xremote";
pub const INFO: &str = "/info";
pub const STATUS: &str = "/status";
/// Fire-and-forget clear-solo (`1` = clear, like pressing CLEAR SOLO).
pub const CLEAR_SOLO: &str = "/-action/clearsolo";

/// Global strip index space (80 strips):
/// 0..32 ch, 32..40 auxin, 40..48 fxrtn, 48..64 bus, 64..70 mtx,
/// 70 main stereo, 71 main mono, 72..80 DCA 1..8.
pub const X32_TOPOLOGY: [StripFamily; 8] = [
    StripFamily {
        key: "ch",
        kind: StripKind::In,
        label: "Ch 1-32",
        first: 0,
        count: 32,
        name_prefix: "Ch",
        color_default: 2,
    },
    StripFamily {
        key: "auxin",
        kind: StripKind::Ret,
        label: "AuxIn 1-8",
        first: 32,
        count: 8,
        name_prefix: "Aux",
        color_default: 3,
    },
    StripFamily {
        key: "fxrtn",
        kind: StripKind::Ret,
        label: "FxRtn 1-8",
        first: 40,
        count: 8,
        name_prefix: "FxRtn",
        color_default: 6,
    },
    StripFamily {
        key: "bus",
        kind: StripKind::Bus,
        label: "Bus 1-16",
        first: 48,
        count: 16,
        name_prefix: "Bus",
        color_default: 4,
    },
    StripFamily {
        key: "mtx",
        kind: StripKind::Mtx,
        label: "Mtx 1-6",
        first: 64,
        count: 6,
        name_prefix: "Mtx",
        color_default: 6,
    },
    StripFamily {
        key: "main",
        kind: StripKind::Main,
        label: "Main LR",
        first: 70,
        count: 1,
        name_prefix: "Main",
        color_default: 11,
    },
    StripFamily {
        key: "mainm",
        kind: StripKind::Main,
        label: "Main M/C",
        first: 71,
        count: 1,
        name_prefix: "Mono",
        color_default: 11,
    },
    StripFamily {
        key: "dca",
        kind: StripKind::Dca,
        label: "DCA 1-8",
        first: 72,
        count: 8,
        name_prefix: "DCA",
        color_default: 0,
    },
];

pub const X32_STRIPS: usize = 80;
pub const CHANNELS: usize = 32;
pub const MUTE_GROUPS: u8 = 6;
pub const DCA_GROUPS: u8 = 8;

/// Strip root on the wire plus its family kind.
pub fn root(strip: StripId) -> anyhow::Result<(String, StripKind)> {
    let i = strip.0 as usize;
    let fam = X32_TOPOLOGY
        .iter()
        .find(|f| f.contains(i))
        .ok_or_else(|| anyhow::anyhow!("x32: strip out of range"))?;
    let wire = match fam.key {
        "ch" => format!("/ch/{:02}", i - fam.first + 1),
        "auxin" => format!("/auxin/{:02}", i - fam.first + 1),
        "fxrtn" => format!("/fxrtn/{:02}", i - fam.first + 1),
        "bus" => format!("/bus/{:02}", i - fam.first + 1),
        "mtx" => format!("/mtx/{:02}", i - fam.first + 1),
        "main" => "/main/st".to_string(),
        "mainm" => "/main/m".to_string(),
        "dca" => format!("/dca/{}", i - fam.first + 1),
        _ => anyhow::bail!("x32: unknown family"),
    };
    Ok((wire, fam.kind))
}

/// True for `/info` models of the X32/M32 family. The X-Air driver uses
/// the mirror predicate; both sides refuse foreign maps.
pub fn is_x32_model(model: &str) -> bool {
    let norm: String = model
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    matches!(
        norm.as_str(),
        "x32" | "x32rack" | "x32c" | "x32p" | "x32core" | "m32" | "m32c" | "m32r"
    )
}

/// `/info` → `(model, firmware)`; index 0 is the protocol revision,
/// index 1 the console name. Short replies yield empties, never guesses.
pub fn info_model_firmware(strings: &[String]) -> (String, String) {
    (
        strings.get(2).cloned().unwrap_or_default(),
        strings.get(3).cloned().unwrap_or_default(),
    )
}

/// Scribble color names in wire order (OFF..WHi, 0..15).
pub const COLORS: &[&str] = &[
    "OFF", "RD", "GN", "YE", "BL", "MG", "CY", "WH", "OFFi", "RDi", "GNi", "YEi", "BLi", "MGi",
    "CYi", "WHi",
];

/// Channel/aux input source options: index = wire int 0..64
/// (`OFF`, `In01..32`, `Aux 1..6`, `USB L`, `USB R`, `Fx 1L..4R`,
/// `Bus 01..16`).
pub fn source_label(index: i32) -> anyhow::Result<String> {
    Ok(match index {
        0 => "OFF".to_string(),
        1..=32 => format!("In{index:02}"),
        33..=38 => format!("Aux {}", index - 32),
        39 => "USB L".to_string(),
        40 => "USB R".to_string(),
        41..=48 => {
            let fx = (index - 41) / 2 + 1;
            let side = if (index - 41) % 2 == 0 { "L" } else { "R" };
            format!("Fx {fx}{side}")
        }
        49..=64 => format!("Bus {:02}", index - 48),
        _ => anyhow::bail!("x32: source index {index} out of range 0..64"),
    })
}

/// Key-source options share the same 0..64 table.
pub fn keysrc_label(index: i32) -> anyhow::Result<String> {
    source_label(index)
}

/// Insert select options: index = wire int 0..22
/// (`OFF`, `FX1L..FX8R`, `AUX1..AUX6`).
pub fn insert_label(index: i32) -> anyhow::Result<String> {
    Ok(match index {
        0 => "OFF".to_string(),
        1..=16 => {
            let fx = (index - 1) / 2 + 1;
            let side = if (index - 1) % 2 == 0 { "L" } else { "R" };
            format!("FX{fx}{side}")
        }
        17..=22 => format!("AUX{}", index - 16),
        _ => anyhow::bail!("x32: insert index {index} out of range 0..22"),
    })
}

/// Dynamics ratio options (wire enum 0..11).
pub const DYN_RATIOS: &[f32] = &[
    1.1, 1.3, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.0, 10.0, 20.0, 100.0,
];

/// Gate modes (wire enum 0..4).
pub const GATE_MODES: &[&str] = &["EXP2", "EXP3", "EXP4", "GATE", "DUCK"];
/// Dynamics modes / detectors / envelopes (wire enums 0..1).
pub const DYN_MODES: &[&str] = &["COMP", "EXP"];
pub const DYN_DETS: &[&str] = &["PEAK", "RMS"];
pub const DYN_ENVS: &[&str] = &["LIN", "LOG"];
/// Key filter types (wire enum 0..8).
pub const KEY_FILTER_TYPES: &[&str] = &[
    "LC6", "LC12", "HC6", "HC12", "1.0", "2.0", "3.0", "5.0", "10.0",
];
/// 4-band EQ types for ch/aux/fxrtn/bus (wire enum 0..5).
pub const EQ_TYPES_4BAND: &[&str] = &["LCut", "LShv", "PEQ", "VEQ", "HShv", "HCut"];
/// 6-band EQ types for mtx/main (wire enum 0..13). Indices 6..13 are
/// the documented Butterworth/Bessel/Linkwitz-Riley variants; buses use
/// the 4-band table on all six bands (see `crate::mapping`).
pub const EQ_TYPES_6BAND: &[&str] = &[
    "LCut", "LShv", "PEQ", "VEQ", "HShv", "HCut", "BU6", "BU12", "BS12", "LR12", "BU18", "BU24",
    "BS24", "LR24",
];
/// HPF slopes (wire enum 0..2 → 12/18/24 dB/oct).
pub const HPF_SLOPES: &[&str] = &["12", "18", "24"];
/// Bus-send tap types on odd sends (wire enum 0..5).
pub const SEND_TYPES: &[&str] = &["IN/LC", "<-EQ", "EQ->", "PRE", "POST", "GRP"];
/// Bus/main → matrix send tap types on odd sends (wire enum 0..4).
/// protocol reference v4.09-02 lists five options for bus and main sends (no `GRP`);
/// the header range `[0…5]` is a copy of the channel line, so `GRP` stays
/// rejected until hardware proves otherwise.
pub const MATRIX_SEND_TYPES: &[&str] = &["IN/LC", "<-EQ", "EQ->", "PRE", "POST"];
/// Output tap positions (wire enum 0..8).
pub const OUT_POSITIONS: &[&str] = &[
    "IN/LC", "IN/LC+M", "<-EQ", "<-EQ+M", "EQ->", "EQ->+M", "PRE", "PRE+M", "POST",
];
/// FX slot source options (wire enum 0..17: INS, MIX1..16, M/C).
pub const FX_SOURCES: &[&str] = &[
    "INS", "MIX1", "MIX2", "MIX3", "MIX4", "MIX5", "MIX6", "MIX7", "MIX8", "MIX9", "MIX10",
    "MIX11", "MIX12", "MIX13", "MIX14", "MIX15", "MIX16", "M/C",
];

/// Physical-output patch planes: `(plane, label, slot count, has delay)`.
/// protocol reference v4.09-02 documents `/outputs/main|aux|p16|aes|rec`; only `main`
/// carries per-output delay. There is no documented `card`/`user` output
/// plane: card and user routing live under `/config/routing/*` and
/// `/config/userrout/*` (see `crate::routing`).
pub const OUTPUT_PLANES: &[(&str, &str, u16, bool)] = &[
    ("main", "Main outputs", 16, true),
    ("aux", "Aux outputs", 6, false),
    ("p16", "P16 outputs", 16, false),
    ("aes", "AES outputs", 2, false),
    ("rec", "Recorder outputs", 2, false),
];

/// Output-patch source count: wire int `0..76`.
pub const OUTPUT_SRC_COUNT: i32 = 77;

/// FX insert-source selector: slots 1..4 only, sides L/R, wire int
/// `0..17` (`INS`, `MIX1..16`, `M/C`; protocol reference v4.09-02 p.40). The
/// routing-plane canonical identity is 1-based `{Source} {index}`
/// (`Source 1` = native `INS` = wire `0`), so native `0` maps
/// correctly and never collides with the cleared input-source `0`.
pub const FX_SOURCE_SLOTS: u8 = 4;
pub const FX_SOURCE_COUNT: i32 = 18;

/// Output phase-invert leaf: documented on `main`, `aux`, `p16`,
/// `aes` (the document lists no `rec` invert leaf, so `rec` invert
/// reads may honestly report unavailable).
pub const OUTPUT_INVERT_LEAF: &str = "invert";

/// Per-physical-output delay leaves: `main` outputs only
/// (protocol reference v4.09-02 p.40: `/outputs/main/NN/delay/on|time`).
/// Range `[0.3, 500.0]` ms, desk step `0.1` ms.
pub const OUTPUT_DELAY_MIN_MS: f32 = 0.3;
pub const OUTPUT_DELAY_MAX_MS: f32 = 500.0;
pub const OUTPUT_DELAY_STEP_MS: f32 = 0.1;

/// Wire address for one FX insert-source leaf (`/fx/N/source/l|r`).
pub fn fx_source_addr(slot: u8, side: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        (1..=FX_SOURCE_SLOTS).contains(&slot),
        "x32: FX source is slots 1..4 only"
    );
    anyhow::ensure!(side == "l" || side == "r", "x32: FX source side l|r");
    Ok(format!("/fx/{slot}/source/{side}"))
}

/// Canonical `{Source} {index}` label for an FX insert-source wire int
/// (native `0` = `Source 1` = `INS`).
pub fn fx_source_label(index: i32) -> anyhow::Result<String> {
    anyhow::ensure!(
        (0..FX_SOURCE_COUNT).contains(&index),
        "x32: FX source {index} out of range 0..17"
    );
    Ok(format!("Source {}", index + 1))
}

/// Wire int behind a canonical FX insert-source label.
pub fn fx_source_index(source: &str) -> anyhow::Result<i32> {
    let (group, index) = output_source_split(source)?;
    anyhow::ensure!(
        group == "Source" && (1..=(FX_SOURCE_COUNT as usize)).contains(&index),
        "x32: FX source is not advertised: {source:?}"
    );
    Ok(index as i32 - 1)
}

/// Output-patch source label for a wire int (canonical `{Group} {index}`
/// identity used by the routing plane; the desk-facing order is
/// OFF, Main L/R/M-C, MixBus 01..16, Matrix 1..6, direct outs, Monitor,
/// Talkback).
pub fn output_source_label(index: i32) -> anyhow::Result<String> {
    Ok(match index {
        0 => "Off 1".to_string(),
        1..=3 => format!("Main {}", index),
        4..=19 => format!("MixBus {}", index - 3),
        20..=25 => format!("Matrix {}", index - 19),
        26..=57 => format!("DirCh {}", index - 25),
        58..=65 => format!("DirAux {}", index - 57),
        66..=73 => format!("DirFx {}", index - 65),
        74..=75 => format!("Monitor {}", index - 73),
        76 => "Talkback 1".to_string(),
        _ => anyhow::bail!("x32: output source {index} out of range 0..76"),
    })
}

/// `(group, index-in-group)` for an output-patch source label.
pub fn output_source_split(label: &str) -> anyhow::Result<(String, usize)> {
    let (group, index) = label
        .split_once(' ')
        .ok_or_else(|| anyhow::anyhow!("x32: invalid output source {label:?}"))?;
    let index: usize = index
        .parse()
        .map_err(|_| anyhow::anyhow!("x32: invalid output source {label:?}"))?;
    Ok((group.to_string(), index))
}

/// Wire address for one output-patch leaf (`src`, `pos`, `invert`,
/// `delay/on`, `delay/time`). Delay leaves only exist on `main`.
pub fn output_addr(plane: &str, slot: u16, leaf: &str) -> anyhow::Result<String> {
    let (_, _, count, delay) = OUTPUT_PLANES
        .iter()
        .find(|(key, _, _, _)| *key == plane)
        .ok_or_else(|| anyhow::anyhow!("x32: unknown output plane {plane:?}"))?;
    anyhow::ensure!(
        (1..=*count).contains(&slot),
        "x32: output slot out of range"
    );
    match leaf {
        "src" | "pos" => Ok(format!("/outputs/{plane}/{slot:02}/{leaf}")),
        "invert" => {
            anyhow::ensure!(
                plane != "rec",
                "x32: recorder output invert is undocumented"
            );
            Ok(format!("/outputs/{plane}/{slot:02}/{leaf}"))
        }
        "delay/on" | "delay/time" => {
            anyhow::ensure!(*delay, "x32: output delay is main outputs only");
            Ok(format!("/outputs/{plane}/{slot:02}/{leaf}"))
        }
        _ => anyhow::bail!("x32: unknown output leaf {leaf:?}"),
    }
}
