//! [`Engine`]: the transport-agnostic mixer session, moved out of
//! `crates/serve` (ME1). All session behaviour lives here; adapters only
//! move [`Cmd`]/[`Event`](crate::protocol::Event) JSON.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::Future,
    net::IpAddr,
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
    params::{self, ParamRegistry},
    rta::RtaProcessor,
    show::{Scene, Show, DEFAULT_FADE_MS},
    MeterFrame, ParamValue, StripId,
};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::drivers::{
    driver_names, DynDriver, FxFlavor, RegisteredDriver,
};
use crate::protocol::{Cmd, DeviceDesc, Event, FxSlotState, OfflineChange, ParamDef, PatchEntry, Scene as SceneFrame, Show as ShowFrame};
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Link re-broadcast interval (R2): the `link` event goes out on every
/// change plus on this cadence with a fresh round-trip measurement.
pub const LINK_INTERVAL: Duration = Duration::from_secs(2);

/// R2 file-transfer upload cap (16 MiB): bounds one WS JSON frame's base64
/// payload so a bad client cannot blow the engine's memory.
pub const FILE_MAX_BYTES: usize = 16 << 20;

/// Validate a desk filename for `file_download` / `file_upload` (mirrors
/// the driver-side rules so bad names fail before touching the desk).
pub fn check_filename(name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 128 {
        anyhow::bail!("filename must be 1..=128 chars");
    }
    if name.contains(['/', '\\', '\0']) {
        anyhow::bail!("filename {name:?} must not contain path separators");
    }
    Ok(name.to_string())
}

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
    /// Routing snapshot (positional strip + stable key, group, 1-based index).
    pub routing: Vec<StoredRoute>,
}

/// One strip's contribution to a [`StoredScene`].
#[derive(Clone, Debug)]
pub struct StoredStrip {
    pub strip: u16,
    /// Stable strip key at capture (`ch/01`, …, `main`): `None` for scenes
    /// captured before keys existed. Recall resolves it against the active
    /// topology first and falls back to `strip`.
    pub key: Option<String>,
    pub name: String,
    pub fader: f32,
    pub mute: bool,
    pub params: HashMap<String, Value>,
}

/// One routing row in a [`StoredScene`]: the positional strip plus its
/// stable key at capture (desk-switch survival, same rule as
/// [`StoredStrip::key`]). The `group`/`index` are 1-based desk sockets.
#[derive(Clone, Debug)]
pub struct StoredRoute {
    pub strip: u16,
    pub key: Option<String>,
    pub group: String,
    pub index: u8,
}

/// Scoped recall: which strips / param groups / globals a `scene_recall`
/// applies (`None` scope = full plane, the historical behavior). The
/// groups mirror the desk Edit-Snapshots layout: Source, Input, Config,
/// EQ, Dyn, Fdr-Pan, Mute, per-bus sends, per-FX sends; the globals are
/// Main LR, the FX rack and routing. Unknown group/global names are
/// ignored (never fail a recall); an empty allowlist recalls nothing in
/// that axis.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct RecallScope {
    /// Strip allowlist (engine strip indices). `None` = all strips.
    #[serde(default)]
    pub strips: Option<Vec<u16>>,
    /// Stable strip-key allowlist (`ch/01`, …, `main`, `mix/..` legacy
    /// alias accepted): resolved against the active topology and unioned
    /// with `strips`. `None` = no key filter. Unknown keys are ignored
    /// (never fail a recall).
    #[serde(default)]
    pub strip_keys: Option<Vec<String>>,
    /// Param-group allowlist (see [`param_scope_group`]). `None` = all.
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    /// Global allowlist (`main`, `fx`, `routing`). `None` = all.
    #[serde(default)]
    pub globals: Option<Vec<String>>,
}

impl RecallScope {
    pub fn allows_strip(&self, strip: u16) -> bool {
        self.strips.as_ref().is_none_or(|s| s.contains(&strip))
    }
    /// Key-aware strip match against the active topology: union semantics —
    /// both lists absent means all strips; otherwise a strip matches when
    /// its index is in `strips` OR its canonical key (see
    /// [`openmix_core::strip_key_for_index`]) is in `strip_keys`. Unknown
    /// keys never match (they name strips from another desk, not a failure).
    pub fn matches_strip(
        &self,
        strip: u16,
        topology: &[openmix_core::StripFamily],
    ) -> bool {
        if self.strips.is_none() && self.strip_keys.is_none() {
            return true;
        }
        if self
            .strips
            .as_ref()
            .is_some_and(|s| s.contains(&strip))
        {
            return true;
        }
        let Some(keys) = self.strip_keys.as_ref() else {
            return false;
        };
        openmix_core::strip_key_for_index(topology, strip as usize).is_some_and(|canon| {
            keys.iter().any(|want| {
                openmix_core::parse_strip_key(want).is_some_and(|(wf, wi)| {
                    openmix_core::parse_strip_key(&canon)
                        .is_some_and(|(cf, ci)| wf == cf && wi == ci)
                })
            })
        })
    }
    pub fn allows_group(&self, group: &str) -> bool {
        self.groups.as_ref().is_none_or(|g| g.iter().any(|x| x == group))
    }
    pub fn allows_param(&self, id: &str) -> bool {
        self.allows_group(param_scope_group(id))
    }
    pub fn allows_global(&self, global: &str) -> bool {
        self.globals.as_ref().is_none_or(|g| g.iter().any(|x| x == global))
    }
}

/// Catalogue param id -> recall scope group (the desk Edit-Snapshots
/// columns). `fader`/`pan` ride Fdr-Pan, `mute` rides Mute, `name`/
/// `link`/`color` ride Config; send ids split by slot number (1..6 bus,
/// 7..10 FX) across all send suffixes (`level`, `tap`, `pan`, `on`);
/// anything unrecognized falls back to Config (never silently dropped
/// from a partial recall — Config off excludes it like the rest).
pub fn param_scope_group(id: &str) -> &'static str {
    if id == "fader" || id == "pan" {
        return "fader_pan";
    }
    if id == "mute" {
        return "mute";
    }
    if id == "source" {
        return "source";
    }
    if matches!(
        id,
        "gain" | "trim" | "phantom" | "polarity" | "delay_on" | "delay_ms"
    ) {
        return "input";
    }
    if id == "link" || id == "color" || id == "name" {
        return "config";
    }
    if id.starts_with("eq_") || id.starts_with("hpf_") || id == "eq_on" || id == "hpf_on" {
        return "eq";
    }
    if id.starts_with("gate_") || id.starts_with("comp_") || id.starts_with("dyn_") {
        return "dyn";
    }
    if let Some(n) = send_slot_number(id) {
        return if n <= 6 { "sends_bus" } else { "sends_fx" };
    }
    "config"
}

/// Numeric send slot behind `auxN_*` / `fxN_*` / `sendN_*` ids (`None`
/// when the id is not a send leaf).
fn send_slot_number(id: &str) -> Option<u8> {
    let rest = id
        .strip_prefix("aux")
        .or_else(|| id.strip_prefix("fx"))
        .or_else(|| id.strip_prefix("send"))?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || !rest[digits.len()..].starts_with('_') {
        return None;
    }
    digits.parse::<u8>().ok()
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

/// Production LAN scanner: broadcast probes for every row of the
/// transport [`PROBE_TABLE`](openmix_transport::discovery::PROBE_TABLE)
/// via `crates/transport`. UNVERIFIED-ON-RIG: mapping replies to devices
/// has only ever run against the fake in tests.
pub struct RealScanner;

impl DeviceScanner for RealScanner {
    fn scan(
        &self,
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + '_>> {
        Box::pin(async move {
            let mut out = Vec::new();
            if let Ok(devs) = openmix_transport::discovery::discover_all(timeout).await {
                for d in devs {
                    let ip = d.addr.ip().to_string();
                    out.push(DeviceDesc {
                        id: format!("{}-{ip}", d.driver),
                        name: match d.driver {
                            "xair" => format!("X-Air {}", d.model),
                            _ => format!("{} {}", d.driver, d.model),
                        },
                        model: d.model,
                        ip,
                        kind: "console".to_string(),
                        state: "available".to_string(),
                        fw: None,
                        via: Some("broadcast".to_string()),
                    });
                }
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

/// Cached visible-group lane lists: (driver, visible set, pairs).
/// Rebuilt when the visible set changes (focus is infrequent).
type PollVisibleCache = Option<(String, Vec<u16>, Vec<(u16, &'static str)>)>;

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
/// Erased dispatch over the driver registry ([`DynDriver`], one entry per
/// provider in `drivers`). Owns the driver bank + active selection, the
/// strip state and param-plane caches, the poller, the meter/spectrum/GR
/// ticker, the show library, locks, per-client policies and the roster.
/// Background tasks (poller, lock sweeper, meter ticker) start on the first
/// [`attach`](Engine::attach) and exit when the last `Arc<Engine>` is
/// dropped.
pub struct Engine {
    drivers: HashMap<String, RegisteredDriver>,
    /// Registry definition order (sim first, then hardware): device-claim
    /// precedence and deterministic iteration.
    driver_order: Vec<String>,
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
    /// Merged param registries per driver (canonical catalogue + that
    /// provider's [`DynDriver::param_defs`](crate::drivers::DynDriver::param_defs)
    /// rows, built once at boot). Every caps / poll / snapshot / recall
    /// path resolves through these instead of the global static, so a
    /// mixer with unique DSP arrives without editing core. Registries never
    /// change at runtime (topology × caps × scopes are fixed per driver),
    /// so no lock is needed.
    registries: HashMap<String, ParamRegistry>,
    /// Cached visible-group lane lists: (driver, visible set, pairs).
    /// Rebuilt when the visible set changes (focus is infrequent).
    poll_visible: RwLock<PollVisibleCache>,    /// Engine construction time: source of the monotonic `t_ms` clock in
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
    /// Which driver holds an active hardware meter subscription (`None`
    /// until the first hardware connect kicks one). The xair
    /// `subscribe_meters` installs the blob-order cache behind
    /// `poll_meters`; without this kick the meter ticker would fan out
    /// silence on hardware (the sim synthesizes its own levels).
    meter_driver: RwLock<Option<String>>,
    /// Guard receiver for the hardware meter stream: held so the broadcast
    /// stays alive for the process lifetime (the driver's cache drain
    /// holds its own resubscribed clone).
    meter_guard: RwLock<Option<tokio::sync::broadcast::Receiver<openmix_core::MeterFrame>>>,
    /// Last connected address per driver (the `addr` of [`Engine::connect`]):
    /// broadcast discovery never crosses subnets, so a hardware `scan` also
    /// verifies this address and lists the linked desk when it answers.
    desk_addrs: RwLock<HashMap<String, String>>,
}

impl Engine {
    /// Build a new session. No background work starts here (safe to call
    /// outside a runtime); the poller/sweeper/meter tasks spawn on the
    /// first [`attach`](Engine::attach).
    pub fn new(config: EngineConfig) -> Arc<Self> {
        let data_dir = config.data_dir.clone();
        let scenes_lib = load_scenes(data_dir.as_deref());
        // Registry definition order is the device-claim precedence
        // (sim fallback first, then hardware in priority order).
        let rows = crate::drivers::registered_drivers(config.preset);
        let driver_order: Vec<String> = rows.iter().map(|r| r.name.to_string()).collect();
        // Merged param registries (canonical + provider rows) ride the same
        // definition order; unknown drivers fall back to `sim` (see
        // `Engine::registry`), which is always registered.
        let registries: HashMap<String, ParamRegistry> = rows
            .iter()
            .map(|r| (r.name.to_string(), r.param_registry()))
            .collect();
        let drivers: HashMap<String, RegisteredDriver> =
            rows.into_iter().map(|r| (r.name.to_string(), r)).collect();
        let engine = Arc::new(Self {
            drivers,
            driver_order,
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
            registries,
            boot: Instant::now(),
            focus_queue: RwLock::new(VecDeque::new()),
            lock_ttl: config.lock_ttl,
            poll_interval: config.poll_interval,
            tasks_started: AtomicBool::new(false),
            devices: RwLock::new(Vec::new()),
            scanned_at: AtomicU64::new(0),
            link: RwLock::new(LinkState::default()),
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
            meter_driver: RwLock::new(None),
            meter_guard: RwLock::new(None),
            desk_addrs: RwLock::new(HashMap::new()),
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

    /// Erased driver for `name` (falls back to `sim`, which is always
    /// registered — `known` gates every path that reaches here).
    fn drv(&self, name: &str) -> Arc<dyn DynDriver> {
        self.drivers
            .get(name)
            .map(|r| r.driver.clone())
            .unwrap_or_else(|| {
                self.drivers
                    .get("sim")
                    .expect("sim is always registered")
                    .driver
                    .clone()
            })
    }

    /// Erased driver behind the current selection.
    fn active_drv(&self) -> Arc<dyn DynDriver> {
        let active = self.active_driver();
        self.drv(&active)
    }

    /// Sorted registry keys for `unknown driver` diagnostics.
    fn expected_drivers(&self) -> String {
        driver_names(&self.drivers).join("|")
    }

    fn known(&self, driver: &str) -> bool {
        self.drivers.contains_key(driver)
    }

    fn default_addr(&self, driver: &str) -> &str {
        self.drivers
            .get(driver)
            .map(|r| r.default_addr)
            .unwrap_or("192.168.1.1")
    }

    /// Select `driver` and connect it. Failures leave the old driver
    /// active; callers (adapters) stay up so a client can retry.
    pub async fn connect(&self, driver: &str, addr: Option<&str>) -> anyhow::Result<()> {
        if !self.known(driver) {
            anyhow::bail!("unknown driver {driver:?}; expected {}", self.expected_drivers());
        }
        let target = addr
            .map(str::to_string)
            .unwrap_or_else(|| self.default_addr(driver).to_string());
        // Remember the desk address: hardware scans verify it (broadcast
        // discovery never crosses subnets).
        self.desk_addrs
            .write()
            .unwrap()
            .insert(driver.to_string(), target.clone());
        let drv = self.drv(driver);
        drv.connect(&target).await?;
        *self.active.write().unwrap() = driver.to_string();
        // The FX cache mirrors the active driver (sim rows / xair
        // placeholders); refresh it on every switch, including the boot
        // connect — otherwise a hardware boot keeps the sim-seeded rows
        // and the UI shows a fake rack.
        self.refresh_fx_cache().await;
        // Hardware meter streams need an explicit subscribe (sim
        // synthesizes its own levels via `poll_meters`). Best-effort:
        // a failed subscribe leaves the ticker on silence, never fails
        // the connect. One subscription per driver (the driver's renewal
        // loop keeps it alive across reconnects of the same driver).
        if drv.wants_meter_subscribe()
            && self.meter_driver.read().unwrap().as_deref() != Some(driver)
        {
            match drv.subscribe_meters().await {
                Ok(rx) => {
                    *self.meter_guard.write().unwrap() = Some(rx);
                    *self.meter_driver.write().unwrap() = Some(driver.to_string());
                }
                Err(e) => {
                    eprintln!("engine: {driver} meter subscribe failed (meters stay silent): {e:#}");
                }
            }
        }
        Ok(())
    }

    /// Attach a client: allocate its id, register it, and queue the
    /// initial snapshot (`you`, `state`, `shows`, `clients`, `locks`,
    /// `caps`, `devices`, `link`, `info`?, `scenes`, `fx`, `routing`,
    /// master-section frames (`bus`/`subgroup`/`mute_group` + sparse
    /// assigns + `main` + sparse `solo`), then one `params` frame per
    /// cached channel) into the new
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
        // Master-section snapshot: bus/subgroup/mute-group masters +
        // memberships + Main LR (fresh pages learn existing assignments
        // without a write; sparse solos where the driver serves them).
        let masters = self.masters_snapshot().await;

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
        // (devices, link, info?, scenes, fx, routing, masters) before params.
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
            for frame in masters {
                let _ = entry.mailbox.send(frame);
            }
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
        // R2 desk-broadcast ingest: drivers that publish desk-originated
        // mix-surface updates (another client's moves, echoed to every
        // subscriber) get one task each — apply them to the param cache
        // live as `params` diffs, no poll-sweep wait. While another driver
        // is active, a driver's updates never touch the cache.
        for name in self.driver_order.clone() {
            let rx = match self.drv(&name).subscribe_updates() {
                Some(rx) => rx,
                None => continue,
            };
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut rx = rx;
                loop {
                    let update = match rx.recv().await {
                        Ok(u) => u,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    if engine.active_driver() != name {
                        continue;
                    }
                    engine.apply_inbound(update.strip.0, &update.param, &update.value);
                }
            });
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
        self.active_drv().poll_meters().levels
    }

    /// Gate/comp GR (dB reduction `>= 0`) for one 0-based channel.
    ///
    /// Drivers with a DYN blob cache read the `gate_gr`/`comp_gr` param
    /// path; no data yet (or out of range) falls back to 0.0 so the tick
    /// never drops a frame. Drivers without a GR source always read
    /// (0.0, 0.0).
    async fn gr_db(&self, ch: usize) -> (f32, f32) {
        let drv = self.active_drv();
        if !drv.dyn_gr() {
            return (0.0, 0.0);
        }
        let Ok(strip) = u16::try_from(ch) else {
            return (0.0, 0.0);
        };
        let id = StripId(strip);
        let gate = match drv.get_param(id, "gate_gr").await {
            Ok(ParamValue::Float(v)) => v.max(0.0),
            _ => 0.0,
        };
        let comp = match drv.get_param(id, "comp_gr").await {
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
    /// [`DynDriver::capabilities`]).
    fn driver_capabilities(&self, driver: &str) -> Vec<&'static str> {
        self.drv(driver).capabilities().to_vec()
    }

    /// Merged param registry of the named driver (canonical catalogue +
    /// that provider's contributed rows). Unknown drivers fall back to
    /// `sim`, mirroring [`Engine::drv`] — `sim` is always registered.
    fn registry(&self, driver: &str) -> &ParamRegistry {
        self.registries.get(driver).unwrap_or_else(|| {
            self.registries
                .get("sim")
                .expect("sim is always registered")
        })
    }

    /// Channel-strip count of the named driver (legacy `channel_count`;
    /// poll/attach geometry now uses [`Engine::driver_strips`]).
    #[allow(dead_code)]
    fn driver_channels(&self, driver: &str) -> usize {
        self.drv(driver).channel_count()
    }

    /// Total addressable strips of the named driver (topology total).
    fn driver_strips(&self, driver: &str) -> usize {
        self.drv(driver).strip_count()
    }

    /// Ordered strip families of the named driver (`caps.topology`).
    fn driver_topology(&self, driver: &str) -> Vec<openmix_core::StripFamily> {
        self.drv(driver).topology()
    }

    /// Stable key for one strip on the named driver (`None` off-topology).
    fn strip_key(&self, driver: &str, strip: u16) -> Option<String> {
        openmix_core::strip_key_for_index(&self.driver_topology(driver), strip as usize)
    }

    /// Stable key for one strip on the ACTIVE driver (`None` off-topology).
    fn active_strip_key(&self, strip: u16) -> Option<String> {
        let driver = self.active_driver();
        self.strip_key(&driver, strip)
    }

    /// Resolve a strip-targeted command's `ch` + optional `key` against the
    /// active topology: a resolvable key wins (desk-switch survival —
    /// profiles replay keys, not positions); otherwise `ch` is validated in
    /// range. An unresolvable key is an honest error naming the key.
    fn resolve_strip(&self, ch: u16, key: Option<&str>) -> anyhow::Result<u16> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
            if let Some(idx) = openmix_core::strip_index_for_key(&topo, k) {
                let max = u16::try_from(self.driver_strips(&driver)).unwrap_or(u16::MAX);
                if (idx as u16) < max {
                    return Ok(idx as u16);
                }
            }
            anyhow::bail!("unknown strip key {k:?} on {driver}");
        }
        if (ch as usize) < self.driver_strips(&driver) {
            Ok(ch)
        } else {
            anyhow::bail!("strip {ch} out of range 0..{}", self.driver_strips(&driver))
        }
    }

    /// Resolve a `focus` strip list mixing indices and stable keys
    /// (`chs` arrives as `Vec<Value>` so keys ride the same frame).
    fn resolve_focus_list(&self, items: &[serde_json::Value]) -> anyhow::Result<Vec<u16>> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        let n = self.driver_strips(&driver);
        let mut out = Vec::new();
        for item in items {
            if let Some(k) = item.as_str() {
                let idx = openmix_core::strip_index_for_key(&topo, k)
                    .ok_or_else(|| anyhow::anyhow!("focus: unknown strip key {k:?} on {driver}"))?;
                if idx >= n {
                    anyhow::bail!("focus: strip key {k:?} out of range 0..{n}");
                }
                out.push(idx as u16);
            } else if let Some(v) = item.as_u64() {
                let v: u16 = v
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("focus channel {v} out of range"))?;
                out.push(v);
            } else {
                anyhow::bail!("focus \"chs\" entries must be integers or strip keys, got {item}");
            }
        }
        Ok(out)
    }

    /// Declared meter taps of the named driver (`caps.meter_taps`).
    fn driver_meter_taps(&self, driver: &str) -> Vec<String> {
        self.drv(driver)
            .meter_taps()
            .iter()
            .map(|s| s.to_string())
            .collect()
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
                let model = self
                    .drv(&driver)
                    .query_info()
                    .await
                    .map(|i| i.model)
                    .unwrap_or_else(|_| driver.clone());
                (model, None)
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
    /// (old fields byte-identical). Both sides resolve through the
    /// driver's merged [`ParamRegistry`](params::ParamRegistry), so
    /// provider-contributed rows ride `caps` once the driver lists them in
    /// its capabilities.
    fn caps_message(&self) -> Event {
        let driver = self.active_driver();
        let supported = self.driver_capabilities(&driver);
        let reg = self.registry(&driver);
        let dropped = self.param_dropped.read().unwrap();
        let live: Vec<&&str> = supported
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .collect();
        let params: Vec<ParamDef> = live
            .iter()
            .filter_map(|id| reg.find(id).map(param_def_frame))
            .collect();
        let mut unsupported: Vec<String> = reg
            .iter()
            .map(|d| d.id.to_string())
            .filter(|id| {
                !supported.contains(&id.as_str())
                    || dropped.contains(&(driver.clone(), id.clone()))
            })
            .collect();
        unsupported.sort();
        let topo_fams = self.driver_topology(&driver);
        let topology = topo_fams
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
                strip_keys: (f.first..f.first + f.count)
                    .filter_map(|i| openmix_core::strip_key_for_index(&topo_fams, i))
                    .collect(),
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
            groups: self.groups_caps(&driver),
            files: self.files_caps(&driver),
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
                    via: d.via.clone(),
                }
            })
            .collect();
        Event::Devices {
            list,
            scanned_at: self.scanned_at.load(Ordering::Relaxed),
        }
    }

    /// Verified device entry for the relay's linked desk (hardware drivers
    /// only): the stored connect address answers the driver's own info
    /// query. `None` when never connected, unconfigured, or unreachable —
    /// a scan then honestly reports only what broadcast discovery found.
    async fn linked_desk_device(&self, driver: &str) -> Option<DeviceDesc> {
        if !self.drv(driver).is_hardware() {
            return None;
        }
        let ip = self.desk_addrs.read().unwrap().get(driver).cloned()?;
        let info = self.drv(driver).query_info().await.ok()?;
        if info.model.trim().is_empty() {
            return None;
        }
        let name = if info.name.trim().is_empty() {
            info.model.clone()
        } else {
            info.name.clone()
        };
        Some(DeviceDesc {
            id: format!("{driver}-{ip}"),
            name,
            model: info.model,
            ip,
            kind: "console".to_string(),
            state: "available".to_string(),
            fw: if info.firmware.trim().is_empty() {
                None
            } else {
                Some(info.firmware)
            },
            via: Some("linked".to_string()),
        })
    }

    /// Directed verify of one desk address WITHOUT switching the link:
    /// unicast `/xinfo` (+ legacy `/info` fallback) at `addr`. Returns the
    /// device row (`via: "manual"`) when the desk answers, `None` on
    /// silence. Broadcast discovery never crosses subnets; this is the
    /// manual-add and cross-subnet path (read-only on the desk).
    async fn probe_desk(&self, driver: &str, addr: &str) -> Option<DeviceDesc> {
        use openmix_transport::discovery::{probe_addr, PROBE_TABLE};
        let raw = addr.trim().to_string();
        // Accept `ip`, `ip:port`, or a hostname; resolve to one IP.
        let ip: IpAddr = raw
            .parse()
            .ok()
            .or_else(|| {
                raw.rsplit_once(':')
                    .and_then(|(h, _)| h.trim_matches(|c| c == '[' || c == ']').parse().ok())
            })
            .or_else(|| {
                std::net::ToSocketAddrs::to_socket_addrs(
                    format!("{raw}:{}", openmix_transport::discovery::XAIR_PORT).as_str(),
                )
                .ok()?
                .next()
                .map(|s| s.ip())
            })?;
        let mut answered_model: Option<String> = None;
        for spec in PROBE_TABLE.iter().filter(|s| s.driver == driver) {
            if let Ok(devs) = probe_addr(spec, ip, Duration::from_millis(900)).await {
                if let Some(d) = devs.into_iter().next() {
                    answered_model = Some(d.model);
                    break;
                }
            }
        }
        let model = answered_model?;
        let ip_s = ip.to_string();
        Some(DeviceDesc {
            id: format!("{driver}-{ip_s}"),
            name: format!("X-Air {model}"),
            model,
            ip: ip_s,
            kind: "console".to_string(),
            state: "available".to_string(),
            fw: None,
            via: Some("manual".to_string()),
        })
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
        if self.active_drv().is_hardware() {
            let t0 = Instant::now();
            let target = self.active_drv().get_main_fader().await;
            match target {
                Ok(_) => t0.elapsed().as_secs_f32() * 1000.0,
                Err(_) => last,
            }
        } else {
            1.0 + ((self.now_ms() / 100) % 5) as f32 * 0.1
        }
    }

    /// Desk identity block for `info` (connect + on request). Errors when
    /// the driver cannot answer (hardware drivers while unconnected).
    async fn info_message(&self) -> anyhow::Result<Event> {
        let driver = self.active_driver();
        let info = self.drv(&driver).query_info().await?;
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
        let reg = self.registry(&driver);
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
                    let scoped = reg
                        .find(id)
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
                    key: openmix_core::strip_key_for_index(&topo, s),
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
    /// Scoped recall: `scope` restricts the recall to strips / param
    /// groups / globals (`None` = full plane, the historical behavior).
    /// Axes outside the scope keep their live values: the fade targets
    /// use current desk reads for excluded fader/mute axes (best-effort;
    /// a failed read falls back to the scene value), excluded params are
    /// skipped, and FX/routing/main apply only under their global.
    async fn recall_full(
        &self,
        scene: &StoredScene,
        fade_ms: u64,
        scope: Option<&RecallScope>,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Vec<u16>> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        let resolve = |s: &StoredStrip| -> u16 {
            s.key
                .as_deref()
                .and_then(|k| openmix_core::strip_index_for_key(&topo, k))
                .map(|i| i as u16)
                .unwrap_or(s.strip)
        };
        // Strips whose key resolves on this desk recall at the resolved
        // index (desk-switch survival); key-less rows and unresolvable keys
        // fall back to the positional strip (back-compat). Scope matching
        // runs on the resolved index (`strip_keys` name the same strips).
        let strips: Vec<(&StoredStrip, u16)> = scene
            .strips
            .iter()
            .map(|s| (s, resolve(s)))
            .filter(|(_, r)| scope.is_none_or(|sc| sc.matches_strip(*r, &topo)))
            .collect();
        // Faders ride the single shared fade (legacy order: pre-unmute
        // targets, ramp, snap, target mutes); params/FX/routing/main follow.
        let mut targets: Vec<(StripId, f32, bool)> = Vec::with_capacity(strips.len());
        for (s, r) in &strips {
            let id = StripId(*r);
            let want_fader = scope.is_none_or(|sc| sc.allows_param("fader"));
            let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
            let (cur_fader, cur_mute) = if !want_fader || !want_mute {
                self.current_fader_mute(id).await
            } else {
                (s.fader, s.mute)
            };
            targets.push((id, if want_fader { s.fader } else { cur_fader }, if want_mute { s.mute } else { cur_mute }));
        }
        openmix_core::show::fade_strips(self, &targets, fade_ms).await?;
        for (s, r) in &strips {
            let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
            if want_mute && s.mute {
                self.drv_set_mute(StripId(*r), true).await?;
            }
            let key = openmix_core::strip_key_for_index(&topo, *r as usize);
            if scope.is_none_or(|sc| sc.allows_param("fader")) {
                out.push(Event::Param {
                    ch: *r,
                    field: "fader".to_string(),
                    value: json!(s.fader),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                });
            }
            if want_mute {
                out.push(Event::Param {
                    ch: *r,
                    field: "mute".to_string(),
                    value: json!(s.mute),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                });
            }
        }
        // Remaining params in safe order (names ride along unless the
        // scope excludes them).
        for (s, r) in &strips {
            let mut ids: Vec<&String> = s.params.keys().collect();
            ids.sort_by_key(|id| safe_order_key(id));
            for id in ids {
                if scope.is_some_and(|sc| !sc.allows_param(id)) {
                    continue;
                }
                let value = s.params.get(id).expect("key just listed");
                let pv = json_to_param_value(value)?;
                // Best-effort per param on hardware (a desk that went
                // silent mid-recall must not abort the rest); strict on
                // the model.
                let strict = !self.active_drv().is_hardware();
                let rr = self.drv_set_param(StripId(*r), id, pv).await;
                if strict {
                    rr?;
                }
                out.push(Event::Param {
                    ch: *r,
                    field: (*id).clone(),
                    value: value.clone(),
                    by: Some(writer.to_string()),
                    key: openmix_core::strip_key_for_index(&topo, *r as usize),
                });
            }
            if s.name.is_empty() || scope.is_some_and(|sc| !sc.allows_param("name")) {
                continue;
            }
            let rr = self
                .drv_set_param(StripId(*r), "name", ParamValue::Text(s.name.clone()))
                .await;
            if !self.active_drv().is_hardware() {
                rr?;
            }
        }
        // FX + routing snapshots (model-strict, best-effort elsewhere),
        // gated on their globals; Main LR on its own global.
        if scope.is_none_or(|sc| sc.allows_global("fx")) {
            self.recall_fx(&scene.fx).await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("routing")) {
            self.recall_routing(&scene.routing).await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("main")) {
            if let Some(m) = scene.main_fader {
                self.drv_set_main(m).await?;
            }
        }
        self.refresh_fx_cache().await;
        let mut overrode: Vec<u16> = Vec::new();
        for (_, r) in &strips {
            if let Some(holder) = self.lock_holder(*r) {
                if holder != writer && !overrode.contains(r) {
                    overrode.push(*r);
                }
            }
        }
        overrode.sort_unstable();
        Ok(overrode)
    }

    /// Current fader + mute of one strip (best-effort; failures read back
    /// subscriber silence so a scoped recall degrades to a no-op axis
    /// rather than failing the whole recall).
    async fn current_fader_mute(&self, strip: StripId) -> (f32, bool) {
        let drv = self.active_drv();
        let fader = drv.get_fader(strip).await.ok().unwrap_or(0.0);
        let mute = drv.get_mute(strip).await.ok().unwrap_or(false);
        (fader, mute)
    }

    // ================= R2: FX =================

    /// `caps.fx` for the named driver: slot count + type tables (design
    /// racks carry the six design tables from `core::fx`; the X-Air
    /// desk-native rack carries the 60 documented algorithms from
    /// `proto-xair::fx` with labeled `parNN` params; rack-less drivers
    /// carry none — see README matrix).
    fn fx_caps(&self, driver: &str) -> crate::protocol::FxCaps {
        let drv = self.drv(driver);
        let types = if drv.fx_flavor() == FxFlavor::Design {
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
        } else if driver == "xair" {
            xair_fx_caps()
        } else {
            Vec::new()
        };
        crate::protocol::FxCaps {
            slots: drv.fx_slot_count(),
            types,
        }
    }

    /// Current `fx` event from the engine-side cache + return levels.
    async fn fx_message(&self) -> Event {
        let levels = self.meter_levels().await;
        let design = self.active_drv().fx_flavor() == FxFlavor::Design;
        let slots = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .map(|s| {
                let level = if design {
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

    /// Synchronous FX cache rebuild for construction (the default driver
    /// is always sim here).
    fn refresh_fx_cache_sync(&self) {
        let rows: Vec<FxSlotRt> = self
            .drv("sim")
            .fx_snapshot()
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

    /// Read FX rows from the active driver (design racks fully mirrored;
    /// the X-Air desk-native rack reads the live type + used params per
    /// slot over the verified `/fx/N/type` + `/fx/N/par/MM` path —
    /// best-effort, unreadable slots stay honest placeholders; rack-less
    /// drivers keep placeholders).
    async fn read_fx_rows(&self) -> Vec<FxSlotRt> {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Design {
            return drv
                .fx_snapshot()
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
        }
        if drv.fx_flavor() == FxFlavor::Numbered && self.active_driver() == "xair" {
            let mut rows = Vec::new();
            for slot in 1..=drv.fx_slot_count() as u8 {
                rows.push(self.read_xair_slot(slot).await);
            }
            return rows;
        }
        // Rack-less drivers are not polled (no verified state
        // path — writes are fire-and-forget there). Placeholder rows stay
        // placeholders: never inherit sim-seeded rows (or any unverified
        // state) as `prev` — the UI renders "—" honestly.
        (1..=drv.fx_slot_count() as u8)
            .map(|slot| FxSlotRt {
                slot,
                fx_type: "—".to_string(),
                on: true,
                bypass: false,
                params: Default::default(),
            })
            .collect()
    }

    /// Best-effort live read of one X-Air FX slot: type first (unknown on
    /// error), then each used param of the loaded effect (`parNN` keys).
    /// On/off has no verified leaf and stays at the honest placeholder
    /// (`on: true`, `bypass: false`) so the UI keeps the toggle hidden.
    async fn read_xair_slot(&self, slot: u8) -> FxSlotRt {
        let drv = self.active_drv();
        let name = drv
            .fx_type_index(slot)
            .await
            .and_then(xair_type_name)
            .unwrap_or("—")
            .to_string();
        let mut params = BTreeMap::new();
        if let Some(index) = xair_type_index_of(&name) {
            let used = openmix_proto_xair::fx::fx_param_count(index).unwrap_or(0);
            for i in 0..used {
                if let Ok(v) = drv.get_fx_param(slot, i).await {
                    params.insert(format!("par{:02}", i + 1), v);
                }
            }
        }
        FxSlotRt {
            slot,
            fx_type: name,
            on: true,
            bypass: false,
            params,
        }
    }

    /// One FX poll read (round-robin over the design rack, or one live
    /// X-Air slot param per tick; silent unless a value changed). Other
    /// numbered/rack-less racks are not polled (no verified state path —
    /// writes are fire-and-forget there).
    async fn fx_refresh_one(&self) {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Numbered && self.active_driver() == "xair" {
            self.fx_refresh_one_xair().await;
            return;
        }
        if drv.fx_flavor() != FxFlavor::Design {
            return;
        }
        let slots = drv.fx_snapshot();
        if slots.is_empty() {
            return;
        }
        let i = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize % slots.len();
        let slot = &slots[i];
        let changed = {
            let mut cache = self.fx_cache.write().unwrap();
            let Some(row) = cache.iter_mut().find(|r| r.slot == (i + 1) as u8) else {
                return;
            };
            let fresh = param_vec_to_map(&slot.fx_type, &slot.params);
            let diff = row.fx_type != slot.fx_type
                || row.on != slot.on
                || row.bypass != slot.bypass
                || row.params != fresh;
            if diff {
                row.fx_type = slot.fx_type.clone();
                row.on = slot.on;
                row.bypass = slot.bypass;
                row.params = fresh;
            }
            diff
        };
        if changed {
            self.broadcast(&self.fx_message().await);
        }
    }

    /// One X-Air poll tick: a single live read (type when the cached row
    /// is still a placeholder, else one rotating param of the loaded
    /// effect). One query per tick keeps LAN load flat; desk-side moves
    /// converge within a few ticks and broadcasts only on change.
    async fn fx_refresh_one_xair(&self) {
        let count = self.active_drv().fx_slot_count();
        if count == 0 {
            return;
        }
        let n = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize;
        let slot = (n % count) as u8 + 1;
        let drv = self.active_drv();
        // Placeholder row: read the type (one query); params fill in on
        // later ticks once the effect (and its used count) is known.
        let cached_type = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .find(|r| r.slot == slot)
            .map(|r| r.fx_type.clone())
            .unwrap_or_default();
        if xair_type_index_of(&cached_type).is_none() {
            let fresh = self.read_xair_slot(slot).await;
            let changed = {
                let mut cache = self.fx_cache.write().unwrap();
                match cache.iter_mut().find(|r| r.slot == slot) {
                    Some(row) => {
                        let diff =
                            row.fx_type != fresh.fx_type || row.params != fresh.params;
                        if diff {
                            row.fx_type = fresh.fx_type.clone();
                            row.params = fresh.params.clone();
                        }
                        diff
                    }
                    None => false,
                }
            };
            if changed {
                self.broadcast(&self.fx_message().await);
            }
            return;
        }
        let index = match xair_type_index_of(&cached_type) {
            Some(i) => i,
            None => return,
        };
        let used = openmix_proto_xair::fx::fx_param_count(index).unwrap_or(0);
        if used == 0 {
            return;
        }
        let idx = ((n / count) % used as usize) as u8;
        let Ok(v) = drv.get_fx_param(slot, idx).await else {
            return;
        };
        let key = format!("par{:02}", idx + 1);
        let changed = {
            let mut cache = self.fx_cache.write().unwrap();
            match cache.iter_mut().find(|r| r.slot == slot) {
                Some(row) => {
                    let diff = row.params.get(&key).copied() != Some(v);
                    if diff {
                        row.params.insert(key, v);
                    }
                    diff
                }
                None => false,
            }
        };
        if changed {
            self.broadcast(&self.fx_message().await);
        }
    }

    /// Recall FX snapshot rows (design-strict, best-effort elsewhere).
    async fn recall_fx(&self, fx: &[StoredFx]) -> anyhow::Result<()> {
        let strict = self.active_drv().fx_flavor() == FxFlavor::Design;
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
        self.active_drv()
            .fx_apply_row(row.slot, &row.fx_type, row.on, row.bypass, &row.params)
            .await
    }

    // ================= R2: routing =================

    /// `caps.routing` for the named driver (design racks fully supported;
    /// elsewhere unsupported).
    fn routing_caps(&self, driver: &str) -> crate::protocol::RoutingCaps {
        let groups: Vec<crate::protocol::RoutingGroupCaps> = self
            .drv(driver)
            .routing_groups()
            .iter()
            .map(|g| {
                // `RoutingGroup` fields project 1:1 onto the wire caps.
                crate::protocol::RoutingGroupCaps {
                    key: g.key.to_string(),
                    label: g.label.to_string(),
                    count: g.count,
                }
            })
            .collect();
        let supported = !groups.is_empty();
        crate::protocol::RoutingCaps {
            groups,
            per_strip: supported,
            supported,
        }
    }

    /// R2 `caps.groups`: master-section counts of the named driver (bus /
    /// subgroup / mute-group masters behind the new commands; zero =
    /// honest `unsupported`). Assign-leaf support rides alongside so the
    /// UI collapses assign toggles where the desk has masters only.
    fn groups_caps(&self, driver: &str) -> crate::protocol::GroupsCaps {
        let drv = self.drv(driver);
        let (mg_assign, sub_assign) = drv.group_assign_caps();
        crate::protocol::GroupsCaps {
            buses: drv.bus_count(),
            subgroups: drv.subgroup_count(),
            mute_groups: drv.mute_group_count(),
            mg_assign,
            sub_assign,
        }
    }

    /// R2 `caps.files`: desk file-transfer support of the named driver.
    fn files_caps(&self, driver: &str) -> crate::protocol::FilesCaps {
        let supported = self.drv(driver).file_transfer();
        crate::protocol::FilesCaps { supported }
    }

    /// Full-state `bus` echo: both master fields re-read after the write.
    async fn bus_message(&self, bus: u8) -> anyhow::Result<Event> {
        let drv = self.active_drv();
        let (fader, muted) = (drv.get_bus_fader(bus).await?, drv.get_bus_mute(bus).await?);
        Ok(Event::Bus {
            bus,
            fader,
            muted,
            key: Some(openmix_core::strip_key("bus", bus.saturating_add(1) as u16)),
        })
    }

    /// Full-state `subgroup` echo (drivers with subgroup masters in
    /// memory or native).
    async fn subgroup_message(&self, group: u8) -> anyhow::Result<Event> {
        let drv = self.active_drv();
        let (fader, muted) = (
            drv.get_subgroup_fader(group).await?,
            drv.get_subgroup_mute(group).await?,
        );
        Ok(Event::Subgroup {
            group,
            fader,
            muted,
            key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
        })
    }

    /// Full-state `main` echo: Main LR fader + mute re-read from the
    /// active driver (`None` fields where the driver has no master).
    async fn main_message(&self) -> Event {
        let drv = self.active_drv();
        let (fader, muted) = (
            drv.get_main_fader().await.ok(),
            drv.get_main_mute().await.ok(),
        );
        Event::Main { fader, muted }
    }

    /// Master-section + membership snapshot for attach/post_switch: every
    /// bus/subgroup/mute-group master re-read live, every known membership
    /// (sparse — members only), Main LR, and sparse solos. Fresh pages
    /// learn existing assignments without a write (gap: the engine used to
    /// broadcast these only as write echoes). Assign/solo reads are
    /// driver-gated: sim/dl32s serve assigns from memory/cache (xair has
    /// no assign leaf at all); sim solos come from memory while xair
    /// re-reads the live solo bank per channel strip. All reads are
    /// best-effort: a failed re-read drops that frame instead of failing
    /// the snapshot.
    async fn masters_snapshot(&self) -> Vec<Event> {
        let mut out = Vec::new();
        let driver = self.active_driver();
        let drv = self.drv(&driver);
        let (buses, subgroups, mute_groups) = (
            drv.bus_count(),
            drv.subgroup_count(),
            drv.mute_group_count(),
        );
        for bus in 0..buses {
            let msg = match (drv.get_bus_fader(bus).await, drv.get_bus_mute(bus).await) {
                (Ok(fader), Ok(muted)) => Some(Event::Bus {
                    bus,
                    fader,
                    muted,
                    key: Some(openmix_core::strip_key("bus", bus.saturating_add(1) as u16)),
                }),
                _ => None,
            };
            if let Some(m) = msg {
                out.push(m);
            }
        }
        for group in 0..subgroups {
            let msg = match (
                drv.get_subgroup_fader(group).await,
                drv.get_subgroup_mute(group).await,
            ) {
                (Ok(fader), Ok(muted)) => Some(Event::Subgroup {
                    group,
                    fader,
                    muted,
                    key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                }),
                _ => None,
            };
            if let Some(m) = msg {
                out.push(m);
            }
        }
        for group in 0..mute_groups {
            let muted = drv.get_mute_group(group).await.ok();
            if let Some(muted) = muted {
                out.push(Event::MuteGroup {
                    group,
                    muted,
                    key: Some(format!("mgroup/{:02}", group.saturating_add(1))),
                });
            }
        }
        // Membership: sparse (members only). Reads are memory/cache-speed
        // on drivers that serve the assign leaf; elsewhere the default
        // reads honestly fail and no members are reported.
        if drv.group_assign_caps() != (false, false) {
            let topo = self.driver_topology(&driver);
            let strips = self.driver_strips(&driver);
            for ch in 0..strips as u16 {
                let id = StripId(ch);
                let key = openmix_core::strip_key_for_index(&topo, ch as usize);
                for group in 0..subgroups {
                    let member = drv.get_subgroup_assign(id, group).await.unwrap_or(false);
                    if member {
                        out.push(Event::SubgroupAssign { ch, group, member, key: key.clone() });
                    }
                }
                for group in 0..mute_groups {
                    let member = drv.get_mute_group_assign(id, group).await.unwrap_or(false);
                    if member {
                        out.push(Event::MuteGroupAssign { ch, group, member, key: key.clone() });
                    }
                }
            }
            // Sparse solos served from driver memory (sim in-memory;
            // dl32s solos ride echoes, so no rows here).
            if !drv.is_hardware() {
                for ch in 0..strips as u16 {
                    if drv.get_solo(StripId(ch)).await.unwrap_or(false) {
                        out.push(Event::Solo {
                            ch,
                            soloed: true,
                            key: openmix_core::strip_key_for_index(&topo, ch as usize),
                        });
                    }
                }
            }
        }
        // Sparse desk-side solos: best-effort live bank re-read (drivers
        // whose solos live in the desk) so a fresh attach (or driver
        // switch) learns them without waiting for the poll sweep. Cached
        // alongside (the poll diff then stays quiet); a silent desk yields
        // no frames, never fails the snapshot.
        if drv.live_solo_bank() {
            let topo = self.driver_topology(&driver);
            for ch in 0..drv.channel_count() as u16 {
                if drv.get_solo(StripId(ch)).await.unwrap_or(false) {
                    {
                        let mut cache = self.param_cache.write().unwrap();
                        cache_store(&mut cache, ch, "solo", json!(true));
                    }
                    out.push(Event::Solo {
                        ch,
                        soloed: true,
                        key: openmix_core::strip_key_for_index(&topo, ch as usize),
                    });
                }
            }
        }
        out.push(self.main_message().await);
        out
    }

    /// Current `routing` event: one row per patched input strip (sim reads
    /// the `source` param plane; elsewhere the list is empty).
    async fn routing_message(&self) -> Event {
        Event::Routing {
            patches: self.routing_snapshot().await.into_iter().map(|r| {
                PatchEntry {
                    strip: r.strip,
                    key: r.key.clone(),
                    label: format!("{} {:02}", r.group, r.index),
                    group: r.group,
                    index: r.index,
                }
            }).collect(),
        }
    }

    /// (strip + stable key, group, 1-based index) rows for patched inputs.
    async fn routing_snapshot(&self) -> Vec<StoredRoute> {
        let drv = self.active_drv();
        if drv.routing_groups().is_empty() {
            return Vec::new();
        }
        let topo = drv.topology();
        let mut out = Vec::new();
        for fam in &topo {
            if fam.kind != openmix_core::StripKind::In {
                continue;
            }
            for s in fam.first..fam.first + fam.count {
                let Ok(pv) = drv.get_param(StripId(s as u16), "source").await else {
                    continue;
                };
                if let ParamValue::Text(label) = pv {
                    if let Some((group, index)) = parse_source(&label) {
                        out.push(StoredRoute {
                            strip: s as u16,
                            key: openmix_core::strip_key_for_index(&topo, s),
                            group,
                            index,
                        });
                    }
                }
            }
        }
        out
    }

    /// Recall routing snapshot rows (routing-capable drivers only;
    /// skipped elsewhere). Rows resolve their stable key first (desk-switch
    /// survival) and fall back to the positional strip.
    async fn recall_routing(&self, routing: &[StoredRoute]) -> anyhow::Result<()> {
        let drv = self.active_drv();
        if drv.routing_groups().is_empty() {
            return Ok(());
        }
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        for row in routing {
            let strip = row
                .key
                .as_deref()
                .and_then(|k| openmix_core::strip_index_for_key(&topo, k))
                .map(|i| i as u16)
                .unwrap_or(row.strip);
            drv.patch_set(StripId(strip), &row.group, row.index)?;
        }
        Ok(())
    }

    // ================= R2: meter taps =================

    /// Per-tap level vectors for the active driver, in
    /// `caps.meter_taps` order filtered to known taps. Drivers with a tap
    /// source serve all their taps; hardware serves `post` from the meter
    /// blob cache and GR from the DYN cache (`in` unavailable — no
    /// `/meters/0` subscribe); tap-less drivers serve `post` only. Every
    /// entry is best-effort: a tap with no source reads back silence,
    /// never an error.
    async fn tap_frames(&self) -> Vec<(String, Vec<f32>)> {
        let drv = self.active_drv();
        if let Some(taps) = drv.poll_taps() {
            return vec![
                ("in".to_string(), taps.iter().map(|t| t.input).collect()),
                ("post".to_string(), taps.iter().map(|t| t.post).collect()),
                ("gr_gate".to_string(), taps.iter().map(|t| t.gr_gate_db).collect()),
                ("gr_comp".to_string(), taps.iter().map(|t| t.gr_comp_db).collect()),
            ];
        }
        let post = drv.poll_meters().levels;
        let mut out = vec![("post".to_string(), post)];
        if drv.dyn_gr() {
            let n = out[0].1.len().max(1);
            let mut gr_gate = vec![0.0f32; n];
            let mut gr_comp = vec![0.0f32; n];
            for (i, (g, c)) in gr_gate.iter_mut().zip(gr_comp.iter_mut()).enumerate() {
                let (gate, comp) = self.gr_db(i).await;
                *g = gate;
                *c = comp;
            }
            out.push(("gr_gate".to_string(), gr_gate));
            out.push(("gr_comp".to_string(), gr_comp));
        }
        out
    }

    // ================= R2: offline editing =================

    /// Record a param-plane write into the offline change list (no-op
    /// unless offline editing is active). Same (strip, id) keeps its
    /// original `from` and takes the newest `to`; the stable key is
    /// captured alongside for desk-switch survival.
    async fn offline_note(&self, strip: u16, id: &str, from: Value, to: Value) {
        let key = self.active_strip_key(strip);
        let mut off = self.offline.write().unwrap();
        if !off.active {
            return;
        }
        match off.changes.iter_mut().find(|c| c.strip == strip && c.id == id) {
            Some(e) => {
                e.to = to;
                e.key = key;
            }
            None => off.changes.push(OfflineChange {
                strip,
                key,
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
    /// Design-strict, best-effort elsewhere; returns the applied Param echoes.
    async fn offline_push_apply(&self, writer: &str, out: &mut Vec<Event>) -> anyhow::Result<()> {
        let changes = self.offline.read().unwrap().changes.clone();
        let strict = self.active_drv().fx_flavor() == FxFlavor::Design;
        let mut failures = Vec::new();
        for c in &changes {
            let r = self.offline_apply_one(c).await;
            if let Err(e) = r {
                failures.push(format!("strip {} {}: {e:#}", c.strip, c.id));
                continue;
            }
            // Echo the resolved strip (keys may have moved the write).
            let ch = c
                .key
                .as_deref()
                .and_then(|k| {
                    let topo = self.driver_topology(&self.active_driver());
                    openmix_core::strip_index_for_key(&topo, k)
                })
                .map(|i| i as u16)
                .unwrap_or(c.strip);
            out.push(Event::Param {
                ch,
                field: c.id.clone(),
                value: c.to.clone(),
                by: Some(writer.to_string()),
                key: self.active_strip_key(ch),
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

    /// Apply one recorded offline change to the active driver. Strip keys
    /// resolve first (desk-switch survival); the positional strip is the
    /// fallback for key-less rows.
    async fn offline_apply_one(&self, c: &OfflineChange) -> anyhow::Result<()> {
        if c.id == "main_fader" {
            let Value::Number(n) = &c.to else {
                anyhow::bail!("main_fader push value must be a number")
            };
            let v = n.as_f64().unwrap_or(0.0) as f32;
            return self.drv_set_main(v).await;
        }
        let strip = c
            .key
            .as_deref()
            .and_then(|k| {
                let topo = self.driver_topology(&self.active_driver());
                openmix_core::strip_index_for_key(&topo, k)
            })
            .map(|i| i as u16)
            .unwrap_or(c.strip);
        if c.id == "patch" {
            let drv = self.active_drv();
            if drv.routing_groups().is_empty() {
                anyhow::bail!("patch push not supported here");
            }
            let Value::String(label) = &c.to else {
                anyhow::bail!("patch push value must be a label")
            };
            if label == "—" {
                return drv.patch_clear(StripId(strip));
            }
            let Some((group, index)) = parse_source(label) else {
                anyhow::bail!("unparseable patch label {label:?}")
            };
            drv.patch_set(StripId(strip), &group, index)?;
            return Ok(());
        }
        if let Some(param) = c.id.strip_prefix("fx/") {
            return self.active_drv().fx_push(c.strip as u8, param, &c.to).await;
        }
        let pv = json_to_param_value(&c.to)?;
        self.drv_set_param(StripId(strip), &c.id, pv).await
    }

    // ================= R2: driver write helpers =================

    async fn drv_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.active_drv().set_fader(strip, value).await
    }

    async fn drv_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.active_drv().get_fader(strip).await
    }

    async fn drv_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.active_drv().set_mute(strip, muted).await
    }

    async fn drv_set_param(
        &self,
        strip: StripId,
        id: &str,
        pv: ParamValue,
    ) -> anyhow::Result<()> {
        self.active_drv().set_param(strip, id, pv).await
    }

    async fn drv_set_main(&self, value: f32) -> anyhow::Result<()> {
        self.active_drv().set_main_fader(value).await
    }

    // ================= R2: desk-broadcast ingest =================

    /// Apply one desk-originated update (broadcast ingest): same
    /// compare-and-broadcast as a poll read, minus the I/O.
    fn apply_inbound(&self, ch: u16, param: &str, pv: &ParamValue) {
        // Solo bypasses the param plane: desk broadcasts (the `solo`
        // id decoded by `parse_inbound`), poll reads (the `read_param`
        // bank route), and the `Solo` command echo all converge on
        // `Event::Solo` — the channel the UI solos view renders. The
        // param cache still diffs (polls stay quiet, attach snapshots
        // carry solo state) but no `Params` frame goes out for it.
        if param == "solo" {
            if let ParamValue::Bool(soloed) = pv {
                let v = param_value_json(pv);
                let changed = {
                    let mut cache = self.param_cache.write().unwrap();
                    cache_store(&mut cache, ch, param, v.clone())
                };
                if changed {
                    self.broadcast(&Event::Solo {
                        ch,
                        soloed: *soloed,
                        key: self.active_strip_key(ch),
                    });
                }
            }
            return;
        }
        let v = param_value_json(pv);
        let changed = {
            let mut cache = self.param_cache.write().unwrap();
            cache_store(&mut cache, ch, param, v.clone())
        };
        if changed {
            self.broadcast(&Event::Params {
                ch,
                values: [(param.to_string(), v)].into_iter().collect(),
                key: self.active_strip_key(ch),
            });
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
        let reg = self.registry(driver);
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
                let scoped = reg
                    .find(id)
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
            let bg_share = if t.is_multiple_of(2) { (n / 4).max(1).min(remaining) } else { 0 };
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
            if let Ok((i, ch, param, r)) = joined { ordered[i] = Some((ch, param, r)) }
        }
        for slot in ordered.into_iter().flatten() {
            let (ch, param, r) = slot;
            match r {
                Ok(pv) => {
                    let v = param_value_json(&pv);
                    let key = self.strip_key(&driver, ch);
                    let changed = {
                        let mut cache = self.param_cache.write().unwrap();
                        cache_store(&mut cache, ch, &param, v.clone())
                    };
                    if changed {
                        self.broadcast(&Event::Params {
                            ch,
                            values: [(param, v)].into_iter().collect(),
                            key,
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
        if self.fx_cursor.fetch_add(1, Ordering::Relaxed).is_multiple_of(FX_POLL_EVERY) {
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
        // Master-section snapshot: fresh pages (and driver switches) learn
        // existing masters/assignments without a write.
        out.extend(self.masters_snapshot().await);
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
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
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
                    key: openmix_core::strip_key_for_index(&topo, ch as usize),
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
        let reg = self.registry(driver);
        let mut out = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                for id in &caps {
                    let scoped = reg
                        .find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    out.push((s as u16, *id));
                }
                // Solo rides the poll set as its own id (never a catalogue
                // param — the solosw bank has no per-strip query address,
                // so it can never enter `caps`): xair channel strips
                // re-read the bank via `get_solo` (`read_param` routes
                // it). Appended to the strip's own block, so the list
                // stays strip-major. This is the desk→app backstop for
                // solos the live ingest never saw (attach gaps, dropped
                // broadcasts); the ingest is the fast path.
                if driver == "xair" && fam.kind == openmix_core::StripKind::In {
                    out.push((s as u16, "solo"));
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
    /// Drain order is core-first across the whole focused set: pass 1 queues
    /// `fader`+`mute` of every strip (a 16-strip mixer bank is fader-live
    /// after ~32 ticks ≈ 1.6 s at the 20 q/s budget instead of waiting out
    /// a full strip-major catalogue sweep), pass 2 `name`+`pan`, pass 3 the
    /// remaining scoped ids in catalogue order. A single-strip focus still
    /// drains as one unbroken block ahead of the background sweep.
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
        let reg = self.registry(&driver);
        let kind_of = |ch: u16| {
            topo.iter()
                .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                .map(|f| f.kind)
        };
        let mut q = self.focus_queue.write().unwrap();
        q.retain(|(d, c, _, _)| d != &driver || !chs.contains(c));
        // Core-first drain order (three push waves; each wave pushes its
        // strips back-to-front so the queue head ends up in `chs` order):
        // fader+mute for every strip, then name+pan, then the rest.
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
                    !is_core_id(id, true)
                        && !is_core_id(id, false)
                        && reg
                            .find(id)
                            .map(|def| params::supports_kind(def, kind))
                            .unwrap_or(true)
                })
                .collect();
            for id in scoped.iter().rev() {
                q.push_front((driver.clone(), ch, (*id).clone(), writer.to_string()));
            }
        }
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in ["pan", "name"].iter().rev() {
                if !ids.iter().any(|s| s == id) {
                    continue;
                }
                if !reg
                    .find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true)
                {
                    continue;
                }
                q.push_front((driver.clone(), ch, id.to_string(), writer.to_string()));
            }
        }
        for &ch in chs.iter().rev() {
            if (ch as usize) >= nstrips {
                continue;
            }
            let Some(kind) = kind_of(ch) else {
                continue;
            };
            for id in ["mute", "fader"].iter().rev() {
                if !ids.iter().any(|s| s == id) {
                    continue;
                }
                if !reg
                    .find(id)
                    .map(|def| params::supports_kind(def, kind))
                    .unwrap_or(true)
                {
                    continue;
                }
                q.push_front((driver.clone(), ch, id.to_string(), writer.to_string()));
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
        // Solo is a poll-set id, not a catalogue param (no `get_param`
        // arm serves it): drivers with solo state answer the bank read
        // directly; the rest stay honestly unsupported.
        let drv = self.drv(driver);
        if param == "solo" {
            return drv.get_solo(ch).await.map(ParamValue::Bool);
        }
        drv.get_param(ch, param).await
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
            Ok(pv) => self.apply_inbound(ch, param, &pv),
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
    ///
    /// The desk applies writes with a delay of tens of ms: an immediate
    /// re-read usually returns the PRE-write value (owner-rig measured on
    /// XR18 FW 1.20 — every clip of `fader` echo read back the previous
    /// value). A re-read that disagrees with what was just written is
    /// therefore treated as that stale pre-image: it is NOT cached (it
    /// would regress the broadcast-confirmed value) and the caller echoes
    /// the written value instead; the desk's own change broadcast confirms
    /// within ~100 ms. Floats agree within the desk's ~10-bit quantization
    /// step; bools/text must match exactly.
    async fn refresh_param(&self, ch: StripId, param: &str, written: &Value) -> Option<Value> {
        let driver = self.active_driver();
        let pv = self.read_param(&driver, ch, param).await.ok()?;
        let v = param_value_json(&pv);
        if !refresh_agrees(&v, written) {
            return None;
        }
        {
            let mut cache = self.param_cache.write().unwrap();
            cache_store(&mut cache, ch.0, param, v.clone());
        }
        Some(v)
    }

    /// Snapshot the active driver (used by `capture_scene`).
    async fn active_snapshot(&self) -> anyhow::Result<openmix_core::MixerSnapshot> {
        self.active_drv().snapshot().await
    }

    /// Main fader of the active driver, `None` when the driver has none.
    async fn active_main_fader(&self) -> Option<f32> {
        self.active_drv().get_main_fader().await.ok()
    }

    /// Recall `scene` on the active driver, ramping over `fade_ms`.
    async fn recall_on_active(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        self.active_drv().recall_show(scene, fade_ms).await
    }

    /// Full `state` event for the active driver, optionally tagged with the
    /// mutation writer (`by` is `None` for snapshots).
    async fn state_message(&self, by: Option<&str>) -> Event {
        let name = self.active_driver();
        let strips = snapshot_strips(&*self.drv(&name)).await;
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
                let ip = addr.unwrap_or_else(|| self.default_addr(&driver).to_string());
                self.set_link_connected(None, &ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Fader { ch, value, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_fader(StripId(ch), value)?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                let from = self.offline_from(StripId(ch), "fader").await;
                self.drv_set_fader(StripId(ch), value).await?;
                if let Some(from) = from {
                    self.offline_note(ch, "fader", from, json!(value)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo (same as the `param` arm): the state
                // frame alone cannot confirm the write on drivers whose
                // snapshot is empty (xair), so re-read and broadcast what
                // the desk reports, falling back to the written value
                // (including when the re-read races the desk's apply lag —
                // see `refresh_param`).
                let confirmed = self
                    .refresh_param(StripId(ch), "fader", &json!(value))
                    .await
                    .unwrap_or(json!(value));
                out.push(Event::Param {
                    ch,
                    field: "fader".to_string(),
                    value: confirmed,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Mute { ch, muted, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_mute(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                let from = self.offline_from(StripId(ch), "mute").await;
                self.drv_set_mute(StripId(ch), muted).await?;
                if let Some(from) = from {
                    self.offline_note(ch, "mute", from, json!(muted)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo (same rationale as `fader` above).
                let confirmed = self
                    .refresh_param(StripId(ch), "mute", &json!(muted))
                    .await
                    .unwrap_or(json!(muted));
                out.push(Event::Param {
                    ch,
                    field: "mute".to_string(),
                    value: confirmed,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                });
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
                self.drv_set_main(value).await?;
                if let Some(from) = from {
                    self.offline_note(u16::MAX, "main_fader", from, json!(value)).await;
                }
                self.note_write(writer);
                self.touch_holder_locks(writer);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // R2 Main LR master mute (all drivers via `get_main_mute`;
            // unsupported drivers bail honestly). Master gate: the main
            // master is a shared mix like the bus masters.
            Cmd::MainMute { muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_main_mute(muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.main_message().await);
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // R2 per-strip solo (monitor bus only). Channel-scoped lock +
            // strip gate (no FOH impact, but a locked strip is someone's).
            Cmd::Solo { ch, soloed, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_strip(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                self.active_drv().set_solo(StripId(ch), soloed).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Echo the commanded state (house pattern, same as mute:
                // the re-read races the desk's apply lag, so a disagreeing
                // read falls back to the written value instead of flashing
                // a phantom unsoloed. The write path itself is rig-verified:
                // `/-stat/solosw/NN` 1-based sets the bank bit, `/-action/
                // clearsolo` clears it — XR18 FW 1.20, 2026-10-18.)
                //
                // Cache alongside the echo (see `apply_inbound`): the poll
                // diff then stays quiet instead of re-emitting this state.
                {
                    let mut cache = self.param_cache.write().unwrap();
                    cache_store(&mut cache, ch, "solo", json!(soloed));
                }
                out.push(Event::Solo {
                    ch,
                    soloed,
                    key: self.active_strip_key(ch),
                });
                Ok(None)
            }
            Cmd::Param { ch, field, value, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
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
                self.drv_set_param(StripId(ch), &field, pv).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Confirmed-value echo: re-read the param just written and
                // broadcast what the driver reports (wire quantization may
                // shift it); fall back to the written value when the
                // re-read fails — or when it races the desk's apply lag
                // (see `refresh_param`) — so the echo still lands.
                let confirmed = self
                    .refresh_param(StripId(ch), &field, &value)
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
                    key: self.active_strip_key(ch),
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::Focus { ch, chs } => {
                // Poll-priority hint: either `{"cmd":"focus","ch":N}` or
                // `{"cmd":"focus","chs":[...]}`. Entries may be indices or
                // stable strip keys (`ch/01`, …, `main`; `mix/..` accepted).
                // Valid channels move to the front of the poll queue
                // (re-read at the normal budget, then round-robin resumes);
                // unknown/out-of-range channels are ignored with an `error`
                // event — never a disconnect.
                let mut want: Vec<u16> = Vec::new();
                match (ch, chs) {
                    (_, Some(list)) => {
                        want = self.resolve_focus_list(&list)?;
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
            Cmd::Lock { ch, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
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
            Cmd::Unlock { ch, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
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
                        key: None,
                    });
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "mute".to_string(),
                        value: json!(s.mute),
                        by: Some(writer.to_string()),
                        key: None,
                    });
                    for (field, val) in &s.params {
                        out.push(Event::Param {
                            ch: s.strip.0,
                            field: field.clone(),
                            value: param_value_json(val),
                            by: Some(writer.to_string()),
                            key: None,
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
                // Real LAN discovery behind the trait (fake in tests),
                // on EVERY driver: a sim-backed relay has no desks of its
                // own, so scanning from sim must run the same broadcast
                // discovery the phone uses — never the design seed rows.
                // Broadcast never crosses subnets, so the relay's linked
                // desk address is verified too (unicast round-trip via
                // the driver's own info query). The list is REPLACED:
                // an empty result updates scanned_at and honestly
                // reports no desks — stale entries are never kept.
                let mut merged = self.scanner.scan(Duration::from_secs(2)).await;
                let driver = self.active_driver();
                if let Some(linked) = self.linked_desk_device(&driver).await {
                    if !merged.iter().any(|d| d.ip == linked.ip) {
                        merged.push(linked);
                    }
                }
                *self.devices.write().unwrap() = merged;
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
                // Discovered devices dial their claiming driver
                // (UNVERIFIED-ON-RIG for dl32s); anything else re-links
                // the local sim model (dev fixture ids, old rows).
                let mut claimed = "sim";
                for name in &self.driver_order {
                    if self.drv(name).claims_device(&id, &entry.model) {
                        claimed = name.as_str();
                        break;
                    }
                }
                self.connect(claimed, Some(&entry.ip)).await?;
                self.set_link_connected(Some(&id), &entry.ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Probe { addr } => {
                // Manual add-by-IP: verify WITHOUT switching the link.
                // The embedded app boots on sim and the user has never
                // switched drivers, so sim probes with the aliased probe
                // table (the only broadcast table so far); drivers with
                // their own table probe natively. A driver with no probe
                // table is an honest error, never a blind row.
                let driver = self.active_driver();
                let Some(table_driver) = self.drv(&driver).probe_table() else {
                    anyhow::bail!(
                        "probe needs the xair driver (relay runs {driver}) — switch drivers first"
                    );
                };
                match self.probe_desk(table_driver, &addr).await {
                    Some(found) => {
                        {
                            let mut devs = self.devices.write().unwrap();
                            // Never duplicate: a row for this IP already
                            // answers from broadcast or a prior probe.
                            if !devs.iter().any(|d| d.ip == found.ip) {
                                devs.push(found);
                            }
                        }
                        out.push(self.devices_message());
                        Ok(None)
                    }
                    None => anyhow::bail!(
                        "no reply from {addr} — check the IP (desks on another subnet never answer broadcast; unicast to this address got nothing back)"
                    ),
                }
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
            Cmd::SceneRecall { n, fade_ms, scope } => {
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
                let overrode = self.recall_full(&scene, fade_ms, scope.as_ref(), writer, out).await?;
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
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                let fx_type = fx_type.trim().to_string();
                openmix_core::fx::fx_type_meta(&fx_type)
                    .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
                if drv.fx_flavor() != FxFlavor::Design {
                    anyhow::bail!(
                        "{driver}: fx type select not supported (desk-native types are read-only here; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = drv.fx_get_type(slot).unwrap_or_default();
                drv.fx_set_type(slot, &fx_type)?;
                self.offline_note(slot as u16, "fx/type", json!(from), json!(fx_type.clone())).await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::FxParam { slot, id, value } => {
                policy.allow_recall()?;
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    anyhow::bail!("fx param {value} out of range 0.0..=1.0");
                }
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() == FxFlavor::Design {
                    let from = drv
                        .fx_get_param_by_id(slot, &id)
                        .map(|v| json!(v))
                        .unwrap_or(Value::Null);
                    drv.fx_set_param_by_id(slot, &id, value)?;
                    self.offline_note(slot as u16, &format!("fx/{id}"), from, json!(value)).await;
                    self.refresh_fx_cache().await;
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.fx_message().await);
                    Ok(None)
                } else {
                    drv.fx_set_param(slot, &id, value).await?;
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.fx_message().await);
                    Ok(None)
                }
            }
            Cmd::FxBypass { slot, bypass } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() != FxFlavor::Design {
                    anyhow::bail!(
                        "{driver}: fx bypass not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = self
                    .fx_cache
                    .read()
                    .unwrap()
                    .iter()
                    .find(|r| r.slot == slot)
                    .map(|r| r.bypass)
                    .unwrap_or(!bypass);
                drv.fx_set_bypass(slot, bypass)?;
                self.offline_note(slot as u16, "fx/bypass", json!(from), json!(bypass)).await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::FxOn { slot, on } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() != FxFlavor::Design {
                    anyhow::bail!(
                        "{driver}: fx on/off not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = self
                    .fx_cache
                    .read()
                    .unwrap()
                    .iter()
                    .find(|r| r.slot == slot)
                    .map(|r| r.on)
                    .unwrap_or(!on);
                drv.fx_set_on(slot, on)?;
                self.offline_note(slot as u16, "fx/on", json!(from), json!(on)).await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::PatchSet { strip, group, index, key } => {
                let strip = self.resolve_strip(strip, key.as_deref())?;
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                let drv = self.active_drv();
                let mut from = Value::Null;
                if !drv.routing_groups().is_empty() {
                    from = drv
                        .get_param(StripId(strip), "source")
                        .await
                        .map(|pv| param_value_json(&pv))
                        .unwrap_or(Value::Null);
                }
                let label = drv.patch_set(StripId(strip), &group, index)?;
                self.offline_note(strip, "patch", from, json!(label)).await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.routing_message().await);
                Ok(None)
            }
            Cmd::PatchClear { strip, key } => {
                let strip = self.resolve_strip(strip, key.as_deref())?;
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                let drv = self.active_drv();
                let mut from = Value::Null;
                if !drv.routing_groups().is_empty() {
                    from = drv
                        .get_param(StripId(strip), "source")
                        .await
                        .map(|pv| param_value_json(&pv))
                        .unwrap_or(Value::Null);
                }
                drv.patch_clear(StripId(strip))?;
                self.offline_note(strip, "patch", from, json!("—")).await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.routing_message().await);
                Ok(None)
            }
            Cmd::Offline { model } => {
                if !self.known(&model) {
                    anyhow::bail!("unknown driver {model:?}; expected {}", self.expected_drivers());
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
            // R2 mix-bus masters (0-based `bus` over `0..bus_count()`).
            Cmd::BusFader { bus, value } => {
                let value = openmix_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                self.active_drv().set_bus_fader(bus, value).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.bus_message(bus).await?);
                Ok(None)
            }
            Cmd::BusMute { bus, muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_bus_mute(bus, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.bus_message(bus).await?);
                Ok(None)
            }
            // R2 subgroup masters (0-based `group` over `0..subgroup_count()`).
            Cmd::SubgroupFader { group, value } => {
                let value = openmix_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                self.active_drv().set_subgroup_fader(group, value).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.subgroup_message(group).await?);
                Ok(None)
            }
            Cmd::SubgroupMute { group, muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_subgroup_mute(group, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.subgroup_message(group).await?);
                Ok(None)
            }
            // R2 group membership (channel-scoped lock + master gate).
            Cmd::SubgroupAssign { ch, group, member, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_assign(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                self.active_drv().set_subgroup_assign(StripId(ch), group, member).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(Event::SubgroupAssign { ch, group, member, key: self.active_strip_key(ch) });
                Ok(None)
            }
            Cmd::MuteGroup { group, muted } => {
                policy.allow_master_mute()?;
                self.active_drv().set_mute_group(group, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                let muted = self.active_drv().get_mute_group(group).await?;
                out.push(Event::MuteGroup {
                    group,
                    muted,
                    key: Some(format!("mgroup/{:02}", group.saturating_add(1))),
                });
                Ok(None)
            }
            Cmd::MuteGroupAssign { ch, group, member, key } => {
                let ch = self.resolve_strip(ch, key.as_deref())?;
                policy.allow_assign(StripId(ch))?;
                if let Some(holder) = self.lock_holder(ch) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(ch, &holder)));
                    }
                }
                self.active_drv().set_mute_group_assign(StripId(ch), group, member).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(Event::MuteGroupAssign { ch, group, member, key: self.active_strip_key(ch) });
                Ok(None)
            }
            // R2 desk file transfer (drivers with a file plane natively,
            // honestly unsupported elsewhere).
            Cmd::FileDownload { filename } => {
                let filename = check_filename(&filename)?;
                let bytes = self.active_drv().download_file(&filename).await?;
                self.note_write(writer);
                out.push(Event::File {
                    filename,
                    size: bytes.len(),
                    file_type: 0,
                    data: Some(STANDARD.encode(&bytes)),
                });
                Ok(None)
            }
            Cmd::FileUpload { filename, data, file_type } => {
                policy.allow_upload()?;
                let filename = check_filename(&filename)?;
                let bytes = STANDARD.decode(data.trim()).map_err(|e| anyhow::anyhow!("invalid base64 data: {e}"))?;
                if bytes.len() > FILE_MAX_BYTES {
                    anyhow::bail!("file {filename:?} too large ({} > {FILE_MAX_BYTES} bytes)", bytes.len());
                }
                self.active_drv().upload_file(&filename, &bytes, file_type).await?;
                self.note_write(writer);
                out.push(Event::File {
                    filename,
                    size: bytes.len(),
                    file_type,
                    data: None,
                });
                Ok(None)
            }
            // R2 desk fire-and-forget action: clear all solos (monitoring-side
            // only). No desk echo — the desk ACKs by silence — so fan out
            // explicit unsoloed frames for every known-soloed strip and drop
            // them from the cache; otherwise the UI would stick on soloed.
            // (A surface clear arrives the same way, via `apply_inbound`
            // solosw broadcasts.)
            Cmd::ClearSolo => {
                self.active_drv().clear_solo().await?;
                let cleared: Vec<u16> = {
                    let mut cache = self.param_cache.write().unwrap();
                    let mut cleared = Vec::new();
                    for (ch, map) in cache.iter_mut() {
                        if map.get("solo") == Some(&json!(true)) {
                            cleared.push(*ch);
                            map.remove("solo");
                        }
                    }
                    cleared
                };
                for ch in cleared {
                    out.push(Event::Solo {
                        ch,
                        soloed: false,
                        key: self.active_strip_key(ch),
                    });
                }
                self.note_write(writer);
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

async fn snapshot_strips(d: &dyn DynDriver) -> Vec<crate::protocol::StateStrip> {
    let topo = d.topology();
    match d.snapshot().await {
        Ok(snap) => snap
            .strips
            .iter()
            .map(|s| crate::protocol::StateStrip {
                ch: s.id.0,
                key: openmix_core::strip_key_for_index(&topo, s.id.0 as usize),
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

/// Post-write re-read/write agreement (see `refresh_param`): floats agree
/// within the desk's ~10-bit quantization step (2e-3 covers the measured
/// 1e-3 readback epsilon plus f32 noise), bools/text must match exactly.
/// A disagreeing re-read is the desk's stale pre-image, not a rejection.
fn refresh_agrees(read: &Value, written: &Value) -> bool {
    match (read, written) {
        (Value::Number(a), Value::Number(b)) => {
            match (a.as_f64(), b.as_f64()) {
                (Some(a), Some(b)) => (a - b).abs() <= 2e-3,
                _ => false,
            }
        }
        _ => read == written,
    }
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

/// Desk-native FX type tables for `caps.fx` on `xair`: the 60 documented
/// algorithms from `proto-xair::fx` (index 15 is an undocumented gap in
/// the wire order and stays out, like the `None` fallback there).
///
/// Param ids are the verified wire numbers (`par01..par64`); labels are
/// the X-Air display names with the dual-block bank (`A`/`B`) appended
/// and a `?` suffix on research-low-confidence (`inferred`) rows. Ranges
/// are normalized (`0..1`, the desk's wire domain for FX pars —
/// bit-exact, no fader epsilon); the display ranges in the notes are
/// strings (`1k`, `20k`) that do not fit the numeric caps shape, so the
/// UI formats normalized values as percent and shows `unit` alongside.
fn xair_fx_caps() -> Vec<crate::protocol::FxTypeCaps> {
    use openmix_proto_xair::fx as xfx;
    let mut out = Vec::new();
    for index in 0..=xfx::FX_TYPE_MAX {
        let Some(name) = xfx::fx_effect_name(index) else {
            continue;
        };
        let mut params = Vec::new();
        for par in 1..=xfx::PARAM_COUNT as u8 {
            let Some(def) = xfx::fx_param_def(index, par) else {
                continue;
            };
            let mut label = def.label.to_string();
            if let Some(bank) = def.bank {
                label.push(' ');
                label.push_str(bank);
            }
            if def.inferred {
                label.push('?');
            }
            params.push(crate::protocol::FxParamCaps {
                id: format!("par{par:02}"),
                label,
                unit: def.unit.to_string(),
                min: 0.0,
                max: 1.0,
                default: 0.5,
                scale: "lin".to_string(),
            });
        }
        out.push(crate::protocol::FxTypeCaps {
            id: name.to_string(),
            label: name.to_string(),
            params,
        });
    }
    out
}

/// Desk-native effect name for a `/fx/N/type` index (`None` = gap/unknown).
fn xair_type_name(index: u8) -> Option<&'static str> {
    openmix_proto_xair::fx::fx_effect_name(index)
}

/// Reverse lookup: effect name back to its `/fx/N/type` index (`None`
/// for placeholders like `—` or unknown names).
fn xair_type_index_of(name: &str) -> Option<u8> {
    (0..=openmix_proto_xair::fx::FX_TYPE_MAX)
        .find(|&index| openmix_proto_xair::fx::fx_effect_name(index) == Some(name))
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

/// R2 focus core ids: the mixer strip essentials (`fader`+`mute` first
/// wave, `name`+`pan` second — see [`Engine::request_focus`]). `first`
/// selects the fader/mute wave; `false` the name/pan wave.
fn is_core_id(id: &str, first: bool) -> bool {
    if first {
        id == "fader" || id == "mute"
    } else {
        id == "name" || id == "pan"
    }
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
                key: st.get("key").and_then(Value::as_str).map(str::to_string),
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
            routing.push(StoredRoute {
                strip,
                key: r.get("key").and_then(Value::as_str).map(str::to_string),
                group,
                index,
            });
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
                    "key": st.key,
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
                "routing": s.routing.iter().map(|r| json!({
                    "strip": r.strip,
                    "key": r.key,
                    "group": r.group,
                    "index": r.index,
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
                // Legacy named shows are index-based (core snapshots carry
                // no topology); the numbered-scene plane carries keys.
                key: None,
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

    #[test]
    fn probe_cmd_parses() {
        let cmd: Cmd =
            serde_json::from_value(json!({"cmd": "probe", "addr": "192.168.50.123"})).unwrap();
        assert!(matches!(cmd, Cmd::Probe { .. }));
        assert!(Cmd::known_tags().contains(&"probe"));
    }

    #[test]
    fn device_via_omitted_when_absent() {
        // Additive wire change: rows without a source serialize exactly
        // like before (golden devices.json keeps passing unmodified).
        let d = DeviceDesc {
            id: "x".to_string(),
            name: "X".to_string(),
            model: "M".to_string(),
            ip: "1.2.3.4".to_string(),
            kind: "console".to_string(),
            state: "available".to_string(),
            fw: None,
            via: None,
        };
        let v = serde_json::to_value(&d).unwrap();
        assert!(v.get("via").is_none());
        assert!(v.get("fw").is_none());
    }

    #[tokio::test]
    async fn probe_rejects_unresolvable_addr() {
        let engine = Engine::new(EngineConfig::default());
        // Garbage never resolves: silence, never a blind row, and the
        // stored device list is untouched.
        let before = engine.devices.read().unwrap().len();
        assert!(engine.probe_desk("xair", "not-a-desk!!!").await.is_none());
        assert_eq!(engine.devices.read().unwrap().len(), before);
    }

    #[tokio::test]
    async fn probe_silent_loopback_answers_nothing() {
        let engine = Engine::new(EngineConfig::default());
        // Nothing listens on UDP 10024 here: no reply, no row.
        assert!(engine.probe_desk("xair", "127.0.0.1").await.is_none());
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

    #[test]
    fn registries_merge_canonical_plus_provider_rows() {
        use openmix_core::params::ParamRegistry;
        let engine = Engine::new(EngineConfig::default());
        // No provider ships extra rows today: every merged registry is the
        // canonical table in catalogue order, so `caps` stays byte-identical.
        for driver in ["sim", "xair", "dl32s"] {
            let reg = engine.registry(driver);
            assert_eq!(reg.len(), openmix_core::params::CATALOGUE.len(), "{driver}");
            for def in openmix_core::params::CATALOGUE {
                assert_eq!(reg.find(def.id).map(|d| d.id), Some(def.id), "{driver}");
            }
            // Canonical fader/mute/name/pan stable through the merge.
            for id in ["fader", "mute", "name", "pan"] {
                assert!(reg.find(id).is_some(), "{driver} misses {id}");
            }
        }
        // Unknown drivers fall back to the sim registry (mirrors `drv`).
        assert_eq!(
            engine.registry("nope").len(),
            engine.registry("sim").len()
        );
        // The merge helper itself: canonical wins, provider rows append.
        let mut reg = ParamRegistry::canonical();
        reg.register(engine.drv("xair").param_defs());
        assert_eq!(reg.len(), openmix_core::params::CATALOGUE.len());
    }

    #[tokio::test]
    async fn dropout_shrinks_caps_once() {
        let engine = Engine::new(EngineConfig::default());
        let mut client = engine.attach(None).await;
        // Drain the attach snapshot (R1 six-frame prefix, then the R2
        // frames: devices/link/info/scenes/fx/routing + the master-section
        // snapshot: sim serves 8 bus + 6 subgroup + 6 mute-group masters +
        // main, no assigns/solos on a fresh engine).
        let mut tags: Vec<String> = Vec::new();
        for _ in 0..33 {
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
            &tags[6..12],
            ["devices", "link", "info", "scenes", "fx", "routing"],
            "R2 snapshot frames"
        );
        assert_eq!(
            &tags[12..20],
            ["bus", "bus", "bus", "bus", "bus", "bus", "bus", "bus"],
            "attach carries all sim bus masters"
        );
        assert_eq!(
            &tags[20..26],
            ["subgroup", "subgroup", "subgroup", "subgroup", "subgroup", "subgroup"],
            "attach carries all sim subgroup masters"
        );
        assert_eq!(
            &tags[26..32],
            ["mute_group", "mute_group", "mute_group", "mute_group", "mute_group", "mute_group"],
            "attach carries all sim mute-group masters"
        );
        assert_eq!(&tags[32..], ["main"], "attach ends masters with Main LR");
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
                        engine
                            .registry(driver)
                            .find(id)
                            .map(|def| openmix_core::params::supports_kind(def, fam.kind))
                            .unwrap_or(true)
                    })
                    .count();
                // Solo rides the poll set as its own id on xair channel
                // strips (see `static_poll_list`): one extra entry per
                // strip of the In family.
                let n = n
                    + usize::from(
                        driver == "xair" && fam.kind == openmix_core::StripKind::In,
                    );
                expect += fam.count * n;
                for s in fam.first..fam.first + fam.count {
                    let ids: Vec<&str> = list
                        .iter()
                        .filter(|(c, _)| *c == s as u16)
                        .map(|(_, id)| id.as_str())
                        .collect();
                    let mut want: Vec<&str> = engine
                        .driver_capabilities(driver)
                        .iter()
                        .filter(|id| {
                            engine
                                .registry(driver)
                                .find(id)
                                .map(|def| openmix_core::params::supports_kind(def, fam.kind))
                                .unwrap_or(true)
                        })
                        .copied()
                        .collect();
                    if driver == "xair" && fam.kind == openmix_core::StripKind::In {
                        want.push("solo");
                    }
                    assert_eq!(ids, want, "strip {s} carries the scoped set for {driver}");
                }
            }
            assert_eq!(list.len(), expect, "poll list size for {driver}");
            let max_strip = list.iter().map(|(c, _)| *c).max().unwrap() as usize;
            assert_eq!(max_strip + 1, total, "poll list covers the topology for {driver}");
        }
    }

    #[tokio::test]
    async fn poll_list_carries_xair_solo_bank() {
        // Desk→app solo backstop: xair channel strips carry the `solo`
        // poll id (bank re-read via `read_param`); nothing else does —
        // solo is not a catalogue param and never enters `caps`.
        let engine = Engine::new(EngineConfig::default());
        let list = engine.poll_list("xair").await;
        let nch = engine.driver_channels("xair");
        assert!(nch > 0, "xair has channel strips");
        for ch in 0..nch as u16 {
            assert!(
                list.contains(&(ch, "solo".to_string())),
                "xair ch{ch} misses the solo poll id"
            );
        }
        for (ch, id) in &list {
            if id == "solo" {
                assert!(
                    (*ch as usize) < nch,
                    "solo poll id off-channel: ch{ch} (channels 0..{nch})"
                );
            }
        }
        for driver in ["sim", "dl32s"] {
            let list = engine.poll_list(driver).await;
            assert!(
                !list.iter().any(|(_, id)| id == "solo"),
                "{driver} must not poll solo"
            );
        }
    }

    #[test]
    fn strip_keys_survive_desk_switch() {
        // Positional indices collide across desks (sim strip 32 = first bus
        // master, xair strip 32 = third DCA); stable keys do not.
        use openmix_core::{strip_index_for_key, strip_key_for_index};
        let engine = Engine::new(EngineConfig::default());
        let sim = engine.driver_topology("sim");
        let xair = engine.driver_topology("xair");
        // sim strip 32 keys as the first bus master on both desks.
        let k = strip_key_for_index(&sim, 32).expect("sim strip 32 keys");
        assert_eq!(k, "bus/01");
        assert_eq!(strip_index_for_key(&xair, &k), Some(20));
        // The same index means different strips per desk.
        assert_eq!(strip_key_for_index(&xair, 32), Some("dca/02".to_string()));
        // The sim `mix/..` alias resolves on xair too.
        assert_eq!(strip_index_for_key(&xair, "mix/01"), Some(20));
        // Main is bare on every desk.
        assert_eq!(strip_key_for_index(&sim, 72), Some("main".to_string()));
        assert_eq!(strip_index_for_key(&xair, "main"), Some(30));
        // Unknown families miss instead of aliasing a wrong strip.
        assert_eq!(strip_index_for_key(&xair, "bus/07"), None);
        assert_eq!(strip_index_for_key(&sim, "grp/01"), None);
    }

    #[test]
    fn recall_scope_matches_strip_keys() {
        // Union semantics: indices OR resolved keys, unknown keys ignored.
        let engine = Engine::new(EngineConfig::default());
        let sim = engine.driver_topology("sim");
        let all = RecallScope::default();
        assert!(all.matches_strip(0, &sim));
        let by_idx = RecallScope {
            strips: Some(vec![1]),
            ..Default::default()
        };
        assert!(by_idx.matches_strip(1, &sim));
        assert!(!by_idx.matches_strip(0, &sim));
        let by_key = RecallScope {
            strip_keys: Some(vec!["ch/02".to_string()]),
            ..Default::default()
        };
        assert!(by_key.matches_strip(1, &sim));
        assert!(!by_key.matches_strip(0, &sim));
        // Legacy alias participates in the union.
        let by_alias = RecallScope {
            strips: Some(vec![0]),
            strip_keys: Some(vec!["mix/01".to_string()]),
            ..Default::default()
        };
        assert!(by_alias.matches_strip(0, &sim));
        assert!(by_alias.matches_strip(32, &sim));
        assert!(!by_alias.matches_strip(2, &sim));
        // Unknown keys never match (another desk's strips, not a failure).
        let unknown = RecallScope {
            strip_keys: Some(vec!["grp/01".to_string()]),
            ..Default::default()
        };
        assert!(!unknown.matches_strip(0, &sim));
    }

    #[test]
    fn old_frames_parse_without_keys() {
        // Wire back-compat: every key-bearing frame parses when `key` (and
        // `strip_keys` / `strip_keys`) is absent, as the committed golden
        // fixtures prove.
        for text in [
            r#"{"type":"state","driver":"sim","strips":[{"ch":0,"name":"CH01","fader":0.5,"muted":false}],"main_fader":0.8}"#,
            r#"{"type":"param","ch":2,"field":"gain","value":12.5}"#,
            r#"{"type":"params","ch":3,"values":{"gain":12.5}}"#,
            r#"{"type":"solo","ch":1,"soloed":true}"#,
            r#"{"type":"subgroup_assign","ch":0,"group":1,"member":true}"#,
            r#"{"type":"mute_group_assign","ch":5,"group":2,"member":false}"#,
            r#"{"type":"bus","bus":0,"fader":0.75,"muted":false}"#,
            r#"{"type":"subgroup","group":1,"fader":0.7,"muted":true}"#,
            r#"{"type":"mute_group","group":0,"muted":true}"#,
            r#"{"type":"routing","patches":[{"strip":0,"group":"Net","index":5,"label":"Net 05"}]}"#,
            r#"{"type":"offline_diff","changes":[{"strip":0,"id":"gain","from":20.0,"to":25.0}],"count":1}"#,
            r#"{"type":"caps","driver":"sim","params":[],"unsupported":[],"topology":[{"key":"ch","kind":"in","label":"Ch","first":0,"count":2,"name_prefix":"Ch","color_default":2,"params_scope":["in"]}]}"#,
        ] {
            let v: serde_json::Value = serde_json::from_str(text).unwrap();
            let evt: Event = serde_json::from_value(v).unwrap_or_else(|e| panic!("old frame parses: {text}: {e}"));
            let back = serde_json::to_value(&evt).unwrap();
            assert!(back.get("key").is_none(), "old frame stays key-less: {text}");
        }
        // Old commands (no `key`) parse; new commands accept `key`.
        for text in [
            r#"{"cmd":"fader","ch":0,"value":0.5}"#,
            r#"{"cmd":"mute","ch":0,"muted":true}"#,
            r#"{"cmd":"param","ch":1,"field":"gain","value":12.5}"#,
            r#"{"cmd":"lock","ch":0}"#,
            r#"{"cmd":"patch_set","strip":0,"group":"Lcl","index":1}"#,
            r#"{"cmd":"subgroup_assign","ch":0,"group":1,"member":true}"#,
        ] {
            crate::protocol::parse_cmd(text).unwrap_or_else(|e| panic!("old cmd parses: {text}: {e}"));
        }
        let cmd = crate::protocol::parse_cmd(r#"{"cmd":"fader","ch":0,"key":"ch/02","value":0.5}"#).unwrap();
        assert!(matches!(cmd, Cmd::Fader { key: Some(_), .. }));
        // Recall scopes accept `strip_keys` alongside `strips`.
        let scope: RecallScope = serde_json::from_value(serde_json::json!({"strips":[0],"strip_keys":["ch/02"]})).unwrap();
        assert_eq!(scope.strip_keys, Some(vec!["ch/02".to_string()]));
        let legacy: RecallScope = serde_json::from_value(serde_json::json!({"strips":[0]})).unwrap();
        assert_eq!(legacy.strip_keys, None);
    }

    #[tokio::test]
    async fn cmd_key_authoritative_over_index() {
        // A profile replayed on another desk addresses strips by key: the
        // key wins over the positional `ch`.
        let engine = parked_engine();
        let mut client = engine.attach(None).await;
        settle_parked_poller(&engine, &mut client).await;
        client
            .send(Cmd::Fader {
                ch: 0,
                key: Some("ch/02".to_string()),
                value: 0.5,
            })
            .await
            .unwrap();
        let mut saw: Option<(u16, Option<String>)> = None;
        for _ in 0..128 {
            match client.events.try_recv() {
                Ok(Event::Param { ch, key, field, .. }) if field == "fader" => {
                    saw = Some((ch, key));
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert_eq!(saw, Some((1, Some("ch/02".to_string()))), "key resolves to strip 1");
        // Unknown keys are honest errors, never a silent wrong-strip write.
        client
            .send(Cmd::Mute {
                ch: 0,
                muted: true,
                key: Some("grp/01".to_string()),
            })
            .await
            .unwrap();
        let mut erred = false;
        for _ in 0..128 {
            match client.events.try_recv() {
                Ok(Event::Error { message, .. }) if message.contains("grp/01") => {
                    erred = true;
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(erred, "unknown strip key errors honestly");
    }

    #[tokio::test]
    async fn scene_recall_resolves_keys() {
        // A scene captured on one desk recalls at resolved keys: save with
        // strip 0 at 0.9, rewrite its key to ch/02, recall fades strip 1.
        let engine = parked_engine();
        let mut client = engine.attach(None).await;
        settle_parked_poller(&engine, &mut client).await;
        client
            .send(Cmd::SceneSave {
                n: 7,
                name: "keys".to_string(),
            })
            .await
            .unwrap();
        {
            let mut lib = engine.scenes_lib.write().unwrap();
            let scene = lib.get_mut(&7).expect("scene saved");
            // Drop the native ch/02 row so the re-keyed row is the only
            // writer to strip 1 (otherwise the later row would win).
            scene.strips.retain(|s| s.strip != 1);
            let row = scene.strips.iter_mut().find(|s| s.strip == 0).expect("strip 0 captured");
            row.fader = 0.9;
            row.key = Some("ch/02".to_string());
        }
        client
            .send(Cmd::SceneRecall {
                n: 7,
                fade_ms: Some(0),
                scope: None,
            })
            .await
            .unwrap();
        let drv = engine.drv("sim");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let v = drv
                .get_fader(openmix_core::StripId(1))
                .await
                .unwrap_or(0.0);
            if (v - 0.9).abs() < 1e-6 {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("key-resolved recall never landed on strip 1");
            }
            tokio::task::yield_now().await;
        }
        // Scoped recall by key: only the key-named strip moves.
        client
            .send(Cmd::Fader {
                ch: 0,
                key: None,
                value: 0.1,
            })
            .await
            .unwrap();
        client
            .send(Cmd::SceneRecall {
                n: 7,
                fade_ms: Some(0),
                scope: Some(RecallScope {
                    strips: None,
                    strip_keys: Some(vec!["ch/02".to_string()]),
                    groups: None,
                    globals: Some(vec![]),
                }),
            })
            .await
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let v = drv
                .get_fader(openmix_core::StripId(1))
                .await
                .unwrap_or(0.0);
            if (v - 0.9).abs() < 1e-6 {
                break;
            }
            if std::time::Instant::now() > deadline {
                panic!("scoped key recall never landed");
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn inbound_solo_fans_out_solo_event() {
        // Desk-originated solo (ingest `solo` id, poll bank read) reaches
        // clients as `Event::Solo` — the channel the UI solos view
        // renders — while other params still ride `Params` frames.
        // Parked poller: after the settle only `apply_inbound` moves the
        // mailbox, so the sequence below is exact.
        use tokio::sync::mpsc::error::TryRecvError;
        let engine = parked_engine();
        let mut handle = engine.attach(None).await;
        settle_parked_poller(&engine, &mut handle).await;
        engine.apply_inbound(2, "solo", &ParamValue::Bool(true));
        let mut found = None;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(Event::Solo { ch, soloed, .. }) => {
                    found = Some((ch, soloed));
                    break;
                }
                Ok(_) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert_eq!(found, Some((2, true)), "solo-in must fan out Solo");
        // A repeat of the same state stays quiet (poll diff semantics —
        // no Solo echo storm every sweep).
        engine.apply_inbound(2, "solo", &ParamValue::Bool(true));
        let mut dup = false;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(Event::Solo { .. }) => {
                    dup = true;
                    break;
                }
                Ok(_) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert!(!dup, "duplicate solo state must stay quiet");
        // Unsolo fans out, and the param plane is untouched by solos.
        engine.apply_inbound(2, "solo", &ParamValue::Bool(false));
        let mut found = None;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(Event::Solo { ch, soloed, .. }) => {
                    found = Some((ch, soloed));
                    break;
                }
                Ok(_) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert_eq!(found, Some((2, false)), "solo-out must fan out Solo");
        engine.apply_inbound(2, "fader", &ParamValue::Float(0.5));
        let mut saw_params = false;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(Event::Params { ch: 2, .. }) => {
                    saw_params = true;
                    break;
                }
                Ok(_) => continue,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert!(saw_params, "non-solo params still ride Params frames");
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
                    engine
                        .registry("sim")
                        .find(id)
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
