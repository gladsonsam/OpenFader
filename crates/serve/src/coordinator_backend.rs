//! Lazy session-bound adapter for journaled soundcheck playback.
use crate::{
    engine_adapter::EngineAdapter,
    playback_client::PlaybackClient,
    recorder::soundcheck_backend::{CancellationHandle, SoundcheckBackend, SoundcheckState},
    routing_transaction::RoutingTransaction,
    soundcheck::{InputMode, Soundcheck, Transport},
    soundcheck_transport::Coordinator,
};
use openfader_audio::DeviceInfo;
use openfader_engine::Engine;
use serde_json::Value;
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

/// Configuration for the session-bound soundcheck backend.
pub struct BackendConfig {
    pub journal: PathBuf,
    pub helper: Option<PathBuf>,
    pub device: Option<DeviceInfo>,
    pub root: PathBuf,
}
impl BackendConfig {
    pub fn new(journal: PathBuf, root: PathBuf) -> Self {
        Self {
            journal,
            root,
            helper: None,
            device: None,
        }
    }
}

fn validate_take(
    root: &std::path::Path,
    id: &str,
    device: &DeviceInfo,
    channels: u16,
) -> anyhow::Result<()> {
    use std::io::Read;
    anyhow::ensure!(
        !id.is_empty() && id != "." && id != ".." && !id.contains(['/', '\\']),
        "invalid take ID"
    );
    anyhow::ensure!(
        (1..=64).contains(&channels)
            && device.output_channels >= channels
            && device.sample_rate > 0,
        "incompatible playback device"
    );
    let dir = root.join(id);
    let manifest = openfader_audio::read_manifest(&dir)?;
    anyhow::ensure!(
        manifest.complete
            && manifest.frames > 0
            && manifest.sample_rate == device.sample_rate
            && manifest.channels == channels,
        "incompatible take"
    );
    let block_align = u64::from(channels) * 3;
    for segment in &manifest.segments {
        let mut file = std::fs::File::open(dir.join(&segment.file))?;
        let mut h = [0u8; 44];
        file.read_exact(&mut h)?;
        let data = segment
            .frames
            .checked_mul(block_align)
            .ok_or_else(|| anyhow::anyhow!("segment overflow"))?;
        let u32_at = |i| u32::from_le_bytes(h[i..i + 4].try_into().unwrap());
        let u16_at = |i| u16::from_le_bytes(h[i..i + 2].try_into().unwrap());
        anyhow::ensure!(
            &h[0..4] == b"RIFF"
                && &h[8..16] == b"WAVEfmt "
                && &h[36..40] == b"data"
                && u32_at(16) == 16
                && u16_at(20) == 1
                && u16_at(22) == channels
                && u32_at(24) == manifest.sample_rate
                && u32_at(28) == manifest.sample_rate.saturating_mul(u32::from(channels) * 3)
                && u16_at(32) == channels * 3
                && u16_at(34) == 24
                && u64::from(u32_at(40)) == data
                && u64::from(u32_at(4)) == data + 36
                && file.metadata()?.len() == data + 44,
            "invalid WAV segment"
        );
    }
    Ok(())
}

fn valid_playback_status(status: &Value) -> bool {
    status.is_object()
        && status["position_frames"].as_u64().is_some()
        && status["sample_rate"].as_u64().is_some_and(|rate| rate > 0)
        && matches!(
            status["transport"].as_str(),
            Some("playing" | "paused" | "stopped")
        )
}

pub struct CoordinatorBackend {
    engine: Arc<Engine>,
    config: BackendConfig,
    coordinator: Option<Coordinator<EngineAdapter, PlaybackClient>>,
    generation: u64,
    telemetry: Option<Value>,
    manual: Option<PlaybackClient>,
    manual_transport: String,
    cancellation: Mutex<CancellationSlot>,
    probe_key: Option<(u64, String, u16, u16, u32)>,
    probe: Option<tokio::task::JoinHandle<Result<(), String>>>,
    profile_result: Option<Result<(), String>>,
}

#[derive(Default)]
struct CancellationSlot {
    epoch: u64,
    reserved: bool,
    handle: Option<CancellationHandle>,
}
struct PendingOperation {
    handle: CancellationHandle,
    started: Arc<AtomicBool>,
}
impl Drop for PendingOperation {
    fn drop(&mut self) {
        if !self.started.load(Ordering::Acquire) {
            self.handle.terminated.store(true, Ordering::Release);
        }
    }
}
impl CoordinatorBackend {
    pub fn new(engine: Arc<Engine>, config: BackendConfig) -> Self {
        Self {
            engine,
            config,
            coordinator: None,
            generation: 0,
            telemetry: None,
            manual: None,
            manual_transport: "stopped".into(),
            cancellation: Mutex::new(CancellationSlot::default()),
            probe_key: None,
            probe: None,
            profile_result: None,
        }
    }
    async fn acquire(&mut self) -> anyhow::Result<()> {
        if self.coordinator.is_none() {
            let session = self.engine.selector_session().await?;
            let transaction = RoutingTransaction::new(&self.config.journal);
            let route = if transaction.pending() {
                EngineAdapter::with_snapshot_keys(session, transaction.recovery_keys()?)
            } else {
                EngineAdapter::new(session).await?
            };
            self.coordinator = Some(Coordinator::new(
                route,
                Soundcheck::new(RoutingTransaction::new(&self.config.journal)),
            ));
        }
        Ok(())
    }
    async fn run(&mut self, action: &str, args: &Value) -> anyhow::Result<()> {
        if !matches!(action, "enter_soundcheck" | "enter_manual_soundcheck") {
            return self.run_inner(action, args, None).await;
        }
        let cancellation = {
            let mut slot = self
                .cancellation
                .lock()
                .expect("soundcheck cancellation lock poisoned");
            if !slot.reserved {
                anyhow::ensure!(
                    slot.handle
                        .as_ref()
                        .is_none_or(CancellationHandle::output_terminated),
                    "playback operation still active"
                );
                slot.epoch = slot
                    .epoch
                    .checked_add(1)
                    .expect("cancellation epoch exhausted");
                slot.handle = Some(CancellationHandle::default());
            }
            slot.reserved = false;
            slot.handle.as_ref().unwrap().clone()
        };
        let started = Arc::new(AtomicBool::new(false));
        let _pending = PendingOperation {
            handle: cancellation.clone(),
            started: started.clone(),
        };
        let result = self
            .run_inner(action, args, Some((cancellation.clone(), started.clone())))
            .await;
        result
    }
    async fn run_inner(
        &mut self,
        action: &str,
        args: &Value,
        operation: Option<(CancellationHandle, Arc<AtomicBool>)>,
    ) -> anyhow::Result<()> {
        let marker = self.config.root.join("manual-routing-pending");
        if action == "enter_manual_soundcheck" {
            anyhow::ensure!(
                args["routing_confirmed"] == true,
                "confirm that mixer inputs are routed to interface returns"
            );
            anyhow::ensure!(
                !marker.exists()
                    && !self.config.journal.exists()
                    && self.coordinator.is_none()
                    && self.manual.is_none(),
                "finish existing routing recovery first"
            );
            // Persist before touching output: external mixer routing was already changed.
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            let nonce = format!(
                "{}-{}-{}",
                std::process::id(),
                self.generation,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            );
            f.write_all(nonce.as_bytes())?;
            f.sync_all()?;
            #[cfg(unix)]
            std::fs::File::open(&self.config.root)?.sync_all()?;
            let device = self
                .config
                .device
                .clone()
                .ok_or_else(|| anyhow::anyhow!("select an audio interface"))?;
            anyhow::ensure!(
                args["expected_device_id"].as_str() == Some(device.id.as_str()),
                "selected interface changed; confirm the mapping again"
            );
            let helper = self
                .config
                .helper
                .clone()
                .filter(|p| p.is_file())
                .ok_or_else(|| anyhow::anyhow!("playback helper unavailable"))?;
            let take = args["take_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("take_id required"))?;
            anyhow::ensure!(
                !take.is_empty() && take != "." && take != ".." && !take.contains(['/', '\\']),
                "invalid take ID"
            );
            let manifest = openfader_audio::read_manifest(self.config.root.join(take))?;
            let mut take_device = device.clone();
            take_device.output_channels = take_device.output_channels.max(manifest.channels);
            validate_take(&self.config.root, take, &take_device, manifest.channels)?;
            let mapping: Vec<Option<usize>> =
                serde_json::from_value(args["output_mapping"].clone())?;
            anyhow::ensure!(
                mapping.len() == usize::from(manifest.channels),
                "map every take track"
            );
            let mut used = std::collections::HashSet::new();
            anyhow::ensure!(
                mapping
                    .iter()
                    .flatten()
                    .all(|&v| v < usize::from(device.output_channels) && used.insert(v))
                    && !used.is_empty(),
                "outputs must be unique and within interface range"
            );
            let (cancellation, started) = operation.expect("manual enter has operation");
            anyhow::ensure!(!cancellation.cancellation_requested(), "playback cancelled");
            self.generation = self.generation.wrapping_add(1);
            let generation = self.generation;
            started.store(true, Ordering::Release);
            let c = PlaybackClient::prepare_mapped(
                helper,
                device.clone(),
                self.config.root.clone(),
                take.into(),
                generation,
                Instant::now() + Duration::from_secs(10),
                cancellation,
                vec![true; device.output_channels as usize],
                Some(mapping),
                Some(nonce),
            )
            .ready()
            .await
            .map_err(anyhow::Error::msg)?;
            self.manual = Some(c);
            self.manual
                .as_ref()
                .unwrap()
                .play(generation)
                .await
                .map_err(anyhow::Error::msg)?;
            self.manual_transport = "playing".into();
            return Ok(());
        }
        if marker.exists() {
            anyhow::ensure!(
                !self.config.journal.exists(),
                "automatic routing recovery must be resolved separately"
            );
            if action == "return_manual_live" {
                anyhow::ensure!(
                    args["routing_restored"] == true,
                    "confirm physical live routing has been restored"
                );
                anyhow::ensure!(
                    self.manual.is_none(),
                    "stop and close playback before restoring live routing"
                );
                let _guard = openfader_audio::playback_guard(&self.config.root)
                    .map_err(|_| anyhow::anyhow!("playback helper has not exited"))?;
                std::fs::remove_file(&marker)?;
                #[cfg(unix)]
                std::fs::File::open(&self.config.root)?.sync_all()?;
                return Ok(());
            }
            if action == "stop_manual_output" {
                if let Some(c) = self.manual.as_ref() {
                    c.stop(Instant::now() + Duration::from_secs(3))
                        .await
                        .map_err(anyhow::Error::msg)?;
                    self.manual = None;
                }
                // A previous host may have died while its helper was blocked in
                // native initialization. The helper holds this lock until exit.
                let _guard = openfader_audio::playback_guard(&self.config.root).map_err(|_| {
                    anyhow::anyhow!(
                        "playback helper has not exited; wait until output is confirmed stopped"
                    )
                })?;
                self.manual_transport = "stopped".into();
                return Ok(());
            }
            let c = self.manual.as_ref().ok_or_else(|| {
                anyhow::anyhow!("restore physical live routing before continuing")
            })?;
            let gen = self.generation;
            let value = match action {
                "play" => {
                    let v = c.play(gen).await;
                    if v.is_ok() {
                        self.manual_transport = "playing".into();
                    }
                    v
                }
                "pause" => {
                    let v = c.pause(gen).await;
                    if v.is_ok() {
                        self.manual_transport = "paused".into();
                    }
                    v
                }
                "stop" => {
                    if self.manual_transport == "playing" {
                        c.pause(gen).await.map_err(anyhow::Error::msg)?;
                    }
                    let v = c.seek(gen, 0).await;
                    if v.is_ok() {
                        self.manual_transport = "stopped".into();
                    }
                    v
                }
                "seek" => {
                    c.seek(
                        gen,
                        args["position_frames"]
                            .as_u64()
                            .ok_or_else(|| anyhow::anyhow!("position_frames required"))?,
                    )
                    .await
                }
                "set_loop" => {
                    c.set_loop(
                        gen,
                        args["start_frame"]
                            .as_u64()
                            .ok_or_else(|| anyhow::anyhow!("start_frame required"))?,
                        args["end_frame"]
                            .as_u64()
                            .ok_or_else(|| anyhow::anyhow!("end_frame required"))?,
                        args["enabled"]
                            .as_bool()
                            .ok_or_else(|| anyhow::anyhow!("enabled required"))?,
                    )
                    .await
                }
                _ => anyhow::bail!("manual routing recovery required"),
            }
            .map_err(anyhow::Error::msg)?;
            if value.get("status").is_some() {
                self.telemetry = Some(value["status"].clone());
            }
            return Ok(());
        }
        if action == "accept_current_routing" {
            anyhow::ensure!(
                args["accept_current"] == true,
                "explicit acceptance of current routing is required"
            );
            // Confirm output termination before releasing any existing session.
            // Acquire a fresh provider: manual restoration may have invalidated
            // the previous provider's expected route values.
            if let Some(coordinator) = &mut self.coordinator {
                coordinator
                    .stop(Instant::now() + Duration::from_secs(2))
                    .await?;
                anyhow::ensure!(
                    !coordinator.output_may_be_active(),
                    "output termination is unconfirmed"
                );
            }
            self.coordinator = None;
            let session = self.engine.selector_session().await?;
            let transaction = RoutingTransaction::new(&self.config.journal);
            let mut provider = if transaction.pending() {
                EngineAdapter::with_snapshot_keys(session, transaction.recovery_keys()?)
            } else {
                EngineAdapter::new(session).await?
            };
            RoutingTransaction::new(&self.config.journal)
                .archive_current_if_restored(&mut provider, true, true)
                .await?;
            return Ok(());
        }
        if action == "enter_soundcheck" {
            if let Some(probe) = self.probe.take() {
                probe.abort();
            }
            let helper = self
                .config
                .helper
                .clone()
                .filter(|p| p.is_file())
                .ok_or_else(|| anyhow::anyhow!("playback helper unavailable"))?;
            let device = self
                .config
                .device
                .clone()
                .ok_or_else(|| anyhow::anyhow!("playback device unavailable"))?;
            let take = args["take_id"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow::anyhow!("take_id required"))?
                .to_owned();
            let (cancellation, started) = operation.expect("enter has an operation");
            anyhow::ensure!(!cancellation.cancellation_requested(), "playback cancelled");
            anyhow::ensure!(self.coordinator.is_none(), "soundcheck already active");
            let session = self.engine.selector_session().await?;
            let mut profile = session.soundcheck_profile().await?.ok_or_else(|| {
                anyhow::anyhow!("connected driver does not provide soundcheck routing")
            })?;
            profile.validate()?;
            let descriptor = session.recorder_descriptor().await?.ok_or_else(|| anyhow::anyhow!("The connected provider has no verified audio interface association; use manual virtual soundcheck"))?;
            anyhow::ensure!(crate::recorder::recorder_device_matches(&descriptor, &device), "The selected audio interface does not match the connected provider; use manual virtual soundcheck");
            validate_take(&self.config.root, &take, &device, profile.capture_channels)?;
            anyhow::ensure!(
                device.output_channels >= profile.output_channels,
                "insufficient playback outputs"
            );
            profile
                .output_mask
                .resize(usize::from(device.output_channels), false);
            let route = EngineAdapter::with_profile(session, &profile)?;
            self.coordinator = Some(Coordinator::new(
                route,
                Soundcheck::new(RoutingTransaction::new(&self.config.journal)),
            ));
            anyhow::ensure!(!cancellation.cancellation_requested(), "playback cancelled");
            self.coordinator
                .as_mut()
                .unwrap()
                .routing_mut()
                .set_cancellation(Some(cancellation.clone()));
            self.generation = self.generation.wrapping_add(1);
            let generation = self.generation;
            let root = self.config.root.clone();
            let deadline = Instant::now() + Duration::from_secs(10);
            let result = self
                .coordinator
                .as_mut()
                .unwrap()
                .enter(
                    take,
                    &profile.route_changes,
                    true,
                    false,
                    generation,
                    deadline,
                    move |take| async move {
                        started.store(true, Ordering::Release);
                        PlaybackClient::prepare_masked(
                            helper,
                            device,
                            root,
                            take,
                            generation,
                            deadline,
                            cancellation,
                            profile.output_mask,
                        )
                        .ready()
                        .await
                        .map_err(anyhow::Error::msg)
                    },
                )
                .await;
            self.release_if_live();
            return result;
        }
        if matches!(action, "return_live" | "rebind_recovery") {
            self.acquire().await?;
        }
        let c = self
            .coordinator
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("soundcheck is not active"))?;
        if matches!(action, "return_live" | "rebind_recovery") {
            c.routing_mut()
                .set_cancellation(Some(CancellationHandle::default()));
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = match action {
            "play" => c.play().await.map(|_| ()),
            "pause" => c.pause().await.map(|_| ()),
            "stop" => {
                let result = c.stop(deadline).await;
                if result.is_ok() {
                    let status = self.telemetry.get_or_insert_with(|| serde_json::json!({}));
                    status["transport"] = serde_json::json!("stopped");
                    status["position_frames"] = serde_json::json!(0);
                    status.as_object_mut().map(|o| o.remove("playback_error"));
                }
                result
            }
            "return_live" => c.return_live(deadline).await,
            "rebind_recovery" => c.rebind_for_recovery().await,
            "seek" => c
                .seek(
                    args["position_frames"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("position_frames required"))?,
                )
                .await
                .map(|_| ()),
            "set_loop" => {
                let start = args["start_frame"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("start_frame required"))?;
                let end = args["end_frame"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("end_frame required"))?;
                let enabled = args["enabled"]
                    .as_bool()
                    .ok_or_else(|| anyhow::anyhow!("enabled required"))?;
                anyhow::ensure!(start < end, "invalid loop range");
                c.set_loop(start, end, enabled).await.map(|_| ())
            }
            _ => anyhow::bail!("unknown soundcheck action"),
        };
        self.release_if_live();
        result
    }
    fn release_if_live(&mut self) {
        if self
            .coordinator
            .as_ref()
            .is_some_and(|c| c.mode() == InputMode::Live && !c.output_may_be_active())
        {
            self.coordinator = None;
        }
    }
}
impl Drop for CoordinatorBackend {
    fn drop(&mut self) {
        if let Some(probe) = self.probe.take() {
            probe.abort();
        }
    }
}
impl SoundcheckBackend for CoordinatorBackend {
    fn cancellation_handle(&self) -> CancellationHandle {
        let mut slot = self
            .cancellation
            .lock()
            .expect("soundcheck cancellation lock poisoned");
        if slot.reserved || slot.handle.as_ref().is_some_and(|h| !h.output_terminated()) {
            return slot.handle.as_ref().unwrap().clone();
        }
        slot.epoch = slot
            .epoch
            .checked_add(1)
            .expect("cancellation epoch exhausted");
        let handle = CancellationHandle::default();
        slot.handle = Some(handle.clone());
        slot.reserved = true;
        handle
    }
    fn available(&self) -> bool {
        self.coordinator.is_some()
            || (self.config.helper.as_ref().is_some_and(|p| p.is_file())
                && self.config.device.is_some()
                && self
                    .probe_key
                    .as_ref()
                    .is_some_and(|k| k.0 == self.engine.connection_generation())
                && matches!(self.profile_result, Some(Ok(()))))
    }
    fn unavailable_reason(&self) -> Option<String> {
        if self.coordinator.is_some() {
            return None;
        }
        if self
            .probe_key
            .as_ref()
            .is_none_or(|k| k.0 != self.engine.connection_generation())
        {
            return Some("Checking soundcheck profile".into());
        }
        match &self.profile_result {
            Some(Err(e)) => Some(e.clone()),
            None => Some("Checking soundcheck profile".into()),
            Some(Ok(())) if self.config.helper.as_ref().is_none_or(|p| !p.is_file()) => {
                Some("playback helper unavailable".into())
            }
            Some(Ok(())) => None,
        }
    }
    fn configure(&mut self, helper: Option<PathBuf>, device: Option<DeviceInfo>) {
        if let Some(probe) = self.probe.take() {
            probe.abort();
        }
        self.probe_key = None;
        self.profile_result = None;
        self.config.helper = helper;
        self.config.device = device;
    }
    fn state(&self) -> SoundcheckState {
        if self.config.root.join("manual-routing-pending").exists() {
            return SoundcheckState {
                input_mode: if self.manual.is_some() {
                    "soundcheck"
                } else {
                    "manual_recovery_required"
                }
                .into(),
                transport: if self.manual.is_none()
                    && openfader_audio::playback_guard(&self.config.root).is_err()
                {
                    "unknown".into()
                } else {
                    self.manual_transport.clone()
                },
            };
        }
        let mode = self
            .coordinator
            .as_ref()
            .map(|c| c.mode())
            .unwrap_or_else(|| {
                if self.config.journal.exists() {
                    InputMode::RecoveryRequired
                } else {
                    InputMode::Live
                }
            });
        let transport = self
            .coordinator
            .as_ref()
            .map(|c| c.transport())
            .unwrap_or(Transport::Stopped);
        SoundcheckState {
            input_mode: match mode {
                InputMode::Live => "live",
                InputMode::Soundcheck => "soundcheck",
                InputMode::RecoveryRequired => "recovery_required",
            }
            .into(),
            transport: match transport {
                Transport::Stopped => "stopped",
                Transport::Playing => "playing",
                Transport::Paused => "paused",
            }
            .into(),
        }
    }
    fn telemetry(&mut self) -> Pin<Box<dyn Future<Output = Option<Value>> + Send + '_>> {
        Box::pin(async move {
            if let Some(c) = &self.manual {
                match c.status().await {
                    Ok(v) if valid_playback_status(&v["status"]) => {
                        let mut status = v["status"].clone();
                        status["manual_routing"] = Value::Bool(true);
                        if self.manual_transport == "stopped" {
                            status["transport"] = Value::String("stopped".into());
                        }
                        self.manual_transport =
                            status["transport"].as_str().unwrap_or("unknown").to_owned();
                        self.telemetry = Some(status);
                    }
                    Ok(_) => {
                        self.manual_transport = "unknown".into();
                        self.telemetry = Some(serde_json::json!({
                            "manual_routing": true,
                            "playback_error": "invalid playback status",
                            "transport": "unknown"
                        }));
                    }
                    Err(e) => {
                        self.manual_transport = "unknown".into();
                        self.telemetry = Some(
                            serde_json::json!({"manual_routing":true,"playback_error":e,"transport":"unknown"}),
                        );
                    }
                }
                return self.telemetry.clone();
            }
            if self.coordinator.is_none() {
                let key = self.config.device.as_ref().map(|d| {
                    (
                        self.engine.connection_generation(),
                        d.id.clone(),
                        d.input_channels,
                        d.output_channels,
                        d.sample_rate,
                    )
                });
                if key != self.probe_key {
                    if let Some(probe) = self.probe.take() {
                        probe.abort();
                    }
                    self.probe_key = key.clone();
                    self.profile_result = None;
                }
                if self.probe.as_ref().is_some_and(|p| p.is_finished()) {
                    if let Some(probe) = self.probe.take() {
                        self.profile_result = Some(
                            probe
                                .await
                                .unwrap_or_else(|e| Err(format!("profile probe failed: {e}"))),
                        );
                    }
                }
                if self.probe.is_none() && self.profile_result.is_none() {
                    if let Some(device) = self.config.device.clone() {
                        let engine = self.engine.clone();
                        self.probe = Some(tokio::spawn(async move {
                            match tokio::time::timeout(Duration::from_secs(5), async {
                                let session = engine.selector_session().await?;
                                let profile =
                                    session.soundcheck_profile().await?.ok_or_else(|| {
                                        anyhow::anyhow!(
                                            "connected driver does not provide soundcheck routing"
                                        )
                                    })?;
                                anyhow::ensure!(
                                    device.output_channels >= profile.output_channels,
                                    "insufficient playback outputs"
                                );
                                let descriptor = session.recorder_descriptor().await?.ok_or_else(|| anyhow::anyhow!("The connected provider has no verified audio interface association; use manual virtual soundcheck"))?;
                                anyhow::ensure!(crate::recorder::recorder_device_matches(&descriptor, &device), "The selected audio interface does not match the connected provider; use manual virtual soundcheck");
                                Ok::<_, anyhow::Error>(())
                            })
                            .await
                            {
                                Ok(result) => result.map_err(|e| format!("{e:#}")),
                                Err(_) => Err("soundcheck profile check timed out".into()),
                            }
                        }));
                    }
                }
            }
            if let Some(c) = &self.coordinator {
                if c.mode() == InputMode::Soundcheck && c.has_playback() {
                    match tokio::time::timeout(Duration::from_secs(2), c.status()).await {
                        Ok(Ok(response)) if valid_playback_status(&response["status"]) => {
                            let mut status = response["status"].clone();
                            if c.transport() == Transport::Stopped {
                                status["transport"] = serde_json::json!("stopped");
                            }
                            self.telemetry = Some(status);
                        }
                        result => {
                            let error = match result {
                                Ok(Err(e)) => format!("{e:#}"),
                                Ok(Ok(_)) => "invalid playback status".into(),
                                Err(_) => "playback status timed out".into(),
                            };
                            let status =
                                self.telemetry.get_or_insert_with(|| serde_json::json!({}));
                            status["transport"] = serde_json::json!("unknown");
                            status["playback_error"] = serde_json::json!(error);
                        }
                    }
                }
            }
            self.telemetry.clone()
        })
    }
    fn command<'a>(
        &'a mut self,
        action: &'a str,
        args: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { self.run(action, args).await.map_err(|e| format!("{e:#}")) })
    }
}

#[cfg(test)]
mod preflight_tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn manual_mapping_and_recovery_without_mixer() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "manual-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut w = openfader_audio::TakeWriter::create_with_layout(
            root.join("take"),
            48000,
            100,
            openfader_audio::ChannelLayout::new(3).unwrap(),
        )
        .unwrap();
        w.append_interleaved(&[1, 2, 3]).unwrap();
        w.finish().unwrap();
        let helper = root.join("helper");
        std::fs::write(&helper, "#!/bin/sh\nread line\nprintf '{\"state\":\"READY\",\"generation\":1}\n'\nwhile read line; do case \"$line\" in *play*) printf '{\"state\":\"PLAYED\",\"generation\":1}\n';; *pause*) printf '{\"state\":\"PAUSED\",\"generation\":1}\n';; *status*) printf '{\"state\":\"STATUS\",\"generation\":1,\"status\":42}\n';; *stop*) printf '{\"state\":\"STOPPED\",\"generation\":1}\n'; exit 0;; esac; done\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let engine = Engine::new(openfader_engine::EngineConfig::default());
        let mut b = CoordinatorBackend::new(
            engine.clone(),
            BackendConfig::new(root.join("journal"), root.clone()),
        );
        b.configure(
            Some(helper),
            Some(DeviceInfo {
                id: "fake".into(),
                name: "fake".into(),
                input_channels: 0,
                output_channels: 2,
                sample_rate: 48000,
            }),
        );
        for mapping in [
            serde_json::json!([0, 0, null]),
            serde_json::json!([0, 2, null]),
            serde_json::json!([null, null, null]),
        ] {
            assert!(b.command("enter_manual_soundcheck", &serde_json::json!({"take_id":"take","routing_confirmed":true,"expected_device_id":"fake","output_mapping":mapping})).await.is_err());
            assert!(root.join("manual-routing-pending").exists());
            b.command(
                "return_manual_live",
                &serde_json::json!({"routing_restored":true}),
            )
            .await
            .unwrap();
        }
        b.command("enter_manual_soundcheck", &serde_json::json!({"take_id":"take","routing_confirmed":true,"expected_device_id":"fake","output_mapping":[1,null,0]})).await.unwrap();
        assert_eq!(b.state().input_mode, "soundcheck");
        let status = b.telemetry().await.expect("malformed status is reported");
        assert_eq!(status["transport"], "unknown");
        assert_eq!(status["playback_error"], "invalid playback status");
        assert!(!root.join("journal").exists());
        assert!(b
            .command(
                "return_manual_live",
                &serde_json::json!({"routing_restored":true})
            )
            .await
            .is_err());
        b.command("stop_manual_output", &serde_json::json!({}))
            .await
            .unwrap();
        drop(b);
        let mut recovered = CoordinatorBackend::new(
            engine,
            BackendConfig::new(root.join("journal"), root.clone()),
        );
        assert_eq!(recovered.state().input_mode, "manual_recovery_required");
        let blocked = openfader_audio::playback_guard(&root).unwrap();
        assert_eq!(recovered.state().transport, "unknown");
        assert!(recovered
            .command("stop_manual_output", &serde_json::json!({}))
            .await
            .is_err());
        assert!(recovered
            .command(
                "return_manual_live",
                &serde_json::json!({"routing_restored":true})
            )
            .await
            .is_err());
        drop(blocked);
        recovered
            .command("stop_manual_output", &serde_json::json!({}))
            .await
            .unwrap();
        assert!(recovered
            .command("return_manual_live", &serde_json::json!({}))
            .await
            .is_err());
        recovered
            .command(
                "return_manual_live",
                &serde_json::json!({"routing_restored":true}),
            )
            .await
            .unwrap();
        assert_eq!(recovered.state().input_mode, "live");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn accepting_restored_legacy_routing_archives_without_changing_routes() {
        let engine = Engine::new(openfader_engine::EngineConfig::default());
        engine.connect("sim", None).await.unwrap();
        let (identity, before) = {
            let session = engine.selector_session().await.unwrap();
            (
                serde_json::to_string(session.identity()).unwrap(),
                session.read_input_route(0).await.unwrap(),
            )
        };
        let root = std::env::temp_dir().join(format!(
            "openfader-archive-command-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let journal = root.join("routing-journal.json");
        let original = serde_json::to_vec(&serde_json::json!({
            "version": 3, "identity": identity,
            "changes": [{"key":"input/0/bank", "before":before.bank,
                "after":1-before.bank, "attempted":true, "skipped":false}]
        }))
        .unwrap();
        std::fs::write(&journal, &original).unwrap();
        let mut backend = CoordinatorBackend::new(
            engine.clone(),
            BackendConfig::new(journal.clone(), root.clone()),
        );
        assert!(backend
            .command("accept_current_routing", &serde_json::json!({}))
            .await
            .is_err());
        assert!(journal.exists());
        backend
            .command(
                "accept_current_routing",
                &serde_json::json!({"accept_current":true}),
            )
            .await
            .unwrap();
        assert!(!journal.exists());
        assert_eq!(backend.state().input_mode, "live");
        let archive = std::fs::read_dir(root.join("recoveryarchive"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(archive).unwrap(), original);
        let session = engine.selector_session().await.unwrap();
        assert_eq!(session.read_input_route(0).await.unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn generic_32_channel_profile_reaches_playback_prepare() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "openfader-prepare-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut writer = openfader_audio::TakeWriter::create_with_layout(
            root.join("take"),
            48000,
            100,
            openfader_audio::ChannelLayout::new(32).unwrap(),
        )
        .unwrap();
        writer.append_interleaved(&[0; 32]).unwrap();
        writer.finish().unwrap();
        let helper = root.join("helper");
        let captured = root.join("prepare.json");
        std::fs::write(&helper, format!("#!/bin/sh\nread line\nprintf '%s' \"$line\" > '{}'\nprintf '{{\"state\":\"READY\",\"generation\":1}}\\n'\nwhile read line; do case \"$line\" in *play*) printf '{{\"state\":\"PLAYED\",\"generation\":1}}\\n';; *stop*) printf '{{\"state\":\"STOPPED\",\"generation\":1}}\\n'; exit 0;; esac; done\n", captured.display())).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let engine = Engine::new(openfader_engine::EngineConfig::default());
        engine.connect("sim", None).await.unwrap();
        let mut backend = CoordinatorBackend::new(
            engine,
            BackendConfig::new(root.join("journal"), root.clone()),
        );
        backend.configure(
            Some(helper),
            Some(DeviceInfo {
                id: "generic".into(),
                name: "generic".into(),
                input_channels: 32,
                output_channels: 32,
                sample_rate: 48000,
            }),
        );
        backend
            .command("enter_soundcheck", &serde_json::json!({"take_id":"take"}))
            .await
            .unwrap();
        let request: Value = serde_json::from_slice(&std::fs::read(captured).unwrap()).unwrap();
        assert_eq!(request["output_mask"].as_array().unwrap().len(), 32);
        assert_eq!(request["device"]["output_channels"], 32);
        backend
            .command("return_live", &serde_json::json!({}))
            .await
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
