//! DCA / mute-group bitmask membership and mute-group masters.
//!
//! `{root}/grp/dca` is an 8-bit bitmask (DCA 1..8), `{root}/grp/mute`
//! a 6-bit bitmask (mute groups 1..6). Both are read-modify-write:
//! the desk has no per-group leaf. Masters live at
//! `/config/mute/1..6` (`1` = group muted).
use crate::{mapping::fam_of, X32Driver};
use anyhow::ensure;
use openfader_core::StripId;

fn dca_group(param: &str) -> anyhow::Result<u8> {
    param
        .strip_prefix("dca")
        .and_then(|s| s.strip_suffix("_assign"))
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|n| (1..=crate::addrs::DCA_GROUPS).contains(n))
        .ok_or_else(|| anyhow::anyhow!("x32: invalid DCA assignment"))
}

fn mg_group(param: &str) -> anyhow::Result<u8> {
    param
        .strip_prefix("_mg")
        .and_then(|s| s.parse::<u8>().ok())
        .filter(|n| (1..=crate::addrs::MUTE_GROUPS).contains(n))
        .ok_or_else(|| anyhow::anyhow!("x32: invalid mute-group assignment"))
}

fn mask_bit(mask: i32, group: u8) -> bool {
    mask & (1 << (group - 1)) != 0
}

fn mask_edit(mask: i32, group: u8, member: bool, bits: u32) -> i32 {
    let bit = 1 << (group - 1);
    let next = if member { mask | bit } else { mask & !bit };
    next & ((1 << bits) - 1)
}

impl X32Driver {
    async fn group_mask(&self, strip: StripId, leaf: &str) -> anyhow::Result<(String, i32)> {
        let (root, fam) = fam_of(strip)?;
        ensure!(!fam.is_dca, "x32: group assignment unavailable on DCA");
        let path = format!("{root}/{leaf}");
        let value = self.read_int(&path).await?;
        Ok((path, value))
    }

    pub async fn group_member(&self, strip: StripId, param: &str) -> anyhow::Result<bool> {
        if param.starts_with("dca") {
            let group = dca_group(param)?;
            let (_, mask) = self.group_mask(strip, "grp/dca").await?;
            anyhow::ensure!(
                (0..=255).contains(&mask),
                "x32: DCA mask {mask} outside legal 0..255 width"
            );
            Ok(mask_bit(mask, group))
        } else {
            let group = mg_group(param)?;
            let (_, mask) = self.group_mask(strip, "grp/mute").await?;
            anyhow::ensure!(
                (0..=63).contains(&mask),
                "x32: mute mask {mask} outside legal 0..63 width"
            );
            Ok(mask_bit(mask, group))
        }
    }

    pub async fn set_group_member(
        &self,
        strip: StripId,
        param: &str,
        member: bool,
    ) -> anyhow::Result<()> {
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        let (leaf, group, bits) = if param.starts_with("dca") {
            ("grp/dca", dca_group(param)?, 8)
        } else {
            ("grp/mute", mg_group(param)?, 6)
        };
        let (root, fam) = fam_of(strip)?;
        ensure!(!fam.is_dca, "x32: group assignment unavailable on DCA");
        let path = format!("{root}/{leaf}");
        let max: i32 = (1 << bits) - 1;
        let current = {
            let msg = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
            self.ensure_current(&session)?;
            crate::mapping::strict_int(&msg).map_err(|e| anyhow::anyhow!("x32: {e}"))?
        };
        anyhow::ensure!(
            (0..=max).contains(&current),
            "x32: group mask {current} outside legal 0..{max} width"
        );
        let desired = mask_edit(current, group, member, bits as u32);
        session
            .send(&path, vec![rosc::OscType::Int(desired)])
            .await?;
        let actual = {
            let msg = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
            self.ensure_current(&session)?;
            crate::mapping::strict_int(&msg).map_err(|e| anyhow::anyhow!("x32: {e}"))?
        };
        ensure!(
            actual == desired,
            "x32: group readback did not match the requested edit"
        );
        Ok(())
    }

    async fn mute_master(&self, group: u8) -> anyhow::Result<String> {
        ensure!(
            (1..=crate::addrs::MUTE_GROUPS).contains(&group),
            "x32: mute group out of range"
        );
        Ok(format!("/config/mute/{group}"))
    }

    pub async fn get_mute_master(&self, group: u8) -> anyhow::Result<bool> {
        let path = self.mute_master(group).await?;
        let session = self.session()?;
        let msg = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        crate::mapping::strict_bool(&msg).map_err(|e| anyhow::anyhow!("x32: {e}"))
    }

    pub async fn set_mute_master(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let path = self.mute_master(group).await?;
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session
            .send(&path, vec![rosc::OscType::Int(i32::from(muted))])
            .await?;
        let reply = session.query_timeout(&path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let actual =
            crate::mapping::strict_bool(&reply).map_err(|e| anyhow::anyhow!("x32: {e}"))?;
        anyhow::ensure!(
            actual == muted,
            "x32: mute-group readback did not match the requested write"
        );
        Ok(())
    }
}
