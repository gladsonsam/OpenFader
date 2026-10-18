//! [`Engine`]: the transport-agnostic mixer session, moved out of
//! `crates/serve` (ME1). All session behaviour lives here; adapters only
//! move [`Cmd`]/[`Event`](crate::protocol::Event) JSON.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use openmix_core::{
    access::{Policy, Role},
    params,
    rta::RtaProcessor,
    show::{Scene, Show, DEFAULT_FADE_MS},
    MeterFrame, MixerDriver, ParamValue, StripId,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::protocol::{Cmd, DeviceDesc, Event, FxSlotState, OfflineChange, ParamDef, PatchEntry, Scene as SceneFrame, Show as ShowFrame};

/// Link re-broadcast interval (R2): the `link` event goes out on every
/// change plus on this cadence with a fresh round-trip measurement.
pub const LINK_INTERVAL: Duration = Duration::from_secs(2);

/// R2 scene slot range: numbered library slots `001..=999`.
pub const SCENE_MIN: u16 = 1;
pub const SCENE_MAX: u16 = 999;

/// R2 FX poll interleave: every Nth param-poller tick also refreshes one
/// FX slot param (cache-only broadcast on change; silent on error).
pub const FX_POLL_EVERY: u64 = 32;

/// R2 default desk sample rate reported in `link`/`info` (all supported
/// desks run 48 kHz; a desk reporting otherwise is a finding, not a const
/// change — see `docs/LIVE-RIG.md`).
pub const DESK_SAMPLE_RATE: u32 = 48000;

/// Per-driver default poller pipelining (`max_inflight`): concurrent
/// driver queries per poll tick. `xair` stays at 1 until the rig measures
/// a sustainable query rate (LIVE-RIG P19+); `sim` pipelines higher (its
/// reads are in-process); `dl32s` is small enough that 1 suffices.
pub fn default_inflight(driver: &str) -> usize {
    match driver {
        "sim" => 4,
        "dl32s" => 1,
        _ => 1,
    }
}

/// R2 scene numbers live in `001..=999` (`scene_save`/`scene_recall`).
pub fn validate_scene_n(n: u16) -> anyhow::Result<u16> {
    if !(SCENE_MIN..=SCENE_MAX).contains(&n) {
        anyhow::bail!("scene number {n} out of range 001..=999");
    }
    Ok(n)
}

/// Current unix time in seconds (scene `saved_at` / devices `scanned_at`).
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One stored R2 numbered scene: full-plane capture (every catalogue
/// value of every strip + main + FX + routing).
#[derive(Clone, Debug)]
pub struct StoredScene {
    pub number: u16,
    pub name: String,
    pub saved_at: u64,
    pub main_fader: Option<f32>,
    /// Per strip: name + fader + mute + extra catalogue params (JSON
    /// engineering values, excluding fader/mute/name which ride above).
    pub strips: Vec<StoredStrip>,
    /// FX rack snapshot (slot, type, on, bypass, normalized params).
    pub fx: Vec<StoredFx>,
    /// Routing snapshot (strip, group, 1-based index).
    pub routing: Vec<(u16, String, u8)>,
}

/// One strip's contribution to a [`StoredScene`].
#[derive(Clone, Debug)]
pub struct StoredStrip {
    pub strip: u16,
    pub name: String,
    pub fader: f32,
    pub mute: bool,
    pub params: HashMap<String, Value>,
}

/// One FX slot's contribution to a [`StoredScene`].
#[derive(Clone, Debug)]
pub struct StoredFx {
    pub slot: u8,
    pub fx_type: String,
    pub on: bool,
    pub bypass: bool,
    pub params: BTreeMap<String, f32>,
}

/// Engine-side FX rack row (runtime mirror of the driver's rack).
#[derive(Clone, Debug)]
struct FxSlotRt {
    slot: u8,
    fx_type: String,
    on: bool,
    bypass: bool,
    params: BTreeMap<String, f32>,
}

/// R2 offline-editing state: while `active`, param writes land on the
/// offline (sim) model and every write is recorded as a change; after a
/// reconnect with pending changes the engine emits `offline_diff` and
/// waits for `offline_push {confirm:true}` (or `offline_discard`).
#[derive(Clone, Debug, Default)]
pub struct OfflineState {
    pub active: bool,
    pub model: String,
    pub prev_driver: String,
    pub changes: Vec<OfflineChange>,
    pub pending_push: bool,
}

/// R2 link state behind the `link` event.
#[derive(Clone, Debug)]
pub struct LinkState {
    pub state: String,
    pub name: String,
    pub ip: String,
    pub fs: u32,
    pub latency_ms: f32,
    pub device_id: Option<String>,
}

impl Default for LinkState {
    fn default() -> Self {
        Self {
            state: "disconnected".to_string(),
            name: String::new(),
            ip: String::new(),
            fs: DESK_SAMPLE_RATE,
            latency_ms: 0.0,
            device_id: None,
        }
    }
}

/// R2 LAN discovery behind a trait: production uses [`RealScanner`]
/// (`crates/transport` mDNS/OSC broadcast discovery); tests inject a fake
/// so no test ever touches the network.
pub trait DeviceScanner: Send + Sync {
    fn scan(
        &self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + '_>>;
}

/// Production LAN scanner: X-Air OSC broadcast probes
/// (`/xinfo`, `/info`) via `crates/transport`. UNVERIFIED-ON-RIG: mapping
/// replies to devices has only ever run against the fake in tests.
pub struct RealScanner;

impl DeviceScanner for RealScanner {
    fn scan(
        &self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + '_>> {
        Box::pin(async move {
            let mut out = Vec::new();
            match openmix_transport::discovery::discover_xair(timeout).await {
                Ok(devs) => {
                    for d in devs {
                        let ip = d.addr.ip().to_string();
                        out.push(DeviceDesc {
                            id: format!("xair-{ip}"),
                            name: format!("X-Air {}", d.model),
                            model: d.model,
                            ip,
                            kind: "console".to_string(),
                            state: "available".to_string(),
                            fw: None,
                        });
                    }
                }
                Err(_) => {}
            }
            out
        })
    }
}

/// Max channels with per-tick spectrum output (first N channels only).
///
/// 16 covers a full drum/band layer on typical 16/24/32ch surfaces while
/// keeping the source-rate fanout bounded: 16 msgs x 64 floats/tick stays
/// well under one MTU-friendly JSON burst per tick. Channels beyond this cap
/// still stream `meters` levels.
pub const SPECTRUM_CHANNELS: usize = 16;

/// Max channels with per-tick GR output (first N channels only).
///
/// Same 16ch cap as [`SPECTRUM_CHANNELS`]. GR reads are cache-only (no extra
/// wire traffic), so GR rides every 20 Hz meter tick. `gate` / `comp` read
/// 0.0 until the DYN blob cache fills.
pub const GR_CHANNELS: usize = 16;

/// Default per-strip lock TTL: holder id + expiry, refreshed by holder
/// activity, auto-released on disconnect/expiry.
pub const LOCK_TTL: Duration = Duration::from_secs(60);

/// Param-plane poller tick: one driver query per tick, so the production
/// budget is exactly 20 queries/s total. The poller runs on its own task
/// and never blocks fader/meter traffic.
pub const PARAM_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Startup options for [`Engine::new`].
#[derive(Clone)]
pub struct EngineConfig {
    /// Strip-lock TTL (production: [`LOCK_TTL`]).
    pub lock_ttl: Duration,
    /// Param-plane poll tick (production: [`PARAM_POLL_INTERVAL`]).
    pub poll_interval: Duration,
    /// Optional sim desk preset (hardware drivers ignore it).
    pub preset: Option<openmix_sim::SimPreset>,
    /// R2 scene library directory: numbered scenes persist as
    /// `scenes.json` here and reload on the next [`Engine::new`].
    /// `None` = in-memory only (tests default here and pass a temp dir
    /// when they want persistence).
    pub data_dir: Option<PathBuf>,
    /// R2 poller pipelining: max concurrent driver queries per poll tick.
    /// `None` = per-driver default ([`default_inflight`]: sim 4, xair 1
    /// until rig-verified, dl32s 1). `--poll-inflight` sets this.
    pub poll_inflight: Option<usize>,
    /// R2 LAN scanner (default: [`RealScanner`]). Tests inject a fake so
    /// discovery never touches the network.
    pub scanner: Option<Arc<dyn DeviceScanner>>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            lock_ttl: LOCK_TTL,
            poll_interval: PARAM_POLL_INTERVAL,
            preset: None,
            data_dir: None,
            poll_inflight: None,
            scanner: None,
        }
    }
}

/// One connected client (presence tracking + mailbox + role).
struct ClientEntry {
    mailbox: mpsc::UnboundedSender<Event>,
    #[allow(dead_code)]
    label: String,
    #[allow(dead_code)]
    connected_at: Instant,
    last_write: Option<Instant>,
    policy: Policy,
    /// MR1 realtime preference: when true the adapter encodes `Meters`/`Gr`
    /// as WS BINARY frames (via [`Event::to_binary`](crate::protocol::Event::to_binary))
    /// instead of JSON text. Set by [`Cmd::Hello`](crate::protocol::Cmd::Hello);
    /// default off so old clients keep byte-identical JSON.
    binary_meters: bool,
    /// R2 meter-tap preference: when true the engine fans per-strip tap
    /// frames (`in`/`post`/`gr_gate`/`gr_comp`) to this client (JSON, or
    /// WS BINARY type 3 in the adapter). Set by `hello {"taps":true}`;
    /// default off so old clients see no new traffic.
    taps: bool,
}

/// One strip lock: holder id + expiry.
#[derive(Clone, Debug)]
struct StripLock {
    holder: String,
    expires_at: Instant,
}

/// RAII marker for the MR1 write lane: increments `writes_in_flight` on
/// creation, decrements on drop, so every [`Engine::apply`] return path
/// (including `?` early-outs) releases the poller-yield flag.
struct WriteGuard<'a> {
    n: &'a AtomicU64,
}

impl<'a> WriteGuard<'a> {
    fn new(n: &'a AtomicU64) -> Self {
        n.fetch_add(1, Ordering::SeqCst);
        Self { n }
    }
}

impl Drop for WriteGuard<'_> {
    fn drop(&mut self) {
        self.n.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The transport-agnostic mixer session.
///
/// Static dispatch over drivers (`MixerDriver` is not dyn-compatible due to
/// async methods). Owns the driver bank + active selection, the strip state
/// and param-plane caches, the poller, the meter/spectrum/GR ticker, the
/// show library, locks, per-client policies and the roster. Background tasks
/// (poller, lock sweeper, meter ticker) start on the first [`attach`](Engine::attach)
/// and exit when the last `Arc<Engine>` is dropped.
pub struct Engine {
    sim: openmix_sim::SimDriver,
    xair: openmix_proto_xair::XAirDriver,
    dl32s: openmix_proto_dl32s::Dl32sDriver,
    active: RwLock<String>,
    clients: RwLock<HashMap<String, ClientEntry>>,
    next_client: AtomicU64,
    locks: RwLock<HashMap<u16, StripLock>>,
    shows: RwLock<Vec<Show>>,
    /// Param-plane cache: channel -> catalogue id -> last broadcast
    /// engineering-unit JSON value. Updated by the poller and by the
    /// `param`-write refresh; cleared on driver switch.
    param_cache: RwLock<HashMap<u16, HashMap<String, Value>>>,
    /// (`driver`, `param`) pairs dropped from the poll set after an
    /// unsupported/unknown/timeout read, reported in `caps.unsupported`.
    param_dropped: RwLock<HashSet<(String, String)>>,
    /// Round-robin cursor into the poll list.
    poll_cursor: AtomicU64,
    /// Write-priority lane (MR1): number of commands currently inside
    /// [`apply`](Engine::apply). The param poller checks this at the top of
    /// every tick and skips the tick (counting [`poll_skips`](Engine::poll_skips))
    /// while any write is in flight, so a fader/mute/param write is never
    /// queued behind a poll read. Writes themselves run inline on the
    /// caller's task — they never wait on poller or meter state — and meter
    /// fan-out is a third lane (read-only driver calls from the ticker
    /// task). See `crates/engine/README.md` "Write priority".
    writes_in_flight: AtomicU64,
    /// Diagnostic: poller ticks skipped due to [`writes_in_flight`](Engine::writes_in_flight).
    poll_skips: AtomicU64,
    /// Static poll lists per driver: every topology strip crossed with the
    /// driver's supported catalogue ids, filtered by catalogue scopes.
    /// Built once per driver (topology × caps × scopes never changes at
    /// runtime); drop-class errors only SKIP entries (see `poll_once`), so
    /// the per-tick path does zero allocation and stays at budget even for
    /// the 77-strip sim sweep (~8k pairs).
    poll_static: RwLock<HashMap<String, Vec<(u16, &'static str)>>>,
    /// Cached visible-group lane lists: (driver, visible set, pairs).
    /// Rebuilt when the visible set changes (focus is infrequent).
    poll_visible: RwLock<Option<(String, Vec<u16>, Vec<(u16, &'static str)>)>>,    /// Engine construction time: source of the monotonic `t_ms` clock in
    /// binary meter frames ([`now_ms`](Engine::now_ms)).
    boot: Instant,
    /// Focus queue: `(driver, ch, param, owner)` tuples pushed by the `focus`
    /// command, drained one per poll tick ahead of round-robin (same 20
    /// q/s budget — focus is priority, not a faster lane). Most-recent
    /// focus goes to the FRONT (a new focus supersedes older ones) with
    /// pending (strip, param) pairs de-duplicated; the queue is capped and
    /// a detached client's pending focus is dropped with it (see
    /// [`Engine::remove_client`]). Stale entries (driver switch) are
    /// skipped on drain.
    focus_queue: RwLock<VecDeque<(String, u16, String, String)>>,
    lock_ttl: Duration,
    poll_interval: Duration,
    tasks_started: AtomicBool,
    /// R2 devices: last discovery result (`scan` refreshes; `scanned_at`
    /// is unix seconds, 0 = never scanned).
    devices: RwLock<Vec<DeviceDesc>>,
    scanned_at: AtomicU64,
    /// R2 link state behind the `link` event.
    link: RwLock<LinkState>,
    /// R2 numbered scene library (slot -> scene) + current slot marker.
    /// Persisted as `scenes.json` under `data_dir` when configured.
    scenes_lib: RwLock<HashMap<u16, StoredScene>>,
    scenes_current: RwLock<Option<u16>>,
    data_dir: Option<PathBuf>,
    /// R2 FX cache: engine-side mirror of the active driver's rack
    /// (rebuilt on every driver switch; sim-backed, best-effort on xair).
    fx_cache: RwLock<Vec<FxSlotRt>>,
    /// R2 FX poll cursor (round-robin over the rack, [`FX_POLL_EVERY`]).
    fx_cursor: AtomicU64,
    /// R2 offline-editing state.
    offline: RwLock<OfflineState>,
    /// R2 visible strips (most-recent `focus`, capped): the poller's
    /// visible-group lane serves these strips' EQ/dyn/sends first.
    visible: RwLock<Vec<u16>>,
    /// Round-robin cursor into the visible-group lane.
    prio_cursor: AtomicU64,
    /// Entries served since the visible boost armed (sweep tracking).
    prio_taken: AtomicU64,
    /// Visible boost armed: the lane serves one full sweep of the visible
    /// strips' EQ/dyn/sends after each focus, then yields to the
    /// background (one-shot — a perpetual round-robin would starve the
    /// background sweep, since the cursor wraps without end).
    prio_armed: AtomicBool,
    /// Lane splitter tick: alternates the background reservation (see
    /// [`Engine::poll_tick`]).
    lane_tick: AtomicU64,
    /// R2 poller pipelining: max concurrent driver queries per tick
    /// (`None` in config = [`default_inflight`] per driver).
    poll_inflight: Option<usize>,
    /// Poll-batch RTT exponential moving average in microseconds
    /// (adaptive pipelining: the effective inflight halves while the EMA
    /// exceeds the poll tick).
    rtt_ema_us: AtomicU64,
    /// R2 LAN scanner (real discovery or an injected fake).
    scanner: Arc<dyn DeviceScanner>,
}

impl Engine {
    /// Build a new session. No background work starts here (safe to call
    /// outside a runtime); the poller/sweeper/meter tasks spawn on the
    /// first [`attach`](Engine::attach).
    pub fn new(config: EngineConfig) -> Arc<Self> {
        let data_dir = config.data_dir.clone();
        let scenes_lib = load_scenes(data_dir.as_deref());
        let engine = Arc::new(Self {
            sim: match config.preset {
                Some(p) => openmix_sim::SimDriver::with_preset(p),
                None => openmix_sim::SimDriver::new(),
            },
            xair: openmix_proto_xair::XAirDriver::new(),
            dl32s: openmix_proto_dl32s::Dl32sDriver::new(),
            active: RwLock::new("sim".to_string()),
            clients: RwLock::new(HashMap::new()),
            next_client: AtomicU64::new(1),
            locks: RwLock::new(HashMap::new()),
            shows: RwLock::new(Vec::new()),
            param_cache: RwLock::new(HashMap::new()),
            param_dropped: RwLock::new(HashSet::new()),
            poll_cursor: AtomicU64::new(0),
            writes_in_flight: AtomicU64::new(0),
            poll_skips: AtomicU64::new(0),
            poll_static: RwLock::new(HashMap::new()),
            poll_visible: RwLock::new(None),
            boot: Instant::now(),
            focus_queue: RwLock::new(VecDeque::new()),
            lock_ttl: config.lock_ttl,
            poll_interval: config.poll_interval,
            tasks_started: AtomicBool::new(false),
            devices: RwLock::new(sim_devices()),
            scanned_at: AtomicU64::new(0),
            link: RwLock::new(LinkState {
                state: "connected".to_string(),
                name: "FOH Console".to_string(),
                ip: "192.168.1.20".to_string(),
                fs: DESK_SAMPLE_RATE,
                latency_ms: 1.2,
                device_id: Some("foh".to_string()),
            }),
            scenes_lib: RwLock::new(scenes_lib),
            scenes_current: RwLock::new(None),
            data_dir,
            fx_cache: RwLock::new(Vec::new()),
            fx_cursor: AtomicU64::new(0),
            offline: RwLock::new(OfflineState::default()),
            visible: RwLock::new(Vec::new()),
            prio_cursor: AtomicU64::new(0),
            prio_taken: AtomicU64::new(0),
            prio_armed: AtomicBool::new(false),
            lane_tick: AtomicU64::new(0),
            poll_inflight: config.poll_inflight,
            rtt_ema_us: AtomicU64::new(0),
            scanner: config
                .scanner
                .unwrap_or_else(|| Arc::new(RealScanner)),
        });
        // FX cache for the default driver (sim): seeded synchronously so
        // the first attach already carries the rack.
        engine.refresh_fx_cache_sync();
        engine
    }

    /// Name of the active driver (`sim`/`xair`/`dl32s`).
    pub fn active_driver(&self) -> String {
        self.active.read().unwrap().clone()
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

    /// Select `driver` and connect it. Failures leave the old driver
    /// active; callers (adapters) stay up so a client can retry.
    pub async fn connect(&self, driver: &str, addr: Option<&str>) -> anyhow::Result<()> {
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
        *self.active.write().unwrap() = driver.to_string();
        Ok(())
    }

    /// Attach a client: allocate its id, register it, and queue the
    /// initial snapshot (`you`, `state`, `shows`, `clients`, `locks`,
    /// `caps`, `devices`, `link`, `info`?, `scenes`, `fx`, `routing`,
    /// then one `params` frame per cached channel) into the new
    /// mailbox before any live broadcast can reach it. The join roster
    /// update goes to the already-connected siblings only.
    ///
    /// Dropping the returned handle detaches the client (roster update +
    /// lock auto-release); call [`ClientHandle::detach`] for an explicit
    /// async detach. Starts the background poller/sweeper/meter tasks on
    /// first call (requires a Tokio runtime).
    pub async fn attach(self: &Arc<Self>, label: Option<String>) -> ClientHandle {
        self.ensure_tasks();
        let (id, clean) = self.alloc_client(label.as_deref());
        let (tx, rx) = mpsc::unbounded_channel();

        // Snapshot the world before registering, so no live broadcast can
        // overtake the initial frames in the newcomer's mailbox.
        let you = Event::You { id: id.clone() };
        let state = self.state_message(None).await;
        let shows = self.shows_message();
        let locks = self.locks_message();
        let caps = self.caps_message();
        let devices = self.devices_message();
        let link = self.link_message_cached();
        let info = self.info_message().await.ok();
        let scenes = self.scenes_message();
        let fx = self.fx_message().await;
        let routing = self.routing_message().await;
        let params = self.params_snapshot();

        {
            let mut clients = self.clients.write().unwrap();
            clients.insert(
                id.clone(),
                ClientEntry {
                    mailbox: tx,
                    label: clean,
                    connected_at: Instant::now(),
                    last_write: None,
                    policy: Policy::admin(),
                    binary_meters: false,
                    taps: false,
                },
            );
        }
        let roster = self.clients_message();

        let handle = ClientHandle {
            id: id.clone(),
            engine: Arc::clone(self),
            events: rx,
        };
        // The entry exists again (unless detached concurrently); push the
        // snapshot in socket order. The R1 six-frame prefix (you, state,
        // shows, clients, locks, caps) is unchanged; R2 frames follow
        // (devices, link, info?, scenes, fx, routing) before params.
        if let Some(entry) = self.clients.read().unwrap().get(&id) {
            let _ = entry.mailbox.send(you);
            let _ = entry.mailbox.send(state);
            let _ = entry.mailbox.send(shows);
            let _ = entry.mailbox.send(roster);
            let _ = entry.mailbox.send(locks);
            let _ = entry.mailbox.send(caps);
            let _ = entry.mailbox.send(devices);
            let _ = entry.mailbox.send(link);
            if let Some(info) = info {
                let _ = entry.mailbox.send(info);
            }
            let _ = entry.mailbox.send(scenes);
            let _ = entry.mailbox.send(fx);
            let _ = entry.mailbox.send(routing);
            for frame in params {
                let _ = entry.mailbox.send(frame);
            }
        }
        // Siblings learn about the join; the newcomer already has it.
        self.broadcast_except(&self.clients_message(), &id);
        handle
    }

    /// Replace a connected client's access policy (default: admin).
    ///
    /// Adapters use this to host restricted roles (e.g. musician) on one
    /// shared session; denied writes surface as unicast `error` events.
    /// Returns false when the client is unknown.
    pub fn set_client_policy(&self, id: &str, policy: Policy) -> bool {
        match self.clients.write().unwrap().get_mut(id) {
            Some(entry) => {
                entry.policy = policy;
                true
            }
            None => false,
        }
    }

    /// The client's current role (admin when unknown).
    pub fn client_role(&self, id: &str) -> Role {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .map(|e| e.policy.role)
            .unwrap_or(Role::Admin)
    }

    fn ensure_tasks(self: &Arc<Self>) {
        if self.tasks_started.swap(true, Ordering::SeqCst) {
            return;
        }
        // Param-plane poller: pipelined batch per tick (R2; one query per
        // tick at default inflight 1, more when configured), diff
        // broadcasts on change, caps updates on unsupported-dropout.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            let interval = self.poll_interval;
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(interval);
                loop {
                    tick.tick().await;
                    match weak.upgrade() {
                        Some(engine) => engine.poll_tick().await,
                        None => break,
                    }
                }
            });
        }
        // Expiry sweeper: releases + broadcasts expired pins so an idle
        // session still notifies siblings, and prunes detached mailboxes.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            let interval =
                (self.lock_ttl / 2).clamp(Duration::from_millis(50), Duration::from_secs(5));
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(interval);
                loop {
                    tick.tick().await;
                    match weak.upgrade() {
                        Some(engine) => {
                            engine.prune_closed();
                            if engine.prune_expired_locks() {
                                engine.broadcast(&engine.locks_message_inner());
                            }
                        }
                        None => break,
                    }
                }
            });
        }
        // Meter/spectrum/GR ticker at the SOURCE rate
        // ([`METER_INTERVAL`](openmix_transport::METER_INTERVAL) = 50 ms =
        // 20 Hz, the desk rate): no adapter-side downsampling. GR rides the
        // same tick (cache-only reads, no extra wire traffic). Slow clients
        // get latest-only delivery in the adapter (`crates/serve`), so the
        // engine always fans out the freshest frame. R2 taps ride here too,
        // but only while at least one client negotiated them (old clients
        // see no new traffic).
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut rta = RtaProcessor::new(0);
                let mut tick =
                    tokio::time::interval(openmix_transport::METER_INTERVAL);
                // Skip the immediate first tick so a fresh client gets
                // state first.
                tick.tick().await;
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    let levels = engine.meter_levels().await;
                    rta.push_frame(&MeterFrame {
                        levels: levels.clone(),
                    });
                    engine.broadcast(&Event::Meters {
                        levels: levels.clone(),
                    });
                    if engine.any_taps_client() {
                        for (tap, tap_levels) in engine.tap_frames().await {
                            engine.broadcast(&Event::Taps {
                                tap,
                                levels: tap_levels,
                            });
                        }
                    }
                    for (ch, _) in levels.iter().enumerate().take(SPECTRUM_CHANNELS) {
                        let bins = rta.spectrum(ch);
                        engine.broadcast(&Event::Spectrum {
                            ch: ch as u16,
                            bins,
                        });
                    }
                    let m = levels.len().min(GR_CHANNELS);
                    for ch in 0..m {
                        let (gate, comp) = engine.gr_db(ch).await;
                        engine.broadcast(&Event::Gr {
                            ch: ch as u16,
                            gate,
                            comp,
                        });
                    }
                }
            });
        }
        // R2 link ticker: re-emit `link` every ~2 s with a fresh
        // round-trip measurement (spec: on change + periodic).
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(LINK_INTERVAL);
                // Skip the immediate first tick (connect paths emit link).
                tick.tick().await;
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    let event = engine.link_message().await;
                    engine.broadcast(&event);
                }
            });
        }
    }

    /// Send an event to every connected client (skips closed mailboxes;
    /// the sweeper prunes them).
    fn broadcast(&self, event: &Event) {
        let clients = self.clients.read().unwrap();
        for entry in clients.values() {
            let _ = entry.mailbox.send(event.clone());
        }
    }

    /// [`broadcast`] to everyone except one client (join roster updates).
    fn broadcast_except(&self, event: &Event, skip: &str) {
        let clients = self.clients.read().unwrap();
        for (id, entry) in clients.iter() {
            if id != skip {
                let _ = entry.mailbox.send(event.clone());
            }
        }
    }

    /// Unicast one event to a single client (direct replies, denials).
    fn unicast(&self, id: &str, event: Event) {
        if let Some(entry) = self.clients.read().unwrap().get(id) {
            let _ = entry.mailbox.send(event);
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

    /// Detach a client: roster update + auto-release of its strip locks +
    /// drop of its pending focus work (so a gone client's flood never
    /// starves the next focus). Idempotent; also runs on [`ClientHandle`]
    /// drop.
    fn remove_client(&self, id: &str) {
        let existed = self.clients.write().unwrap().remove(id).is_some();
        if !existed {
            return;
        }
        self.focus_queue
            .write()
            .unwrap()
            .retain(|(_, _, _, owner)| owner != id);
        self.broadcast(&self.clients_message());
        let released = {
            let mut locks = self.locks.write().unwrap();
            let before = locks.len();
            locks.retain(|_, l| l.holder != id);
            locks.len() != before
        };
        if released {
            self.broadcast(&self.locks_message_inner());
        }
    }

    /// Drop entries whose mailbox is closed (receiver dropped without
    /// detach); the next roster broadcast carries the pruned count.
    fn prune_closed(&self) {
        let dead: Vec<String> = self
            .clients
            .read()
            .unwrap()
            .iter()
            .filter(|(_, e)| e.mailbox.is_closed())
            .map(|(id, _)| id.clone())
            .collect();
        for id in dead {
            self.remove_client(&id);
        }
    }

    /// Record a mutation write for `writers` tracking (last 10 s).
    fn note_write(&self, id: &str) {
        let now = Instant::now();
        if let Some(info) = self.clients.write().unwrap().get_mut(id) {
            info.last_write = Some(now);
        }
    }

    fn policy_of(&self, id: &str) -> Policy {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .map(|e| e.policy.clone())
            .unwrap_or_else(Policy::admin)
    }

    /// Current `clients` event (`count` + `writers` with a mutation in the
    /// last 10 s, sorted for deterministic snapshots).
    fn clients_message(&self) -> Event {
        let clients = self.clients.read().unwrap();
        let now = Instant::now();
        let mut writers: Vec<String> = clients
            .iter()
            .filter(|(_, c)| {
                c.last_write
                    .is_some_and(|t| now.duration_since(t).as_secs() < 10)
            })
            .map(|(id, _)| id.clone())
            .collect();
        writers.sort();
        Event::Clients {
            count: clients.len(),
            writers,
        }
    }

    /// Drop expired holds. Returns true when anything was removed.
    fn prune_expired_locks(&self) -> bool {
        let now = Instant::now();
        let mut locks = self.locks.write().unwrap();
        let before = locks.len();
        locks.retain(|_, l| now < l.expires_at);
        locks.len() != before
    }

    /// Holder of `ch`, or `None` when unlocked/expired.
    fn lock_holder(&self, ch: u16) -> Option<String> {
        let locks = self.locks.read().unwrap();
        let entry = locks.get(&ch)?;
        if Instant::now() >= entry.expires_at {
            return None;
        }
        Some(entry.holder.clone())
    }

    /// Refresh expiry on every non-expired hold owned by `writer`.
    fn touch_holder_locks(&self, writer: &str) {
        let deadline = Instant::now() + self.lock_ttl;
        let now = Instant::now();
        let mut locks = self.locks.write().unwrap();
        for entry in locks.values_mut() {
            if entry.holder == writer && now < entry.expires_at {
                entry.expires_at = deadline;
            }
        }
    }

    /// Current `locks` event, pruning expired holds first.
    fn locks_message(&self) -> Event {
        let _ = self.prune_expired_locks();
        self.locks_message_inner()
    }

    /// Build the `locks` event from the current table. Expired entries are
    /// skipped defensively even if pruning was skipped.
    fn locks_message_inner(&self) -> Event {
        let locks = self.locks.read().unwrap();
        let now = Instant::now();
        let mut holds = std::collections::BTreeMap::new();
        let mut chs: Vec<u16> = locks.keys().copied().collect();
        chs.sort_unstable();
        for ch in chs {
            if let Some(entry) = locks.get(&ch) {
                if now < entry.expires_at {
                    holds.insert(ch.to_string(), entry.holder.clone());
                }
            }
        }
        Event::Locks { holds }
    }

    /// Denied-write error event (`locked_by` names the holder).
    fn locked_error(ch: u16, holder: &str) -> Event {
        Event::Error {
            message: format!("strip {ch} locked by {holder}"),
            locked_by: Some(holder.to_string()),
            ch: Some(ch),
        }
    }

    /// Current meter levels for the active driver (one entry per strip).
    async fn meter_levels(&self) -> Vec<f32> {
        match self.active_driver().as_str() {
            "xair" => self.xair.poll_meters().levels,
            "dl32s" => self.dl32s.poll_meters().levels,
            _ => self.sim.poll_meters().levels,
        }
    }

    /// Gate/comp GR (dB reduction `>= 0`) for one 0-based channel.
    ///
    /// xair reads the DYN blob cache (`gate_gr`/`comp_gr` param path); no
    /// data yet (or out of range) falls back to 0.0 so the tick never drops
    /// a frame. sim/dl32s have no GR source and always read (0.0, 0.0).
    async fn gr_db(&self, ch: usize) -> (f32, f32) {
        if self.active_driver().as_str() != "xair" {
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

    /// Full `shows` event: every show with its scenes.
    fn shows_message(&self) -> Event {
        let shows = self.shows.read().unwrap();
        Event::Shows {
            shows: shows.iter().map(show_frame).collect(),
        }
    }

    /// Supported catalogue ids of the named driver (its
    /// [`MixerDriver::capabilities`]).
    fn driver_capabilities(&self, driver: &str) -> Vec<&'static str> {
        match driver {
            "xair" => self.xair.capabilities().to_vec(),
            "dl32s" => self.dl32s.capabilities().to_vec(),
            _ => self.sim.capabilities().to_vec(),
        }
    }

    /// Channel-strip count of the named driver (legacy `channel_count`;
    /// poll/attach geometry now uses [`Engine::driver_strips`]).
    #[allow(dead_code)]
    fn driver_channels(&self, driver: &str) -> usize {
        match driver {
            "xair" => self.xair.channel_count(),
            "dl32s" => self.dl32s.channel_count(),
            _ => self.sim.channel_count(),
        }
    }

    /// Total addressable strips of the named driver (topology total).
    fn driver_strips(&self, driver: &str) -> usize {
        match driver {
            "xair" => self.xair.strip_count(),
            "dl32s" => self.dl32s.strip_count(),
            _ => self.sim.strip_count(),
        }
    }

    /// Ordered strip families of the named driver (`caps.topology`).
    fn driver_topology(&self, driver: &str) -> Vec<openmix_core::StripFamily> {
        match driver {
            "xair" => self.xair.topology(),
            "dl32s" => self.dl32s.topology(),
            _ => self.sim.topology(),
        }
    }

    /// Declared meter taps of the named driver (`caps.meter_taps`).
    fn driver_meter_taps(&self, driver: &str) -> Vec<String> {
        let taps: &[&str] = match driver {
            "xair" => self.xair.meter_taps(),
            "dl32s" => self.dl32s.meter_taps(),
            _ => self.sim.meter_taps(),
        };
        taps.iter().map(|s| s.to_string()).collect()
    }

    /// Point the link at a connected desk (R2): canonical device name when
    /// the ip matches a discovered device, else the desk identity / driver.
    async fn set_link_connected(&self, device_id: Option<&str>, ip: &str) {
        let entry = self
            .devices
            .read()
            .unwrap()
            .iter()
            .find(|d| {
                device_id.is_some_and(|id| d.id == id) || d.ip == ip
            })
            .cloned();
        let (name, device) = match entry {
            Some(d) => (d.name.clone(), Some(d.id.clone())),
            None => {
                let driver = self.active_driver();
                let name = match driver.as_str() {
                    "xair" => self.xair.query_info().await.map(|i| i.model).unwrap_or_else(|_| driver.clone()),
                    "dl32s" => self.dl32s.query_info().await.map(|i| i.model).unwrap_or_else(|_| driver.clone()),
                    _ => self.sim.query_info().await.map(|i| i.model).unwrap_or_else(|_| driver.clone()),
                };
                (name, None)
            }
        };
        let mut link = self.link.write().unwrap();
        link.state = "connected".to_string();
        link.name = name;
        link.ip = ip.to_string();
        link.fs = DESK_SAMPLE_RATE;
        link.device_id = device.or_else(|| device_id.map(str::to_string));
    }

    /// Pre-write value for the offline change list (`None` unless offline
    /// editing is active — avoids an extra desk query per write online).
    async fn offline_from(&self, strip: StripId, id: &str) -> Option<Value> {
        if !self.offline.read().unwrap().active {
            return None;
        }
        let driver = self.active_driver();
        self.read_param(&driver, strip, id)
            .await
            .ok()
            .map(|pv| param_value_json(&pv))
    }
    /// catalogue filtered by the driver's capabilities, `unsupported` the
    /// rest in catalogue order; `topology` + `meter_taps` are additive
    /// (old fields byte-identical).
    fn caps_message(&self) -> Event {
        let driver = self.active_driver();
        let supported = self.driver_capabilities(&driver);
        let dropped = self.param_dropped.read().unwrap();
        let live: Vec<&&str> = supported
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .collect();
        let params: Vec<ParamDef> = live
            .iter()
            .filter_map(|id| params::find(id).map(param_def_frame))
            .collect();
        let mut unsupported: Vec<String> = params::CATALOGUE
            .iter()
            .map(|d| d.id.to_string())
            .filter(|id| {
                !supported.contains(&id.as_str())
                    || dropped.contains(&(driver.clone(), id.clone()))
            })
            .collect();
        unsupported.sort();
        let topology = self
            .driver_topology(&driver)
            .iter()
            .map(|f| crate::protocol::TopologyFamily {
                key: f.key.to_string(),
                kind: f.kind.as_str().to_string(),
                label: f.label.to_string(),
                first: f.first,
                count: f.count,
                name_prefix: f.name_prefix.to_string(),
                color_default: f.color_default,
                params_scope: vec![f.kind.as_str().to_string()],
            })
            .collect();
        Event::Caps {
            driver: driver.clone(),
            params,
            unsupported,
            topology,
            meter_taps: self.driver_meter_taps(&driver),
            fx: self.fx_caps(&driver),
            routing: self.routing_caps(&driver),
        }
    }

    // ================= R2: devices / link / info =================

    /// This client's meter-tap preference (set by `hello {"taps":true}`).
    pub fn client_taps(&self, id: &str) -> bool {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .is_some_and(|e| e.taps)
    }

    /// True while any connected client negotiated meter taps.
    fn any_taps_client(&self) -> bool {
        self.clients.read().unwrap().values().any(|e| e.taps)
    }

    /// Max concurrent poller queries for one tick (config override or
    /// per-driver default; clamped to 1..=16).
    pub fn max_inflight(&self, driver: &str) -> usize {
        self.poll_inflight
            .unwrap_or_else(|| default_inflight(driver))
            .clamp(1, 16)
    }

    /// Adaptive effective inflight: halves while the batch-RTT EMA exceeds
    /// the poll tick (the desk is saturated — back off instead of piling
    /// on), otherwise the configured max.
    pub fn effective_inflight(&self, driver: &str) -> usize {
        let max = self.max_inflight(driver);
        let ema_us = self.rtt_ema_us.load(Ordering::Relaxed);
        let tick_us = self.poll_interval.as_micros() as u64;
        if tick_us > 0 && ema_us > tick_us {
            (max / 2).max(1)
        } else {
            max
        }
    }

    /// Current `devices` event (link-aware states: the linked console
    /// reads `connected`, other consoles `available`, I/O `in_use`).
    fn devices_message(&self) -> Event {
        let link = self.link.read().unwrap();
        let list = self
            .devices
            .read()
            .unwrap()
            .iter()
            .map(|d| {
                let state = if d.kind == "io" {
                    "in_use".to_string()
                } else if link.state == "connected" && link.device_id.as_deref() == Some(d.id.as_str()) {
                    "connected".to_string()
                } else {
                    "available".to_string()
                };
                DeviceDesc {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    model: d.model.clone(),
                    ip: d.ip.clone(),
                    kind: d.kind.clone(),
                    state,
                    fw: d.fw.clone(),
                }
            })
            .collect();
        Event::Devices {
            list,
            scanned_at: self.scanned_at.load(Ordering::Relaxed),
        }
    }

    /// Cached `link` event (no measurement; the snapshot + change paths).
    fn link_message_cached(&self) -> Event {
        let link = self.link.read().unwrap();
        Event::Link {
            state: link.state.clone(),
            name: link.name.clone(),
            ip: link.ip.clone(),
            fs: link.fs,
            latency_ms: link.latency_ms,
        }
    }

    /// Fresh `link` event with a new round-trip measurement stored.
    async fn link_message(&self) -> Event {
        let latency_ms = self.measure_latency().await;
        {
            let mut link = self.link.write().unwrap();
            link.latency_ms = latency_ms;
        }
        self.link_message_cached()
    }

    /// One round-trip latency sample in ms: OSC query timing on hardware
    /// (a lightweight main-fader query), small deterministic jitter on
    /// sim. Failures keep the last sample (never block the ticker).
    async fn measure_latency(&self) -> f32 {
        let last = self.link.read().unwrap().latency_ms;
        match self.active_driver().as_str() {
            "xair" | "dl32s" => {
                let t0 = Instant::now();
                let target = if self.active_driver().as_str() == "xair" {
                    self.xair.get_main_fader().await
                } else {
                    self.dl32s.get_main_fader().await
                };
                match target {
                    Ok(_) => t0.elapsed().as_secs_f32() * 1000.0,
                    Err(_) => last,
                }
            }
            _ => 1.0 + ((self.now_ms() / 100) % 5) as f32 * 0.1,
        }
    }

    /// Desk identity block for `info` (connect + on request). Errors when
    /// the driver cannot answer (hardware drivers while unconnected).
    async fn info_message(&self) -> anyhow::Result<Event> {
        let driver = self.active_driver();
        let info = match driver.as_str() {
            "xair" => self.xair.query_info().await?,
            "dl32s" => self.dl32s.query_info().await?,
            _ => self.sim.query_info().await?,
        };
        Ok(Event::Info {
            model: info.model,
            firmware: info.firmware,
            sample_rate: DESK_SAMPLE_RATE,
            console_name: info.name,
            driver,
        })
    }

    // ================= R2: numbered scenes =================

    /// Current `scenes` event (slots sorted by number).
    fn scenes_message(&self) -> Event {
        let lib = self.scenes_lib.read().unwrap();
        let current = *self.scenes_current.read().unwrap();
        let mut scenes: Vec<crate::protocol::SceneEntry> = lib
            .values()
            .map(|s| crate::protocol::SceneEntry {
                number: s.number,
                name: s.name.clone(),
                current: current == Some(s.number),
                saved_at: s.saved_at,
            })
            .collect();
        scenes.sort_by_key(|e| e.number);
        Event::Scenes { scenes }
    }

    /// Persist the scene library (best-effort; in-memory when no data dir).
    fn save_scenes(&self) {
        persist_scenes(self.data_dir.as_deref(), &self.scenes_lib.read().unwrap());
    }

    /// Capture the FULL param plane: every catalogue value of every strip
    /// (topology × scoped capabilities) + main + FX + routing.
    async fn capture_full(&self, number: u16, name: &str) -> anyhow::Result<StoredScene> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        let caps = self.driver_capabilities(&driver);
        let mut strips = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                let strip = StripId(s as u16);
                let fader = self.read_param(&driver, strip, "fader").await.ok();
                let mute = self.read_param(&driver, strip, "mute").await.ok();
                let name_v = self.read_param(&driver, strip, "name").await.ok();
                let mut params = HashMap::new();
                for id in &caps {
                    if *id == "fader" || *id == "mute" || *id == "name" {
                        continue;
                    }
                    let scoped = params::find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    if let Ok(pv) = self.read_param(&driver, strip, id).await {
                        // Skip catalogue defaults? No — full plane means
                        // every value, so restores are exact.
                        params.insert(id.to_string(), param_value_json(&pv));
                    }
                }
                strips.push(StoredStrip {
                    strip: s as u16,
                    name: match name_v {
                        Some(ParamValue::Text(n)) => n,
                        _ => format!("{}{}", fam.name_prefix, s - fam.first + 1),
                    },
                    fader: match fader {
                        Some(ParamValue::Float(v)) => v,
                        _ => 0.0,
                    },
                    mute: matches!(mute, Some(ParamValue::Bool(true))),
                    params,
                });
            }
        }
        let fx = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .map(|s| StoredFx {
                slot: s.slot,
                fx_type: s.fx_type.clone(),
                on: s.on,
                bypass: s.bypass,
                params: s.params.clone(),
            })
            .collect();
        let routing = self.routing_snapshot().await;
        Ok(StoredScene {
            number,
            name: name.to_string(),
            saved_at: now_unix(),
            main_fader: self.active_main_fader().await,
            strips,
            fx,
            routing,
        })
    }

    /// Recall a full-plane scene: faders ride the shared fade
    /// ([`openmix_core::show::fade_strips`], same as named shows), every
    /// other param goes through `set_param` in [`safe_order_key`] order, then
    /// FX + routing + main. Locks are bypassed (show control) and reported
    /// via the returned `overrode_locks` (existing error semantics for
    /// direct writes are untouched).
    async fn recall_full(
        &self,
        scene: &StoredScene,
        fade_ms: u64,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Vec<u16>> {
        // Faders ride the single shared fade (legacy order: pre-unmute
        // targets, ramp, snap, target mutes); params/FX/routing/main follow.
        let targets: Vec<(StripId, f32, bool)> = scene
            .strips
            .iter()
            .map(|s| (StripId(s.strip), s.fader, s.mute))
            .collect();
        openmix_core::show::fade_strips(self, &targets, fade_ms).await?;
        for s in &scene.strips {
            if s.mute {
                self.drv_set_mute(StripId(s.strip), true).await?;
            }
            out.push(Event::Param {
                ch: s.strip,
                field: "fader".to_string(),
                value: json!(s.fader),
                by: Some(writer.to_string()),
            });
            out.push(Event::Param {
                ch: s.strip,
                field: "mute".to_string(),
                value: json!(s.mute),
                by: Some(writer.to_string()),
            });
        }
        // Remaining params in safe order (names ride along: full plane).
        for s in &scene.strips {
            let mut ids: Vec<&String> = s.params.keys().collect();
            ids.sort_by_key(|id| safe_order_key(id));
            for id in ids {
                let value = s.params.get(id).expect("key just listed");
                let pv = json_to_param_value(value)?;
                // Best-effort per param on hardware (a desk that went
                // silent mid-recall must not abort the rest); strict on sim.
                let r = self.drv_set_param(StripId(s.strip), id, pv).await;
                if self.active_driver().as_str() == "sim" {
                    r?;
                }
                out.push(Event::Param {
                    ch: s.strip,
                    field: (*id).clone(),
                    value: value.clone(),
                    by: Some(writer.to_string()),
                });
            }
            if s.name.is_empty() {
                continue;
            }
            let r = self
                .drv_set_param(StripId(s.strip), "name", ParamValue::Text(s.name.clone()))
                .await;
            if self.active_driver().as_str() == "sim" {
                r?;
            }
        }
        // FX + routing snapshots (sim-strict, best-effort elsewhere).
        self.recall_fx(&scene.fx).await?;
        self.recall_routing(&scene.routing).await?;
        if let Some(m) = scene.main_fader {
            self.drv_set_main(m).await?;
        }
        self.refresh_fx_cache().await;
        let mut overrode: Vec<u16> = Vec::new();
        for s in &scene.strips {
            if let Some(holder) = self.lock_holder(s.strip) {
                if holder != writer && !overrode.contains(&s.strip) {
                    overrode.push(s.strip);
                }
            }
        }
        overrode.sort_unstable();
        Ok(overrode)
    }

    // ================= R2: FX =================

    /// FX slot count of the named driver (sim 8 design slots, xair 4,
    /// dl32s 0).
    fn fx_slots_of(&self, driver: &str) -> usize {
        match driver {
            "sim" => openmix_sim::SIM_FX_SLOTS,
            "xair" => 4,
            _ => 0,
        }
    }

    /// `caps.fx` for the named driver: slot count + type tables (sim: the
    /// six design tables from `core::fx`; xair: 4 slots, desk-native
    /// 61-type table NOT projected yet — see README matrix; dl32s: none).
    fn fx_caps(&self, driver: &str) -> crate::protocol::FxCaps {
        let types = if driver == "sim" {
            openmix_core::fx::FX_TYPES
                .iter()
                .map(|t| crate::protocol::FxTypeCaps {
                    id: t.id.to_string(),
                    label: t.label.to_string(),
                    params: t
                        .params
                        .iter()
                        .map(|p| crate::protocol::FxParamCaps {
                            id: p.id.to_string(),
                            label: p.label.to_string(),
                            unit: p.unit.to_string(),
                            min: p.min,
                            max: p.max,
                            default: p.default,
                            scale: p.scale.to_string(),
                        })
                        .collect(),
                })
                .collect()
        } else {
            Vec::new()
        };
        crate::protocol::FxCaps {
            slots: self.fx_slots_of(driver),
            types,
        }
    }

    /// Current `fx` event from the engine-side cache + return levels.
    async fn fx_message(&self) -> Event {
        let levels = self.meter_levels().await;
        let slots = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .map(|s| {
                let level = if self.active_driver().as_str() == "sim" {
                    let idx = 56 + ((s.slot as usize - 1) % 4);
                    levels.get(idx).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                FxSlotState {
                    slot: s.slot,
                    fx_type: s.fx_type.clone(),
                    on: s.on,
                    bypass: s.bypass,
                    ret_label: format!("Ret {}/{}", 2 * s.slot - 1, 2 * s.slot),
                    level,
                    params: s.params.clone(),
                }
            })
            .collect();
        Event::Fx { slots }
    }

    /// Rebuild the FX cache from the active driver (sim: full rack;
    /// elsewhere: honest placeholder rows — type unknown, no params).
    async fn refresh_fx_cache(&self) {
        let rows = self.read_fx_rows().await;
        *self.fx_cache.write().unwrap() = rows;
    }

    /// Synchronous FX cache rebuild for construction (sim only; the
    /// default driver is always sim here).
    fn refresh_fx_cache_sync(&self) {
        let rows: Vec<FxSlotRt> = self
            .sim
            .fx_slots()
            .iter()
            .enumerate()
            .map(|(i, s)| FxSlotRt {
                slot: (i + 1) as u8,
                fx_type: s.fx_type.clone(),
                on: s.on,
                bypass: s.bypass,
                params: param_vec_to_map(&s.fx_type, &s.params),
            })
            .collect();
        *self.fx_cache.write().unwrap() = rows;
    }

    /// Read FX rows from the active driver (sim-backed; xair/dl32s yield
    /// placeholder rows — only numbered `fx_param parNN` writes are
    /// implemented there).
    async fn read_fx_rows(&self) -> Vec<FxSlotRt> {
        match self.active_driver().as_str() {
            "sim" => {
                self.sim
                    .fx_slots()
                    .iter()
                    .enumerate()
                    .map(|(i, s)| FxSlotRt {
                        slot: (i + 1) as u8,
                        fx_type: s.fx_type.clone(),
                        on: s.on,
                        bypass: s.bypass,
                        params: param_vec_to_map(&s.fx_type, &s.params),
                    })
                    .collect()
            }
            "xair" => (1..=4u8)
                .map(|slot| {
                    let prev = self.fx_cache.read().unwrap().iter().find(|r| r.slot == slot).cloned();
                    prev.unwrap_or(FxSlotRt {
                        slot,
                        fx_type: "—".to_string(),
                        on: true,
                        bypass: false,
                        params: Default::default(),
                    })
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// One FX poll read (round-robin over the sim rack; silent unless a
    /// value changed). xair/dl32s racks are not polled (no verified state
    /// path — writes are fire-and-forget there).
    async fn fx_refresh_one(&self) {
        if self.active_driver().as_str() != "sim" {
            return;
        }
        let slots = self.sim.fx_slots();
        if slots.is_empty() {
            return;
        }
        let i = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize % slots.len();
        let sim_slot = &slots[i];
        let changed = {
            let mut cache = self.fx_cache.write().unwrap();
            let Some(row) = cache.iter_mut().find(|r| r.slot == (i + 1) as u8) else {
                return;
            };
            let fresh = param_vec_to_map(&sim_slot.fx_type, &sim_slot.params);
            let diff = row.fx_type != sim_slot.fx_type
                || row.on != sim_slot.on
                || row.bypass != sim_slot.bypass
                || row.params != fresh;
            if diff {
                row.fx_type = sim_slot.fx_type.clone();
                row.on = sim_slot.on;
                row.bypass = sim_slot.bypass;
                row.params = fresh;
            }
            diff
        };
        if changed {
            self.broadcast(&self.fx_message().await);
        }
    }

    /// Recall FX snapshot rows (sim-strict, best-effort elsewhere).
    async fn recall_fx(&self, fx: &[StoredFx]) -> anyhow::Result<()> {
        let strict = self.active_driver().as_str() == "sim";
        for row in fx {
            let r = self.apply_fx_row(row).await;
            if strict {
                r?;
            }
        }
        Ok(())
    }

    /// Apply one FX snapshot row to the active driver.
    async fn apply_fx_row(&self, row: &StoredFx) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "sim" => {
                // Type first (resets params to defaults), then values.
                let cur = self.sim.get_fx_type(row.slot).unwrap_or_default();
                if cur != row.fx_type {
                    self.sim.set_fx_type(row.slot, &row.fx_type)?;
                }
                self.sim.set_fx_on(row.slot, row.on)?;
                self.sim.set_fx_bypass(row.slot, row.bypass)?;
                let mut ids: Vec<&String> = row.params.keys().collect();
                ids.sort();
                for id in ids {
                    self.sim.set_fx_param(row.slot, id, row.params[id])?;
                }
                Ok(())
            }
            "xair" => {
                // Numbered pars only (design ids have no verified mapping).
                let mut ids: Vec<&String> = row.params.keys().collect();
                ids.sort();
                for id in ids {
                    if let Some(idx) = parse_par_id(id) {
                        self.xair.set_fx_param(row.slot, idx, row.params[id]).await?;
                    }
                }
                Ok(())
            }
            d => anyhow::bail!("{d}: fx recall not supported"),
        }
    }

    // ================= R2: routing =================

    /// `caps.routing` for the named driver (sim: Lcl/Net/USB × 32 fully
    /// supported; xair: unsupported — `insrc` is live-silent on FW 1.20;
    /// dl32s: unsupported).
    fn routing_caps(&self, driver: &str) -> crate::protocol::RoutingCaps {
        if driver == "sim" {
            crate::protocol::RoutingCaps {
                groups: openmix_sim::SIM_PATCH_GROUPS
                    .iter()
                    .map(|(key, label, count)| crate::protocol::RoutingGroupCaps {
                        key: key.to_string(),
                        label: label.to_string(),
                        count: *count,
                    })
                    .collect(),
                per_strip: true,
                supported: true,
            }
        } else {
            crate::protocol::RoutingCaps {
                groups: Vec::new(),
                per_strip: false,
                supported: false,
            }
        }
    }

    /// Current `routing` event: one row per patched input strip (sim reads
    /// the `source` param plane; elsewhere the list is empty).
    async fn routing_message(&self) -> Event {
        Event::Routing {
            patches: self.routing_snapshot().await.into_iter().map(|(strip, group, index)| {
                PatchEntry {
                    strip,
                    label: format!("{group} {index:02}"),
                    group,
                    index,
                }
            }).collect(),
        }
    }

    /// (strip, group, 1-based index) rows for patched input strips.
    async fn routing_snapshot(&self) -> Vec<(u16, String, u8)> {
        if self.active_driver().as_str() != "sim" {
            return Vec::new();
        }
        let mut out = Vec::new();
        for fam in self.driver_topology("sim") {
            if fam.kind != openmix_core::StripKind::In {
                continue;
            }
            for s in fam.first..fam.first + fam.count {
                let Ok(pv) = self.sim.get_param(StripId(s as u16), "source").await else {
                    continue;
                };
                if let ParamValue::Text(label) = pv {
                    if let Some((group, index)) = parse_source(&label) {
                        out.push((s as u16, group, index));
                    }
                }
            }
        }
        out
    }

    /// Recall routing snapshot rows (sim only; skipped elsewhere).
    async fn recall_routing(&self, routing: &[(u16, String, u8)]) -> anyhow::Result<()> {
        if self.active_driver().as_str() != "sim" {
            return Ok(());
        }
        for (strip, group, index) in routing {
            self.sim.set_patch(StripId(*strip), group, *index)?;
        }
        Ok(())
    }

    // ================= R2: meter taps =================

    /// Per-tap level vectors for the active driver, in
    /// `caps.meter_taps` order filtered to known taps. sim synthesizes
    /// all four; xair serves `post` from the meter blob cache and GR
    /// from the DYN cache (`in` unavailable — no `/meters/0` subscribe);
    /// dl32s serves `post` only. Every entry is best-effort: a tap with
    /// no source reads back silence, never an error.
    async fn tap_frames(&self) -> Vec<(String, Vec<f32>)> {
        match self.active_driver().as_str() {
            "sim" => {
                let taps = self.sim.poll_taps();
                vec![
                    ("in".to_string(), taps.iter().map(|t| t.input).collect()),
                    ("post".to_string(), taps.iter().map(|t| t.post).collect()),
                    ("gr_gate".to_string(), taps.iter().map(|t| t.gr_gate_db).collect()),
                    ("gr_comp".to_string(), taps.iter().map(|t| t.gr_comp_db).collect()),
                ]
            }
            "xair" => {
                let post = self.xair.poll_meters().levels;
                let n = post.len().max(1);
                let mut gr_gate = vec![0.0f32; n];
                let mut gr_comp = vec![0.0f32; n];
                for (i, (g, c)) in gr_gate.iter_mut().zip(gr_comp.iter_mut()).enumerate() {
                    let (gate, comp) = self.gr_db(i).await;
                    *g = gate;
                    *c = comp;
                }
                vec![
                    ("post".to_string(), post),
                    ("gr_gate".to_string(), gr_gate),
                    ("gr_comp".to_string(), gr_comp),
                ]
            }
            _ => {
                let post = self.dl32s.poll_meters().levels;
                vec![("post".to_string(), post)]
            }
        }
    }

    // ================= R2: offline editing =================

    /// Record a param-plane write into the offline change list (no-op
    /// unless offline editing is active). Same (strip, id) keeps its
    /// original `from` and takes the newest `to`.
    async fn offline_note(&self, strip: u16, id: &str, from: Value, to: Value) {
        let mut off = self.offline.write().unwrap();
        if !off.active {
            return;
        }
        match off.changes.iter_mut().find(|c| c.strip == strip && c.id == id) {
            Some(e) => e.to = to,
            None => off.changes.push(OfflineChange {
                strip,
                id: id.to_string(),
                from,
                to,
            }),
        }
    }

    /// Current `offline_diff` event (pending change list).
    fn offline_diff_message(&self) -> Event {
        let off = self.offline.read().unwrap();
        Event::OfflineDiff {
            count: off.changes.len(),
            changes: off.changes.clone(),
        }
    }

    /// Push the pending offline change list to the active driver, in
    /// record order. Namespaced ids: `fx/<slot-param>` for FX writes,
    /// `patch` for routing writes, `main_fader` (strip 0xFFFF) for main.
    /// Sim-strict, best-effort elsewhere; returns the applied Param echoes.
    async fn offline_push_apply(&self, writer: &str, out: &mut Vec<Event>) -> anyhow::Result<()> {
        let changes = self.offline.read().unwrap().changes.clone();
        let strict = self.active_driver().as_str() == "sim";
        let mut failures = Vec::new();
        for c in &changes {
            let r = self.offline_apply_one(c).await;
            if let Err(e) = r {
                failures.push(format!("strip {} {}: {e:#}", c.strip, c.id));
                continue;
            }
            out.push(Event::Param {
                ch: c.strip,
                field: c.id.clone(),
                value: c.to.clone(),
                by: Some(writer.to_string()),
            });
        }
        {
            let mut off = self.offline.write().unwrap();
            off.changes.clear();
            off.pending_push = false;
        }
        self.refresh_fx_cache().await;
        if failures.is_empty() {
            Ok(())
        } else if strict {
            anyhow::bail!("offline push had {} failure(s): {}", failures.len(), failures.join("; "))
        } else {
            Ok(())
        }
    }

    /// Apply one recorded offline change to the active driver.
    async fn offline_apply_one(&self, c: &OfflineChange) -> anyhow::Result<()> {
        if c.id == "main_fader" {
            let Value::Number(n) = &c.to else {
                anyhow::bail!("main_fader push value must be a number")
            };
            let v = n.as_f64().unwrap_or(0.0) as f32;
            return self.drv_set_main(v).await;
        }
        if c.id == "patch" {
            if self.active_driver().as_str() != "sim" {
                anyhow::bail!("patch push not supported here");
            }
            let Value::String(label) = &c.to else {
                anyhow::bail!("patch push value must be a label")
            };
            if label == "—" {
                return self.sim.clear_patch(StripId(c.strip)).map_err(anyhow::Error::from);
            }
            let Some((group, index)) = parse_source(label) else {
                anyhow::bail!("unparseable patch label {label:?}")
            };
            self.sim.set_patch(StripId(c.strip), &group, index)?;
            return Ok(());
        }
        if let Some(param) = c.id.strip_prefix("fx/") {
            match self.active_driver().as_str() {
                "sim" => {
                    match param {
                        "type" => {
                            let Value::String(t) = &c.to else {
                                anyhow::bail!("fx type push value must be a string")
                            };
                            self.sim.set_fx_type(c.strip as u8, t)?;
                            return Ok(());
                        }
                        "on" => {
                            let Value::Bool(b) = &c.to else {
                                anyhow::bail!("fx on push value must be a bool")
                            };
                            self.sim.set_fx_on(c.strip as u8, *b)?;
                            return Ok(());
                        }
                        "bypass" => {
                            let Value::Bool(b) = &c.to else {
                                anyhow::bail!("fx bypass push value must be a bool")
                            };
                            self.sim.set_fx_bypass(c.strip as u8, *b)?;
                            return Ok(());
                        }
                        _ => {}
                    }
                    let Value::Number(n) = &c.to else {
                        anyhow::bail!("fx push value must be a number")
                    };
                    self.sim.set_fx_param(c.strip as u8, param, n.as_f64().unwrap_or(0.0) as f32)?;
                    return Ok(());
                }
                "xair" => {
                    let Some(idx) = parse_par_id(param) else {
                        anyhow::bail!("fx push id {param:?} has no xair mapping")
                    };
                    let Value::Number(n) = &c.to else {
                        anyhow::bail!("fx push value must be a number")
                    };
                    self.xair.set_fx_param(c.strip as u8, idx, n.as_f64().unwrap_or(0.0) as f32).await?;
                    return Ok(());
                }
                d => anyhow::bail!("{d}: fx push not supported"),
            }
        }
        let pv = json_to_param_value(&c.to)?;
        self.drv_set_param(StripId(c.strip), &c.id, pv).await
    }

    // ================= R2: driver write helpers =================

    async fn drv_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "xair" => self.xair.set_fader(strip, value).await,
            "dl32s" => self.dl32s.set_fader(strip, value).await,
            _ => self.sim.set_fader(strip, value).await,
        }
    }

    async fn drv_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        match self.active_driver().as_str() {
            "xair" => self.xair.get_fader(strip).await,
            "dl32s" => self.dl32s.get_fader(strip).await,
            _ => self.sim.get_fader(strip).await,
        }
    }

    async fn drv_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "xair" => self.xair.set_mute(strip, muted).await,
            "dl32s" => self.dl32s.set_mute(strip, muted).await,
            _ => self.sim.set_mute(strip, muted).await,
        }
    }

    async fn drv_set_param(
        &self,
        strip: StripId,
        id: &str,
        pv: ParamValue,
    ) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "xair" => self.xair.set_param(strip, id, pv).await,
            "dl32s" => self.dl32s.set_param(strip, id, pv).await,
            _ => self.sim.set_param(strip, id, pv).await,
        }
    }

    async fn drv_set_main(&self, value: f32) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "xair" => self.xair.set_main_fader(value).await,
            "dl32s" => self.dl32s.set_main_fader(value).await,
            _ => self.sim.set_main_fader(value).await,
        }
    }

    // ================= R2: pipelined poller =================

    /// Visible-group lane: (strip, param) pairs serving the currently
    /// visible strips' EQ/dyn/sends first (R2 priority lane between focus
    /// and background). One full sweep per focus (then disarms — see
    /// `prio_armed`); `k` caps the take; the cursor round-robins within
    /// the sweep.
    fn pop_visible_batch(&self, driver: &str, k: usize) -> Vec<(u16, String)> {
        if k == 0 || !self.prio_armed.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let vis = self.visible.read().unwrap().clone();
        if vis.is_empty() {
            return Vec::new();
        }
        let list = self.visible_poll_list(driver, &vis);
        if list.is_empty() {
            return Vec::new();
        }
        let dropped = self.param_dropped.read().unwrap();
        let mut out = Vec::new();
        let mut skips = 0usize;
        while out.len() < k && skips <= list.len() {
            let i = self.prio_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
            // Sweep wrap (index 0 after serving ≥1 entry): the one-shot
            // boost is done — disarm and yield to the background.
            let taken = self.prio_taken.fetch_add(1, Ordering::Relaxed);
            if i == 0 && taken > 0 {
                self.prio_armed.store(false, Ordering::Relaxed);
                break;
            }
            let (ch, id) = &list[i];
            if dropped.contains(&(driver.to_string(), id.to_string())) {
                skips += 1;
                continue;
            }
            skips = 0;
            out.push((*ch, id.to_string()));
        }
        out
    }

    /// Cached visible-group poll list: visible strips × driver caps
    /// filtered to EQ/dyn/sends ids + scopes. Rebuilt when the visible
    /// set or driver changes.
    fn visible_poll_list(&self, driver: &str, vis: &[u16]) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_visible.read().unwrap();
            if let Some((d, v, list)) = cache.as_ref() {
                if d == driver && v == vis {
                    return list.clone();
                }
            }
        }
        let topo = self.driver_topology(driver);
        let caps = self.driver_capabilities(driver);
        let kind_of = |ch: u16| {
            topo.iter()
                .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                .map(|f| f.kind)
        };
        let mut out = Vec::new();
        for &ch in vis {
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in &caps {
                if !is_priority_id(id) {
                    continue;
                }
                let scoped = params::find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true);
                if !scoped {
                    continue;
                }
                out.push((ch, *id));
            }
        }
        *self.poll_visible.write().unwrap() = Some((driver.to_string(), vis.to_vec(), out.clone()));
        out
    }

    /// One pipelined poller tick (R2): up to `effective_inflight` driver
    /// queries in flight, strict lanes (focus > visible-group >
    /// background), results applied in batch order. At default inflight 1
    /// this is exactly [`Engine::poll_once`]'s budget with the visible
    /// lane interleaved; [`Engine::poll_once`] itself is untouched (exact
    /// legacy single-step for tests).
    ///
    /// Fairness: the visible cursor round-robins without end, so a
    /// non-empty visible set would starve the background sweep forever.
    /// Even lane ticks therefore reserve `max(1, n/4)` batch slots for
    /// the background (at n=1 whole ticks alternate); the visible lane
    /// takes everything else, first in batch order.
    ///
    /// Public so integration tests and diagnostics can drive (and time)
    /// the poller deterministically; the background task calls this every
    /// `poll_interval`.
    pub async fn poll_tick(self: &Arc<Self>) {
        if Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
            self.poll_skips.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let driver = self.active_driver();
        let n = self.effective_inflight(&driver);
        let mut batch: Vec<(u16, String)> = Vec::with_capacity(n);
        while batch.len() < n {
            match self.pop_focus(&driver) {
                Some(e) => batch.push(e),
                None => break,
            }
        }
        if batch.len() < n {
            // Visible bias with guaranteed background progress: on even
            // lane ticks the background reserves max(1, n/4) slots, on odd
            // ticks the visible lane takes everything left. At n=1 this
            // alternates whole ticks (visible, background, …); at n=4 the
            // background keeps ~1 slot in 8. Without the reservation a
            // non-empty visible set would starve the background sweep
            // forever (the visible cursor round-robins without end).
            let remaining = n - batch.len();
            let t = self.lane_tick.fetch_add(1, Ordering::Relaxed);
            let bg_share = if t % 2 == 0 { (n / 4).max(1).min(remaining) } else { 0 };
            batch.extend(self.pop_visible_batch(&driver, remaining - bg_share));
        }
        if batch.len() < n {
            let list = self.static_poll_list(&driver);
            if !list.is_empty() {
                let dropped = self.param_dropped.read().unwrap();
                let mut skips = 0usize;
                while batch.len() < n && skips <= list.len() {
                    let i = self.poll_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
                    let (ch, id) = &list[i];
                    if dropped.contains(&(driver.clone(), id.to_string())) {
                        skips += 1;
                        continue;
                    }
                    skips = 0;
                    batch.push((*ch, id.to_string()));
                }
            }
        }
        if batch.is_empty() {
            return;
        }
        // N queries in flight (pipelining); results re-ordered into batch
        // order for cache/diff application.
        let t0 = Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for (i, (ch, param)) in batch.iter().enumerate() {
            let engine = Arc::clone(self);
            let driver = driver.clone();
            let ch = *ch;
            let param = param.clone();
            set.spawn(async move {
                let r = engine.read_param(&driver, StripId(ch), &param).await;
                (i, ch, param, r)
            });
        }
        let mut ordered: Vec<Option<(u16, String, anyhow::Result<ParamValue>)>> = {
            let mut v = Vec::with_capacity(batch.len());
            v.resize_with(batch.len(), || None);
            v
        };
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((i, ch, param, r)) => ordered[i] = Some((ch, param, r)),
                Err(_) => {}
            }
        }
        for slot in ordered.into_iter().flatten() {
            let (ch, param, r) = slot;
            match r {
                Ok(pv) => {
                    let v = param_value_json(&pv);
                    let changed = {
                        let mut cache = self.param_cache.write().unwrap();
                        cache_store(&mut cache, ch, &param, v.clone())
                    };
                    if changed {
                        self.broadcast(&Event::Params {
                            ch,
                            values: [(param, v)].into_iter().collect(),
                        });
                    }
                }
                Err(e) => {
                    self.note_poll_error(&driver, &param, &e).await;
                }
            }
        }
        // Adaptive pipelining: EMA of the batch wall time.
        let dt_us = t0.elapsed().as_micros() as u64;
        let prev = self.rtt_ema_us.load(Ordering::Relaxed);
        let ema = if prev == 0 { dt_us } else { (prev * 3 + dt_us) / 4 };
        self.rtt_ema_us.store(ema, Ordering::Relaxed);
        if self.fx_cursor.fetch_add(1, Ordering::Relaxed) % FX_POLL_EVERY == 0 {
            self.fx_refresh_one().await;
        }
    }

    /// Shared post-switch flow for `connect` / `connect_device` / `offline`:
    /// invalidate caches, rebuild the FX mirror, and queue the full
    /// state+caps+link+info+fx+routing broadcast set. When an offline
    /// change list is pending, the `offline_diff` frame rides along and
    /// the push gate opens (nothing is pushed without `offline_push`).
    /// `entering_offline` keeps the just-armed offline session intact
    /// (otherwise a fresh connect resolves any pending diff).
    async fn post_switch(
        &self,
        writer: &str,
        out: &mut Vec<Event>,
        entering_offline: bool,
    ) -> anyhow::Result<()> {
        self.param_cache.write().unwrap().clear();
        self.refresh_fx_cache().await;
        self.note_write(writer);
        self.touch_holder_locks(writer);
        out.push(self.state_message(Some(writer)).await);
        out.push(self.caps_message());
        out.push(self.link_message().await);
        if let Ok(info) = self.info_message().await {
            out.push(info);
        }
        out.push(self.fx_message().await);
        out.push(self.routing_message().await);
        if entering_offline {
            return Ok(());
        }
        // A fresh connect ends any offline session: pending changes raise
        // offline_diff and wait for offline_push (or offline_discard).
        let pending = {
            let mut off = self.offline.write().unwrap();
            off.active = false;
            if off.changes.is_empty() {
                off.pending_push = false;
                false
            } else {
                off.pending_push = true;
                true
            }
        };
        if pending {
            out.push(self.offline_diff_message());
        }
        Ok(())
    }


    /// Full param-cache snapshot: one `params` event per channel that has
    /// any cached value (unicast to newly attached clients).
    fn params_snapshot(&self) -> Vec<Event> {
        let cache = self.param_cache.read().unwrap();
        let mut chs: Vec<u16> = cache.keys().copied().collect();
        chs.sort_unstable();
        chs.into_iter()
            .filter_map(|ch| {
                let values = cache.get(&ch)?;
                if values.is_empty() {
                    return None;
                }
                Some(Event::Params {
                    ch,
                    values: values.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                })
            })
            .collect()
    }

    /// Round-robin poll list for one driver: every strip of every topology
    /// family crossed with the driver's supported catalogue ids, filtered
    /// by catalogue scopes (DCA strips get fader/mute/name/color only) and
    /// minus dropped pairs. Strip-major: every param of strip N precedes
    /// strip N+1, so one strip's whole set arrives as a block. Built on the
    /// cached static list (zero per-tick allocation in the poller).
    /// Test/diagnostic helper (the poller uses the static list directly).
    #[allow(dead_code)]
    async fn poll_list(&self, driver: &str) -> Vec<(u16, String)> {
        let dropped = self.param_dropped.read().unwrap();
        self.static_poll_list(driver)
            .into_iter()
            .filter(|(_, id)| !dropped.contains(&(driver.to_string(), id.to_string())))
            .map(|(ch, id)| (ch, id.to_string()))
            .collect()
    }

    /// Cached static poll list: topology × capabilities × scopes, built
    /// once per driver. Drop-class filtering happens at drain time.
    fn static_poll_list(&self, driver: &str) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_static.read().unwrap();
            if let Some(v) = cache.get(driver) {
                return v.clone();
            }
        }
        let topo = self.driver_topology(driver);
        let caps = self.driver_capabilities(driver);
        let mut out = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                for id in &caps {
                    let scoped = params::find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    out.push((s as u16, *id));
                }
            }
        }
        self.poll_static.write().unwrap().insert(driver.to_string(), out.clone());
        out
    }

    /// Move `chs` to the front of the poll queue (the `focus` command,
    /// attributed to `writer`). Most-recent-first: pending entries for the
    /// focused strips are dropped (a new focus supersedes older ones — this
    /// also de-duplicates floods) and the fresh set is pushed at the FRONT
    /// in catalogue order, so it drains before everything queued before.
    /// The queue is capped (~4 sweeps, oldest dropped from the back). A
    /// detached client's pending focus is dropped with it (see
    /// [`Engine::remove_client`]).
    ///
    /// R2: the focused strips also become the visible set (capped at 16,
    /// most-recent-first) backing the poller's visible-group lane
    /// (EQ/dyn/sends of visible strips ahead of the background sweep).
    fn request_focus(&self, writer: &str, chs: &[u16]) {
        let driver = self.active_driver();
        let nstrips = self.driver_strips(&driver);
        let valid: Vec<u16> = chs.iter().copied().filter(|c| (*c as usize) < nstrips).collect();
        let dropped = self.param_dropped.read().unwrap();
        let ids: Vec<String> = self
            .driver_capabilities(&driver)
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .map(|s| s.to_string())
            .collect();
        drop(dropped);
        // Scope-filter per strip (mirrors poll_list): only ids the strip's
        // kind supports are queued.
        let topo = self.driver_topology(&driver);
        let kind_of = |ch: u16| {
            topo.iter()
                .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                .map(|f| f.kind)
        };
        let mut q = self.focus_queue.write().unwrap();
        q.retain(|(d, c, _, _)| d != &driver || !chs.contains(c));
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            let scoped: Vec<&String> = ids
                .iter()
                .filter(|id| {
                    params::find(id)
                        .map(|def| params::supports_kind(def, kind))
                        .unwrap_or(true)
                })
                .collect();
            for id in scoped.iter().rev() {
                q.push_front((driver.clone(), ch, (*id).clone(), writer.to_string()));
            }
        }
        let max = 4 * nstrips * ids.len().max(1);
        while q.len() > max {
            q.pop_back();
        }
        drop(q);
        // R2 visible set: most-recent focused strips first, capped at 16.
        // Re-arms the one-shot visible boost (one full sweep of the new
        // set's EQ/dyn/sends, then the background owns the wire again).
        if !valid.is_empty() {
            let fresh = {
                let vis = self.visible.read().unwrap().clone();
                let mut fresh: Vec<u16> = valid;
                fresh.extend(vis.iter().copied());
                fresh.dedup();
                fresh.truncate(16);
                fresh
            };
            *self.visible.write().unwrap() = fresh;
            self.prio_cursor.store(0, Ordering::Relaxed);
            self.prio_taken.store(0, Ordering::Relaxed);
            self.prio_armed.store(true, Ordering::Relaxed);
        }
    }

    /// Pop the next focus entry for `driver`, skipping stale entries and
    /// params dropped since they were queued.
    fn pop_focus(&self, driver: &str) -> Option<(u16, String)> {
        let dropped = self.param_dropped.read().unwrap();
        let mut q = self.focus_queue.write().unwrap();
        while let Some((d, ch, param, _)) = q.pop_front() {
            if d != driver {
                continue;
            }
            if dropped.contains(&(d, param.clone())) {
                continue;
            }
            return Some((ch, param));
        }
        None
    }

    /// Read one catalogue param from the named driver.
    async fn read_param(
        &self,
        driver: &str,
        ch: StripId,
        param: &str,
    ) -> anyhow::Result<ParamValue> {
        match driver {
            "xair" => self.xair.get_param(ch, param).await,
            "dl32s" => self.dl32s.get_param(ch, param).await,
            _ => self.sim.get_param(ch, param).await,
        }
    }

    /// One poller tick: yield to any in-flight write first (MR1 write
    /// priority), then drain one focus entry, else read the next (strip,
    /// param) in round-robin order; broadcast a diff frame on change, or
    /// drop the param on unsupported/unknown/timeout reads. Drop-class
    /// pairs are skipped in place (cursor already advanced) so a dropout
    /// costs one tick, not a rebuild.
    ///
    /// Legacy single-step: the background task runs [`Engine::poll_tick`]
    /// (R2 pipelining); this stays as the exact one-query-per-call path
    /// for tests and diagnostics.
    #[allow(dead_code)]
    async fn poll_once(&self) {
        if Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
            self.poll_skips.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let driver = self.active_driver();
        if let Some((ch, param)) = self.pop_focus(&driver) {
            self.poll_read(&driver, ch, &param).await;
            return;
        }
        let list = self.static_poll_list(&driver);
        if list.is_empty() {
            return;
        }
        let i = self.poll_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
        let (ch, param) = &list[i];
        if self
            .param_dropped
            .read()
            .unwrap()
            .contains(&(driver.clone(), param.to_string()))
        {
            return;
        }
        self.poll_read(&driver, *ch, param).await;
    }

    /// Read one (channel, param) and broadcast the diff frame on change.
    async fn poll_read(&self, driver: &str, ch: u16, param: &str) {
        match self.read_param(driver, StripId(ch), param).await {
            Ok(pv) => {
                let v = param_value_json(&pv);
                let changed = {
                    let mut cache = self.param_cache.write().unwrap();
                    cache_store(&mut cache, ch, param, v.clone())
                };
                if changed {
                    self.broadcast(&Event::Params {
                        ch,
                        values: [(param.to_string(), v)].into_iter().collect(),
                    });
                }
            }
            Err(e) => {
                self.note_poll_error(driver, param, &e).await;
            }
        }
    }

    /// Handle one poll-read error: drop-class errors remove the param from
    /// the poll set and rebroadcast `caps` once; everything else keeps
    /// polling silently. Returns true when the param was dropped.
    async fn note_poll_error(&self, driver: &str, param: &str, e: &anyhow::Error) -> bool {
        if !is_unsupported_error(e) {
            return false;
        }
        let fresh = {
            let mut dropped = self.param_dropped.write().unwrap();
            dropped.insert((driver.to_string(), param.to_string()))
        };
        if fresh {
            self.broadcast(&self.caps_message());
        }
        fresh
    }

    /// Re-read one param after a client write, caching the confirmed value.
    /// Returns the confirmed JSON value (`None` when the re-read fails, in
    /// which case the caller falls back to echoing the written value).
    async fn refresh_param(&self, ch: StripId, param: &str) -> Option<Value> {
        let driver = self.active_driver();
        let pv = self.read_param(&driver, ch, param).await.ok()?;
        let v = param_value_json(&pv);
        {
            let mut cache = self.param_cache.write().unwrap();
            cache_store(&mut cache, ch.0, param, v.clone());
        }
        Some(v)
    }

    /// Snapshot the active driver (used by `capture_scene`).
    async fn active_snapshot(&self) -> anyhow::Result<openmix_core::MixerSnapshot> {
        match self.active_driver().as_str() {
            "xair" => self.xair.snapshot().await,
            "dl32s" => self.dl32s.snapshot().await,
            _ => self.sim.snapshot().await,
        }
    }

    /// Main fader of the active driver, `None` when the driver has none.
    async fn active_main_fader(&self) -> Option<f32> {
        match self.active_driver().as_str() {
            "xair" => self.xair.get_main_fader().await.ok(),
            "dl32s" => self.dl32s.get_main_fader().await.ok(),
            _ => self.sim.get_main_fader().await.ok(),
        }
    }

    /// Recall `scene` on the active driver, ramping over `fade_ms`.
    async fn recall_on_active(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        match self.active_driver().as_str() {
            "xair" => scene.recall_to(&self.xair, fade_ms).await,
            "dl32s" => scene.recall_to(&self.dl32s, fade_ms).await,
            _ => scene.recall_to(&self.sim, fade_ms).await,
        }
    }

    /// Full `state` event for the active driver, optionally tagged with the
    /// mutation writer (`by` is `None` for snapshots).
    async fn state_message(&self, by: Option<&str>) -> Event {
        let name = self.active_driver();
        let strips = match name.as_str() {
            "xair" => snapshot_strips(&self.xair).await,
            "dl32s" => snapshot_strips(&self.dl32s).await,
            _ => snapshot_strips(&self.sim).await,
        };
        let main_fader = self.active_main_fader().await;
        Event::State {
            driver: name,
            strips,
            main_fader,
            by: by.map(str::to_string),
        }
    }

    /// Collect the writer-tagged `state` broadcast for `out` (flushed after
    /// the unicast reply, mirroring the relay socket order).
    async fn push_state_with_by(&self, by: &str, out: &mut Vec<Event>) {
        out.push(self.state_message(Some(by)).await);
    }

    /// Write-priority decision (MR1): the poller skips its tick while any
    /// command is inside [`apply`](Engine::apply) (`writes_in_flight > 0`).
    /// Pure function so the latency test can assert the rule directly.
    fn poller_yields(writes_in_flight: u64) -> bool {
        writes_in_flight > 0
    }

    /// Monotonic engine clock in ms since [`Engine::new`] (source of the
    /// `t_ms` field in binary meter frames). Wraps at ~49.7 days; clients
    /// must treat it as monotonic-per-connection, not wall time.
    pub fn now_ms(&self) -> u32 {
        self.boot.elapsed().as_millis() as u32
    }

    /// Poller ticks skipped while a write was in flight (diagnostic for
    /// the MR1 latency test: proves the yield path executes).
    pub fn poll_skips(&self) -> u64 {
        self.poll_skips.load(Ordering::Relaxed)
    }

    /// This client's binary-meter preference (set by [`Cmd::Hello`]).
    /// Adapters read it after every inbound command to pick the `Meters`/`Gr`
    /// encoding; unknown clients read `false` (JSON).
    pub fn client_binary_meters(&self, id: &str) -> bool {
        self.clients
            .read()
            .unwrap()
            .get(id)
            .is_some_and(|e| e.binary_meters)
    }

    /// Detach a client by id (adapter path: the inbound or sender task owns
    /// no [`ClientHandle`]). Idempotent, like [`ClientHandle::detach`].
    pub async fn detach_client(&self, id: &str) {
        self.remove_client(id);
    }

    /// Adapter entry: apply one raw JSON command line from `writer` without
    /// a [`ClientHandle`] (the serve inbound task owns none — the outbound
    /// task owns the handle for `events`). Same semantics as
    /// [`ClientHandle::send_json`]: parse failures and domain errors arrive
    /// as unicast `error` events, never closing the connection.
    pub async fn handle_json(&self, writer: &str, text: &str) {
        match crate::protocol::parse_cmd(text) {
            Ok(cmd) => self.apply_cmd(writer, cmd).await,
            Err(message) => {
                if !self.has_client(writer) {
                    return;
                }
                self.unicast(
                    writer,
                    Event::Error {
                        message,
                        locked_by: None,
                        ch: None,
                    },
                );
            }
        }
    }

    /// Shared command dispatch for [`ClientHandle::send`] and
    /// [`handle_json`](Engine::handle_json): run [`apply`](Engine::apply),
    /// unicast the direct reply (or error), then broadcast.
    async fn apply_cmd(&self, writer: &str, cmd: Cmd) {
        let mut out = Vec::new();
        match self.apply(cmd, writer, &mut out).await {
            Ok(Some(reply)) => self.unicast(writer, reply),
            Ok(None) => {}
            Err(e) => self.unicast(
                writer,
                Event::Error {
                    message: e.to_string(),
                    locked_by: None,
                    ch: None,
                },
            ),
        }
        for event in out {
            self.broadcast(&event);
        }
    }

    /// Apply one client command from `writer` (its client id).
    ///
    /// Returns a direct reply for `list_shows` (unicast `shows`), for
    /// `connect` (unicast `you`), for denials (unicast `error` with
    /// `locked_by`), and for `recall_scene` when it overrode foreign locks
    /// (unicast `recalled`); every other command broadcasts and returns
    /// `None`. Command-triggered broadcasts are collected in `out` (the
    /// caller flushes them after the reply); background tasks (poller,
    /// sweeper, meter ticker) broadcast directly. Mutation broadcasts
    /// carry `"by": writer`.
    async fn apply(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Option<Event>> {
        // MR1 write lane: mark the command in flight so the param poller
        // yields its tick while any command (fader/mute/param/main or
        // otherwise) runs. RAII: every return path below drops the guard.
        let _write_guard = WriteGuard::new(&self.writes_in_flight);
        // Opportunistic expiry: release stale holds before any check so a
        // just-expired strip reads as unlocked, and broadcast when the
        // sweep actually removed something.
        if self.prune_expired_locks() {
            out.push(self.locks_message_inner());
        }
        let policy = self.policy_of(writer);
        match cmd {
            Cmd::Connect { driver, addr } => {
                self.connect(&driver, addr.as_deref()).await?;
                // A fresh explicit connect clears offline editing (the desk
                // is authoritative again) unless a change list is pending,
                // in which case post_switch raises offline_diff.
                let ip = addr.unwrap_or_else(|| Self::default_addr(&driver).to_string());
                self.set_link_connected(None, &ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Fader { ch, value } => {
                policy.allow_fader(StripId(ch), value)?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                let from = self.offline_from(StripId(ch), "fader").await;
                match self.active_driver().as_str() {
                    "xair" => self.xair.set_fader(StripId(ch), value).await?,
                    "dl32s" => self.dl32s.set_fader(StripId(ch), value).await?,
                    _ => self.sim.set_fader(StripId(ch), value).await?,
                }
                if let Some(from) = from {
                    self.offline_note(ch, "fader", from, json!(value)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Mute { ch, muted } => {
                policy.allow_mute(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                let from = self.offline_from(StripId(ch), "mute").await;
                match self.active_driver().as_str() {
                    "xair" => self.xair.set_mute(StripId(ch), muted).await?,
                    "dl32s" => self.dl32s.set_mute(StripId(ch), muted).await?,
                    _ => self.sim.set_mute(StripId(ch), muted).await?,
                }
                if let Some(from) = from {
                    self.offline_note(ch, "mute", from, json!(muted)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // LR main fader (all drivers via `get_main_fader`; `None` in
            // state when the driver has none). Not strip-locked: locks pin
            // per-channel strips.
            Cmd::MainFader { value } => {
                let value = openmix_core::validate_fader(value)?;
                policy.allow_main(value)?;
                let from = if self.offline.read().unwrap().active {
                    Some(self.active_main_fader().await.map(|v| json!(v)).unwrap_or(Value::Null))
                } else {
                    None
                };
                match self.active_driver().as_str() {
                    "xair" => self.xair.set_main_fader(value).await?,
                    "dl32s" => self.dl32s.set_main_fader(value).await?,
                    _ => self.sim.set_main_fader(value).await?,
                }
                if let Some(from) = from {
                    self.offline_note(u16::MAX, "main_fader", from, json!(value)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Param { ch, field, value } => {
                let pv = match &value {
                    Value::Number(n) => ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32),
                    Value::Bool(b) => ParamValue::Bool(*b),
                    Value::String(s) => ParamValue::Text(s.clone()),
                    _ => anyhow::bail!("param value must be number|bool|string"),
                };
                match (&field as &str, &pv) {
                    ("fader", ParamValue::Float(v)) => {
                        policy.allow_fader(StripId(ch), *v)?;
                    }
                    ("name", _) => {
                        policy.allow_rename(StripId(ch))?;
                    }
                    _ => {
                        policy.allow_strip(StripId(ch))?;
                    }
                }
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                let from = self.offline_from(StripId(ch), &field).await;
                match self.active_driver().as_str() {
                    "xair" => self.xair.set_param(StripId(ch), &field, pv).await?,
                    "dl32s" => self.dl32s.set_param(StripId(ch), &field, pv).await?,
                    _ => self.sim.set_param(StripId(ch), &field, pv).await?,
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo: re-read the param just written and
                // broadcast what the driver reports (wire quantization may
                // shift it); fall back to the written value when the
                // re-read fails so the echo still lands.
                let confirmed = self
                    .refresh_param(StripId(ch), &field)
                    .await
                    .unwrap_or(value);
                if let Some(from) = from {
                    self.offline_note(ch, &field, from, confirmed.clone()).await;
                }
                out.push(Event::Param {
                    ch,
                    field,
                    value: confirmed,
                    by: Some(writer.to_string()),
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Focus { ch, chs } => {
                // Poll-priority hint: either `{"cmd":"focus","ch":N}` or
                // `{"cmd":"focus","chs":[...]}`. Valid channels move to the
                // front of the poll queue (re-read at the normal budget,
                // then round-robin resumes); unknown/out-of-range channels
                // are ignored with an `error` event — never a disconnect.
                let mut want: Vec<u16> = Vec::new();
                match (ch, chs) {
                    (_, Some(list)) => {
                        for item in &list {
                            let n = item.as_u64().ok_or_else(|| {
                                anyhow::anyhow!(
                                    "focus \"chs\" entries must be integers, got {item}"
                                )
                            })?;
                            let n: u16 = n
                                .try_into()
                                .map_err(|_| anyhow::anyhow!("focus channel {n} out of range"))?;
                            want.push(n);
                        }
                    }
                    (Some(n), None) => want.push(n),
                    (None, None) => anyhow::bail!(
                        "focus needs \"ch\" (integer) or \"chs\" (array of integers)"
                    ),
                }
                let nch = self.driver_strips(&self.active_driver());
                let valid: Vec<u16> =
                    want.iter().copied().filter(|c| (*c as usize) < nch).collect();
                let ignored: Vec<u16> =
                    want.iter().copied().filter(|c| (*c as usize) >= nch).collect();
                if valid.is_empty() {
                    anyhow::bail!("focus channel(s) out of range 0..{nch}");
                }
                self.request_focus(writer, &valid);
                if ignored.is_empty() {
                    Ok(None)
                } else {
                    anyhow::bail!("focus ignored out-of-range channel(s): {ignored:?}")
                }
            }
            Cmd::Lock { ch } => {
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                    // Already ours: refresh expiry below.
                }
                {
                    let mut locks = self.locks.write().unwrap();
                    locks.insert(
                        ch,
                        StripLock {
                            holder: writer.to_string(),
                            expires_at: Instant::now() + self.lock_ttl,
                        },
                    );
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.locks_message());
                Ok(None)
            }
            Cmd::Unlock { ch } => {
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                    {
                        let mut locks = self.locks.write().unwrap();
                        locks.remove(&ch);
                    }
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.locks_message());
                }
                // Unlocking an unlocked/expired strip is a no-op success.
                Ok(None)
            }
            // Server-side show library: scenes survive UI reloads and
            // driver switches with no localStorage round-trip.
            Cmd::ListShows => Ok(Some(self.shows_message())),
            Cmd::CaptureScene { show, name } => {
                let show = clean_name(&show, "show")?;
                let name = clean_name(&name, "name")?;
                let snap = self.active_snapshot().await?;
                let mut scene = Scene::capture(&name, &snap);
                scene.main_fader = self.active_main_fader().await;
                let mut shows = self.shows.write().unwrap();
                match shows.iter_mut().find(|s| s.name == show) {
                    Some(slot) => slot.add_scene(scene),
                    None => {
                        let mut fresh = Show::new(&show);
                        fresh.add_scene(scene);
                        shows.push(fresh);
                    }
                }
                drop(shows);
                out.push(self.shows_message());
                Ok(None)
            }
            Cmd::RecallScene {
                show,
                name,
                fade_ms,
            } => {
                policy.allow_recall()?;
                let show = clean_name(&show, "show")?;
                let name = clean_name(&name, "name")?;
                let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
                let scene = {
                    let shows = self.shows.read().unwrap();
                    shows
                        .iter()
                        .find(|s| s.name == show)
                        .and_then(|s| s.get(&name))
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {name:?} in show {show:?}"))?
                };
                self.recall_on_active(&scene, fade_ms).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Bypass locks, but report which locked strips were changed:
                // scene strips locked by someone other than the recaller.
                let mut overrode: Vec<u16> = Vec::new();
                for s in &scene.strips {
                    if let Some(holder) = self.lock_holder(s.strip.0) {
                        if holder != writer && !overrode.contains(&s.strip.0) {
                            overrode.push(s.strip.0);
                        }
                    }
                }
                overrode.sort_unstable();
                for s in &scene.strips {
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "fader".to_string(),
                        value: json!(s.fader),
                        by: Some(writer.to_string()),
                    });
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "mute".to_string(),
                        value: json!(s.mute),
                        by: Some(writer.to_string()),
                    });
                    for (field, val) in &s.params {
                        out.push(Event::Param {
                            ch: s.strip.0,
                            field: field.clone(),
                            value: param_value_json(val),
                            by: Some(writer.to_string()),
                        });
                    }
                }
                self.push_state_with_by(writer, out).await;
                if overrode.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Event::Recalled {
                        overrode_locks: overrode,
                    }))
                }
            }
            Cmd::DeleteScene { show, name } => {
                let show = clean_name(&show, "show")?;
                let name = clean_name(&name, "name")?;
                {
                    let mut shows = self.shows.write().unwrap();
                    let slot = shows
                        .iter_mut()
                        .find(|s| s.name == show)
                        .ok_or_else(|| anyhow::anyhow!("unknown show {show:?}"))?;
                    if !slot.remove(&name) {
                        anyhow::bail!("unknown scene {name:?} in show {show:?}");
                    }
                }
                out.push(self.shows_message());
                Ok(None)
            }
            // MR1 realtime negotiation: store the per-client binary-meter
            // preference and unicast the current value back (never
            // broadcast). Missing `binary_meters` just re-acks.
            // R2: `taps` works the same way for meter taps.
            Cmd::Hello { binary_meters, taps } => {
                if let Some(v) = binary_meters {
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.binary_meters = v;
                    }
                }
                if let Some(v) = taps {
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.taps = v;
                    }
                }
                Ok(Some(Event::Hello {
                    binary_meters: self.client_binary_meters(writer),
                    taps: self.client_taps(writer),
                }))
            }
            // ================= R2 =================
            Cmd::Scan => {
                let driver = self.active_driver();
                if driver == "sim" {
                    *self.devices.write().unwrap() = sim_devices();
                } else {
                    // Hardware: real LAN discovery behind the trait (fake
                    // in tests). An empty result still updates scanned_at.
                    let found = self.scanner.scan(Duration::from_secs(2)).await;
                    if !found.is_empty() {
                        *self.devices.write().unwrap() = found;
                    }
                }
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                out.push(self.devices_message());
                Ok(None)
            }
            Cmd::ConnectDevice { id } => {
                let entry = self
                    .devices
                    .read()
                    .unwrap()
                    .iter()
                    .find(|d| d.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("unknown device {id:?} (scan first)"))?;
                if entry.kind == "io" {
                    anyhow::bail!("{:?} is I/O (in use) — connect a console", entry.name);
                }
                // Discovered xair devices dial hardware (UNVERIFIED-ON-RIG);
                // the design's sim devices re-link the local model.
                if id.starts_with("xair-") {
                    self.connect("xair", Some(&entry.ip)).await?;
                } else {
                    self.connect("sim", Some(&entry.ip)).await?;
                }
                self.set_link_connected(Some(&id), &entry.ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Disconnect => {
                {
                    let mut link = self.link.write().unwrap();
                    link.state = "disconnected".to_string();
                }
                out.push(self.link_message_cached());
                Ok(None)
            }
            Cmd::Info => Ok(Some(self.info_message().await?)),
            Cmd::SceneSave { n, name } => {
                let n = validate_scene_n(n)?;
                let name = clean_name(&name, "name")?;
                let scene = self.capture_full(n, &name).await?;
                self.scenes_lib.write().unwrap().insert(n, scene);
                self.save_scenes();
                out.push(self.scenes_message());
                Ok(None)
            }
            Cmd::SceneRecall { n, fade_ms } => {
                policy.allow_recall()?;
                let n = validate_scene_n(n)?;
                let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
                let scene = {
                    self.scenes_lib
                        .read()
                        .unwrap()
                        .get(&n)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {n:03}"))?
                };
                let overrode = self.recall_full(&scene, fade_ms, writer, out).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                *self.scenes_current.write().unwrap() = Some(n);
                self.push_state_with_by(writer, out).await;
                out.push(self.scenes_message());
                if overrode.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Event::Recalled {
                        overrode_locks: overrode,
                    }))
                }
            }
            Cmd::SceneRename { n, name } => {
                let n = validate_scene_n(n)?;
                let name = clean_name(&name, "name")?;
                {
                    let mut lib = self.scenes_lib.write().unwrap();
                    let slot = lib
                        .get_mut(&n)
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {n:03}"))?;
                    slot.name = name;
                    slot.saved_at = now_unix();
                }
                self.save_scenes();
                out.push(self.scenes_message());
                Ok(None)
            }
            Cmd::SceneDelete { n } => {
                let n = validate_scene_n(n)?;
                {
                    let mut lib = self.scenes_lib.write().unwrap();
                    if lib.remove(&n).is_none() {
                        anyhow::bail!("unknown scene {n:03}");
                    }
                }
                if *self.scenes_current.read().unwrap() == Some(n) {
                    *self.scenes_current.write().unwrap() = None;
                }
                self.save_scenes();
                out.push(self.scenes_message());
                Ok(None)
            }
            Cmd::SceneClear => {
                self.scenes_lib.write().unwrap().clear();
                *self.scenes_current.write().unwrap() = None;
                self.save_scenes();
                out.push(self.scenes_message());
                Ok(None)
            }
            Cmd::FxSetType { slot, fx_type } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let max = self.fx_slots_of(&driver);
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                let fx_type = fx_type.trim().to_string();
                openmix_core::fx::fx_type_meta(&fx_type)
                    .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
                match driver.as_str() {
                    "sim" => {
                        let from = self.sim.get_fx_type(slot).unwrap_or_default();
                        self.sim.set_fx_type(slot, &fx_type)?;
                        self.offline_note(slot as u16, "fx/type", json!(from), json!(fx_type.clone())).await;
                        self.refresh_fx_cache().await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.fx_message().await);
                        Ok(None)
                    }
                    _ => anyhow::bail!(
                        "{driver}: fx type select not supported (desk-native types are read-only here; UNVERIFIED-ON-RIG)"
                    ),
                }
            }
            Cmd::FxParam { slot, id, value } => {
                policy.allow_recall()?;
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    anyhow::bail!("fx param {value} out of range 0.0..=1.0");
                }
                let driver = self.active_driver();
                let max = self.fx_slots_of(&driver);
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                match driver.as_str() {
                    "sim" => {
                        let from = self
                            .sim
                            .get_fx_param(slot, &id)
                            .map(|v| json!(v))
                            .unwrap_or(Value::Null);
                        self.sim.set_fx_param(slot, &id, value)?;
                        self.offline_note(slot as u16, &format!("fx/{id}"), from, json!(value)).await;
                        self.refresh_fx_cache().await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.fx_message().await);
                        Ok(None)
                    }
                    "xair" => {
                        let idx = parse_par_id(&id).ok_or_else(|| {
                            anyhow::anyhow!("{id:?} has no xair mapping (use par01..par64; UNVERIFIED-ON-RIG)")
                        })?;
                        self.xair.set_fx_param(slot, idx, value).await?;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.fx_message().await);
                        Ok(None)
                    }
                    d => anyhow::bail!("{d}: fx params not supported"),
                }
            }
            Cmd::FxBypass { slot, bypass } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let max = self.fx_slots_of(&driver);
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                match driver.as_str() {
                    "sim" => {
                        let from = self
                            .fx_cache
                            .read()
                            .unwrap()
                            .iter()
                            .find(|r| r.slot == slot)
                            .map(|r| r.bypass)
                            .unwrap_or(!bypass);
                        self.sim.set_fx_bypass(slot, bypass)?;
                        self.offline_note(slot as u16, "fx/bypass", json!(from), json!(bypass)).await;
                        self.refresh_fx_cache().await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.fx_message().await);
                        Ok(None)
                    }
                    _ => anyhow::bail!(
                        "{driver}: fx bypass not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    ),
                }
            }
            Cmd::FxOn { slot, on } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let max = self.fx_slots_of(&driver);
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                match driver.as_str() {
                    "sim" => {
                        let from = self
                            .fx_cache
                            .read()
                            .unwrap()
                            .iter()
                            .find(|r| r.slot == slot)
                            .map(|r| r.on)
                            .unwrap_or(!on);
                        self.sim.set_fx_on(slot, on)?;
                        self.offline_note(slot as u16, "fx/on", json!(from), json!(on)).await;
                        self.refresh_fx_cache().await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.fx_message().await);
                        Ok(None)
                    }
                    _ => anyhow::bail!(
                        "{driver}: fx on/off not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    ),
                }
            }
            Cmd::PatchSet { strip, group, index } => {
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                let driver = self.active_driver();
                match driver.as_str() {
                    "sim" => {
                        let from = self
                            .sim
                            .get_param(StripId(strip), "source")
                            .await
                            .map(|pv| param_value_json(&pv))
                            .unwrap_or(Value::Null);
                        let label = self.sim.set_patch(StripId(strip), &group, index)?;
                        self.offline_note(strip, "patch", from, json!(label)).await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.routing_message().await);
                        Ok(None)
                    }
                    "xair" => anyhow::bail!(
                        "xair: routing patch not supported (insrc is live-silent on FW 1.20)"
                    ),
                    d => anyhow::bail!("{d}: routing patch not supported"),
                }
            }
            Cmd::PatchClear { strip } => {
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                match self.active_driver().as_str() {
                    "sim" => {
                        let from = self
                            .sim
                            .get_param(StripId(strip), "source")
                            .await
                            .map(|pv| param_value_json(&pv))
                            .unwrap_or(Value::Null);
                        self.sim.clear_patch(StripId(strip))?;
                        self.offline_note(strip, "patch", from, json!("—")).await;
                        self.note_write(writer);
                        self.touch_holder_locks(writer);
                        out.push(self.routing_message().await);
                        Ok(None)
                    }
                    "xair" => anyhow::bail!(
                        "xair: routing patch not supported (insrc is live-silent on FW 1.20)"
                    ),
                    d => anyhow::bail!("{d}: routing patch not supported"),
                }
            }
            Cmd::Offline { model } => {
                if !Self::known(&model) {
                    anyhow::bail!("unknown driver {model:?}; expected sim|xair|dl32s");
                }
                {
                    let mut off = self.offline.write().unwrap();
                    off.active = true;
                    off.model = model.clone();
                    off.prev_driver = self.active_driver();
                    off.changes.clear();
                    off.pending_push = false;
                }
                *self.active.write().unwrap() = "sim".to_string();
                {
                    let mut link = self.link.write().unwrap();
                    link.state = "offline".to_string();
                    link.name = format!("Offline ({model})");
                    link.ip = String::new();
                }
                self.post_switch(writer, out, true).await?;
                // post_switch broadcasts the pre-offline link frame; swap
                // it for the offline one.
                if let Some(last) = out.iter_mut().rev().find(|e| matches!(e, Event::Link { .. })) {
                    *last = self.link_message_cached();
                }
                Ok(None)
            }
            Cmd::OfflinePush { confirm } => {
                if !confirm {
                    anyhow::bail!("offline_push needs confirm:true");
                }
                if !self.offline.read().unwrap().pending_push {
                    anyhow::bail!("no offline diff pending (connect with offline changes first)");
                }
                policy.allow_recall()?;
                self.offline_push_apply(writer, out).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::OfflineDiscard => {
                {
                    let mut off = self.offline.write().unwrap();
                    off.active = false;
                    off.changes.clear();
                    off.pending_push = false;
                }
                out.push(self.offline_diff_message());
                Ok(None)
            }
        }
    }
}

/// A client's handle on the [`Engine`]: its id, its live event mailbox,
/// and typed/JSON command entry points.
///
/// `events` first yields the attach snapshot (`you`, `state`, `shows`,
/// `clients`, `locks`, `caps`, `devices`, `link`, `info`?, `scenes`,
/// `fx`, `routing`, then one `params` frame per cached channel)
/// in that order, then every live broadcast. Unicast frames (`you` replies,
/// `list_shows` replies, denials, `recalled` reports, command errors) arrive
/// only in the offending/sender's mailbox.
///
/// Dropping the handle detaches the client (best-effort synchronous;
/// prefer an explicit [`detach`](ClientHandle::detach) so the roster update
/// and lock auto-release are guaranteed to run).
pub struct ClientHandle {
    /// Server-side id (`client-N`); matches later `by` tags.
    pub id: String,
    engine: Arc<Engine>,
    /// Live event mailbox (unbounded: slow clients never apply
    /// backpressure; the sweeper prunes detached mailboxes).
    pub events: mpsc::UnboundedReceiver<Event>,
}

impl ClientHandle {
    /// Send one typed command. Domain errors (bad args, denied writes,
    /// driver failures) arrive as unicast [`Event::Error`] in `events`;
    /// `Err` only means the client is already detached.
    ///
    /// Delivery order mirrors the relay socket: the unicast reply (or
    /// error) lands in the sender's mailbox *before* the broadcasts the
    /// command triggered, so a client waiting for its reply never consumes
    /// its own follow-up frames first.
    pub async fn send(&self, cmd: Cmd) -> anyhow::Result<()> {
        if !self.engine.has_client(&self.id) {
            anyhow::bail!("client {} is detached", self.id);
        }
        self.engine.apply_cmd(&self.id, cmd).await;
        Ok(())
    }

    /// Send one raw JSON command line (what adapters get off the socket).
    /// Never fails: parse failures also arrive as unicast `error` events,
    /// so bad input never closes the connection.
    pub async fn send_json(&self, text: &str) -> anyhow::Result<()> {
        if !self.engine.has_client(&self.id) {
            anyhow::bail!("client {} is detached", self.id);
        }
        self.engine.handle_json(&self.id, text).await;
        Ok(())
    }

    /// Explicit detach (roster update + lock auto-release).
    pub async fn detach(&self) {
        self.engine.remove_client(&self.id);
    }
}

impl Drop for ClientHandle {
    fn drop(&mut self) {
        self.engine.remove_client(&self.id);
    }
}

impl Engine {
    fn has_client(&self, id: &str) -> bool {
        self.clients.read().unwrap().contains_key(id)
    }
}

/// Write-through [`openmix_core::show::FadeDriver`]: numbered-scene recall
/// shares the single [`openmix_core::show::fade_strips`] implementation with
/// named-show recall (which fades through the driver blanket impl).
impl openmix_core::show::FadeDriver for Engine {
    async fn fade_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.drv_set_mute(strip, muted).await
    }

    async fn fade_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.drv_get_fader(strip).await
    }

    async fn fade_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.drv_set_fader(strip, value).await
    }
}

async fn snapshot_strips(d: &impl MixerDriver) -> Vec<crate::protocol::StateStrip> {
    match d.snapshot().await {
        Ok(snap) => snap
            .strips
            .iter()
            .map(|s| crate::protocol::StateStrip {
                ch: s.id.0,
                name: s.name.clone(),
                fader: s.fader,
                muted: s.mute,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// One `caps` catalogue entry.
fn param_def_frame(def: &params::ParamDef) -> ParamDef {
    use params::ParamKind;
    let (min, max) = match def.kind {
        ParamKind::Float | ParamKind::Enum => (json!(def.min), json!(def.max)),
        ParamKind::Bool | ParamKind::Text => (Value::Null, Value::Null),
    };
    ParamDef {
        id: def.id.to_string(),
        kind: def.kind.as_str().to_string(),
        scope: def.scope.as_str().to_string(),
        scopes: def.scopes.iter().map(|k| k.as_str().to_string()).collect(),
        min,
        max,
        unit: def.unit.to_string(),
        default: param_value_json(&def.default.as_param_value()),
        label: def.label.to_string(),
    }
}

/// Drop-class poll errors: the param has no wire address on this driver
/// (unknown/unsupported) or the desk never answers (timeouts). Everything
/// else — empty sim slots (`no value yet`), unpushed dl32s cache
/// (`no value pushed yet`), `not connected`, transient I/O — keeps polling.
fn is_unsupported_error(e: &anyhow::Error) -> bool {
    let m = format!("{e:#}");
    m.contains("not supported") || m.contains("unknown param") || m.contains("timed out")
}

fn param_value_json(pv: &ParamValue) -> Value {
    match pv {
        ParamValue::Float(v) => json!(v),
        ParamValue::Bool(b) => json!(b),
        ParamValue::Text(s) => json!(s),
    }
}

/// Compare-and-store one poll read; true when the value is new or
/// changed (i.e. a `params` frame is due).
fn cache_store(
    cache: &mut HashMap<u16, HashMap<String, Value>>,
    ch: u16,
    id: &str,
    v: Value,
) -> bool {
    let entry = cache.entry(ch).or_default();
    match entry.get(id) {
        Some(old) if old == &v => false,
        _ => {
            entry.insert(id.to_string(), v);
            true
        }
    }
}

/// R2 seed devices: the design's connection screen (FOH .20, Monitor .21,
/// Stage Box A .40). States here are the scan-time base; the `devices`
/// event overrides the linked console with `connected`.
fn sim_devices() -> Vec<DeviceDesc> {
    openmix_sim::SIM_DEVICES
        .iter()
        .map(|d| DeviceDesc {
            id: d.id.to_string(),
            name: d.name.to_string(),
            model: d.model.to_string(),
            ip: d.ip.to_string(),
            kind: d.kind.to_string(),
            state: d.state.to_string(),
            fw: Some("0.1.0".to_string()),
        })
        .collect()
}

/// Map a sim slot's positional param vector onto design param ids.
fn param_vec_to_map(fx_type: &str, values: &[f32]) -> BTreeMap<String, f32> {
    match openmix_core::fx::fx_type_meta(fx_type) {
        Some(meta) => meta
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id.to_string(), values.get(i).copied().unwrap_or(p.default)))
            .collect(),
        None => values
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("p{i}"), *v))
            .collect(),
    }
}

/// Parse an xair numbered-param id (`par01`..`par64`, 1-based wire) into
/// the 0-based trait index. Design ids (`mix`, `rate`, …) have no verified
/// xair mapping and return `None`.
fn parse_par_id(id: &str) -> Option<u8> {
    let rest = id.strip_prefix("par")?;
    let n: u8 = rest.parse().ok()?;
    if (1..=64).contains(&n) {
        Some(n - 1)
    } else {
        None
    }
}

/// Parse a patch label (`Lcl 01`, `Net 12`, `USB 03`) into (group, 1-based
/// index). The design's unpatched tile (`—`) and anything else → `None`.
fn parse_source(label: &str) -> Option<(String, u8)> {
    let (group, num) = label.split_once(' ')?;
    if !["Lcl", "Net", "USB"].contains(&group) {
        return None;
    }
    let index: u8 = num.parse().ok()?;
    if (1..=32).contains(&index) {
        Some((group.to_string(), index))
    } else {
        None
    }
}

/// R2 visible-lane priority ids: EQ + dynamics + sends (+HPF). Everything
/// else drains on the background lane.
fn is_priority_id(id: &str) -> bool {
    id == "eq_on"
        || id.starts_with("eq")
        || id.starts_with("gate_")
        || id.starts_with("comp_")
        || id.starts_with("send")
        || id.starts_with("aux")
        || id.starts_with("fx")
        || id.starts_with("hpf")
}

/// Safe recall order for full-plane scene params: input stage first
/// (gain → HPF), then gate → comp → EQ → sends, then placement/look.
/// Faders/mutes/names ride outside this order (fade block + name block).
fn safe_order_key(id: &str) -> (u8, &str) {
    let lane = if ["gain", "trim", "phantom", "polarity", "delay_on", "delay_ms"].contains(&id) {
        2u8
    } else if id.starts_with("hpf") {
        3
    } else if id.starts_with("gate_") {
        4
    } else if id.starts_with("comp_") {
        5
    } else if id == "eq_on" || id.starts_with("eq") {
        6
    } else if id.starts_with("send") || id.starts_with("aux") || id.starts_with("fx") {
        7
    } else if id == "pan" {
        8
    } else {
        9
    };
    (lane, id)
}

/// JSON engineering value back to a driver [`ParamValue`].
fn json_to_param_value(v: &Value) -> anyhow::Result<ParamValue> {
    match v {
        Value::Number(n) => Ok(ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32)),
        Value::Bool(b) => Ok(ParamValue::Bool(*b)),
        Value::String(s) => Ok(ParamValue::Text(s.clone())),
        _ => anyhow::bail!("scene value must be number|bool|string, got {v}"),
    }
}

/// Scene library file (`scenes.json`) under the data dir.
fn scene_file(data_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join("scenes.json"))
}

/// Load the persisted R2 scene library (best-effort: missing/corrupt
/// files read back empty — a bad file must never block engine start).
fn load_scenes(data_dir: Option<&std::path::Path>) -> HashMap<u16, StoredScene> {
    let Some(path) = scene_file(data_dir) else {
        return HashMap::new();
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let mut out = HashMap::new();
    let scenes = v.get("scenes").and_then(Value::as_array).cloned().unwrap_or_default();
    for s in scenes {
        let number = s.get("number").and_then(Value::as_u64).unwrap_or(0) as u16;
        if validate_scene_n(number).is_err() {
            continue;
        }
        let name = s.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.trim().is_empty() {
            continue;
        }
        let saved_at = s.get("saved_at").and_then(Value::as_u64).unwrap_or(0);
        let main_fader = s.get("main_fader").and_then(Value::as_f64).map(|f| f as f32);
        let mut strips = Vec::new();
        for st in s.get("strips").and_then(Value::as_array).cloned().unwrap_or_default() {
            let strip = st.get("strip").and_then(Value::as_u64).unwrap_or(0) as u16;
            let mut params = HashMap::new();
            if let Some(obj) = st.get("params").and_then(Value::as_object) {
                for (k, val) in obj {
                    if val.is_number() || val.is_boolean() || val.is_string() {
                        params.insert(k.clone(), val.clone());
                    }
                }
            }
            strips.push(StoredStrip {
                strip,
                name: st.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
                fader: st.get("fader").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                mute: st.get("mute").and_then(Value::as_bool).unwrap_or(false),
                params,
            });
        }
        let mut fx = Vec::new();
        for f in s.get("fx").and_then(Value::as_array).cloned().unwrap_or_default() {
            let mut params = BTreeMap::new();
            if let Some(obj) = f.get("params").and_then(Value::as_object) {
                for (k, val) in obj {
                    if let Some(n) = val.as_f64() {
                        params.insert(k.clone(), n as f32);
                    }
                }
            }
            fx.push(StoredFx {
                slot: f.get("slot").and_then(Value::as_u64).unwrap_or(0) as u8,
                fx_type: f.get("type").and_then(Value::as_str).unwrap_or("").to_string(),
                on: f.get("on").and_then(Value::as_bool).unwrap_or(true),
                bypass: f.get("bypass").and_then(Value::as_bool).unwrap_or(false),
                params,
            });
        }
        let mut routing = Vec::new();
        for r in s.get("routing").and_then(Value::as_array).cloned().unwrap_or_default() {
            let strip = r.get("strip").and_then(Value::as_u64).unwrap_or(0) as u16;
            let group = r.get("group").and_then(Value::as_str).unwrap_or("").to_string();
            let index = r.get("index").and_then(Value::as_u64).unwrap_or(0) as u8;
            routing.push((strip, group, index));
        }
        out.insert(
            number,
            StoredScene {
                number,
                name,
                saved_at,
                main_fader,
                strips,
                fx,
                routing,
            },
        );
    }
    out
}

/// Persist the R2 scene library (best-effort: failures are swallowed — a
/// read-only data dir must never break scene commands).
fn persist_scenes(data_dir: Option<&std::path::Path>, lib: &HashMap<u16, StoredScene>) {
    let Some(path) = scene_file(data_dir) else {
        return;
    };
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let mut scenes: Vec<Value> = lib
        .values()
        .map(|s| {
            json!({
                "number": s.number,
                "name": s.name,
                "saved_at": s.saved_at,
                "main_fader": s.main_fader,
                "strips": s.strips.iter().map(|st| json!({
                    "strip": st.strip,
                    "name": st.name,
                    "fader": st.fader,
                    "mute": st.mute,
                    "params": st.params,
                })).collect::<Vec<_>>(),
                "fx": s.fx.iter().map(|f| json!({
                    "slot": f.slot,
                    "type": f.fx_type,
                    "on": f.on,
                    "bypass": f.bypass,
                    "params": f.params,
                })).collect::<Vec<_>>(),
                "routing": s.routing.iter().map(|(strip, group, index)| json!({
                    "strip": strip,
                    "group": group,
                    "index": index,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    scenes.sort_by_key(|s| s.get("number").and_then(Value::as_u64).unwrap_or(0));
    let doc = json!({ "version": 1, "scenes": scenes });
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap_or_default());
}

/// Trimmed non-empty arg (`"show"` / `"name"` for scene ops).
fn clean_name(raw: &str, key: &str) -> anyhow::Result<String> {
    let clean = raw.trim().to_string();
    if clean.is_empty() {
        anyhow::bail!("scene command needs non-empty \"{key}\"");
    }
    Ok(clean)
}

/// One show in the `shows` event, in library order.
fn show_frame(show: &Show) -> ShowFrame {
    ShowFrame {
        name: show.name.clone(),
        scenes: show.scenes.iter().map(scene_frame).collect(),
    }
}

/// One scene in the `shows` event.
fn scene_frame(scene: &Scene) -> SceneFrame {
    SceneFrame {
        name: scene.name.clone(),
        main_fader: scene.main_fader,
        strips: scene
            .strips
            .iter()
            .map(|s| crate::protocol::SceneStrip {
                ch: s.strip.0,
                name: s.name.clone(),
                fader: s.fader,
                muted: s.mute,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poller_yield_rule() {
        // MR1 write priority: the poller skips its tick while ANY command
        // is inside apply (writes_in_flight > 0), and only then.
        assert!(!Engine::poller_yields(0));
        assert!(Engine::poller_yields(1));
        assert!(Engine::poller_yields(u64::MAX));
    }

    #[test]
    fn cache_store_diffs() {
        let mut cache: HashMap<u16, HashMap<String, Value>> = HashMap::new();
        // New value: changed.
        assert!(cache_store(&mut cache, 0, "gain", json!(0.0)));
        // Same value: quiet (no frame when unchanged).
        assert!(!cache_store(&mut cache, 0, "gain", json!(0.0)));
        // Changed value: one frame on change.
        assert!(cache_store(&mut cache, 0, "gain", json!(1.5)));
        assert!(!cache_store(&mut cache, 0, "gain", json!(1.5)));
        // Other channels/ids are independent.
        assert!(cache_store(&mut cache, 1, "gain", json!(1.5)));
        assert!(cache_store(&mut cache, 0, "pan", json!(0.0)));
    }

    #[test]
    fn unsupported_error_shapes() {
        let timeout = anyhow::anyhow!("/ch/01/eq/on: timed out after 2s");
        assert!(is_unsupported_error(&timeout));
        assert!(is_unsupported_error(&anyhow::anyhow!("xair: unknown param \"nope\"")));
        assert!(is_unsupported_error(&anyhow::anyhow!("xair: get_fader not supported")));
        // No-data errors keep polling: empty sim slots, unpushed dl32s
        // cache, and a driver that is not connected yet.
        assert!(!is_unsupported_error(&anyhow::anyhow!("sim: gain has no value yet")));
        assert!(!is_unsupported_error(&anyhow::anyhow!("fader: no value pushed yet")));
        assert!(!is_unsupported_error(&anyhow::anyhow!("not connected: call connect() first")));
    }

    fn event_json(event: Event) -> Value {
        serde_json::to_value(event).unwrap()
    }

    #[tokio::test]
    async fn caps_sim_lists_full_catalogue() {
        let engine = Engine::new(EngineConfig::default());
        let caps = event_json(engine.caps_message());
        assert_eq!(caps.get("type"), Some(&json!("caps")));
        assert_eq!(caps.get("driver"), Some(&json!("sim")));
        let params = caps.get("params").and_then(Value::as_array).unwrap();
        assert_eq!(params.len(), openmix_core::params::CATALOGUE.len());
        let ids: Vec<&str> = params
            .iter()
            .map(|p| p.get("id").and_then(Value::as_str).unwrap())
            .collect();
        for def in openmix_core::params::CATALOGUE {
            assert!(ids.contains(&def.id), "caps miss {}", def.id);
        }
        // Every entry carries the pinned frame shape, v2 scopes included
        // (legacy `scope` kept for back-compat).
        for p in params {
            for key in ["id", "kind", "scope", "scopes", "min", "max", "unit", "default", "label"] {
                assert!(p.get(key).is_some(), "caps entry misses {key}: {p}");
            }
        }
        // `caps.topology` covers the sim strip space; `meter_taps` names the
        // declared taps (additive: old fields byte-identical).
        let topo = caps.get("topology").and_then(Value::as_array).unwrap();
        assert_eq!(topo.len(), openmix_sim::SIM_TOPOLOGY.len());
        let first = &topo[0];
        assert_eq!(first.get("key"), Some(&json!("ch")));
        assert_eq!(first.get("kind"), Some(&json!("in")));
        assert_eq!(first.get("first"), Some(&json!(0)));
        assert_eq!(first.get("count"), Some(&json!(32)));
        for key in ["key", "kind", "label", "first", "count", "name_prefix", "color_default", "params_scope"] {
            assert!(first.get(key).is_some(), "topology entry misses {key}: {first}");
        }
        assert_eq!(
            caps.get("meter_taps"),
            Some(&json!(["in", "post", "gr_gate", "gr_comp"]))
        );
        assert_eq!(caps.get("unsupported"), Some(&json!([])));
    }

    #[test]
    fn driver_capability_sets() {
        let engine = Engine::new(EngineConfig::default());
        // sim = full catalogue; xair = loopback-tested subset; dl32s =
        // fader/mute/name only (unchanged driver behaviour otherwise).
        assert_eq!(
            engine.driver_capabilities("sim").len(),
            openmix_core::params::CATALOGUE.len()
        );
        assert_eq!(
            engine.driver_capabilities("xair"),
            openmix_proto_xair::XAIR_CAPABILITIES
        );
        assert_eq!(engine.driver_capabilities("dl32s"), &["fader", "mute", "name"]);
        assert_eq!(engine.driver_capabilities("nope"), engine.driver_capabilities("sim"));
        assert_eq!(engine.driver_channels("xair"), 16);
        assert_eq!(engine.driver_channels("sim"), 32);
        // Strip totals (topology): sim 77, xair 35, dl32s 32.
        assert_eq!(engine.driver_strips("sim"), openmix_sim::SIM_STRIPS);
        assert_eq!(engine.driver_strips("xair"), openmix_proto_xair::XAIR_STRIPS);
        assert_eq!(engine.driver_strips("dl32s"), 32);
        assert_eq!(engine.driver_topology("sim").len(), openmix_sim::SIM_TOPOLOGY.len());
        assert_eq!(engine.driver_topology("xair").len(), openmix_proto_xair::XAIR_TOPOLOGY.len());
        assert_eq!(engine.driver_meter_taps("sim"), vec!["in", "post", "gr_gate", "gr_comp"]);
    }

    #[tokio::test]
    async fn dropout_shrinks_caps_once() {
        let engine = Engine::new(EngineConfig::default());
        let mut client = engine.attach(None).await;
        // Drain the attach snapshot (R1 six-frame prefix, then the R2
        // frames: devices/link/info/scenes/fx/routing).
        let mut tags: Vec<String> = Vec::new();
        for _ in 0..12 {
            let event = tokio::time::timeout(Duration::from_secs(2), client.events.recv())
                .await
                .expect("snapshot in time")
                .expect("mailbox open");
            tags.push(event_json(event).get("type").and_then(Value::as_str).unwrap_or("?").to_string());
        }
        assert_eq!(
            &tags[..6],
            ["you", "state", "shows", "clients", "locks", "caps"],
            "R1 snapshot prefix unchanged"
        );
        assert_eq!(
            &tags[6..],
            ["devices", "link", "info", "scenes", "fx", "routing"],
            "R2 snapshot suffix"
        );
        // Timeout-class read: dropped from the poll set and reported.
        let err = anyhow::anyhow!("/ch/01/eq/on: timed out after 2s");
        assert!(engine.note_poll_error("xair", "eq_on", &err).await);
        assert!(
            engine
                .param_dropped
                .read()
                .unwrap()
                .contains(&("xair".to_string(), "eq_on".to_string()))
        );
        // Re-report is suppressed (no log/caps spam): second call is a no-op.
        assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
        // Other drivers are unaffected.
        assert!(
            !engine
                .param_dropped
                .read()
                .unwrap()
                .contains(&("sim".to_string(), "eq_on".to_string()))
        );
        // One caps update went out on the fanout.
        let msg = tokio::time::timeout(Duration::from_secs(2), client.events.recv())
            .await
            .expect("caps update in time")
            .expect("mailbox open");
        assert_eq!(event_json(msg).get("type"), Some(&json!("caps")));
        // No-data errors never drop.
        let nodata = anyhow::anyhow!("sim: gain has no value yet");
        assert!(!engine.note_poll_error("sim", "gain", &nodata).await);
        assert!(engine.param_dropped.read().unwrap().len() == 1);
    }

    #[tokio::test]
    async fn poll_list_is_channel_major() {
        let engine = Engine::new(EngineConfig::default());
        for driver in ["sim", "xair", "dl32s"] {
            let list = engine.poll_list(driver).await;
            assert!(!list.is_empty(), "poll list for {driver}");
            // Strips non-decreasing: every param of strip N precedes
            // strip N+1, so one strip's whole set arrives as a block.
            let mut last_ch: u16 = 0;
            for (ch, _) in &list {
                assert!(*ch >= last_ch, "poll list not strip-major for {driver}");
                last_ch = *ch;
            }
            // Each strip carries the driver's supported id set filtered by
            // catalogue scopes (DCA strips: fader/mute/name/color only), in
            // capabilities order. Strips covered: the whole topology.
            let topo = engine.driver_topology(driver);
            let total: usize = topo.iter().map(|f| f.count).sum();
            let mut expect = 0usize;
            for fam in &topo {
                let n = engine
                    .driver_capabilities(driver)
                    .iter()
                    .filter(|id| {
                        openmix_core::params::find(id)
                            .map(|def| openmix_core::params::supports_kind(def, fam.kind))
                            .unwrap_or(true)
                    })
                    .count();
                expect += fam.count * n;
                for s in fam.first..fam.first + fam.count {
                    let ids: Vec<&str> = list
                        .iter()
                        .filter(|(c, _)| *c == s as u16)
                        .map(|(_, id)| id.as_str())
                        .collect();
                    let want: Vec<&str> = engine
                        .driver_capabilities(driver)
                        .iter()
                        .filter(|id| {
                            openmix_core::params::find(id)
                                .map(|def| openmix_core::params::supports_kind(def, fam.kind))
                                .unwrap_or(true)
                        })
                        .copied()
                        .collect();
                    assert_eq!(ids, want, "strip {s} carries the scoped set for {driver}");
                }
            }
            assert_eq!(list.len(), expect, "poll list size for {driver}");
            let max_strip = list.iter().map(|(c, _)| *c).max().unwrap() as usize;
            assert_eq!(max_strip + 1, total, "poll list covers the topology for {driver}");
        }
    }

    /// Engine with the background poller parked for exact-sequence
    /// tests: the interval is huge, so only the single immediate first
    /// tick ever fires (tokio intervals tick once immediately, then
    /// sleep). Pair with [`settle_parked_poller`] after attach.
    fn parked_engine() -> Arc<Engine> {
        Engine::new(EngineConfig {
            poll_interval: Duration::from_secs(3600),
            ..EngineConfig::default()
        })
    }

    /// Wait out the parked poller's single immediate tick, then drain the
    /// mailbox: after this returns `poll_once` is the only thing moving
    /// the cursor/queue/cache, so exact broadcast-sequence assertions are
    /// deterministic even under workspace-parallel load.
    async fn settle_parked_poller(engine: &Engine, client: &mut ClientHandle) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while engine.poll_cursor.load(Ordering::Relaxed) < 4 {
            if Instant::now() > deadline {
                panic!("parked poller never ran its first tick");
            }
            tokio::task::yield_now().await;
        }
        // The cursor advances before the tick's reads/broadcasts land;
        // let them land, then drain so the manual phase starts clean.
        tokio::time::sleep(Duration::from_millis(100)).await;
        while client.events.try_recv().is_ok() {}
    }

    #[tokio::test]
    async fn focus_jumps_queue() {
        // Parked poller: exact broadcast-sequence assertions below race
        // the live 50 ms poller (it steals focus entries mid-test; the
        // tail of the manual block then falls through to round-robin).
        let engine = parked_engine();
        let mut client = engine.attach(None).await;
        // Drain the attach snapshot (no params frames: cache starts empty).
        for _ in 0..6 {
            client.events.recv().await.expect("snapshot");
        }
        settle_parked_poller(&engine, &mut client).await;
        // Advance partway into channel 0's block.
        for _ in 0..10 {
            engine.poll_once().await;
        }
        // Focus channel 5: the next whole block of reads is channel 5.
        engine.request_focus("client-1", &[5]);
        // The focus block is ch5's SCOPED set (catalogue scopes decide;
        // e.g. bus/main-only EQ bands are not in it).
        let nparams = engine
            .poll_list("sim")
            .await
            .iter()
            .filter(|(c, _)| *c == 5)
            .count();
        assert!(nparams > 100, "focused strip carries the input set");
        for _ in 0..nparams {
            engine.poll_once().await;
        }
        // Diff frames land in the mailbox in poll order: 10 round-robin
        // reads, then the whole focus block on ch5.
        let mut chs: Vec<u16> = Vec::new();
        while let Ok(event) = client.events.try_recv() {
            if let Event::Params { ch, .. } = event {
                chs.push(ch);
            }
        }
        assert_eq!(chs.len(), 10 + nparams, "10 round-robin + {nparams} focus reads");
        assert!(
            chs[10..].iter().all(|&c| c == 5),
            "focus block is all ch5, got {chs:?}"
        );
        // Channel 5's cache is complete while channel 0's is still partial.
        let cache = engine.param_cache.read().unwrap();
        assert_eq!(
            cache.get(&5).map(|m| m.len()),
            Some(nparams),
            "focused ch5 complete"
        );
        assert!(
            cache.get(&0).map(|m| m.len()).unwrap_or(0) < nparams,
            "ch0 still partial"
        );
    }

    #[tokio::test]
    async fn sim_full_cycle_fills_every_strip() {
        // Engine invariant: after one full poll cycle every strip has a
        // value for every supported scoped id — the poller never silently
        // skips a supported (strip, param) pair.
        let engine = Engine::new(EngineConfig::default());
        let list = engine.poll_list("sim").await;
        for _ in 0..list.len() {
            engine.poll_once().await;
        }
        let cache = engine.param_cache.read().unwrap();
        let topo = engine.driver_topology("sim");
        let sim_caps = engine.driver_capabilities("sim");
        for fam in &topo {
            let want: Vec<&&str> = sim_caps
                .iter()
                .filter(|id| {
                    openmix_core::params::find(id)
                        .map(|def| openmix_core::params::supports_kind(def, fam.kind))
                        .unwrap_or(true)
                })
                .collect();
            for s in fam.first..fam.first + fam.count {
                let got = cache.get(&(s as u16)).map(|m| m.len()).unwrap_or(0);
                assert_eq!(
                    got, want.len(),
                    "strip {s} has every scoped param after one cycle ({got}/{})",
                    want.len()
                );
                for id in &want {
                    assert!(
                        cache.get(&(s as u16)).is_some_and(|m| m.contains_key(**id)),
                        "strip {s} misses {id}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn focus_flood_superseded_by_latest() {
        // CATALOGUE-V2 §5: flood the queue with 30 focus commands, then
        // focus strip N — N's full set drains first (most-recent-first),
        // ahead of the flood backlog. Parked poller + settle: the live
        // poller would otherwise steal focus entries mid-test (see
        // `focus_jumps_queue`).
        let engine = parked_engine();
        let mut client = engine.attach(None).await;
        for _ in 0..6 {
            client.events.recv().await.expect("snapshot");
        }
        settle_parked_poller(&engine, &mut client).await;
        for i in 0..30u16 {
            engine.request_focus("client-1", &[i % 32]);
        }
        // Strip 40 (first extended input, Ch33) was never flooded: its whole
        // scoped set must drain before any flooded strip's leftovers.
        engine.request_focus("client-1", &[40]);
        let scoped = engine
            .poll_list("sim")
            .await
            .iter()
            .filter(|(c, _)| *c == 40)
            .count();
        assert!(scoped > 100, "strip 40 has the full input set");
        for _ in 0..scoped {
            engine.poll_once().await;
        }
        let mut chs: Vec<u16> = Vec::new();
        while let Ok(event) = client.events.try_recv() {
            if let Event::Params { ch, .. } = event {
                chs.push(ch);
            }
        }
        assert_eq!(chs.len(), scoped, "focus block is exactly strip 40's set");
        assert!(chs.iter().all(|&c| c == 40), "newest focus drains first: {chs:?}");
        // No duplicates from the flood: the queue holds one entry per
        // (strip, param) at most.
        let q = engine.focus_queue.read().unwrap();
        let mut seen = std::collections::HashSet::new();
        for (_, ch, param, _) in q.iter() {
            assert!(seen.insert((*ch, param.clone())), "duplicate ({ch}, {param})");
        }
    }

    #[tokio::test]
    async fn focus_queue_capped_and_detach_drops() {
        let engine = Engine::new(EngineConfig::default());
        let alice = engine.attach(None).await;
        let bob = engine.attach(None).await;
        // Repeated focus on every strip grows the queue, but it stays capped
        // (~4 sweeps) instead of growing without bound.
        for _ in 0..10 {
            for ch in 0..engine.driver_strips("sim") as u16 {
                engine.request_focus(&alice.id, &[ch]);
            }
        }
        let nstrips = engine.driver_strips("sim");
        let nids = engine.driver_capabilities("sim").len();
        assert!(engine.focus_queue.read().unwrap().len() <= 4 * nstrips * nids);
        // Bob's focus is queued…
        engine.request_focus(&bob.id, &[7]);
        assert!(
            engine.focus_queue.read().unwrap().iter().any(|(_, _, _, o)| o == &bob.id),
            "bob's focus queued"
        );
        let bob_id = bob.id.clone();
        // …and dropped with him when he detaches, while Alice's work stays.
        drop(bob);
        let q = engine.focus_queue.read().unwrap();
        assert!(
            q.iter().all(|(_, _, _, o)| o != &bob_id),
            "detached client's focus dropped"
        );
        assert!(
            q.iter().any(|(_, _, _, o)| o == &alice.id),
            "survivor's focus kept"
        );
    }
}
