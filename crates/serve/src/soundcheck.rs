//! Soundcheck state machine. The caller owns serialization with all local
//! routing writers and must provide a verified routing plan and fresh provider.
//! Audio integration calls `play_requested` before starting output, `stop` when
//! output stops, and `return_live` only after output is stopped. No audio is
//! started by this module. A pending journal requires explicit recovery.
use crate::routing_transaction::{RoutingProvider, RoutingTransaction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Live,
    Soundcheck,
    RecoveryRequired,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

pub struct Soundcheck {
    transaction: RoutingTransaction,
    mode: InputMode,
    transport: Transport,
    recovery_rebound: bool,
}
impl Soundcheck {
    pub fn new(transaction: RoutingTransaction) -> Self {
        let mode = if transaction.pending() {
            InputMode::RecoveryRequired
        } else {
            InputMode::Live
        };
        Self {
            transaction,
            mode,
            transport: Transport::Stopped,
            recovery_rebound: false,
        }
    }
    pub fn recovery_keys(&self) -> anyhow::Result<Option<Vec<String>>> {
        if self.transaction.pending() {
            Ok(Some(self.transaction.recovery_keys()?))
        } else {
            Ok(None)
        }
    }
    pub fn mode(&self) -> InputMode {
        self.mode
    }
    pub fn transport(&self) -> Transport {
        self.transport
    }
    pub async fn enter(
        &mut self,
        provider: &mut impl RoutingProvider,
        verified_plan: &[(String, i32)],
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.mode == InputMode::Live && self.transport == Transport::Stopped,
            "soundcheck transition unavailable"
        );
        if let Err(error) = self.transaction.enter(provider, verified_plan).await {
            if self.transaction.pending() {
                self.mode = InputMode::RecoveryRequired;
                if self.transaction.restore(provider).await.is_ok() {
                    self.mode = InputMode::Live;
                }
            }
            return Err(error);
        }
        self.mode = InputMode::Soundcheck;
        Ok(())
    }
    /// Call only after the audio backend has accepted the play request.
    pub fn play_requested(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.mode == InputMode::Soundcheck,
            "soundcheck is not active"
        );
        self.transport = Transport::Playing;
        Ok(())
    }
    pub fn pause(&mut self) {
        self.transport = Transport::Paused;
    }
    pub fn stop(&mut self) {
        self.transport = Transport::Stopped;
    }
    /// After an explicit user recovery choice, validate a fresh complete routing
    /// snapshot and bind the journal to this session. This never writes routing;
    /// call `return_live` separately to restore after playback has stopped.
    pub async fn rebind_for_recovery(
        &mut self,
        provider: &mut impl RoutingProvider,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.mode == InputMode::RecoveryRequired && self.transport == Transport::Stopped,
            "recovery unavailable"
        );
        self.transaction.rebind_for_recovery(provider).await?;
        self.recovery_rebound = true;
        Ok(())
    }
    pub async fn return_live(&mut self, provider: &mut impl RoutingProvider) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.transport == Transport::Stopped,
            "stop playback before restoring routing"
        );
        anyhow::ensure!(
            self.mode != InputMode::RecoveryRequired || self.recovery_rebound,
            "routing recovery requires explicit rebind"
        );
        if let Err(error) = self.transaction.restore(provider).await {
            self.mode = InputMode::RecoveryRequired;
            self.recovery_rebound = false;
            return Err(error);
        }
        self.mode = InputMode::Live;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[derive(Default)]
    struct Fake {
        values: HashMap<String, i32>,
        fail: bool,
    }
    #[async_trait::async_trait]
    impl RoutingProvider for Fake {
        fn snapshot_keys(&self) -> Vec<String> {
            vec!["route".into()]
        }
        async fn identity(&mut self) -> anyhow::Result<String> {
            Ok("desk".into())
        }
        async fn read(&mut self, key: &str) -> anyhow::Result<i32> {
            Ok(*self
                .values
                .get(key)
                .ok_or_else(|| anyhow::anyhow!("missing"))?)
        }
        async fn write_confirmed(&mut self, key: &str, value: i32) -> anyhow::Result<()> {
            if self.fail {
                self.fail = false;
                anyhow::bail!("injected failure")
            }
            self.values.insert(key.into(), value);
            Ok(())
        }
    }
    fn setup() -> (Soundcheck, Fake, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "openfader-soundcheck-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut p = Fake::default();
        p.values.insert("route".into(), 1);
        p.values.insert("fader".into(), 5);
        (Soundcheck::new(RoutingTransaction::new(&path)), p, path)
    }
    #[tokio::test]
    async fn transport_and_restore() {
        let (mut c, mut p, _) = setup();
        c.enter(&mut p, &[("route".into(), 2)]).await.unwrap();
        c.play_requested().unwrap();
        assert!(c.return_live(&mut p).await.is_err());
        c.stop();
        assert_eq!(c.mode(), InputMode::Soundcheck);
        p.values.insert("fader".into(), 9);
        c.return_live(&mut p).await.unwrap();
        assert_eq!(p.values["route"], 1);
        assert_eq!(p.values["fader"], 9);
    }
    #[tokio::test]
    async fn failure_restart_and_conflict() {
        let (mut c, mut p, path) = setup();
        p.fail = true;
        assert!(c.enter(&mut p, &[("route".into(), 2)]).await.is_err());
        assert_eq!(c.mode(), InputMode::Live);
        c.enter(&mut p, &[("route".into(), 2)]).await.unwrap();
        let mut restarted = Soundcheck::new(RoutingTransaction::new(&path));
        assert_eq!(restarted.mode(), InputMode::RecoveryRequired);
        assert!(restarted
            .enter(&mut p, &[("route".into(), 2)])
            .await
            .is_err());
        p.values.insert("route".into(), 3);
        assert!(restarted.return_live(&mut p).await.is_err());
        p.values.insert("route".into(), 2);
        restarted.rebind_for_recovery(&mut p).await.unwrap();
        restarted.return_live(&mut p).await.unwrap();
    }
}
