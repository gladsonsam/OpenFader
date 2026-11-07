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

pub use super::providers::xair_routes::InputRouteSelectors;

pub struct SelectorSession {
    engine: Arc<Engine>,
    pub(in crate::engine) driver: Arc<dyn DynDriver>,
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

    pub async fn recorder_descriptor(
        &self,
    ) -> anyhow::Result<Option<crate::drivers::RecorderDescriptor>> {
        self.check()?;
        let mut descriptor = self.driver.recorder_descriptor().await?;
        self.check()?;
        if let Some(descriptor) = &mut descriptor {
            if !descriptor.capture_strip_ids.is_empty() {
                let cache = self.engine.param_cache.read().unwrap();
                descriptor.channel_names = descriptor
                    .capture_strip_ids
                    .iter()
                    .map(|ids| {
                        let mut names = Vec::new();
                        for id in ids {
                            if let Some(name) = cache
                                .get(id)
                                .and_then(|params| params.get("name"))
                                .and_then(serde_json::Value::as_str)
                                .map(str::trim)
                                .filter(|name| !name.is_empty())
                            {
                                let name: String = name.chars().take(128).collect();
                                if !names.contains(&name) {
                                    names.push(name);
                                }
                            }
                        }
                        names.join(" / ")
                    })
                    .collect();
            }
        }
        self.check()?;
        Ok(descriptor)
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
    pub async fn soundcheck_readback(&self) -> anyhow::Result<serde_json::Value> {
        self.check()?;
        let result = self.driver.soundcheck_readback().await;
        self.check()?;
        result?.ok_or_else(|| anyhow::anyhow!("soundcheck readback unsupported"))
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
