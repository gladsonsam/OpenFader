//! XR18 RTA100, verified on FW1.20. See XR18 RTA verification notes.
//! This actor shares the driver OSC session; selector queries run concurrently
//! with hydration and never enter the strip poll budget. It is the sole writer for the shared selector. Other
//! desk clients can take over; we then relinquish ownership instead of fighting.
use openfader_core::{AudioSpectrum, StripId};
use openfader_transport::OscSession;
use rosc::{OscMessage, OscType};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

const SELECTOR: &str = "/-stat/rta";
const TIMEOUT: Duration = Duration::from_millis(300);
const SETTLE: Duration = Duration::from_millis(250);
type Cache = Arc<Mutex<Option<(Instant, AudioSpectrum)>>>;

struct Request {
    owner: String,
    strip: Option<StripId>,
    stop: bool,
    reply: oneshot::Sender<Result<(), String>>,
}

pub struct RtaStream {
    tx: mpsc::Sender<Request>,
    cache: Cache,
}

/// The official mixer control software 1.8 `_rtasrctxt` table, mapped to our topology.
fn source_strip(source: i32) -> Option<StripId> {
    Some(StripId(match source {
        0..=15 => source as u16,
        16 => 35,
        17..=20 => (source - 1) as u16,
        21..=26 => (source - 1) as u16,
        27..=30 => (source - 1) as u16,
        31 => 30,
        _ => return None, // Monitor has no strip.
    }))
}
fn strip_source(strip: StripId) -> anyhow::Result<i32> {
    match strip.0 {
        0..=15 => Ok(strip.0 as i32),
        16..=29 => Ok(strip.0 as i32 + 1),
        30 => Ok(31),
        35 => Ok(16),
        _ => anyhow::bail!("xair: this strip has no RTA audio source"),
    }
}
fn source_label(source: i32) -> String {
    match source {
        0..=15 => format!("Ch{:02}", source + 1),
        16 => "Aux".into(),
        17..=20 => format!("FX{} return", source - 16),
        21..=26 => format!("Bus{}", source - 20),
        27..=30 => format!("FX{} send", source - 26),
        31 => "Main LR".into(),
        32 => "Monitor".into(),
        _ => format!("source {source}"),
    }
}
fn selector(msg: &OscMessage) -> Option<(i32, i32)> {
    match msg.args.as_slice() {
        [OscType::Int(s @ 0..=32), OscType::Int(p @ 0..=1)] => Some((*s, *p)),
        _ => None,
    }
}
/// Exact count and length gate; never use the linear strip-meter decoder.
fn bins(msg: &OscMessage) -> Option<Vec<f32>> {
    if !matches!(msg.addr.as_str(), "/meters/4" | "meters/4") {
        return None;
    }
    let [OscType::Blob(blob)] = msg.args.as_slice() else {
        return None;
    };
    if blob.len() != 204 || i32::from_le_bytes(blob[..4].try_into().ok()?) != 100 {
        return None;
    }
    Some(
        blob[4..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| i16::from_le_bytes([v[0], v[1]]) as f32 / 256.0)
            .collect(),
    )
}
async fn read(session: &OscSession) -> anyhow::Result<(i32, i32)> {
    // A node SET emits an empty parent ACK at this same address. It is not
    // a selector value and must never satisfy association/readback.
    for _ in 0..3 {
        let msg = session.query_timeout(SELECTOR, TIMEOUT).await?;
        if let Some(value) = selector(&msg) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    anyhow::bail!("xair: no complete RTA selector readback")
}
async fn write(session: &OscSession, value: (i32, i32)) -> anyhow::Result<()> {
    session
        .send(SELECTOR, vec![OscType::Int(value.0), OscType::Int(value.1)])
        .await?;
    // The desk applies node writes asynchronously, after its empty ACK.
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        if read(session).await? == value {
            return Ok(());
        }
    }
    anyhow::bail!("xair: RTA selector readback mismatch")
}

struct Lease {
    owner: String,
    original: (i32, i32),
    selected: (i32, i32),
}

impl RtaStream {
    pub fn start(session: Arc<OscSession>, generation: u64) -> Self {
        let (tx, mut requests) = mpsc::channel::<Request>(16);
        let cache: Cache = Arc::new(Mutex::new(None));
        let shared = cache.clone();
        tokio::spawn(async move {
            let mut updates = session.subscribe_updates();
            let mut lease: Option<Lease> = None;
            let mut current = None;
            let mut confirmed = Instant::now() - Duration::from_secs(10);
            let mut ready = Instant::now() + SETTLE;
            let mut epoch = generation << 32;
            let mut check = tokio::time::interval(Duration::from_millis(500));
            check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut renewal = tokio::time::interval(Duration::from_secs(5));
            renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    req = requests.recv() => {
                        // Closing the driver/client restores only a still-owned selector.
                        let Some(req) = req else { break; };
                        let result: anyhow::Result<()> = async {
                            if req.strip.is_none() {
                                if lease.as_ref().is_some_and(|l| req.owner.is_empty() || l.owner == req.owner) {
                                    let old = lease.take().unwrap();
                                    *shared.lock().unwrap() = None;
                                    current = None;
                                    // Compare before restoring; never overwrite another client's move.
                                    if read(&session).await? == old.selected { write(&session, old.original).await?; }
                                    ready = Instant::now() + SETTLE;
                                    epoch += 1;
                                }
                                return Ok(());
                            }
                            let source = strip_source(req.strip.unwrap())?;
                            anyhow::ensure!(lease.as_ref().is_none_or(|l| l.owner == req.owner), "RTA is in use by another openfader client");
                            let before = read(&session).await?;
                            if let Some(old) = &lease {
                                if before != old.selected {
                                    lease = None;
                                    anyhow::bail!("RTA source changed in another desk client; reopen EQ to follow again");
                                }
                            }
                            let original = lease.as_ref().map_or(before, |l| l.original);
                            // PRE EQ is intentional: filter edits should not hide incoming energy.
                            let selected = (source, 0);
                            *shared.lock().unwrap() = None;
                            current = None;
                            epoch += 1;
                            // Record intent before sending, so a lost acknowledgement still
                            // permits compare-and-restore on release.
                            lease = Some(Lease { owner: req.owner.clone(), original, selected });
                            session.send("/meters", vec![OscType::String("/meters/4".into())]).await?;
                            write(&session, selected).await?;
                            // Discard pre-switch meter packets and our queued node ACK/
                            // leaf echoes. Future packets also pass the settle window.
                            while updates.try_recv().is_ok() {}
                            current = Some(selected);
                            confirmed = Instant::now();
                            ready = Instant::now() + SETTLE;
                            Ok(())
                        }.await;
                        if result.is_err() { *shared.lock().unwrap() = None; current = None; }
                        let _ = req.reply.send(result.map_err(|e| e.to_string()));
                        if req.stop { break; }
                    }
                    _ = renewal.tick() => {
                        if lease.is_none() { continue; }
                        let _ = session.send("/meters", vec![OscType::String("/meters/4".into())]).await;
                    }
                    _ = check.tick() => {
                        if lease.is_none() { continue; }
                        match read(&session).await {
                            Ok(value) => {
                                if current != Some(value) {
                                    *shared.lock().unwrap() = None;
                                    epoch += 1;
                                    ready = Instant::now() + SETTLE;
                                }
                                if lease.as_ref().is_some_and(|l| l.selected != value) { lease = None; }
                                current = lease.as_ref().filter(|l| l.selected == value).map(|_| value);
                                confirmed = Instant::now();
                            }
                            Err(_) => { current = None; *shared.lock().unwrap() = None; }
                        }
                    }
                    msg = updates.recv() => {
                        match msg {
                            Ok(msg) if msg.addr == SELECTOR => {
                                // Empty parent ACKs are not source changes.
                                let Some(value) = selector(&msg) else { continue; };
                                let value = Some(value);
                                if value != current {
                                    current = lease.as_ref().filter(|l| Some(l.selected) == value).map(|l| l.selected);
                                    epoch += 1; ready = Instant::now() + SETTLE;
                                    *shared.lock().unwrap() = None;
                                    if lease.as_ref().is_some_and(|l| Some(l.selected) != value) { lease = None; }
                                }
                            }
                            Ok(msg) if matches!(msg.addr.as_str(), "/-stat/rta/source" | "/-stat/rta/pos") => {
                                if let ([OscType::Int(value)], Some(selected)) = (msg.args.as_slice(), current) {
                                    let expected = if msg.addr.ends_with("/source") { selected.0 } else { selected.1 };
                                    if *value != expected {
                                        lease = None; current = None; epoch += 1;
                                        *shared.lock().unwrap() = None;
                                    }
                                }
                            }
                            Ok(msg) => {
                                if let (Some((source, pos)), Some(bins_db)) = (current, bins(&msg)) {
                                    if lease.is_none() || Instant::now() < ready || confirmed.elapsed() > Duration::from_millis(750) { continue; }
                                    let frame = AudioSpectrum {
                                        strip: source_strip(source), bins_db,
                                        // mixer control software renders 100 equal-width bars on its
                                        // log(20)..log(20000) axis: these are band EDGES.
                                        band_edges_hz: (0..=100).map(|i| 20.0 * 1000f32.powf(i as f32 / 100.0)).collect(),
                                        source: source_label(source),
                                        position: if pos == 0 { "pre EQ" } else { "post EQ" }.into(), epoch,
                                    };
                                    *shared.lock().unwrap() = Some((Instant::now(), frame));
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                current = None; *shared.lock().unwrap() = None;
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            *shared.lock().unwrap() = None;
            if let Some(old) = lease {
                if read(&session).await.ok() == Some(old.selected) {
                    let _ = write(&session, old.original).await;
                }
            }
            // Session drop stops receive + keepalive. Meter subscription expires
            // on the desk; no global subscription or audio settings are touched.
        });
        Self { tx, cache }
    }
    pub fn poll(&self) -> Vec<AudioSpectrum> {
        self.cache
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(at, _)| at.elapsed() < Duration::from_millis(750))
            .map(|(_, frame)| vec![frame.clone()])
            .unwrap_or_default()
    }
    pub async fn target(
        &self,
        owner: &str,
        strip: Option<StripId>,
        stop: bool,
    ) -> anyhow::Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Request {
                owner: owner.into(),
                strip,
                stop,
                reply,
            })
            .await
            .map_err(|_| anyhow::anyhow!("xair: RTA session closed"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("xair: RTA session closed"))?
            .map_err(anyhow::Error::msg)
    }
}
