//! openmix-serve library: HTTP static UI + WebSocket mixer API on one port.
//!
//! JSON protocol (channels are 0-based, the [`openmix_core`] convention):
//!
//! client -> server:
//! ```json
//! {"cmd":"connect","driver":"sim|xair|dl32s","addr":"..."}
//! {"cmd":"fader","ch":0,"value":0.0}
//! {"cmd":"mute","ch":0,"muted":true}
//! {"cmd":"param","ch":0,"field":"gain","value":0.5}
//! ```
//!
//! server -> client:
//! ```json
//! {"type":"state","driver":"sim","strips":[{"ch":0,"name":"CH01","fader":0.7,"muted":false}]}
//! {"type":"meters","levels":[0.1,0.2]}
//! ```
//!
//! `state` is sent on connect and after every mutation; `meters` ticks at
//! ~10 Hz. Unknown/invalid commands get `{"type":"error","message":"..."}`.

use std::{path::PathBuf, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State as AxumState,
    },
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use openmix_core::{MixerDriver, ParamValue, StripId};
use serde_json::{json, Value};
use tokio::{
    net::TcpListener,
    sync::{broadcast, RwLock},
};

/// Startup options for [`run`].
#[derive(Clone, Debug)]
pub struct ServeOptions {
    /// Initial driver: `sim` (default), `xair`, `dl32s`.
    pub driver: String,
    /// Initial mixer address (hardware drivers; sim ignores it).
    pub addr: Option<String>,
    /// Directory of static UI files. `None` = auto-detect (`ui/`).
    pub static_dir: Option<PathBuf>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            driver: "sim".to_string(),
            addr: None,
            static_dir: None,
        }
    }
}

/// Shared server state: driver bank + active selection + state-change fanout.
///
/// Static dispatch over drivers (`MixerDriver` is not dyn-compatible due to
/// async methods). New providers add one field + one match arm, as in
/// `openmix-cli`.
pub struct Shared {
    sim: openmix_sim::SimDriver,
    xair: openmix_proto_xair::XAirDriver,
    dl32s: openmix_proto_dl32s::Dl32sDriver,
    active: RwLock<String>,
    state_tx: broadcast::Sender<()>,
}

impl Shared {
    fn new() -> Self {
        let (state_tx, _) = broadcast::channel(16);
        Self {
            sim: openmix_sim::SimDriver::new(),
            xair: openmix_proto_xair::XAirDriver::new(),
            dl32s: openmix_proto_dl32s::Dl32sDriver::new(),
            active: RwLock::new("sim".to_string()),
            state_tx,
        }
    }

    async fn active_name(&self) -> String {
        self.active.read().await.clone()
    }

    fn known(driver: &str) -> bool {
        matches!(driver, "sim" | "xair" | "dl32s")
    }

    fn default_addr(driver: &str) -> &str {
        if driver == "sim" {
            "sim://local"
        } else {
            "192.168.1.1"
        }
    }

    /// Select `driver` and connect it. Broadcasts a state refresh on success.
    async fn connect(&self, driver: &str, addr: Option<&str>) -> anyhow::Result<()> {
        if !Self::known(driver) {
            anyhow::bail!("unknown driver {driver:?}; expected sim|xair|dl32s");
        }
        let target = addr
            .map(str::to_string)
            .unwrap_or_else(|| Self::default_addr(driver).to_string());
        match driver {
            "xair" => self.xair.connect(&target).await?,
            "dl32s" => self.dl32s.connect(&target).await?,
            _ => self.sim.connect(&target).await?,
        }
        *self.active.write().await = driver.to_string();
        Ok(())
    }

    /// Full `state` message for the active driver.
    async fn state_message(&self) -> Value {
        let name = self.active_name().await;
        let strips = match name.as_str() {
            "xair" => snapshot_strips(&self.xair).await,
            "dl32s" => snapshot_strips(&self.dl32s).await,
            _ => snapshot_strips(&self.sim).await,
        };
        json!({"type":"state","driver":name,"strips":strips})
    }

    /// Current `meters` message for the active driver (~10 Hz tick).
    async fn meters_message(&self) -> Value {
        let levels = match self.active_name().await.as_str() {
            "xair" => self.xair.poll_meters().levels,
            "dl32s" => self.dl32s.poll_meters().levels,
            _ => self.sim.poll_meters().levels,
        };
        json!({"type":"meters","levels":levels})
    }

    /// Apply one client command. Broadcasts a state refresh on success.
    async fn apply(&self, msg: &Value) -> anyhow::Result<()> {
        let cmd = msg
            .get("cmd")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing \"cmd\""))?;
        match cmd {
            "connect" => {
                let driver = msg.get("driver").and_then(Value::as_str).unwrap_or("sim");
                let addr = msg.get("addr").and_then(Value::as_str);
                self.connect(driver, addr).await?;
            }
            "fader" => {
                let ch = channel_arg(msg)?;
                let value = msg
                    .get("value")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| anyhow::anyhow!("fader needs numeric \"value\" 0.0..=1.0"))?
                    as f32;
                match self.active_name().await.as_str() {
                    "xair" => self.xair.set_fader(ch, value).await?,
                    "dl32s" => self.dl32s.set_fader(ch, value).await?,
                    _ => self.sim.set_fader(ch, value).await?,
                }
            }
            "mute" => {
                let ch = channel_arg(msg)?;
                let muted = msg
                    .get("muted")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| anyhow::anyhow!("mute needs boolean \"muted\""))?;
                match self.active_name().await.as_str() {
                    "xair" => self.xair.set_mute(ch, muted).await?,
                    "dl32s" => self.dl32s.set_mute(ch, muted).await?,
                    _ => self.sim.set_mute(ch, muted).await?,
                }
            }
            "param" => {
                let ch = channel_arg(msg)?;
                let field = msg
                    .get("field")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("param needs string \"field\""))?;
                let raw = msg
                    .get("value")
                    .ok_or_else(|| anyhow::anyhow!("param needs \"value\""))?;
                let pv = match raw {
                    Value::Number(n) => ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32),
                    Value::Bool(b) => ParamValue::Bool(*b),
                    Value::String(s) => ParamValue::Text(s.clone()),
                    _ => anyhow::bail!("param value must be number|bool|string"),
                };
                match self.active_name().await.as_str() {
                    "xair" => self.xair.set_param(ch, field, pv).await?,
                    "dl32s" => self.dl32s.set_param(ch, field, pv).await?,
                    _ => self.sim.set_param(ch, field, pv).await?,
                }
            }
            other => anyhow::bail!("unknown cmd {other:?}"),
        }
        let _ = self.state_tx.send(());
        Ok(())
    }
}

async fn snapshot_strips(d: &impl MixerDriver) -> Vec<Value> {
    match d.snapshot().await {
        Ok(snap) => snap
            .strips
            .iter()
            .map(|s| {
                json!({
                    "ch": s.id.0,
                    "name": s.name,
                    "fader": s.fader,
                    "muted": s.mute,
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn channel_arg(msg: &Value) -> anyhow::Result<StripId> {
    let ch = msg
        .get("ch")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("command needs integer \"ch\" (0-based)"))?;
    let ch: u16 = ch
        .try_into()
        .map_err(|_| anyhow::anyhow!("channel {ch} out of range"))?;
    Ok(StripId(ch))
}

fn error_message(e: impl std::fmt::Display) -> String {
    json!({"type":"error","message":e.to_string()}).to_string()
}

async fn ws_handler(ws: WebSocketUpgrade, AxumState(app): AxumState<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, app.shared))
}

async fn handle_socket(socket: WebSocket, shared: Arc<Shared>) {
    let (mut tx, mut rx) = socket.split();
    // Initial snapshot.
    if tx
        .send(Message::Text(shared.state_message().await.to_string()))
        .await
        .is_err()
    {
        return;
    }
    let mut state_rx = shared.state_tx.subscribe();
    let mut meters = tokio::time::interval(std::time::Duration::from_millis(100));
    // Skip the immediate first tick so a fresh client gets state first.
    meters.tick().await;
    loop {
        tokio::select! {
            incoming = rx.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Value>(&text) {
                            Ok(cmd) => {
                                if let Err(e) = shared.apply(&cmd).await {
                                    let _ = tx.send(Message::Text(error_message(e))).await;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Message::Text(error_message(format!("invalid JSON: {e}")))).await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    // Ignore binary/ping/pong frames.
                    _ => {}
                }
            }
            _ = meters.tick() => {
                let m = shared.meters_message().await;
                if tx.send(Message::Text(m.to_string())).await.is_err() {
                    break;
                }
            }
            changed = state_rx.recv() => {
                if changed.is_err() {
                    break;
                }
                let s = shared.state_message().await;
                if tx.send(Message::Text(s.to_string())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn health(AxumState(app): AxumState<AppState>) -> impl IntoResponse {
    Json(json!({"ok": true, "driver": app.shared.active_name().await}))
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
        Html("<h1>openmix-serve</h1><p>No static UI found. WebSocket API at <code>/ws</code>.</p>"),
    )
        .into_response()
}

async fn read_index(dir: &PathBuf) -> axum::response::Response {
    match tokio::fs::read(dir.join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, Html("<h1>not found</h1>")).into_response(),
    }
}

async fn static_file(
    AxumState(app): AxumState<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let Some(dir) = app.static_dir.as_deref() else {
        return api_only();
    };
    // Fall back to index.html for unknown paths (SPA).
    match tokio::fs::read(dir.join(&path)).await {
        Ok(bytes) => {
            return ([(header::CONTENT_TYPE, content_type(&path))], bytes).into_response();
        }
        Err(_) => read_index(dir).await,
    }
}

/// Resolve the static UI directory: explicit flag > `$OPENMIX_UI_DIR` >
/// `ui/` under the CWD > `ui/` next to the workspace checkout (dev).
pub fn resolve_static_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return dir.is_dir().then_some(dir);
    }
    if let Ok(env) = std::env::var("OPENMIX_UI_DIR") {
        let p = PathBuf::from(env);
        if p.is_dir() {
            return Some(p);
        }
    }
    for candidate in [
        PathBuf::from("ui"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui"),
    ] {
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

/// Single axum state for every route: WS/API share [`Shared`], static
/// files read the (optional) UI directory.
#[derive(Clone)]
struct AppState {
    shared: Arc<Shared>,
    static_dir: Option<Arc<PathBuf>>,
}

/// Serve the API + static UI until the listener errors or the future is
/// dropped (tests abort the spawned task).
pub async fn run(listener: TcpListener, opts: ServeOptions) -> anyhow::Result<()> {
    let local = listener.local_addr()?;
    let shared = Arc::new(Shared::new());

    // Initial driver connect. Hardware may be unreachable at boot; stay up
    // so a browser client can retry via the `connect` command.
    if let Err(e) = shared.connect(&opts.driver, opts.addr.as_deref()).await {
        eprintln!(
            "openmix-serve: initial connect ({} @ {}) failed: {e:#}; continuing, \
             retry with {{\"cmd\":\"connect\",...}}",
            opts.driver,
            opts.addr.as_deref().unwrap_or("(default)"),
        );
    }

    let static_dir = resolve_static_dir(opts.static_dir).map(Arc::new);
    match &static_dir {
        Some(dir) => eprintln!("openmix-serve: serving UI from {}", dir.display()),
        None => eprintln!("openmix-serve: no ui/ directory found; API only"),
    }
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(health))
        .route("/", get(static_root))
        .route("/*path", get(static_file))
        .with_state(AppState { shared, static_dir });

    eprintln!("openmix-serve: listening on http://{local} (ws://{local}/ws)");
    axum::serve(listener, app).await?;
    Ok(())
}
