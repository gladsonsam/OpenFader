//! Owned input-selector transaction for adapters.
//!
//! `Arc<Engine>::selector_session(&self) -> Result<SelectorSession>` acquires
//! the routing boundary until the returned value is dropped. Use
//! `SelectorSession::identity(&self) -> &SelectorSessionIdentity`,
//! `read_input_route(&self, channel: u16) -> Result<InputRouteSelectors>`, and
//! `guarded_single_selector_write(&self, channel: u16, expected: InputRouteSelectors,
//! desired: InputRouteSelectors) -> Result<()>` (the latter two are async).
//! A stale session fails; callers must discard it after a connection change.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SelectorSessionIdentity {
    pub mixer_id: String,
    pub generation: u64,
    pub model: String,
    pub address: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InputRouteSelectors {
    pub analog: i32,
    pub usb: i32,
    pub bank: i32,
}

pub struct SelectorSession {
    engine: Arc<Engine>,
    driver: Arc<dyn DynDriver>,
    driver_name: String,
    identity: SelectorSessionIdentity,
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

impl Engine {
    pub fn connection_generation(&self) -> u64 {
        self.connection_gen.load(Ordering::Acquire)
    }

    pub async fn selector_session(self: &Arc<Self>) -> anyhow::Result<SelectorSession> {
        let guard = self.routing_mutation_guard().await;
        let (generation, driver_name, suspended) = self.sample_capture_pin();
        anyhow::ensure!(!suspended, "connection is changing");
        let driver = self.drv(&driver_name);
        let (model, address, name) = if driver_name == "sim" {
            ("sim".to_owned(), String::new(), String::new())
        } else {
            let (state, address) = {
                let link = self.link.read().unwrap();
                (link.state.clone(), link.ip.clone())
            };
            anyhow::ensure!(
                state == "connected" && !address.is_empty(),
                "no live connection"
            );
            let info = driver.query_info().await?;
            anyhow::ensure!(!info.model.trim().is_empty(), "reported model unavailable");
            (info.model, address, info.name)
        };
        let nonce = self
            .selector_nonce
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let session = SelectorSession {
            engine: Arc::clone(self),
            driver,
            driver_name: driver_name.clone(),
            identity: SelectorSessionIdentity {
                mixer_id: format!("{nonce}:{generation}:{driver_name}"),
                generation,
                model,
                address,
                name,
            },
            _guard: guard,
        };
        session.check()?;
        Ok(session)
    }
}

impl SelectorSession {
    pub fn identity(&self) -> &SelectorSessionIdentity {
        &self.identity
    }

    pub fn check(&self) -> anyhow::Result<()> {
        let (generation, driver, suspended) = self.engine.sample_capture_pin();
        anyhow::ensure!(
            !suspended && generation == self.identity.generation && driver == self.driver_name,
            "stale selector session"
        );
        Ok(())
    }

    /// Read and validate a profile while the connection remains pinned.
    pub async fn soundcheck_profile(
        &self,
    ) -> anyhow::Result<Option<crate::drivers::SoundcheckProfile>> {
        self.check()?;
        let result = self.driver.soundcheck_profile().await;
        self.check()?;
        let profile = result?;
        if let Some(profile) = &profile {
            profile.validate()?;
        }
        Ok(profile)
    }

    /// Read a provider-owned route while the connection remains pinned.
    pub async fn soundcheck_route_read(
        &self,
        key: &str,
        driver_state: &serde_json::Value,
    ) -> anyhow::Result<i32> {
        self.check()?;
        let check = |_: &[StripId]| self.check();
        let result = self
            .driver
            .soundcheck_route_read(key, driver_state, &check)
            .await;
        self.check()?;
        result
    }

    /// Write a provider-owned route while the connection remains pinned.
    pub async fn soundcheck_route_write(
        &self,
        key: &str,
        driver_state: &serde_json::Value,
        expected: i32,
        desired: i32,
    ) -> anyhow::Result<serde_json::Value> {
        self.check()?;
        let check = |_: &[StripId]| self.check();
        let result = self
            .driver
            .soundcheck_route_write(key, driver_state, expected, desired, &check)
            .await;
        self.check()?;
        result
    }

    /// Fresh provider JSON; rejects a changed engine connection before and after reading.
    /// See `openfader_proto_xair::soundcheck_readback` for the XR18 JSON schema.
    pub async fn soundcheck_readback(&self) -> anyhow::Result<serde_json::Value> {
        self.check()?;
        let result = self.driver.soundcheck_readback().await;
        self.check()?;
        result?.ok_or_else(|| anyhow::anyhow!("soundcheck readback unsupported"))
    }

    pub async fn read_input_route(&self, channel: u16) -> anyhow::Result<InputRouteSelectors> {
        self.check()?;
        let result = self.driver.input_route_guard(StripId(channel)).await;
        self.check()?;
        let value = result?.ok_or_else(|| anyhow::anyhow!("input route unsupported"))?;
        Ok(serde_json::from_value(value)?)
    }

    /// Read a pair and its link state using the pinned driver and routing guard.
    pub async fn read_link_pair(
        &self,
        left: u16,
    ) -> anyhow::Result<([InputRouteSelectors; 2], bool)> {
        anyhow::ensure!(left < 16 && left.is_multiple_of(2), "invalid input pair");
        self.check()?;
        let link = self
            .driver
            .input_pair_link(StripId(left))
            .await?
            .ok_or_else(|| anyhow::anyhow!("input link unsupported"))?;
        let routes = [
            self.read_input_route(left).await?,
            self.read_input_route(left + 1).await?,
        ];
        self.check()?;
        Ok((routes, link))
    }

    /// Switch a linked pair only if both captured routes and link still match.
    pub async fn guarded_pair_bank(
        &self,
        left: u16,
        expected: [InputRouteSelectors; 2],
        bank: i32,
    ) -> anyhow::Result<()> {
        self.check()?;
        let check = |_: &[StripId]| self.check();
        self.driver
            .linked_input_bank_guarded(
                StripId(left),
                [
                    serde_json::to_value(expected[0])?,
                    serde_json::to_value(expected[1])?,
                ],
                bank,
                &check,
            )
            .await?;
        self.check()
    }

    pub async fn guarded_single_selector_write(
        &self,
        channel: u16,
        expected: InputRouteSelectors,
        desired: InputRouteSelectors,
    ) -> anyhow::Result<()> {
        self.check()?;
        anyhow::ensure!(
            self.read_input_route(channel).await? == expected,
            "input route changed"
        );
        let differences = [
            expected.analog != desired.analog,
            expected.usb != desired.usb,
            expected.bank != desired.bank,
        ];
        anyhow::ensure!(
            differences.iter().filter(|v| **v).count() == 1,
            "exactly one selector must change"
        );
        let (target, source) = if differences[0] {
            match desired.analog {
                0..=15 => ("analog", Some(("Mic", (desired.analog + 1) as u8))),
                16..=17 => ("analog", Some(("Aux", (desired.analog - 15) as u8))),
                18 => ("analog", None),
                _ => anyhow::bail!("invalid analog selector"),
            }
        } else if differences[1] {
            anyhow::ensure!((0..=17).contains(&desired.usb), "invalid usb selector");
            ("usb", Some(("USB", (desired.usb + 1) as u8)))
        } else {
            anyhow::ensure!((0..=1).contains(&desired.bank), "invalid bank selector");
            ("bank", Some(("Bank", (desired.bank + 1) as u8)))
        };
        let guard = serde_json::to_value(expected)?;
        let check = |_: &[StripId]| self.check();
        let result = self
            .driver
            .patch_target_guarded(StripId(channel), target, source, Some(&guard), &check)
            .await;
        self.check()?;
        result?;
        anyhow::ensure!(
            self.read_input_route(channel).await? == desired,
            "selector readback mismatch"
        );
        Ok(())
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[tokio::test]
    async fn unsupported_profile_is_none_and_stale_session_fails() {
        let engine = Engine::new(EngineConfig::default());
        let mut session = engine.selector_session().await.unwrap();
        session.driver = Arc::new(openfader_proto_dl32s::Dl32sDriver::new());
        assert!(session.soundcheck_profile().await.is_err());
        engine.invalidate_connection();
        assert!(session.soundcheck_profile().await.is_err());
    }
}
