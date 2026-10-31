//! Session-bound, driver-defined routing provider.
use crate::routing_transaction::RoutingProvider;
use openfader_engine::engine::selector_session::SelectorSession;
use serde_json::Value;

pub struct EngineAdapter {
    session: SelectorSession,
    keys: Vec<String>,
    driver_state: Value,
    cancellation: Option<crate::recorder::soundcheck_backend::CancellationHandle>,
}
impl EngineAdapter {
    pub fn with_profile(
        session: SelectorSession,
        profile: &openfader_engine::drivers::SoundcheckProfile,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            session,
            keys: profile.snapshot_keys.clone(),
            driver_state: profile.driver_state.clone(),
            cancellation: None,
        })
    }
    pub async fn new(session: SelectorSession) -> anyhow::Result<Self> {
        let profile = session
            .soundcheck_profile()
            .await?
            .ok_or_else(|| anyhow::anyhow!("soundcheck routing unsupported by driver"))?;
        Self::with_profile(session, &profile)
    }
    pub fn with_snapshot_keys(session: SelectorSession, keys: Vec<String>) -> Self {
        Self {
            session,
            keys,
            driver_state: Value::Null,
            cancellation: None,
        }
    }
    pub fn set_cancellation(
        &mut self,
        handle: Option<crate::recorder::soundcheck_backend::CancellationHandle>,
    ) {
        self.cancellation = handle;
    }
}
#[async_trait::async_trait]
impl RoutingProvider for EngineAdapter {
    fn check_cancelled(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self
                .cancellation
                .as_ref()
                .is_some_and(|h| h.cancellation_requested()),
            "routing cancelled"
        );
        Ok(())
    }
    fn snapshot_keys(&self) -> Vec<String> {
        self.keys.clone()
    }
    async fn identity(&mut self) -> anyhow::Result<String> {
        self.session.check()?;
        Ok(serde_json::to_string(self.session.identity())?)
    }
    async fn read(&mut self, key: &str) -> anyhow::Result<i32> {
        self.session
            .soundcheck_route_read(key, &self.driver_state)
            .await
    }
    async fn write_if_expected(
        &mut self,
        key: &str,
        expected: i32,
        value: i32,
    ) -> anyhow::Result<()> {
        self.check_cancelled()?;
        self.driver_state = self
            .session
            .soundcheck_route_write(key, &self.driver_state, expected, value)
            .await?;
        self.check_cancelled()?;
        Ok(())
    }
    async fn write_confirmed(&mut self, key: &str, value: i32) -> anyhow::Result<()> {
        let expected = self.read(key).await?;
        self.write_if_expected(key, expected, value).await?;
        anyhow::ensure!(self.read(key).await? == value, "selector readback mismatch");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_transaction::RoutingTransaction;
    use openfader_engine::{Engine, EngineConfig};
    #[tokio::test]
    async fn simulated_transaction_restores() {
        let engine = Engine::new(EngineConfig::default());
        let session = engine.selector_session().await.unwrap();
        let before = session.read_input_route(0).await.unwrap();
        let mut wrong = before;
        wrong.bank = 1;
        let mut desired = wrong;
        desired.usb = 5;
        assert!(session
            .guarded_single_selector_write(0, wrong, desired)
            .await
            .is_err());
        assert_eq!(session.read_input_route(0).await.unwrap(), before);
        let mut provider = EngineAdapter::new(session).await.unwrap();
        let path = std::env::temp_dir().join(format!(
            "openfader-adapter-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let tx = RoutingTransaction::new(&path);
        tx.enter(
            &mut provider,
            &[("input/0/usb".into(), 5), ("input/0/bank".into(), 1)],
        )
        .await
        .unwrap();
        assert_eq!(provider.read("input/0/usb").await.unwrap(), 5);
        tx.restore(&mut provider).await.unwrap();
        assert_eq!(provider.read("input/0/usb").await.unwrap(), 0);
        assert_eq!(provider.read("input/0/bank").await.unwrap(), 0);
    }
    #[tokio::test]
    async fn channel_31_journal_restores_through_driver() {
        let engine = Engine::new(EngineConfig::default());
        let session = engine.selector_session().await.unwrap();
        let profile = session.soundcheck_profile().await.unwrap().unwrap();
        assert_eq!(profile.capture_channels, 32);
        let mut provider = EngineAdapter::with_profile(session, &profile).unwrap();
        let path = std::env::temp_dir().join(format!("openfader-channel31-{}", std::process::id()));
        let tx = RoutingTransaction::new(&path);
        let original = provider.read("input/31/usb").await.unwrap();
        tx.enter(
            &mut provider,
            &[("input/31/usb".into(), 5), ("input/31/bank".into(), 1)],
        )
        .await
        .unwrap();
        assert!(tx
            .recovery_keys()
            .unwrap()
            .contains(&"input/31/usb".to_string()));
        tx.restore(&mut provider).await.unwrap();
        assert_eq!(provider.read("input/31/usb").await.unwrap(), original);
        assert_eq!(provider.read("input/31/bank").await.unwrap(), 0);
    }
    #[tokio::test]
    async fn preflight_route_mismatch_rejects_before_journal() {
        let engine = Engine::new(EngineConfig::default());
        let session = engine.selector_session().await.unwrap();
        let actual = session.read_input_route(0).await.unwrap();
        let mut stale = actual;
        stale.analog = if actual.analog == 0 { 1 } else { 0 };
        let mut profile = session.soundcheck_profile().await.unwrap().unwrap();
        profile.driver_state["expected"]["0"]["analog"] = stale.analog.into();
        let mut provider = EngineAdapter::with_profile(session, &profile).unwrap();
        let path = std::env::temp_dir().join(format!("openfader-stale-{}", std::process::id()));
        let tx = RoutingTransaction::new(&path);
        assert!(tx
            .enter(&mut provider, &[("input/0/usb".into(), 5)])
            .await
            .is_err());
        assert!(!path.exists());
        assert_eq!(provider.session.read_input_route(0).await.unwrap(), actual);
    }
    #[tokio::test]
    async fn independent_sessions_require_validated_rebind() {
        let engine = Engine::new(EngineConfig::default());
        let other = Engine::new(EngineConfig::default());
        let mut first = EngineAdapter::new(engine.selector_session().await.unwrap())
            .await
            .unwrap();
        let mut second = EngineAdapter::new(other.selector_session().await.unwrap())
            .await
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "openfader-rebind-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let tx = RoutingTransaction::new(&path);
        tx.enter(&mut first, &[("input/0/usb".into(), 5)])
            .await
            .unwrap();
        assert!(tx
            .restore(&mut second)
            .await
            .unwrap_err()
            .is::<crate::routing_transaction::RecoveryRequired>());
        // A conflicting snapshot cannot be rebound or overwritten.
        let original = second.read("input/1/usb").await.unwrap();
        second.write_confirmed("input/1/usb", 7).await.unwrap();
        assert!(tx.rebind_for_recovery(&mut second).await.is_err());
        second
            .write_confirmed("input/1/usb", original)
            .await
            .unwrap();
        tx.rebind_for_recovery(&mut second).await.unwrap();
        tx.restore(&mut second).await.unwrap();
    }
}
