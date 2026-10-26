//! FX slot queries and commands.
use super::*;

impl XAirDriver {
    /// Single-query FX param read when the loaded effect index is already
    /// known (connect burst / poller fast path): skips the per-param
    /// `/fx/N/type` re-query in the trait `get_fx_param`. `None`
    /// (unknown/unreadable type) falls back to raw decode, exactly like
    /// the two-query path.
    pub async fn get_fx_param_with_type(
        &self,
        slot: u8,
        param: u8,
        effect: Option<u8>,
    ) -> anyhow::Result<f32> {
        let addr = fx::fx_param(slot, param)?;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        let def = effect.and_then(|t| fx::fx_param_def(t, param + 1));
        let reply = session.query(&addr).await?;
        fx::parse_fx_param(&reply.args, def.as_ref())
    }

    /// Loaded effect-type index for one FX slot (`/fx/N/type`, int
    /// `0..=60`; see [`crate::fx`]). The type-select write path is
    /// rig-verified (owner XR18 FW 1.20, 2026-10-19: slot 4 swept
    /// 60→0/7/26/59/30/56→60 round-trip) — see [`Self::set_fx_type`].
    /// Out-of-range slots bail locally, never hitting the desk.
    pub async fn get_fx_type(&self, slot: u8) -> anyhow::Result<u8> {
        let addr = fx::fx_type(slot)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        fx::parse_fx_type(&reply.args)
    }

    /// Select the loaded effect for one FX slot (`/fx/N/type`, int
    /// `0..=60`): 61 firmware types, one more than the app table's 0..=59.
    /// Rig-verified on the owner XR18 (FW 1.20, 2026-10-19 — slot-4 sweep,
    /// restored bit-exact). Type swaps are state-changing on a live desk;
    /// loopback-only callers treat this like any other verified write.
    /// Out-of-range slots/types bail locally, never hitting the desk.
    pub async fn set_fx_type(&self, slot: u8, effect: u8) -> anyhow::Result<()> {
        let addr = fx::fx_type(slot)?;
        let arg = fx::fx_type_to_arg(effect)?;
        let guard = self.session().await?;
        guard.as_ref().unwrap().send(&addr, vec![arg]).await
    }

    /// FX slot insert switch (`/fx/N/insert`, int `1` = inserted).
    /// Rig-verified answering on the owner XR18 (FW 1.20, 2026-10-19:
    /// `/fx/4/insert` = 1 matches its container). Out-of-range slots bail
    /// locally, never hitting the desk.
    pub async fn get_fx_insert(&self, slot: u8) -> anyhow::Result<bool> {
        let addr = fx::fx_insert(slot)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::is_on(&reply)
    }

    /// Insert/bypass one FX slot. Same verified leaf as
    /// [`Self::get_fx_insert`]; out-of-range slots bail locally.
    pub async fn set_fx_insert(&self, slot: u8, inserted: bool) -> anyhow::Result<()> {
        let addr = fx::fx_insert(slot)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![codec::bool_arg(inserted)])
            .await
    }

    pub(super) async fn driver_set_fx_param(
        &self,
        slot: u8,
        param: u8,
        value: f32,
    ) -> anyhow::Result<()> {
        // 0-based trait index; the wire is 1-based on both axes (`fx`).
        // Validated BEFORE any I/O so out-of-range never hits the desk.
        // Bit-exact on hardware (baseline §6 row 5) — no fader epsilon here.
        let addr = fx::fx_param(slot, param)?;
        let value = fx::check_value(value)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Float(value)])
            .await
    }

    pub(super) async fn driver_get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        // Loaded effect selects the par def (enum pars reply as INT option
        // indices on hardware — see `crate::fx`). A missing/unparseable
        // type falls back to raw decode (`None` def); plain floats stay
        // bit-exact either way (baseline §6 row 5).
        let effect = match self
            .session()
            .await?
            .as_ref()
            .unwrap()
            .query(&fx::fx_type(slot)?)
            .await
        {
            Ok(treply) => fx::parse_fx_type(&treply.args).ok(),
            Err(_) => None,
        };
        self.get_fx_param_with_type(slot, param, effect).await
    }
}
