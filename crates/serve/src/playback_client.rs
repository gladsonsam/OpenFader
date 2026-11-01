//! Isolated playback-helper client. All process I/O runs on one dedicated thread.
use crate::soundcheck_backend::CancellationHandle;
use openfader_audio::{process_supervisor::ProcessSupervisor, DeviceInfo};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::oneshot;

const MAX_MESSAGE: usize = 16 * 1024;
const OP_TIMEOUT: Duration = Duration::from_secs(2);
type Reply = oneshot::Sender<Result<Value, String>>;
struct Request {
    value: Value,
    state: &'static str,
    deadline: Instant,
    reply: Reply,
}

/// A preparation handle remains cancellable while the helper is starting.
pub struct PlaybackPreparation {
    client: Option<PlaybackClient>,
    ready: oneshot::Receiver<Result<Value, String>>,
}
pub struct PlaybackClient {
    generation: u64,
    tx: mpsc::SyncSender<Request>,
    cancelled: Arc<AtomicBool>,
    terminated: Arc<AtomicBool>,
}
impl PlaybackClient {
    pub fn prepare(
        helper: PathBuf,
        device: DeviceInfo,
        root: PathBuf,
        take_id: String,
        generation: u64,
        deadline: Instant,
    ) -> PlaybackPreparation {
        Self::prepare_with_cancellation(
            helper,
            device,
            root,
            take_id,
            generation,
            deadline,
            CancellationHandle::default(),
        )
    }
    pub(crate) fn prepare_with_cancellation(
        helper: PathBuf,
        device: DeviceInfo,
        root: PathBuf,
        take_id: String,
        generation: u64,
        deadline: Instant,
        cancellation: CancellationHandle,
    ) -> PlaybackPreparation {
        let mask = vec![true; usize::from(device.output_channels)];
        Self::prepare_masked(
            helper,
            device,
            root,
            take_id,
            generation,
            deadline,
            cancellation,
            mask,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_masked(
        helper: PathBuf,
        device: DeviceInfo,
        root: PathBuf,
        take_id: String,
        generation: u64,
        deadline: Instant,
        cancellation: CancellationHandle,
        output_mask: Vec<bool>,
    ) -> PlaybackPreparation {
        Self::prepare_mapped(
            helper,
            device,
            root,
            take_id,
            generation,
            deadline,
            cancellation,
            output_mask,
            None,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_mapped(
        helper: PathBuf,
        device: DeviceInfo,
        root: PathBuf,
        take_id: String,
        generation: u64,
        deadline: Instant,
        cancellation: CancellationHandle,
        output_mask: Vec<bool>,
        output_mapping: Option<Vec<Option<usize>>>,
        manual_session: Option<String>,
    ) -> PlaybackPreparation {
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        let (ready_tx, ready) = oneshot::channel();
        let cancelled = cancellation.requested.clone();
        let terminated = cancellation.terminated.clone();
        let worker_terminated = terminated.clone();
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            let run = || -> Result<(), String> {
                let mut child = match ProcessSupervisor::spawn(helper, std::iter::empty::<&str>()) {
                    Ok(child) => child,
                    Err(error) => {
                        worker_terminated.store(true, Ordering::Release);
                        return Err(error.to_string());
                    }
                };
                let execute = || -> Result<(), String> {
                    let prepare = json!({"op":"prepare_playback", "device":device, "root":root, "take_id":take_id, "generation":generation, "output_mask":output_mask, "output_mapping":output_mapping, "manual_session":manual_session});
                    let result =
                        exchange(&mut child, &prepare, "READY", generation, deadline, &flag);
                    let ok = result.is_ok();
                    if ready_tx.send(result).is_err() || !ok {
                        return Ok(());
                    }
                    loop {
                        let request = match rx.recv_timeout(Duration::from_millis(20)) {
                            Ok(request) => request,
                            Err(mpsc::RecvTimeoutError::Timeout)
                                if !flag.load(Ordering::Acquire) =>
                            {
                                continue
                            }
                            Err(_) => break,
                        };
                        if flag.load(Ordering::Acquire) {
                            break;
                        }
                        let mut result = exchange(
                            &mut child,
                            &request.value,
                            request.state,
                            generation,
                            request.deadline,
                            &flag,
                        );
                        let terminal = result.is_err() || request.state == "STOPPED";
                        if request.state == "STOPPED" && result.is_ok() {
                            match child.stop(request.deadline) {
                                Ok(_) => worker_terminated.store(true, Ordering::Release),
                                Err(error) => result = Err(error.to_string()),
                            }
                        }
                        let _ = request.reply.send(result);
                        if terminal {
                            break;
                        }
                    }
                    Ok(())
                };
                let result = execute();
                if child.stop(Instant::now()).is_ok() {
                    worker_terminated.store(true, Ordering::Release);
                }
                result
            };
            // Dropping the ready sender also reports spawn failures to the caller.
            let _ = run();
        });
        PlaybackPreparation {
            client: Some(Self {
                generation,
                tx,
                cancelled,
                terminated,
            }),
            ready,
        }
    }
    async fn request(
        &self,
        value: Value,
        state: &'static str,
        deadline: Instant,
    ) -> Result<Value, String> {
        if self.cancelled.load(Ordering::Acquire) || self.terminated.load(Ordering::Acquire) {
            return Err("playback cancelled or terminated".into());
        }
        let (reply, rx) = oneshot::channel();
        self.tx
            .try_send(Request {
                value,
                state,
                deadline,
                reply,
            })
            .map_err(|e| e.to_string())?;
        rx.await
            .map_err(|_| "playback helper terminated".to_string())?
    }
    pub async fn play(&self, generation: u64) -> Result<Value, String> {
        self.command(
            generation,
            json!({"op":"play", "generation":generation}),
            "PLAYED",
        )
        .await
    }
    pub async fn pause(&self, generation: u64) -> Result<Value, String> {
        self.command(
            generation,
            json!({"op":"pause", "generation":generation}),
            "PAUSED",
        )
        .await
    }
    pub async fn seek(&self, generation: u64, frame: u64) -> Result<Value, String> {
        self.command(
            generation,
            json!({"op":"seek", "generation":generation, "frame":frame}),
            "SEEKED",
        )
        .await
    }
    pub async fn set_loop(
        &self,
        generation: u64,
        start: u64,
        end: u64,
        enabled: bool,
    ) -> Result<Value, String> {
        self.command(generation, json!({"op":"set_loop", "generation":generation, "start":start, "end":end, "enabled":enabled}), "LOOP_SET").await
    }
    async fn command(
        &self,
        generation: u64,
        value: Value,
        state: &'static str,
    ) -> Result<Value, String> {
        if generation != self.generation {
            return Err("stale generation".into());
        }
        self.request(value, state, Instant::now() + OP_TIMEOUT)
            .await
    }
    pub async fn status(&self) -> Result<Value, String> {
        self.request(
            json!({"op":"status", "generation":self.generation}),
            "STATUS",
            Instant::now() + OP_TIMEOUT,
        )
        .await
    }
    pub async fn stop(&self, deadline: Instant) -> Result<Value, String> {
        if self.terminated.load(Ordering::Acquire) {
            return Ok(Value::Null);
        }
        let result = self
            .request(
                json!({"op":"stop", "generation":self.generation}),
                "STOPPED",
                deadline,
            )
            .await;
        if self.terminated.load(Ordering::Acquire) {
            Ok(Value::Null)
        } else {
            result
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Drop for PlaybackClient {
    fn drop(&mut self) {
        // An acknowledged and reaped Stop is normal completion, not a user
        // cancellation. Keep the operation's cancellation result truthful.
        if !self.terminated.load(Ordering::Acquire) {
            self.cancel();
        }
    }
}
impl PlaybackPreparation {
    pub fn cancel(&self) {
        if let Some(client) = &self.client {
            client.cancel();
        }
    }
    pub async fn ready(mut self) -> Result<PlaybackClient, String> {
        self.ready
            .await
            .map_err(|_| "playback helper terminated during preparation".to_string())??;
        let client = self.client.take().expect("preparation owns client");
        if client.cancelled.load(Ordering::Acquire) {
            return Err("playback cancelled".into());
        }
        Ok(client)
    }
}
fn exchange(
    child: &mut ProcessSupervisor,
    value: &Value,
    state: &str,
    generation: u64,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Value, String> {
    if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
        return Err("playback cancelled or deadline expired".into());
    }
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > MAX_MESSAGE {
        return Err("playback command exceeds limit".into());
    }
    child.send(value).map_err(|e| e.to_string())?;
    let response = child
        .wait_for_ready_cancelled(
            deadline,
            MAX_MESSAGE,
            |_| true,
            || cancelled.load(Ordering::Acquire),
        )
        .map_err(|e| e.to_string())?;
    if cancelled.load(Ordering::Acquire) {
        return Err("playback cancelled".into());
    }
    if response["state"] != state || response["generation"].as_u64() != Some(generation) {
        return Err(format!("unexpected helper response: {response}"));
    }
    Ok(response)
}

/// Generation-bound playback control. Every future resolves only after the helper
/// acknowledges the operation or its deadline expires. Dropping the client cancels
/// and reaps the helper. Status contains the service's callback frame position,
/// transport state and xrun count.
///
/// The helper uses newline-delimited JSON: `prepare_playback` includes `device`,
/// `root`, `take_id`, and `generation` and replies `READY` without opening output.
/// `play`, `pause`, `seek` (`frame`), `set_loop` (`start`, `end`, `enabled`),
/// `status`, and `stop` all carry that generation. Replies carry `generation`,
/// `state`, and, except READY, `status`; their states are respectively PLAYED,
/// PAUSED, SEEKED, LOOP_SET, STATUS, and STOPPED. Errors terminate the session.
#[allow(async_fn_in_trait)]
pub trait PlaybackTransport {
    async fn play(&self, generation: u64) -> Result<Value, String>;
    async fn pause(&self, generation: u64) -> Result<Value, String>;
    async fn seek(&self, generation: u64, frame: u64) -> Result<Value, String>;
    async fn set_loop(
        &self,
        generation: u64,
        start: u64,
        end: u64,
        enabled: bool,
    ) -> Result<Value, String>;
    async fn status(&self) -> Result<Value, String>;
    async fn stop(&self, deadline: Instant) -> Result<Value, String>;
}
impl PlaybackTransport for PlaybackClient {
    async fn play(&self, generation: u64) -> Result<Value, String> {
        PlaybackClient::play(self, generation).await
    }
    async fn pause(&self, generation: u64) -> Result<Value, String> {
        PlaybackClient::pause(self, generation).await
    }
    async fn seek(&self, generation: u64, frame: u64) -> Result<Value, String> {
        PlaybackClient::seek(self, generation, frame).await
    }
    async fn set_loop(
        &self,
        generation: u64,
        start: u64,
        end: u64,
        enabled: bool,
    ) -> Result<Value, String> {
        PlaybackClient::set_loop(self, generation, start, end, enabled).await
    }
    async fn status(&self) -> Result<Value, String> {
        PlaybackClient::status(self).await
    }
    async fn stop(&self, deadline: Instant) -> Result<Value, String> {
        PlaybackClient::stop(self, deadline).await
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[tokio::test]
    async fn cloned_handle_interrupts_preparation_and_reaps() {
        let path = std::env::temp_dir().join(format!("openfader-blocked-{}", std::process::id()));
        let pid_path = path.with_extension("pid");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\necho $$ > '{}'
exec sleep 30\n",
                pid_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let handle = CancellationHandle::default();
        let device = DeviceInfo {
            id: "fake".into(),
            name: "fake".into(),
            input_channels: 18,
            output_channels: 18,
            sample_rate: 48000,
        };
        let preparation = PlaybackClient::prepare_with_cancellation(
            path.clone(),
            device,
            PathBuf::from("/tmp"),
            "take".into(),
            1,
            Instant::now() + Duration::from_secs(2),
            handle.clone(),
        );
        let pending = preparation.ready();
        for _ in 0..100 {
            if pid_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let pid = fs::read_to_string(&pid_path).unwrap();
        handle.clone().cancel();
        assert!(tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .is_err());
        for _ in 0..100 {
            if handle.output_terminated() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(handle.cancellation_requested());
        assert!(handle.output_terminated());
        assert!(!std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .unwrap()
            .success());
        let next = CancellationHandle::default();
        handle.cancel();
        assert!(!next.cancellation_requested());
        fs::remove_file(path).unwrap();
        fs::remove_file(pid_path).unwrap();
    }

    #[tokio::test]
    async fn failure_before_ready_reports_termination_after_reap() {
        let path =
            std::env::temp_dir().join(format!("openfader-fail-ready-{}", std::process::id()));
        let pid_path = path.with_extension("pid");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\necho $$ > '{}'\necho invalid\nexec sleep 30\n",
                pid_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let handle = CancellationHandle::default();
        let device = DeviceInfo {
            id: "fake".into(),
            name: "fake".into(),
            input_channels: 18,
            output_channels: 18,
            sample_rate: 48000,
        };
        assert!(PlaybackClient::prepare_with_cancellation(
            path.clone(),
            device,
            PathBuf::from("/tmp"),
            "take".into(),
            1,
            Instant::now() + Duration::from_secs(2),
            handle.clone()
        )
        .ready()
        .await
        .is_err());
        for _ in 0..100 {
            if handle.output_terminated() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(handle.output_terminated());
        let pid = fs::read_to_string(&pid_path).unwrap();
        assert!(!std::process::Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .unwrap()
            .success());
        fs::remove_file(path).unwrap();
        fs::remove_file(pid_path).unwrap();
    }

    #[tokio::test]
    async fn cancellation_alone_does_not_confirm_stop() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let client = PlaybackClient {
            generation: 1,
            tx,
            cancelled: Arc::new(AtomicBool::new(true)),
            terminated: Arc::new(AtomicBool::new(false)),
        };
        assert!(client.stop(Instant::now()).await.is_err());
        client.terminated.store(true, Ordering::Release);
        assert!(client.stop(Instant::now()).await.is_ok());
    }

    #[tokio::test]
    async fn delayed_stop_is_retryable_only_after_reap() {
        let path =
            std::env::temp_dir().join(format!("openfader-delay-stop-{}", std::process::id()));
        fs::write(&path, "#!/bin/sh\nread line\nprintf '{\"state\":\"READY\",\"generation\":1}\\n'\nread line\nexec sleep 30\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let handle = CancellationHandle::default();
        let device = DeviceInfo {
            id: "fake".into(),
            name: "fake".into(),
            input_channels: 18,
            output_channels: 18,
            sample_rate: 48000,
        };
        let client = PlaybackClient::prepare_with_cancellation(
            path.clone(),
            device,
            PathBuf::from("/tmp"),
            "take".into(),
            1,
            Instant::now() + Duration::from_secs(2),
            handle.clone(),
        )
        .ready()
        .await
        .unwrap();
        let _ = client
            .stop(Instant::now() + Duration::from_millis(50))
            .await;
        for _ in 0..200 {
            if handle.output_terminated() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(handle.output_terminated());
        client.stop(Instant::now()).await.unwrap();
        fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn fake_helper_protocol_and_generation() {
        let path =
            std::env::temp_dir().join(format!("openfader-playback-fake-{}", std::process::id()));
        fs::write(&path, r##"#!/bin/sh
read line
case "$line" in *'"output_mask":[true,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]'*) ;; *) exit 2;; esac
printf '{"state":"READY","generation":7}\n'
while read line; do
 case "$line" in
 *'"op":"play"'*) printf '{"state":"PLAYED","generation":7,"status":{"position_frames":0}}\n';;
 *'"op":"status"'*) printf '{"state":"STATUS","generation":7,"status":{"position_frames":42,"transport":"stopped"}}\n';;
 *'"op":"stop"'*) printf '{"state":"STOPPED","generation":7}\n'; exit 0;;
 *) exit 3;;
 esac
done
"##).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        let device = DeviceInfo {
            id: "fake".into(),
            name: "fake".into(),
            input_channels: 18,
            output_channels: 18,
            sample_rate: 48000,
        };
        let handle = CancellationHandle::default();
        let client = PlaybackClient::prepare_masked(
            path.clone(),
            device,
            PathBuf::from("/tmp"),
            "take".into(),
            7,
            Instant::now() + Duration::from_secs(2),
            handle.clone(),
            (0..18).map(|i| i == 0).collect(),
        )
        .ready()
        .await
        .unwrap();
        assert!(client.play(8).await.is_err());
        client.play(7).await.unwrap();
        let status = client.status().await.unwrap();
        assert_eq!(status["status"]["position_frames"], 42);
        assert_eq!(status["status"]["transport"], "stopped");
        client
            .stop(Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        assert!(handle.output_terminated());
        assert!(!handle.cancellation_requested());
        drop(client);
        assert!(!handle.cancellation_requested());
        fs::remove_file(path).unwrap();
    }
}
