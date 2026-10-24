//! Factory channel-strip starting points.
//!
//! Ten conservative presets (Kick, Snare, Hat, Tom, OH, Bass DI, E-Gtr,
//! Vox, Keys, Playback) expressed as generic `set_param` field/value pairs.
//! Apply with [`apply_preset`]; drivers need no changes.
//!
//! Field vocabulary is the canonical param catalogue
//! ([`crate::params::CATALOGUE`]): `hpf_*`, `gate_*`, `comp_*`,
//! `eq{1..4}_*`. Every id resolves on every driver whose capabilities cover
//! it (sim covers the whole catalogue; X-Air covers the loopback-tested
//! param plane). No sends (mix-dependent), no unverified comp/gate timing
//! words, those stay the engineer's call on the desk.
//!
//! Values are STARTING POINTS, not targets: HPFs sit at the safe end,
//! cuts are shallow (-2..-3 dB), boosts cap at +3 dB, ratios at 2:1..4:1.
//! The engineer still dials in against the room and the source.
//! These values ship to every client, keep them conservative.

use crate::{MixerDriver, ParamValue, StripId};

#[derive(Clone, Debug, PartialEq)]
pub struct PresetSetting {
    pub field: &'static str,
    pub value: ParamValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    /// One-line conservative-use note shown in the preview.
    pub blurb: &'static str,
    pub settings: &'static [PresetSetting],
}

const fn f(field: &'static str, v: f32) -> PresetSetting {
    PresetSetting {
        field,
        value: ParamValue::Float(v),
    }
}

const fn b(field: &'static str, v: bool) -> PresetSetting {
    PresetSetting {
        field,
        value: ParamValue::Bool(v),
    }
}

static KICK: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 45.0),
    b("gate_on", true),
    f("gate_thr", -30.0),
    b("comp_on", true),
    f("comp_ratio", 4.0),
    f("comp_thr", -18.0),
    b("eq_on", true),
    f("eq1_gain", -2.0),
    f("eq1_freq", 80.0),
    f("eq4_gain", 3.0),
    f("eq4_freq", 4000.0),
];

static SNARE: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 90.0),
    b("gate_on", true),
    f("gate_thr", -28.0),
    b("comp_on", true),
    f("comp_ratio", 3.0),
    f("comp_thr", -16.0),
    b("eq_on", true),
    f("eq2_gain", -3.0),
    f("eq2_freq", 450.0),
    f("eq4_gain", 2.0),
    f("eq4_freq", 6000.0),
];

static HAT: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 300.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 2.0),
    f("comp_thr", -20.0),
    b("eq_on", true),
    f("eq1_gain", -3.0),
    f("eq1_freq", 250.0),
    f("eq4_gain", -2.0),
    f("eq4_freq", 10000.0),
];

static TOM: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 70.0),
    b("gate_on", true),
    f("gate_thr", -28.0),
    b("comp_on", true),
    f("comp_ratio", 3.0),
    f("comp_thr", -16.0),
    b("eq_on", true),
    f("eq2_gain", 2.0),
    f("eq2_freq", 220.0),
    f("eq4_gain", 2.0),
    f("eq4_freq", 5000.0),
];

static OH: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 120.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 2.0),
    f("comp_thr", -22.0),
    b("eq_on", true),
    f("eq1_gain", -3.0),
    f("eq1_freq", 200.0),
    f("eq4_gain", 1.5),
    f("eq4_freq", 10000.0),
];

static BASS_DI: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 35.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 4.0),
    f("comp_thr", -16.0),
    b("eq_on", true),
    f("eq1_gain", 2.0),
    f("eq1_freq", 90.0),
    f("eq2_gain", -2.0),
    f("eq2_freq", 350.0),
];

static EGTR: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 90.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 3.0),
    f("comp_thr", -18.0),
    b("eq_on", true),
    f("eq1_gain", -3.0),
    f("eq1_freq", 120.0),
    f("eq4_gain", -3.0),
    f("eq4_freq", 8000.0),
];

static VOX: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 90.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 3.0),
    f("comp_thr", -14.0),
    b("eq_on", true),
    f("eq1_gain", -2.0),
    f("eq1_freq", 200.0),
    f("eq2_gain", 2.0),
    f("eq2_freq", 3000.0),
    f("eq4_gain", 2.0),
    f("eq4_freq", 12000.0),
];

static KEYS: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 40.0),
    b("gate_on", false),
    b("comp_on", true),
    f("comp_ratio", 2.0),
    f("comp_thr", -20.0),
    b("eq_on", true),
    f("eq2_gain", 1.0),
    f("eq2_freq", 2500.0),
];

static PLAYBACK: &[PresetSetting] = &[
    b("hpf_on", true),
    f("hpf_freq", 40.0),
    b("gate_on", false),
    b("comp_on", false),
    b("eq_on", true),
    f("eq1_gain", 0.0),
    f("eq1_freq", 100.0),
    f("eq2_gain", 0.0),
    f("eq2_freq", 2000.0),
    f("eq3_gain", 0.0),
    f("eq3_freq", 3000.0),
    f("eq4_gain", 0.0),
    f("eq4_freq", 12000.0),
];

/// Factory presets in picker order.
pub static PRESETS: &[ChannelPreset] = &[
    ChannelPreset {
        id: "kick",
        label: "Kick",
        group: "Drums",
        blurb: "Low-cut 45 Hz, gate, 4:1, mud trim + beater click. Starting point.",
        settings: KICK,
    },
    ChannelPreset {
        id: "snare",
        label: "Snare",
        group: "Drums",
        blurb: "Low-cut 90 Hz, gate, 3:1, box cut + snap. Starting point.",
        settings: SNARE,
    },
    ChannelPreset {
        id: "hat",
        label: "Hat",
        group: "Drums",
        blurb: "High low-cut, no gate, light 2:1, harshness trim. Starting point.",
        settings: HAT,
    },
    ChannelPreset {
        id: "tom",
        label: "Tom",
        group: "Drums",
        blurb: "Low-cut 70 Hz, gate, 3:1, body + attack. Starting point.",
        settings: TOM,
    },
    ChannelPreset {
        id: "oh",
        label: "OH",
        group: "Drums",
        blurb: "Low-cut 120 Hz, no gate, glue 2:1, low trim + air. Starting point.",
        settings: OH,
    },
    ChannelPreset {
        id: "bass",
        label: "Bass DI",
        group: "Instruments",
        blurb: "Sub-cut 35 Hz, no gate, 4:1, weight + mud cut. Starting point.",
        settings: BASS_DI,
    },
    ChannelPreset {
        id: "egtr",
        label: "E-Gtr",
        group: "Instruments",
        blurb: "Low-cut 90 Hz, no gate, 3:1, rumble + fizz trim. Starting point.",
        settings: EGTR,
    },
    ChannelPreset {
        id: "vox",
        label: "Vox",
        group: "Vocals",
        blurb: "Low-cut 90 Hz, no gate, 3:1, low trim + presence + air. Starting point.",
        settings: VOX,
    },
    ChannelPreset {
        id: "keys",
        label: "Keys",
        group: "Instruments",
        blurb: "Low-cut 40 Hz, no gate, gentle 2:1, touch of presence. Starting point.",
        settings: KEYS,
    },
    ChannelPreset {
        id: "playback",
        label: "Playback",
        group: "Playback",
        blurb: "Protection low-cut only; dynamics off, EQ flat. Starting point.",
        settings: PLAYBACK,
    },
];

/// Look up a preset by id (`"kick"`, `"vox"`, …). Case-sensitive.
pub fn preset_by_id(id: &str) -> Option<&'static ChannelPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Apply a preset to one strip via the existing `set_param` path 
/// no driver changes. Bails on the first rejected field.
pub async fn apply_preset<D: MixerDriver + Sync + ?Sized>(
    driver: &D,
    strip: StripId,
    preset: &ChannelPreset,
) -> anyhow::Result<()> {
    for s in preset.settings {
        driver.set_param(strip, s.field, s.value.clone()).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_presets_unique_ids() {
        assert_eq!(PRESETS.len(), 10);
        let mut ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 10);
        for id in ["kick", "snare", "hat", "tom", "oh", "bass", "egtr", "vox", "keys", "playback"] {
            assert!(preset_by_id(id).is_some(), "missing {id}");
        }
        assert!(preset_by_id("nope").is_none());
    }

    #[test]
    fn values_finite_and_conservative() {
        for p in PRESETS {
            assert!(!p.settings.is_empty(), "{} has no settings", p.id);
            for s in p.settings {
                if let ParamValue::Float(v) = s.value {
                    assert!(v.is_finite(), "{}: {} not finite", p.id, s.field);
                }
            }
            // Boosts cap at +3 dB, cuts at -3 dB or deeper trim only.
            for s in p.settings {
                if !s.field.ends_with("_gain") {
                    continue;
                }
                if let ParamValue::Float(v) = s.value {
                    assert!(
                        (-20.0..=3.0).contains(&v),
                        "{}: {} gain {v} exceeds +3 dB cap",
                        p.id,
                        s.field
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn vox_has_lowcut_and_presence() {
        let vox = preset_by_id("vox").unwrap();
        let get = |f: &str| {
            vox.settings
                .iter()
                .find(|s| s.field == f)
                .unwrap_or_else(|| panic!("vox missing {f}"))
                .value
                .clone()
        };
        assert_eq!(get("hpf_on"), ParamValue::Bool(true));
        assert_eq!(get("hpf_freq"), ParamValue::Float(90.0));
        assert_eq!(get("eq2_gain"), ParamValue::Float(2.0));
        assert_eq!(get("eq2_freq"), ParamValue::Float(3000.0));
    }

    #[tokio::test]
    async fn kick_has_click() {
        let kick = preset_by_id("kick").unwrap();
        let gain4 = kick
            .settings
            .iter()
            .find(|s| s.field == "eq4_gain")
            .unwrap();
        assert_eq!(gain4.value, ParamValue::Float(3.0));
        let freq4 = kick
            .settings
            .iter()
            .find(|s| s.field == "eq4_freq")
            .unwrap();
        assert_eq!(freq4.value, ParamValue::Float(4000.0));
    }
}
