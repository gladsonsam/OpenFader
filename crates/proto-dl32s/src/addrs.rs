//! DL32S input-strip address table.
//!
//! Port of the proven Python `addressmap.py`: every parameter is an address
//! in one flat value space, and input channels are a regular array,
//! `address = base + (channel - 1) * stride + offset`.
//!
//! The DL32S map below is verified against hardware (firmware
//! `0106000100000002`, full 5257-address dump). DL16S/DL32R layouts are NOT
//! verified here and stay unimplemented — see [`model_status`].

use openfader_core::FaderScale;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// DL32S input strip geometry.
pub const INPUT_BASE: u32 = 1;
pub const INPUT_STRIDE: u32 = 106;
pub const INPUT_COUNT: u32 = 32;

/// DL16S geometry (inherited, unverified): 16 inputs, 6 aux sends.
pub const DL16S_STRIDE: u32 = 100;
/// DL32R geometry (inherited, unverified): 32 inputs on a different base.
pub const DL32R_BASE: u32 = 41;
pub const DL32R_STRIDE: u32 = 132;

/// Fader curve in dB, matching the mixer's own scale.
pub const DB_POINTS: &[f32] = &[-120.0, -60.0, -40.0, -30.0, -20.0, -10.0, -5.0, 0.0, 5.0, 10.0];

/// Canonical 0.0..=1.0 level to wire dB and back.
pub static DL32S_FADER_SCALE: FaderScale = FaderScale::from_uniform(DB_POINTS);

/// Gate a configured model name. Only the verified DL32S map is served;
/// anything else explains itself as unimplemented.
pub fn model_status(model: &str) -> anyhow::Result<()> {
    match model.trim().to_lowercase().as_str() {
        "dl32s" | "auto" | "" => Ok(()),
        "dl16s" => anyhow::bail!(
            "dl16s is not mapped: its EQ block (offsets 34-49) is packed \
            differently from the DL32S and the inherited layout is unconfirmed"
        ),
        "dl32r" => anyhow::bail!(
            "dl32r is not mapped: only mute (offset 7) and fader \
            (offset 8) are trustworthy; the rest is inherited and unverified"
        ),
        _ => anyhow::bail!("unknown mixer model"),
    }
}

/// Wire encoding of one field: floats travel as BE IEEE-754 (dB for levels,
/// plain floats for pans/freqs), everything else as plain u32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Bool,
    Db,
    Float,
    Enum,
    Int,
}

impl Encoding {
    pub fn is_float(self) -> bool {
        matches!(self, Self::Db | Self::Float)
    }
}

/// One addressable parameter within the input strip.
#[derive(Clone, Debug)]
pub struct Field {
    pub key: &'static str,
    pub offset: u32,
    /// Natural-unit bounds for validation; `None` means unbounded.
    pub limits: Option<(f32, f32)>,
    pub encoding: Encoding,
    pub label: &'static str,
    /// `false` when the position is certain but the meaning is inferred from
    /// structure rather than seen changing under a known control movement.
    pub verified: bool,
}

const fn field(
    key: &'static str,
    offset: u32,
    encoding: Encoding,
    label: &'static str,
    limits: Option<(f32, f32)>,
    verified: bool,
) -> Field {
    Field {
        key,
        offset,
        encoding,
        label,
        limits,
        verified,
    }
}

/// The head of the strip (offsets 0-34), identical across DL16S/DL32S.
fn input_head() -> Vec<Field> {
    use Encoding::*;
    vec![
        field("source_a", 0, Int, "Input A source", None, true),
        field("source_b", 1, Int, "Input B source", None, true),
        field("source_select", 2, Enum, "Source A/B", None, true),
        field("trim", 3, Db, "Trim", Some((-30.0, 30.0)), true),
        field("icon", 4, Int, "Icon", None, true),
        field("colour", 5, Int, "Colour", None, true),
        field("polarity", 6, Bool, "Polarity invert", None, true),
        field("mute", 7, Bool, "Mute", None, true),
        field("fader", 8, Db, "LR fader", Some((-120.0, 10.0)), true),
        field("pan", 9, Float, "Pan", Some((-1.0, 1.0)), true),
        field("main_assign", 10, Bool, "Assign to main", None, true),
        field("stereo_link", 11, Bool, "Stereo link", None, true),
        field("gain", 12, Db, "Preamp gain", Some((-20.0, 60.0)), true),
        field("phantom", 13, Bool, "48V phantom", None, true),
        field("hpf_on", 14, Bool, "HPF on", None, true),
        field("hpf_freq", 15, Float, "HPF frequency", Some((20.0, 400.0)), true),
        field("hpf_slope", 16, Int, "HPF slope", None, false),
        field("gate_mode", 17, Enum, "Gate modern/vintage", None, true),
        field("gate_on", 18, Bool, "Gate on", None, true),
        field("gate_threshold", 19, Db, "Gate threshold", Some((-120.0, 0.0)), true),
        field("gate_attack", 20, Float, "Gate attack ms", None, false),
        field("gate_release", 21, Float, "Gate release ms", None, false),
        field("gate_range", 22, Float, "Gate range", None, false),
        field("gate_hold", 23, Float, "Gate hold ms", None, false),
        field("comp_mode", 24, Enum, "Comp modern/vintage", None, true),
        field("comp_on", 25, Bool, "Comp on", None, true),
        field("comp_gain", 26, Db, "Comp makeup gain", None, false),
        // Live survey 2026-10-20 (32 channels, read-only): -36.8..0.0.
        // The inherited +20 top is unobserved; the desk-facing bound is 0.
        field("comp_threshold", 27, Db, "Comp threshold", Some((-60.0, 0.0)), false),
        field("comp_attack", 28, Float, "Comp attack ms", None, false),
        field("comp_release", 29, Float, "Comp release ms", None, false),
        field("comp_ratio", 30, Float, "Comp ratio", None, false),
        field("comp_knee", 31, Enum, "Comp knee soft/hard", None, false),
        field("eq_mode", 32, Enum, "EQ modern/vintage", None, true),
        field("eq_on", 33, Bool, "EQ on", None, true),
        field("eq_bands", 34, Int, "EQ band count", None, true),
    ]
}

/// EQ block: bands 4/3/2 of four words at 35/39/43, then band 1 with only
/// three (gain, freq, Q) at 47-49 — the low band has no filter-type word.
/// Band 4 is the TOP band (seen at 17.25 kHz vs band 1 at 100 Hz).
fn input_eq() -> Vec<Field> {
    use Encoding::*;
    let mut out = Vec::with_capacity(15);
    for (band, base) in [(4, 35), (3, 39), (2, 43), (1, 47)] {
        // `format!` is not const: leak a static id string per band field.
        // Field keys are looked up by content, so owned keys are fine.
        let key = |suffix: &str| -> &'static str {
            Box::leak(format!("eq{band}_{suffix}").into_boxed_str())
        };
        let label = |suffix: &str| -> &'static str {
            Box::leak(format!("EQ band {band} {suffix}").into_boxed_str())
        };
        out.push(field(
            key("gain"),
            base,
            Db,
            label("gain"),
            Some((-20.0, 20.0)),
            true,
        ));
        out.push(field(
            key("freq"),
            base + 1,
            Float,
            label("frequency"),
            Some((20.0, 20000.0)),
            true,
        ));
        out.push(field(
            key("q"),
            base + 2,
            Float,
            label("Q"),
            Some((0.1, 20.0)),
            true,
        ));
        if band != 1 {
            out.push(field(key("type"), base + 3, Enum, label("filter type"), None, true));
        }
    }
    out
}

/// Send block: aux sends are (level, mute, unknown) triples, FX sends drop
/// the third word (reads 0 on all channels: mapped, flagged unverified).
fn sends(prefix: &str, label: &str, first: u32, count: u32, with_third: bool) -> Vec<Field> {
    let mut out = Vec::new();
    for n in 1..=count {
        let base = first + (n - 1) * if with_third { 3 } else { 2 };
        let key = |suffix: &str| -> &'static str {
            Box::leak(format!("{prefix}{n}_{suffix}").into_boxed_str())
        };
        let lab = |suffix: &str| -> &'static str {
            Box::leak(format!("{label} {n} {suffix}").into_boxed_str())
        };
        out.push(field(
            key("level"),
            base,
            Encoding::Db,
            lab("send level"),
            Some((-120.0, 10.0)),
            true,
        ));
        out.push(field(key("mute"), base + 1, Encoding::Bool, lab("send mute"), None, true));
        if with_third {
            out.push(field(key("pan"), base + 2, Encoding::Float, lab("send pan"), Some((-1.0, 1.0)), false));
        }
    }
    out
}

/// Six-word membership rows (mute group / view / subgroup / VCA).
fn membership(prefix: &str, label: &str, first: u32) -> Vec<Field> {
    (1..=6)
        .map(|n| {
            field(
                Box::leak(format!("{prefix}{n}").into_boxed_str()),
                first + n - 1,
                Encoding::Bool,
                Box::leak(format!("{label} {n} membership").into_boxed_str()),
                None,
                false,
            )
        })
        .collect()
}

/// One regular array of identical strips in the value space.
#[derive(Clone, Debug)]
pub struct ChannelBlock {
    pub name: &'static str,
    pub base: u32,
    pub stride: u32,
    pub count: u32,
    pub fields: Vec<Field>,
}

impl ChannelBlock {
    /// 1-based channel index to absolute address.
    pub fn address(&self, index_1_based: u32, key: &str) -> anyhow::Result<u32> {
        if !(1..=self.count).contains(&index_1_based) {
            anyhow::bail!("{} index {index_1_based} out of range 1..={}", self.name, self.count);
        }
        let f = self.field(key)?;
        Ok(self.base + (index_1_based - 1) * self.stride + f.offset)
    }

    pub fn field(&self, key: &str) -> anyhow::Result<&Field> {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(|| anyhow::anyhow!("unknown {} field {key:?}", self.name))
    }
}

/// The verified DL32S input block: 106-word strips, 8 aux sends.
/// Built once per process (field keys are interned statics).
pub fn dl32s_inputs() -> &'static ChannelBlock {
    static INPUTS: OnceLock<ChannelBlock> = OnceLock::new();
    INPUTS.get_or_init(build_dl32s_inputs)
}

fn build_dl32s_inputs() -> ChannelBlock {
    let aux_first = 50;
    let fx_first = aux_first + 8 * 3;
    let membership_first = fx_first + 4 * 2;
    let mut fields = input_head();
    fields.extend(input_eq());
    fields.extend(sends("aux", "Aux", aux_first, 8, true));
    fields.extend(sends("fx", "FX", fx_first, 4, false));
    fields.extend(membership("mutegroup", "Mute group", membership_first));
    fields.extend(membership("view", "View", membership_first + 6));
    fields.extend(membership("sub", "Subgroup", membership_first + 12));
    fields.extend(membership("vca", "VCA", membership_first + 18));
    ChannelBlock {
        name: "input",
        base: INPUT_BASE,
        stride: INPUT_STRIDE,
        count: INPUT_COUNT,
        fields,
    }
}


/// Subgroup master strips: 6 strips of 51 words from 4041. Mute + fader
/// are live-verified (net-zero write+relay observed 2026-10-18 — see
/// DL32S protocol notes); the rest of each 51-word strip is unmapped.
pub fn dl32s_subgroups() -> &'static ChannelBlock {
    static B: OnceLock<ChannelBlock> = OnceLock::new();
    B.get_or_init(|| ChannelBlock {
        name: "subgroup",
        base: 4041,
        stride: 51,
        count: 6,
        fields: vec![
            field("mute", 0, Encoding::Bool, "Subgroup master mute", None, true),
            field("fader", 1, Encoding::Db, "Subgroup master fader", Some((-120.0, 10.0)), true),
        ],
    })
}

/// Main-out master strip: 87 words from 4347.
pub fn dl32s_main() -> &'static ChannelBlock {
    static B: OnceLock<ChannelBlock> = OnceLock::new();
    B.get_or_init(|| ChannelBlock {
        name: "main",
        base: 4347,
        stride: 87,
        count: 1,
        fields: vec![
            field("fader", 0, Encoding::Db, "Main master fader", Some((-120.0, 10.0)), false),
            field("mute", 1, Encoding::Bool, "Main master mute", None, false),
            field("balance", 2, Encoding::Float, "Main balance", None, false),
        ],
    })
}

/// Aux master strips: 8 strips of 90 words from 4434.
pub fn dl32s_auxmasters() -> &'static ChannelBlock {
    static B: OnceLock<ChannelBlock> = OnceLock::new();
    B.get_or_init(|| ChannelBlock {
        name: "auxmaster",
        base: 4434,
        stride: 90,
        count: 8,
        fields: vec![
            field("mute", 3, Encoding::Bool, "Aux master mute", None, false),
            field("fader", 4, Encoding::Db, "Aux master fader", Some((-120.0, 10.0)), false),
        ],
    })
}


/// Declarative map of the Mackie DL value space (mirrors the Python
/// `addressmap.MixerMap`). Only the verified DL32S map is served.
#[derive(Clone, Debug)]
pub struct MixerMap {
    pub model: &'static str,
    pub inputs: ChannelBlock,
    pub blocks: BTreeMap<&'static str, ChannelBlock>,
    pub notes: &'static str,
}

/// Resolve a configured mixer model to its map. `auto` resolves to DL32S.
pub fn get_map(model: &str) -> anyhow::Result<&'static MixerMap> {
    match model.trim().to_lowercase().as_str() {
        "dl32s" | "auto" | "" => Ok(DL32S_MAP.get_or_init(build_dl32s_map)),
        _ => anyhow::bail!("no address map for mixer model {model:?}"),
    }
}

static DL32S_MAP: OnceLock<MixerMap> = OnceLock::new();

fn build_dl32s_map() -> MixerMap {
    let mut blocks = BTreeMap::new();
    blocks.insert("subgroup", dl32s_subgroups().clone());
    blocks.insert("main", dl32s_main().clone());
    blocks.insert("auxmaster", dl32s_auxmasters().clone());
    MixerMap {
        model: "dl32s",
        inputs: dl32s_inputs().clone(),
        blocks,
        notes: "verified DL32S map",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_geometry() {
        let block = dl32s_inputs();
        assert_eq!(block.fields.len(), INPUT_STRIDE as usize);
        // Every offset 0..106 covered exactly once — a shifted layout fails here.
        let mut offsets: Vec<u32> = block.fields.iter().map(|f| f.offset).collect();
        offsets.sort_unstable();
        assert_eq!(offsets, (0..INPUT_STRIDE).collect::<Vec<_>>());
    }

    #[test]
    fn anchor_addresses() {
        // The classic off-by-one detector: channel 2 mute must land on 114,
        // plus the fader and send-block starts both sources agree on.
        let block = dl32s_inputs();
        assert_eq!(block.address(2, "mute").unwrap(), 114);
        assert_eq!(block.address(1, "fader").unwrap(), 9);
        assert_eq!(block.address(1, "aux1_level").unwrap(), 51);
        assert_eq!(block.address(32, "fader").unwrap(), 1 + 31 * 106 + 8);
        assert!(block.address(0, "mute").is_err());
        assert!(block.address(33, "mute").is_err());
        assert!(block.address(1, "nope").is_err());
    }

    #[test]
    fn field_flags_mirror_provenance() {
        let block = dl32s_inputs();
        // Observed-live fields are verified; inferred ones are flagged.
        assert!(block.field("gain").unwrap().verified);
        assert!(block.field("phantom").unwrap().verified);
        assert!(block.field("hpf_freq").unwrap().verified);
        assert!(block.field("gate_threshold").unwrap().verified);
        assert!(!block.field("gate_attack").unwrap().verified);
        assert!(!block.field("comp_gain").unwrap().verified);
        assert!(block.field("eq4_gain").unwrap().verified);
        assert!(block.field("aux8_mute").unwrap().verified);
        assert!(!block.field("aux8_pan").unwrap().verified);
        assert!(!block.field("mutegroup1").unwrap().verified);
        assert!(!block.field("vca6").unwrap().verified);
        // Subgroup masters are live-verified (net-zero relay 2026-10-18);
        // main/aux-master writes never ran (audible path) — unconfirmed.
        assert!(dl32s_subgroups().field("mute").unwrap().verified);
        assert!(dl32s_subgroups().field("fader").unwrap().verified);
        assert!(!dl32s_main().field("mute").unwrap().verified);
        assert!(!dl32s_main().field("fader").unwrap().verified);
        assert!(!dl32s_auxmasters().field("mute").unwrap().verified);
        assert!(!dl32s_auxmasters().field("fader").unwrap().verified);
        // Band 1 has no filter-type word.
        assert!(block.field("eq1_type").is_err());
        assert_eq!(block.field("eq1_q").unwrap().offset, 49);
    }

    #[test]
    fn fader_scale_endpoints() {
        assert_eq!(DL32S_FADER_SCALE.forward(0.0), -120.0);
        assert_eq!(DL32S_FADER_SCALE.forward(1.0), 10.0);
        assert_eq!(DL32S_FADER_SCALE.inverse(-120.0), 0.0);
        assert_eq!(DL32S_FADER_SCALE.inverse(10.0), 1.0);
        for i in 0..=100 {
            let level = i as f32 / 100.0;
            let back = DL32S_FADER_SCALE.inverse(DL32S_FADER_SCALE.forward(level));
            assert!((back - level).abs() < 1e-5, "level {level}");
        }
    }

    #[test]
    fn model_gating() {
        assert!(model_status("dl32s").is_ok());
        assert!(model_status("auto").is_ok());
        assert!(model_status("dl16s").is_err());
        assert!(model_status("dl32r").is_err());
        assert!(model_status("x32").is_err());
    }
}
