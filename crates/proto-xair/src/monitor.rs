//! Solo bank and monitor controls.
use super::*;

impl XAirDriver {
    /// Clear all solos (`/-action/clearsolo`, fire-and-forget int `1` =
    /// button press). Monitoring-side only — main LR untouched. The desk
    /// answers queries on this leaf with int `0` (nothing soloed); the
    /// trigger semantic itself is UNVERIFIED-ON-RIG (no solo was active to
    /// clear during the single live fire, 2026-10-18).
    pub async fn clear_solo(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::CLEAR_SOLO, vec![OscType::Int(1)])
            .await
    }

    /// Live solo bank in ONE query (`/-stat/solosw` int args, 1 per
    /// channel): index `i` set = channel `i + 1` soloed. Backs the engine
    /// snapshot fan-out so a connect burst costs 1 query, not 16.
    pub async fn solo_bank(&self) -> anyhow::Result<Vec<bool>> {
        let n = self.model.channels() as usize;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::SOLO_SW).await?;
        Ok((0..n)
            .map(|i| matches!(reply.args.get(i), Some(OscType::Int(v)) if *v != 0))
            .collect())
    }

    /// Solo monitoring globals (`/config/solo/*`, see [`crate::solo`]).
    /// Rig-verified on the owner XR18 (FW 1.20, 2026-10-19 — dim/mute/level
    /// flips round-trip bit-exact, source reads 3). Monitoring-side only:
    /// main LR untouched. These are globals, not strip params (out of
    /// strip caps, never polled).
    pub async fn get_solo_dim(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_DIM).await?;
        osc::is_on(&reply)
    }

    /// Solo dim switch. Same verified leaf as [`Self::get_solo_dim`].
    pub async fn set_solo_dim(&self, dim: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_DIM, vec![codec::bool_arg(dim)])
            .await
    }

    /// Solo mute switch (`/config/solo/mute`). Same verification as
    /// [`Self::get_solo_dim`].
    pub async fn get_solo_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_MUTE).await?;
        osc::is_on(&reply)
    }

    /// Solo mute switch. Same verified leaf as [`Self::get_solo_mute`].
    pub async fn set_solo_mute(&self, mute: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_MUTE, vec![codec::bool_arg(mute)])
            .await
    }

    /// Solo headphone level (`/config/solo/level`, wire float `0..1`).
    /// Same verification as [`Self::get_solo_dim`].
    pub async fn get_solo_level(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_LEVEL).await?;
        solo::wire_to_level(osc::fader_value(&reply)?)
    }

    /// Solo headphone level. Validated locally (`0..=1`) before any I/O.
    pub async fn set_solo_level(&self, level: f32) -> anyhow::Result<()> {
        let wire = solo::level_to_wire(level)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_LEVEL, vec![OscType::Float(wire)])
            .await
    }

    /// Solo headphone source (`/config/solo/source`, int `0..=14`).
    /// Same verification as [`Self::get_solo_dim`].
    pub async fn get_solo_source(&self) -> anyhow::Result<u8> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_SOURCE).await?;
        solo::source_from_msg(&reply)
    }

    /// Solo headphone source. Validated locally (`0..=14`) before any I/O.
    pub async fn set_solo_source(&self, source: u8) -> anyhow::Result<()> {
        let arg = solo::source_to_arg(source)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_SOURCE, vec![arg])
            .await
    }

    /// Per-channel solo via the `/-stat/solosw` switch bank: live-verified
    /// on the owner rig (XR18 FW 1.20, 2026-10-18) — writing `1` to the
    /// 1-based `/-stat/solosw/NN` sets bank bit `NN - 1` (soloing ch16 set
    /// bits 14+15: ch15/16 are stereo-linked on that desk), and
    /// `/-action/clearsolo` clears the bank. Only channel strips
    /// (`0..16`) map (1-based index `strip + 1`); buses/mains/DCAs have no
    /// verified solo leaf and bail out (never faked).
    pub(super) async fn driver_set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let ch = self.check_channel(strip.0)?;
        let addr = format!("{}/{ch:02}", addrs::SOLO_SW);
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(soloed))])
            .await
    }

    pub(super) async fn driver_get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::SOLO_SW).await?;
        let idx = ch as usize - 1;
        match reply.args.get(idx) {
            Some(OscType::Int(i)) => Ok(*i != 0),
            other => {
                anyhow::bail!("/-stat/solosw: solo bank needs an int at index {idx}, got {other:?}")
            }
        }
    }
}
