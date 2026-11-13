use crate::addrs::INPUT_COUNT;
use crate::assignments::INPUT_SOLO_FIRST_ADDRESS;
use crate::input::{canonical_for_table_key, decode_colour};
use crate::{addrs, DL32S_CAPABILITIES};
use openfader_core::{ParamUpdate, ParamValue, StripId};
use openfader_transport::mackie::{
    PushEvent, MUTE_GROUP_BASE_ADDRESS, MUTE_GROUP_COUNT, VALUE_TYPE_MUTE_GROUP, VALUE_TYPE_PARAM,
};

pub(super) fn decode_push_value(addr: u32, raw: u32) -> Vec<(StripId, String, ParamValue)> {
    let Some((first, table)) = crate::strips::topology().iter().find_map(|family| {
        let table = crate::strips::resolve(StripId(family.first as u16)).ok()?.0;
        (table.base..table.base + table.stride * table.count)
            .contains(&addr)
            .then_some((family.first, table))
    }) else {
        return Vec::new();
    };
    let ch0 = (first + ((addr - table.base) / table.stride) as usize) as u16;
    let off = (addr - table.base) % table.stride;
    let Some(field) = table.fields.iter().find(|f| f.offset == off) else {
        return Vec::new();
    };
    let strip = StripId(ch0);
    let f = f32::from_bits(raw);
    let mut out: Vec<(StripId, String, ParamValue)> = Vec::new();
    let mut push = |param: String, value: ParamValue| {
        out.push((strip, param, value));
    };
    match field.key {
        // Physical preamp addresses need routing-aware fanout, not ch=socket.
        "gain" | "phantom" => return out,
        "fader" => push(
            "fader".to_string(),
            ParamValue::Float(addrs::DL32S_FADER_SCALE.inverse(f)),
        ),
        "mute" => push("mute".to_string(), ParamValue::Bool(raw != 0)),
        "pan" => push("pan".to_string(), ParamValue::Float(f * 100.0)),
        "stereo_link" => {
            if ch0.is_multiple_of(2) {
                return vec![
                    (strip, "link".to_string(), ParamValue::Bool(raw != 0)),
                    (
                        StripId(ch0 + 1),
                        "link".to_string(),
                        ParamValue::Bool(raw != 0),
                    ),
                ];
            }
            return out;
        }
        // Native None is u32::MAX, which cannot round-trip through f32.
        "colour" => push("color".to_string(), ParamValue::Float(decode_colour(raw))),
        key => {
            for (prefix, event) in [("mutegroup", "_mg"), ("sub", "_sg")] {
                if let Some(group) = key.strip_prefix(prefix).and_then(|s| s.parse::<u8>().ok()) {
                    if raw <= 1 {
                        push(format!("{event}{group}"), ParamValue::Bool(raw != 0));
                    }
                    return out;
                }
            }
            // Pure renames ride the canonical id.
            if let Some(canonical) = canonical_for_table_key(key) {
                let v = match field.encoding {
                    addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                    _ => ParamValue::Float(f),
                };
                push(canonical.to_string(), v);
                return out;
            }
            // Legacy + FX send leaves ride twice (native id and generic
            // slot), exactly like the poller reads them. Send pans convert
            // wire -1.0..+1.0 to canonical -100..+100.
            if let Some((slot, leaf)) = send_slot_for_table_key(key) {
                let val = if leaf == "pan" { f * 100.0 } else { f };
                push(format!("send{slot}_{leaf}"), ParamValue::Float(val));
            }
            if !DL32S_CAPABILITIES.contains(&key) {
                return out;
            }
            let value = match field.encoding {
                addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                addrs::Encoding::Db | addrs::Encoding::Float => ParamValue::Float(f),
                addrs::Encoding::Enum | addrs::Encoding::Int => ParamValue::Float(raw as f32),
            };
            push(key.to_string(), value);
        }
    }
    out
}

pub(super) fn send_slot_for_table_key(key: &str) -> Option<(u32, &'static str)> {
    if let Some(rest) = key.strip_prefix("aux") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=8).contains(&n) {
            return None;
        }
        return match leaf {
            "level" => Some((n, "level")),
            "pan" => Some((n, "pan")),
            _ => None,
        };
    }
    if let Some(rest) = key.strip_prefix("fx") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=4).contains(&n) || leaf != "level" {
            return None;
        }
        return Some((n + 8, "level"));
    }
    None
}

pub(super) fn decode_push(push: PushEvent) -> Vec<ParamUpdate> {
    let received_at = std::time::Instant::now();
    match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } => {
            // Mute-group masters ride their own address space (type 2):
            // master for 1-based group g lives at
            // MUTE_GROUP_BASE_ADDRESS + g. The engine already routes
            // `_mute_group` ingest updates to MuteGroup events (same
            // strip convention as `get_mute_group`), so another client's
            // master change converges live instead of waiting for the
            // next masters refresh.
            if value_type == VALUE_TYPE_MUTE_GROUP {
                let mut out = Vec::new();
                for (i, raw) in raws.iter().enumerate() {
                    let address = start + i as u32;
                    let input0 = address.wrapping_sub(INPUT_SOLO_FIRST_ADDRESS);
                    if input0 < INPUT_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(input0 as u16),
                            param: "solo".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                    let group0 = address.wrapping_sub(MUTE_GROUP_BASE_ADDRESS + 1);
                    if group0 < MUTE_GROUP_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(group0 as u16),
                            param: "_mute_group".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                }
                return out;
            }
            if value_type != VALUE_TYPE_PARAM {
                return Vec::new();
            }
            let mut out = Vec::new();
            for (i, raw) in raws.iter().enumerate() {
                let address = start + i as u32;
                let changes = crate::push_masters::decode(address, *raw)
                    .into_iter()
                    .chain(decode_push_value(address, *raw));
                for (strip, param, value) in changes {
                    out.push(ParamUpdate {
                        received_at,
                        strip,
                        param,
                        value,
                    });
                }
            }
            out
        }
        PushEvent::Names { pairs } => pairs
            .into_iter()
            .filter_map(|(slot, name)| {
                let group = (0..MUTE_GROUP_COUNT).find(|group| {
                    openfader_transport::mackie::mute_group_name_slot(group + 1).ok() == Some(slot)
                });
                let (index, param) = if let Some(group) = group {
                    (group as u16, "_mute_group_name")
                } else {
                    let strip = (0..crate::strips::STRIP_COUNT).find(|i| {
                        crate::strips::name_slot(StripId(*i as u16)).ok() == Some(slot)
                    })?;
                    (strip as u16, "name")
                };
                Some(ParamUpdate {
                    received_at,
                    strip: StripId(index),
                    param: param.to_string(),
                    value: ParamValue::Text(name),
                })
            })
            .collect(),
        PushEvent::SnapshotRecall(_) | PushEvent::SnapshotRename { .. } => Vec::new(),
    }
}
