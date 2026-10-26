//! Mute-group membership and bank controls.
use super::*;

impl XAirDriver {
    /// Mute-group master mute (`/config/mute/N`, int). Polarity is
    /// INFERRED from the leaf name (`1` = group muted): only `0` has ever
    /// been observed live and the master has never been toggled on hardware
    /// (toggling could mute member channels audibly), so the `1` direction
    /// is UNVERIFIED-ON-RIG. Same-value writes round-trip bit-exact.
    pub(super) async fn driver_set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(muted))])
            .await
    }

    pub(super) async fn driver_get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        match reply.args.first() {
            Some(OscType::Int(i)) => Ok(*i != 0),
            other => anyhow::bail!("{addr}: mute group needs an int arg, got {other:?}"),
        }
    }

    /// Channel -> mute-group membership, bit `group` of the strip's
    /// `{root}/grp/mute` bitmask (rig-verified XR18 FW 1.20, 2026-10-20:
    /// write `1` → `1`, `2` → `2`, restore `0`; out-of-range writes
    /// saturate on the desk, so the group is validated locally first).
    /// Read-modify-write: the desk has no per-group leaf. Main-LR and DCA
    /// strips carry no `grp` leaf (live-silent) and bail locally instead
    /// of timing out on hardware — this also keeps the engine attach
    /// snapshot (which scans every strip) fast.
    pub(super) async fn driver_set_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        let (kind, root) = self.assign_root(ch.0, "mute-group assign")?;
        let _ = kind;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        let reply = session.query(&groups::grp_mute(&root)).await?;
        let mask = groups::mask_from_msg(&reply)?;
        let next = groups::mask_with(mask, group, member, "mute group")?;
        session
            .send(&groups::grp_mute(&root), vec![OscType::Int(next)])
            .await
    }

    /// Channel -> mute-group membership read (same leaf; groups with no
    /// desk leaf bail locally — see [`Self::set_mute_group_assign`]).
    pub(super) async fn driver_get_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
    ) -> anyhow::Result<bool> {
        let (kind, root) = self.assign_root(ch.0, "mute-group assign")?;
        let _ = kind;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(&groups::grp_mute(&root))
            .await?;
        let mask = groups::mask_from_msg(&reply)?;
        groups::mask_has(mask, group, "mute group")
    }
}
