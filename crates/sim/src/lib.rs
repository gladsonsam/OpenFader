//! Offline fake mixer for UI dev: full v2 topology (81 strips), fake meters.
//!
//! Strip index map (global, stable, indices 0..39 mean what they always
//! meant so existing UI/tests keep working; new families append after):
//!
//! | indices | family | kind  | label     | default names       | colour |
//! |---------|--------|-------|-----------|---------------------|--------|
//! | 0..32   | ch     | in    | Ch 1-32   | CH01.. (per preset) | green 2 |
//! | 32..40  | mix    | bus   | Mix 1-8   | BUS1.. / preset     | cyan 6 |
//! | 40..56  | ch2    | in    | Ch 33-48  | CH33..CH48 (spares) | green 2 |
//! | 56..60  | fxret  | ret   | FxRet 1-4 | FXR1..FXR4          | cyan 6 |
//! | 60..64  | fxsnd  | fxsnd | FxSnd 1-4 | FXS1..FXS4          | blue 4 |
//! | 64..72  | mtx    | mtx   | Mtx 1-8   | MTX1..MTX8          | mag 11 |
//! | 72..73  | main   | main  | Main LR   | MAIN                | mag 11 |
//! | 73..79  | dca    | dca   | VCA 1-6   | VCA1..VCA6          | red 0 |
//! | 79..81  | ret    | return| Returns   | Return 1..2        | green 2 |
//!
//! Total [`SIM_STRIPS`] = 81. DCA strips carry fader/mute/name/color only
//! (catalogue scopes); every other family answers its full scoped set from
//! the first poll (seeded state, no silent gaps).
pub mod strip_schema;

use openfader_core::{
    validate_fader, ChannelStrip, MeterFrame, MixerDriver, MixerSnapshot, ParamValue, StripFamily,
    StripId, StripKind,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Instant;

/// 32 input channels.
pub const SIM_CHANNELS: usize = 32;
/// 8 buses (FX / aux / LR-adjacent), ids 32..40.
pub const SIM_BUSES: usize = 8;
/// Extra inputs: Ch 33..48 (so the sim covers the 48-input design).
pub const SIM_INPUTS_EXT: usize = 16;
/// FX returns.
pub const SIM_RETS: usize = 4;
/// FX send masters.
pub const SIM_FXSNDS: usize = 4;
pub const SIM_MTX: usize = 8;
pub const SIM_MAINS: usize = 1;
pub const SIM_DCAS: usize = 6;
pub const SIM_RETURNS: usize = 2;
/// R2 subgroup master count (matches the DL32S desk family).
pub const SIM_SUBGROUPS: usize = 6;
/// R2 mute-group master count (matches the DL32S desk family).
pub const SIM_MUTE_GROUPS: usize = 6;
/// Total strips (0..81). Indices 0..40 keep their historic meaning.
pub const SIM_STRIPS: usize =
    SIM_CHANNELS + SIM_BUSES + SIM_INPUTS_EXT + SIM_RETS + SIM_FXSNDS + SIM_MTX + SIM_MAINS + SIM_DCAS + SIM_RETURNS;

pub const SIM_TOPOLOGY: [StripFamily; 9] = [
    StripFamily { key: "ch", kind: StripKind::In, label: "Ch 1-32", first: 0, count: SIM_CHANNELS, name_prefix: "Ch", color_default: 2 },
    StripFamily { key: "mix", kind: StripKind::Bus, label: "Mix 1-8", first: 32, count: SIM_BUSES, name_prefix: "Mix", color_default: 6 },
    StripFamily { key: "ch2", kind: StripKind::In, label: "Ch 33-48", first: 40, count: SIM_INPUTS_EXT, name_prefix: "Ch", color_default: 2 },
    StripFamily { key: "fxret", kind: StripKind::Ret, label: "FxRet 1-4", first: 56, count: SIM_RETS, name_prefix: "FxRet", color_default: 6 },
    StripFamily { key: "fxsnd", kind: StripKind::FxSnd, label: "FxSnd 1-4", first: 60, count: SIM_FXSNDS, name_prefix: "FxSnd", color_default: 4 },
    StripFamily { key: "mtx", kind: StripKind::Mtx, label: "Mtx 1-8", first: 64, count: SIM_MTX, name_prefix: "Mtx", color_default: 11 },
    StripFamily { key: "main", kind: StripKind::Main, label: "Main LR", first: 72, count: SIM_MAINS, name_prefix: "Main", color_default: 11 },
    StripFamily { key: "dca", kind: StripKind::Dca, label: "VCA 1-6", first: 73, count: SIM_DCAS, name_prefix: "VCA", color_default: 0 },
    StripFamily { key: "ret", kind: strip_schema::RETURN_KIND, label: "Returns", first: 79, count: SIM_RETURNS, name_prefix: "Return", color_default: 2 },
];

pub fn kind_of_strip(idx: usize) -> anyhow::Result<StripKind> {
    for f in SIM_TOPOLOGY {
        if f.contains(idx) {
            return Ok(f.kind);
        }
    }
    anyhow::bail!("sim: strip {idx} out of range 0..{SIM_STRIPS}")
}

/// Named scene preset for [`SimDriver`]: realistic channel/bus scribbles
/// for the two supported hardware desks, plus a `Studio` clone of the
/// owner's live XR18 desk (see XR18 baseline notes §2). Geometry stays
/// 32ch + 8 buses (the sim is a stand-in, not a channel-count model); only
/// names change (plus the reported model string in [`MixerDriver::query_info`]).
/// `Generic` is the historical default (`CH01..`, `BUS1..`) and is what
/// `SimDriver::new()` / `Default` produce, so existing snapshots stay valid.
/// `Studio` additionally mirrors the desk-as-found faders/mutes on strips
/// 1..16, buses 1..6 and the main LR fader; strips past the 16-channel desk
/// (17..32, buses 7..8) are parked spares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimPreset {
    /// Historical default: `CH01..CH32`, `BUS1..BUS8`.
    #[default]
    Generic,
    /// Behringer XR18 layout: 18 real inputs + `CH19..CH32` spare.
    Xr18,
    /// Mackie DL32S layout: 32 real inputs.
    Dl32s,
    /// Owner's XR18 desk as found (XR18 baseline notes §2): real
    /// scribbles/order (`BASS`, `VOX`, `GTR L/R`, `DRUM L/R`, `KEYS L/R`,
    /// `AC GTR`, `Abelton`, desk spelling kept), bus scribbles
    /// (`COMP L/R`, `DRUM`, `KEYS`, `VOX`) and baseline faders/mutes.
    Studio,
}

impl SimPreset {
    /// Parse a `--preset` flag value (`generic|xr18|dl32s|studio`,
    /// case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "generic" | "gen" | "default" | "sim" => Some(SimPreset::Generic),
            "xr18" | "xair" | "x-air" => Some(SimPreset::Xr18),
            "dl32s" | "dl32-s" | "dl" => Some(SimPreset::Dl32s),
            "studio" | "rig" | "home" => Some(SimPreset::Studio),
            _ => None,
        }
    }

    /// CLI-facing name (`--preset <name>`).
    pub fn name(self) -> &'static str {
        match self {
            SimPreset::Generic => "generic",
            SimPreset::Xr18 => "xr18",
            SimPreset::Dl32s => "dl32s",
            SimPreset::Studio => "studio",
        }
    }
}

/// XR18 input scribbles (18 physical inputs of a typical live band layout).
/// Strips 19..=32 keep their `CHxx` spare labels.
pub const XR18_CHANNELS: [&str; 18] = [
    "Kick", "Snare", "Hat", "Tom 1", "Tom 2", "Floor", "OH L", "OH R", "Bass", "Gtr L",
    "Gtr R", "Keys L", "Keys R", "Vox 1", "Vox 2", "Vox 3", "Play L", "Play R",
];

/// DL32S input scribbles (32 physical inputs, typical live band layout).
pub const DL32S_CHANNELS: [&str; 32] = [
    "Kick", "Snare", "Hat", "Tom 1", "Tom 2", "Tom 3", "Floor", "OH L", "OH R", "Bass",
    "Gtr 1", "Gtr 2", "Keys L", "Keys R", "Ac Gtr", "Perc", "Vox 1", "Vox 2", "Vox 3",
    "Vox 4", "Choir 1", "Choir 2", "Play L", "Play R", "Click", "Talkbk", "Spare 1",
    "Spare 2", "Spare 3", "Spare 4", "Spare 5", "Spare 6",
];

/// Sim bus scribbles, shared by the generic-desk presets: front mix, four
/// monitor sends, two FX returns, one spare. The LR mains live on the
/// separate main fader (`set_main_fader`), as on the real desks.
pub const PRESET_BUSES: [&str; 8] = [
    "MixBus", "Mon 1", "Mon 2", "Mon 3", "Mon 4", "Reverb", "Delay", "FX",
];

/// Owner's XR18 desk scribbles, in channel order (XR18 baseline notes
/// §2). `""` is an empty desk scribble, not a spare: channels 02/03/06/14
/// read back empty on the hardware. Desk spelling kept (`Abelton`).
/// Strips 17..=32 keep their `CHxx` spare labels (XR18 is a 16-channel desk).
pub const STUDIO_CHANNELS: [&str; 16] = [
    "BASS", "", "", "VOX", "VOX", "", "GTR L", "GTR R", "DRUM L", "DRUM R", "KEYS L",
    "KEYS R", "AC GTR", "", "Abelton", "Abelton",
];

/// Owner's desk-as-found channel faders, same order (XR18 baseline notes
/// §2; `/ch/NN/mix/fader`).
pub const STUDIO_FADERS: [f32; 16] = [
    0.197458, 0.0, 0.337243, 0.348974, 0.375367, 0.0, 0.434995, 0.434995, 0.0, 0.482893,
    0.438905, 0.438905, 0.339198, 0.0, 0.524927, 0.524927,
];

/// Owner's desk-as-found mute state, same order: `true` = muted.
/// Baseline column is `mix/on` (1 = ON/unmuted), so mute = `on == 0`.
pub const STUDIO_MUTES: [bool; 16] = [
    true, true, true, true, false, true, false, false, false, false, true, true, true,
    true, true, true,
];

/// Owner's bus scribbles (`/bus/1..6/config/name`; bus 3 is empty on the
/// desk). Buses 7..8 have no desk equivalent and stay parked spares.
pub const STUDIO_BUSES: [&str; 8] = [
    "COMP L", "COMP R", "", "DRUM", "KEYS", "VOX", "BUS7", "BUS8",
];

/// Owner's desk-as-found bus faders (`/bus/1..6/mix/fader` all ON/unmuted);
/// buses 7..8 are parked spares at 0.75.
pub const STUDIO_BUS_FADERS: [f32; 8] = [
    0.749756, 0.749756, 0.749756, 1.0, 1.0, 1.0, 0.75, 0.75,
];

/// Owner's desk-as-found main LR fader (`/lr/mix/fader`, ON/unmuted).
pub const STUDIO_MAIN: f32 = 0.749756;

fn default_strips_for(preset: SimPreset) -> Vec<ChannelStrip> {
    let mut v = Vec::with_capacity(SIM_STRIPS);
    for i in 0..SIM_CHANNELS {
        let name = match preset {
            SimPreset::Generic => format!("CH{:02}", i + 1),
            SimPreset::Xr18 => XR18_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
            SimPreset::Dl32s => DL32S_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
            SimPreset::Studio => STUDIO_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
        };
        let mut s = ChannelStrip::new(i as u16, name);
        // Engineering pan (catalogue `-100..=+100`, `0.0` = centre), not
        // the legacy `0.5`-centre position scale; seeded with a spread so
        // the first poll already shows L/R placement.
        s.pan = seed_pan(i, &s.name);
        match preset {
            SimPreset::Studio => {
                // Desk-as-found scene (§2): baseline fader/mute on strips
                // 1..16, parked spares past the 16-channel desk.
                if let (Some(f), Some(m)) = (STUDIO_FADERS.get(i), STUDIO_MUTES.get(i)) {
                    s.fader = *f;
                    s.mute = *m;
                } else {
                    s.fader = 0.0;
                    s.mute = true;
                }
            }
            _ => {
                s.fader = 0.7 + 0.1 * ((i % 5) as f32 / 4.0);
            }
        }
        v.push(s);
    }
    for b in 0..SIM_BUSES {
        let bus_name = match preset {
            SimPreset::Generic => format!("BUS{}", b + 1),
            SimPreset::Studio => STUDIO_BUSES
                .get(b)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("BUS{}", b + 1)),
            _ => PRESET_BUSES
                .get(b)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("BUS{}", b + 1)),
        };
        let mut s = ChannelStrip::new((SIM_CHANNELS + b) as u16, bus_name);
        s.pan = 0.0;
        s.fader = match preset {
            SimPreset::Studio => STUDIO_BUS_FADERS.get(b).copied().unwrap_or(0.75),
            _ => 0.75,
        };
        s.mute = false;
        v.push(s);
    }
    // Extended families (new in v2; parked spare scene for every preset so
    // indices 0..40 keep their historic meaning and new strips still answer
    // the first poll with plausible values).
    for i in 0..SIM_INPUTS_EXT {
        let mut s = ChannelStrip::new((SIM_CHANNELS + SIM_BUSES + i) as u16, format!("CH{:02}", 33 + i));
        s.pan = seed_pan(32 + i, &s.name);
        s.fader = 0.0;
        s.mute = true;
        v.push(s);
    }
    for i in 0..SIM_RETS {
        let mut s = ChannelStrip::new((56 + i) as u16, format!("FXR{}", i + 1));
        s.pan = 0.0;
        s.fader = 0.75;
        s.mute = false;
        v.push(s);
    }
    for i in 0..SIM_FXSNDS {
        let mut s = ChannelStrip::new((60 + i) as u16, format!("FXS{}", i + 1));
        s.pan = 0.0;
        s.fader = 0.75;
        s.mute = false;
        v.push(s);
    }
    for i in 0..SIM_MTX {
        let mut s = ChannelStrip::new((64 + i) as u16, format!("MTX{}", i + 1));
        s.pan = if i % 2 == 0 { -20.0 } else { 20.0 };
        s.fader = 0.75;
        s.mute = false;
        v.push(s);
    }
    {
        let mut s = ChannelStrip::new(72, "MAIN".to_string());
        s.pan = 0.0;
        s.fader = match preset {
            SimPreset::Studio => STUDIO_MAIN,
            _ => 0.8,
        };
        s.mute = false;
        v.push(s);
    }
    for i in 0..SIM_DCAS {
        let mut s = ChannelStrip::new((73 + i) as u16, format!("VCA{}", i + 1));
        s.pan = 0.0;
        s.fader = 0.75;
        s.mute = false;
        v.push(s);
    }
    for i in 0..SIM_RETURNS {
        v.push(ChannelStrip::new((79 + i) as u16, format!("Return {}", i + 1)));
    }
    debug_assert_eq!(v.len(), SIM_STRIPS);
    v
}

/// Channel role inferred from the scribble (uppercased match): drives the
/// seeded param-plane scene (gain by channel type, phantom/HPF/gate/comp
/// choices). Unknown/empty scribbles read as `"other"` (neutral defaults).
fn chan_role(name: &str) -> &'static str {
    let u = name.to_uppercase();
    if u.contains("BASS") {
        "bass"
    } else if u.contains("KICK") {
        "kick"
    } else if u.contains("SNARE") {
        "snare"
    } else if u.contains("TOM") {
        "tom"
    } else if u.contains("HAT") || u.contains("OH ") || u.starts_with("OH") {
        "oh"
    } else if u.contains("VOX") {
        "vox"
    } else if u.contains("GTR") {
        "gtr"
    } else if u.contains("DRUM") {
        "drum"
    } else if u.contains("KEYS") {
        "keys"
    } else if u.contains("PLAY") || u.contains("ABELTON") || u.contains("ABLETON") {
        "playback"
    } else {
        "other"
    }
}

/// Seeded engineering pan (`-100..=+100`, catalogue scale): kick/bass sit
/// centre, `* L`/`* R` pairs spread +-30, everything else fans out by
/// index (`-45..=+45`) so the first poll already shows a spread.
fn seed_pan(idx: usize, name: &str) -> f32 {
    match chan_role(name) {
        "bass" | "kick" => 0.0,
        _ if name.ends_with(" L") => -30.0,
        _ if name.ends_with(" R") => 30.0,
        _ => ((idx % 7) as f32 - 3.0) * 15.0,
    }
}

/// Seeded param-plane state for one strip: every non-strip-backed catalogue
/// id (`gain` … `fx4_mute`) gets a plausible live-desk value, so
/// `get_param` answers from the first poll, no "unset optionals skipped".
/// Strip-backed ids (`fader`/`mute`/`name`/`pan`) are NOT included here;
/// they live in [`ChannelStrip`]. Writes via `set_param` overwrite these
/// seeds and read back bit-exact.
fn seed_params_for(idx: usize, name: &str) -> HashMap<String, ParamValue> {
    use openfader_core::params::CATALOGUE;
    let mut m: HashMap<String, ParamValue> = HashMap::new();
    // Catalogue defaults first, then a plausible scene on top.
    for def in CATALOGUE {
        match def.id {
            "fader" | "mute" | "name" | "pan" => {}
            id => {
                m.insert(id.to_string(), def.default.as_param_value());
            }
        }
    }
    let role = chan_role(name);
    let f = ParamValue::Float;
    let b = ParamValue::Bool;
    // Gain by channel type (catalogue range -12..=+60 dB).
    let gain = match role {
        "bass" => 30.0,
        "kick" => 27.0,
        "snare" => 24.0,
        "vox" => 25.0,
        "gtr" => 20.0,
        "keys" => 18.0,
        "drum" | "tom" => 22.0,
        "oh" => 21.0,
        "playback" => 15.0,
        _ => 20.0 + (idx % 5) as f32,
    };
    m.insert("gain".into(), f(gain));
    // Condenser-ish sources get phantom; small trim variation everywhere.
    m.insert(
        "phantom".into(),
        b(matches!(role, "vox" | "oh")),
    );
    m.insert("trim".into(), f((idx % 5) as f32 - 2.0));
    // HPF on for everything except bass/kick (the review's example).
    let hpf = !matches!(role, "bass" | "kick");
    m.insert("hpf_on".into(), b(hpf));
    m.insert("hpf_freq".into(), f(80.0 + (idx % 5) as f32 * 10.0));
    // Gate on drums, comp on bass/vox/gtr, EQ on for most strips.
    m.insert(
        "gate_on".into(),
        b(matches!(role, "kick" | "snare" | "tom" | "drum")),
    );
    m.insert(
        "comp_on".into(),
        b(matches!(role, "bass" | "vox" | "gtr")),
    );
    m.insert("eq_on".into(), b(idx % 4 != 3));
    // EQ bands with plausible freq/gain/q/type (types 0..=5 whole numbers).
    let low_boost = if matches!(role, "bass" | "kick") {
        3.0
    } else {
        (idx % 3) as f32 - 1.0
    };
    m.insert("eq1_gain".into(), f(low_boost));
    m.insert("eq1_freq".into(), f(100.0));
    m.insert("eq1_q".into(), f(1.0));
    m.insert("eq1_type".into(), f(1.0));
    m.insert(
        "eq2_gain".into(),
        f(if role == "vox" { 2.0 } else { 0.0 }),
    );
    m.insert("eq2_freq".into(), f(800.0 + (idx % 4) as f32 * 100.0));
    m.insert("eq2_q".into(), f(1.2));
    m.insert("eq2_type".into(), f(2.0));
    m.insert(
        "eq3_gain".into(),
        f(if matches!(role, "gtr" | "keys") {
            2.0
        } else {
            0.0
        }),
    );
    m.insert("eq3_freq".into(), f(3000.0));
    m.insert("eq3_q".into(), f(1.4));
    m.insert("eq3_type".into(), f(2.0));
    m.insert(
        "eq4_gain".into(),
        f(if role == "vox" { 1.0 } else { 0.0 }),
    );
    m.insert("eq4_freq".into(), f(12000.0));
    m.insert("eq4_q".into(), f(0.7));
    m.insert("eq4_type".into(), f(4.0));
    // A few sends non-zero (vox to monitors/FX, band to monitors);
    // everything else parks at -inf (-90 dB). Mutes stay off. The generic
    // `sendN_*` ids mirror the legacy aux/fx slots (send1..6 = aux1..6 bus
    // sends, send7..10 = fx1..4).
    if role == "vox" {
        m.insert("aux1_level".into(), f(-10.0));
        m.insert("fx1_level".into(), f(-12.0));
        m.insert("fx2_level".into(), f(-18.0));
        m.insert("send1_level".into(), f(-10.0));
        m.insert("send1_tap".into(), f(1.0));
        m.insert("send7_level".into(), f(-12.0));
        m.insert("send7_tap".into(), f(0.0));
        m.insert("send8_level".into(), f(-18.0));
    } else if matches!(role, "gtr" | "keys") {
        m.insert("aux2_level".into(), f(-14.0));
        m.insert("send2_level".into(), f(-14.0));
    } else if matches!(role, "drum" | "kick" | "snare" | "tom") {
        m.insert("fx1_level".into(), f(-14.0));
        m.insert("send7_level".into(), f(-14.0));
    } else if role == "playback" {
        m.insert("aux1_level".into(), f(-16.0));
        m.insert("send1_level".into(), f(-16.0));
    }
    // ---- v2 strip scene (colour/icon/link/source/delay/gate+comp full/eq5) ----
    // Design bank colour: inputs green (2); vox channels amber (3) so the
    // seeded scene shows variation.
    m.insert("color".into(), f(if role == "vox" { 3.0 } else { 2.0 }));
    m.insert("icon".into(), f(0.0));
    m.insert("link".into(), b(name.ends_with(" L") || name.ends_with(" R")));
    m.insert("source".into(), ParamValue::Text(format!("Lcl {:02}", (idx % 32) + 1)));
    m.insert("delay_on".into(), b(false));
    m.insert("delay_ms".into(), f(0.0));
    m.insert("hpf_slope".into(), f(0.0));
    // Gate full set: plausible per-role scene (drums gated, others open).
    let gated = matches!(role, "kick" | "snare" | "tom" | "drum");
    m.insert("gate_thr".into(), f(if gated { -30.0 } else { -50.0 }));
    m.insert("gate_range".into(), f(60.0));
    m.insert("gate_att".into(), f(1.0));
    m.insert("gate_hold".into(), f(50.0));
    m.insert("gate_rel".into(), f(100.0));
    m.insert("gate_mode".into(), f(1.0));
    m.insert("gate_key_src".into(), f(0.0));
    m.insert("gate_key_filter".into(), f(0.0));
    // Comp full set: bass/vox/gtr compressed (matches comp_on above).
    let compressed = matches!(role, "bass" | "vox" | "gtr");
    m.insert("comp_thr".into(), f(if compressed { -20.0 } else { -10.0 }));
    m.insert("comp_ratio".into(), f(if compressed { 4.0 } else { 2.0 }));
    m.insert("comp_knee".into(), f(3.0));
    m.insert("comp_makeup".into(), f(if compressed { 3.0 } else { 0.0 }));
    m.insert("comp_att".into(), f(10.0));
    m.insert("comp_hold".into(), f(20.0));
    m.insert("comp_rel".into(), f(100.0));
    m.insert("comp_mode".into(), f(0.0));
    m.insert("comp_det".into(), f(0.0));
    m.insert("comp_env".into(), f(0.0));
    m.insert("comp_mix".into(), f(100.0));
    m.insert("comp_auto".into(), b(false));
    // EQ5 (where the desk has it): gentle air on vox, flat elsewhere.
    m.insert("eq5_gain".into(), f(if role == "vox" { 1.5 } else { 0.0 }));
    m.insert("eq5_freq".into(), f(16000.0));
    m.insert("eq5_q".into(), f(0.7));
    m.insert("eq5_type".into(), f(2.0));
    // Remaining send slots: post tap, centre pan, unmuted, parked at -inf.
    for n in 1..=16u32 {
        m.entry(format!("send{n}_tap")).or_insert(f(1.0));
        m.entry(format!("send{n}_pan")).or_insert(f(0.0));
        m.entry(format!("send{n}_on")).or_insert(b(true));
        m.entry(format!("send{n}_level")).or_insert(f(-90.0));
    }
    // Legacy aux/fx mutes + taps stay catalogue defaults (off/unmuted).
    m
}

/// Seeded param-plane state for a non-input strip (bus/ret/fxsnd/mtx/main):
/// catalogue defaults plus a plausible mix scene (EQ on for most, comp on
/// for alternate buses/matrices + main, sends untouched, out of scope).
/// DCA strips only ever answer fader/mute/name/color (scopes); the extra
/// seeds are harmless (hidden by the scope check in `get_param`).
fn seed_params_for_family(idx: usize, name: &str, kind: StripKind) -> HashMap<String, ParamValue> {
    use openfader_core::params::CATALOGUE;
    let mut m: HashMap<String, ParamValue> = HashMap::new();
    for def in CATALOGUE {
        match def.id {
            "fader" | "mute" | "name" | "pan" => {}
            id => {
                m.insert(id.to_string(), def.default.as_param_value());
            }
        }
    }
    let f = ParamValue::Float;
    let b = ParamValue::Bool;
    let color = match kind {
        StripKind::In => 2.0,
        StripKind::Ret => 6.0,
        StripKind::Bus => 6.0,
        StripKind::FxSnd => 4.0,
        StripKind::Mtx => 11.0,
        StripKind::Main => 11.0,
        StripKind::Dca => 0.0,
        // Provider-extended kinds (no sim family uses one): default strip
        // colour until the provider seeds its own look.
        _ => 2.0,
    };
    m.insert("color".into(), f(color));
    m.insert("icon".into(), f(0.0));
    // EQ on for most mix strips (every 4th off, mirroring inputs); comp on
    // for alternate mix/matrix strips + main.
    let pos_in_family = (idx % 8) as u8;
    m.insert("eq_on".into(), b(idx % 4 != 3));
    let compressed = matches!(kind, StripKind::Bus | StripKind::Mtx | StripKind::Main)
        && pos_in_family.is_multiple_of(2);
    m.insert("comp_on".into(), b(compressed));
    m.insert("comp_thr".into(), f(if compressed { -20.0 } else { -10.0 }));
    m.insert("comp_ratio".into(), f(if compressed { 4.0 } else { 2.0 }));
    m.insert("comp_makeup".into(), f(if compressed { 3.0 } else { 0.0 }));
    let _ = name;
    m
}

/// One R2 FX slot on the sim desk: design type + on/bypass + normalized
/// params (values 0.0..=1.0, bit-exact like the param plane).
#[derive(Clone, Debug)]
pub struct SimFxSlot {
    pub fx_type: String,
    /// Slot on (design `on`; false reads as bypassed on the FX screen).
    pub on: bool,
    /// Explicit bypass flag (design has no separate bypass; kept so
    /// `fx_bypass` round-trips).
    pub bypass: bool,
    pub params: Vec<f32>,
}

/// Sim FX slot count: the design's 8-slot rack (XR18 hardware has 4).
pub const SIM_FX_SLOTS: usize = 8;

/// A design device for the R2 `devices` event (mirrors `devices()` in
/// design logic reference).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimDevice {
    pub id: &'static str,
    pub name: &'static str,
    pub model: &'static str,
    pub ip: &'static str,
    pub kind: &'static str,
    pub state: &'static str,
}

/// The design's three devices: FOH Console (.20, connected), Monitor
/// Console (.21, available), Stage Box A (.40, I/O in use).
pub const SIM_DEVICES: &[SimDevice] = &[
    SimDevice { id: "foh", name: "FOH Console", model: "SimConsole 48", ip: "192.168.1.120", kind: "console", state: "connected" },
    SimDevice { id: "mon", name: "Monitor Console", model: "SimConsole 48", ip: "192.168.1.21", kind: "console", state: "available" },
    SimDevice { id: "sb1", name: "Stage Box A", model: "SimIO 32", ip: "192.168.1.40", kind: "io", state: "in_use" },
];

/// R2 routing patch groups: Local / Network / USB, 32 sockets each (the
/// design's `srcTiles` count per group).
pub const SIM_PATCH_GROUPS: &[(&str, &str, usize)] = &[
    ("Lcl", "Local", 32),
    ("Net", "Network", 32),
    ("USB", "USB", 32),
];

/// Per-strip meter taps (R2): `in` (pre-fader input level), `post`
/// (post-fader), `gr_gate` / `gr_comp` (dB reduction `>= 0`).
#[derive(Clone, Debug, PartialEq)]
pub struct StripTaps {
    pub input: f32,
    pub post: f32,
    pub gr_gate_db: f32,
    pub gr_comp_db: f32,
}

/// First global strip whose topology kind supports catalogue `id`
/// (test helper: scopes decide where an id round-trips).
#[cfg(test)]
fn first_supporting_strip(id: &str) -> Option<StripId> {
    let def = openfader_core::params::find(id)?;
    for fam in SIM_TOPOLOGY {
        if strip_schema::supports(def, fam.kind) {
            return Some(StripId(fam.first as u16));
        }
    }
    None
}

fn default_fx_for(preset: SimPreset) -> Vec<SimFxSlot> {
    let _ = preset;
    openfader_core::fx::DESIGN_FX_DEFAULT_TYPES
        .iter()
        .enumerate()
        .map(|(i, t)| SimFxSlot {
            fx_type: t.to_string(),
            on: i != 7,
            bypass: false,
            params: openfader_core::fx::fx_default_params(t),
        })
        .collect()
}
mod input_processors;
pub mod output_routing;
mod global_controls;
mod solo;

pub struct SimDriver {
    output_routes: RwLock<HashMap<String, (String, u64)>>,
    globals: RwLock<Vec<openfader_core::destinations::DestinationControl>>,
    processor_revision: AtomicU64,
    processor_generation: AtomicU64,
    strips: RwLock<Vec<ChannelStrip>>,
    /// Per-strip detail state (`gain`, `phantom`, `eq1_gain`, …): catalogue
    /// params, stored bit-exact (no wire quantization in sim). Strip-backed params (`fader`, `mute`, `name`, `pan`) live in
    /// [`ChannelStrip`] and are intentionally NOT mirrored here; `get_param`
    /// reads those from the strip so the two views cannot disagree.
    params: RwLock<Vec<HashMap<String, ParamValue>>>,
    /// Per-strip solo (monitor bus), in-memory.
    solos: RwLock<HashMap<u16, bool>>,
    signal_started: Instant,
    tick_offset: AtomicU64,
    connected: RwLock<bool>,
    preset: SimPreset,
    /// R2 FX rack: [`SIM_FX_SLOTS`] design-type slots (type/on/bypass +
    /// normalized params). Seeded from [`openfader_core::fx`] design defaults
    /// (`buildFx` order); writes overwrite and read back bit-exact.
    fx: RwLock<Vec<SimFxSlot>>,
    /// R2 subgroup masters: 6 × (fader 0..1, mute), in-memory (no desk
    /// leaf on the sim model, the reference surface for the DL32S desk).
    subgroups: RwLock<[(f32, bool); SIM_SUBGROUPS]>,
    /// R2 mute-group masters: 6 × muted, in-memory.
    mute_groups: RwLock<[bool; SIM_MUTE_GROUPS]>,
    /// R2 group membership: (strip, 0-based group) -> member. Subgroup and
    /// mute-group assigns share the shape; absent entries read false.
    sub_assign: RwLock<HashMap<(u16, u8), bool>>,
    mg_assign: RwLock<HashMap<(u16, u8), bool>>,
    /// R2 desk files: filename -> (bytes, file type), in-memory stand-in
    /// for the DL32S up/download path.
    files: RwLock<HashMap<String, (Vec<u8>, u32)>>,
}

impl SimDriver {
    pub fn new() -> Self {
        Self::with_preset(SimPreset::Generic)
    }

    /// Named desk preset (`--preset xr18|dl32s|studio` in `openfader-cli`).
    /// Geometry is unchanged (32ch + 8 buses); only scribbles differ
    /// (studio additionally mirrors desk-as-found faders/mutes + main).
    pub fn with_preset(preset: SimPreset) -> Self {
        let strips = default_strips_for(preset);
        // Seed the param-plane map from the strip scribbles so every
        // catalogue id reads back a plausible live-desk value from the
        // first poll (no "unset optionals skipped"); writes overwrite.
        // Input strips 0..32 get the role scene; every other strip gets
        // the family scene (buses included).
        let params: Vec<HashMap<String, ParamValue>> = strips
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let kind = kind_of_strip(i).expect("topology covers every strip");
                if i < SIM_CHANNELS {
                    seed_params_for(i, &s.name)
                } else {
                    seed_params_for_family(i, &s.name, kind)
                }
            })
            .collect();
        Self {
            output_routes: RwLock::new(HashMap::new()),
            globals: RwLock::new(openfader_core::global_controls::reference_controls().into_iter().chain(solo::controls()).collect()),
            processor_revision: AtomicU64::new(0),
            processor_generation: AtomicU64::new(0),
            strips: RwLock::new(strips),
            params: RwLock::new(params),
            solos: RwLock::new(HashMap::new()),
            signal_started: Instant::now(),
            tick_offset: AtomicU64::new(0),
            connected: RwLock::new(false),
            preset,
            fx: RwLock::new(default_fx_for(preset)),
            subgroups: RwLock::new([(0.0, false); SIM_SUBGROUPS]),
            mute_groups: RwLock::new([false; SIM_MUTE_GROUPS]),
            sub_assign: RwLock::new(HashMap::new()),
            mg_assign: RwLock::new(HashMap::new()),
            files: RwLock::new(HashMap::new()),
        }
    }

    pub fn preset(&self) -> SimPreset {
        self.preset
    }

    /// Engineering pan (`-100..=+100`, [`openfader_core::params`] catalogue)
    /// for one strip. Strip storage uses the catalogue scale directly
    /// (default `0.0` = centre, set in [`default_strips_for`]) so `params`
    /// frames never carry the legacy `0.5`-centre position scale.
    fn check_pan(v: f32) -> anyhow::Result<f32> {
        if !v.is_finite() {
            anyhow::bail!("sim: pan must be finite, got {v}");
        }
        if !(-100.0..=100.0).contains(&v) {
            anyhow::bail!("sim: pan {v} out of range -100..=100");
        }
        Ok(v)
    }

    fn read(&self) -> RwLockReadGuard<'_, Vec<ChannelStrip>> {
        self.strips.read().expect("sim strips poisoned")
    }

    fn write(&self) -> RwLockWriteGuard<'_, Vec<ChannelStrip>> {
        self.strips.write().expect("sim strips poisoned")
    }

    fn idx(&self, strip: StripId) -> anyhow::Result<usize> {
        let i = strip.0 as usize;
        if i < self.read().len() {
            Ok(i)
        } else {
            anyhow::bail!("sim: strip {} out of range 0..{}", i, SIM_STRIPS)
        }
    }

    /// Bulk subgroup-assign snapshot for the engine's attach fast path:
    /// one lock acquisition instead of ~strips × groups awaits. Same
    /// content `get_subgroup_assign` serves (absent entries read false 
    /// callers filter or default identically).
    pub fn bulk_sub_assign(&self) -> HashMap<(u16, u8), bool> {
        self.sub_assign.read().expect("sim sub_assign poisoned").clone()
    }

    /// Bulk mute-group-assign snapshot, mirroring
    /// [`bulk_sub_assign`](SimDriver::bulk_sub_assign).
    pub fn bulk_mg_assign(&self) -> HashMap<(u16, u8), bool> {
        self.mg_assign.read().expect("sim mg_assign poisoned").clone()
    }

    /// Bulk solo snapshot: strip -> soloed (only strips ever soloed
    /// appear; callers default the rest to false, as `get_solo` does).
    pub fn bulk_solos(&self) -> HashMap<u16, bool> {
        self.solos.read().expect("sim solos poisoned").clone()
    }

    /// Bulk `source`-param snapshot: strip -> raw text. Mirrors what
    /// `get_param(strip, "source")` serves (the seeded scene over
    /// catalogue defaults); the engine still applies the `parse_source`
    /// filter, so unpatched strips stay silent exactly as before.
    pub fn bulk_sources(&self) -> HashMap<u16, String> {
        let params = self.params.read().expect("sim params poisoned");
        let mut out = HashMap::new();
        for (i, strip) in params.iter().enumerate() {
            if let Some(ParamValue::Text(label)) = strip.get("source") {
                if let Ok(strip) = u16::try_from(i) {
                    out.insert(strip, label.clone());
                }
            }
        }
        out
    }

    /// Advance the fake-meter phase (1 tick == one 50ms frame).
    pub fn advance(&self, ticks: u64) {
        self.tick_offset.fetch_add(ticks, Ordering::Relaxed);
    }

    /// Reads share a monotonic 20 Hz phase. Meter, tap and per-channel GR
    /// consumers must not advance the signal each time they sample it.
    fn signal_tick(&self) -> u64 {
        (self.signal_started.elapsed().as_millis() as u64 / 50)
            .wrapping_add(self.tick_offset.load(Ordering::Relaxed))
    }

    fn input_db(i: usize, tick: u64) -> f32 {
        // Slow, staggered phrases instead of full-scale rhythmic flashing.
        let t = tick as f32 * 0.025;
        let ph = i as f32 * 0.7;
        let rate = 0.65 + (i % 5) as f32 * 0.07;
        let phrase = (0.5 + 0.35 * (t * rate + ph).sin()
            + 0.15 * (t * 1.7 + ph * 1.3).sin()).clamp(0.0, 1.0);
        -36.0 + 24.0 * phrase
    }

    fn meter_level(&self, i: usize, tick: u64) -> f32 {
        if kind_of_strip(i).is_ok_and(|kind| kind == StripKind::Dca) { return 0.0; }
        let strips = self.read();
        let Some(strip) = strips.get(i) else { return 0.0; };
        if strip.mute { return 0.0; }
        10.0_f32.powf(Self::input_db(i, tick) / 20.0) * strip.fader
    }

    /// Demo-only frequency display: fixed logarithmic-band peaks with a slow
    /// level envelope. Never FFT the handful of control-rate meter samples or
    /// normalize each frame, which makes tiny timing differences look full scale.
    pub fn demo_spectrum(&self, ch: usize) -> Vec<f32> {
        let level = self.meter_level(ch, self.signal_tick());
        if level <= 0.0 { return vec![0.0; 64]; }
        let envelope = ((20.0 * level.log10() + 90.0) / 70.0).clamp(0.0, 1.0);
        (0..64).map(|bin| {
            let x = bin as f32;
            let peak = |center: f32, width: f32| (-0.5 * ((x - center) / width).powi(2)).exp();
            let profile = 0.012
                + 0.92 * peak(12.0 + (ch % 3) as f32, 3.0)
                + 0.55 * peak(23.0 + (ch % 4) as f32, 3.8)
                + 0.20 * peak(34.0, 5.0);
            let texture = 0.85 + 0.15 * (x * 1.9 + ch as f32 * 0.7).cos();
            (profile * texture * envelope).clamp(0.0, 1.0)
        }).collect()
    }

    // ---- R2: FX rack (design types, [`SIM_FX_SLOTS`] slots) ----

    /// Design FX slot count (8; the XR18 desk has 4).
    pub fn fx_slot_count(&self) -> usize {
        SIM_FX_SLOTS
    }

    fn fx_idx(&self, slot: u8) -> anyhow::Result<usize> {
        if slot == 0 || (slot as usize) > SIM_FX_SLOTS {
            anyhow::bail!("sim: fx slot {slot} out of range 1..={SIM_FX_SLOTS}");
        }
        Ok((slot - 1) as usize)
    }

    pub fn fx_slots(&self) -> Vec<SimFxSlot> {
        self.fx.read().expect("sim fx poisoned").clone()
    }

    /// Set a slot's design type (case-insensitive id/label); params reset
    /// to the type's design defaults (mirrors the UI picking a new type).
    pub fn set_fx_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        let i = self.fx_idx(slot)?;
        let meta = openfader_core::fx::fx_type_meta(fx_type)
            .ok_or_else(|| anyhow::anyhow!("sim: unknown fx type {fx_type:?}"))?;
        let mut fx = self.fx.write().expect("sim fx poisoned");
        fx[i].fx_type = meta.label.to_string();
        fx[i].params = meta.params.iter().map(|p| p.default).collect();
        Ok(())
    }

    pub fn get_fx_type(&self, slot: u8) -> anyhow::Result<String> {
        let i = self.fx_idx(slot)?;
        Ok(self.fx.read().expect("sim fx poisoned")[i].fx_type.clone())
    }

    /// Set one normalized FX param by id (must be in the slot's type table).
    pub fn set_fx_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            anyhow::bail!("sim: fx param {value} out of range 0.0..=1.0");
        }
        let i = self.fx_idx(slot)?;
        let mut fx = self.fx.write().expect("sim fx poisoned");
        let meta = openfader_core::fx::fx_type_meta(&fx[i].fx_type.clone())
            .ok_or_else(|| anyhow::anyhow!("sim: fx slot {slot} has unknown type"))?;
        let pos = meta
            .params
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("sim: unknown fx param {id:?} for {}", meta.label))?;
        fx[i].params[pos] = value;
        Ok(())
    }

    pub fn get_fx_param(&self, slot: u8, id: &str) -> anyhow::Result<f32> {
        let i = self.fx_idx(slot)?;
        let fx = self.fx.read().expect("sim fx poisoned");
        let meta = openfader_core::fx::fx_type_meta(&fx[i].fx_type)
            .ok_or_else(|| anyhow::anyhow!("sim: fx slot {slot} has unknown type"))?;
        let pos = meta
            .params
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("sim: unknown fx param {id:?} for {}", meta.label))?;
        Ok(fx[i].params[pos])
    }

    pub fn set_fx_on(&self, slot: u8, on: bool) -> anyhow::Result<()> {
        let i = self.fx_idx(slot)?;
        self.fx.write().expect("sim fx poisoned")[i].on = on;
        Ok(())
    }

    pub fn set_fx_bypass(&self, slot: u8, bypass: bool) -> anyhow::Result<()> {
        let i = self.fx_idx(slot)?;
        self.fx.write().expect("sim fx poisoned")[i].bypass = bypass;
        Ok(())
    }

    // ---- R2: routing patch (source sockets Lcl/Net/USB × 32) ----

    fn check_patch_group(group: &str) -> anyhow::Result<(&'static str, usize)> {
        SIM_PATCH_GROUPS
            .iter()
            .find(|(key, _, _)| *key == group)
            .map(|(key, _, count)| (*key, *count))
            .ok_or_else(|| anyhow::anyhow!("sim: unknown patch group {group:?} (want Lcl|Net|USB)"))
    }

    /// Patch an input strip to a source socket (`group` Lcl/Net/USB,
    /// `index` 1-based). Writes the `source` param (`"Lcl 01"` style, the
    /// design's tile labels); only input strips are patchable.
    pub fn set_patch(&self, strip: StripId, group: &str, index: u8) -> anyhow::Result<String> {
        let i = self.idx(strip)?;
        let kind = kind_of_strip(i)?;
        if kind != StripKind::In && kind != strip_schema::RETURN_KIND {
            anyhow::bail!("sim: patch only supported on input strips");
        }
        let (key, count) = Self::check_patch_group(group)?;
        if index == 0 || (index as usize) > count {
            anyhow::bail!("sim: patch index {index} out of range 1..={count}");
        }
        let label = format!("{key} {index:02}");
        self.params.write().expect("sim params poisoned")[i]
            .insert("source".to_string(), ParamValue::Text(label.clone()));
        Ok(label)
    }

    /// Clear a strip's patch (source reads back `" "`, the design's
    /// unpatched tile).
    pub fn clear_patch(&self, strip: StripId) -> anyhow::Result<()> {
        let i = self.idx(strip)?;
        let kind = kind_of_strip(i)?;
        if kind != StripKind::In && kind != strip_schema::RETURN_KIND {
            anyhow::bail!("sim: patch only supported on input strips");
        }
        self.params.write().expect("sim params poisoned")[i]
            .insert("source".to_string(), ParamValue::Text("—".to_string()));
        Ok(())
    }

    // ---- R2: meter taps ----

    /// Per-strip meter taps: `in` (pre-fader, ignores mute/fader), `post`
    /// (post-fader, silent when muted), `gr_gate`/`gr_comp` (dB reduction).
    /// Deterministic pseudo-live phase, same family as [`SimDriver::poll_meters`].
    pub fn poll_taps(&self) -> Vec<StripTaps> {
        let tick = self.signal_tick();
        let strips = self.read();
        let params = self.params.read().unwrap();
        (0..strips.len())
            .map(|i| {
                if kind_of_strip(i).is_ok_and(|kind| kind == StripKind::Dca) {
                    return StripTaps { input: 0.0, post: 0.0, gr_gate_db: 0.0, gr_comp_db: 0.0 };
                }
                let input_db = Self::input_db(i, tick);
                let input = 10.0_f32.powf(input_db / 20.0);
                let s = &strips[i];
                let post = if s.mute { 0.0 } else { input * s.fader };
                // Dynamics act before mute/fader. The fake input swings
                // through thresholds so both reduction indicators animate.
                let param = |name: &str, fallback: f32| match params[i].get(name) {
                    Some(ParamValue::Float(v)) => *v,
                    _ => fallback,
                };
                let enabled = |name: &str| matches!(params[i].get(name), Some(ParamValue::Bool(true)));
                let gr_gate_db = if enabled("gate_on") {
                    ((param("gate_thr", -40.0) - input_db) / 12.0).clamp(0.0, 1.0)
                        * param("gate_range", 30.0)
                } else { 0.0 };
                let gr_comp_db = if enabled("comp_on") {
                    (input_db - param("comp_thr", -20.0)).max(0.0)
                        * (1.0 - 1.0 / param("comp_ratio", 4.0).max(1.0))
                } else { 0.0 };
                StripTaps { input, post, gr_gate_db, gr_comp_db }
            })
            .collect()
    }
}

impl Default for SimDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl SimDriver {
    fn sim_check_filename(filename: &str) -> anyhow::Result<()> {
        if filename.is_empty() || filename.len() > 128 {
            anyhow::bail!("sim: filename must be 1..=128 chars");
        }
        if filename.contains(['/', '\\', '\0']) {
            anyhow::bail!("sim: filename {filename:?} must not contain path separators");
        }
        Ok(())
    }

    fn sim_check_subgroup(group: u8) -> anyhow::Result<usize> {
        if (group as usize) >= SIM_SUBGROUPS {
            anyhow::bail!("sim: subgroup {group} out of range 0..{SIM_SUBGROUPS}");
        }
        Ok(group as usize)
    }

    fn sim_check_sub_assign(&self, ch: StripId, group: u8) -> anyhow::Result<(u16, u8)> {
        let _ = self.idx(ch)?;
        Ok((ch.0, Self::sim_check_subgroup(group)? as u8))
    }

    fn sim_check_mute_group(group: u8) -> anyhow::Result<usize> {
        if (group as usize) >= SIM_MUTE_GROUPS {
            anyhow::bail!("sim: mute group {group} out of range 0..{SIM_MUTE_GROUPS}");
        }
        Ok(group as usize)
    }

    fn sim_check_mg_assign(&self, ch: StripId, group: u8) -> anyhow::Result<(u16, u8)> {
        let _ = self.idx(ch)?;
        Ok((ch.0, Self::sim_check_mute_group(group)? as u8))
    }
}

impl MixerDriver for SimDriver {
    fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> { self.global_controls() }
    async fn set_destination_control(&self, target: &str, control: &str, value: f32, expected: f32) -> anyhow::Result<()> {
        self.write_global(target, control, value, expected, None).map(|_| ())
    }
    async fn set_target_solo(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        self.write_sim_solo(target, control, value, expected, snapshot, context)
    }
    async fn set_destination_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl]) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
        self.write_global(target, control, value, expected, Some(snapshot)).map(Some)
    }

    fn stereo_pairs(&self) -> Vec<[u16; 2]> {
        self.topology().into_iter().filter(|f| matches!(f.kind, StripKind::In) || f.kind == strip_schema::RETURN_KIND)
            .flat_map(|f| (f.first as u16..(f.first + f.count) as u16).step_by(2)
                .filter(move |left| *left + 1 < (f.first + f.count) as u16)
                .map(|left| [left, left + 1])).collect()
    }

    async fn control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
        if matches!(param, "link" | "source" | "source_a" | "source_b" | "source_select" | "trim" | "gain" | "phantom" | "usbreturn") {
            return Ok(vec![strip]);
        }
        if let Some(pair) = self.stereo_pairs().into_iter().find(|pair| pair.contains(&strip.0)) {
            if self.get_param(StripId(pair[0]), "link").await? == ParamValue::Bool(true) {
                return Ok(pair.into_iter().map(StripId).collect());
            }
        }
        Ok(vec![strip])
    }

    async fn set_stereo_link(&self, source: StripId, linked: bool)
        -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        let pair = self.stereo_pairs().into_iter().find(|p| p.contains(&source.0))
            .ok_or_else(|| anyhow::anyhow!("sim: channel has no stereo partner"))?;
        let peer = StripId(if pair[0] == source.0 { pair[1] } else { pair[0] });
        let mut changes = Vec::new();
        if linked && self.get_param(StripId(pair[0]), "link").await? != ParamValue::Bool(true) {
            let values = self.params.read().expect("sim params poisoned")[source.0 as usize].clone();
            for (field, value) in values {
                if matches!(field.as_str(), "link" | "pan" | "source" | "source_a" | "source_b" | "source_select" | "gain" | "trim" | "phantom" | "usbreturn") { continue; }
                let Some(def) = strip_schema::definition(&field) else { continue; };
                if !strip_schema::supports(def, kind_of_strip(source.0 as usize)?) { continue; }
                self.set_param(peer, &field, value.clone()).await?;
                changes.push((peer, field, value));
            }
            let source_strip = self.read()[source.0 as usize].clone();
            for (field, value) in [("name", ParamValue::Text(source_strip.name)), ("fader", ParamValue::Float(source_strip.fader)), ("mute", ParamValue::Bool(source_strip.mute))] {
                self.set_param(peer, field, value.clone()).await?;
                changes.push((peer, field.to_string(), value));
            }
            self.set_param(StripId(pair[0]), "pan", ParamValue::Float(0.0)).await?;
            changes.push((StripId(pair[0]), "pan".to_string(), ParamValue::Float(0.0)));
        }
        for member in pair {
            self.set_param(StripId(member), "link", ParamValue::Bool(linked)).await?;
            changes.push((StripId(member), "link".to_string(), ParamValue::Bool(linked)));
        }
        Ok(changes)
    }

    fn param_defs(&self) -> &'static [openfader_core::params::ParamDef] {
        static DEFS: std::sync::OnceLock<Vec<openfader_core::params::ParamDef>> = std::sync::OnceLock::new();
        DEFS.get_or_init(|| {
            let mut balance = *openfader_core::params::find("pan").expect("catalogue has pan");
            balance.id = "balance";
            balance.label = "Balance";
            balance.scope = openfader_core::params::ParamScope::Main;
            balance.scopes = &[StripKind::Main];
            std::iter::once(balance).chain(strip_schema::extra().iter().copied()).collect()
        })
    }

    fn input_processor_guard(&self) -> bool { true }
    async fn input_processor(&self, strip: StripId, processor: &str, write: Option<(&str, ParamValue)>, expected: Option<&serde_json::Value>, check: openfader_core::input_processors::RecallCheck<'_>) -> anyhow::Result<serde_json::Value> {
        self.guarded_input_processor(strip, processor, write, expected, check)
    }
    fn name(&self) -> &'static str {
        "sim"
    }

    /// The whole MS param-plane catalogue: every id reads/writes bit-exact
    /// per-strip state (strip-backed `fader`/`mute`/`name`/`pan` live in
    /// [`ChannelStrip`]; the rest in the per-strip param map). Tests mutate
    /// state desk-side through `set_param`, the sim has no separate desk,
    /// so the set path IS the desk-mutation path the relay poller observes.
    fn capabilities(&self) -> &'static [&'static str] {
        static IDS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
        IDS.get_or_init(|| {
            openfader_core::params::CATALOGUE
                .iter()
                .map(|d| d.id)
                .chain(self.param_defs().iter().map(|d| d.id))
                .collect()
        })
    }

    fn param_text_rules(&self, param: &str) -> Option<openfader_core::params::ParamTextRules> {
        (param == "name").then_some(openfader_core::params::ParamTextRules {
            max_utf8_bytes: Some(16), allow_empty: false, allow_edge_whitespace: true,
        })
    }

    fn channel_count(&self) -> usize {
        SIM_CHANNELS
    }

    /// Full v2 topology (81 strips, indices 0..39 stable; see module docs).
    fn topology(&self) -> Vec<StripFamily> {
        SIM_TOPOLOGY.to_vec()
    }

    /// Total addressable strips (81: inputs + buses + extended families).
    fn strip_count(&self) -> usize {
        SIM_STRIPS
    }

    async fn connect(&self, _addr: &str) -> anyhow::Result<()> {
        let _routes = self.output_routes.write().expect("sim output routes poisoned");
        self.processor_generation.fetch_add(1, Ordering::AcqRel);
        *self.connected.write().expect("sim flag poisoned") = true;
        Ok(())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        let i = self.idx(strip)?;
        self.write()[i].fader = v;
        Ok(())
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].fader)
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let i = self.idx(strip)?;
        self.write()[i].mute = muted;
        Ok(())
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].mute)
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        if name.is_empty() || name.len() > 16 {
            anyhow::bail!("sim: name must be 1..=16 chars");
        }
        let i = self.idx(strip)?;
        self.write()[i].name = name.to_string();
        Ok(())
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].name.clone())
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        if bus as usize >= SIM_BUSES {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        let i = SIM_CHANNELS + bus as usize;
        self.write()[i].fader = v;
        Ok(())
    }

    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        if bus as usize >= SIM_BUSES {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        let i = SIM_CHANNELS + bus as usize;
        Ok(self.read()[i].fader)
    }

    fn bus_count(&self) -> u8 {
        SIM_BUSES as u8
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        if bus as usize >= SIM_BUSES {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        let i = SIM_CHANNELS + bus as usize;
        self.write()[i].mute = muted;
        Ok(())
    }

    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        if bus as usize >= SIM_BUSES {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        let i = SIM_CHANNELS + bus as usize;
        Ok(self.read()[i].mute)
    }

    fn subgroup_count(&self) -> u8 {
        SIM_SUBGROUPS as u8
    }


    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        let i = Self::sim_check_subgroup(group)?;
        self.subgroups.write().expect("sim subgroups poisoned")[i].0 = v;
        Ok(())
    }

    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
        let i = Self::sim_check_subgroup(group)?;
        Ok(self.subgroups.read().expect("sim subgroups poisoned")[i].0)
    }

    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let i = Self::sim_check_subgroup(group)?;
        self.subgroups.write().expect("sim subgroups poisoned")[i].1 = muted;
        Ok(())
    }

    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
        let i = Self::sim_check_subgroup(group)?;
        Ok(self.subgroups.read().expect("sim subgroups poisoned")[i].1)
    }

    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let key = self.sim_check_sub_assign(ch, group)?;
        self.sub_assign.write().expect("sim sub_assign poisoned").insert(key, member);
        Ok(())
    }

    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let key = self.sim_check_sub_assign(ch, group)?;
        Ok(self.sub_assign.read().expect("sim sub_assign poisoned").get(&key).copied().unwrap_or(false))
    }

    fn mute_group_count(&self) -> u8 {
        SIM_MUTE_GROUPS as u8
    }

    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let i = Self::sim_check_mute_group(group)?;
        self.mute_groups.write().expect("sim mute_groups poisoned")[i] = muted;
        Ok(())
    }

    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let i = Self::sim_check_mute_group(group)?;
        Ok(self.mute_groups.read().expect("sim mute_groups poisoned")[i])
    }

    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let key = self.sim_check_mg_assign(ch, group)?;
        self.mg_assign.write().expect("sim mg_assign poisoned").insert(key, member);
        Ok(())
    }

    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let key = self.sim_check_mg_assign(ch, group)?;
        Ok(self.mg_assign.read().expect("sim mg_assign poisoned").get(&key).copied().unwrap_or(false))
    }

    fn file_transfer(&self) -> bool {
        true
    }

    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        Self::sim_check_filename(filename)?;
        self.files
            .read()
            .expect("sim files poisoned")
            .get(filename)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| anyhow::anyhow!("sim: file {filename:?} not found"))
    }

    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        Self::sim_check_filename(filename)?;
        self.files
            .write()
            .expect("sim files poisoned")
            .insert(filename.to_string(), (data.to_vec(), file_type));
        Ok(())
    }

    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        // Master commands, strip params, snapshots, recall and meters all
        // address the same LR fader, just as they do for the LR mute.
        self.set_fader(StripId(SIM_TOPOLOGY[6].first as u16), value).await
    }

    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        self.get_fader(StripId(SIM_TOPOLOGY[6].first as u16)).await
    }

    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        // Share the Main strip with snapshots, param reads and meters.
        // A separate mute bit lets the next state frame undo the main echo.
        self.set_mute(StripId(SIM_TOPOLOGY[6].first as u16), muted).await
    }

    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        self.get_mute(StripId(SIM_TOPOLOGY[6].first as u16)).await
    }

    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        self.idx(strip)?;
        self.legacy_solo_descriptor(strip.0, soloed);
        self.solos.write().expect("sim solos poisoned").insert(strip.0, soloed);
        Ok(())
    }

    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        self.idx(strip)?;
        Ok(self.solos.read().expect("sim solos poisoned").get(&strip.0).copied().unwrap_or(false))
    }

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        let i = self.idx(strip)?;
        let kind = kind_of_strip(i)?;
        match (param, &value) {
            ("fader", ParamValue::Float(v)) => return self.set_fader(strip, *v).await,
            ("mute", ParamValue::Bool(m)) => return self.set_mute(strip, *m).await,
            ("name", ParamValue::Text(n)) => return self.set_name(strip, n).await,
            ("balance", ParamValue::Float(v)) => {
                if kind != StripKind::Main {
                    anyhow::bail!("sim: balance out of scope for {kind:?} strips");
                }
                self.write()[i].pan = Self::check_pan(*v)?;
                return Ok(());
            }
            ("pan", ParamValue::Float(v)) => {
                if !strip_schema::supports(
                    openfader_core::params::find("pan").expect("catalogue has pan"),
                    kind,
                ) {
                    anyhow::bail!("sim: pan out of scope for {kind:?} strips");
                }
                let v = Self::check_pan(*v)?;
                self.write()[i].pan = v;
                return Ok(());
            }
            _ => {}
        }
        if let Some(def) = strip_schema::definition(param) {
            if !strip_schema::supports(def, kind) {
                anyhow::bail!("sim: {param} out of scope for {kind:?} strips");
            }
            if openfader_core::params::find(param).is_some() { openfader_core::params::validate(param, &value)?; }
            else { anyhow::ensure!(matches!(value, ParamValue::Bool(_)), "sim membership requires a boolean"); }
            let mut params = self.params.write().expect("sim params poisoned");
            if params[i].get(param) != Some(&value) { self.processor_revision.fetch_add(1, Ordering::AcqRel); }
            params[i].insert(param.to_string(), value);
            return Ok(());
        }
        anyhow::bail!("sim: unknown param {param:?}")
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        let i = self.idx(strip)?;
        let kind = kind_of_strip(i)?;
        match param {
            "fader" => return self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => return self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => return self.get_name(strip).await.map(ParamValue::Text),
            "balance" => {
                if kind != StripKind::Main {
                    anyhow::bail!("sim: balance out of scope for {kind:?} strips");
                }
                return Ok(ParamValue::Float(self.read()[i].pan));
            }
            "pan" => {
                if !strip_schema::supports(
                    openfader_core::params::find("pan").expect("catalogue has pan"),
                    kind,
                ) {
                    anyhow::bail!("sim: pan out of scope for {kind:?} strips");
                }
                return Ok(ParamValue::Float(self.read()[i].pan));
            }
            _ => {}
        }
        if let Some(def) = strip_schema::definition(param) {
            if !strip_schema::supports(def, kind) {
                anyhow::bail!("sim: {param} out of scope for {kind:?} strips");
            }
            if let Some(v) = self.params.read().expect("sim params poisoned")[i].get(param) {
                return Ok(v.clone());
            }
            return Ok(def.default.as_param_value());
        }
        anyhow::bail!("sim: unknown param {param:?}")
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        Ok(MixerSnapshot {
            strips: self.read().clone(),
        })
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        if snapshot.strips.len() != SIM_STRIPS {
            anyhow::bail!(
                "sim: snapshot has {} strips, need {SIM_STRIPS}",
                snapshot.strips.len()
            );
        }
        *self.write() = snapshot.strips.clone();
        Ok(())
    }

    async fn query_info(&self) -> anyhow::Result<openfader_core::MixerInfo> {
        let model = match self.preset {
            SimPreset::Generic => "SimDriver 32+8".to_string(),
            p => format!("SimDriver 32+8 ({})", p.name()),
        };
        Ok(openfader_core::MixerInfo {
            name: "sim".into(),
            model,
            firmware: "0.1.0".into(),
            addr: "sim://local".into(),
        })
    }

    fn poll_meters(&self) -> MeterFrame {
        let tick = self.signal_tick();
        let n = self.read().len();
        let levels = (0..n).map(|i| self.meter_level(i, tick)).collect();
        MeterFrame { levels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn studio_preset_mirrors_rig() {
        assert_eq!(SimPreset::parse("studio"), Some(SimPreset::Studio));
        assert_eq!(SimPreset::parse("STUDIO"), Some(SimPreset::Studio));
        assert_eq!(SimPreset::Studio.name(), "studio");
        assert_eq!(SimPreset::parse("xr18"), Some(SimPreset::Xr18));
        assert_eq!(SimPreset::parse("dl32s"), Some(SimPreset::Dl32s));
        assert_eq!(SimPreset::parse("generic"), Some(SimPreset::Generic));

        let d = SimDriver::with_preset(SimPreset::Studio);
        assert_eq!(d.preset(), SimPreset::Studio);

        let names = async {
            let mut out = Vec::with_capacity(SIM_CHANNELS);
            for i in 0..SIM_CHANNELS {
                out.push(d.get_name(StripId(i as u16)).await.unwrap());
            }
            out
        }
        .await;
        let expected: Vec<String> = STUDIO_CHANNELS
            .iter()
            .map(|s| s.to_string())
            .chain((STUDIO_CHANNELS.len()..SIM_CHANNELS).map(|i| format!("CH{:02}", i + 1)))
            .collect();
        assert_eq!(names, expected);
        assert_eq!(names[0], "BASS");
        assert_eq!(names[3], "VOX");
        assert_eq!(names[4], "VOX");
        assert_eq!(names[6], "GTR L");
        assert_eq!(names[8], "DRUM L");
        assert_eq!(names[12], "AC GTR");
        assert_eq!(names[14], "Abelton");

        for (i, (f, m)) in STUDIO_FADERS.iter().zip(STUDIO_MUTES.iter()).enumerate() {
            let got = d.get_fader(StripId(i as u16)).await.unwrap();
            assert!((got - f).abs() < 1e-6, "ch{} fader {got} != {f}", i + 1);
            assert_eq!(d.get_mute(StripId(i as u16)).await.unwrap(), *m, "ch{}", i + 1);
        }
        assert_eq!(names[16], "CH17");
        assert_eq!(d.get_fader(StripId(16)).await.unwrap(), 0.0);
        assert!(d.get_mute(StripId(16)).await.unwrap());

        for (b, name) in STUDIO_BUSES.iter().enumerate() {
            let strip = StripId((SIM_CHANNELS + b) as u16);
            assert_eq!(d.get_name(strip).await.unwrap(), *name, "bus{}", b + 1);
            let got = d.get_bus_fader(b as u8).await.unwrap();
            assert!((got - STUDIO_BUS_FADERS[b]).abs() < 1e-6, "bus{} {got}", b + 1);
            assert!(!d.get_mute(strip).await.unwrap(), "bus{}", b + 1);
        }

        assert!((d.get_main_fader().await.unwrap() - STUDIO_MAIN).abs() < 1e-6);
        let info = d.query_info().await.unwrap();
        assert!(info.model.contains("studio"), "model: {}", info.model);
    }

    #[tokio::test]
    async fn desk_presets_untouched() {
        let g = SimDriver::new();
        assert_eq!(g.preset(), SimPreset::Generic);
        assert_eq!(g.get_name(StripId(0)).await.unwrap(), "CH01");
        assert_eq!(g.get_name(StripId(32)).await.unwrap(), "BUS1");
        assert!((g.get_main_fader().await.unwrap() - 0.8).abs() < 1e-6);

        let x = SimDriver::with_preset(SimPreset::Xr18);
        assert_eq!(x.get_name(StripId(0)).await.unwrap(), "Kick");
        assert_eq!(x.get_name(StripId(18)).await.unwrap(), "CH19");
        let dl = SimDriver::with_preset(SimPreset::Dl32s);
        assert_eq!(dl.get_name(StripId(0)).await.unwrap(), "Kick");
        assert_eq!(dl.get_name(StripId(31)).await.unwrap(), "Spare 6");
    }

    #[tokio::test]
    async fn param_roundtrip_float_gain() {
        let d = SimDriver::new();
        for v in [12.5f32, 0.1, -11.95, 60.0] {
            d.set_param(StripId(0), "gain", ParamValue::Float(v))
                .await
                .unwrap();
            assert_eq!(
                d.get_param(StripId(0), "gain").await.unwrap(),
                ParamValue::Float(v),
                "gain {v}"
            );
        }
    }

    #[tokio::test]
    async fn param_roundtrip_bool_phantom() {
        let d = SimDriver::new();
        d.set_param(StripId(3), "phantom", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(3), "phantom").await.unwrap(),
            ParamValue::Bool(true)
        );
        d.set_param(StripId(3), "phantom", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(3), "phantom").await.unwrap(),
            ParamValue::Bool(false)
        );
    }

    #[tokio::test]
    async fn param_roundtrip_text_name() {
        let d = SimDriver::new();
        d.set_param(StripId(0), "name", ParamValue::Text("Kick".into()))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(0), "name").await.unwrap(),
            ParamValue::Text("Kick".into())
        );
        assert_eq!(d.get_name(StripId(0)).await.unwrap(), "Kick");
    }

    #[tokio::test]
    async fn param_covers_detail_panel_families() {
        let d = SimDriver::new();
        let s = StripId(1);
        let fields: &[(&str, ParamValue)] = &[
            ("pan", ParamValue::Float(-25.0)),
            ("hpf_on", ParamValue::Bool(true)),
            ("hpf_freq", ParamValue::Float(80.0)),
            ("gate_on", ParamValue::Bool(true)),
            ("gate_thr", ParamValue::Float(-30.0)),
            ("comp_on", ParamValue::Bool(true)),
            ("comp_ratio", ParamValue::Float(4.0)),
            ("eq_on", ParamValue::Bool(true)),
            ("eq1_gain", ParamValue::Float(3.0)),
            ("eq4_freq", ParamValue::Float(12000.0)),
            ("eq2_q", ParamValue::Float(1.4)),
            ("eq2_type", ParamValue::Float(1.0)),
            ("aux1_level", ParamValue::Float(-10.0)),
            ("aux1_mute", ParamValue::Bool(true)),
            ("fx2_level", ParamValue::Float(-20.0)),
            ("fx2_mute", ParamValue::Bool(false)),
            ("send1_level", ParamValue::Float(-10.0)),
        ];
        for (f, v) in fields {
            d.set_param(s, f, v.clone())
                .await
                .unwrap_or_else(|e| panic!("set {f}: {e}"));
        }
        for (f, v) in fields {
            assert_eq!(
                d.get_param(s, f).await.unwrap(),
                *v,
                "roundtrip {f}"
            );
        }
        for legacy in ["gate_threshold", "comp_threshold", "mutegroup1"] {
            let err = d
                .set_param(s, legacy, ParamValue::Float(0.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown param"), "{legacy}: {err}");
            let err = d.get_param(s, legacy).await.unwrap_err().to_string();
            assert!(err.contains("unknown param"), "{legacy}: {err}");
        }
    }

    #[tokio::test]
    async fn r2_fx_rack_matches_design() {
        let d = SimDriver::new();
        assert_eq!(d.fx_slot_count(), SIM_FX_SLOTS);
        let slots = d.fx_slots();
        let types: Vec<&str> = slots.iter().map(|s| s.fx_type.as_str()).collect();
        assert_eq!(types, vec!["Hall", "Plate", "Delay", "Chorus", "Room", "Delay", "Flanger", "Hall"]);
        assert!(slots[..7].iter().all(|s| s.on));
        assert!(!slots[7].on);
        assert_eq!(slots[0].params, vec![0.42, 0.15, 0.6, 0.55, 1.0]);
        d.set_fx_type(1, "Chorus").unwrap();
        assert_eq!(d.get_fx_type(1).unwrap(), "Chorus");
        assert_eq!(d.get_fx_param(1, "rate").unwrap(), 0.4);
        d.set_fx_param(1, "rate", 0.75).unwrap();
        assert_eq!(d.get_fx_param(1, "rate").unwrap(), 0.75);
        assert!(d.set_fx_param(1, "decay", 0.5).is_err());
        assert!(d.set_fx_param(1, "rate", 1.5).is_err());
        assert!(d.set_fx_param(0, "rate", 0.5).is_err());
        assert!(d.set_fx_param(9, "rate", 0.5).is_err());
        assert!(d.set_fx_type(1, "Reverb").is_err());
        d.set_fx_on(8, true).unwrap();
        d.set_fx_bypass(8, true).unwrap();
        let slots = d.fx_slots();
        assert!(slots[7].on && slots[7].bypass);
    }

    #[tokio::test]
    async fn r2_patch_roundtrip() {
        let d = SimDriver::new();
        assert_eq!(
            d.get_param(StripId(0), "source").await.unwrap(),
            ParamValue::Text("Lcl 01".into())
        );
        let label = d.set_patch(StripId(0), "Net", 5).unwrap();
        assert_eq!(label, "Net 05");
        assert_eq!(
            d.get_param(StripId(0), "source").await.unwrap(),
            ParamValue::Text("Net 05".into())
        );
        d.clear_patch(StripId(0)).unwrap();
        assert_eq!(
            d.get_param(StripId(0), "source").await.unwrap(),
            ParamValue::Text("—".into())
        );
        assert!(d.set_patch(StripId(32), "Lcl", 1).is_err());
        assert!(d.set_patch(StripId(0), "AES", 1).is_err());
        assert!(d.set_patch(StripId(0), "Lcl", 0).is_err());
        assert!(d.set_patch(StripId(0), "Lcl", 33).is_err());
        assert_eq!(SIM_DEVICES.len(), 3);
        assert_eq!(SIM_DEVICES[0].ip, "192.168.1.120");
        assert_eq!(SIM_DEVICES[0].state, "connected");
        assert_eq!(SIM_DEVICES[1].state, "available");
        assert_eq!(SIM_DEVICES[2].kind, "io");
    }

    #[tokio::test]
    async fn r2_taps_cover_every_strip() {
        let d = SimDriver::new();
        let taps = d.poll_taps();
        assert_eq!(taps.len(), SIM_STRIPS);
        for t in &taps {
            assert!((0.0..=1.0).contains(&t.input), "in {t:?}");
            assert!((0.0..=1.0).contains(&t.post), "post {t:?}");
            assert!(t.gr_gate_db >= 0.0 && t.gr_comp_db >= 0.0, "gr {t:?}");
        }
        d.set_mute(StripId(0), true).await.unwrap();
        let taps = d.poll_taps();
        assert_eq!(taps[0].post, 0.0);
        assert!(taps[0].input > 0.0);
    }

    #[tokio::test]
    async fn param_snapshot_includes_params() {
        let d = SimDriver::new();
        let s = StripId(0);
        d.set_param(s, "gain", ParamValue::Float(12.5))
            .await
            .unwrap();
        d.set_param(s, "phantom", ParamValue::Bool(true))
            .await
            .unwrap();
        d.set_param(s, "name", ParamValue::Text("Kick".into()))
            .await
            .unwrap();
        d.set_param(s, "pan", ParamValue::Float(-25.0))
            .await
            .unwrap();
        let snap = d.snapshot().await.unwrap();
        assert_eq!(snap.strips[0].name, "Kick");
        assert_eq!(snap.strips[0].pan, -25.0);
        d.recall(&snap).await.unwrap();
        assert_eq!(
            d.get_param(s, "gain").await.unwrap(),
            ParamValue::Float(12.5)
        );
        assert_eq!(
            d.get_param(s, "phantom").await.unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            d.get_param(s, "name").await.unwrap(),
            ParamValue::Text("Kick".into())
        );
        assert_eq!(
            d.get_param(s, "pan").await.unwrap(),
            ParamValue::Float(-25.0)
        );
    }

    #[tokio::test]
    async fn param_rejects_unknown_and_wrong_kind() {
        let d = SimDriver::new();
        let s = StripId(0);
        for bad in ["nope", "eq7_gain", "aux9_level", "fx5_mute", "gate_key_filter"] {
            let err = d
                .set_param(s, bad, ParamValue::Float(1.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{bad}: {err}");
            let err = d.get_param(s, bad).await.unwrap_err().to_string();
            assert!(err.contains("unknown"), "{bad}: {err}");
        }
        let err = d
            .set_param(s, "gain", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Float"), "{err}");
        let err = d
            .set_param(s, "phantom", ParamValue::Float(1.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Bool"), "{err}");
        assert!(d
            .set_param(s, "gain", ParamValue::Float(f32::NAN))
            .await
            .is_err());
        d.set_param(s, "pan", ParamValue::Float(-100.0))
            .await
            .unwrap();
        d.set_param(s, "pan", ParamValue::Float(100.0))
            .await
            .unwrap();
        assert!(d
            .set_param(s, "pan", ParamValue::Float(100.1))
            .await
            .is_err());
        assert!(d
            .set_param(s, "pan", ParamValue::Float(f32::NAN))
            .await
            .is_err());
        let g = d.get_param(StripId(5), "gain").await.unwrap();
        assert!(
            openfader_core::params::validate("gain", &g).is_ok(),
            "seeded gain validates: {g:?}"
        );
        assert!(d
            .set_param(StripId(99), "gain", ParamValue::Float(1.0))
            .await
            .is_err());
        assert!(d.get_param(StripId(99), "gain").await.is_err());
    }

    #[tokio::test]
    async fn capabilities_cover_the_whole_catalogue() {
        use openfader_core::params;
        let d = SimDriver::new();
        let caps = d.capabilities();
        assert_eq!(
            caps.len(),
            params::CATALOGUE.len() + d.param_defs().len(),
            "sim implements the whole catalogue"
        );
        for def in params::CATALOGUE {
            assert!(
                caps.contains(&def.id),
                "sim capabilities miss catalogue id {}",
                def.id
            );
        }
    }

    #[tokio::test]
    async fn param_roundtrip_every_catalogue_id() {
        use openfader_core::params::ParamKind;
        let d = SimDriver::new();
        for def in openfader_core::params::CATALOGUE {
            let s = first_supporting_strip(def.id).expect("every id is supported somewhere");
            let v = match def.kind {
                ParamKind::Float => ParamValue::Float((def.min + def.max) / 2.0),
                ParamKind::Enum => ParamValue::Float(def.min),
                ParamKind::Bool => ParamValue::Bool(true),
                ParamKind::Text => ParamValue::Text("T".into()),
            };
            d.set_param(s, def.id, v.clone())
                .await
                .unwrap_or_else(|e| panic!("set {}: {e}", def.id));
            assert_eq!(
                d.get_param(s, def.id).await.unwrap(),
                v,
                "roundtrip {}",
                def.id
            );
        }
        let s = StripId(2);
        for id in ["usbreturn", "eq1_type", "eq2_type"] {
            let def = openfader_core::params::find(id).unwrap();
            d.set_param(s, id, ParamValue::Float(def.max)).await.unwrap();
            assert_eq!(
                d.get_param(s, id).await.unwrap(),
                ParamValue::Float(def.max)
            );
        }
    }

    #[tokio::test]
    async fn every_catalogue_id_readable_from_first_poll() {
        use openfader_core::params;
        for preset in [
            SimPreset::Generic,
            SimPreset::Xr18,
            SimPreset::Dl32s,
            SimPreset::Studio,
        ] {
            let d = SimDriver::with_preset(preset);
            let mut checked = 0usize;
            for ch in 0..SIM_CHANNELS {
                let s = StripId(ch as u16);
                for def in params::CATALOGUE {
                    if !params::supports_kind(def, openfader_core::StripKind::In) {
                        continue;
                    }
                    checked += 1;
                    let v = d
                        .get_param(s, def.id)
                        .await
                        .unwrap_or_else(|e| panic!("{preset:?} ch{ch} {}: {e}", def.id));
                    params::validate(def.id, &v)
                        .unwrap_or_else(|e| panic!("{preset:?} ch{ch} {} = {v:?}: {e}", def.id));
                }
            }
            assert!(checked > 100, "input strips carry the bulk of the catalogue");
            for def in params::CATALOGUE {
                if params::supports_kind(def, openfader_core::StripKind::In) {
                    continue;
                }
                let s = first_supporting_strip(def.id).expect("scoped id has a strip");
                d.get_param(s, def.id)
                    .await
                    .unwrap_or_else(|e| panic!("{preset:?} strip{} {}: {e}", s.0, def.id));
            }
        }
        let d = SimDriver::with_preset(SimPreset::Studio);
        async fn gain_of(d: &SimDriver, ch: u16) -> f32 {
            match d.get_param(StripId(ch), "gain").await.unwrap() {
                ParamValue::Float(v) => v,
                v => panic!("gain not float: {v:?}"),
            }
        }
        assert!((gain_of(&d, 0).await - 30.0).abs() < 1e-6, "BASS gain");
        assert_eq!(
            d.get_param(StripId(0), "hpf_on").await.unwrap(),
            ParamValue::Bool(false),
            "no HPF on bass"
        );
        assert_eq!(
            d.get_param(StripId(3), "hpf_on").await.unwrap(),
            ParamValue::Bool(true),
            "HPF on vox"
        );
        assert_eq!(
            d.get_param(StripId(8), "gate_on").await.unwrap(),
            ParamValue::Bool(true),
            "gate on drums"
        );
        assert_eq!(
            d.get_param(StripId(0), "comp_on").await.unwrap(),
            ParamValue::Bool(true),
            "comp on bass"
        );
        assert!(
            (gain_of(&d, 14).await - 15.0).abs() < 1e-6,
            "playback gain, got {}",
            gain_of(&d, 14).await
        );
        d.set_param(StripId(7), "gain", ParamValue::Float(33.0))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(7), "gain").await.unwrap(),
            ParamValue::Float(33.0)
        );
    }

    #[test]
    fn topology_is_ordered_contiguous_and_stable() {
        assert_eq!(SIM_STRIPS, 81);
        let mut next = 0usize;
        for fam in SIM_TOPOLOGY {
            assert_eq!(fam.first, next, "gap/overlap before {}", fam.key);
            assert!(fam.count > 0, "empty family {}", fam.key);
            assert!(!fam.label.is_empty() && !fam.name_prefix.is_empty(), "{}", fam.key);
            next = fam.end();
        }
        assert_eq!(next, SIM_STRIPS);
        assert_eq!(SIM_TOPOLOGY[0].kind, StripKind::In);
        assert_eq!((SIM_TOPOLOGY[0].first, SIM_TOPOLOGY[0].count), (0, 32));
        assert_eq!(SIM_TOPOLOGY[1].kind, StripKind::Bus);
        assert_eq!((SIM_TOPOLOGY[1].first, SIM_TOPOLOGY[1].count), (32, 8));
        let colours: Vec<u8> = SIM_TOPOLOGY.iter().map(|f| f.color_default).collect();
        assert_eq!(colours, vec![2, 6, 2, 6, 4, 11, 11, 0, 2]);
        for (i, fam) in SIM_TOPOLOGY.iter().enumerate() {
            for s in fam.first..fam.end() {
                assert_eq!(kind_of_strip(s).unwrap(), fam.kind, "strip {s}");
            }
            let _ = i;
        }
        assert!(kind_of_strip(SIM_STRIPS).is_err());
    }

    #[tokio::test]
    async fn every_strip_answers_its_scoped_set() {
        use openfader_core::params;
        let d = SimDriver::new();
        for fam in SIM_TOPOLOGY {
            for s in fam.first..fam.end() {
                let strip = StripId(s as u16);
                for def in params::CATALOGUE {
                    let supported = strip_schema::supports(def, fam.kind);
                    let got = d.get_param(strip, def.id).await;
                    if supported {
                        let v = got.unwrap_or_else(|e| {
                            panic!("strip {s} ({:?}) {}: {e}", fam.kind, def.id)
                        });
                        params::validate(def.id, &v).unwrap_or_else(|e| {
                            panic!("strip {s} {} = {v:?}: {e}", def.id)
                        });
                    } else {
                        assert!(
                            got.is_err(),
                            "strip {s} ({:?}) unexpectedly answers {}",
                            fam.kind,
                            def.id
                        );
                    }
                }
            }
        }
        d.set_param(StripId(56), "eq2_gain", ParamValue::Float(2.5))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(56), "eq2_gain").await.unwrap(),
            ParamValue::Float(2.5)
        );
        d.set_param(StripId(73), "fader", ParamValue::Float(0.5))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(73), "fader").await.unwrap(),
            ParamValue::Float(0.5)
        );
        for (strip, id) in [(73u16, "gain"), (73, "pan"), (72, "pan"), (56, "gain")] {
            let err = d
                .get_param(StripId(strip), id)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("out of scope"), "{id} on {strip}: {err}");
            assert!(!err.contains("not supported"), "{err}");
            assert!(!err.contains("unknown param"), "{err}");
        }
    }
}
