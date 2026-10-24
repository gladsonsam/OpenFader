//! openfader-serve: thin adapter over [`openfader_engine::Engine`].
//!
//! Serves the static `ui/` directory over HTTP plus a WebSocket JSON mixer
//! API on one port. Every socket owns one engine client: inbound text lines
//! decode to [`Cmd`](openfader_engine::Cmd), live
//! [`Event`](openfader_engine::Event)s encode back to text (or to WS BINARY
//! frames for meter fan-out when the client opted in via `hello`). No mixer
//! state lives here, sessions, the driver bank, the poller, shows, pins
//! and the roster all live in the engine. The wire protocol is specified in
//! `crates/engine/README.md`.
//!
//! Realtime duties of this adapter (MR1):
//! - `TCP_NODELAY` on every accepted socket ([`configure_accepted`], wired
//!   into the accept loop below); no permessage-deflate anywhere, the
//!   upgrade path (`axum` over a raw hyper connection) never negotiates
//!   extensions, so frames go out uncompressed.
//! - Latest-only meter delivery: per client, per stream (`meters`, `gr`,
//!   `spectrum`) a bounded slot of 1 ([`Outbox`]). A slow socket's stale
//!   frame is replaced, never queued; drops are counted and exposed via the
//!   [`stats_json`] reply to `{"cmd":"stats"}`. Control frames (`state`,
//!   `params`, `locks`, …) keep reliable ordered delivery.
//! - Binary meter encoding for opted-in clients
//!   (`{"cmd":"hello","binary_meters":true}` → [`Event::to_binary`]).

use std::{
    collections::{HashMap, VecDeque},
    path::{Path as StdPath, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State as AxumState,
    },
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use tokio::sync::{mpsc, Mutex, Notify};

pub use openfader_engine::{LOCK_TTL, PARAM_POLL_INTERVAL};

#[derive(Clone, Debug)]
pub struct ServeOptions {
    /// Restrict this relay to in-memory simulation; disable LAN discovery.
    pub sim_only: bool,
    /// Initial driver: `sim` (default), `xair`, `dl32s`.
    pub driver: String,
    /// Initial mixer address (hardware drivers; sim ignores it).
    pub addr: Option<String>,
    /// Sim desk preset (`generic|xr18|dl32s|studio`). Hardware drivers ignore it.
    pub preset: Option<String>,
    /// Directory of static UI files. `None` = auto-detect (`ui/`).
    pub static_dir: Option<PathBuf>,
    /// R2 poller pipelining: max concurrent driver queries per poll tick
    /// (`--poll-inflight`; `None` = per-driver default: sim 4, xair 1).
    pub poll_inflight: Option<usize>,
    /// R2 scene library directory (`--data-dir`; persisted `scenes.json`,
    /// reloaded on restart). `None` = in-memory scenes only.
    pub data_dir: Option<PathBuf>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            sim_only: false,
            driver: "sim".to_string(),
            addr: None,
            preset: None,
            static_dir: None,
            poll_inflight: None,
            data_dir: None,
        }
    }
}

/// Socket options for an accepted connection (MR1): disable Nagle so
/// fader/meter frames go out immediately. No permessage-deflate is
/// configured anywhere (the upgrade path never negotiates extensions),
/// which this function documents by taking no compression parameter.
pub(crate) fn configure_accepted(stream: &tokio::net::TcpStream) -> std::io::Result<()> {
    stream.set_nodelay(true)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<std::collections::HashMap<String, String>>,
    AxumState(app): AxumState<AppState>,
) -> impl IntoResponse {
    let name = params.get("name").cloned();
    ws.on_upgrade(move |socket| handle_socket(socket, app.engine, name, app.sim_only))
}

/// Which meter stream a queued outbound frame belongs to (for drop/send
/// accounting). `Control` frames are never dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamKind {
    Meters,
    Spectrum,
    Gr,
    Control,
}

/// Per-connection latest-only outbox (MR1).
///
/// Meter-class events land in bounded slots of 1 (`meters`, per-tap
/// `taps` maps, per-channel `spectrum` / `gr` maps): storing a frame
/// while the previous one is still unsent replaces it and counts a drop.
/// Everything else queues in `control` FIFO with reliable ordered
/// delivery. The sender task drains control first, then one stream frame
/// per pass.
struct Outbox {
    binary: bool,
    /// R2 taps preference (`hello {"taps":true}`): taps frames encode as
    /// WS BINARY type 3 when set, JSON text otherwise.
    taps_binary: bool,
    seq: u32,
    meters: Option<openfader_engine::SharedEvent>,
    meters_sent: u64,
    meters_dropped: u64,
    /// R2 per-tap latest-only slots (one frame per tap id). Drops/sends
    /// fold into the meters counters (the `stats` shape is pinned).
    taps: HashMap<String, openfader_engine::SharedEvent>,
    spectrum: HashMap<u16, openfader_engine::SharedEvent>,
    spectrum_sent: u64,
    spectrum_dropped: u64,
    gr: HashMap<u16, openfader_engine::SharedEvent>,
    gr_sent: u64,
    gr_dropped: u64,
    control: VecDeque<openfader_engine::SharedEvent>,
    pump_closed: bool,
}

impl Outbox {
    fn new() -> Self {
        Self {
            binary: false,
            taps_binary: false,
            seq: 0,
            meters: None,
            meters_sent: 0,
            meters_dropped: 0,
            taps: HashMap::new(),
            spectrum: HashMap::new(),
            spectrum_sent: 0,
            spectrum_dropped: 0,
            gr: HashMap::new(),
            gr_sent: 0,
            gr_dropped: 0,
            control: VecDeque::new(),
            pump_closed: false,
        }
    }

    /// Store one engine event: meter-class events collapse into bounded
    /// slots of 1 (replacement counts a drop), everything else queues in
    /// the reliable control FIFO. The pump task calls this per mailbox
    /// event; it never blocks, so the engine mailbox stays drained even
    /// when the socket stalls. Frames carry their wire JSON serialized
    /// once at broadcast time and shared across clients (see
    /// [`openfader_engine::SharedEvent`]); the text path below reuses it
    /// instead of re-serializing per client.
    fn store(&mut self, frame: openfader_engine::SharedEvent) {
        use openfader_engine::Event as E;
        match &*frame.event {
            E::Meters { .. } => {
                if self.meters.replace(frame).is_some() {
                    self.meters_dropped += 1;
                }
            }
            E::Taps { tap, .. } => {
                let key = tap.clone();
                if self.taps.insert(key, frame).is_some() {
                    self.meters_dropped += 1;
                }
            }
            E::AudioSpectrum { .. } => {
                if self.spectrum.insert(u16::MAX, frame).is_some() {
                    self.spectrum_dropped += 1;
                }
            }
            E::Spectrum { ch, .. } => {
                let key = *ch;
                if self.spectrum.insert(key, frame).is_some() {
                    self.spectrum_dropped += 1;
                }
            }
            E::Gr { ch, .. } => {
                let key = *ch;
                if self.gr.insert(key, frame).is_some() {
                    self.gr_dropped += 1;
                }
            }
            _ => self.control.push_back(frame),
        }
    }

    fn queue_depth(&self) -> usize {
        self.control.len()
            + usize::from(self.meters.is_some())
            + self.taps.len()
            + self.spectrum.len()
            + self.gr.len()
    }

    fn is_empty(&self) -> bool {
        self.queue_depth() == 0
    }

    fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            meters_sent: self.meters_sent,
            meters_dropped: self.meters_dropped,
            gr_sent: self.gr_sent,
            gr_dropped: self.gr_dropped,
            spectrum_sent: self.spectrum_sent,
            spectrum_dropped: self.spectrum_dropped,
            queue_depth: self.queue_depth(),
            binary: self.binary,
        }
    }

    /// Next frame to put on the socket: control FIFO first, then one
    /// stream frame. Encodes binary when the client opted in and the event
    /// supports it ([`Event::to_binary`](openfader_engine::Event::to_binary));
    /// control frames (and `spectrum`, which stays JSON) always encode as
    /// text, so JSON stays byte-identical for clients that did not opt in.
    /// R2 taps frames encode binary (type 3) only for taps-negotiated
    /// clients and count into the meters stream (the `stats` shape is
    /// pinned, so taps fold into `meters_sent`/`meters_dropped`).
    fn take_next(
        &mut self,
        engine: &openfader_engine::Engine,
    ) -> Option<(Message, StreamKind)> {
        if let Some(frame) = self.control.pop_front() {
            return Some((Message::Text(frame.json.to_string().into()), StreamKind::Control));
        }
        let t_ms = engine.now_ms();
        if let Some(event) = self.meters.take() {
            return Some((encode_stream(event, &mut self.seq, t_ms, self.binary), StreamKind::Meters));
        }
        if let Some(tap) = self.taps.keys().next().cloned() {
            let event = self.taps.remove(&tap).expect("key just read");
            return Some((
                encode_stream(event, &mut self.seq, t_ms, self.taps_binary),
                StreamKind::Meters,
            ));
        }
        if let Some(ch) = self.spectrum.keys().next().copied() {
            let event = self.spectrum.remove(&ch).expect("key just read");
            return Some((encode_stream(event, &mut self.seq, t_ms, self.binary), StreamKind::Spectrum));
        }
        if let Some(ch) = self.gr.keys().next().copied() {
            let event = self.gr.remove(&ch).expect("key just read");
            return Some((encode_stream(event, &mut self.seq, t_ms, self.binary), StreamKind::Gr));
        }
        None
    }

    fn mark_sent(&mut self, kind: StreamKind) {
        match kind {
            StreamKind::Meters => self.meters_sent += 1,
            StreamKind::Spectrum => self.spectrum_sent += 1,
            StreamKind::Gr => self.gr_sent += 1,
            StreamKind::Control => {}
        }
    }
}

fn encode_stream(
    frame: openfader_engine::SharedEvent,
    seq: &mut u32,
    t_ms: u32,
    binary: bool,
) -> Message {
    if binary {
        if let Some(bytes) = frame.event.to_binary(*seq, t_ms) {
            *seq = seq.wrapping_add(1);
            return Message::Binary(bytes.into());
        }
    }
    // JSON fallback reuses the broadcast-time serialization shared across
    // clients (binary stays per-client: the sequence number is per
    // connection).
    Message::Text(frame.json.to_string().into())
}

/// Per-connection realtime counters behind the `stats` event.
#[derive(Clone, Copy, Debug, Default)]
pub struct StatsSnapshot {
    pub meters_sent: u64,
    pub meters_dropped: u64,
    pub gr_sent: u64,
    pub gr_dropped: u64,
    pub spectrum_sent: u64,
    pub spectrum_dropped: u64,
    pub queue_depth: usize,
    pub binary: bool,
}

/// Build the unicast `stats` reply (`{"type":"stats",…}`).
///
/// Keys are byte-stable (serde_json sorts map keys); the example shape is
/// pinned in `crates/serve/tests/golden/stats.json`.
pub fn stats_json(s: &StatsSnapshot) -> String {
    serde_json::json!({
        "type": "stats",
        "meters_sent": s.meters_sent,
        "meters_dropped": s.meters_dropped,
        "gr_sent": s.gr_sent,
        "gr_dropped": s.gr_dropped,
        "spectrum_sent": s.spectrum_sent,
        "spectrum_dropped": s.spectrum_dropped,
        "queue_depth": s.queue_depth,
        "binary": s.binary,
    })
    .to_string()
}

/// True when `text` is a `stats` request (answered locally from the
/// connection's [`Outbox`] counters, never forwarded to the engine).
fn is_stats_request(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| {
            v.get("cmd")
                .and_then(serde_json::Value::as_str)
                .map(|cmd| cmd == "stats")
        })
        .unwrap_or(false)
}

enum SenderJob {
    Stats,
}

/// One socket: inbound text lines → engine, engine events → socket.
///
/// Three tasks, one concern each: `inbound` forwards commands (and answers
/// `stats` requests via the sender), `pump` drains the engine mailbox into
/// the latest-only [`Outbox`] without ever blocking on the socket, and
/// `sender` puts frames on the wire. Parse failures and command errors
/// arrive as unicast `error` events; leaving the loop detaches the client.
async fn handle_socket(
    socket: WebSocket,
    engine: Arc<openfader_engine::Engine>,
    name: Option<String>,
    sim_only: bool,
) {
    let client = engine.attach(name).await;
    let id = client.id.clone();
    let (ws_tx, ws_rx) = socket.split();
    let outbox = Arc::new(Mutex::new(Outbox::new()));
    let notify = Arc::new(Notify::new());
    let (job_tx, job_rx) = mpsc::channel::<SenderJob>(8);

    // Inbound: socket -> engine. Owns no handle: commands go through
    // `Engine::handle_json`; the binary/taps preferences are re-read from
    // the engine after every command (single source of truth).
    let inbound = {
        let engine = Arc::clone(&engine);
        let outbox = Arc::clone(&outbox);
        let notify = Arc::clone(&notify);
        let id = id.clone();
        tokio::spawn(async move {
            let mut rx = ws_rx;
            while let Some(msg) = rx.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if sim_only {
                            use openfader_engine::Cmd;
                            let operation = match openfader_engine::parse_cmd(&text) {
                                Ok(Cmd::RefreshDevices { request_id, .. }) => {
                                    Some((request_id, "refresh"))
                                }
                                Ok(Cmd::DiscoverDevices { request_id }) => {
                                    Some((request_id, "discover"))
                                }
                                _ => None,
                            };
                            if let Some((request_id, operation)) = operation {
                                outbox.lock().await.control.push_back(
                                    openfader_engine::SharedEvent::new(
                                        openfader_engine::Event::DeviceOperationComplete {
                                            request_id,
                                            operation: operation.into(),
                                            results: Vec::new(),
                                            error: Some(
                                                "Simulation only: hardware discovery is disabled"
                                                    .into(),
                                            ),
                                        },
                                    ),
                                );
                                notify.notify_one();
                                continue;
                            }
                            let blocked = match openfader_engine::parse_cmd(&text) {
                                Ok(Cmd::Connect { driver, .. }) => driver != "sim",
                                Ok(
                                    Cmd::Scan
                                    | Cmd::Probe { .. }
                                    | Cmd::ConnectDevice { .. }
                                    | Cmd::RefreshDevices { .. }
                                    | Cmd::DiscoverDevices { .. },
                                ) => true,
                                _ => false,
                            };
                            if blocked {
                                outbox.lock().await.control.push_back(openfader_engine::SharedEvent::new(openfader_engine::Event::Error {
                                    message: "Simulation only: hardware connections and discovery are disabled".into(),
                                    locked_by: None,
                                    ch: None,
                                }));
                                notify.notify_one();
                                continue;
                            }
                        }
                        if is_stats_request(&text) {
                            if job_tx.send(SenderJob::Stats).await.is_err() {
                                break;
                            }
                        } else {
                            engine.handle_json(&id, &text).await;
                            let mut outbox = outbox.lock().await;
                            outbox.binary =
                                engine.client_binary_meters(&id);
                            outbox.taps_binary = engine.client_taps(&id);
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    // Ignore binary/ping/pong frames and read errors.
                    _ => {}
                }
            }
            engine.detach_client(&id).await;
        })
    };

    // Pump: engine mailbox -> outbox. Never blocks on the socket, so the
    // engine mailbox stays drained even when the peer stalls; meter-class
    // events collapse into slots of 1 (drop counted), control stays FIFO.
    let pump = {
        let outbox = Arc::clone(&outbox);
        let notify = Arc::clone(&notify);
        tokio::spawn(async move {
            let mut client = client;
            while let Some(event) = client.events.recv().await {
                outbox.lock().await.store(event);
                notify.notify_one();
            }
            outbox.lock().await.pump_closed = true;
            notify.notify_one();
            client.detach().await;
        })
    };

    // Sender: outbox -> socket. The only task that blocks on the wire;
    // backpressure here never grows the engine mailbox (the pump keeps
    // draining it into bounded slots).
    let sender = {
        let engine = Arc::clone(&engine);
        let outbox = Arc::clone(&outbox);
        let notify = Arc::clone(&notify);
        let id = id.clone();
        let mut job_rx = job_rx;
        tokio::spawn(async move {
            let mut tx = ws_tx;
            loop {
                while job_rx.try_recv().is_ok() {
                    let text = stats_json(&outbox.lock().await.snapshot());
                    if tx.send(Message::Text(text.into())).await.is_err() {
                        engine.detach_client(&id).await;
                        return;
                    }
                }
                let next = outbox.lock().await.take_next(&engine);
                match next {
                    Some((msg, kind)) => {
                        if tx.send(msg).await.is_err() {
                            engine.detach_client(&id).await;
                            return;
                        }
                        outbox.lock().await.mark_sent(kind);
                    }
                    None => {
                        let drained = {
                            let o = outbox.lock().await;
                            o.pump_closed && o.is_empty() && job_rx.is_closed()
                        };
                        if drained {
                            engine.detach_client(&id).await;
                            return;
                        }
                        // Wait for pump output or a stats request. A job
                        // received here must be answered, not dropped (the
                        // top-of-loop drain only sees jobs queued while a
                        // frame was ready).
                        let job = if job_rx.is_closed() {
                            notify.notified().await;
                            None
                        } else {
                            tokio::select! {
                                _ = notify.notified() => None,
                                res = job_rx.recv() => res,
                            }
                        };
                        if job.is_some() {
                            let text = stats_json(&outbox.lock().await.snapshot());
                            if tx.send(Message::Text(text.into())).await.is_err() {
                                engine.detach_client(&id).await;
                                return;
                            }
                        }
                    }
                }
            }
        })
    };

    let _ = tokio::join!(inbound, pump, sender);
}

async fn health(AxumState(app): AxumState<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({"ok": true, "driver": app.engine.active_driver()}))
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

async fn static_root(AxumState(app): AxumState<AppState>) -> impl IntoResponse {
    match app.static_dir.as_deref() {
        Some(dir) => read_index(dir).await,
        None => api_only(),
    }
}

fn api_only() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Html("<h1>openfader-serve</h1><p>No static UI found. WebSocket API at <code>/ws</code>.</p>"),
    )
        .into_response()
}

async fn read_index(dir: &StdPath) -> axum::response::Response {
    match tokio::fs::read(dir.join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Html("<h1>not found</h1>")).into_response(),
    }
}

async fn static_file(
    AxumState(app): AxumState<AppState>,
    Path(path): Path<String>,
) -> axum::response::Response {
    let Some(dir) = app.static_dir.as_deref() else {
        return api_only();
    };
    // Fall back to index.html for unknown paths (SPA).
    match tokio::fs::read(dir.join(&path)).await {
        Ok(bytes) => {
            ([(header::CONTENT_TYPE, content_type(&path))], bytes).into_response()
        }
        Err(_) => read_index(dir).await,
    }
}

/// Resolve the static UI directory: explicit flag > `$OPENFADER_UI_DIR` >
/// `ui/` under the CWD > `ui/` next to the workspace checkout (dev).
pub fn resolve_static_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return dir.is_dir().then_some(dir);
    }
    if let Ok(env) = std::env::var("OPENFADER_UI_DIR") {
        let p = PathBuf::from(env);
        if p.is_dir() {
            return Some(p);
        }
    }
    [
        PathBuf::from("ui"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_dir())
}

/// Single axum state for every route: WS/API share the [`Engine`](openfader_engine::Engine), static
/// files read the (optional) UI directory.
#[derive(Clone)]
struct AppState {
    sim_only: bool,
    engine: Arc<openfader_engine::Engine>,
    static_dir: Option<Arc<PathBuf>>,
}

/// Accept loop with MR1 socket options (see [`configure_accepted`]).
///
/// `axum::serve` applies the same `TCP_NODELAY` via `.tcp_nodelay(true)`,
/// but its accept path is not unit-testable from here; this loop keeps the
/// option-setting in [`configure_accepted`], covered by
/// `accepted_socket_has_nodelay` below. Otherwise identical behaviour:
/// HTTP/1 + upgrades, one task per connection.
async fn serve_loop(
    listener: tokio::net::TcpListener,
    app: Router,
) -> std::io::Result<()> {
    loop {
        let (stream, _peer) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("openfader-serve: accept error: {e:#}; continuing");
                continue;
            }
        };
        if let Err(e) = configure_accepted(&stream) {
            eprintln!("openfader-serve: TCP_NODELAY failed: {e:#}");
        }
        let svc = TowerToHyperService::new(app.clone());
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            if let Err(e) = Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(io, svc)
                .await
            {
                eprintln!("openfader-serve: connection error: {e:#}");
            }
        });
    }
}

/// Serve the API + static UI until the listener errors or the future is
/// dropped (tests abort the spawned task).
pub async fn run(
    listener: tokio::net::TcpListener,
    opts: ServeOptions,
) -> anyhow::Result<()> {
    run_with_lock_ttl(listener, opts, LOCK_TTL).await
}

/// Production passes [`LOCK_TTL`] (60 s); tests pass a short TTL (e.g.
/// 200 ms) to cover expiry without waiting a minute.
pub async fn run_with_lock_ttl(
    listener: tokio::net::TcpListener,
    opts: ServeOptions,
    lock_ttl: Duration,
) -> anyhow::Result<()> {
    run_with_poll(listener, opts, lock_ttl, PARAM_POLL_INTERVAL).await
}

/// Production passes [`PARAM_POLL_INTERVAL`] (one driver query per 50 ms =
/// 20 queries/s total); tests pass a short interval (e.g. 1 ms) so a full
/// sweep finishes in milliseconds.
pub async fn run_with_poll(
    listener: tokio::net::TcpListener,
    opts: ServeOptions,
    lock_ttl: Duration,
    poll_interval: Duration,
) -> anyhow::Result<()> {
    run_with_full(listener, opts, lock_ttl, poll_interval, None, None).await
}

pub async fn run_with_full(
    listener: tokio::net::TcpListener,
    opts: ServeOptions,
    lock_ttl: Duration,
    poll_interval: Duration,
    poll_inflight: Option<usize>,
    data_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    use openfader_engine::{Engine, EngineConfig};

    let local = listener.local_addr()?;
    if opts.sim_only && (opts.driver != "sim" || opts.addr.is_some() || opts.data_dir.is_some() || data_dir.is_some()) {
        anyhow::bail!("--sim-only requires the sim driver, no mixer address, and an in-memory scene library");
    }
    let preset = match opts.preset.as_deref() {
        None => None,
        Some(name) => Some(openfader_sim::SimPreset::parse(name).ok_or_else(|| {
            anyhow::anyhow!("unknown --preset {name:?} (want generic|xr18|dl32s|studio)")
        })?),
    };
    let engine = Engine::new(EngineConfig {
        lock_ttl,
        poll_interval,
        preset,
        data_dir: data_dir.or(opts.data_dir),
        poll_inflight: poll_inflight.or(opts.poll_inflight),
        scanner: None,
    });

    // Initial driver connect. Hardware may be unreachable at boot; stay up
    // so a browser client can retry via the `connect` command.
    if let Err(e) = engine.connect(&opts.driver, opts.addr.as_deref()).await {
        eprintln!(
            "openfader-serve: initial connect ({} @ {}) failed: {e:#}; continuing, \
             retry with {{\"cmd\":\"connect\",...}}",
            opts.driver,
            opts.addr.as_deref().unwrap_or("(default)"),
        );
    }

    let static_dir = resolve_static_dir(opts.static_dir).map(Arc::new);
    match &static_dir {
        Some(dir) => eprintln!("openfader-serve: serving UI from {}", dir.display()),
        None => eprintln!("openfader-serve: no ui/ directory found; API only"),
    }
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(health))
        .route("/", get(static_root))
        .route("/{*path}", get(static_file))
        .with_state(AppState { engine, static_dir, sim_only: opts.sim_only });

    eprintln!("openfader-serve: listening on http://{local} (ws://{local}/ws)");
    serve_loop(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The accept path sets TCP_NODELAY on the accepted stream (MR1):
    /// real loopback pair, real `configure_accepted`, real `getsockopt`.
    #[tokio::test]
    async fn accepted_socket_has_nodelay() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let accepted = tokio::spawn(async move { listener.accept().await.unwrap().0 });
        let _client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let stream = accepted.await.unwrap();
        configure_accepted(&stream).unwrap();
        assert!(
            socket2::SockRef::from(&stream).tcp_nodelay().unwrap(),
            "TCP_NODELAY must be set on accepted sockets"
        );
    }

    #[test]
    fn stats_json_shape() {
        let s = StatsSnapshot {
            meters_sent: 120,
            meters_dropped: 3,
            gr_sent: 40,
            gr_dropped: 1,
            spectrum_sent: 40,
            spectrum_dropped: 0,
            queue_depth: 2,
            binary: true,
        };
        let v: serde_json::Value = serde_json::from_str(&stats_json(&s)).unwrap();
        assert_eq!(v.get("type"), Some(&serde_json::json!("stats")));
        for key in [
            "meters_sent",
            "meters_dropped",
            "gr_sent",
            "gr_dropped",
            "spectrum_sent",
            "spectrum_dropped",
            "queue_depth",
        ] {
            assert!(
                v.get(key).and_then(serde_json::Value::as_u64).is_some(),
                "stats carries integer {key}: {v}"
            );
        }
        assert_eq!(v.get("binary"), Some(&serde_json::json!(true)));
    }

    #[test]
    fn stats_request_detection() {
        assert!(is_stats_request(r#"{"cmd":"stats"}"#));
        assert!(!is_stats_request(r#"{"cmd":"hello","binary_meters":true}"#));
        assert!(!is_stats_request(r#"{"cmd":"list_shows"}"#));
        assert!(!is_stats_request("not json"));
    }

    /// Latest-only slots (MR1): a second meter-class frame replaces the
    /// first (drop counted); control stays FIFO and reliable; the sender
    /// drains control first, then one stream frame per pass.
    /// R2 taps slots: one latest-only slot per tap id (replacement counts
    /// into the meters drop counter, the `stats` shape is pinned), and
    /// taps frames go binary (type 3) only for taps-negotiated clients.
    #[test]
    fn outbox_taps_latest_only_and_binary() {
        use openfader_engine::protocol::BIN_TYPE_TAPS;
        use openfader_engine::Event;
        let engine = openfader_engine::Engine::new(openfader_engine::EngineConfig::default());
        let mut o = Outbox::new();
        let shared = openfader_engine::SharedEvent::new;
        o.store(shared(Event::Taps { tap: "in".into(), levels: vec![0.1] }));
        o.store(shared(Event::Taps { tap: "post".into(), levels: vec![0.2] }));
        assert_eq!(o.queue_depth(), 2);
        o.store(shared(Event::Taps { tap: "in".into(), levels: vec![0.3] }));
        assert_eq!(o.meters_dropped, 1);
        assert_eq!(o.queue_depth(), 2);
        // JSON by default…
        let (msg, kind) = o.take_next(&engine).unwrap();
        assert!(matches!(kind, StreamKind::Meters));
        assert!(matches!(msg, Message::Text(_)));
        // …binary type 3 once the client negotiated taps.
        o.taps_binary = true;
        o.store(shared(Event::Taps { tap: "gr_comp".into(), levels: vec![6.0] }));
        // Drain remaining queued taps first (per-tap slots are unordered).
        while o.taps.len() > 1 {
            let _ = o.take_next(&engine).unwrap();
        }
        let (msg, _) = o.take_next(&engine).unwrap();
        match msg {
            Message::Binary(bytes) => assert_eq!(bytes[0], BIN_TYPE_TAPS),
            Message::Text(_) => panic!("taps-negotiated client gets binary taps"),
            _ => panic!("unexpected control frame kind"),
        }
        o.mark_sent(StreamKind::Meters);
        let snap = o.snapshot();
        assert_eq!(snap.queue_depth, 0);
    }

    #[test]
    fn outbox_latest_only_slots() {
        use openfader_engine::Event;
        let engine = openfader_engine::Engine::new(openfader_engine::EngineConfig::default());
        let mut o = Outbox::new();
        let shared = openfader_engine::SharedEvent::new;
        // Two meter frames with no take in between: one drop.
        o.store(shared(Event::Meters { levels: vec![0.1] }));
        o.store(shared(Event::Meters { levels: vec![0.2] }));
        assert_eq!(o.meters_dropped, 1);
        assert_eq!(o.queue_depth(), 1);
        // Per-channel spectrum/gr slots are independent per channel.
        o.store(shared(Event::Spectrum { ch: 3, bins: vec![] }));
        o.store(shared(Event::Gr { ch: 3, gate: 1.0, comp: 0.0 }));
        assert_eq!(o.queue_depth(), 3);
        o.store(shared(Event::Spectrum { ch: 3, bins: vec![] }));
        assert_eq!(o.spectrum_dropped, 1);
        o.store(shared(Event::Gr { ch: 4, gate: 0.0, comp: 0.0 }));
        assert_eq!(o.gr_dropped, 0, "other channels do not collide");
        // Control first, then streams; marks advance the sent counters.
        o.store(shared(Event::You { id: "client-1".into() }));
        let (first, kind) = o.take_next(&engine).unwrap();
        assert!(matches!(kind, StreamKind::Control));
        assert!(matches!(first, Message::Text(_)));
        let (_, kind) = o.take_next(&engine).unwrap();
        assert!(matches!(kind, StreamKind::Meters));
        o.mark_sent(kind);
        assert_eq!(o.meters_sent, 1);
        let snap = o.snapshot();
        assert_eq!(snap.queue_depth, 3, "spectrum(3) + gr(3,4) remain");
    }
}
