//! Isolated capture-helper client. All process I/O runs on one dedicated thread.
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
pub struct CapturePreparation {
    client: Option<CaptureClient>,
    ready: oneshot::Receiver<Result<Value, String>>,
}
pub struct CaptureClient {
    generation: u64,
    tx: mpsc::SyncSender<Request>,
    cancelled: Arc<AtomicBool>,
}
impl CaptureClient {
    pub fn prepare(
        helper: PathBuf,
        device: DeviceInfo,
        root: PathBuf,
        generation: u64,
        deadline: Instant,
    ) -> CapturePreparation {
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        let (ready_tx, ready) = oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        std::thread::spawn(move || {
            let run = || -> Result<(), String> {
                let mut child = ProcessSupervisor::spawn(helper, std::iter::empty::<&str>())
                    .map_err(|e| e.to_string())?;
                let prepare =
                    json!({"op":"prepare", "device":device, "root":root, "generation":generation});
                let result = exchange(&mut child, &prepare, "READY", generation, deadline, &flag);
                let ok = result.is_ok();
                if ready_tx.send(result).is_err() || !ok {
                    return Ok(());
                }
                loop {
                    let request = match rx.recv_timeout(Duration::from_millis(20)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) if !flag.load(Ordering::Acquire) => {
                            continue
                        }
                        Err(_) => break,
                    };
                    if flag.load(Ordering::Acquire) {
                        break;
                    }
                    let result = exchange(
                        &mut child,
                        &request.value,
                        request.state,
                        generation,
                        request.deadline,
                        &flag,
                    );
                    let terminal = result.is_err() || request.state == "STOPPED";
                    if request.state == "STOPPED" && result.is_ok() {
                        let _ = child.stop(request.deadline);
                    }
                    let _ = request.reply.send(result);
                    if terminal {
                        break;
                    }
                }
                Ok(())
            };
            // Dropping the ready sender also reports spawn failures to the caller.
            let _ = run();
            flag.store(true, Ordering::Release);
        });
        CapturePreparation {
            client: Some(Self {
                generation,
                tx,
                cancelled,
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
        if self.cancelled.load(Ordering::Acquire) {
            return Err("capture cancelled or terminated".into());
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
            .map_err(|_| "capture helper terminated".to_string())?
    }
    pub async fn commit(&self, generation: u64, take_id: &str) -> Result<Value, String> {
        if generation != self.generation {
            return Err("stale generation".into());
        }
        self.request(
            json!({"op":"commit", "generation":generation, "take_id":take_id}),
            "RECORDING",
            Instant::now() + OP_TIMEOUT,
        )
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
        self.request(
            json!({"op":"stop", "generation":self.generation}),
            "STOPPED",
            deadline,
        )
        .await
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
impl Drop for CaptureClient {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl CapturePreparation {
    pub fn cancel(&self) {
        if let Some(client) = &self.client {
            client.cancel();
        }
    }
    pub async fn ready(mut self) -> Result<CaptureClient, String> {
        self.ready
            .await
            .map_err(|_| "capture helper terminated during preparation".to_string())??;
        let client = self.client.take().expect("preparation owns client");
        if client.cancelled.load(Ordering::Acquire) {
            return Err("capture cancelled".into());
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
        return Err("capture cancelled or deadline expired".into());
    }
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > MAX_MESSAGE {
        return Err("capture command exceeds limit".into());
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
        return Err("capture cancelled".into());
    }
    if response["state"] != state || response["generation"].as_u64() != Some(generation) {
        return Err(format!("unexpected helper response: {response}"));
    }
    Ok(response)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn fixture(script: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "openfader-helper-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn device() -> DeviceInfo {
        DeviceInfo {
            id: "test".into(),
            name: "test".into(),
            input_channels: 18,
            output_channels: 0,
            sample_rate: 48000,
        }
    }
    fn prepare(path: PathBuf, deadline: Instant) -> CapturePreparation {
        CaptureClient::prepare(path, device(), PathBuf::from("/tmp"), 7, deadline)
    }
    #[tokio::test]
    async fn protocol_and_stale_generation() {
        let path = fixture(
            r#"read line
printf '{"state":"READY","generation":7}\n'
while read line; do
 case "$line" in
 *commit*) printf '{"state":"RECORDING","generation":7,"status":{}}\n';;
 *status*) printf '{"state":"STATUS","generation":7,"status":{}}\n';;
 *stop*) printf '{"state":"STOPPED","generation":7,"status":{}}\n'; exit 0;;
 esac
done"#,
        );
        let client = prepare(path.clone(), Instant::now() + Duration::from_secs(2))
            .ready()
            .await
            .unwrap();
        assert!(client.commit(8, "take").await.is_err());
        assert_eq!(
            client.commit(7, "take").await.unwrap()["state"],
            "RECORDING"
        );
        assert_eq!(client.status().await.unwrap()["state"], "STATUS");
        assert_eq!(
            client
                .stop(Instant::now() + Duration::from_secs(2))
                .await
                .unwrap()["state"],
            "STOPPED"
        );
        fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn timeout_and_crash() {
        let path = fixture("read line; exec sleep 30");
        assert!(
            prepare(path.clone(), Instant::now() + Duration::from_millis(100))
                .ready()
                .await
                .is_err()
        );
        fs::remove_file(path).unwrap();
        let path = fixture("exit 1");
        assert!(
            prepare(path.clone(), Instant::now() + Duration::from_secs(2))
                .ready()
                .await
                .is_err()
        );
        fs::remove_file(path).unwrap();
    }
}
