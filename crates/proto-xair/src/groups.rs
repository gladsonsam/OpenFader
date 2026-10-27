//! X-Air per-strip group assignment (`{root}/grp/dca`, `{root}/grp/mute`).
//!
//! Doc sources (address shapes):
//!
//! - Behringer XR18 firmware OSC map, FW 1.14 (xr-meters `parameters.txt`):
//!   every channel/bus strip carries a `grp/` node with `dca` + `mute`
//!   leaves; the XR18 protocol notes from native client 1.8.1 research
//!   (plus its live full-plane capture (2026-10-19)) show
//!   `/ch/NN/grp/dca`, `/ch/NN/grp/mute` (and the `/bus/N`, `/rtn/N`,
//!   `/rtn/aux`, `/fxsend/N` parallels) answering int `0`, with the
//!   two-int `/ch/NN/grp` container reading `(dca, mute)`.
//!
//! Owner-rig verified 2026-10-20 (XR18 FW 1.20, ch16 muted/unused, every
//! write restored): both leaves are 4-bit bitmasks, bit `n - 1` = group
//! `n`. `grp/dca` write `1` → `1`, `5` → `5`, `255` → `15`, restore `0`
//! (DCA 1..4). `grp/mute` write `1` → `1`, `2` → `2`, restore `0` —
//! out-of-range writes (`32`, `63`) saturate to `15`, so the driver
//! validates group indices locally and never sends upper bits (the desk
//! has 4 mute groups on this firmware: `/config/mute/5..8` are silent
//! and only the low 4 bits stick).
//!
//! `/lr/grp/*` and `/dca/N/grp/*` are live-silent (no leaf) — the driver
//! bails locally for main/DCA strips instead of timing out on hardware.

/// DCA group count (bits 0..=3 of `{root}/grp/dca`).
pub const DCA_GROUPS: u8 = 4;
/// Mute group count (bits 0..=3 of `{root}/grp/mute`, matching the four
/// `/config/mute/N` masters).
pub const MUTE_GROUPS: u8 = 4;
/// Bitmask covering the valid group bits (both leaves).
pub const GROUP_MASK: i32 = 0x0F;

/// ```
/// # use openfader_proto_xair::groups::grp_dca;
/// assert_eq!(grp_dca("/ch/01"), "/ch/01/grp/dca");
/// assert_eq!(grp_dca("/bus/1"), "/bus/1/grp/dca");
/// ```
pub fn grp_dca(root: &str) -> String {
    format!("{root}/grp/dca")
}

/// ```
/// # use openfader_proto_xair::groups::grp_mute;
/// assert_eq!(grp_mute("/ch/01"), "/ch/01/grp/mute");
/// assert_eq!(grp_mute("/rtn/aux"), "/rtn/aux/grp/mute");
/// ```
pub fn grp_mute(root: &str) -> String {
    format!("{root}/grp/mute")
}

/// Rejects out-of-range groups locally: the desk saturates them to
/// all-bits, which would (mis)assign every group.
pub fn mask_has(mask: i32, group: u8, what: &str) -> anyhow::Result<bool> {
    check_group(group, what)?;
    Ok(mask & (1 << group) != 0)
}

/// Read-modify-write (the desk has no per-group leaf); result masked to
/// [`GROUP_MASK`] so upper bits never reach the desk.
pub fn mask_with(mask: i32, group: u8, member: bool, what: &str) -> anyhow::Result<i32> {
    check_group(group, what)?;
    let bit = 1 << group;
    Ok(if member { mask | bit } else { mask & !bit } & GROUP_MASK)
}

fn check_group(group: u8, what: &str) -> anyhow::Result<()> {
    if group >= DCA_GROUPS {
        anyhow::bail!("{what} group {group} out of range 0..{DCA_GROUPS} (0-based)");
    }
    Ok(())
}

pub fn mask_from_msg(msg: &rosc::OscMessage) -> anyhow::Result<i32> {
    match msg.args.first() {
        Some(rosc::OscType::Int(i)) => Ok(*i),
        other => anyhow::bail!("{}: grp mask needs an int arg, got {other:?}", msg.addr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_shapes() {
        assert_eq!(grp_dca("/ch/01"), "/ch/01/grp/dca");
        assert_eq!(grp_dca("/ch/16"), "/ch/16/grp/dca");
        assert_eq!(grp_dca("/bus/1"), "/bus/1/grp/dca");
        assert_eq!(grp_mute("/ch/01"), "/ch/01/grp/mute");
        assert_eq!(grp_mute("/rtn/aux"), "/rtn/aux/grp/mute");
    }

    #[test]
    fn mask_bits_match_desk() {
        assert_eq!(mask_with(0, 0, true, "dca").unwrap(), 1);
        assert_eq!(mask_with(1, 2, true, "dca").unwrap(), 5);
        assert_eq!(mask_with(0, 0, false, "dca").unwrap(), 0);
        assert!(mask_has(5, 0, "dca").unwrap());
        assert!(!mask_has(5, 1, "dca").unwrap());
        assert!(mask_has(5, 2, "dca").unwrap());
        // Upper bits never escape, even from a saturated read-back.
        assert_eq!(mask_with(0xFF, 0, false, "mute").unwrap(), 0x0E);
        // Out-of-range groups bail locally (the desk would saturate).
        assert!(mask_with(0, 4, true, "mute").is_err());
        assert!(mask_has(15, 6, "dca").is_err());
    }
}
