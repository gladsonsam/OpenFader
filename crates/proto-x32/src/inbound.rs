//! Desk-originated push parsing: `/xremote` echoes changes addressed
//! as the param itself. Only unambiguous mix-surface leaves map here;
//! everything else stays poll-only. Values decode to engineering units,
//! the same contract as `get_param`.
use crate::{
    addrs, codec,
    mapping::{fam_of, Mapping, WireKind},
};
use openfader_core::{ParamValue, StripId};
use rosc::OscMessage;

/// Parse one desk-originated message into `(strip, param, value)` rows.
pub fn parse_inbound(msg: &OscMessage) -> Vec<(StripId, String, ParamValue)> {
    // Solo bank leaves are strip-addressed 1:1.
    if let Some(rest) = msg.addr.strip_prefix("/-stat/solosw/") {
        if let Ok(index) = rest.parse::<u8>() {
            if (1..=80).contains(&index) {
                // Strict provider-owned decode: exactly one arg, 0/1 only.
                // NaN, 2, fractional floats and multi-arg replies are
                // malformed and ignored, never served as solo state.
                if let Ok(soloed) = crate::mapping::strict_bool(msg) {
                    if let Some(strip) = solo_index_to_strip(index) {
                        return vec![(strip, "solo".into(), ParamValue::Bool(soloed))];
                    }
                }
            }
        }
        return Vec::new();
    }
    // Mute-group masters.
    if let Some(rest) = msg.addr.strip_prefix("/config/mute/") {
        if let Ok(group) = rest.parse::<u8>() {
            if (1..=addrs::MUTE_GROUPS).contains(&group) {
                if let Ok(on) = crate::mapping::strict_bool(msg) {
                    return vec![(
                        StripId(group as u16 - 1),
                        "_mute_group".into(),
                        ParamValue::Bool(on),
                    )];
                }
            }
        }
        return Vec::new();
    }
    // Strip mix leaves: find the owning strip by root prefix.
    for index in 0..addrs::X32_STRIPS as u16 {
        let strip = StripId(index);
        let Ok((root, _)) = fam_of(strip) else {
            continue;
        };
        let Some(leaf) = msg.addr.strip_prefix(root.as_str()) else {
            continue;
        };
        let leaf = leaf.strip_prefix('/').unwrap_or(leaf);
        let Some(param) = mix_leaf_param(leaf) else {
            continue;
        };
        let mapping = Mapping {
            path: msg.addr.clone(),
            kind: param,
        };
        if let Ok(value) = mapping.decode(msg) {
            // Group bitmask leaves decode to whole-mask floats; the
            // engine re-derives members on poll, so pushes stay silent.
            if leaf == "grp/dca" || leaf == "grp/mute" {
                return Vec::new();
            }
            // Desk `mix/on` ON = audible; canonical `mute` is inverted.
            if inbound_param_name(leaf) == "mute" {
                if let ParamValue::Bool(on) = value {
                    return vec![(strip, "mute".into(), ParamValue::Bool(!on))];
                }
                return Vec::new();
            }
            return vec![(strip, inbound_param_name(leaf).into(), value)];
        }
        return Vec::new();
    }
    Vec::new()
}

/// Canonical param name for a served mix leaf.
fn inbound_param_name(leaf: &str) -> &str {
    match leaf {
        "mix/fader" | "fader" => "fader",
        "mix/on" | "on" => "mute",
        "mix/pan" => "pan",
        "config/name" => "name",
        _ => "",
    }
}

/// Wire kind for a served mix leaf (`None` = stays poll-only).
fn mix_leaf_param(leaf: &str) -> Option<WireKind> {
    match leaf {
        "mix/fader" | "fader" => Some(WireKind::Raw01),
        "mix/on" | "on" => Some(WireKind::Bool),
        "mix/pan" => Some(WireKind::Linf(-100.0, 100.0)),
        "config/name" => Some(WireKind::Text(12)),
        _ => None,
    }
}

/// `/-stat/solosw` 1-based index → strip.
fn solo_index_to_strip(index: u8) -> Option<StripId> {
    let strip = match index {
        1..=32 => index - 1,
        33..=40 => 32 + (index - 33),
        41..=48 => 40 + (index - 41),
        49..=64 => 48 + (index - 49),
        65..=70 => 64 + (index - 65),
        71 => 70,
        72 => 71,
        73..=80 => 72 + (index - 73),
        _ => return None,
    };
    Some(StripId(strip as u16))
}

/// fader-law decode shared with the driver (kept here so the push
/// plane and the query plane cannot drift apart).
pub fn _decode_level(wire: f32) -> anyhow::Result<f32> {
    codec::fader_db(wire)
}
