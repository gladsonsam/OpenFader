//! [`Engine`]: the transport-agnostic mixer session. All session behaviour
//! is composed here and implemented by private child modules; adapters only
//! move [`Cmd`]/[`Event`](crate::protocol::Event) JSON.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    future::Future,
    net::IpAddr,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering},
        Arc, RwLock, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use openfader_core::{
    access::{Policy, Role},
    params::{self, ParamRegistry},
    show::{Scene, Show, DEFAULT_FADE_MS},
    MeterFrame, NativeSnapshots, ParamValue, StripId,
};
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex, Notify};

use crate::drivers::{driver_names, DynDriver, FxFlavor, RegisteredDriver};
use crate::local_library::{self, LibraryHealth, LibraryMode, LibraryStatus};
use crate::protocol::{
    Cmd, DeviceCheck, DeviceDesc, Event, FxSlotState, LibraryAction, NativeSnapshotEntry,
    NativeSnapshotsCaps, OfflineChange, ParamDef, PatchEntry, Scene as SceneFrame, SharedEvent,
    Show as ShowFrame,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};

// Implementations share the coordinator state; child visibility stays within engine.
mod broadcast;
mod cache;
mod capabilities;
mod clients;
mod commands;
mod destinations;
mod discovery;
mod fx;
mod hydration;
mod library_status;
mod library_transactions;
mod link;
mod mailbox;
mod metadata;
mod models;
mod offline;
mod poll_catalogue;
mod poll_lanes;
mod poll_reads;
mod poll_tick;
mod readback;
mod routing;
mod saved_mixers;
mod scene_output_eq;
mod scene_recall;
mod session;
mod state;
mod tasks;
mod telemetry;

use cache::*;
use discovery::{device_row, push_row};
pub use discovery::{DeviceScanner, RealScanner};
use library_status::*;
use mailbox::{rt_slot_key, ClientEntry, RtSlots};
pub use mailbox::{ClientHandle, ClientMailbox};
pub use models::{
    check_filename, now_unix, param_scope_group, validate_scene_n, EngineConfig, LinkState,
    OfflineState, RecallScope, StoredAutomix, StoredFx, StoredFxInsert, StoredMonitor,
    StoredMonitorValue, StoredMuteGroupMembership, StoredMuteGroups, StoredOutputEq,
    StoredOutputEqTarget, StoredOutputTaps, StoredRoute, StoredRta, StoredScene, StoredStrip,
    DESK_SAMPLE_RATE, FILE_MAX_BYTES, FX_POLL_EVERY, GR_CHANNELS, LINK_INTERVAL, LOCK_TTL,
    PARAM_POLL_INTERVAL, POLL_TIMEOUT_STRIKES, SCENE_MAX, SCENE_MIN, SPECTRUM_CHANNELS,
};
use models::{
    pv_bool, CommitDisposition, FxSlotRt, ReadFailures, SyncState, DIRECT_IDENTIFY_TIMEOUT,
    SCAN_VERIFY_TIMEOUT,
};
use poll_lanes::*;
use poll_reads::*;
use readback::*;
use routing::*;
use saved_mixers::*;

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
    /// engineering-unit JSON value. Provenance invariant (R2 preset-truth
    /// contract): every entry is desk-evidenced, a poll read, a desk push,
    /// or a post-write re-read from a provider with genuine readback (see
    /// [`refresh_param`](Engine::refresh_param)). Write fallbacks (the
    /// `accepted` echo) never touch this map, so poll/params/snapshot paths
    /// cannot launder an unverified write into a confirmed value. Updated
    /// by the poller and by the `param`-write refresh; cleared on driver
    /// switch.
    param_cache: RwLock<HashMap<u16, HashMap<String, Value>>>,
    routing_dirty: AtomicBool,
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
    /// caller's task, they never wait on poller or meter state, and meter
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
    poll_visible: RwLock<PollVisibleCache>,
    /// Cached essentials-lane lists per driver: every strip's
    /// fader/mute/name/pan/color (+ provider `solo` backstops where
    /// served). Built once per driver (topology × caps × scopes never
    /// change at runtime, like `poll_static`); drop-class errors only SKIP
    /// entries at drain time.
    poll_core: RwLock<HashMap<String, Vec<(u16, &'static str)>>>,
    /// Round-robin cursor into the essentials lane.
    core_cursor: AtomicU64,
    /// Engine construction time: source of the monotonic `t_ms` clock in
    /// binary meter frames ([`now_ms`](Engine::now_ms)).
    boot: Instant,
    /// Focus queue: `(driver, ch, param, owner)` tuples pushed by the `focus`
    /// command, drained one per poll tick ahead of round-robin (same 20
    /// q/s budget, focus is priority, not a faster lane). Most-recent
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
    /// Persisted as `scenes.json` under `data_dir` when configured, through
    /// the durable kernel (`crate::local_library`): strict whole-document
    /// v2 load, serialized atomic commits. The current slot stays
    /// session-only (loading never recalls audio).
    scenes_lib: RwLock<HashMap<u16, StoredScene>>,
    scenes_current: RwLock<Option<u16>>,
    /// Serializes ALL numbered/named library mutations (one engine-owned
    /// async lock; never a `std` lock across `.await`).
    lib_lock: Mutex<()>,
    native_lane: Mutex<()>,
    native_requests: std::sync::Mutex<commands::NativeRequests>,
    /// Durability health of the local library (ready/error/uncertain +
    /// mode + latch message + owned file bytes).
    lib_health: RwLock<LibraryHealth>,
    /// Monotonic library revision (bumped on every committed/uncertain
    /// replace; `expected_revision` rejects stale dialogs).
    lib_revision: AtomicU64,
    /// Temp-file uniqueness counter for atomic commits (serialized by
    /// `lib_lock`).
    lib_temp: AtomicU64,
    /// Last broadcast read-only native snapshot view: (connection
    /// generation, driver view). The meter ticker re-samples the driver
    /// hook every tick and broadcasts only on change (or generation
    /// change) under the `sync_state` barrier with the generation as the
    /// frame `epoch`, so a disconnected/driver switch can never retain the
    /// last desk's slots and an old sample can never land after a newer
    /// `SyncEpoch`. Recorded by every path that emits the
    /// `native_snapshots` frame (attach, resume, post-switch, disconnect,
    /// retained-session replay) to keep the ticker quiet afterwards.
    native_last: RwLock<(u64, Option<NativeSnapshots>)>,
    data_dir: Option<PathBuf>,
    /// R2 FX cache: engine-side mirror of the active driver's rack
    /// (rebuilt on every driver switch; in-memory for Design racks,
    /// live-read for numbered racks).
    fx_cache: RwLock<Vec<FxSlotRt>>,
    /// R2 FX poll cursor (round-robin over the rack, [`FX_POLL_EVERY`]).
    fx_cursor: AtomicU64,
    fx_tick: AtomicU64,
    fx_refill_requested: AtomicBool,
    sync_state: std::sync::Mutex<SyncState>,
    repair_dirty: AtomicBool,
    /// Consecutive control-health query failures (see
    /// [`Engine::measure_latency`]): the `link` holds `connected` until
    /// [`POLL_TIMEOUT_STRIKES`] in a row, one lost datagram on venue wifi
    /// must not flap the status UI while the independent meter stream stays
    /// live. Any success (or any link reset) clears the count, so a truly
    /// dead control path still surfaces as `reconnecting` after bounded failures and is
    /// never masked by fresh meters.
    health_strikes: AtomicU8,
    read_failures: RwLock<ReadFailures>,
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
    /// background (one-shot, a perpetual round-robin would starve the
    /// background sweep, since the cursor wraps without end).
    prio_armed: AtomicBool,
    /// Lane splitter tick: alternates the background reservation (see
    /// [`Engine::poll_tick`]).
    lane_tick: AtomicU64,
    /// R2 poller pipelining: max concurrent driver queries per tick
    /// (`None` in config = per-driver default).
    poll_inflight: Option<usize>,
    /// Poll-batch RTT exponential moving average in microseconds
    /// (adaptive pipelining: the effective inflight halves while the EMA
    /// exceeds the poll tick).
    rtt_ema_us: AtomicU64,
    /// Consecutive per-(driver, param) query timeouts (transient-loss
    /// tolerance: a param drops from the poll set only after
    /// [`POLL_TIMEOUT_STRIKES`] timeouts in a row; unknown/unsupported
    /// drops immediately, successes clear the count, see
    /// [`Engine::note_poll_error`]).
    poll_timeouts: RwLock<HashMap<(String, String), u8>>,
    /// R2 LAN scanner (real discovery or an injected fake).
    scanner: Arc<dyn DeviceScanner>,
    /// Which driver holds the current hardware meter subscription
    /// (`None` until a hardware connect subscribes, and after a
    /// disconnect/switch-away clears it). A provider `subscribe_meters`
    /// installs the blob-order cache behind `poll_meters`; without this kick the meter ticker would fan out
    /// silence on hardware (the sim synthesizes its own levels).
    /// Resubscribed on every successful hardware connect (one per
    /// connection generation).
    meter_driver: RwLock<Option<String>>,
    /// Guard receiver for the current hardware meter stream: held so the
    /// broadcast stays alive while subscribed (the driver's cache drain
    /// holds its own resubscribed clone). Replaced on every hardware
    /// connect, cleared on disconnect/switch-away.
    meter_guard: RwLock<Option<tokio::sync::broadcast::Receiver<openfader_core::MeterFrame>>>,
    /// Last connected/probed address per driver (the `addr` of
    /// [`Engine::connect`]): broadcast discovery never crosses subnets, so a
    /// hardware `scan` also verifies these addresses and lists the linked
    /// desk when it answers. Persisted as `desks.json` under `data_dir` so
    /// a restart still re-finds a manually added desk.
    desk_addrs: RwLock<HashMap<String, String>>,
    /// Wakeups for edge adapters (P0): notified on every mailbox write
    /// ([`broadcast`](Engine::broadcast), `broadcast_except`, `unicast`,
    /// and the attach snapshot). An edge-adapter `push_task` waits on this
    /// instead of a second 50 ms poll, so ticker frames go out in ~ms
    /// instead of up to 50 ms of phase-added age; the 50 ms interval stays
    /// as a fallback ceiling. Coalescing is fine, receivers always drain.
    notify: Arc<Notify>,
    /// Connect-fill generation (fast-first `post_switch`): bumped on every
    /// switch; the fill worker completes the deferred remainder (state +
    /// info refresh, full FX fill, routing, masters) once per generation
    /// and broadcasts each frame as it lands. A second switch while a fill
    /// is in flight restarts it instead of piling up.
    fill_gen: AtomicU64,
    /// Wakeup for the connect-fill worker (see [`Engine::ensure_tasks`]).
    fill_notify: Arc<Notify>,
    /// Invalidates in-flight reads even when reconnecting the same driver.
    connection_gen: AtomicU64,
    fill_connection_gen: AtomicU64,
    connection_changed: Notify,
    connection_lifecycle: crate::connection_lifecycle::ConnectionLifecycle,
    /// Nonzero while the XR18 startup pass owns the read lane.
    hydrating: AtomicU64,
    reads_suspended: AtomicBool,
}

impl Engine {
    /// Build a new session. No background work starts here (safe to call
    /// outside a runtime); the poller/sweeper/meter tasks spawn on the
    /// first [`attach`](Engine::attach).
    pub fn new(config: EngineConfig) -> Arc<Self> {
        let data_dir = config.data_dir.clone();
        // Strict whole-document load of the combined v2 library (numbered +
        // named). Failures latch a visible error, empty the in-memory
        // libraries, keep the source file untouched, and block overwriting
        // writes until reset or restart. `None` stays valid in-memory.
        let loaded = local_library::load_library(data_dir.as_deref());
        let scenes_lib = loaded.numbered;
        let shows_lib = loaded.shows;
        let lib_health = loaded.health;
        // Remembered desk addresses survive a restart (scan re-verify +
        // directed unicast sweep need them; broadcast never crosses
        // subnets). In-memory only when no data dir is configured.
        let desk_addrs = load_desks(data_dir.as_deref());
        // Registry definition order is the device-claim precedence
        // (sim fallback first, then hardware in priority order).
        let rows = crate::drivers::registered_drivers(config.preset);
        let scanner = config.scanner.clone().unwrap_or_else(|| {
            Arc::new(RealScanner {
                drivers: rows
                    .iter()
                    .filter(|r| r.driver.is_hardware())
                    .map(|r| r.driver.clone())
                    .collect(),
            })
        });
        let driver_order: Vec<String> = rows.iter().map(|r| r.name.to_string()).collect();
        // Merged param registries (canonical + provider rows) ride the same
        // definition order; unknown drivers fall back to `sim` (see
        // `Engine::registry`), which is always registered.
        let registries: HashMap<String, ParamRegistry> = rows
            .iter()
            .map(|r| {
                let mut reg = r.param_registry();
                // Provider-declared scope restrictions (e.g. a desk whose
                // LR has dynamics but no external key-source leaf).
                for (id, kinds) in r.driver.scope_restrictions() {
                    reg.restrict_scopes(id, kinds);
                }
                (r.name.to_string(), reg)
            })
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
            shows: RwLock::new(shows_lib),
            param_cache: RwLock::new(HashMap::new()),
            routing_dirty: AtomicBool::new(false),
            param_dropped: RwLock::new(HashSet::new()),
            poll_cursor: AtomicU64::new(0),
            writes_in_flight: AtomicU64::new(0),
            poll_skips: AtomicU64::new(0),
            poll_static: RwLock::new(HashMap::new()),
            poll_visible: RwLock::new(None),
            poll_core: RwLock::new(HashMap::new()),
            core_cursor: AtomicU64::new(0),
            poll_timeouts: RwLock::new(HashMap::new()),
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
            lib_lock: Mutex::new(()),
            native_lane: Mutex::new(()),
            native_requests: std::sync::Mutex::new(Default::default()),
            lib_health: RwLock::new(lib_health),
            lib_revision: AtomicU64::new(0),
            lib_temp: AtomicU64::new(0),
            native_last: RwLock::new((1, None)),
            data_dir,
            fx_cache: RwLock::new(Vec::new()),
            fx_cursor: AtomicU64::new(0),
            fx_tick: AtomicU64::new(0),
            fx_refill_requested: AtomicBool::new(false),
            sync_state: std::sync::Mutex::new(SyncState::default()),
            repair_dirty: AtomicBool::new(false),
            health_strikes: AtomicU8::new(0),
            read_failures: RwLock::new(HashMap::new()),
            offline: RwLock::new(OfflineState::default()),
            visible: RwLock::new(Vec::new()),
            prio_cursor: AtomicU64::new(0),
            prio_taken: AtomicU64::new(0),
            prio_armed: AtomicBool::new(false),
            lane_tick: AtomicU64::new(0),
            poll_inflight: config.poll_inflight,
            rtt_ema_us: AtomicU64::new(0),
            scanner,
            meter_driver: RwLock::new(None),
            meter_guard: RwLock::new(None),
            desk_addrs: RwLock::new(desk_addrs),
            notify: Arc::new(Notify::new()),
            fill_gen: AtomicU64::new(0),
            fill_notify: Arc::new(Notify::new()),
            connection_gen: AtomicU64::new(1),
            fill_connection_gen: AtomicU64::new(1),
            connection_changed: Notify::new(),
            connection_lifecycle: Default::default(),
            hydrating: AtomicU64::new(0),
            reads_suspended: AtomicBool::new(false),
        });
        // FX cache for the default driver (sim): seeded synchronously so
        // the first attach already carries the rack.
        engine.refresh_fx_cache_sync();
        engine
    }
}

#[cfg(test)]
mod tests;
