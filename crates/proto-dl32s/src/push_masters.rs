use crate::addrs;
use openfader_core::{ParamValue, StripId};

pub(super) fn decode(address: u32, raw: u32) -> Option<(StripId, String, ParamValue)> {
    for (family, table) in [
        ("main", addrs::dl32s_main()),
        ("bus", addrs::dl32s_auxmasters()),
        ("subgroup", addrs::dl32s_subgroups()),
    ] {
        let offset = address.checked_sub(table.base);
        let Some(offset) = offset.filter(|offset| *offset < table.stride * table.count) else {
            continue;
        };
        let field = table
            .fields
            .iter()
            .find(|field| field.offset == offset % table.stride)?;
        let value = match field.key {
            "fader" => {
                let db = f32::from_bits(raw);
                if !db.is_finite() {
                    return None;
                }
                ParamValue::Float(addrs::DL32S_FADER_SCALE.inverse(db))
            }
            "mute" if raw <= 1 => ParamValue::Bool(raw != 0),
            _ => return None,
        };
        return Some((
            StripId((offset / table.stride) as u16),
            format!("_{family}_{}", field.key),
            value,
        ));
    }
    None
}
