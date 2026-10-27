//! Provider-owned strip identity and schema resolution. Input indices stay stable.
//! Return is mono/linkable; FX return is intrinsically stereo; VCA has no audio DSP.
use crate::{
    addrs::{self, ChannelBlock},
    input::{alias_target, send_table_key},
    DL32S_CAPABILITIES,
};
use openfader_core::{StripFamily, StripId, StripKind};

pub(crate) const STRIP_COUNT: usize = 44;
pub(crate) fn topology() -> Vec<StripFamily> {
    vec![
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
            key: "ret",
            kind: StripKind::Ret,
            label: "Returns",
            first: 32,
            count: 2,
            name_prefix: "Return",
            color_default: 2,
        },
        StripFamily {
            key: "fxret",
            kind: StripKind::Custom("fxret"),
            label: "FX returns",
            first: 34,
            count: 4,
            name_prefix: "FX Return",
            color_default: 15,
        },
        StripFamily {
            key: "dca",
            kind: StripKind::Dca,
            label: "VCA 1-6",
            first: 38,
            count: 6,
            name_prefix: "VCA",
            color_default: 8,
        },
    ]
}

pub(crate) fn resolve(strip: StripId) -> anyhow::Result<(&'static ChannelBlock, u32)> {
    Ok(match strip.0 {
        0..=31 => (addrs::dl32s_inputs(), u32::from(strip.0) + 1),
        32..=33 => (addrs::dl32s_returns(), u32::from(strip.0) - 31),
        34..=37 => (addrs::dl32s_fxreturns(), u32::from(strip.0) - 33),
        38..=43 => (addrs::dl32s_vcas(), u32::from(strip.0) - 37),
        _ => anyhow::bail!("DL32S strip identity {} is unavailable", strip.0),
    })
}
pub(crate) fn name_slot(strip: StripId) -> anyhow::Result<u32> {
    let (_, n) = resolve(strip)?;
    // DLF32ParameterModel::{input,ipadin,fxreturn,vca}Name getters.
    Ok(match strip.0 {
        0..=31 => n,
        32..=33 => 32 + n,
        34..=37 => 38 + n,
        _ => 57 + n,
    })
}
pub(crate) fn native_key(param: &str) -> String {
    alias_target(param).unwrap_or(param).to_string()
}
pub(crate) fn supports(table: &ChannelBlock, param: &str) -> bool {
    (table.name == "return" && matches!(param, "gain" | "phantom"))
        || param == "name"
        || table
            .field(&send_table_key(param).unwrap_or_else(|| native_key(param)))
            .is_ok()
}

/// Scopes are derived from the provider's actual schemas, including canonical
/// aliases. No screen knows a DL32S family or assumes input capabilities apply.
pub fn scopes() -> Vec<(&'static str, &'static [StripKind])> {
    static SCOPES: std::sync::OnceLock<Vec<(&'static str, &'static [StripKind])>> =
        std::sync::OnceLock::new();
    SCOPES
        .get_or_init(|| {
            DL32S_CAPABILITIES
                .iter()
                .map(|&id| {
                    let kinds: Vec<_> = topology()
                        .iter()
                        .filter(|f| {
                            supports(
                                resolve(StripId(f.first as u16)).expect("static schema").0,
                                id,
                            )
                        })
                        .map(|f| f.kind)
                        .collect();
                    (id, &*Box::leak(kinds.into_boxed_slice()))
                })
                .collect()
        })
        .clone()
}
