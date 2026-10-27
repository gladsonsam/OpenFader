//! Group membership, assignment labels, solo and stereo-link policy.
use crate::{addrs, input::decode_push_value, Dl32sDriver};
use crate::addrs::dl32s_inputs;
use openfader_core::{ParamValue, StripId};
use openfader_transport::mackie::{MUTE_GROUP_COUNT, VALUE_TYPE_PARAM};

// Type-2 input Solo: CH7=29 and CH8=30 independently verified against
// manufacturer mixer software, 2026-10-25. The contiguous input bank starts at CH1=23.
pub(super) const INPUT_SOLO_FIRST_ADDRESS: u32 = 23;

// Canonical assignment leaves map to provider membership words; group count
// is advertised by the actual leaf set rather than inferred by the frontend.
pub(super) const MEMBERSHIP_ALIASES: &[(&str, &str)] = &[
    ("dca1_assign", "vca1"),
    ("dca2_assign", "vca2"),
    ("dca3_assign", "vca3"),
    ("dca4_assign", "vca4"),
    ("dca5_assign", "vca5"),
    ("dca6_assign", "vca6"),
    ("view1_assign", "view1"),
    ("view2_assign", "view2"),
    ("view3_assign", "view3"),
    ("view4_assign", "view4"),
    ("view5_assign", "view5"),
    ("view6_assign", "view6"),
];

/// Native VCA1..6 names occupy slots58..63; View A..F uses70..75.
/// Both banks were matched read-only against manufacturer mixer software.
pub(super) fn assignment_name_parameter(slot: u32) -> Option<&'static str> {
    match slot {
        58 => Some("dca1_assign"),
        59 => Some("dca2_assign"),
        60 => Some("dca3_assign"),
        61 => Some("dca4_assign"),
        62 => Some("dca5_assign"),
        63 => Some("dca6_assign"),
        70 => Some("view1_assign"),
        71 => Some("view2_assign"),
        72 => Some("view3_assign"),
        73 => Some("view4_assign"),
        74 => Some("view5_assign"),
        75 => Some("view6_assign"),
        _ => None,
    }
}

pub(super) fn assignment_name_label(slot: u32, name: &str) -> Option<String> {
    if !name.is_empty() { return Some(name.to_string()); }
    // Canonical DCA rows use DCA labels; a known blank on this desk is
    // displayed as VCA N, matching its native naming convention.
    (58..=63).contains(&slot).then(|| format!("VCA {}", slot - 57))
}

impl Dl32sDriver {
    /// Desk-proven input Solo; pending own writes are withheld from polling.
    pub async fn get_solo_verified(&self, strip: StripId) -> anyhow::Result<bool> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .cached_type2_verified(address)
            .filter(|raw| *raw<=1)
            .map(|raw| raw != 0)
            .ok_or_else(|| anyhow::anyhow!("solo: no desk-confirmed value yet"))
    }

    /// Mute/unmute a mute group master (fixed protocol address, no table needed).
    pub async fn set_mute_group(&self, group_1_based: u32, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_mute_group(group_1_based, muted)
            .await
    }

    /// Mute-group master state for DISPLAY continuity (attach/snapshot/health):
    /// inclusive read — own-write-pending words included, no proof claimed.
    /// Verification paths must use [`get_mute_group_verified`](Self::get_mute_group_verified).
    pub async fn get_mute_group(&self, group_1_based: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_mute_group(group_1_based)
            .ok_or_else(|| anyhow::anyhow!("mute group {group_1_based}: no value pushed yet"))
    }

    /// Desk-proven mute-group master (withholds own-write-pending values).
    /// Verification/poll/readback paths must use this, never [`get_mute_group`](Self::get_mute_group).
    pub async fn get_mute_group_verified(&self, group_1_based: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_mute_group_verified(group_1_based)
            .ok_or_else(|| anyhow::anyhow!("mute group {group_1_based}: no desk-confirmed value yet"))
    }

    /// Desk-proven mute-group assign (withholds own-write-pending words).
    /// Verification paths must use this, never the inclusive trait getter.
    pub async fn get_mute_group_assign_verified(&self, ch: StripId, group_0_based: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group_0_based)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
        let v = self.verified_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("mute-group assign: no desk-confirmed value yet"))?;
        Ok(v != 0)
    }

    pub(super) fn dl_check_subgroup(group: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_subgroups().count;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: subgroup {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }

    pub(super) fn dl_check_mute_group(group: u8) -> anyhow::Result<u32> {
        let count = MUTE_GROUP_COUNT;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: mute group {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }
}
impl Dl32sDriver {
    pub(super) async fn assignments_control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
        if matches!(param, "link" | "stereo_link" | "source" | "source_a" | "source_b" | "source_select" | "trim" | "gain" | "phantom" | "usbreturn") {
            return Ok(vec![strip]);
        }
        let channel = Self::channel(strip)?;
        let left = if channel.is_multiple_of(2) { channel - 1 } else { channel };
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let address = dl32s_inputs().address(left, "stereo_link")?;
        let peer_address = dl32s_inputs().address(left+1, "stereo_link")?;
        let peer = session.cached(peer_address);
        let raw = session.cached(address)
            .ok_or_else(|| anyhow::anyhow!("Stereo link state is not confirmed; reopen the control."))?;
        anyhow::ensure!(raw <= 1, "Unknown stereo link state");
        // Own accepted writes are intentionally absent from the verified
        // cache. Keep both members protected while that state is pending.
        Ok(if raw == 1 || peer != Some(0) || session.cached_verified(address).is_none() || session.cached_verified(peer_address).is_none() {
            vec![StripId((left - 1) as u16), StripId(left as u16)]
        } else { vec![strip] })
    }

    pub(super) async fn assignments_set_stereo_link(&self, source: StripId, linked: bool)
        -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        let _write = self.output_write.lock().await;
        let channel = Self::channel(source)?;
        let left = if !channel.is_multiple_of(2) { channel } else { channel - 1 };
        let right = left + 1;
        let peer = if channel == left { right } else { left };
        let table = dl32s_inputs();
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let left_link = table.address(left, "stereo_link")?;
        let right_link = table.address(right, "stereo_link")?;
        // Use the inclusive snapshot so recent accepted edits can be
        // copied and link/unlink can repeat on a desk with no own-write
        // echo. Returned changes remain accepted, never readback proof.
        let current = session.cached(left_link)
            .ok_or_else(|| anyhow::anyhow!("Stereo link state is not available; reopen the control."))?;
        anyhow::ensure!(current <= 1, "Unknown stereo link state");
        let direction = session.cached(right_link)
            .ok_or_else(|| anyhow::anyhow!("Partner link state is not available; reopen the control."))?;
        anyhow::ensure!(direction <= 1, "Unknown stereo link direction");
        let mut changes = Vec::new();
        if linked && current == 0 {
            let mut writes = Vec::new();
            // Native copy excludes source A/B, selection, trim, gain and
            // phantom. Pan is the combined strip's balance, centered below.
            // Both directions and name ownership were independently probed.
            for offset in 0..addrs::INPUT_STRIDE {
                if matches!(offset, 0..=3 | 9 | 11..=13) { continue; }
                let from = addrs::INPUT_BASE + (channel - 1) * addrs::INPUT_STRIDE + offset;
                let to = addrs::INPUT_BASE + (peer - 1) * addrs::INPUT_STRIDE + offset;
                let value = session.cached(from)
                    .ok_or_else(|| anyhow::anyhow!("Source setting {offset} is not available; reopen the control."))?;
                let old = session.cached(to)
                    .ok_or_else(|| anyhow::anyhow!("Partner setting {offset} is not available; reopen the control."))?;
                if value != old { writes.push((to, value)); }
            }
            let name = session.name_slot(channel)
                .ok_or_else(|| anyhow::anyhow!("Source name is not available; reopen the control."))?;
            let old_name = session.name_slot(peer)
                .ok_or_else(|| anyhow::anyhow!("Partner name is not available; reopen the control."))?;
            let balance = table.address(left, "pan")?;
            session.cached(balance)
                .ok_or_else(|| anyhow::anyhow!("Balance is not available; reopen the control."))?;
            session.cached(right_link)
                .ok_or_else(|| anyhow::anyhow!("Partner link state is not available; reopen the control."))?;
            writes.push((balance, 0.0f32.to_bits()));
            writes.push((right_link, u32::from(channel == right)));
            writes.push((left_link, 1));
            // Validate the entire capture before the first mutation.
            if name != old_name {
                session.set_channel_name(peer, &name).await?;
                changes.push((StripId((peer - 1) as u16), "name".to_string(), ParamValue::Text(name)));
            }
            // DL32S firmware can acknowledge multi-address frames without
            // applying every pair. Use the established single-write path;
            // activate the pair only after copying its shared settings.
            for &(address, raw) in &writes {
                session.write_int(address, raw, VALUE_TYPE_PARAM).await?;
            }
            for (address, raw) in writes {
                if address == left_link || address == right_link { continue; }
                changes.extend(decode_push_value(address, raw));
            }
        } else if !linked && current != 0 {
            session.write_int(left_link, 0, VALUE_TYPE_PARAM).await?;
            session.write_int(right_link, 0, VALUE_TYPE_PARAM).await?;
        }
        for member in [left, right] {
            changes.push((StripId((member - 1) as u16), "link".to_string(), ParamValue::Bool(linked)));
        }
        Ok(changes)
    }
}
