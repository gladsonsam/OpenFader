//! Coordinates journaled routing with a prepared playback session. The caller must
//! serialize this coordinator with capture and all local routing writers.
use crate::{
    playback_client::PlaybackTransport,
    routing_transaction::RoutingProvider,
    soundcheck::{InputMode, Soundcheck, Transport},
};
use serde_json::Value;
use std::{future::Future, time::Instant};

/// The recorder must supply current capture state and a desk-specific verified
/// plan; no production profile is verified by this module.
pub struct Coordinator<R, P> {
    routing: R,
    controller: Soundcheck,
    playback: Option<P>,
    generation: u64,
    output_may_be_active: bool,
}
impl<R: RoutingProvider, P: PlaybackTransport> Coordinator<R, P> {
    /// Construct with the journal-backed controller and the pinned routing provider.
    pub fn new(routing: R, controller: Soundcheck) -> Self {
        Self {
            routing,
            controller,
            playback: None,
            generation: 0,
            output_may_be_active: false,
        }
    }
    pub fn routing_mut(&mut self) -> &mut R {
        &mut self.routing
    }
    /// Input mode remains independent of playback state.
    pub fn mode(&self) -> InputMode {
        self.controller.mode()
    }
    /// Last confirmed controller transport state; query status for actual position.
    pub fn transport(&self) -> Transport {
        self.controller.transport()
    }
    /// Whether output cannot yet be confirmed stopped.
    pub fn output_may_be_active(&self) -> bool {
        self.output_may_be_active
    }
    /// Recorder wiring must check capture under the same serialization lock, prove
    /// the profile, and prepare the requested take silently before resolving `prepare`.
    /// The callback receives the take; its resulting transport must use `generation`.
    #[allow(clippy::too_many_arguments)]
    pub async fn enter<T, F, Fut>(
        &mut self,
        take: T,
        plan: &[(String, i32)],
        profile_verified: bool,
        capture_active: bool,
        generation: u64,
        deadline: Instant,
        prepare: F,
    ) -> anyhow::Result<()>
    where
        F: FnOnce(T) -> Fut,
        Fut: Future<Output = anyhow::Result<P>>,
    {
        anyhow::ensure!(!capture_active, "capture is active");
        anyhow::ensure!(profile_verified, "routing profile is unverified");
        anyhow::ensure!(
            self.mode() == InputMode::Live && !self.output_may_be_active && self.playback.is_none(),
            "soundcheck transition unavailable"
        );
        let playback = prepare(take).await?;
        self.playback = Some(playback);
        self.generation = generation;
        if let Err(e) = self.controller.enter(&mut self.routing, plan).await {
            // Preparation is silent, but still close the helper before returning.
            if let Err(stop) = self.close_helper(deadline).await {
                return Err(e.context(format!("playback stop failed: {stop:#}")));
            }
            return Err(e);
        }
        if let Err(e) = self.routing.check_cancelled() {
            if let Err(stop) = self.close_helper(deadline).await {
                return Err(e.context(format!("playback stop failed: {stop:#}")));
            }
            return Err(e);
        }
        self.output_may_be_active = true;
        if let Err(e) = self
            .playback
            .as_ref()
            .expect("prepared")
            .play(generation)
            .await
        {
            let error = anyhow::anyhow!(e);
            if let Err(stop) = self.close_helper(deadline).await {
                return Err(
                    error.context(format!("playback stop failed; routing retained: {stop:#}"))
                );
            }
            if let Err(restore) = self.controller.return_live(&mut self.routing).await {
                return Err(error.context(format!("routing recovery required: {restore:#}")));
            }
            return Err(error);
        }
        self.controller.play_requested()?;
        Ok(())
    }
    /// Stop output while retaining the prepared helper and routing for replay.
    pub async fn stop(&mut self, _deadline: Instant) -> anyhow::Result<()> {
        if self.controller.transport() == Transport::Playing {
            self.session()?
                .pause(self.generation)
                .await
                .map_err(anyhow::Error::msg)?;
        }
        self.session()?
            .seek(self.generation, 0)
            .await
            .map_err(anyhow::Error::msg)?;
        self.output_may_be_active = false;
        self.controller.stop();
        Ok(())
    }
    /// Close and reap the helper before restoring routing.
    async fn close_helper(&mut self, deadline: Instant) -> anyhow::Result<()> {
        if let Some(p) = &self.playback {
            self.output_may_be_active = true;
            p.stop(deadline).await.map_err(anyhow::Error::msg)?;
            self.playback = None;
        }
        self.output_may_be_active = false;
        self.controller.stop();
        Ok(())
    }
    pub async fn return_live(&mut self, deadline: Instant) -> anyhow::Result<()> {
        self.close_helper(deadline).await?;
        self.controller.return_live(&mut self.routing).await
    }
    /// After an explicit recovery choice, validate and rebind the existing journal;
    /// then call `return_live` separately. No routing is written by this method.
    pub async fn rebind_for_recovery(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.output_may_be_active, "output may still be active");
        self.controller.rebind_for_recovery(&mut self.routing).await
    }
    fn session(&self) -> anyhow::Result<&P> {
        anyhow::ensure!(
            self.mode() == InputMode::Soundcheck,
            "soundcheck is not active"
        );
        self.playback
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("playback session is stopped"))
    }
    /// Pause the current session; an error leaves its previous state intact.
    pub async fn pause(&mut self) -> anyhow::Result<Value> {
        let result = self
            .session()?
            .pause(self.generation)
            .await
            .map_err(anyhow::Error::msg)?;
        self.controller.pause();
        self.output_may_be_active = false;
        Ok(result)
    }
    /// Resume a paused session; a failed request may still have started output.
    pub async fn play(&mut self) -> anyhow::Result<Value> {
        self.session()?;
        self.output_may_be_active = true;
        let result = self
            .session()?
            .play(self.generation)
            .await
            .map_err(anyhow::Error::msg)?;
        self.controller.play_requested()?;
        Ok(result)
    }
    /// Seek within the prepared session; return the helper's response unchanged.
    pub async fn seek(&self, frame: u64) -> anyhow::Result<Value> {
        self.session()?
            .seek(self.generation, frame)
            .await
            .map_err(anyhow::Error::msg)
    }
    /// Configure looping in the helper; return its response unchanged.
    pub async fn set_loop(&self, start: u64, end: u64, enabled: bool) -> anyhow::Result<Value> {
        self.session()?
            .set_loop(self.generation, start, end, enabled)
            .await
            .map_err(anyhow::Error::msg)
    }
    pub fn has_playback(&self) -> bool {
        self.playback.is_some()
    }
    /// Read the helper's real callback position and state without synthesizing progress.
    pub async fn status(&self) -> anyhow::Result<Value> {
        self.session()?.status().await.map_err(anyhow::Error::msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing_transaction::RoutingTransaction;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        time::Duration,
    };
    #[derive(Default)]
    struct Route {
        values: HashMap<String, i32>,
        events: Arc<Mutex<Vec<&'static str>>>,
        cancel_before_play: bool,
        reads_after_write: usize,
    }
    #[async_trait::async_trait]
    impl RoutingProvider for Route {
        fn check_cancelled(&self) -> anyhow::Result<()> {
            anyhow::ensure!(
                !(self.cancel_before_play && self.reads_after_write > 0),
                "cancelled"
            );
            Ok(())
        }
        fn snapshot_keys(&self) -> Vec<String> {
            vec!["route".into()]
        }
        async fn identity(&mut self) -> anyhow::Result<String> {
            Ok("desk".into())
        }
        async fn read(&mut self, k: &str) -> anyhow::Result<i32> {
            if self.cancel_before_play && self.values[k] == 2 {
                self.reads_after_write += 1;
            }
            Ok(self.values[k])
        }
        async fn write_confirmed(&mut self, k: &str, v: i32) -> anyhow::Result<()> {
            self.events.lock().unwrap().push("route");
            self.values.insert(k.into(), v);
            Ok(())
        }
    }
    struct FakePlayback {
        events: Arc<Mutex<Vec<&'static str>>>,
        fail_play: bool,
        fail_stop: bool,
    }
    impl PlaybackTransport for FakePlayback {
        async fn play(&self, _: u64) -> Result<Value, String> {
            self.events.lock().unwrap().push("play");
            if self.fail_play {
                Err("play failed".into())
            } else {
                Ok(Value::Null)
            }
        }
        async fn pause(&self, _: u64) -> Result<Value, String> {
            Ok(Value::Null)
        }
        async fn seek(&self, _: u64, _: u64) -> Result<Value, String> {
            Ok(Value::Null)
        }
        async fn set_loop(&self, _: u64, _: u64, _: u64, _: bool) -> Result<Value, String> {
            Ok(Value::Null)
        }
        async fn status(&self) -> Result<Value, String> {
            Ok(serde_json::json!({"position_frames": 0}))
        }
        async fn stop(&self, _: Instant) -> Result<Value, String> {
            self.events.lock().unwrap().push("stop");
            if self.fail_stop {
                Err("stop failed".into())
            } else {
                Ok(Value::Null)
            }
        }
    }
    fn setup() -> Coordinator<Route, FakePlayback> {
        let path = std::env::temp_dir().join(format!(
            "soundcheck-coordinator-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut route = Route::default();
        route.values.insert("route".into(), 1);
        route.values.insert("fader".into(), 5);
        Coordinator::new(route, Soundcheck::new(RoutingTransaction::new(path)))
    }
    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }
    async fn enter(
        c: &mut Coordinator<Route, FakePlayback>,
        fail_play: bool,
        fail_stop: bool,
    ) -> anyhow::Result<()> {
        let events = c.routing.events.clone();
        c.enter(
            "take",
            &[("route".into(), 2)],
            true,
            false,
            1,
            deadline(),
            move |_| async move {
                events.lock().unwrap().push("prepare");
                Ok(FakePlayback {
                    events,
                    fail_play,
                    fail_stop,
                })
            },
        )
        .await
    }
    #[tokio::test]
    async fn cancellation_before_play_stops_helper_and_keeps_routing() {
        let mut c = setup();
        c.routing.cancel_before_play = true;
        assert!(enter(&mut c, false, false).await.is_err());
        assert_eq!(
            *c.routing.events.lock().unwrap(),
            ["prepare", "route", "stop"]
        );
        assert_eq!(c.routing.values["route"], 2);
        c.routing.cancel_before_play = false;
        c.return_live(deadline()).await.unwrap();
    }
    #[tokio::test]
    async fn order_stop_and_restore() {
        let mut c = setup();
        enter(&mut c, false, false).await.unwrap();
        assert_eq!(
            *c.routing.events.lock().unwrap(),
            ["prepare", "route", "play"]
        );
        c.stop(deadline()).await.unwrap();
        assert_eq!(c.mode(), InputMode::Soundcheck);
        assert_eq!(c.routing.values["route"], 2);
        c.play().await.unwrap();
        assert_eq!(c.routing.values["route"], 2);
        c.routing.values.insert("fader".into(), 9);
        c.return_live(deadline()).await.unwrap();
        assert_eq!(c.routing.values["route"], 1);
        assert_eq!(c.routing.values["fader"], 9);
    }
    #[tokio::test]
    async fn failed_stop_retains_journal_until_retry_succeeds() {
        let mut c = setup();
        enter(&mut c, false, true).await.unwrap();
        assert!(c.return_live(deadline()).await.is_err());
        assert_eq!(c.routing.values["route"], 2);
        assert!(c.output_may_be_active());
        c.playback.as_mut().unwrap().fail_stop = false;
        c.return_live(deadline()).await.unwrap();
        assert_eq!(c.routing.values["route"], 1);
        assert_eq!(c.mode(), InputMode::Live);
    }
    #[tokio::test]
    async fn failures_and_conflict_recovery() {
        let mut c = setup();
        assert!(enter(&mut c, true, false).await.is_err());
        assert_eq!(c.routing.values["route"], 1);
        let mut c = setup();
        assert!(enter(&mut c, true, true).await.is_err());
        assert!(c.return_live(deadline()).await.is_err());
        assert_eq!(c.routing.values["route"], 2);
        assert!(c.output_may_be_active());
        let mut c = setup();
        enter(&mut c, false, false).await.unwrap();
        c.routing.values.insert("route".into(), 3);
        assert!(c.return_live(deadline()).await.is_err());
        assert_eq!(c.mode(), InputMode::RecoveryRequired);
        assert!(c.rebind_for_recovery().await.is_err());
        c.routing.values.insert("route".into(), 2);
        c.rebind_for_recovery().await.unwrap();
        c.return_live(deadline()).await.unwrap();
    }
}
