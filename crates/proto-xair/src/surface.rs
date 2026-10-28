//! Channel, bus, main, and auxiliary surface operations.
use super::*;

impl XAirDriver {
    pub async fn set_bus_master_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let _dispatch = self.bus_dispatch.read().await;
        let (kind, root) = strip_root(strip.0)?;
        anyhow::ensure!(kind == openfader_core::StripKind::Bus, "not a bus master");
        let (address, argument) = match (param, value) {
            ("fader", ParamValue::Float(value)) => (
                strip_mix_fader(&root),
                osc::fader_arg(openfader_core::validate_fader(value)?),
            ),
            ("mute", ParamValue::Bool(muted)) => (strip_mix_on(&root), osc::on_arg(muted)),
            _ => anyhow::bail!("unsupported checked bus master control"),
        };
        let session = self.session().await?;
        session
            .as_ref()
            .unwrap()
            .send_checked(&address, vec![argument], &|| {
                let members = self
                    .current_bus_control_members(strip)?
                    .ok_or_else(|| anyhow::anyhow!("bus topology changed"))?;
                check(&members)
            })
            .await
    }

    /// Stereo aux return strip (`/rtn/aux`, hardware-verified present,
    /// baseline §2). Kept as explicit methods — not a `StripId` — so the
    /// 0..16 channel range maps 1:1 to wire `/ch/01..16` with no phantom
    /// strips (T3).
    pub async fn get_aux_fader(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::RTN_AUX_FADER).await?;
        osc::fader_value(&reply)
    }

    pub async fn set_aux_fader(&self, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::RTN_AUX_FADER, vec![osc::fader_arg(value)])
            .await
    }

    pub async fn get_aux_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::RTN_AUX_ON).await?;
        osc::mute_is_muted(&reply)
    }

    pub async fn set_aux_mute(&self, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::RTN_AUX_ON, vec![osc::on_arg(muted)])
            .await
    }

    pub async fn get_aux_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::RTN_AUX_NAME).await?;
        Ok(osc::string_args(&reply)
            .first()
            .cloned()
            .unwrap_or_default())
    }

    pub async fn set_aux_name(&self, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::RTN_AUX_NAME, vec![OscType::String(name.to_string())])
            .await
    }

    pub(super) async fn driver_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_fader(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    pub(super) async fn driver_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_fader(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    pub(super) async fn driver_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let _main_lane = if root == "/lr" {
            Some(self.main_mute_write.lock().await)
        } else {
            None
        };
        let guard = self.session().await?;
        let addr = strip_mix_on(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::on_arg(muted)])
            .await
    }

    pub(super) async fn driver_get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_on(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::mute_is_muted(&reply)
    }

    pub(super) async fn driver_set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_config_name(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::String(name.to_string())])
            .await
    }

    pub(super) async fn driver_get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_config_name(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        Ok(osc::string_args(&reply)
            .first()
            .cloned()
            .unwrap_or_default())
    }

    pub(super) async fn driver_set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let wire = addrs::bus(bus)?;
        let guard = self.session().await?;
        let addr = addrs::bus_fader(wire);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    pub(super) async fn driver_get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let wire = addrs::bus(bus)?;
        let guard = self.session().await?;
        let addr = addrs::bus_fader(wire);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    pub(super) async fn driver_set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        if bus >= addrs::MAX_BUS {
            anyhow::bail!("bus {bus} out of range 0..{} (0-based)", addrs::MAX_BUS);
        }
        self.set_mute(StripId(20 + bus as u16), muted).await
    }

    pub(super) async fn driver_get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        if bus >= addrs::MAX_BUS {
            anyhow::bail!("bus {bus} out of range 0..{} (0-based)", addrs::MAX_BUS);
        }
        self.get_mute(StripId(20 + bus as u16)).await
    }

    pub(super) async fn driver_set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::MAIN_FADER, vec![osc::fader_arg(value)])
            .await
    }

    pub(super) async fn driver_get_main_fader(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::MAIN_FADER).await?;
        osc::fader_value(&reply)
    }

    /// Main LR master mute (`/lr/mix/on`, int `1` = unmuted / `0` = muted,
    /// same polarity as channel `/mix/on`). The leaf answers queries
    /// (rig-verified XR18 FW 1.20, 2026-10-18: reads `1`).
    pub(super) async fn driver_set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let _main_lane = self.main_mute_write.lock().await;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::MAIN_ON, vec![osc::on_arg(muted)])
            .await
    }

    pub(super) async fn driver_get_main_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::MAIN_ON).await?;
        osc::mute_is_muted(&reply)
    }
}
