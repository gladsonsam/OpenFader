//! Desk-originated push parsing. The receive plane uses the same canonical
//! mappings and engineering-unit codecs as queried values, including processing,
//! sends and group membership. Unsupported family/leaf combinations never enter
//! the map, so an external edit cannot make an unavailable control appear valid.
use crate::{
    addrs, codec,
    mapping::{mapping, Mapping, WireKind},
    presentation,
};
use openfader_core::{ParamValue, StripId};
use rosc::OscMessage;
use std::{collections::HashMap, sync::OnceLock};

struct InboundLeaf {
    strip: StripId,
    param: &'static str,
    mapping: Mapping,
}

/// Build once from the provider's actual mapping surface. Many group controls
/// deliberately share one native mask leaf, so each address owns a list rather
/// than silently retaining only the final DCA or mute-group membership.
fn mapped_leaves() -> &'static HashMap<String, Vec<InboundLeaf>> {
    static LEAVES: OnceLock<HashMap<String, Vec<InboundLeaf>>> = OnceLock::new();
    LEAVES.get_or_init(|| {
        let mut leaves: HashMap<String, Vec<InboundLeaf>> = HashMap::new();
        for index in 0..addrs::X32_STRIPS as u16 {
            let strip = StripId(index);
            for &param in presentation::capabilities() {
                let Ok(mapping) = mapping(strip, param) else {
                    continue;
                };
                leaves
                    .entry(mapping.path.clone())
                    .or_default()
                    .push(InboundLeaf {
                        strip,
                        param,
                        mapping,
                    });
            }
            // Source is a computed label in get_param, not a generic mapping.
            // Only channels and aux inputs have the native source selector.
            if index < 40 {
                let (root, _) = addrs::root(strip).expect("registered X32 strip");
                let mapping = Mapping {
                    path: format!("{root}/config/source"),
                    kind: WireKind::Int(0, 64),
                };
                leaves
                    .entry(mapping.path.clone())
                    .or_default()
                    .push(InboundLeaf {
                        strip,
                        param: "source",
                        mapping,
                    });
            }
        }
        leaves
    })
}

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
    let Some(leaves) = mapped_leaves().get(&msg.addr) else {
        return Vec::new();
    };
    leaves
        .iter()
        .filter_map(|leaf| {
            let value = leaf.mapping.decode(msg).ok()?;
            let value = match (leaf.param, value) {
                // Desk mix/on ON = audible; canonical mute is inverted.
                ("mute", ParamValue::Bool(on)) => ParamValue::Bool(!on),
                ("source", ParamValue::Float(index)) => {
                    ParamValue::Text(addrs::source_label(index as i32).ok()?)
                }
                (param, ParamValue::Float(mask)) if param.starts_with("dca") => {
                    let group = param
                        .strip_prefix("dca")?
                        .strip_suffix("_assign")?
                        .parse::<u8>()
                        .ok()?;
                    ParamValue::Bool(mask as i32 & (1 << (group - 1)) != 0)
                }
                (param, ParamValue::Float(mask)) if param.starts_with("_mg") => {
                    let group = param.strip_prefix("_mg")?.parse::<u8>().ok()?;
                    ParamValue::Bool(mask as i32 & (1 << (group - 1)) != 0)
                }
                (_, value) => value,
            };
            Some((leaf.strip, leaf.param.to_string(), value))
        })
        .collect()
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
