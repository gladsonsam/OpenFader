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
//! {"cmd":"lock","ch":0}
//! {"cmd":"unlock","ch":0}
//! {"cmd":"list_shows"}
//! {"cmd":"capture_scene","show":"gig","name":"verse"}
//! {"cmd":"recall_scene","show":"gig","name":"verse","fade_ms":400}
//! {"cmd":"delete_scene","show":"gig","name":"verse"}
//! ```
//!
//! server -> client:
//! ```json
//! {"type":"state","driver":"sim","strips":[{"ch":0,"name":"CH01","fader":0.7,"muted":false}]}
//! {"type":"param","ch":0,"field":"gain","value":0.5}
//! {"type":"shows","shows":[{"name":"gig","scenes":[{"name":"verse","main_fader":null,"strips":[{"ch":0,"name":"CH01","fader":0.7,"muted":false}]}]}]}
//! {"type":"meters","levels":[0.1,0.2]}
//! {"type":"spectrum","ch":0,"bins":[...64 floats...]}
//! {"type":"gr","ch":0,"gate":0.0,"comp":0.0}
//! ```
//!
//! `state` is sent on connect and after every mutation; `param` echoes every
//! successful `param` set (all drivers) and every param applied by
//! `recall_scene` so sibling UIs stay in sync; `shows` is sent on
//! connect, as the direct reply to `list_shows`, and broadcast after
//! `capture_scene`/`delete_scene` so every UI stays in sync. The show library
//! lives server-side (mirroring the Tauri `capture_scene`/`recall_scene`
//! semantics in `src-tauri/src/main.rs`), so scenes survive UI reloads.
//! `meters` ticks at ~10 Hz. One `spectrum` message per channel per tick follows each `meters`
//! tick (see [`SPECTRUM_CHANNELS`]). One `gr` message per channel follows every
//! second `meters` tick (~5 Hz, see [`GR_CHANNELS`]): the DYN blob cache
//! fills asynchronously, so `gate`/`comp` read 0.0 until the desk streams
//! `/meters/6`. Unknown/invalid commands get
//! `{"type":"error","message":"..."}`.
//!
//! Multi-client awareness (no arbitration yet): the server tracks connected
//! WS clients (id, connect time, label from `?name=` or `client-N`) and
//! broadcasts `{"type":"clients","count":N,"writers":[...]}` on join/leave.
//! Every `state`/`param` frame emitted by a `fader`/`mute`/`param`
//! (`recall_scene` included) mutation carries `"by":"<writer-id>"` so UIs
//! can show who moved what. Writers are ids with a mutation in the last
//! 10 s.
//!
//! Strip locks (light arbitration): `{"cmd":"lock","ch":N}` claims a strip
//! for the sender id; `{"cmd":"unlock","ch":N}` releases it. Holds are
//! `ch -> holder-id` with a 60 s expiry refreshed by holder activity
//! (any successful command from the holder) and auto-released on
//! disconnect. The server broadcasts `{"type":"locks","holds":{...}}`
//! (ch keys as strings) on every change including expiry. A strip locked
//! by another writer rejects `fader`/`mute`/`param` with
//! `{"type":"error","locked_by":"<holder-id>",...}`. `recall_scene`
//! bypasses locks but unicasts `{"type":"recalled","overrode_locks":[ch..]}`
//! to the recaller when it changed strips locked by someone else.
//! Lock TTL defaults to [`LOCK_TTL`]; tests use [`run_with_lock_ttl`] with
//! a short TTL to cover expiry without waiting 60 s.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
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
use openmix_core::{
    rta::RtaProcessor,
    show::{Scene, Show, DEFAULT_FADE_MS},
    MeterFrame, MixerDriver, ParamValue, StripId,
};
use serde_json::{json, Value};
use tokio::{
    net::TcpListener,
    sync::{broadcast, RwLock},
};

/// Max channels with per-tick spectrum output (first N channels only).
///
/// 16 covers a full drum/band layer on typical 16/24/32ch surfaces while
/// keeping the ~10 Hz WS fanout bounded: 16 msgs x 64 floats/tick stays
/// well under one MTU-friendly JSON burst per tick, and the per-client
/// naive-DFT cost (128-pt, no `fft` feature needed — `RtaProcessor` falls
/// back to an identical O(N^2) path) is ~1.3M flops/s, negligible next to
/// the socket writes. Channels beyond this cap still stream `meters`
/// levels; raise the cap only with a matching client-side subscription
/// filter so idle 32ch surfaces don't 2x the broadcast bytes.
pub const SPECTRUM_CHANNELS: usize = 16;

/// Max channels with per-tick GR output (first N channels only).
///
/// Same 16ch cap as [`SPECTRUM_CHANNELS`]: covers a full drum/band layer on
/// typical 16/24/32ch surfaces while keeping the WS fanout bounded (16 small
/// `gr` frames per GR tick). GR ticks at ~5 Hz — every second 10 Hz `meters`
/// tick — so dynamics bars stay live without doubling the meter broadcast
/// bytes. Channels beyond this cap still stream `meters` levels; `gate` /
/// `comp` read 0.0 until the driver's DYN cache fills (xair `/meters/6`
/// blob; sim/dl32s have no GR source and always read 0.0).
pub const GR_CHANNELS: usize = 16;

/// Default per-strip lock TTL: holder id + expiry, refreshed by holder
/// activity, auto-released on disconnect/expiry.
///
/// Tests needing expiry coverage use [`run_with_lock_ttl`] with a short
/// TTL instead of waiting 60 s.
pub const LOCK_TTL: Duration = Duration::from_secs(60);

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

/// One connected WS client (presence tracking; no arbitration yet).
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct ClientInfo {
    id: String,
    label: String,
    connected_at: Instant,
    last_write: Option<Instant>,
}

/// One strip lock: holder id + expiry.
#[derive(Clone, Debug)]
struct StripLock {
    holder: String,
    expires_at: Instant,
}

/// Shared server state: driver bank + active selection + state-change fanout.
///
/// Static dispatch over drivers (`MixerDriver` is not dyn-compatible due to
/// async methods). New providers add one field + one match arm, as in
/// `openmix-cli`.
///
/// The show library (`shows`) is server-side, mirroring the Tauri shell
/// (`src-tauri/src/main.rs`): scenes captured here survive UI reloads and
/// driver switches with no localStorage round-trip.
///
/// Presence: `clients` tracks live sockets by id; `clients_tx` carries a
/// prebuilt `clients` frame on join/leave. Mutation broadcasts (`state`
/// via `state_tx`, `param` via `param_tx`) carry `"by":"<writer-id>"`.
/// Strip locks (`locks` + `locks_tx`) arbitrate per-channel `fader`/`mute`/
/// `param` writes; `recall_scene` bypasses them but reports overrides.
pub struct Shared {
    sim: openmix_sim::SimDriver,
    xair: openmix_proto_xair::XAirDriver,
    dl32s: openmix_proto_dl32s::Dl32sDriver,
    active: RwLock<String>,
    state_tx: broadcast::Sender<String>,
    shows: RwLock<Vec<Show>>,
    shows_tx: broadcast::Sender<()>,
    param_tx: broadcast::Sender<Value>,
    clients: RwLock<HashMap<String, ClientInfo>>,
    clients_tx: broadcast::Sender<Value>,
    next_client: AtomicU64,
    locks: RwLock<HashMap<u16, StripLock>>,
    locks_tx: broadcast::Sender<Value>,
    lock_ttl: Duration,
}

impl Shared {
    #[allow(dead_code)]
    fn new() -> Self {
        Self::with_lock_ttl(LOCK_TTL)
    }

    /// `Shared` with an explicit lock TTL (tests use a short TTL to cover
    /// expiry without waiting 60 s; production uses [`LOCK_TTL`]).
    pub fn with_lock_ttl(lock_ttl: Duration) -> Self {
        let (state_tx, _) = broadcast::channel(16);
        let (shows_tx, _) = broadcast::channel(16);
        let (param_tx, _) = broadcast::channel(256);
        let (clients_tx, _) = broadcast::channel(16);
        let (locks_tx, _) = broadcast::channel(16);
        Self {
            sim: openmix_sim::SimDriver::new(),
            xair: openmix_proto_xair::XAirDriver::new(),
            dl32s: openmix_proto_dl32s::Dl32sDriver::new(),
            active: RwLock::new("sim".to_string()),
            state_tx,
            shows: RwLock::new(Vec::new()),
            shows_tx,
            param_tx,
            clients: RwLock::new(HashMap::new()),
            clients_tx,
            next_client: AtomicU64::new(1),
            locks: RwLock::new(HashMap::new()),
            locks_tx,
            lock_ttl,
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
        self.state_message_with_by(None).await
    }

    /// Full `state` message tagged with the mutation writer (`"by"`).
    ///
    /// `by` is `None` for the initial per-connection snapshot; every
    /// broadcast that follows a `fader`/`mute`/`param`/`recall_scene`/
    /// `connect` mutation carries `Some(writer-id)`.
    async fn state_message_with_by(&self, by: Option<&str>) -> Value {
        let name = self.active_name().await;
        let strips = match name.as_str() {
            "xair" => snapshot_strips(&self.xair).await,
            "dl32s" => snapshot_strips(&self.dl32s).await,
            _ => snapshot_strips(&self.sim).await,
        };
        match by {
            Some(writer) => json!({"type":"state","driver":name,"strips":strips,"by":writer}),
            None => json!({"type":"state","driver":name,"strips":strips}),
        }
    }

    /// Allocate the next client id/label pair (`client-N` fallback).
    fn alloc_client(&self, name: Option<&str>) -> (String, String) {
        let n = self.next_client.fetch_add(1, Ordering::Relaxed);
        let id = format!("client-{n}");
        let clean = name.map(str::trim).filter(|s| !s.is_empty());
        let label = clean.unwrap_or(&id).to_string();
        (id, label)
    }

    /// Register a client and broadcast the updated `clients` frame.
    async fn add_client(&self, id: &str, label: &str) {
        {
            let mut clients = self.clients.write().await;
            clients.insert(
                id.to_string(),
                ClientInfo {
                    id: id.to_string(),
                    label: label.to_string(),
                    connected_at: Instant::now(),
                    last_write: None,
                },
            );
        }
        let _ = self.clients_tx.send(self.clients_message().await);
    }

    /// Remove a client (on disconnect), auto-release its strip locks, and
    /// broadcast the updated frames.
    async fn remove_client(&self, id: &str) {
        {
            let mut clients = self.clients.write().await;
            clients.remove(id);
        }
        let _ = self.clients_tx.send(self.clients_message().await);
        // Auto-release: a disconnecting holder must not pin strips.
        let released = {
            let mut locks = self.locks.write().await;
            let before = locks.len();
            locks.retain(|_, l| l.holder != id);
            locks.len() != before
        };
        if released {
            let _ = self.locks_tx.send(self.locks_message_inner().await);
        }
    }

    /// Record a mutation write for `writers` tracking (last 10 s).
    async fn note_write(&self, id: &str) {
        let now = Instant::now();
        let mut clients = self.clients.write().await;
        if let Some(info) = clients.get_mut(id) {
            info.last_write = Some(now);
        }
    }

    /// Current `clients` frame: `{"type":"clients","count":N,"writers":[...]}`.
    ///
    /// `writers` holds ids with a mutation in the last 10 s, sorted for
    /// deterministic snapshots.
    async fn clients_message(&self) -> Value {
        let clients = self.clients.read().await;
        let now = Instant::now();
        let mut writers: Vec<String> = clients
            .values()
            .filter(|c| {
                c.last_write
                    .is_some_and(|t| now.duration_since(t).as_secs() < 10)
            })
            .map(|c| c.id.clone())
            .collect();
        writers.sort();
        json!({"type":"clients","count":clients.len(),"writers":writers})
    }

    /// Drop expired holds. Returns true when anything was removed.
    async fn prune_expired_locks(&self) -> bool {
        let now = Instant::now();
        let mut locks = self.locks.write().await;
        let before = locks.len();
        locks.retain(|_, l| now < l.expires_at);
        locks.len() != before
    }

    /// Holder of `ch`, or `None` when unlocked/expired.
    async fn lock_holder(&self, ch: u16) -> Option<String> {
        let locks = self.locks.read().await;
        let entry = locks.get(&ch)?;
        if Instant::now() >= entry.expires_at {
            return None;
        }
        Some(entry.holder.clone())
    }

    /// Refresh expiry on every non-expired hold owned by `writer`
    /// (holder activity keeps its strips pinned for another TTL).
    async fn touch_holder_locks(&self, writer: &str) {
        let deadline = Instant::now() + self.lock_ttl;
        let now = Instant::now();
        let mut locks = self.locks.write().await;
        for entry in locks.values_mut() {
            if entry.holder == writer && now < entry.expires_at {
                entry.expires_at = deadline;
            }
        }
    }

    /// Current `locks` frame: `{"type":"locks","holds":{ch:holder-id}}`
    /// with channel keys as strings (JSON object keys). Prunes expired
    /// holds first so newcomers never see stale pins.
    async fn locks_message(&self) -> Value {
        let _ = self.prune_for_message().await;
        self.locks_message_inner().await
    }

    /// Prune helper split out so [`Self::remove_client`] (which already
    /// holds no lock across the broadcast) and the expiry sweeper share
    /// one path without double-locking.
    async fn prune_for_message(&self) -> bool {
        self.prune_expired_locks().await
    }

    /// Build the `locks` frame from the current (already pruned) table.
    /// Expired entries are skipped defensively even if pruning was skipped.
    async fn locks_message_inner(&self) -> Value {
        let locks = self.locks.read().await;
        let now = Instant::now();
        let mut holds = serde_json::Map::new();
        let mut chs: Vec<u16> = locks.keys().copied().collect();
        chs.sort_unstable();
        for ch in chs {
            if let Some(entry) = locks.get(&ch) {
                if now < entry.expires_at {
                    holds.insert(ch.to_string(), Value::String(entry.holder.clone()));
                }
            }
        }
        Value::Object({
            let mut m = serde_json::Map::new();
            m.insert("type".to_string(), Value::String("locks".to_string()));
            m.insert("holds".to_string(), Value::Object(holds));
            m
        })
    }

    /// Broadcast the current `locks` frame (after pruning).
    async fn broadcast_locks(&self) {
        // Prune inside `locks_message`; a second prune here is a no-op.
        let msg = self.locks_message().await;
        let _ = self.locks_tx.send(msg);
    }

    /// Denied-write error frame: `{"type":"error","locked_by":id,...}`.
    fn locked_error(ch: u16, holder: &str) -> Value {
        json!({
            "type": "error",
            "message": format!("strip {ch} locked by {holder}"),
            "locked_by": holder,
            "ch": ch,
        })
    }

    /// Current meter levels for the active driver (one entry per strip).
    async fn meter_levels(&self) -> Vec<f32> {
        match self.active_name().await.as_str() {
            "xair" => self.xair.poll_meters().levels,
            "dl32s" => self.dl32s.poll_meters().levels,
            _ => self.sim.poll_meters().levels,
        }
    }

    /// Current `meters` message for the active driver (~10 Hz tick).
    #[allow(dead_code)]
    async fn meters_message(&self) -> Value {
        let levels = self.meter_levels().await;
        meters_message_for(&levels)
    }

    /// Gate/comp GR (dB reduction `>= 0`) for one 0-based channel.
    ///
    /// xair reads the DYN blob cache (`gate_gr`/`comp_gr` param path); no
    /// data yet (or out of range) falls back to 0.0 so the tick never drops
    /// a frame — the UI treats 0.0 as "no reduction", same as an idle gate.
    /// sim/dl32s have no GR source and always read (0.0, 0.0).
    async fn gr_db(&self, ch: usize) -> (f32, f32) {
        if self.active_name().await.as_str() != "xair" {
            return (0.0, 0.0);
        }
        let Ok(strip) = u16::try_from(ch) else {
            return (0.0, 0.0);
        };
        let id = StripId(strip);
        let gate = match self.xair.get_param(id, "gate_gr").await {
            Ok(ParamValue::Float(v)) => v.max(0.0),
            _ => 0.0,
        };
        let comp = match self.xair.get_param(id, "comp_gr").await {
            Ok(ParamValue::Float(v)) => v.max(0.0),
            _ => 0.0,
        };
        (gate, comp)
    }

    /// Full `shows` message: every show with its scenes and per-strip
    /// fader/mute (the Tauri `get_scene` shape per scene, so the web
    /// `ShowEditor` can rebuild its library from one frame).
    async fn shows_message(&self) -> Value {
        let shows = self.shows.read().await;
        json!({
            "type": "shows",
            "shows": shows.iter().map(show_json).collect::<Vec<_>>(),
        })
    }

    /// Snapshot the active driver (used by `capture_scene`).
    async fn active_snapshot(&self) -> anyhow::Result<openmix_core::MixerSnapshot> {
        match self.active_name().await.as_str() {
            "xair" => self.xair.snapshot().await,
            "dl32s" => self.dl32s.snapshot().await,
            _ => self.sim.snapshot().await,
        }
    }

    /// Main fader of the active driver, `None` when the driver has none.
    async fn active_main_fader(&self) -> Option<f32> {
        match self.active_name().await.as_str() {
            "xair" => self.xair.get_main_fader().await.ok(),
            "dl32s" => self.dl32s.get_main_fader().await.ok(),
            _ => self.sim.get_main_fader().await.ok(),
        }
    }

    /// Recall `scene` on the active driver, ramping over `fade_ms`.
    async fn recall_on_active(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        match self.active_name().await.as_str() {
            "xair" => scene.recall_to(&self.xair, fade_ms).await,
            "dl32s" => scene.recall_to(&self.dl32s, fade_ms).await,
            _ => scene.recall_to(&self.sim, fade_ms).await,
        }
    }

    /// Apply one client command from `writer` (its presence id).
    ///
    /// Returns a direct reply for `list_shows` (unicast `shows` frame),
    /// for lock denials (`{"type":"error","locked_by":...}` unicast), and
    /// for `recall_scene` when it overrode foreign locks
    /// (`{"type":"recalled","overrode_locks":[...]}` unicast); every other
    /// command broadcasts (`state` for mixer mutations like the pre-scene
    /// protocol, `shows` for library mutations, `locks` for lock changes,
    /// both `state`+`param` for `recall_scene`) and returns `None`.
    /// Mutation broadcasts carry `"by": writer`; `fader`/`mute`/`param`
    /// on a strip locked by someone else are rejected, `recall_scene`
    /// bypasses locks.
    async fn apply(&self, msg: &Value, writer: &str) -> anyhow::Result<Option<Value>> {
        // Opportunistic expiry: release stale holds before any check so a
        // just-expired strip reads as unlocked, and broadcast when the
        // sweep actually removed something.
        if self.prune_expired_locks().await {
            let _ = self.locks_tx.send(self.locks_message_inner().await);
        }
        let cmd = msg
            .get("cmd")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("missing \"cmd\""))?;
        match cmd {
            "connect" => {
                let driver = msg.get("driver").and_then(Value::as_str).unwrap_or("sim");
                let addr = msg.get("addr").and_then(Value::as_str);
                self.connect(driver, addr).await?;
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                let _ = self.state_tx.send(writer.to_string());
                Ok(None)
            }
            "fader" => {
                let ch = channel_arg(msg)?;
                if let Some(holder) = self.lock_holder(ch.0).await {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch.0, &holder)));
                    }
                }
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
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                let _ = self.state_tx.send(writer.to_string());
                Ok(None)
            }
            "mute" => {
                let ch = channel_arg(msg)?;
                if let Some(holder) = self.lock_holder(ch.0).await {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch.0, &holder)));
                    }
                }
                let muted = msg
                    .get("muted")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| anyhow::anyhow!("mute needs boolean \"muted\""))?;
                match self.active_name().await.as_str() {
                    "xair" => self.xair.set_mute(ch, muted).await?,
                    "dl32s" => self.dl32s.set_mute(ch, muted).await?,
                    _ => self.sim.set_mute(ch, muted).await?,
                }
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                let _ = self.state_tx.send(writer.to_string());
                Ok(None)
            }
            "param" => {
                let ch = channel_arg(msg)?;
                if let Some(holder) = self.lock_holder(ch.0).await {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch.0, &holder)));
                    }
                }
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
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                let _ = self
                    .param_tx
                    .send(param_message(ch.0, field, raw.clone(), Some(writer)));
                let _ = self.state_tx.send(writer.to_string());
                Ok(None)
            }
            "lock" => {
                let ch = channel_arg(msg)?;
                if let Some(holder) = self.lock_holder(ch.0).await {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch.0, &holder)));
                    }
                    // Already ours: refresh expiry below.
                }
                {
                    let mut locks = self.locks.write().await;
                    locks.insert(
                        ch.0,
                        StripLock {
                            holder: writer.to_string(),
                            expires_at: Instant::now() + self.lock_ttl,
                        },
                    );
                }
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                self.broadcast_locks().await;
                Ok(None)
            }
            "unlock" => {
                let ch = channel_arg(msg)?;
                if let Some(holder) = self.lock_holder(ch.0).await {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch.0, &holder)));
                    }
                    {
                        let mut locks = self.locks.write().await;
                        locks.remove(&ch.0);
                    }
                    self.note_write(writer).await;
                    self.touch_holder_locks(writer).await;
                    self.broadcast_locks().await;
                }
                // Unlocking an unlocked/expired strip is a no-op success.
                Ok(None)
            }
            // Server-side show library (mirrors the Tauri scene ops).
            "list_shows" => Ok(Some(self.shows_message().await)),
            "capture_scene" => {
                let show = name_arg(msg, "show")?;
                let name = name_arg(msg, "name")?;
                let snap = self.active_snapshot().await?;
                let mut scene = Scene::capture(&name, &snap);
                scene.main_fader = self.active_main_fader().await;
                let mut shows = self.shows.write().await;
                match shows.iter_mut().find(|s| s.name == show) {
                    Some(slot) => slot.add_scene(scene),
                    None => {
                        let mut fresh = Show::new(&show);
                        fresh.add_scene(scene);
                        shows.push(fresh);
                    }
                }
                drop(shows);
                let _ = self.shows_tx.send(());
                Ok(None)
            }
            "recall_scene" => {
                let show = name_arg(msg, "show")?;
                let name = name_arg(msg, "name")?;
                let fade_ms = match msg.get("fade_ms") {
                    None | Some(Value::Null) => DEFAULT_FADE_MS,
                    Some(v) => v.as_u64().ok_or_else(|| {
                        anyhow::anyhow!("recall_scene needs integer \"fade_ms\" (ms)")
                    })?,
                };
                let scene = {
                    let shows = self.shows.read().await;
                    shows
                        .iter()
                        .find(|s| s.name == show)
                        .and_then(|s| s.get(&name))
                        .cloned()
                        .ok_or_else(|| {
                            anyhow::anyhow!("unknown scene {name:?} in show {show:?}")
                        })?
                };
                self.recall_on_active(&scene, fade_ms).await?;
                self.note_write(writer).await;
                self.touch_holder_locks(writer).await;
                // Bypass locks, but report which locked strips were changed:
                // scene strips locked by someone other than the recaller.
                let mut overrode: Vec<u16> = Vec::new();
                for s in &scene.strips {
                    if let Some(holder) = self.lock_holder(s.strip.0).await {
                        if holder != writer && !overrode.contains(&s.strip.0) {
                            overrode.push(s.strip.0);
                        }
                    }
                }
                overrode.sort_unstable();
                for s in &scene.strips {
                    let _ = self
                        .param_tx
                        .send(param_message(s.strip.0, "fader", json!(s.fader), Some(writer)));
                    let _ = self
                        .param_tx
                        .send(param_message(s.strip.0, "mute", json!(s.mute), Some(writer)));
                    for (field, value) in &s.params {
                        let _ = self.param_tx.send(param_message(
                            s.strip.0,
                            field,
                            param_value_json(value),
                            Some(writer),
                        ));
                    }
                }
                let _ = self.state_tx.send(writer.to_string());
                if overrode.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(json!({"type":"recalled","overrode_locks":overrode})))
                }
            }
            "delete_scene" => {
                let show = name_arg(msg, "show")?;
                let name = name_arg(msg, "name")?;
                {
                    let mut shows = self.shows.write().await;
                    let slot = shows
                        .iter_mut()
                        .find(|s| s.name == show)
                        .ok_or_else(|| anyhow::anyhow!("unknown show {show:?}"))?;
                    if !slot.remove(&name) {
                        anyhow::bail!("unknown scene {name:?} in show {show:?}");
                    }
                }
                let _ = self.shows_tx.send(());
                Ok(None)
            }
            other => anyhow::bail!("unknown cmd {other:?}"),
        }
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

fn meters_message_for(levels: &[f32]) -> Value {
    json!({"type":"meters","levels":levels})
}

/// One `spectrum` message for channel `ch` (see `Spectrum.svelte`):
/// `{"type":"spectrum","ch":N,"bins":[...64 floats in 0..1...]}`.
fn spectrum_message(ch: usize, bins: &[f32]) -> Value {
    json!({"type":"spectrum","ch":ch,"bins":bins})
}

/// One `gr` message for channel `ch` (see `Detail.svelte` GR bars):
/// `{"type":"gr","ch":N,"gate":db,"comp":db}` — dB reduction `>= 0`.
fn gr_message(ch: usize, gate: f32, comp: f32) -> Value {
    json!({"type":"gr","ch":ch,"gate":gate,"comp":comp})
}

/// One `param` echo for channel `ch`:
/// `{"type":"param","ch":N,"field":"...","value":number|bool|string,"by":"..."}`.
///
/// Emitted on every successful `param` command (all drivers) and once per
/// applied param on `recall_scene`, so sibling UIs learn about
/// server-side changes without re-polling. `shows`/`meters`/
/// `spectrum`/`gr` frames are byte-identical to before; `state` frames
/// from mutations carry the same `"by"` writer tag.
fn param_message(ch: u16, field: &str, value: Value, by: Option<&str>) -> Value {
    match by {
        Some(writer) => json!({"type":"param","ch":ch,"field":field,"value":value,"by":writer}),
        None => json!({"type":"param","ch":ch,"field":field,"value":value}),
    }
}

fn param_value_json(pv: &ParamValue) -> Value {
    match pv {
        ParamValue::Float(v) => json!(v),
        ParamValue::Bool(b) => json!(b),
        ParamValue::Text(s) => json!(s),
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

/// Trimmed non-empty string arg (`"show"` / `"name"` for scene ops).
fn name_arg(msg: &Value, key: &str) -> anyhow::Result<String> {
    let raw = msg
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("scene command needs string \"{key}\""))?;
    let clean = raw.trim().to_string();
    if clean.is_empty() {
        anyhow::bail!("scene command needs non-empty \"{key}\"");
    }
    Ok(clean)
}

/// One scene in the `shows` event (the Tauri `get_scene` shape).
fn scene_json(scene: &Scene) -> Value {
    json!({
        "name": scene.name,
        "main_fader": scene.main_fader,
        "strips": scene.strips.iter().map(|s| json!({
            "ch": s.strip.0,
            "name": s.name,
            "fader": s.fader,
            "muted": s.mute,
        })).collect::<Vec<_>>(),
    })
}

/// One show in the `shows` event, in library order.
fn show_json(show: &Show) -> Value {
    json!({
        "name": show.name,
        "scenes": show.scenes.iter().map(scene_json).collect::<Vec<_>>(),
    })
}

fn error_message(e: impl std::fmt::Display) -> String {
    json!({"type":"error","message":e.to_string()}).to_string()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    AxumState(app): AxumState<AppState>,
) -> impl IntoResponse {
    let name = params.get("name").cloned();
    ws.on_upgrade(move |socket| handle_socket(socket, app.shared, name))
}

async fn handle_socket(socket: WebSocket, shared: Arc<Shared>, name: Option<String>) {
    // Presence: allocate + register before subscribing the new socket, so
    // the join broadcast reaches only the already-connected siblings; the
    // newcomer gets the same content as a unicast snapshot below.
    let (client_id, label) = shared.alloc_client(name.as_deref());
    shared.add_client(&client_id, &label).await;
    let (mut tx, mut rx) = socket.split();
    // Initial snapshot: mixer state, then the server-side show library,
    // then the client roster and the lock table (so the newcomer sees
    // siblings and pins immediately).
    if tx
        .send(Message::Text(shared.state_message().await.to_string()))
        .await
        .is_err()
    {
        shared.remove_client(&client_id).await;
        return;
    }
    if tx
        .send(Message::Text(shared.shows_message().await.to_string()))
        .await
        .is_err()
    {
        shared.remove_client(&client_id).await;
        return;
    }
    if tx
        .send(Message::Text(shared.clients_message().await.to_string()))
        .await
        .is_err()
    {
        shared.remove_client(&client_id).await;
        return;
    }
    if tx
        .send(Message::Text(shared.locks_message().await.to_string()))
        .await
        .is_err()
    {
        shared.remove_client(&client_id).await;
        return;
    }
    let mut state_rx = shared.state_tx.subscribe();
    let mut shows_rx = shared.shows_tx.subscribe();
    let mut param_rx = shared.param_tx.subscribe();
    let mut clients_rx = shared.clients_tx.subscribe();
    let mut locks_rx = shared.locks_tx.subscribe();
    // Per-connection RTA: fed by the same meter frames broadcast on the
    // ~10 Hz tick below (no new ports/threads — spectrum is derived
    // per-socket from the already-polled levels). Starts empty and grows
    // to the mixer width on the first frame.
    let mut rta = RtaProcessor::new(0);
    let mut meters = tokio::time::interval(std::time::Duration::from_millis(100));
    // Skip the immediate first tick so a fresh client gets state first.
    meters.tick().await;
    // Counts 10 Hz meter ticks; GR broadcasts on every second tick (~5 Hz)
    // to bound traffic separately from meters/spectrum.
    let mut tick: u64 = 0;
    loop {
        tokio::select! {
            incoming = rx.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Value>(&text) {
                            Ok(cmd) => {
                                match shared.apply(&cmd, &client_id).await {
                                    Ok(Some(reply)) => {
                                        let _ = tx.send(Message::Text(reply.to_string())).await;
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        let _ = tx.send(Message::Text(error_message(e))).await;
                                    }
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
                tick += 1;
                let levels = shared.meter_levels().await;
                rta.push_frame(&MeterFrame { levels: levels.clone() });
                if tx.send(Message::Text(meters_message_for(&levels).to_string())).await.is_err() {
                    break;
                }
                // Spectrum follows meters, first SPECTRUM_CHANNELS channels.
                let mut dead = false;
                for (ch, _) in levels.iter().enumerate().take(SPECTRUM_CHANNELS) {
                    let bins = rta.spectrum(ch);
                    if tx.send(Message::Text(spectrum_message(ch, &bins).to_string())).await.is_err() {
                        dead = true;
                        break;
                    }
                }
                if dead {
                    break;
                }
                // GR follows at ~5 Hz (every 2nd meter tick), first
                // GR_CHANNELS channels, bounded by the live strip count so
                // phantom strips never get a frame.
                if tick % 2 == 0 {
                    let n = levels.len().min(GR_CHANNELS);
                    for ch in 0..n {
                        let (gate, comp) = shared.gr_db(ch).await;
                        if tx.send(Message::Text(gr_message(ch, gate, comp).to_string())).await.is_err() {
                            dead = true;
                            break;
                        }
                    }
                    if dead {
                        break;
                    }
                }
            }
            changed = state_rx.recv() => {
                let writer = match changed {
                    Ok(by) => by,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                let by = if writer.is_empty() { None } else { Some(writer.as_str()) };
                let s = shared.state_message_with_by(by).await;
                if tx.send(Message::Text(s.to_string())).await.is_err() {
                    break;
                }
            }
            changed = shows_rx.recv() => {
                if changed.is_err() {
                    break;
                }
                let s = shared.shows_message().await;
                if tx.send(Message::Text(s.to_string())).await.is_err() {
                    break;
                }
            }
            param = param_rx.recv() => {
                match param {
                    Ok(msg) => {
                        if tx.send(Message::Text(msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
            clients = clients_rx.recv() => {
                match clients {
                    Ok(msg) => {
                        if tx.send(Message::Text(msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Re-send the current roster after a lag so the UI
                        // never sticks on a stale count.
                        let s = shared.clients_message().await;
                        if tx.send(Message::Text(s.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            locks = locks_rx.recv() => {
                match locks {
                    Ok(msg) => {
                        if tx.send(Message::Text(msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Same lag-recovery as clients: re-send current pins.
                        let s = shared.locks_message().await;
                        if tx.send(Message::Text(s.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }
    shared.remove_client(&client_id).await;
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
    run_with_lock_ttl(listener, opts, LOCK_TTL).await
}

/// [`run`] with an explicit strip-lock TTL.
///
/// Production passes [`LOCK_TTL`] (60 s); tests pass a short TTL (e.g.
/// 200 ms) to cover expiry without waiting a minute. A background sweeper
/// prunes expired holds every `ttl/2` (clamped 50 ms..5 s) and broadcasts
/// the updated `locks` frame so idle expiry still notifies siblings.
pub async fn run_with_lock_ttl(
    listener: TcpListener,
    opts: ServeOptions,
    lock_ttl: Duration,
) -> anyhow::Result<()> {
    let local = listener.local_addr()?;
    let shared = Arc::new(Shared::with_lock_ttl(lock_ttl));

    // Expiry sweeper: lazy pruning in `apply` covers the next-command path,
    // but an idle server still needs to release + broadcast expired pins.
    {
        let sweeper = Arc::clone(&shared);
        let interval = (lock_ttl / 2).clamp(Duration::from_millis(50), Duration::from_secs(5));
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                if sweeper.prune_expired_locks().await {
                    let _ = sweeper
                        .locks_tx
                        .send(sweeper.locks_message_inner().await);
                }
            }
        });
    }

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
