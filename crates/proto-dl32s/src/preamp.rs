//! Routed physical-preamp context and guarded gain/phantom access.
use crate::{addrs, Dl32sDriver};
use crate::addrs::{dl32s_inputs, INPUT_COUNT};
use openfader_core::{ParamUpdate, ParamValue, StripId};
use openfader_transport::mackie::{MackieSession, PushEvent, VALUE_TYPE_PARAM};

// Routing is a display/accepted-intent context. The physical preamp words
// themselves still require desk evidence; own preamp writes remain pending.
pub(super) fn selected_preamp(session: &MackieSession, channel: u32) -> anyhow::Result<u32> {
    let base = dl32s_inputs().address(channel, "source_a")?;
    let [a, b, selected] = session.cached_range::<3>(base)
        .ok_or_else(|| anyhow::anyhow!("preamp routing context is unavailable"))?;
    let source = match selected { 1 => a, 0 => b, _ => anyhow::bail!("preamp source selection is unavailable") };
    anyhow::ensure!((1000..=1031).contains(&source), "preamp unavailable for selected input source");
    Ok(source - 999)
}

pub(super) fn routed_preamp_updates(session: &MackieSession, push: &PushEvent) -> Vec<ParamUpdate> {
    let PushEvent::Values { value_type: VALUE_TYPE_PARAM, start, raws } = push else { return Vec::new() };
    let touched = |addr: u32| addr >= *start && (addr - *start) < raws.len() as u32;
    let mut out = Vec::new();
    let received_at = std::time::Instant::now();
    for channel in 1..=INPUT_COUNT {
        let Ok(preamp) = selected_preamp(session, channel) else { continue };
        let Ok(source_base) = dl32s_inputs().address(channel, "source_a") else { continue };
        let route_changed = (0..3).any(|i| touched(source_base + i));
        for key in ["gain", "phantom"] {
            let Ok(addr) = dl32s_inputs().address(preamp, key) else { continue };
            if !route_changed && !touched(addr) { continue; }
            let Some(raw) = session.cached_verified(addr) else { continue };
            out.push(ParamUpdate { strip: StripId((channel - 1) as u16), param: key.to_string(),
                value: if key == "gain" { ParamValue::Float(f32::from_bits(raw)) } else { ParamValue::Bool(raw != 0) }, received_at });
        }
    }
    out
}

pub(super) fn affects_preamp_binding(push: &PushEvent) -> bool {
    let PushEvent::Values { value_type: VALUE_TYPE_PARAM, start, raws } = push else { return false };
    raws.iter().enumerate().any(|(i, _)| {
        let addr = *start + i as u32;
        (addrs::INPUT_BASE..addrs::INPUT_BASE + INPUT_COUNT * addrs::INPUT_STRIDE).contains(&addr)
            && matches!((addr - addrs::INPUT_BASE) % addrs::INPUT_STRIDE, 0..=2 | 12 | 13)
    })
}

impl Dl32sDriver {
    /// Display continuity for the routed physical preamp, including pending writes.
    pub async fn preamp_display(&self, strip: StripId, key: &str) -> anyhow::Result<ParamValue> {
        anyhow::ensure!(matches!(key, "gain" | "phantom"), "display projection requires a preamp control");
        self.get_field_mode(strip, key, false).await
    }

    pub async fn set_preamp_guarded(&self, strip: StripId, key: &str, value: ParamValue, expected: &std::collections::BTreeMap<String, ParamValue>) -> anyhow::Result<()> {
        anyhow::ensure!(matches!(key, "gain" | "phantom"), "guarded write requires a preamp control");
        self.set_field_checked(strip, key, value, Some(expected)).await
    }
}
