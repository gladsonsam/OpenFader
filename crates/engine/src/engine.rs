//! [`Engine`]: the transport-agnostic mixer session. All session behaviour
//! lives here; adapters only move [`Cmd`]/[`Event`](crate::protocol::Event) JSON.

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

/// Link re-broadcast interval (R2): the `link` event goes out on every
/// change plus on this cadence with a fresh round-trip measurement.
pub const LINK_INTERVAL: Duration = Duration::from_secs(2);

/// R2 file-transfer upload cap (16 MiB): bounds one WS JSON frame's base64
/// payload so a bad client cannot blow the engine's memory.
pub const FILE_MAX_BYTES: usize = 16 << 20;

/// Per-desk verify cap inside [`Cmd::Scan`]: the linked-desk info query plus
/// the unicast probe fallback. Keeps a connected-but-unresponsive driver from
/// stretching a scan past the broadcast budget (the UI allows ~10 s).
const SCAN_VERIFY_TIMEOUT: Duration = Duration::from_millis(2200);

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
pub const FX_POLL_EVERY: u64 = 4;

/// R2 default desk sample rate reported in `link`/`info` (all supported
/// desks run 48 kHz; a desk reporting otherwise is a finding, not a const
/// change).
pub const DESK_SAMPLE_RATE: u32 = 48000;

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

/// Outcome of a serialized library commit (see
/// [`Engine::commit_locked`]): memory was swapped in both cases —
/// committed durably, or renamed-but-unproven. Callers publish success
/// frames only on `Committed`.
enum CommitDisposition {
    Committed { revision: u64 },
    Uncertain { revision: u64, error: String },
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
    /// Key-aware strip match against the active topology: union semantics 
    /// both lists absent means all strips; otherwise a strip matches when
    /// its index is in `strips` OR its canonical key (see
    /// [`openfader_core::strip_key_for_index`]) is in `strip_keys`. Unknown
    /// keys never match (they name strips from another desk, not a failure).
    pub fn matches_strip(
        &self,
        strip: u16,
        topology: &[openfader_core::StripFamily],
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
        openfader_core::strip_key_for_index(topology, strip as usize).is_some_and(|canon| {
            keys.iter().any(|want| {
                openfader_core::parse_strip_key(want).is_some_and(|(wf, wi)| {
                    openfader_core::parse_strip_key(&canon)
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
/// from a partial recall, Config off excludes it like the rest).
pub fn param_scope_group(id: &str) -> &'static str {
    if id == "fader" || id == "pan" || id == "balance" {
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

fn pv_bool(value: &ParamValue) -> Option<bool> { if let ParamValue::Bool(v) = value { Some(*v) } else { None } }

type ReadFailures = HashMap<(String, u16, String), (u8, Instant)>;

/// Receive ordering and snapshot publication share one short, non-async lock.
struct SyncState {
    epoch: Instant,
    revision: u64,
    received: HashMap<(u16, String), (Instant, u64)>,
    typed: BTreeMap<String, Event>,
}
impl Default for SyncState {
    fn default() -> Self { Self { epoch: Instant::now(), revision: 0, received: HashMap::new(), typed: BTreeMap::new() } }
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
    fn scan<'a>(
        &'a self,
        display: &'a (dyn Fn(&str, &str) -> String + Send + Sync),
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + 'a>>;
}

/// Production LAN scanner: broadcast probes for every row of the
/// transport [`PROBE_TABLE`](openfader_transport::discovery::PROBE_TABLE)
/// via `crates/transport`. UNVERIFIED-ON-RIG: mapping replies to devices
/// has only ever run against the fake in tests.
pub struct RealScanner;

impl DeviceScanner for RealScanner {
    fn scan<'a>(
        &'a self,
        display: &'a (dyn Fn(&str, &str) -> String + Send + Sync),
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + 'a>> {
        Box::pin(async move {
            let mut out = Vec::new();
            if let Ok(devs) = openfader_transport::discovery::discover_all(timeout).await {
                for d in devs {
                    let name = display(d.driver, &d.model);
                    push_row(&mut out, device_row(d, "broadcast", name));
                }
            }
            out
        })
    }
}

/// One discovery reply as a device row. Same mapping for broadcast, the
/// unicast verify (manual add), the linked desk and the /24 sweep; only
/// `via` differs. No sim rows ever come from here.
fn device_row(
    d: openfader_transport::discovery::DiscoveredDevice,
    via: &str,
    display_name: String,
) -> DeviceDesc {
    let ip = d.addr.ip().to_string();
    DeviceDesc {
        id: format!("{}-{ip}", d.driver),
        name: display_name,
        model: d.model,
        ip,
        kind: "console".to_string(),
        state: "available".to_string(),
        fw: None,
        via: Some(via.to_string()),
    }
}

/// Append `row` unless a row for the same IP is already listed (first row
/// wins: broadcast beats linked beats manual beats sweep).
fn push_row(out: &mut Vec<DeviceDesc>, row: DeviceDesc) {
    if !out.iter().any(|d| d.ip == row.ip) {
        out.push(row);
    }
}

/// Legacy simulator envelope-activity budget (first N inputs only).
/// Driver audio spectra use the full topology and never use this cap.
///
/// 16 covers a full drum/band layer on typical 16/24/32ch surfaces while
/// keeping the source-rate fanout bounded: 16 msgs x 64 floats/tick stays
/// well under one MTU-friendly JSON burst per tick. Channels beyond this cap
/// still stream `meters` levels.
pub const SPECTRUM_CHANNELS: usize = 16;

/// Legacy simulator GR fanout budget. Hardware uses its full topology.
pub const GR_CHANNELS: usize = 16;

/// Default per-strip lock TTL: holder id + expiry, refreshed by holder
/// activity, auto-released on disconnect/expiry.
pub const LOCK_TTL: Duration = Duration::from_secs(60);

/// Param-plane poller tick: one pipelined batch per tick (`max_inflight`
/// driver queries per tick; the 50 ms tick = 80 q/s at the default budget
/// of 4). The poller runs on its own task and never blocks
/// fader/meter traffic.
pub const PARAM_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Consecutive-timeout tolerance before a poll pair drops: a single lost
/// datagram on venue wifi must not evict a live param (eviction also flips
/// `caps.unsupported`, hiding the control until reconnect). Unknown/
/// unsupported params still drop on the first failure.
pub const POLL_TIMEOUT_STRIKES: u8 = 3;

/// Startup options for [`Engine::new`].
#[derive(Clone)]
pub struct EngineConfig {
    /// Strip-lock TTL (production: [`LOCK_TTL`]).
    pub lock_ttl: Duration,
    /// Param-plane poll tick (production: [`PARAM_POLL_INTERVAL`]).
    pub poll_interval: Duration,
    /// Optional sim desk preset (hardware drivers ignore it).
    pub preset: Option<openfader_sim::SimPreset>,
    /// R2 scene library directory: the combined numbered+named library
    /// persists as `scenes.json` (v2) here and reloads on the next
    /// [`Engine::new`]. Load is strict whole-document: a corrupt file
    /// latches a visible library error and blocks overwriting writes
    /// until reset or restart. `None` = in-memory only (tests default
    /// here and pass a temp dir when they want persistence).
    pub data_dir: Option<PathBuf>,
    /// R2 poller pipelining: max concurrent driver queries per tick.
    /// `None` in config = per-driver default (see [`DynDriver::poll_inflight`]).
    /// `--poll-inflight` sets this.
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

/// Realtime stream key: one slot per meter-class family that a newer
/// frame supersedes. `Meters` is a single global frame; `Taps` fan out per
/// negotiated tap id; `Spectrum`/`AudioSpectrum`/`Gr` fan out per channel
/// (`AudioSpectrum` without a strip shares the `u16::MAX` key, mirroring
/// the serve-adapter outbox convention).
#[derive(Clone, PartialEq, Eq, Hash)]
enum RtSlot {
    Meters,
    Tap(String),
    Spectrum(u16),
    Audio(u16),
    Gr(u16),
}

/// Classify a frame: `Some` key for supersede-by-newer meter-class frames
/// (`meters`, per-tap `taps`, envelope `spectrum`, provider
/// `audio_spectrum`, per-channel `gr`), `None` for everything else.
/// Control frames (state changes, params, roster, locks, errors, link,
/// scenes, …) are never coalescible: they queue reliably in order.
fn rt_slot_key(event: &Event) -> Option<RtSlot> {
    match event {
        Event::Meters { .. } => Some(RtSlot::Meters),
        Event::Taps { tap, .. } => Some(RtSlot::Tap(tap.clone())),
        Event::Spectrum { ch, .. } => Some(RtSlot::Spectrum(*ch)),
        Event::AudioSpectrum { ch, .. } => Some(RtSlot::Audio(ch.unwrap_or(u16::MAX))),
        Event::Gr { ch, .. } => Some(RtSlot::Gr(*ch)),
        _ => None,
    }
}

/// Per-mailbox latest-only realtime slots.
///
/// While [`inflight`](RtSlots::inflight) (a realtime frame sits queued in
/// the channel, unconsumed), incoming meter-class frames replace the slot
/// for their stream key instead of queueing: at most one latest frame per
/// key waits, however fast the ticker runs. Consuming a realtime frame
/// clears `inflight` and the [`ClientMailbox`] promotes every slot back
/// into the channel, so an active client loses nothing but superseded
/// (older same-key) frames, and a stalled client's realtime memory stays
/// flat: one queued frame plus one slot set.
#[derive(Default)]
struct RtSlots {
    /// A realtime frame is currently queued in the channel.
    inflight: bool,
    meters: Option<SharedEvent>,
    taps: HashMap<String, SharedEvent>,
    spectrum: HashMap<u16, SharedEvent>,
    audio: HashMap<u16, SharedEvent>,
    gr: HashMap<u16, SharedEvent>,
}

impl RtSlots {
    /// Replace the slot for `key` with a newer frame (the older same-key
    /// frame is superseded and dropped).
    fn store(&mut self, key: RtSlot, frame: SharedEvent) {
        match key {
            RtSlot::Meters => {
                self.meters = Some(frame);
            }
            RtSlot::Tap(tap) => {
                self.taps.insert(tap, frame);
            }
            RtSlot::Spectrum(ch) => {
                self.spectrum.insert(ch, frame);
            }
            RtSlot::Audio(key) => {
                self.audio.insert(key, frame);
            }
            RtSlot::Gr(ch) => {
                self.gr.insert(ch, frame);
            }
        }
    }

    /// Drain every slot, meters first then per-key families in key order
    /// (deterministic promotion order for tests and adapters).
    fn take_all(&mut self) -> Vec<SharedEvent> {
        let mut out = Vec::new();
        if let Some(frame) = self.meters.take() {
            out.push(frame);
        }
        let mut taps: Vec<_> = std::mem::take(&mut self.taps).into_iter().collect();
        taps.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(taps.into_iter().map(|(_, frame)| frame));
        let mut spectrum: Vec<_> = std::mem::take(&mut self.spectrum).into_iter().collect();
        spectrum.sort_by_key(|(ch, _)| *ch);
        out.extend(spectrum.into_iter().map(|(_, frame)| frame));
        let mut audio: Vec<_> = std::mem::take(&mut self.audio).into_iter().collect();
        audio.sort_by_key(|(ch, _)| *ch);
        out.extend(audio.into_iter().map(|(_, frame)| frame));
        let mut gr: Vec<_> = std::mem::take(&mut self.gr).into_iter().collect();
        gr.sort_by_key(|(ch, _)| *ch);
        out.extend(gr.into_iter().map(|(_, frame)| frame));
        out
    }

    /// Frames currently held outside the channel.
    fn occupancy(&self) -> usize {
        usize::from(self.meters.is_some())
            + self.taps.len()
            + self.spectrum.len()
            + self.audio.len()
            + self.gr.len()
    }
}

/// One connected client (presence tracking + mailbox + role).
///
/// The mailbox is a two-level fan-out: an unbounded channel carrying every
/// frame in order, plus latest-only realtime slots (see [`RtSlots`]) shared
/// with the client's [`ClientMailbox`]. Meter-class frames collapse per
/// stream key while a frame is in flight, so a stalled client holds at most
/// one queued realtime frame plus one latest frame per stream key; every
/// other frame queues reliably and is never dropped or coalesced.
struct ClientEntry {
    mailbox: mpsc::UnboundedSender<SharedEvent>,
    /// Latest-only realtime slots for this mailbox (sender side writes,
    /// the [`ClientMailbox`] promotes them back into the channel as the
    /// client drains). Shared, never held across an `.await`.
    rt: Arc<std::sync::Mutex<RtSlots>>,
    /// Estimated channel backlog (every channel send increments, every
    /// yield decrements). Backs [`ClientMailbox::len`] for edge stats.
    queued: Arc<AtomicUsize>,
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
    /// Whether this client already holds the full `caps` catalogue. Attach
    /// queues the slim caps (essentials lane only, see
    /// [`Engine::caps_slim_message`]); the first `hello` completes it with
    /// a unicast full `caps` and flips this. Later hellos are pure re-acks.
    caps_full: bool,
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
    /// engineering-unit JSON value. Provenance invariant (R2 preset-truth
    /// contract): every entry is desk-evidenced, a poll read, a desk push,
    /// or a post-write re-read from a provider with genuine readback (see
    /// [`refresh_param`](Engine::refresh_param)). Write fallbacks (the
    /// `accepted` echo) never touch this map, so poll/params/snapshot paths
    /// cannot launder an unverified write into a confirmed value. Updated
    /// by the poller and by the `param`-write refresh; cleared on driver
    /// switch.
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
    /// dead control path still surfaces as `reconnecting` after ~6 s and is
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
            scanner: config
                .scanner
                .unwrap_or_else(|| Arc::new(RealScanner)),
            meter_driver: RwLock::new(None),
            meter_guard: RwLock::new(None),
            desk_addrs: RwLock::new(desk_addrs),
            notify: Arc::new(Notify::new()),
            fill_gen: AtomicU64::new(0),
            fill_notify: Arc::new(Notify::new()),
            connection_gen: AtomicU64::new(1),
            fill_connection_gen: AtomicU64::new(1),
            connection_changed: Notify::new(),
            hydrating: AtomicU64::new(0),
            reads_suspended: AtomicBool::new(false),
        });
        // FX cache for the default driver (sim): seeded synchronously so
        // the first attach already carries the rack.
        engine.refresh_fx_cache_sync();
        engine
    }

    pub fn active_driver(&self) -> String {
        self.active.read().unwrap().clone()
    }

    /// Erased driver for `name` (falls back to `sim`, which is always
    /// registered, `known` gates every path that reaches here).
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
        // Restoration is best-effort if the desk vanished; it must not
        // prevent disconnecting or selecting another driver.
        let _ = self.active_drv().set_spectrum_target("", None).await;
        let drv = self.drv(driver);
        let was_suspended = self.reads_suspended.swap(true, Ordering::AcqRel);
        let generation = self.invalidate_connection();
        if drv.needs_hydration() {
            self.hydrating.store(generation, Ordering::Release);
        }
        if let Err(e) = drv.connect(&target).await {
            self.hydrating.store(0, Ordering::Release);
            self.reads_suspended.store(was_suspended, Ordering::Release);
            // Updates suppressed during verification belong to a live old
            // session. Repair it instead of silently losing that interval.
            self.repair_dirty.store(true, Ordering::Release);
            // The epoch already went out (clients cleared on it) while the
            // previous driver/session is retained: refill it so clients do
            // not strand empty.
            self.replay_retained_session(generation).await;
            return Err(e);
        }
        let previous = self.active_driver();
        if previous != driver { let _ = self.drv(&previous).disconnect().await; }
        // Remember the desk address only on success: a failed connect must
        // not plant an unreachable address that every later scan unicast-
        // probes (slow scans, phantom rows). Hardware scans verify the
        // remembered addresses (broadcast discovery never crosses subnets).
        self.remember_desk(driver, &target);
        *self.active.write().unwrap() = driver.to_string();
        self.param_cache.write().unwrap().clear();
        self.reads_suspended.store(false, Ordering::Release);
        // The FX cache mirrors the active driver (sim rows / xair
        // placeholders); refresh it on every switch, including boot.
        // Seeding performs no hardware queries: actual FX types/params
        // follow channel levels, masters, mutes, names and EQ in the fill.
        self.seed_fx_cache().await;
        // Hardware meter streams need an explicit subscribe per connection
        // generation (sim synthesizes its own levels via `poll_meters`).
        // Best-effort: a failed subscribe leaves the ticker on silence,
        // never fails the connect. Every successful hardware connect
        // resubscribes: the driver drops its old session (clearing
        // `meter_latest`) and its old drain/renewal tasks exit on the
        // generation bump, so skipping the resubscribe here would leave
        // the ticker on silence forever (reconnect-same-driver,
        // disconnect-reconnect, xair->sim->xair all hit this).
        if drv.wants_meter_subscribe() {
            // Drop the previous generation's guard before resubscribing so
            // a failed subscribe cannot leave a stale receiver behind; the
            // next connect retries from clean state.
            *self.meter_guard.write().unwrap() = None;
            *self.meter_driver.write().unwrap() = None;
            match drv.subscribe_meters().await {
                Ok(rx) => {
                    *self.meter_guard.write().unwrap() = Some(rx);
                    *self.meter_driver.write().unwrap() = Some(driver.to_string());
                }
                Err(e) => {
                    eprintln!("engine: {driver} meter subscribe failed (meters stay silent): {e:#}");
                }
            }
        } else {
            // Leaving a hardware meter subscription: drop the guard so the
            // old broadcast receiver is not held past the switch. The
            // driver's own renewal/drain tasks are generation-gated and
            // exit on the next hardware connect (no accumulation).
            *self.meter_guard.write().unwrap() = None;
            *self.meter_driver.write().unwrap() = None;
        }
        Ok(())
    }

    /// Replay the retained session after a failed [`Engine::connect`]: the
    /// generation was already invalidated (clients cleared on its `SyncEpoch`)
    /// but the previous driver stays active with its caches intact. Re-emit
    /// its `caps` + cached `link` + fresh `state` + `masters` + cached
    /// `params` so clients refill instead of stranding empty. Generation-
    /// gated like [`Engine::deferred_fill`]: a racing switch wins and this
    /// replay stays silent; live reads are best-effort (a silent desk drops
    /// frames, never the caller's error).
    async fn replay_retained_session(&self, generation: u64) {
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        let sampled = Instant::now();
        let state = self.state_message(None).await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        {
            let sync = self.sync_state.lock().unwrap();
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            self.broadcast(self.reconcile_event(self.caps_message(), &sync, sampled));
            self.broadcast(self.reconcile_event(self.link_message_cached(), &sync, sampled));
            self.broadcast(self.reconcile_event(state, &sync, sampled));
        }
        let masters = self.masters_snapshot().await;
        let info = self.info_message().await.ok();
        let routing = self.routing_message().await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        {
            let sync = self.sync_state.lock().unwrap();
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            for frame in info.into_iter()
                .chain([self.fx_message_cached(), routing, self.scenes_message(), self.native_snapshots_message()])
                .chain(masters) {
                self.broadcast(self.reconcile_event(frame, &sync, sampled));
            }
            for frame in self.params_snapshot() {
                self.broadcast(self.reconcile_event(frame, &sync, sampled));
            }
            drop(sync);
            self.record_native_view();
        }
    }

    /// Remember a verified desk address under `driver` and persist the map
    /// (best-effort). Only successful connects/probes reach here, so a scan
    /// never unicast-probes a dead host it invented. Sim addresses are not
    /// desks (a scan skips them anyway), so they never touch `desks.json`.
    /// Holds the `desk_addrs` write lock across the synchronous atomic
    /// persist (never across `.await`) so a racing `forget_saved_mixers`
    /// clear cannot interleave between the map insert and the disk write;
    /// failures only log and never fail the connect/probe.
    fn remember_desk(&self, driver: &str, addr: &str) {
        if !self.drv(driver).is_hardware() {
            return;
        }
        let mut addrs = self.desk_addrs.write().unwrap();
        addrs.insert(driver.to_string(), addr.to_string());
        let Some(dir) = self.data_dir.clone() else {
            return;
        };
        // Best-effort atomic persist while holding the write lock (never
        // across `.await`): `persist_desks` only logs on failure, so a
        // read-only data dir never fails the connect/probe.
        persist_desks(Some(dir.as_path()), &addrs);
    }

    /// Pure-local forget of every saved mixer address: clears the in-memory
    /// remembered desk map and durably persists the empty map. No driver,
    /// query, connect, disconnect, probe, scan or native-view work; no
    /// library revision, health, generation, caps or meter changes. Sync
    /// (no `.await` inside, no `std` lock held across `.await`) so tests
    /// and the `forget_saved_mixers` command share one path.
    pub fn forget_saved_mixers(&self, request_id: String) -> Event {
        const OP: &str = "forget_saved_mixers";
        let ok = |request_id: String| Event::DeviceOperationComplete {
            request_id,
            operation: OP.to_string(),
            results: Vec::new(),
            error: None,
        };
        let err = |request_id: String, message: String| Event::DeviceOperationComplete {
            request_id,
            operation: OP.to_string(),
            results: Vec::new(),
            error: Some(message),
        };
        let Some(dir) = self.data_dir.clone() else {
            self.desk_addrs.write().unwrap().clear();
            return ok(request_id);
        };
        let mut addrs = self.desk_addrs.write().unwrap();
        let empty: HashMap<String, String> = HashMap::new();
        match write_desks_atomic(&dir, &empty) {
            Ok(DesksWriteOutcome::Committed) => {
                addrs.clear();
                drop(addrs);
                ok(request_id)
            }
            Ok(DesksWriteOutcome::Uncertain(message)) => {
                // Rename landed but the directory sync did not prove
                // durability: memory matches the (possibly unproven) disk.
                addrs.clear();
                drop(addrs);
                err(request_id, message)
            }
            Err(message) => err(request_id, message),
        }
    }

    /// Attach a client: allocate its id, register it, and queue the
    /// initial snapshot (`you`, `state`, `shows`, `clients`, `locks`,
    /// slim `caps` (essentials lane, the full catalogue follows on the
    /// client's first `hello`, see
    /// [`caps_slim_message`](Engine::caps_slim_message)), `devices`,
    /// `link`, `info`?, `scenes`, `fx`, `routing`,
    /// master-section frames (`bus`/`subgroup`/`mute_group` + sparse
    /// assigns + `main` + sparse `solo`), then one `params` frame per
    /// cached channel, then the read-only `native_snapshots` frame, then
    /// (device mode only) the `local_library_status` frame) into the new
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
        // A connection can change while the async snapshot is being read.
        // Retry before registering so no old rows follow the new epoch.
        loop {
            let generation = self.connection_gen.load(Ordering::Acquire);

            // Snapshot the world before registering, so no live broadcast can
            // overtake the initial frames in the newcomer's mailbox. The async
            // pieces are independent reads, join them so the wall time is the
            // slowest, not the sum (on hardware drivers these are desk
            // queries); the sync pieces (shows/locks/caps/devices/link/params)
            // are lock-local and stay serial.
            let snapshot_started = Instant::now();
            // Per-mailbox realtime coalescing state: created per attach
            // attempt (a generation retry discards it with the snapshot).
            let rt = Arc::new(std::sync::Mutex::new(RtSlots::default()));
            let queued = Arc::new(AtomicUsize::new(0));
            let you = Event::You { id: id.clone() };
            let shows = self.shows_message();
            let locks = self.locks_message();
            let caps = self.caps_slim_message();
            let devices = self.devices_message();
            let link = self.link_message_cached();
            // Sync lock-local read (no I/O): the read-only native slot
            // frame for this snapshot.
            let native = self.native_snapshots_message();
            let (state, info, scenes, _fx, routing, masters) = tokio::join!(
                self.state_message(None),
                async { self.info_message().await.ok() },
                async { self.scenes_message() },
                self.fx_message(),
                self.routing_message(),
                self.masters_snapshot()
            );

            {
                // No async work under these locks. Prefix, current cache replay
                // and registration are indivisible with respect to live delivery.
                let sync = self.sync_state.lock().unwrap();
                if self.connection_gen.load(Ordering::Acquire) != generation { continue; }
                let params = self.params_snapshot();
                let fx = self.fx_message_cached();
                let mut clients = self.clients.write().unwrap();
                clients.insert(id.clone(), ClientEntry { mailbox: tx.clone(), rt: Arc::clone(&rt), queued: Arc::clone(&queued), label: clean.clone(),
                    connected_at: Instant::now(), last_write: None, policy: Policy::admin(),
                    binary_meters: false, taps: false, caps_full: false });
                let mut writers: Vec<_> = clients.iter().filter(|(_, c)| c.last_write.is_some_and(|t| t.elapsed().as_secs() < 10)).map(|(id, _)| id.clone()).collect();
                writers.sort();
                let roster = Event::Clients { count: clients.len(), writers };
                let entry = clients.get(&id).expect("client just inserted");
                // Read-only native slots ride after the param plane (past
                // every position existing snapshots assert) and before the
                // epoch barrier.
                for frame in [you, state, shows, roster, locks, caps, devices, link].into_iter()
                    .chain(info).chain([scenes, fx, routing]).chain(masters).chain(params)
                    .chain(std::iter::once(native))
                    .chain(self.device_library_status())
                    .chain(sync.typed.values().cloned())
                    .chain(std::iter::once(Event::SyncEpoch { epoch: generation }))
                {
                    Self::push_to(
                        entry,
                        SharedEvent::new(self.reconcile_event(frame, &sync, snapshot_started)),
                    );
                }
                // Lock-free (no client/sync locks held): remember the
                // emitted native view so the ticker stays quiet until it
                // changes. A push landing between the sample above and
                // this record only delays one frame (views are cumulative
                // state, nothing is lost).
                self.record_native_view();
            }
            // Siblings learn about the join; the newcomer already has it.
            self.broadcast_except(self.clients_message(), &id);
            // A fresh page on a seeded-but-incomplete FX cache (boot with a
            // numbered rack) completes the params in the background instead of
            // waiting out the one-param-per-tick poller trickle.
            if self.hydrating.load(Ordering::Acquire) != 0 || !self.fx_cache_complete() {
                self.kick_fill();
            }
            return ClientHandle { id: id.clone(), engine: Arc::clone(self), events: ClientMailbox::new(rx, tx, rt, queued) };
        }
    }

    /// Queue a FRESH snapshot into an already-attached client's mailbox 
    /// the resume path for adapters whose transport was suspended (mobile
    /// WebView backgrounded: the pushes stop, the world moves on). Same
    /// frame set an [`attach`](Engine::attach) snapshot carries, minus
    /// `you`, but with the FULL `caps` catalogue (attach ships the slim
    /// lane; a resumed UI already holds the catalogue and must never have
    /// it downgraded): `state`, `shows`, `clients`, `locks`, `caps`,
    /// `devices`, `link`, `info` (best-effort), `scenes`, `fx`, `routing`,
    /// `masters_snapshot`, then one `params` frame per cached channel,
    /// then the read-only `native_snapshots` frame, then (device mode
    /// only) the `local_library_status` frame.
    ///
    /// Never re-inserts the client, never touches locks/roles, and no-ops
    /// when the id is not attached. Cheap enough to be idempotent: adapters
    /// call it on every foreground transition so a suspended UI catches up
    /// in one batch instead of replaying frames queued while hidden.
    pub async fn push_snapshot(&self, client_id: &str) {
        let attached = self.clients.read().unwrap().contains_key(client_id);
        if !attached {
            return;
        }
        if self.active_drv().is_hardware() && !self.reads_suspended.load(Ordering::Acquire) {
            // Explicit native resync is a generation boundary, unlike
            // ordinary push delivery. Hydrate within the existing 5s budget;
            // unknowns remain absent if that budget is spent.
            let generation = self.invalidate_connection();
            self.param_cache.write().unwrap().clear();
            for row in self.fx_cache.write().unwrap().iter_mut() { row.params.clear(); }
            self.hydrating.store(generation, Ordering::Release);
            if let Err(error) = self.active_drv().refresh_streams().await { eprintln!("engine: resume stream renewal failed: {error}"); }
            self.hydrate_initial(generation).await;
            let _ = self.hydrating.compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
            if self.connection_gen.load(Ordering::Acquire) != generation { return; }
            self.refresh_fx_cache().await;
        }
        // Build everything before touching the mailbox: same order as
        // `attach` (R1 prefix then R2), so a resumed client parses exactly
        // what a fresh one does.
        let generation = self.connection_gen.load(Ordering::Acquire);
        let snapshot_started = Instant::now();
        let state = self.state_message(None).await;
        let shows = self.shows_message();
        let roster = self.clients_message();
        let locks = self.locks_message();
        let caps = self.caps_message();
        let devices = self.devices_message();
        let link = self.link_message_cached();
        let info = self.info_message().await.ok();
        let scenes = self.scenes_message();
        let routing = self.routing_message().await;
        let masters = self.masters_snapshot().await;
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        let params = self.params_snapshot();
        let fx = self.fx_message_cached();
        // Read-only native slots ride after the param plane (same position
        // as `attach`), before the epoch barrier.
        let native = self.native_snapshots_message();
        for event in [state, shows, roster, locks, caps, devices, link]
            .into_iter()
            .chain(info)
            .chain([scenes, fx, routing])
            .chain(masters)
            .chain(params)
            .chain(std::iter::once(native))
            .chain(self.device_library_status())
            .chain(sync.typed.values().cloned())
            .chain(std::iter::once(Event::SyncEpoch { epoch: generation }))
        {
            self.unicast(client_id, self.reconcile_event(event, &sync, snapshot_started));
        }
        drop(sync);
        self.record_native_view();
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
        // subscriber) get one task each, apply them to the param cache
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
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            if let Some(engine) = weak.upgrade() {
                                eprintln!("engine: {name} control update lagged {n}; repair queued");
                                engine.repair_dirty.store(true, Ordering::Release);
                            }
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    if engine.active_driver() != name || engine.reads_suspended.load(Ordering::Acquire) {
                        continue;
                    }
                    engine.apply_received(update.strip.0, &update.param, &update.value, update.received_at, None);
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
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
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
                                engine.broadcast(engine.locks_message_inner());
                            }
                        }
                        None => break,
                    }
                }
            });
        }
        // Meter/spectrum/GR ticker at the SOURCE rate
        // ([`METER_INTERVAL`](openfader_transport::METER_INTERVAL) = 50 ms =
        // 20 Hz, the desk rate): no adapter-side downsampling. GR rides the
        // same tick (cache-only reads, no extra wire traffic). Slow clients
        // get latest-only delivery in the adapter (`crates/serve`), so the
        // engine always fans out the freshest frame. R2 taps ride here too,
        // but only while at least one client negotiated them (old clients
        // see no new traffic).
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut had_audio_spectrum = false;
                let mut audio_spectrum_generation = 0;
                let mut last_audio_spectrum_epoch = 0;
                let mut meters_live = false;
                let mut last_level_sample = None;
                let mut last_gr_sample = None;
                let mut tick =
                    tokio::time::interval(openfader_transport::METER_INTERVAL);
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // Skip the immediate first tick so a fresh client gets
                // state first.
                tick.tick().await;
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    let stamps = engine.active_drv().telemetry_timestamps();
                    let level_updated = stamps.is_none_or(|(at, _)| at.is_some() && at != last_level_sample);
                    let gr_updated = stamps.is_none_or(|(_, at)| at.is_some() && at != last_gr_sample);
                    if let Some((levels, gr)) = stamps { last_level_sample = levels; last_gr_sample = gr; }
                    let levels = engine.meter_levels().await;
                    let frame = MeterFrame { levels };
                    if !frame.levels.is_empty() {
                        if level_updated {
                            engine.broadcast(Event::Meters { levels: frame.levels.clone() });
                        }
                        meters_live = true;
                    } else if meters_live {
                        // Fresh -> unavailable transition (DL32S stale >1 s,
                        // disconnect, or suspended switch): clear the UI once
                        // instead of leaving the last levels frozen (stuck
                        // clip). One-shot; stays silent until live again.
                        // Mirrors the `had_audio_spectrum` clear below, on
                        // the same tick, no new timers.
                        meters_live = false;
                        engine.broadcast(Event::Meters { levels: Vec::new() });
                        if engine.any_taps_client() {
                            let tap_drv = engine.active_drv();
                            // Fallback taps only: `poll_taps` drivers keep
                            // serving their own tap frames, so only clear the
                            // meter-derived tap here, mirroring the
                            // `tap_frames` fallback name (`in` for DL32S,
                            // else `post`). Tap-less drivers emit nothing.
                            if tap_drv.poll_taps().is_none() && !tap_drv.meter_taps().is_empty() {
                                let tap = if tap_drv.meter_taps() == ["in"] { "in" } else { "post" };
                                engine.broadcast(Event::Taps {
                                    tap: tap.to_string(),
                                    levels: Vec::new(),
                                });
                            }
                        }
                    }
                    if engine.any_taps_client() && (level_updated || gr_updated) {
                        for (tap, tap_levels) in engine.tap_frames().await {
                            if (tap == "post" && !level_updated) || (tap.starts_with("gr_") && !gr_updated) { continue; }
                            engine.broadcast(Event::Taps {
                                tap,
                                levels: tap_levels,
                            });
                        }
                    }
                    let drv = engine.active_drv();
                    let spectrum_generation = engine.connection_gen.load(Ordering::Acquire);
                    if spectrum_generation != audio_spectrum_generation {
                        audio_spectrum_generation = spectrum_generation;
                        had_audio_spectrum = false;
                        last_audio_spectrum_epoch = 0;
                    }
                    let mut supplied = std::collections::HashSet::new();
                    let frames = if engine.reads_suspended.load(Ordering::Acquire) { Vec::new() } else { drv.poll_spectrum() };
                    if frames.is_empty() && had_audio_spectrum {
                        engine.broadcast(Event::AudioSpectrum { ch: None, bins_db: Vec::new(),
                            band_edges_hz: Vec::new(), source: "unavailable".into(), position: String::new(), epoch: last_audio_spectrum_epoch });
                    }
                    had_audio_spectrum = !frames.is_empty();
                    {
                        for frame in frames {
                            let n = frame.bins_db.len();
                            if n == 0 || n > 1024 || frame.band_edges_hz.len() != n + 1
                                || frame.strip.is_some_and(|s| usize::from(s.0) >= drv.strip_count())
                                || frame.bins_db.iter().any(|v| !v.is_finite())
                                || frame.band_edges_hz.iter().any(|v| !v.is_finite() || *v <= 0.0)
                                || frame.band_edges_hz.windows(2).any(|w| w[0] >= w[1]) { continue; }
                            if let Some(strip) = frame.strip { supplied.insert(usize::from(strip.0)); }
                            last_audio_spectrum_epoch = frame.epoch;
                            engine.broadcast(Event::AudioSpectrum {
                                ch: frame.strip.map(|s| s.0), bins_db: frame.bins_db,
                                band_edges_hz: frame.band_edges_hz, source: frame.source,
                                position: frame.position, epoch: frame.epoch,
                            });
                        }
                    }
                    // Demo spectra come from the simulator's stable frequency
                    // profile, never an FFT of control-rate meter envelopes.
                    for ch in 0..frame.levels.len().min(SPECTRUM_CHANNELS) {
                        if supplied.contains(&ch) { continue; }
                        if let Some(bins) = drv.demo_spectrum(ch) {
                            engine.broadcast(Event::Spectrum { ch: ch as u16, bins });
                        }
                    }
                    let gr_count = drv.gr_channel_count();
                    for ch in 0..if gr_updated { gr_count } else { 0 } {
                        if let Some((gate, comp)) = engine.gr_db(ch).await {
                            engine.broadcast(Event::Gr { ch: ch as u16, gate, comp });
                        }
                    }
                    // Read-only native desk snapshots: re-sample the driver
                    // hook on this same tick (lock-local session-cache
                    // reads, no I/O, no new timer) and broadcast only on
                    // change or session-generation change. The generation
                    // is captured BEFORE the driver/view sampling, and the
                    // dedup + broadcast run under the `sync_state` barrier
                    // with a final generation + `reads_suspended` recheck 
                    // so an old sample can never land after a newer
                    // `SyncEpoch` (the frontend rejects a mismatched
                    // `epoch`). No audio-param fanout: the frame carries
                    // slot ids, names, the current marker and the engine
                    // `epoch` only. No guard is held across an await (no
                    // await below); `invalidate_connection` shares the same
                    // barrier.
                    {
                        let generation = engine.connection_gen.load(Ordering::Acquire);
                        let driver = engine.active_driver();
                        let view = engine.drv(&driver).native_snapshots();
                        // Held to the end of the block: the barrier shared
                        // with `invalidate_connection`.
                        let _sync = engine.sync_state.lock().unwrap();
                        if engine.connection_gen.load(Ordering::Acquire) == generation
                            && !engine.reads_suspended.load(Ordering::Acquire)
                        {
                            let key = (generation, view);
                            let changed = *engine.native_last.read().unwrap() != key;
                            if changed {
                                *engine.native_last.write().unwrap() = key.clone();
                                engine.broadcast(Self::native_message_for(
                                    &driver, &key.1, generation,
                                ));
                            }
                        }
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
                    engine.broadcast(event);
                }
            });
        }
        // Type pushes invalidate immediately; refill missing FX values
        // separately from snapshots, yielding to control writes. No timer
        // reads happen unless a type actually changed.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(50));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tick.tick().await;
                    let Some(engine) = weak.upgrade() else { break; };
                    if engine.reads_suspended.load(Ordering::Acquire) || engine.hydrating.load(Ordering::Acquire) != 0 { continue; }
                    if engine.active_drv().fx_flavor() == FxFlavor::Numbered && engine.fx_refill_requested.swap(false, Ordering::AcqRel) {
                        let changed = engine.connection_changed.notified();
                        tokio::pin!(changed);
                        changed.as_mut().enable();
                        tokio::select! {
                            _ = &mut changed => {},
                            _ = engine.refresh_fx_cache() => {},
                        }
                    }
                }
            });
        }
        // R2 connect-fill worker: completes the deferred remainder of
        // `post_switch` (state + info refresh, full FX fill, routing,
        // masters) off the command path, one broadcast per frame as it
        // lands. Woken per switch; coalesced by generation so a second
        // switch while a fill is in flight restarts it once instead of
        // piling fills up.
        {
            let weak: Weak<Self> = Arc::downgrade(self);
            tokio::spawn(async move {
                loop {
                    let notified = match weak.upgrade() {
                        Some(engine) => Arc::clone(&engine.fill_notify),
                        None => break,
                    };
                    notified.notified().await;
                    let Some(engine) = weak.upgrade() else {
                        break;
                    };
                    loop {
                        let gen = engine.fill_gen.load(Ordering::Acquire);
                        let changed = engine.connection_changed.notified();
                        tokio::pin!(changed);
                        changed.as_mut().enable();
                        let connection = engine.fill_connection_gen.load(Ordering::Acquire);
                        tokio::select! {
                            _ = &mut changed => {},
                            _ = engine.deferred_fill(connection) => {},
                        }
                        if engine.fill_gen.load(Ordering::Acquire) == gen {
                            break;
                        }
                    }
                }
            });
        }
    }

    /// Send an event to every connected client (skips closed mailboxes;
    /// the sweeper prunes them). Notifies [`tick_notify`](Engine::tick_notify)
    /// waiters (edge push tasks), one permit per call, coalesced by design.
    ///
    /// The event is serialized once and shared: one [`SharedEvent`] (one
    /// `serde_json` run, one backing allocation) is built here and an `Arc`
    /// clone is handed to each mailbox, so N clients pay one serialization
    /// instead of N. Per-client binary meter encoding still happens in the
    /// adapters (binary frames carry a per-connection sequence number).
    fn broadcast(&self, event: Event) {
        let shared = SharedEvent::new(event);
        let clients = self.clients.read().unwrap();
        for entry in clients.values() {
            Self::push_to(entry, shared.clone());
        }
        drop(clients);
        self.notify.notify_one();
    }

    /// [`broadcast`] to everyone except one client (join roster updates).
    /// Notifies tick waiters like [`broadcast`](Engine::broadcast) does.
    /// Serialization is shared exactly like [`broadcast`].
    fn broadcast_except(&self, event: Event, skip: &str) {
        let shared = SharedEvent::new(event);
        let clients = self.clients.read().unwrap();
        for (id, entry) in clients.iter() {
            if id != skip {
                Self::push_to(entry, shared.clone());
            }
        }
        drop(clients);
        self.notify.notify_one();
    }

    /// Unicast one event to a single client (direct replies, denials).
    /// Notifies tick waiters like [`broadcast`](Engine::broadcast) does.
    fn unicast(&self, id: &str, event: Event) {
        if let Some(entry) = self.clients.read().unwrap().get(id) {
            Self::push_to(entry, SharedEvent::new(event));
        }
        self.notify.notify_one();
    }

    /// Deliver one frame to one mailbox (the single choke point behind
    /// [`broadcast`](Engine::broadcast), `broadcast_except` and `unicast`,
    /// plus the attach snapshot).
    ///
    /// Control frames (everything [`rt_slot_key`] rejects: state changes,
    /// params, roster, locks, errors, …) always queue reliably in order and
    /// are never dropped or coalesced. Meter-class frames queue directly on
    /// the fast path (no realtime backlog), but while a realtime frame is
    /// still queued they collapse into that mailbox's latest-only slots 
    /// the newer frame supersedes the older same-key one, so a stalled
    /// client accumulates at most one queued realtime frame plus one slot
    /// set no matter how fast the ticker runs.
    fn push_to(entry: &ClientEntry, frame: SharedEvent) {
        if let Some(key) = rt_slot_key(&frame) {
            let mut rt = entry.rt.lock().unwrap();
            if rt.inflight {
                rt.store(key, frame);
                return;
            }
            if entry.mailbox.send(frame).is_ok() {
                entry.queued.fetch_add(1, Ordering::Relaxed);
                rt.inflight = true;
            }
            return;
        }
        if entry.mailbox.send(frame).is_ok() {
            entry.queued.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Edge-adapter wakeup handle (P0): an edge-adapter `push_task` waits on
    /// this instead of a second 50 ms poll. Notified on every mailbox
    /// write (broadcasts, unicasts, attach snapshot); the 50 ms interval
    /// stays as a fallback ceiling. [`Notify`] stores one permit while no
    /// waiter is parked, so a broadcast between drain and wait still wakes
    /// the next wait (no lost wakeup, at most coalesced).
    pub fn tick_notify(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
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
        let drv = self.active_drv();
        let owner = id.to_string();
        tokio::spawn(async move { let _ = drv.set_spectrum_target(&owner, None).await; });
        self.focus_queue
            .write()
            .unwrap()
            .retain(|(_, _, _, owner)| owner != id);
        self.broadcast(self.clients_message());
        let released = {
            let mut locks = self.locks.write().unwrap();
            let before = locks.len();
            locks.retain(|_, l| l.holder != id);
            locks.len() != before
        };
        if released {
            self.broadcast(self.locks_message_inner());
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
        if self.reads_suspended.load(Ordering::Acquire) { return Vec::new(); }
        self.active_drv().poll_meters().levels
    }

    /// Cached GR in positive dB reduction. Absence is not a received zero.
    /// Sim taps are its explicit synthetic GR source; hardware reads
    /// its DYN cache across topology inputs, buses and main.
    async fn gr_db(&self, ch: usize) -> Option<(f32, f32)> {
        if self.reads_suspended.load(Ordering::Acquire) { return None; }
        let drv = self.active_drv();
        if let Some(taps) = drv.poll_taps() {
            return taps.get(ch).map(|t| (t.gr_gate_db, t.gr_comp_db));
        }
        if !drv.dyn_gr() { return None; }
        let id = StripId(u16::try_from(ch).ok()?);
        let read = |v| match v {
            Ok(ParamValue::Float(v)) if v.is_finite() => Some(v.max(0.0)),
            _ => None,
        };
        let gate = read(drv.get_param(id, "gate_gr").await);
        let comp = read(drv.get_param(id, "comp_gr").await);
        if gate.is_none() && comp.is_none() { return None; }
        // Bus/LR have no gate. The frontend checks per-strip parameter
        // support before exposing either component.
        Some((gate.unwrap_or(0.0), comp.unwrap_or(0.0)))
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
    /// `sim`, mirroring [`Engine::drv`], `sim` is always registered.
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
    fn driver_topology(&self, driver: &str) -> Vec<openfader_core::StripFamily> {
        self.drv(driver).topology()
    }

    /// Stable key for one strip on the named driver (`None` off-topology).
    fn strip_key(&self, driver: &str, strip: u16) -> Option<String> {
        openfader_core::strip_key_for_index(&self.driver_topology(driver), strip as usize)
    }

    /// Stable key for one strip on the ACTIVE driver (`None` off-topology).
    fn active_strip_key(&self, strip: u16) -> Option<String> {
        let driver = self.active_driver();
        self.strip_key(&driver, strip)
    }

    /// Resolve a strip-targeted command's `ch` + optional `key` against the
    /// active topology: a resolvable key wins (desk-switch survival 
    /// profiles replay keys, not positions); otherwise `ch` is validated in
    /// range. An unresolvable key is an honest error naming the key.
    fn resolve_strip(&self, ch: u16, key: Option<&str>) -> anyhow::Result<u16> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
            if let Some(idx) = openfader_core::strip_index_for_key(&topo, k) {
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
                let idx = openfader_core::strip_index_for_key(&topo, k)
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
            .find(|d| device_id.is_some_and(|id| d.id == id) || d.ip == ip)
            .cloned();
        let (name, device) = match entry {
            Some(d) => (d.name.clone(), Some(d.id.clone())),
            None => {
                // Manual add-by-IP with no prior scan: resolve the steady-state
                // name from the desk `/info` reply (index 1 = console name,
                // e.g. `Studio`; index 2 = model, e.g. `XR18`, see
                // `info_from_strings`). The console name wins; the model is
                // only a degraded fallback, never the steady state when
                // `/info` answered. Unverified `/info` paths are NOT probed 
                // this reuses the single supported `query_info` codec.
                let driver = self.active_driver();
                match self.drv(&driver).query_info().await {
                    Ok(info) if !info.name.is_empty() => {
                        eprintln!("engine: link name from console name {:?}", info.name);
                        (info.name, None)
                    }
                    Ok(info) if !info.model.is_empty() => {
                        eprintln!("engine: link name from model fallback {:?}", info.model);
                        (info.model, None)
                    }
                    _ => {
                        eprintln!(
                            "engine: link name from driver key {driver:?} (/info unanswered)"
                        );
                        (driver, None)
                    }
                }
            }
        };
        // A (re)connect is a fresh control path: clear any accumulated
        // health strikes so the new session starts `connected` (see
        // `measure_latency`).
        self.health_strikes.store(0, Ordering::Release);
        let mut link = self.link.write().unwrap();
        link.state = "connected".to_string();
        link.name = name;
        link.ip = ip.to_string();
        link.fs = DESK_SAMPLE_RATE;
        link.device_id = device.or_else(|| device_id.map(str::to_string));
    }

    /// Pre-write value for the offline change list (`None` unless offline
    /// editing is active, avoids an extra desk query per write online).
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
        self.caps_message_filtered(false)
    }

    /// Slim `caps` for the attach snapshot: identical to
    /// [`caps_message`](Engine::caps_message) except `params` carries only
    /// the essentials lane (`fader`/`mute`/`name`/`pan`/`color`, the ids
    /// first paint and strip interaction resolve) instead of the ~180-row
    /// catalogue (~30 KB, three quarters of the snapshot). The full
    /// catalogue follows on the client's first `hello` (sent right after
    /// subscribe on every transport) as a unicast `caps` rebroadcast, the
    /// same frame shape as a param-drop caps update, which every consumer
    /// already applies idempotently, and `push_snapshot` (resume) keeps
    /// the full frame, so a resumed UI never downgrades its catalogue.
    fn caps_slim_message(&self) -> Event {
        self.caps_message_filtered(true)
    }

    /// Shared `caps` builder: `slim` selects the essentials-only `params`
    /// (see [`caps_slim_message`](Engine::caps_slim_message)); every other
    /// field is identical in both shapes.
    fn caps_message_filtered(&self, slim: bool) -> Event {
        let driver = self.active_driver();
        let supported = self.driver_capabilities(&driver);
        let reg = self.registry(&driver);
        let dropped = self.param_dropped.read().unwrap();
        let live: Vec<&&str> = supported
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .filter(|id| !slim || is_essentials_id(id))
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
                    .filter_map(|i| openfader_core::strip_key_for_index(&topo_fams, i))
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
            native_snapshots: self.native_caps(&driver),
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
            .unwrap_or_else(|| self.drv(driver).poll_inflight())
            .clamp(1, 16)
    }

    /// Adaptive effective inflight: halves while the batch-RTT EMA exceeds
    /// the poll tick (the desk is saturated, back off instead of piling
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
                    d.state.clone()
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
    /// query. `None` when never connected, unconfigured, or unreachable 
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
        use openfader_transport::discovery::{probe_addr, PROBE_TABLE};
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
                    format!("{raw}:{}", openfader_transport::discovery::XAIR_PORT).as_str(),
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
            name: self.drv(driver).display_name(&model),
            model,
            ip: ip_s,
            kind: "console".to_string(),
            state: "available".to_string(),
            fw: None,
            via: Some("manual".to_string()),
        })
    }

    /// Probe a supplied host with only verified discovery protocols. DNS and
    /// socket waits are async and bounded by the caller; no driver is linked.
    async fn check_recent_host(&self, addr: String) -> DeviceCheck {
        use openfader_transport::discovery::{probe_addr, PROBE_TABLE};
        let raw = addr.trim();
        let invalid = || DeviceCheck {
            addr: addr.clone(),
            status: "invalid".into(),
            device: None,
        };
        if raw.is_empty() || raw.len() > 255 || raw.chars().any(char::is_whitespace) {
            return invalid();
        }
        let ip = if let Ok(ip) = raw.parse::<IpAddr>() {
            ip
        } else if let Ok(socket) = raw.parse::<std::net::SocketAddr>() {
            socket.ip()
        } else {
            let host = raw.rsplit_once(':').map_or(raw, |(host, _)| host);
            match tokio::net::lookup_host((host, 0)).await {
                Ok(mut ips) => match ips.find(|s| s.is_ipv4()).or_else(|| ips.next()) {
                    Some(socket) => socket.ip(),
                    None => return invalid(),
                },
                Err(_) => return invalid(),
            }
        };
        // DL32S has no verified discovery handshake. In particular its
        // query_info is static identity metadata, not a network health check.
        if self.devices.read().unwrap().iter().any(|d| {
            d.model.to_ascii_lowercase().contains("dl32")
                && (d.ip == raw || d.ip.parse::<IpAddr>().ok() == Some(ip))
        }) {
            return DeviceCheck {
                addr,
                status: "unsupported".into(),
                device: None,
            };
        }
        for spec in PROBE_TABLE {
            if let Ok(rows) = probe_addr(spec, ip, Duration::from_millis(900)).await {
                if let Some(reply) = rows.into_iter().next() {
                    let name = self.drv(reply.driver).display_name(&reply.model);
                    return DeviceCheck {
                        addr,
                        status: "available".into(),
                        device: Some(device_row(reply, "refresh", name)),
                    };
                }
            }
        }
        DeviceCheck {
            addr,
            status: "no_reply".into(),
            device: None,
        }
    }

    /// New operations preserve existing rows and upsert canonical identities
    /// (driver + resolved IP). Legacy scan retains its replace semantics.
    fn merge_device_rows(&self, rows: impl IntoIterator<Item = DeviceDesc>) {
        let mut devices = self.devices.write().unwrap();
        for row in rows {
            if let Some(existing) = devices.iter_mut().find(|d| {
                d.id == row.id
                    || (d.id.split('-').next() == row.id.split('-').next()
                        && d.ip.parse::<IpAddr>().ok().is_some()
                        && d.ip.parse::<IpAddr>().ok() == row.ip.parse::<IpAddr>().ok())
            }) {
                *existing = row;
            } else {
                devices.push(row);
            }
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

    /// Control-health sample duration in ms: an OSC main-fader round trip
    /// on X-Air, a local session/cache check on DL32S, and deterministic
    /// jitter on sim. DL32S samples are not network RTT. Failures keep the
    /// last sample (never block the ticker).
    ///
    /// Liveness, not verification (R2 health contract): the probe reads the
    /// inclusive DISPLAY getter (`get_main_fader` serves own-write-pending
    /// words for continuity), exactly as it did before the verified-only
    /// split, a locally moved main fader must never read as a dead control
    /// path. Cache provenance is never treated as transport failure; only a
    /// transport/session error arms strikes. No second connection is opened.
    ///
    /// Bounded failure policy (matches the poller's
    /// [`POLL_TIMEOUT_STRIKES`] tolerance): a single failed query only arms
    /// the bounded `repair_dirty` reconciliation and keeps `connected`, one
    /// lost datagram must not flap the status UI while the independent UDP
    /// meter stream stays live. The `link` flips to `reconnecting` only
    /// after [`POLL_TIMEOUT_STRIKES`] consecutive failures (~6 s at the 2 s
    /// [`LINK_INTERVAL`]). The strike count resets on any success and on
    /// every link reset (connect/disconnect/offline), and fresh meters
    /// NEVER reset it, so a dead control path always surfaces after ~6 s
    /// and cannot be masked indefinitely by live levels.
    async fn measure_latency(&self) -> f32 {
        let last = self.link.read().unwrap().latency_ms;
        if self.reads_suspended.load(Ordering::Acquire) { return last; }
        if self.active_drv().is_hardware() {
            let t0 = Instant::now();
            let target = self.active_drv().get_main_fader().await;
            match target {
                Ok(_) => {
                    self.health_strikes.store(0, Ordering::Release);
                    let mut link = self.link.write().unwrap();
                    if link.state == "reconnecting" {
                        link.state = "connected".into();
                        eprintln!("engine: control health recovered; link connected");
                    }
                    t0.elapsed().as_secs_f32() * 1000.0
                }
                Err(error) => {
                    self.repair_dirty.store(true, Ordering::Release);
                    let strikes = self
                        .health_strikes
                        .load(Ordering::Acquire)
                        .saturating_add(1);
                    self.health_strikes.store(strikes, Ordering::Release);
                    if strikes >= POLL_TIMEOUT_STRIKES {
                        let mut link = self.link.write().unwrap();
                        if !self.reads_suspended.load(Ordering::Acquire) {
                            link.state = "reconnecting".into();
                        }
                        eprintln!("engine: control health query failed ({strikes} in a row): {error}; link reconnecting, bounded repair armed");
                    } else {
                        eprintln!("engine: control health query failed ({strikes} in a row, tolerating up to {}): {error}; link holds connected, bounded repair armed", POLL_TIMEOUT_STRIKES);
                    }
                    last
                }
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

    /// Synchronous read-only view of the local-library durability state
    /// (mode, status, monotonic revision, latch error, owned file bytes).
    /// Later FRB boot calls this before publishing the singleton and
    /// rejects on `error`/`uncertain` instead of serving a silently empty
    /// library.
    pub fn local_library_status(&self) -> local_library::LibraryStatusView {
        let h = self.lib_health.read().unwrap().clone();
        let bytes =
            local_library::storage_bytes(self.data_dir.as_deref()).unwrap_or_else(|error| {
                eprintln!("engine: storage accounting unavailable: {error}");
                None
            });
        local_library::LibraryStatusView {
            mode: h.mode.as_str().to_string(),
            status: h.state.as_str().to_string(),
            revision: self.lib_revision.load(Ordering::Relaxed),
            error: h.error.clone(),
            bytes,
        }
    }

    /// Current `local_library_status` event (explicit queries, device-mode
    /// snapshots, and post-commit broadcasts share this constructor).
    fn local_library_status_message(&self) -> Event {
        let v = self.local_library_status();
        Event::LocalLibraryStatus {
            mode: v.mode,
            status: v.status,
            revision: v.revision,
            error: v.error,
            bytes: v.bytes,
        }
    }

    /// `local_library_status` for the attach/resume snapshots: `Some` in
    /// device mode only, so the default memory snapshots stay
    /// byte-identical for existing goldens (an explicit status query still
    /// answers in memory mode).
    fn device_library_status(&self) -> Option<Event> {
        self.data_dir
            .as_ref()
            .map(|_| self.local_library_status_message())
    }

    /// Writes-blocked check: a latched `error`/`uncertain` library refuses
    /// every overwriting write until an explicit reset (or a restart, which
    /// re-reads the file). `reset` itself always passes.
    fn ensure_library_writable(&self, is_reset: bool) -> anyhow::Result<()> {
        if is_reset {
            return Ok(());
        }
        let h = self.lib_health.read().unwrap();
        if h.state != LibraryStatus::Ready {
            anyhow::bail!(
                "local library {}: {}; writes blocked until reset or restart",
                h.state.as_str(),
                h.error.as_deref().unwrap_or("unavailable")
            );
        }
        Ok(())
    }

    /// r58 P2(2): capture pin for `Save`/`Capture` actions. Samples the
    /// engine connection generation + active driver + `reads_suspended`
    /// under the existing `sync_state` barrier (brief, never across
    /// `.await`). `gen`-only checks are insufficient: a capture sampled
    /// after `invalidate_connection` but before the active-driver swap
    /// would see the new generation on the old desk, so `suspended` must
    /// gate too. Pure-local actions (rename/delete/reset) never take a
    /// pin and are never blocked by desk state.
    ///
    /// Lock order: `lib_lock` (caller, async) -> `sync_state` (here,
    /// briefly) -> `active` (read). Matches the existing
    /// `sync_state` -> `active` direction in `reconcile_event` /
    /// `deferred_fill`; no path acquires `sync_state` while holding
    /// `scenes_lib`/`shows`/`active`, so no inversion.
    fn sample_capture_pin(&self) -> (u64, String, bool) {
        let sync = self.sync_state.lock().unwrap();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active.read().unwrap().clone();
        let suspended = self.reads_suspended.load(Ordering::Acquire);
        drop(sync);
        (generation, driver, suspended)
    }

    /// Whether a library action reads the desk (`Save` captures the full
    /// plane, `Capture` snapshots it). All other actions are pure-local.
    fn action_needs_capture_pin(action: &LibraryAction) -> bool {
        matches!(
            action,
            LibraryAction::Save { .. } | LibraryAction::Capture { .. }
        )
    }

    /// Recheck the capture pin and commit under the SAME `sync_state`
    /// barrier, so `invalidate_connection` cannot slip between the check
    /// and the publish. Call with [`Engine::lib_lock`] held; `None` pin
    /// (pure-local actions) commits exactly like before. Synchronous
    /// disk commit may hold the barrier briefly, never across `.await`.
    /// Preserves the retained session on failed connects: only a
    /// mid-capture desk change rejects, never a steady state.
    fn commit_with_capture_guard(
        &self,
        pin: Option<(u64, String)>,
        numbered: HashMap<u16, StoredScene>,
        shows: Vec<Show>,
        current: Option<u16>,
    ) -> Result<CommitDisposition, String> {
        if let Some((pin_gen, pin_driver)) = pin {
            let sync = self.sync_state.lock().unwrap();
            let cur_gen = self.connection_gen.load(Ordering::Acquire);
            let cur_driver = self.active.read().unwrap().clone();
            let suspended = self.reads_suspended.load(Ordering::Acquire);
            if suspended {
                drop(sync);
                return Err(
                    "scene capture rejected: desk session changed (reads suspended)".to_string(),
                );
            }
            if cur_gen != pin_gen || cur_driver != pin_driver {
                drop(sync);
                return Err("scene capture rejected: desk changed during capture".to_string());
            }
            let res = self.commit_locked(numbered, shows, current);
            drop(sync);
            res
        } else {
            self.commit_locked(numbered, shows, current)
        }
    }

    /// Validate + durably commit candidate libraries, then swap memory.
    /// Call with [`Engine::lib_lock`] held. Pre-replace failures return
    /// `Err` with disk AND memory untouched; post-replace directory-sync /
    /// stat failures still swap (the rename landed) but latch `uncertain`
    /// and block further writes — never a silent rollback. Memory was
    /// swapped in both success cases, so callers publish success frames
    /// only on [`CommitDisposition::Committed`].
    fn commit_locked(
        &self,
        numbered: HashMap<u16, StoredScene>,
        shows: Vec<Show>,
        current: Option<u16>,
    ) -> Result<CommitDisposition, String> {
        let mode = self.lib_health.read().unwrap().mode;
        if mode == LibraryMode::Device {
            let Some(data_dir) = self.data_dir.clone() else {
                return Err("local library device mode without a data dir".to_string());
            };
            let temp_id = self.lib_temp.fetch_add(1, Ordering::Relaxed);
            match local_library::commit_file(&data_dir, &numbered, &shows, temp_id) {
                Err(e) => Err(e),
                Ok(local_library::CommitResult::Committed { bytes }) => {
                    *self.scenes_lib.write().unwrap() = numbered;
                    *self.shows.write().unwrap() = shows;
                    *self.scenes_current.write().unwrap() = current;
                    let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
                    *self.lib_health.write().unwrap() = LibraryHealth {
                        mode,
                        state: LibraryStatus::Ready,
                        error: None,
                        bytes: Some(bytes),
                    };
                    Ok(CommitDisposition::Committed { revision })
                }
                Ok(local_library::CommitResult::Uncertain { bytes, error }) => {
                    *self.scenes_lib.write().unwrap() = numbered;
                    *self.shows.write().unwrap() = shows;
                    *self.scenes_current.write().unwrap() = current;
                    let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
                    *self.lib_health.write().unwrap() = LibraryHealth {
                        mode,
                        state: LibraryStatus::Uncertain,
                        error: Some(error.clone()),
                        bytes,
                    };
                    Ok(CommitDisposition::Uncertain { revision, error })
                }
            }
        } else {
            local_library::validate_candidate(&numbered, &shows)?;
            *self.scenes_lib.write().unwrap() = numbered;
            *self.shows.write().unwrap() = shows;
            *self.scenes_current.write().unwrap() = current;
            let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
            debug_assert_eq!(
                self.lib_health.read().unwrap().state,
                LibraryStatus::Ready,
                "memory mode never latches"
            );
            Ok(CommitDisposition::Committed { revision })
        }
    }

    /// Build the commit candidate for one library action (legacy and new
    /// commands share this): clones live memory and applies the mutation.
    /// Capture actions read the driver here, under the caller's `lib_lock`,
    /// pinned to the `pin` desk session (generation + driver sampled under
    /// the `sync_state` barrier before the build); preflight/capture errors
    /// leave disk and memory untouched. `reset` makes no driver call.
    async fn build_library_candidate(
        &self,
        action: &LibraryAction,
        pin: Option<(u64, String)>,
    ) -> anyhow::Result<(HashMap<u16, StoredScene>, Vec<Show>, Option<u16>)> {
        let mut numbered = self.scenes_lib.read().unwrap().clone();
        let mut shows = self.shows.read().unwrap().clone();
        let mut current = *self.scenes_current.read().unwrap();
        match action {
            LibraryAction::Save { n, name } => {
                let n = validate_scene_n(*n)?;
                let name = clean_name(name, "name")?;
                let (pin_gen, pin_driver) = pin
                    .as_ref()
                    .map(|(g, d)| (*g, d.clone()))
                    .unwrap_or_else(|| {
                        (
                            self.connection_gen.load(Ordering::Acquire),
                            self.active_driver(),
                        )
                    });
                let scene = self.capture_full_on(n, &name, &pin_driver, pin_gen).await?;
                numbered.insert(n, scene);
            }
            LibraryAction::Rename { n, name } => {
                let n = validate_scene_n(*n)?;
                let name = clean_name(name, "name")?;
                let slot = numbered
                    .get_mut(&n)
                    .ok_or_else(|| anyhow::anyhow!("unknown scene {n:03}"))?;
                slot.name = name;
                slot.saved_at = now_unix();
            }
            LibraryAction::Delete { n } => {
                let n = validate_scene_n(*n)?;
                if numbered.remove(&n).is_none() {
                    anyhow::bail!("unknown scene {n:03}");
                }
                if current == Some(n) {
                    current = None;
                }
            }
            LibraryAction::ClearNumbered => {
                numbered.clear();
                current = None;
            }
            LibraryAction::Capture { show, name } => {
                let show = clean_name(show, "show")?;
                let name = clean_name(name, "name")?;
                let (pin_gen, pin_driver) = pin
                    .as_ref()
                    .map(|(g, d)| (*g, d.clone()))
                    .unwrap_or_else(|| {
                        (
                            self.connection_gen.load(Ordering::Acquire),
                            self.active_driver(),
                        )
                    });
                let snap = self.active_snapshot_on(&pin_driver, pin_gen).await?;
                let mut scene = Scene::capture(&name, &snap);
                scene.main_fader = self.main_fader_on(&pin_driver).await;
                match shows.iter_mut().find(|s| s.name == show) {
                    Some(slot) => slot.add_scene(scene),
                    None => {
                        let mut fresh = Show::new(&show);
                        fresh.add_scene(scene);
                        shows.push(fresh);
                    }
                }
            }
            LibraryAction::DeleteNamed { show, name } => {
                let show = clean_name(show, "show")?;
                let name = clean_name(name, "name")?;
                let slot = shows
                    .iter_mut()
                    .find(|s| s.name == show)
                    .ok_or_else(|| anyhow::anyhow!("unknown show {show:?}"))?;
                if !slot.remove(&name) {
                    anyhow::bail!("unknown scene {name:?} in show {show:?}");
                }
            }
            LibraryAction::Reset => {
                numbered.clear();
                shows.clear();
                current = None;
            }
        }
        Ok((numbered, shows, current))
    }

    /// Run one library action through the serialized candidate commit:
    /// lock, block-check, build, commit-and-swap. Pre-replace failures are
    /// `Err` with disk AND memory untouched.
    async fn library_apply_action(
        &self,
        action: &LibraryAction,
    ) -> anyhow::Result<CommitDisposition> {
        let _guard = self.lib_lock.lock().await;
        self.ensure_library_writable(action.is_reset())?;
        // r58 P2(2): capture actions bind to one desk session. The pin is
        // sampled under the `sync_state` barrier; a suspended read lane
        // fails fast before any driver I/O. Pure-local actions skip this.
        let pin_full = Self::action_needs_capture_pin(action).then(|| self.sample_capture_pin());
        if let Some((_, _, suspended)) = &pin_full {
            if *suspended {
                anyhow::bail!("scene capture rejected: desk session changed (reads suspended)");
            }
        }
        let pin = pin_full.map(|(g, d, _)| (g, d));
        let (numbered, shows, current) = self.build_library_candidate(action, pin.clone()).await?;
        self.commit_with_capture_guard(pin, numbered, shows, current)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Explicit durable mutation: revision-guarded serialized commit with a
    /// correlated unicast result on EVERY path (including preflight and
    /// capture errors — a list refresh never counts as success). Committed
    /// broadcasts the `scenes`, `shows` and status frames; rejected sends
    /// no broadcasts; uncertain broadcasts only the latched status.
    async fn library_mutate(
        &self,
        request_id: String,
        expected_revision: u64,
        action: LibraryAction,
        writer: &str,
        out: &mut Vec<Event>,
    ) {
        let operation = action.op_name().to_string();
        let mode = self.lib_health.read().unwrap().mode.as_str().to_string();
        let rq = request_id.clone();
        let op = operation.clone();
        let md = mode.clone();
        let reject = |error: String| {
            let revision = self.lib_revision.load(Ordering::Relaxed);
            self.unicast(
                writer,
                Event::LocalLibraryResult {
                    request_id: rq.clone(),
                    operation: op.clone(),
                    revision,
                    status: "rejected".to_string(),
                    mode: md.clone(),
                    error: Some(error.clone()),
                },
            );
            self.unicast(
                writer,
                Event::Error {
                    message: error,
                    locked_by: None,
                    ch: None,
                },
            );
        };
        let current = self.lib_revision.load(Ordering::Relaxed);
        if expected_revision != current {
            reject(format!(
                "stale revision: expected {expected_revision}, current {current}"
            ));
            return;
        }
        let _guard = self.lib_lock.lock().await;
        // Re-check under the lock: legacy commands share the revision.
        let current = self.lib_revision.load(Ordering::Relaxed);
        if expected_revision != current {
            reject(format!(
                "stale revision: expected {expected_revision}, current {current}"
            ));
            return;
        }
        if let Err(e) = self.ensure_library_writable(action.is_reset()) {
            reject(format!("{e:#}"));
            return;
        }
        // r58 P2(2): same desk-session binding as the legacy path. The pin
        // is sampled under the `sync_state` barrier while holding `lib_lock`;
        // suspension fails fast with the correlated result+error preserved.
        // Pure-local actions skip the pin entirely.
        let pin_full = Self::action_needs_capture_pin(&action).then(|| self.sample_capture_pin());
        if let Some((_, _, suspended)) = &pin_full {
            if *suspended {
                reject(
                    "scene capture rejected: desk session changed (reads suspended)".to_string(),
                );
                return;
            }
        }
        let pin = pin_full.map(|(g, d, _)| (g, d));
        let candidate = self.build_library_candidate(&action, pin.clone()).await;
        let (numbered, shows, current_slot) = match candidate {
            Ok(v) => v,
            Err(e) => {
                reject(format!("{e:#}"));
                return;
            }
        };
        match self.commit_with_capture_guard(pin, numbered, shows, current_slot) {
            Ok(CommitDisposition::Committed { revision }) => {
                self.unicast(
                    writer,
                    Event::LocalLibraryResult {
                        request_id,
                        operation,
                        revision,
                        status: "committed".to_string(),
                        mode,
                        error: None,
                    },
                );
                out.push(self.scenes_message());
                out.push(self.shows_message());
                out.push(self.local_library_status_message());
            }
            Ok(CommitDisposition::Uncertain { revision, error }) => {
                self.unicast(
                    writer,
                    Event::LocalLibraryResult {
                        request_id,
                        operation,
                        revision,
                        status: "uncertain".to_string(),
                        mode,
                        error: Some(error.clone()),
                    },
                );
                self.unicast(
                    writer,
                    Event::Error {
                        message: format!("local library uncertain: {error}"),
                        locked_by: None,
                        ch: None,
                    },
                );
                out.push(self.local_library_status_message());
            }
            Err(e) => reject(e),
        }
    }

    /// Pinned full-plane capture (r58 P2(2)): every driver read binds to
    /// `pin_driver` + `generation` — strips, routing and main alike — so a
    /// mid-capture connect cannot commit a mixed document. Per-strip
    /// fail-fast plus the commit-time barrier re-validation (see
    /// [`Engine::commit_with_capture_guard`]).
    async fn capture_full_on(
        &self,
        number: u16,
        name: &str,
        pin_driver: &str,
        generation: u64,
    ) -> anyhow::Result<StoredScene> {
        let driver = pin_driver.to_string();
        let topo = self.driver_topology(&driver);
        let caps = self.driver_capabilities(&driver);
        let reg = self.registry(&driver);
        let mut strips = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                let strip = StripId(s as u16);
                // Capture reads (see `capture_read_param`): verified proof
                // first, honest display continuity for fader/mute/name right
                // after local moves. Params with no proof at all are omitted
                // (never fabricated); fader/mute keep their historical
                // silence defaults only when nothing ever proved them.
                let fader = self.capture_read_param(&driver, strip, "fader").await;
                let mute = self.capture_read_param(&driver, strip, "mute").await;
                let name_v = self.capture_read_param(&driver, strip, "name").await;
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
                    if let Some(pv) = self.capture_read_param(&driver, strip, id).await {
                        // Skip catalogue defaults? No, full plane means
                        // every value, so restores are exact.
                        params.insert(id.to_string(), param_value_json(&pv));
                    }
                }
                strips.push(StoredStrip {
                    strip: s as u16,
                    key: openfader_core::strip_key_for_index(&topo, s),
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
                // r58: fail fast when the desk changes mid-capture (the
                // commit barrier re-validates before publish in any case).
                anyhow::ensure!(
                    self.connection_gen.load(Ordering::Acquire) == generation,
                    "desk changed during scene capture"
                );
                anyhow::ensure!(
                    !self.reads_suspended.load(Ordering::Acquire),
                    "desk session changed during scene capture (reads suspended)"
                );
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
        let routing = self.routing_snapshot_on(&driver).await;
        Ok(StoredScene {
            number,
            name: name.to_string(),
            saved_at: now_unix(),
            main_fader: self.main_fader_on(&driver).await,
            strips,
            fx,
            routing,
        })
    }

    /// Recall a full-plane scene: faders ride the shared fade
    /// ([`openfader_core::show::fade_strips`], same as named shows), every
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
                .and_then(|k| openfader_core::strip_index_for_key(&topo, k))
                .map(|i| i as u16)
                .unwrap_or(s.strip)
        };
        // Strips whose key resolves on this desk recall at the resolved
        // index (desk-switch survival); key-less rows and unresolvable keys
        // fall back to the positional strip (back-compat). Scope matching
        // runs on the resolved index (`strip_keys` name the same strips).
        //
        // Recall-path provenance: no per-param readback on this path
        // (best-effort fan-out on hardware), so hardware echoes are
        // `accepted` (write ACK, no value proof). The sim model applies
        // writes to real in-memory state, so its echoes are `verified`.
        let proven = !self.active_drv().is_hardware();
        let authority = || Some(if proven { "verified" } else { "accepted" }.to_string());
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
        openfader_core::show::fade_strips(self, &targets, fade_ms).await?;
        for (s, r) in &strips {
            let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
            if want_mute && s.mute {
                self.drv_set_mute(StripId(*r), true).await?;
            }
            let key = openfader_core::strip_key_for_index(&topo, *r as usize);
            if scope.is_none_or(|sc| sc.allows_param("fader")) {
                // No per-param readback on the recall path (best-effort fan
                // out on hardware): provenance per the `proven` flag above.
                out.push(Event::Param {
                    ch: *r,
                    field: "fader".to_string(),
                    value: json!(s.fader),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                    authority: authority(),
                });
            }
            if want_mute {
                out.push(Event::Param {
                    ch: *r,
                    field: "mute".to_string(),
                    value: json!(s.mute),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                    authority: authority(),
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
                // Recall-path echo: provenance per the `proven` flag above
                // (no readback on this path).
                out.push(Event::Param {
                    ch: *r,
                    field: (*id).clone(),
                    value: value.clone(),
                    by: Some(writer.to_string()),
                    key: openfader_core::strip_key_for_index(&topo, *r as usize),
                    authority: authority(),
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
    /// rather than failing the whole recall). Reads the inclusive display
    /// getters (continuity: own writes included, no proof claimed), never
    /// the stale pre-write `param_cache` entry, which the accepted write
    /// already evicted and which would start an excluded-axis fade from the
    /// wrong pre-move state.
    async fn current_fader_mute(&self, strip: StripId) -> (f32, bool) {
        let drv = self.active_drv();
        let fader = drv.get_fader(strip).await.ok().unwrap_or(0.0);
        let mute = drv.get_mute(strip).await.ok().unwrap_or(false);
        (fader, mute)
    }

    // ================= R2: FX =================

    /// `caps.fx` for the named driver: slot count + type tables (design
    /// racks carry the six design tables from `core::fx`; numbered
    /// desk-native racks carry their documented algorithms with labeled
    /// `parNN` params; rack-less drivers carry none, see README matrix).
    fn fx_caps(&self, driver: &str) -> crate::protocol::FxCaps {
        let drv = self.drv(driver);
        let types = if drv.fx_flavor() == FxFlavor::Design {
            openfader_core::fx::FX_TYPES
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
        } else if drv.fx_flavor() == FxFlavor::Numbered {
            drv.fx_type_caps()
        } else {
            Vec::new()
        };
        crate::protocol::FxCaps {
            slots: drv.fx_slot_count(),
            types,
        }
    }

    /// Current `fx` event from the engine-side cache + return levels.
    async fn fx_message(&self) -> Event { self.fx_message_cached() }

    fn fx_message_cached(&self) -> Event {
        let levels = self.active_drv().poll_meters().levels;
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

    /// Rebuild the FX cache from the active driver (Design: full rack;
    /// numbered: live types + used params; elsewhere: honest placeholder
    /// rows, type unknown, no params).
    async fn refresh_fx_cache(&self) {
        if self.active_drv().fx_flavor() == FxFlavor::Numbered {
            let generation = self.connection_gen.load(Ordering::Acquire);
            let drv = self.active_drv();
            for slot in 1..=drv.fx_slot_count() as u8 {
                let started = Instant::now();
                let Some(effect) = drv.fx_type_index(slot).await else { continue; };
                if self.connection_gen.load(Ordering::Acquire) != generation { return; }
                self.apply_received(slot as u16, "_fx_type", &ParamValue::Float(effect as f32), Instant::now(), Some(started));
                let param_count = drv.fx_type_param_count(effect).unwrap_or(0);
                for index in 0..param_count {
                    if self.fx_cache.read().unwrap().iter().find(|r| r.slot == slot).is_some_and(|r| r.params.contains_key(&format!("par{:02}", index + 1))) { continue; }
                    if self.writes_in_flight.load(Ordering::Relaxed) != 0 {
                        self.fx_refill_requested.store(true, Ordering::Release);
                        return;
                    }
                    let started = Instant::now();
                    let result = drv.get_fx_param_with_type(slot, index as u8, Some(effect)).await;
                    if self.connection_gen.load(Ordering::Acquire) != generation { return; }
                    if !self.fx_cache.read().unwrap().iter().find(|r| r.slot == slot).is_some_and(|r| drv.fx_type_index_of(&r.fx_type) == Some(effect)) { break; }
                    if let Ok(value) = result { self.apply_received(slot as u16, &format!("_fx_par{:02}", index + 1), &ParamValue::Float(value), Instant::now(), Some(started)); }
                }
            }
            return;
        }
        let rows = self.read_fx_rows().await;
        *self.fx_cache.write().unwrap() = rows;
    }

    /// Connect-time FX seed: in-memory Design rows, honest placeholders
    /// otherwise. No FX wire reads precede levels/mutes/names/EQ; the
    /// deferred fill and FX poller supply types and params afterwards.
    async fn seed_fx_cache(&self) {
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let rows = self.read_fx_seed_rows().await;
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        let sync = self.sync_state.lock().unwrap();
        let mut cache = self.fx_cache.write().unwrap();
        if self.active_drv().fx_flavor() != FxFlavor::Numbered { *cache = rows; return; }
        for row in rows {
            if let Some(existing) = cache.iter_mut().find(|r| r.slot == row.slot) {
                if !sync.received.get(&(row.slot as u16, "_fx_type".into())).is_some_and(|(at, _)| *at > started) { *existing = row; }
            } else { cache.push(row); }
        }
    }

    /// Seed rows without I/O: Design racks read memory; other racks keep
    /// placeholders until the deferred fill reads their actual state.
    async fn read_fx_seed_rows(&self) -> Vec<FxSlotRt> {
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
    /// the desk-native numbered rack reads the live type + used params per
    /// slot over the verified type + `parNN` path 
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
        if drv.fx_flavor() == FxFlavor::Numbered {
            let mut rows = Vec::new();
            for slot in 1..=drv.fx_slot_count() as u8 {
                rows.push(self.read_numbered_slot(slot).await);
            }
            return rows;
        }
        // Rack-less drivers are not polled (no verified state
        // path, writes are fire-and-forget there). Placeholder rows stay
        // placeholders: never inherit sim-seeded rows (or any unverified
        // state) as `prev`, the UI renders " " honestly.
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

    /// Best-effort live read of one numbered-rack FX slot: type first
    /// (unknown on error), then each used param of the loaded effect
    /// (`parNN` keys). On/off has no verified leaf and stays at the honest
    /// placeholder (`on: true`, `bypass: false`) so the UI keeps the toggle
    /// hidden. The type query runs once per slot: param reads reuse the
    /// known effect index (single-query path), so a slot costs 1+N queries.
    async fn read_numbered_slot(&self, slot: u8) -> FxSlotRt {
        let drv = self.active_drv();
        let index = drv.fx_type_index(slot).await;
        let name = index
            .and_then(|i| drv.fx_type_name(i))
            .unwrap_or("—")
            .to_string();
        let mut params = BTreeMap::new();
        if let Some(type_idx) = index {
            let used = drv.fx_type_param_count(type_idx).unwrap_or(0);
            for i in 0..used as u8 {
                if let Ok(v) = drv.get_fx_param_with_type(slot, i, Some(type_idx)).await {
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
    /// numbered-rack slot param per tick; silent unless a value changed).
    /// Other rack-less racks are not polled (no verified state path 
    /// writes are fire-and-forget there).
    async fn fx_refresh_one(&self) {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Numbered {
            self.fx_refresh_one_numbered().await;
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
            self.broadcast(self.fx_message().await);
        }
    }

    /// One numbered-rack poll tick: a single live read (type when the cached
    /// row is still a placeholder, else one rotating param of the loaded
    /// effect). One query per tick keeps LAN load flat; desk-side moves
    /// converge within a few ticks and broadcasts only on change.
    async fn fx_refresh_one_numbered(&self) {
        let count = self.active_drv().fx_slot_count();
        if count == 0 { return; }
        let generation = self.connection_gen.load(Ordering::Acquire);
        let n = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize;
        let slot = (n % count) as u8 + 1;
        let drv = self.active_drv();
        let started = Instant::now();
        let Some(effect) = drv.fx_type_index(slot).await else { return; };
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        self.apply_received(slot as u16, "_fx_type", &ParamValue::Float(effect as f32), Instant::now(), Some(started));
        let used = drv.fx_type_param_count(effect).unwrap_or(0);
        if used == 0 { return; }
        let index = {
            let cache = self.fx_cache.read().unwrap();
            let Some(row) = cache.iter().find(|r| r.slot == slot) else { return; };
            (0..used).find(|i| !row.params.contains_key(&format!("par{:02}", i + 1)))
                .map(|i| i as u8)
                .unwrap_or(((n / count) % used) as u8)
        };
        let started = Instant::now();
        if let Ok(value) = drv.get_fx_param_with_type(slot, index, Some(effect)).await {
            if self.connection_gen.load(Ordering::Acquire) == generation {
                // A type push received during the query invalidates this decode.
                let same_type = self.fx_cache.read().unwrap().iter().find(|r| r.slot == slot)
                    .is_some_and(|r| drv.fx_type_index_of(&r.fx_type) == Some(effect));
                if same_type { self.apply_received(slot as u16, &format!("_fx_par{:02}", index + 1), &ParamValue::Float(value), Instant::now(), Some(started)); }
            }
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

    /// `caps.native_snapshots`: read-only native slot support of the named
    /// driver (`supported:false` where the driver models no native slots 
    /// skipped on the wire there, so old frames stay byte-identical).
    fn native_caps(&self, driver: &str) -> NativeSnapshotsCaps {
        match self.drv(driver).native_snapshots() {
            Some(_) => NativeSnapshotsCaps {
                supported: true,
                read_only: true,
            },
            None => NativeSnapshotsCaps {
                supported: false,
                read_only: false,
            },
        }
    }

    /// Current `native_snapshots` event for the active driver (see
    /// [`native_message_for`](Engine::native_message_for)). The `epoch`
    /// is the actual current engine generation, sampled BEFORE the driver
    /// view so a racing connect invalidates this sample.
    fn native_snapshots_message(&self) -> Event {
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active_driver();
        Self::native_message_for(&driver, &self.drv(&driver).native_snapshots(), generation)
    }

    /// Build the `native_snapshots` event for `driver`'s hook view:
    /// `unsupported` (not modelled, never faked), `unavailable`
    /// (modelled but no live desk session), `loading` (live session, no
    /// slot data pushed yet) or `ready` (push-observed PARTIAL metadata
    /// only, never a full inventory). `current` stays `None` until a
    /// desk recall is observed, never a slot-0 default. `epoch` is the
    /// ENGINE connection generation (matches `SyncEpoch`, NOT the RTA
    /// epoch) and is populated at every constructor. Read-only: no
    /// command path writes native slots.
    fn native_message_for(driver: &str, view: &Option<NativeSnapshots>, epoch: u64) -> Event {
        match view {
            None => Event::NativeSnapshots {
                driver: driver.to_string(),
                supported: false,
                read_only: false,
                status: "unsupported".to_string(),
                slots: Vec::new(),
                current: None,
                epoch,
            },
            Some(v) => Event::NativeSnapshots {
                driver: driver.to_string(),
                supported: true,
                read_only: true,
                status: if !v.connected {
                    "unavailable"
                } else if !v.received {
                    "loading"
                } else {
                    "ready"
                }
                .to_string(),
                slots: v
                    .slots
                    .iter()
                    .map(|s| NativeSnapshotEntry {
                        slot: s.slot,
                        name: s.name.clone(),
                    })
                    .collect(),
                current: v.current,
                epoch,
            },
        }
    }

    /// Record the just-emitted native view so the ticker stays quiet until
    /// it actually changes (see [`native_last`](Engine::native_last)).
    /// The generation is captured BEFORE the driver view so a racing
    /// connect invalidates this record.
    fn record_native_view(&self) {
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active_driver();
        let view = self.drv(&driver).native_snapshots();
        *self.native_last.write().unwrap() = (generation, view);
    }

    /// Full-state `bus` echo (DISPLAY continuity, not proof): both master
    /// fields re-read via the inclusive display getters. No `authority`
    /// field; never used for preset capture.
    async fn bus_message(&self, bus: u8) -> anyhow::Result<Event> {
        let drv = self.active_drv();
        let (fader, muted) = (drv.get_bus_fader(bus).await?, drv.get_bus_mute(bus).await?);
        Ok(Event::Bus {
            bus,
            fader,
            muted,
            key: Some(openfader_core::strip_key("bus", bus.saturating_add(1) as u16)),
        })
    }

    /// Full-state `subgroup` echo (DISPLAY continuity, not proof, see
    /// [`bus_message`](Self::bus_message)).
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

    /// Full-state `main` echo (DISPLAY continuity, not proof): Main LR fader
    /// and mute re-read from the active driver inclusive getters (own writes
    /// included so a new attach sees locally moved masters). These frames
    /// carry no `authority` field and are never used for preset capture 
    /// see the `param` event for verified values.
    async fn main_message(&self) -> Event {
        let drv = self.active_drv();
        let (fader, muted) = (
            drv.get_main_fader().await.ok(),
            drv.get_main_mute().await.ok(),
        );
        Event::Main { fader, muted }
    }

    /// Master-section + membership snapshot for attach/post_switch (DISPLAY
    /// continuity, not proof): every bus/subgroup/mute-group master re-read
    /// live via the inclusive display getters (own writes included so a new
    /// attach sees locally moved masters indefinitely), every known
    /// membership (sparse, members only), Main LR, and sparse solos. These
    /// frames carry no `authority` field and are never used for preset
    /// capture, see the `param`/`params` events for verified values. Fresh pages
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
                    key: Some(openfader_core::strip_key("bus", bus.saturating_add(1) as u16)),
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
        out.push(self.main_message().await);
        // Membership: every known true and false. Reads are memory/cache-speed
        // on drivers that serve the assign leaf; elsewhere the default
        // reads honestly fail and no members are reported. Drivers with a
        // bulk source (in-memory maps) serve the whole scan from one lock
        // acquisition; the frame set and order match the per-strip reads
        // exactly, so consumers see no difference.
        if drv.group_assign_caps() != (false, false) {
            let topo = self.driver_topology(&driver);
            let strips = self.driver_strips(&driver);
            let sub_bulk = drv.sub_assign_bulk();
            let mg_bulk = drv.mg_assign_bulk();
            let solo_bulk = if !drv.is_hardware() { drv.solo_bulk() } else { None };
            for ch in 0..strips as u16 {
                let id = StripId(ch);
                let key = openfader_core::strip_key_for_index(&topo, ch as usize);
                for group in 0..subgroups {
                    let member = match &sub_bulk {
                        Some(map) => Some(map.get(&(ch, group)).copied().unwrap_or(false)),
                        None => drv.get_subgroup_assign(id, group).await.ok(),
                    };
                    if let Some(member) = member {
                        out.push(Event::SubgroupAssign { ch, group, member, key: key.clone() });
                    }
                }
                for group in 0..mute_groups {
                    let member = match &mg_bulk {
                        Some(map) => Some(map.get(&(ch, group)).copied().unwrap_or(false)),
                        None => drv.get_mute_group_assign(id, group).await.ok(),
                    };
                    if let Some(member) = member {
                        out.push(Event::MuteGroupAssign { ch, group, member, key: key.clone() });
                    }
                }
            }
            // Sparse solos served from driver memory (sim in-memory;
            // dl32s solos ride echoes, so no rows here).
            if !drv.is_hardware() {
                for ch in 0..strips as u16 {
                    let soloed = match &solo_bulk {
                        Some(map) => map.get(&ch).copied().unwrap_or(false),
                        None => drv.get_solo(StripId(ch)).await.unwrap_or(false),
                    };
                    {
                        out.push(Event::Solo {
                            ch,
                            soloed,
                            key: openfader_core::strip_key_for_index(&topo, ch as usize),
                        });
                    }
                }
            }
        }
        // Sparse desk-side solos: best-effort live bank re-read (drivers
        // whose solos live in the desk) so a fresh attach (or driver
        // switch) learns them without waiting for the poll sweep. One bank
        // query fanned out per channel (not one query per channel), cached
        // alongside (the poll diff then stays quiet); a silent desk yields
        // no frames, never fails the snapshot.
        if drv.live_solo_bank() {
            let topo = self.driver_topology(&driver);
            let started = Instant::now();
            if let Ok(bank) = drv.solo_bank(drv.channel_count()).await {
                for (ch, soloed) in bank.iter().enumerate() {
                    let ch = ch as u16;
                    self.apply_received(ch, "solo", &ParamValue::Bool(*soloed), Instant::now(), Some(started));
                    out.push(Event::Solo {
                        ch,
                        soloed: *soloed,
                        key: openfader_core::strip_key_for_index(&topo, ch as usize),
                    });
                }
            }
        }
        out
    }

    /// True when the FX cache needs no fill: design racks are in-memory
    /// complete and rack-less rows are honest placeholders; only a numbered
    /// row with a known type but a short param set wants the worker.
    fn fx_cache_complete(&self) -> bool {
        if self.active_drv().fx_flavor() != FxFlavor::Numbered {
            return true;
        }
        self.fx_cache.read().unwrap().iter().all(|row| {
            match self.active_drv().fx_type_index_of(&row.fx_type) {
                None => true,
                Some(index) => {
                    let used = self.active_drv().fx_type_param_count(index).unwrap_or(0);
                    row.params.len() >= used
                }
            }
        })
    }

    /// Current `routing` event: one row per patched input strip (sim reads
    /// the `source` param plane; elsewhere the list is empty).
    async fn routing_message(&self) -> Event {
        Event::Routing {
            patches: self
                .routing_snapshot()
                .await
                .into_iter()
                .map(|r| PatchEntry {
                    strip: r.strip,
                    key: r.key.clone(),
                    label: format!("{} {:02}", r.group, r.index),
                    group: r.group,
                    index: r.index,
                })
                .collect(),
        }
    }

    /// (strip + stable key, group, 1-based index) rows for patched inputs.
    async fn routing_snapshot(&self) -> Vec<StoredRoute> {
        let driver = self.active_driver();
        self.routing_snapshot_on(&driver).await
    }

    /// Pinned routing snapshot (r58 P2(2)): same rows as
    /// [`Engine::routing_snapshot`] but read from `driver`, so a numbered
    /// save cannot mix one desk's strips with another's routing.
    async fn routing_snapshot_on(&self, driver: &str) -> Vec<StoredRoute> {
        let drv = self.drv(driver);
        if drv.routing_groups().is_empty() {
            return Vec::new();
        }
        let topo = drv.topology();
        // Bulk `source` plane when the driver serves it (one lock instead
        // of a per-strip `get_param` await); same rows in the same order.
        let bulk = drv.source_bulk();
        let mut out = Vec::new();
        for fam in &topo {
            if fam.kind != openfader_core::StripKind::In {
                continue;
            }
            for s in fam.first..fam.first + fam.count {
                let label: Option<String> = match &bulk {
                    Some(map) => map.get(&(s as u16)).cloned(),
                    None => match drv.get_param(StripId(s as u16), "source").await {
                        Ok(ParamValue::Text(label)) => Some(label),
                        _ => None,
                    },
                };
                let Some(label) = label else { continue };
                if let Some((group, index)) = parse_source(&label) {
                    out.push(StoredRoute {
                        strip: s as u16,
                        key: openfader_core::strip_key_for_index(&topo, s),
                        group,
                        index,
                    });
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
                .and_then(|k| openfader_core::strip_index_for_key(&topo, k))
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
    /// blob cache and GR from the DYN cache (`in` unavailable, no
    /// `/meters/0` subscribe); tap-less drivers emit no tap frames. Every
    /// entry is best-effort: a tap with no source reads back silence,
    /// never an error.
    async fn tap_frames(&self) -> Vec<(String, Vec<f32>)> {
        if self.reads_suspended.load(Ordering::Acquire) { return Vec::new(); }
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
        if drv.meter_taps().is_empty() { return Vec::new(); }
        let n = self.driver_strips(&self.active_driver());
        let mut out = Vec::new();
        if !post.is_empty() {
            // A driver declaring only the pre-fader `in` tap (dl32s: the
            // meter words are verified input dB) serves it through
            // `poll_meters`; labelling it `post` would invent an unverified
            // tap. Anything else keeps the historical `post` label.
            let tap = if drv.meter_taps() == ["in"] { "in" } else { "post" };
            out.push((tap.to_string(), post));
        }
        if drv.dyn_gr() {
            let mut gr_gate = vec![0.0f32; n];
            let mut gr_comp = vec![0.0f32; n];
            let mut available = false;
            for (i, (g, c)) in gr_gate.iter_mut().zip(gr_comp.iter_mut()).enumerate() {
                if let Some((gate, comp)) = self.gr_db(i).await { available = true; *g = gate; *c = comp; }
            }
            if available {
                out.push(("gr_gate".to_string(), gr_gate));
                out.push(("gr_comp".to_string(), gr_comp));
            }
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
        // Same provenance rule as recall: sim applies to real in-memory
        // state (`verified`); hardware pushes are write-ACK only
        // (`accepted`, no readback on this path).
        let authority = || {
            Some(
                if self.active_drv().is_hardware() {
                    "accepted"
                } else {
                    "verified"
                }
                .to_string(),
            )
        };
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
                    openfader_core::strip_index_for_key(&topo, k)
                })
                .map(|i| i as u16)
                .unwrap_or(c.strip);
            out.push(Event::Param {
                ch,
                field: c.id.clone(),
                value: c.to.clone(),
                by: Some(writer.to_string()),
                key: self.active_strip_key(ch),
                // Offline-push echo: provenance per the flag above.
                authority: authority(),
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
                openfader_core::strip_index_for_key(&topo, k)
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
        self.apply_received(ch, param, pv, Instant::now(), None);
    }

    fn apply_received(&self, ch: u16, param: &str, pv: &ParamValue, received: Instant, query_started: Option<Instant>) {
        if param.starts_with("_node/") { return; }
        if param == "_fx_dirty" { self.fx_tick.store(0, Ordering::Release); return; }
        if param == "_sync_dirty" {
            if let ParamValue::Text(suffix) = pv {
                let container = suffix.rsplit_once('/').map(|(parent, _)| parent).unwrap_or(suffix);
                let container = if ["config", "preamp", "gate", "dyn", "eq", "mix", "grp", "insert", "automix"].contains(&suffix.as_str()) || suffix.ends_with("/filter") || suffix.rsplit('/').next().is_some_and(|n| n.parse::<u8>().is_ok()) { suffix.as_str() } else { container };
                let id = format!("_node/{container}");
                let driver = self.active_driver();
                let mut queue = self.focus_queue.write().unwrap();
                if !queue.iter().any(|(d, s, p, _)| d == &driver && *s == ch && p == &id) {
                    queue.push_front((driver, ch, id, String::new()));
                }
                return;
            }
            if ch == u16::MAX { self.repair_dirty.store(true, Ordering::Release); }
            else { self.queue_strip_repair(ch); }
            return;
        }
        let mut sync = self.sync_state.lock().unwrap();
        if received < sync.epoch { return; }
        let key = (ch, param.to_string());
        if let Some((last, _)) = sync.received.get(&key) {
            if *last > query_started.unwrap_or(received) { return; }
        }
        sync.revision += 1;
        let revision = sync.revision;
        sync.received.insert(key, (received, revision));
        // Internal provider IDs bridge already-existing typed library events.
        if param == "_fx_type" || param.starts_with("_fx_par") {
            let ParamValue::Float(value) = pv else { return; };
            let slot = ch as u8;
            let mut cache = self.fx_cache.write().unwrap();
            let Some(row) = cache.iter_mut().find(|r| r.slot == slot) else { return; };
            let changed = if param == "_fx_type" {
                let name = self.active_drv().fx_type_name(*value as u8).unwrap_or("—").to_string();
                if row.fx_type != name {
                    row.fx_type = name;
                    row.params.clear();
                    self.fx_refill_requested.store(true, Ordering::Release);
                    true
                } else { false }
            } else {
                let type_received = sync.received.get(&(ch, "_fx_type".to_string())).map(|(at, _)| *at);
                if type_received.is_some_and(|at| at > received) { return; }
                let name = param.trim_start_matches("_fx_").to_string();
                if row.params.get(&name) == Some(value) { false } else { row.params.insert(name, *value); true }
            };
            drop(cache);
            if changed { self.broadcast(self.fx_message_cached()); }
            return;
        }
        let v = param_value_json(pv);
        let changed = cache_store(&mut self.param_cache.write().unwrap(), ch, param, v.clone());
        let event = if param == "_mute_group" {
            pv_bool(pv).map(|muted| Event::MuteGroup { group: ch as u8, muted, key: Some(format!("mgroup/{:02}", ch + 1)) })
        } else if let Some(group) = param.strip_prefix("_mg").and_then(|s| s.parse::<u8>().ok()) {
            pv_bool(pv).map(|member| Event::MuteGroupAssign { ch, group: group - 1, member, key: self.active_strip_key(ch) })
        } else if param == "solo" {
            pv_bool(pv).map(|soloed| Event::Solo { ch, soloed, key: self.active_strip_key(ch) })
        } else { None };
        if let Some(event) = event {
            sync.typed.insert(format!("{ch}/{param}"), event.clone());
            if changed { self.broadcast(event); }
            return;
        }
        if changed {
            // Desk-evidenced by construction (poll read or desk push 
            // `apply_received` is never called with a write fallback): every
            // emitted key is explicitly marked `verified` (see `authority`).
            // Absent/unknown is NEVER verified, the field is optional only
            // for backward compatibility with old frames.
            let authority = Some(
                [(param.to_string(), "verified".to_string())]
                    .into_iter()
                    .collect(),
            );
            self.broadcast(Event::Params { ch, values: [(param.to_string(), v)].into_iter().collect(), key: self.active_strip_key(ch), authority });
        }
        // Main/bus library mirrors must follow exactly the same canonical cache.
        if matches!(param, "fader" | "mute") {
            let cache = self.param_cache.read().unwrap();
            let values = cache.get(&ch);
            let pair = values.and_then(|v| Some((v.get("fader")?.as_f64()? as f32, v.get("mute")?.as_bool()?)));
            if let Some((fader, muted)) = pair {
                let topo = self.driver_topology(&self.active_driver());
                if let Some(fam) = topo.iter().find(|f| (f.first..f.first + f.count).contains(&(ch as usize))) {
                    let event = match fam.kind {
                        openfader_core::StripKind::Main => Some(Event::Main { fader: Some(fader), muted: Some(muted) }),
                        openfader_core::StripKind::Bus => Some(Event::Bus { bus: (ch as usize - fam.first) as u8, fader, muted, key: self.active_strip_key(ch) }),
                        _ => None,
                    };
                    if let Some(event) = event {
                        sync.typed.insert(format!("master/{ch}"), event.clone());
                        if changed { self.broadcast(event); }
                    }
                }
            }
        }
    }

    fn reconcile_event(&self, mut event: Event, sync: &SyncState, started: Instant) -> Event {
        let cache = self.param_cache.read().unwrap();
        let fresh = |ch: u16, id: &str| {
            sync.received.get(&(ch, id.to_string())).filter(|(at, _)| *at >= started)
                .and_then(|_| cache.get(&ch)?.get(id)).cloned()
        };
        match &mut event {
            Event::State { strips, main_fader, .. } => {
                for row in strips {
                    if let Some(v) = fresh(row.ch, "fader").and_then(|v| v.as_f64()) { row.fader = v as f32; }
                    if let Some(v) = fresh(row.ch, "mute").and_then(|v| v.as_bool()) { row.muted = v; }
                    if let Some(Value::String(v)) = fresh(row.ch, "name") { row.name = v; }
                }
                if let Some(fam) = self.driver_topology(&self.active_driver()).iter().find(|f| f.kind == openfader_core::StripKind::Main) {
                    if let Some(v) = fresh(fam.first as u16, "fader").and_then(|v| v.as_f64()) { *main_fader = Some(v as f32); }
                }
            }
            Event::Param {
                ch, field, value, authority, ..
            } => {
                // A genuinely newer desk read (poll/push landing between
                // the echo and this reconcile) replaces the value, and the
                // provenance with it: the value is now desk-evidenced.
                if let Some(v) = fresh(*ch, field) {
                    *value = v;
                    *authority = Some("verified".to_string());
                }
            }
            Event::Solo { ch, soloed, .. } => {
                if let Some(v) = fresh(*ch, "solo").and_then(|v| v.as_bool()) {
                    *soloed = v;
                }
            }
            Event::Main { fader, muted } => {
                if let Some(fam) = self
                    .driver_topology(&self.active_driver())
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Main)
                {
                    if let Some(v) = fresh(fam.first as u16, "fader").and_then(|v| v.as_f64()) {
                        *fader = Some(v as f32);
                    }
                    if let Some(v) = fresh(fam.first as u16, "mute").and_then(|v| v.as_bool()) {
                        *muted = Some(v);
                    }
                }
            }
            Event::Bus {
                bus, fader, muted, ..
            } => {
                if let Some(fam) = self
                    .driver_topology(&self.active_driver())
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Bus)
                {
                    let ch = fam.first as u16 + *bus as u16;
                    if let Some(v) = fresh(ch, "fader").and_then(|v| v.as_f64()) {
                        *fader = v as f32;
                    }
                    if let Some(v) = fresh(ch, "mute").and_then(|v| v.as_bool()) {
                        *muted = v;
                    }
                }
            }
            Event::MuteGroup { group, muted, .. } => {
                if let Some(v) = fresh(*group as u16, "_mute_group").and_then(|v| v.as_bool()) {
                    *muted = v;
                }
            }
            Event::MuteGroupAssign {
                ch, group, member, ..
            } => {
                if let Some(v) = fresh(*ch, &format!("_mg{}", *group + 1)).and_then(|v| v.as_bool())
                {
                    *member = v;
                }
            }
            Event::Fx { .. } => {
                drop(cache);
                return self.fx_message_cached();
            }
            _ => {}
        }
        event
    }

    fn queue_strip_repair(&self, ch: u16) {
        let driver = self.active_driver();
        let list = self.static_poll_list(&driver);
        let mut queue = self.focus_queue.write().unwrap();
        for (strip, id) in list.into_iter().filter(|(strip, _)| *strip == ch) {
            if !queue.iter().any(|(_, s, p, _)| *s == strip && p == id) {
                queue.push_back((driver.clone(), strip, id.to_string(), String::new()));
            }
        }
    }

    // ================= R2: pipelined poller =================

    /// Visible-group lane: (strip, param) pairs serving the currently
    /// visible strips' EQ/dyn/sends first (R2 priority lane between focus
    /// and background). One full sweep per focus (then disarms, see
    /// `prio_armed`); `k` caps the take; the cursor round-robins within
    /// the sweep.
    fn pop_visible_batch(&self, driver: &str, k: usize) -> Vec<(u16, String)> {
        if k == 0 || (!self.drv(driver).prio_always_armed() && !self.prio_armed.load(Ordering::Relaxed)) {
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
            // boost is done, disarm and yield to the background.
            let taken = self.prio_taken.fetch_add(1, Ordering::Relaxed);
            if i == 0 && taken > 0 {
                self.prio_armed.store(false, Ordering::Relaxed);
                if !self.drv(driver).prio_always_armed() { break; }
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

    /// Cached essentials poll list: every strip × fader/mute/name/pan/
    /// color (caps + scopes filtered, like the static list) plus provider
    /// `solo` backstop ids (same rule as
    /// [`Engine::static_poll_list`]). ~150 pairs on a full-size desk (vs ~2.2k
    /// background): the always-fresh lane for the values every bank
    /// renders, so a reconnected app is desk-fresh in seconds without
    /// waiting out the background sweep. Built once per driver (topology ×
    /// caps × scopes never change at runtime); drop-class errors only SKIP
    /// entries at drain time.
    fn core_poll_list(&self, driver: &str) -> Vec<(u16, &'static str)> {
        {
            let cache = self.poll_core.read().unwrap();
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
                    if !is_essentials_id(id) {
                        continue;
                    }
                    let scoped = reg
                        .find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    out.push((s as u16, *id));
                }
                // Solo rides the essentials lane as its own id (same rule
                // as the static list, see `static_poll_list`).
                for id in self.drv(driver).essentials_extra_ids(fam.kind) {
                    out.push((s as u16, id));
                }
            }
        }
        self.poll_core.write().unwrap().insert(driver.to_string(), out.clone());
        out
    }

    /// Next `k` essentials entries (round-robin; drop-class pairs skipped in
    /// place). The lane never disarms, it is the perpetual fast path for
    /// fader/mute/name/pan/color/solo, so the background reservation in
    /// [`Engine::poll_tick`] is what keeps the background sweep alive.
    fn pop_core_batch(&self, driver: &str, k: usize) -> Vec<(u16, String)> {
        if k == 0 {
            return Vec::new();
        }
        let list = self.core_poll_list(driver);
        if list.is_empty() {
            return Vec::new();
        }
        let dropped = self.param_dropped.read().unwrap();
        let mut out = Vec::new();
        let mut skips = 0usize;
        while out.len() < k && skips <= list.len() {
            let i = self.core_cursor.fetch_add(1, Ordering::Relaxed) as usize % list.len();
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

    /// One pipelined poller tick (R2): up to `effective_inflight` driver
    /// queries per tick, strict lanes (focus > visible-group one-shot >
    /// essentials > background), results applied in batch order. At default
    /// inflight 1 this is exactly [`Engine::poll_once`]'s budget with the
    /// lanes interleaved; [`Engine::poll_once`] itself is untouched (exact
    /// legacy single-step for tests).
    ///
    /// Fairness: the essentials cursor round-robins without end, so a
    /// perpetual essentials lane would starve the background sweep forever.
    /// The background therefore keeps one slot every `max(2, n)` ticks, at
    /// xair/sim n=4 that is 20 q/s, exactly the pre-lane steady-state
    /// background rate, while the essentials lane takes the other ~60 q/s
    /// (≈2-3 s per sweep of the ~150 fader/mute/name/pan/color/solo pairs).
    /// At n=1 whole ticks alternate (lanes one tick, background the next).
    ///
    /// Public so integration tests and diagnostics can drive (and time)
    /// the poller deterministically; the background task calls this every
    /// `poll_interval`.
    pub async fn poll_tick(self: &Arc<Self>) {
        if self.hydrating.load(Ordering::Acquire) != 0
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return;
        }
        let generation = self.connection_gen.load(Ordering::Acquire);
        if Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
            self.poll_skips.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let driver = self.active_driver();
        let n = self.effective_inflight(&driver);
        if self.repair_dirty.swap(false, Ordering::AcqRel) {
            let list = self.static_poll_list(&driver);
            let mut queue = self.focus_queue.write().unwrap();
            for (ch, id) in &list {
                if !queue.iter().any(|(d, s, p, _)| d == &driver && s == ch && p == id) {
                    queue.push_back((driver.clone(), *ch, id.to_string(), String::new()));
                }
            }
            eprintln!("engine: {driver} reconciliation queued {} keys; regular query budget, no snapshot reset", list.len());
        }
        let mut batch: Vec<(u16, String)> = Vec::with_capacity(n);
        let focus_limit = self.drv(&driver).poll_focus_limit(n);
        while batch.len() < focus_limit {
            match self.pop_focus(&driver) {
                Some(e) => batch.push(e),
                None => break,
            }
        }
        if batch.len() < n {
            // Lanes with guaranteed background progress: the background
            // keeps one slot every max(2, n) ticks; the visible one-shot
            // boost takes the next share, the perpetual essentials lane the
            // rest. At n=1 this alternates whole ticks (lanes, background,
            // …); at n=4 the background keeps 1 slot in 4 (20 q/s, the
            // pre-lane steady-state rate) while the lanes take ~60 q/s.
            // Without the reservation the perpetual essentials lane would
            // starve the background sweep forever (its cursor wraps without
            // end, like the visible lane's did).
            let remaining = n - batch.len();
            let t = self.lane_tick.fetch_add(1, Ordering::Relaxed);
            let bg_share = self.drv(&driver).poll_bg_share(n, t, remaining);
            let mut rest = remaining - bg_share;
            let vis = self.pop_visible_batch(&driver, self.drv(&driver).poll_visible_take(rest));
            rest -= vis.len();
            batch.extend(vis);
            batch.extend(self.pop_core_batch(&driver, rest));
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
        let t0 = Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for (ch, param) in batch {
            let engine = Arc::clone(self);
            let driver = driver.clone();
            set.spawn(async move {
                let started = Instant::now();
                let result = engine.read_param(&driver, StripId(ch), &param).await;
                if engine.connection_gen.load(Ordering::Acquire) != generation { return; }
                match result {
                    Ok(pv) => {
                        engine.note_read_ok(&driver, ch, &param);
                        engine.apply_received(ch, &param, &pv, Instant::now(), Some(started));
                    }
                    Err(e) => { engine.note_read_error(&driver, ch, &param, &e); }
                }
            });
        }
        while set.join_next().await.is_some() {}
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        // Adaptive pipelining: EMA of the batch wall time.
        let dt_us = t0.elapsed().as_micros() as u64;
        let prev = self.rtt_ema_us.load(Ordering::Relaxed);
        let ema = if prev == 0 { dt_us } else { (prev * 3 + dt_us) / 4 };
        self.rtt_ema_us.store(ema, Ordering::Relaxed);
        if self.fx_tick.fetch_add(1, Ordering::Relaxed).is_multiple_of(FX_POLL_EVERY) {
            self.fx_refresh_one().await;
        }
    }

    /// Shared post-switch flow for `connect` / `connect_device` / `offline`:
    /// fast-first. Invalidate parameter caches and queue the
    /// paint-critical `state` + `caps` + cached `link` + `info` set
    /// immediately: caps needs no I/O at all, and state/info share one
    /// connect-burst budget (300 ms, not N x 2 s tails). The remainder
    /// (state + info refresh on timeout, full FX params, routing, masters)
    /// lands via the fill worker ([`Engine::deferred_fill`]), one broadcast
    /// per frame as it arrives. When an offline change list is pending, the
    /// `offline_diff` frame rides along and the push gate opens (nothing is
    /// pushed without `offline_push`). `entering_offline` keeps the
    /// just-armed offline session intact (otherwise a fresh connect resolves
    /// any pending diff).
    async fn post_switch(
        &self,
        writer: &str,
        out: &mut Vec<Event>,
        entering_offline: bool,
    ) -> anyhow::Result<()> {
        self.param_cache.write().unwrap().clear();
        let driver = self.active_driver();
        self.param_dropped.write().unwrap().retain(|(d, _)| d != &driver);
        self.poll_timeouts.write().unwrap().retain(|(d, _), _| d != &driver);
        // Push-cache drivers need the same ordered publication as query
        // drivers: reading their initial cache is cheap, but the normal
        // background sweep would otherwise take minutes to expose DSP.
        if self.active_drv().needs_hydration() || self.active_drv().is_hardware() {
            self.hydrating.store(self.connection_gen.load(Ordering::Acquire), Ordering::Release);
        }
        // connect() already seeded FX for this session. Do not repeat its
        // type queries ahead of channel levels and bus masters.
        self.note_write(writer);
        self.touch_holder_locks(writer);
        // Paint-critical frames first: caps + cached link need no I/O at
        // all (the UI flips to online on caps); state + info share one
        // burst budget off the command path.
        out.push(self.caps_message());
        out.push(self.link_message_cached());
        let burst = openfader_transport::session::CONNECT_BURST_TIMEOUT;
        let fast = tokio::time::timeout(
            burst,
            async {
                tokio::join!(self.state_message(Some(writer)), self.info_message())
            },
        )
        .await;
        if let Ok((state, info)) = fast {
            out.push(state);
            // `info` is `Result`: silent desks stay silent, as before.
            if let Ok(info) = info {
                out.push(info);
            }
        }
        // Budget spent (slow venue wifi): the deferred fill re-emits
        // both, so no frame is ever lost, first paint just waits for
        // the fill instead of stalling the command on N x 2 s tails.
        // The read-only native slot frame is sync and needs no I/O, so it
        // always rides along (fresh status on every switch, never the
        // last desk's slots).
        out.push(self.native_snapshots_message());
        self.record_native_view();
        if entering_offline {
            self.kick_fill();
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
        self.kick_fill();
        Ok(())
    }

    /// Deferred remainder of [`Engine::post_switch`]: priority control fill
    /// (including masters), state/info, then FX, routing and assignments.
    /// Each phase streams as it lands. Idempotent (cache diffs keep repeats quiet);
    /// every section is best-effort so a silent desk never fails the fill.
    async fn deferred_fill(self: &Arc<Self>, generation: u64) {
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return;
        }
        if self.hydrating.load(Ordering::Acquire) == generation {
            self.hydrate_initial(generation).await;
            if self.connection_gen.load(Ordering::Acquire) != generation {
                return;
            }
            // The burst already served pending focus/visible entries.
            // Keep failures queued for normal retry; do not re-read every
            // successfully hydrated input before serving other banks.
            {
                let cache = self.param_cache.read().unwrap();
                self.focus_queue.write().unwrap().retain(|(_, ch, id, _)| {
                    !cache.get(ch).is_some_and(|values| values.contains_key(id))
                });
            }
            self.prio_armed.store(false, Ordering::Relaxed);
            let _ = self.hydrating.compare_exchange(generation, 0, Ordering::AcqRel, Ordering::Acquire);
        }
        let sampled = Instant::now();
        let state = self.state_message(None).await;
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        {
            let sync = self.sync_state.lock().unwrap();
            self.broadcast(self.reconcile_event(state, &sync, sampled));
        }
        if let Ok(info) = self.info_message().await {
            self.broadcast(info);
        }
        // Lower-priority rack/routing/assignment detail follows the controls;
        // the initial fill already streamed bus/subgroup/main master levels.
        self.refresh_fx_cache().await;
        self.broadcast(self.fx_message().await);
        self.broadcast(self.routing_message().await);
        let sampled = Instant::now();
        let masters = self.masters_snapshot().await;
        if self.connection_gen.load(Ordering::Acquire) != generation { return; }
        let sync = self.sync_state.lock().unwrap();
        for frame in masters { self.broadcast(self.reconcile_event(frame, &sync, sampled)); }
    }

    /// Stop old-session work before replacing a driver session.
    fn invalidate_connection(&self) -> u64 {
        // Epoch publication and snapshot delivery share the same barrier.
        let mut sync = self.sync_state.lock().unwrap();
        let generation = self.connection_gen.fetch_add(1, Ordering::AcqRel) + 1;
        *sync = SyncState::default();
        self.read_failures.write().unwrap().clear();
        self.broadcast(Event::SyncEpoch { epoch: generation });
        drop(sync);
        self.connection_changed.notify_waiters();
        self.hydrating.store(0, Ordering::Release);
        self.focus_queue.write().unwrap().clear();
        generation
    }

    /// Publish only under the session barrier, so a late read cannot refill
    /// a frontend that has already cleared for a different connection.
    fn publish_initial(&self, generation: u64, event: Event, sampled: Instant) -> bool {
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire)
        {
            return false;
        }
        self.broadcast(self.reconcile_event(event, &sync, sampled));
        true
    }

    /// Bus/subgroup/main display pairs arrive before channel labels or DSP.
    /// Publish each master immediately rather than waiting for assignments,
    /// solo banks, FX, or the rest of the master's snapshot to finish.
    async fn hydrate_master_levels(&self, driver: &str, generation: u64, deadline: Instant) {
        let drv = self.drv(driver);
        for bus in 0..drv.bus_count() {
            let sampled = Instant::now();
            let read = async { Ok::<_, anyhow::Error>((drv.get_bus_fader(bus).await?, drv.get_bus_mute(bus).await?)) };
            if let Ok(Ok((fader, muted))) = tokio::time::timeout(deadline.saturating_duration_since(sampled), read).await {
                if !self.publish_initial(generation, Event::Bus {
                    bus, fader, muted,
                    key: Some(openfader_core::strip_key("bus", bus.saturating_add(1) as u16)),
                }, sampled) { return; }
            }
            if Instant::now() >= deadline { return; }
        }
        for group in 0..drv.subgroup_count() {
            let sampled = Instant::now();
            let read = async { Ok::<_, anyhow::Error>((drv.get_subgroup_fader(group).await?, drv.get_subgroup_mute(group).await?)) };
            if let Ok(Ok((fader, muted))) = tokio::time::timeout(deadline.saturating_duration_since(sampled), read).await {
                if !self.publish_initial(generation, Event::Subgroup {
                    group, fader, muted, key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                }, sampled) { return; }
            }
            if Instant::now() >= deadline { return; }
        }
        let sampled = Instant::now();
        if let Ok((fader, muted)) = tokio::time::timeout(deadline.saturating_duration_since(sampled), async {
            (drv.get_main_fader().await.ok(), drv.get_main_mute().await.ok())
        }).await {
            self.publish_initial(generation, Event::Main { fader, muted }, sampled);
        }
    }

    /// Bounded sequential fill, field-major across the entire topology:
    /// levels -> masters -> mutes -> names/look -> EQ -> remaining fields.
    /// Provider scopes/capabilities still decide which leaves can be read.
    /// A five-second burst never blocks the command path; unfinished fields
    /// retain this order in the existing poller's queue, without a new timer.
    async fn hydrate_initial(&self, generation: u64) {
        let driver = self.active_driver();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(5);
        let mut list = self.static_poll_list(&driver);
        // Stable sorting preserves provider order within each channel/phase.
        list.sort_by_key(|(ch, id)| (initial_load_priority(id), *ch));
        let mut reads = 0usize;
        'fill: for phase in 0..=5 {
            for &(ch, id) in list.iter().filter(|(_, id)| initial_load_priority(id) == phase) {
                if self.connection_gen.load(Ordering::Acquire) != generation {
                    return;
                }
                // Fetch solo through the regular bank lane, after DSP.
                if id == "solo" || self.param_cache.read().unwrap()
                    .get(&ch).is_some_and(|values| values.contains_key(id))
                    || self.param_dropped.read().unwrap().contains(&(driver.clone(), id.to_string()))
                {
                    continue;
                }
                if Instant::now() >= deadline { break 'fill; }
                while Self::poller_yields(self.writes_in_flight.load(Ordering::Relaxed)) {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    if self.connection_gen.load(Ordering::Acquire) != generation { return; }
                    if Instant::now() >= deadline { break 'fill; }
                }
                let started = Instant::now();
                let Ok(result) = tokio::time::timeout(
                    deadline.saturating_duration_since(started), self.read_param(&driver, StripId(ch), id)
                ).await else { break 'fill; };
                if self.connection_gen.load(Ordering::Acquire) != generation { return; }
                match result {
                    Ok(value) => {
                        self.note_read_ok(&driver, ch, id);
                        self.apply_received(ch, id, &value, Instant::now(), Some(started));
                    }
                    Err(e) => { self.note_read_error(&driver, ch, id, &e); }
                }
                reads += 1;
            }
            // Replay each completed phase from verified cache only. Individual
            // reads already stream; this also covers attach/initial-caps gaps.
            for event in self.params_snapshot() {
                if !self.publish_initial(generation, event, start) { return; }
            }
            if phase == 0 {
                self.hydrate_master_levels(&driver, generation, deadline).await;
                eprintln!("engine: {driver} initial levels ready in {:?}", start.elapsed());
            }
        }
        // Do not strand unfilled EQ behind a strip-major background sweep.
        // Preserve explicit focus ahead of this continuation and deduplicate
        // against it. The regular poll budget/fairness/write priority applies.
        let sync = self.sync_state.lock().unwrap();
        if self.connection_gen.load(Ordering::Acquire) != generation
            || self.reads_suspended.load(Ordering::Acquire) { return; }
        let cache = self.param_cache.read().unwrap();
        let dropped = self.param_dropped.read().unwrap();
        let mut queue = self.focus_queue.write().unwrap();
        for (ch, id) in list {
            if id != "solo"
                && !cache.get(&ch).is_some_and(|v| v.contains_key(id))
                && !dropped.contains(&(driver.clone(), id.to_string()))
                && !queue.iter().any(|(d, c, p, _)| d == &driver && *c == ch && p == id)
            {
                queue.push_back((driver.clone(), ch, id.to_string(), String::new()));
            }
        }
        drop(queue);
        drop(dropped);
        drop(cache);
        drop(sync);
        eprintln!("engine: {driver} initial fill {:?} ({reads} reads); remainder streams via poller", start.elapsed());
    }

    /// Wake the fill worker for one [`Engine::deferred_fill`] pass
    /// (coalesced by generation, see [`Engine::ensure_tasks`]).
    fn kick_fill(&self) {
        self.fill_connection_gen.store(self.connection_gen.load(Ordering::Acquire), Ordering::Release);
        self.fill_gen.fetch_add(1, Ordering::AcqRel);
        self.fill_notify.notify_one();
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
                    values: values.iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k.clone(), v.clone())).collect(),
                    key: openfader_core::strip_key_for_index(&topo, ch as usize),
                    // Snapshot of the verified-only cache: every emitted key
                    // is explicitly marked `verified`. Absent/unknown is
                    // NEVER verified, the field is optional only for
                    // backward compatibility with old frames.
                    authority: Some(
                        values
                            .keys()
                            .filter(|k| !k.starts_with('_'))
                            .map(|k| (k.clone(), "verified".to_string()))
                            .collect(),
                    ),
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
            .filter(|(_, id)| {
                !id.starts_with('_')
                    && *id != "rtnsw"
                    && !dropped.contains(&(driver.to_string(), id.to_string()))
            })
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
                // param, a solosw bank has no per-strip query address,
                // so it can never enter `caps`): strips carrying the
                // extra id re-read the bank via `get_solo`
                // (`read_param` routes it). Appended to the strip's own block, so the list
                // stays strip-major. This is the desk→app backstop for
                // solos the live ingest never saw (attach gaps, dropped
                // broadcasts); the ingest is the fast path.
                for id in self.drv(driver).essentials_extra_ids(fam.kind) {
                    out.push((s as u16, id));
                }
            }
        }
        for (ch, id) in self.drv(driver).static_extra_rows(&topo) {
            out.push((ch, id));
        }
        out.sort_by_key(|(ch, _)| *ch);
        self.poll_static.write().unwrap().insert(driver.to_string(), out.clone());
        out
    }

    /// Move `chs` to the front of the poll queue (the `focus` command,
    /// attributed to `writer`). Most-recent-first: pending entries for the
    /// focused strips are dropped (a new focus supersedes older ones, this
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
            for id in ["pan", "balance", "name"].iter().rev() {
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

    fn note_read_ok(&self, driver: &str, ch: u16, param: &str) {
        self.read_failures.write().unwrap().remove(&(driver.to_string(), ch, param.to_string()));
    }

    fn note_read_error(&self, driver: &str, ch: u16, param: &str, error: &anyhow::Error) {
        if !is_unsupported_error(error) { return; }
        let mut failures = self.read_failures.write().unwrap();
        let (strikes, retry) = failures.entry((driver.to_string(), ch, param.to_string())).or_insert((0, Instant::now()));
        *strikes = strikes.saturating_add(1);
        if !is_timeout_error(error) || *strikes >= POLL_TIMEOUT_STRIKES {
            *retry = Instant::now() + Duration::from_secs(15);
            eprintln!("engine: {driver} strip {ch} {param}: {error}; retry in 15s (capability retained)");
        }
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
        if self.read_failures.read().unwrap().get(&(driver.to_string(), ch.0, param.to_string())).is_some_and(|(_, retry)| *retry > Instant::now()) {
            anyhow::bail!("read in scoped retry cooldown");
        }
        let drv = self.drv(driver);
        if param == "_mute_group" { return drv.get_mute_group_verified(ch.0 as u8).await.map(ParamValue::Bool); }
        if let Some(group) = param.strip_prefix("_mg").and_then(|g| g.parse::<u8>().ok()) {
            return drv.get_mute_group_assign_verified(ch, group - 1).await.map(ParamValue::Bool);
        }
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
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        match self.read_param(driver, StripId(ch), param).await {
            Ok(pv) => {
                self.note_read_ok(driver, ch, param);
                if self.connection_gen.load(Ordering::Acquire) == generation {
                    self.apply_received(ch, param, &pv, Instant::now(), Some(started));
                }
            }
            Err(e) => {
                self.note_read_error(driver, ch, param, &e);
            }
        }
    }

    /// Handle one poll-read error: unknown/unsupported params drop from the
    /// poll set at once (rebroadcasting `caps` once); QUERY TIMEOUTS drop
    /// only after [`POLL_TIMEOUT_STRIKES`] consecutive timeouts with no
    /// success in between, one lost datagram on venue wifi must not evict
    /// a live param (eviction also flips `caps.unsupported`, hiding the
    /// control until reconnect). Everything else keeps polling silently.
    /// Returns true when the param was dropped.
    #[cfg(test)]
    async fn note_poll_error(&self, driver: &str, param: &str, e: &anyhow::Error) -> bool {
        if !is_unsupported_error(e) {
            return false;
        }
        if is_timeout_error(e) {
            let strikes = {
                let mut pending = self.poll_timeouts.write().unwrap();
                let c = pending
                    .entry((driver.to_string(), param.to_string()))
                    .or_insert(0);
                *c = c.saturating_add(1);
                *c
            };
            if strikes < POLL_TIMEOUT_STRIKES {
                return false;
            }
            self.poll_timeouts
                .write()
                .unwrap()
                .remove(&(driver.to_string(), param.to_string()));
        }
        let fresh = {
            let mut dropped = self.param_dropped.write().unwrap();
            dropped.insert((driver.to_string(), param.to_string()))
        };
        if fresh {
            self.broadcast(self.caps_message());
        }
        fresh
    }

    /// Clear consecutive-timeout strikes after a successful read: a desk
    /// that answers again is alive and must never be evicted.
    #[cfg(test)]
    fn note_poll_ok(&self, driver: &str, param: &str) {
        self.poll_timeouts
            .write()
            .unwrap()
            .remove(&(driver.to_string(), param.to_string()));
    }

    /// Re-read one param after a client write, caching the confirmed value.
    /// Returns the confirmed JSON value (`None` when the re-read fails, in
    /// which case the caller falls back to echoing the written value as
    /// `accepted`, see the `authority` field on [`Event::Param`]).
    ///
    /// Provenance firewall (R2 preset-truth contract): providers whose
    /// `get_param` cannot see past their own writes (see
    /// [`DynDriver::readback_confirms_writes`], DL32S reads the same push
    /// cache its writes populate, and the desk never echoes own writes)
    /// short-circuit to `None` WITHOUT reading. Their cache would agree
    /// with every write by construction, so treating that "agreement" as
    /// confirmation would launder accepted-but-unverified writes into
    /// verified state (and let preset recall count desk clamps/failures as
    /// genuine). The engine `param_cache` therefore holds only
    /// desk-evidenced values: poll reads, desk pushes, and re-reads from
    /// providers with genuine readback. Write fallbacks never touch it.
    ///
    /// The desk applies writes with a delay of tens of ms: an immediate
    /// re-read usually returns the PRE-write value (owner-rig measured on
    /// XR18 FW 1.20, every clip of `fader` echo read back the previous
    /// value). A re-read that disagrees with what was just written is
    /// therefore treated as that stale pre-image: it is NOT cached (it
    /// would regress the broadcast-confirmed value) and the caller echoes
    /// the written value instead; the desk's own change broadcast confirms
    /// within ~100 ms. Floats agree within the desk's ~10-bit quantization
    /// step; bools/text must match exactly.
    ///
    /// Superseded-entry eviction (R2 preset-truth contract): every `None`
    /// path evicts the pre-write `param_cache` entry (unless a genuinely
    /// newer desk read landed after this write started, see
    /// [`evict_param_unless_fresh`](Self::evict_param_unless_fresh)), so
    /// `params_snapshot`/attach can never re-emit the stale pre-write value
    /// as verified proof after the frontend invalidated it. A genuine desk
    /// push re-inserts proof via `apply_received`.
    async fn refresh_param(&self, ch: StripId, param: &str, written: &Value) -> Option<Value> {
        let write_started = Instant::now();
        if !self.active_drv().readback_confirms_writes() {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        let driver = self.active_driver();
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let pv = match self.read_param(&driver, ch, param).await {
            Ok(pv) => pv,
            Err(_) => {
                self.evict_param_unless_fresh(ch, param, write_started);
                return None;
            }
        };
        if generation != self.connection_gen.load(Ordering::Acquire) {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        let v = param_value_json(&pv);
        if !refresh_agrees(&v, written) {
            self.evict_param_unless_fresh(ch, param, write_started);
            return None;
        }
        self.apply_received(ch.0, param, &pv, Instant::now(), Some(started));
        self.param_cache.read().unwrap().get(&ch.0).and_then(|values| values.get(param)).cloned()
    }

    /// Evict one superseded `param_cache` entry after an accepted
    /// (unverified) write, unless a genuinely newer desk read already
    /// re-proved it: `sync.received` timestamps newer than this write's
    /// start mean a poll/push landed after the write began, and that proof
    /// must survive. Hold the sync guard through cache deletion, matching
    /// the sync-then-cache lock order of `apply_received`/`reconcile_event`:
    /// a push that lands before the evict check is kept, one that lands
    /// after re-inserts via `apply_received`.
    fn evict_param_unless_fresh(&self, ch: StripId, param: &str, write_started: Instant) {
        let sync = self.sync_state.lock().unwrap();
        let fresh = sync
            .received
            .get(&(ch.0, param.to_string()))
            .is_some_and(|(at, _)| *at > write_started);
        if fresh {
            return;
        }
        if let Ok(mut cache) = self.param_cache.write() {
            if let Some(values) = cache.get_mut(&ch.0) {
                values.remove(param);
                if values.is_empty() {
                    cache.remove(&ch.0);
                }
            }
        }
    }

    /// Capture read (R2 preset-truth contract): desk-verified live read
    /// first; on a miss, honest continuity for the display-basic leaves
    /// only (`fader`/`mute`/`name` via the inclusive display getters, which
    /// include own writes without claiming proof). Every other param returns
    /// `None` on a miss, a capture omits unproven values rather than
    /// laundering the stale pre-write cache entry (which the accepted write
    /// already evicted) as current desk state. Continuity values recorded
    /// here carry no provenance in the stored scene; the recall path re-emits
    /// hardware echoes as `accepted`.
    async fn capture_read_param(&self, driver: &str, ch: StripId, param: &str) -> Option<ParamValue> {
        if let Ok(pv) = self.read_param(driver, ch, param).await {
            return Some(pv);
        }
        let drv = self.drv(driver);
        match param {
            "fader" => drv.get_fader(ch).await.ok().map(ParamValue::Float),
            "mute" => drv.get_mute(ch).await.ok().map(ParamValue::Bool),
            "name" => drv.get_name(ch).await.ok().map(ParamValue::Text),
            _ => None,
        }
    }

    /// Pinned snapshot (r58 P2(2)): the bulk read and the param-plane
    /// fallback both bind to `pin_driver` + `generation`, with a fail-fast
    /// check after the bulk read and per strip in the fallback. The commit
    /// barrier re-validates before publish in any case.
    async fn active_snapshot_on(
        &self,
        pin_driver: &str,
        generation: u64,
    ) -> anyhow::Result<openfader_core::MixerSnapshot> {
        let driver = pin_driver.to_string();
        match self.drv(&driver).snapshot().await {
            Ok(snapshot) => {
                anyhow::ensure!(
                    self.connection_gen.load(Ordering::Acquire) == generation,
                    "desk changed during scene capture"
                );
                anyhow::ensure!(
                    !self.reads_suspended.load(Ordering::Acquire),
                    "desk session changed during scene capture (reads suspended)"
                );
                Ok(snapshot)
            }
            Err(first) => {
                if !self.drv(&driver).param_plane_snapshot() {
                    return Err(first);
                }
                let mut snapshot = openfader_core::MixerSnapshot::default();
                for family in self.driver_topology(&driver) {
                    for index in family.first..family.first + family.count {
                        let id = StripId(index as u16);
                        let (fader, mute, name) = tokio::join!(
                            self.capture_read_param(&driver, id, "fader"),
                            self.capture_read_param(&driver, id, "mute"),
                            self.capture_read_param(&driver, id, "name"),
                        );
                        let (
                            Some(ParamValue::Float(fader)),
                            Some(ParamValue::Bool(mute)),
                            Some(ParamValue::Text(name)),
                        ) = (fader, mute, name)
                        else {
                            anyhow::bail!("{driver}: no snapshot values for strip {index}");
                        };
                        anyhow::ensure!(
                            self.connection_gen.load(Ordering::Acquire) == generation,
                            "desk changed during scene capture"
                        );
                        anyhow::ensure!(
                            !self.reads_suspended.load(Ordering::Acquire),
                            "desk session changed during scene capture (reads suspended)"
                        );
                        snapshot.strips.push(openfader_core::ChannelStrip {
                            id,
                            name,
                            fader,
                            mute,
                            pan: 0.5,
                        });
                    }
                }
                Ok(snapshot)
            }
        }
    }

    /// Main fader of the active driver, `None` when the driver has none.
    async fn active_main_fader(&self) -> Option<f32> {
        let driver = self.active_driver();
        self.main_fader_on(&driver).await
    }

    /// Pinned main-fader read (r58 P2(2)): the capture's desk, not
    /// whatever is active when the await lands.
    async fn main_fader_on(&self, driver: &str) -> Option<f32> {
        self.drv(driver).get_main_fader().await.ok()
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
    /// a [`ClientHandle`] (the serve inbound task owns none, the outbound
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
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        // Probe connects on success, so its caps/link/devices must survive
        // the generation change just like an explicit connection command.
        let connecting = matches!(cmd, Cmd::Connect { .. } | Cmd::ConnectDevice { .. } | Cmd::Probe { .. } | Cmd::Disconnect | Cmd::Offline { .. });
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
        let sync = self.sync_state.lock().unwrap();
        if !connecting && generation != self.connection_gen.load(Ordering::Acquire) { return; }
        for event in out { self.broadcast(self.reconcile_event(event, &sync, started)); }
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
                // (including when the re-read races the desk's apply lag 
                // see `refresh_param`). The echo carries readback
                // provenance: `verified` when the desk confirmed it,
                // `accepted` when the write landed without readback proof
                // (DL32S always: no echo, no independent query).
                let (value, authority) = match self
                    .refresh_param(StripId(ch), "fader", &json!(value))
                    .await
                {
                    Some(v) => (v, Some("verified".to_string())),
                    None => (json!(value), Some("accepted".to_string())),
                };
                out.push(Event::Param {
                    ch,
                    field: "fader".to_string(),
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
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
                // Confirmed-value echo (same rationale as `fader` above, with
                // the same `verified`/`accepted` provenance on the echo).
                let (value, authority) = match self
                    .refresh_param(StripId(ch), "mute", &json!(muted))
                    .await
                {
                    Some(v) => (v, Some("verified".to_string())),
                    None => (json!(muted), Some("accepted".to_string())),
                };
                out.push(Event::Param {
                    ch,
                    field: "mute".to_string(),
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            // LR main fader (all drivers via `get_main_fader`; `None` in
            // state when the driver has none). Not strip-locked: locks pin
            // per-channel strips.
            Cmd::MainFader { value } => {
                let value = openfader_core::validate_fader(value)?;
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
                // `main` echo (R2 display continuity): the inclusive getters
                // already carry own writes, so `state` + this frame keep a
                // new attach continuous. No fabrication: the fader IS the
                // submitted value, and `main` frames carry no `authority`
                // field, see the `param` event for verified values.
                self.push_state_with_by(writer, out).await;
                let muted = self.active_drv().get_main_mute().await.ok();
                out.push(Event::Main {
                    fader: Some(value),
                    muted,
                });
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
                // `main` echo (same display-continuity rationale as above).
                let fader = self.active_drv().get_main_fader().await.ok();
                out.push(Event::Main {
                    fader,
                    muted: Some(muted),
                });
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
                // clearsolo` clears it, XR18 FW 1.20, 2026-10-18.)
                //
                // Cache alongside the echo (see `apply_inbound`): the poll
                // diff then stays quiet instead of re-emitting this state.
                if !self.active_drv().is_hardware() { self.apply_inbound(ch, "solo", &ParamValue::Bool(soloed)); }
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
                // re-read fails, or when it races the desk's apply lag
                // (see `refresh_param`), so the echo still lands. Provenance
                // rides along (`verified` on desk agreement, `accepted`
                // otherwise); frontends must not count `accepted` as recall
                // confirmation.
                let refreshed = self.refresh_param(StripId(ch), &field, &value).await;
                let (value, authority) = match refreshed {
                    Some(v) => (v, Some("verified".to_string())),
                    None => (value, Some("accepted".to_string())),
                };
                if let Some(from) = from {
                    self.offline_note(ch, &field, from, value.clone()).await;
                }
                out.push(Event::Param {
                    ch,
                    field,
                    value,
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(ch),
                    authority,
                });
                self.push_state_with_by(writer, out).await;
                Ok(None)
            }
            Cmd::SpectrumTarget { ch } => {
                if ch.is_some() && self.reads_suspended.load(Ordering::Acquire) {
                    anyhow::bail!("RTA unavailable while disconnected");
                }
                if let Some(ch) = ch {
                    anyhow::ensure!((ch as usize) < self.active_drv().strip_count(), "RTA strip out of range");
                }
                self.active_drv().set_spectrum_target(writer, ch.map(StripId)).await?;
                Ok(None)
            }
            Cmd::Focus { ch, chs } => {
                // Poll-priority hint: either `{"cmd":"focus","ch":N}` or
                // `{"cmd":"focus","chs":[...]}`. Entries may be indices or
                // stable strip keys (`ch/01`, …, `main`; `mix/..` accepted).
                // Valid channels move to the front of the poll queue
                // (re-read at the normal budget, then round-robin resumes);
                // unknown/out-of-range channels are ignored with an `error`
                // event, never a disconnect.
                let mut want: Vec<u16> = Vec::new();
                match (ch, chs) {
                    (_, Some(list)) => {
                        want = self.resolve_focus_list(&list)?;
                    }
                    (Some(n), None) => want.push(n),
                    (None, None) => {
                        anyhow::bail!("focus needs \"ch\" (integer) or \"chs\" (array of integers)")
                    }
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
                // Named capture through the SAME serialized candidate
                // commit as the durable API: same tags, same rendered
                // collection frames, plus the status frame.
                let action = LibraryAction::Capture { show, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.shows_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
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
                // Named-show recall provenance (same rule as numbered
                // recall): the sim model applies writes to real in-memory
                // state (`verified`); hardware fades are write-ACK only
                // (`accepted`, no per-param readback on this path).
                let authority = || {
                    Some(
                        if self.active_drv().is_hardware() {
                            "accepted"
                        } else {
                            "verified"
                        }
                        .to_string(),
                    )
                };
                for s in &scene.strips {
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "fader".to_string(),
                        value: json!(s.fader),
                        by: Some(writer.to_string()),
                        key: None,
                        authority: authority(),
                    });
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "mute".to_string(),
                        value: json!(s.mute),
                        by: Some(writer.to_string()),
                        key: None,
                        authority: authority(),
                    });
                    for (field, val) in &s.params {
                        out.push(Event::Param {
                            ch: s.strip.0,
                            field: field.clone(),
                            value: param_value_json(val),
                            by: Some(writer.to_string()),
                            key: None,
                            authority: authority(),
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
                let action = LibraryAction::DeleteNamed { show, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.shows_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
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
                // Slim-caps completion (see `caps_slim_message`): attach
                // queues only the essentials lane so first paint never waits
                // for the ~30 KB catalogue. Every transport sends hello
                // right after subscribing, so the first hello unicasts the
                // full catalogue ahead of any detail UI; later hellos are
                // pure re-acks. Same frame shape as a param-drop caps
                // rebroadcast (idempotent replace for every consumer).
                if self
                    .clients
                    .read()
                    .unwrap()
                    .get(writer)
                    .is_some_and(|e| !e.caps_full)
                {
                    self.unicast(writer, self.caps_message());
                    if let Some(entry) = self.clients.write().unwrap().get_mut(writer) {
                        entry.caps_full = true;
                    }
                }
                Ok(Some(Event::Hello {
                    binary_meters: self.client_binary_meters(writer),
                    taps: self.client_taps(writer),
                }))
            }
            // ================= R2 =================
            Cmd::RefreshDevices { request_id, hosts } => {
                let mut results = Vec::new();
                let error = if hosts.len() > 64 {
                    Some("Refresh accepts at most 64 hosts".to_string())
                } else {
                    let mut seen = HashSet::new();
                    let checks = hosts
                        .into_iter()
                        .filter(|host| seen.insert(host.trim().to_ascii_lowercase()))
                        .map(|addr| async move {
                            match tokio::time::timeout(
                                Duration::from_secs(3),
                                self.check_recent_host(addr.clone()),
                            )
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => DeviceCheck {
                                    addr,
                                    status: "no_reply".into(),
                                    device: None,
                                },
                            }
                        });
                    results = futures::future::join_all(checks).await;
                    // Remove verified hostname aliases before canonical upsert.
                    {
                        let mut devices = self.devices.write().unwrap();
                        for result in &results {
                            if let Some(row) = &result.device {
                                devices.retain(|d| d.ip != result.addr || d.id == row.id);
                            } else {
                                for device in
                                    devices.iter_mut().filter(|d| d.ip == result.addr.trim())
                                {
                                    device.state = "unverified".into();
                                }
                            }
                        }
                    }
                    self.merge_device_rows(results.iter().filter_map(|r| r.device.clone()));
                    None
                };
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                self.unicast(writer, self.devices_message());
                out.push(self.devices_message());
                Ok(Some(Event::DeviceOperationComplete {
                    request_id,
                    operation: "refresh".into(),
                    results,
                    error,
                }))
            }
            Cmd::DiscoverDevices { request_id } => {
                let display = |k: &str, m: &str| self.drv(k).display_name(m);
                let (rows, error) = match tokio::time::timeout(
                    Duration::from_secs(4),
                    self.scanner.scan(&display, Duration::from_secs(2)),
                )
                .await
                {
                    Ok(rows) => (rows, None),
                    Err(_) => (Vec::new(), Some("Discovery timed out".to_string())),
                };
                let results = rows
                    .iter()
                    .map(|row| DeviceCheck {
                        addr: row.ip.clone(),
                        status: "available".into(),
                        device: Some(row.clone()),
                    })
                    .collect();
                self.merge_device_rows(rows);
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                self.unicast(writer, self.devices_message());
                out.push(self.devices_message());
                Ok(Some(Event::DeviceOperationComplete {
                    request_id,
                    operation: "discover".into(),
                    results,
                    error,
                }))
            }
            Cmd::Scan => {
                // Real LAN discovery behind the trait (fake in tests),
                // on EVERY driver: a sim-backed relay has no desks of its
                // own, so scanning from sim must run the same broadcast
                // discovery the phone uses, never the design seed rows.
                //
                // Broadcast discovery and remembered-address verification
                // run CONCURRENTLY: the UI budget is ~10 s, and serialising
                // them (2 s broadcast, then one ~1 s probe per remembered
                // desk) runs out of budget with two desks.
                //
                // Remembered desks (previous connects + manual probes):
                // (a) directed unicast verify of each address (broadcast
                // never crosses subnets) with the linked-desk fast path,
                // (b) ONE bounded /24 sweep per distinct remembered subnet
                // so a desk whose DHCP lease moved, or a sibling desk on
                // that subnet, is re-found across the router. Sweeps run
                // in parallel and are capped, so they never delay the
                // broadcast rows past the scan budget.
                //
                // The list is still REPLACED: an address that verifies
                // nothing adds no row; stale entries are never kept.
                let driver = self.active_driver();
                let remembered: Vec<(String, String)> = self
                    .desk_addrs
                    .read()
                    .unwrap()
                    .iter()
                    .map(|(d, a)| (d.clone(), a.clone()))
                    .collect();

                let display = |k: &str, m: &str| self.drv(k).display_name(m);
                let scan_fut = self.scanner.scan(&display, Duration::from_secs(2));
                let verify_fut = async {
                    // (a) Linked desk + directed verify, all in parallel.
                    // Each item is capped: a connected-but-silent driver
                    // answers nothing and must not stretch the scan.
                    let active_linked = async {
                        if self.drv(&driver).is_hardware() {
                            tokio::time::timeout(
                                SCAN_VERIFY_TIMEOUT,
                                self.linked_desk_device(&driver),
                            )
                            .await
                            .ok()
                            .flatten()
                        } else {
                            None
                        }
                    };
                    let mut probes = Vec::new();
                    for (drv_name, ip) in &remembered {
                        // Hardware desks only: a sim "address" is not a
                        // desk (sim ignores it), so nothing to verify.
                        if !self.drv(drv_name).is_hardware() {
                            continue;
                        }
                        probes.push(async move {
                            tokio::time::timeout(SCAN_VERIFY_TIMEOUT, async {
                                if let Some(linked) = self.linked_desk_device(drv_name).await {
                                    return Some(linked);
                                }
                                let table = self.drv(drv_name).probe_table()?;
                                self.probe_desk(table, ip).await
                            })
                            .await
                            .ok()
                            .flatten()
                        });
                    }

                    // (b) Bounded /24 re-find sweep, one plan per distinct
                    // remembered /24 (specs merged when two drivers share
                    // it), all plans in parallel.
                    let sweep_fut = async {
                        use openfader_transport::discovery::{
                            subnet24, sweep_known_subnet, ProbeSpec, PROBE_TABLE,
                        };
                        let mut plans: Vec<(std::net::Ipv4Addr, Vec<ProbeSpec>)> = Vec::new();
                        for (drv_name, ip) in &remembered {
                            if !self.drv(drv_name).is_hardware() {
                                continue;
                            }
                            let Some(table) = self.drv(drv_name).probe_table() else {
                                continue;
                            };
                            // Hostname addresses have no /24 to sweep; the
                            // directed verify above still covers them.
                            let Ok(seed) = ip.parse::<std::net::Ipv4Addr>() else {
                                continue;
                            };
                            let specs: Vec<ProbeSpec> = PROBE_TABLE
                                .iter()
                                .filter(|s| s.driver == table)
                                .copied()
                                .collect();
                            if specs.is_empty() {
                                continue;
                            }
                            let prefix = subnet24(seed);
                            match plans.iter_mut().find(|(p, _)| *p == prefix) {
                                Some((_, existing)) => {
                                    for s in specs {
                                        if !existing
                                            .iter()
                                            .any(|e| e.port == s.port && e.payload == s.payload)
                                        {
                                            existing.push(s);
                                        }
                                    }
                                }
                                None => plans.push((prefix, specs)),
                            }
                        }
                        let sweeps = plans.into_iter().map(|(prefix, specs)| async move {
                            let budget = openfader_transport::discovery::SWEEP_TIMEOUT
                                + Duration::from_millis(300);
                            match tokio::time::timeout(
                                budget,
                                sweep_known_subnet(
                                    &specs,
                                    std::net::IpAddr::V4(prefix),
                                    openfader_transport::discovery::SWEEP_TIMEOUT,
                                ),
                            )
                            .await
                            {
                                Ok(Ok(devs)) => devs
                                    .into_iter()
                                    .map(|d| {
                                        let name = self.drv(d.driver).display_name(&d.model);
                                        device_row(d, "manual", name)
                                    })
                                    .collect(),
                                _ => Vec::new(), // a slow sweep is dropped, never blocks
                            }
                        });
                        let mut rows: Vec<DeviceDesc> = Vec::new();
                        for group in futures::future::join_all(sweeps).await {
                            for row in group {
                                push_row(&mut rows, row);
                            }
                        }
                        rows
                    };

                    let (linked, probed, swept) = tokio::join!(
                        active_linked,
                        futures::future::join_all(probes),
                        sweep_fut
                    );
                    let mut rows: Vec<DeviceDesc> = Vec::new();
                    if let Some(row) = linked {
                        push_row(&mut rows, row);
                    }
                    for row in probed.into_iter().flatten() {
                        push_row(&mut rows, row);
                    }
                    for row in swept {
                        push_row(&mut rows, row);
                    }
                    rows
                };

                let (broadcast_rows, verified_rows) = tokio::join!(scan_fut, verify_fut);
                let mut merged = broadcast_rows;
                for row in verified_rows {
                    push_row(&mut merged, row);
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
                // Discovered-device dial carries an explicit port when the
                // driver answers there (the row already proved it, so the
                // bare-host sweep is skipped). Other drivers pass through
                // untouched.
                let dial = self.drv(claimed).dial_addr(&entry.ip);
                self.connect(claimed, Some(&dial)).await?;
                self.set_link_connected(Some(&id), &entry.ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Probe { addr } => {
                // Manual add-by-IP: verify, then CONNECT immediately, the
                // user typed the address to use this desk, not to file it in
                // a list and tap Connect again. The embedded app boots on
                // sim and the user has never switched drivers, so sim probes
                // with the aliased probe table (xair) and the verified row's
                // id/name claims the hardware driver below. A driver with no
                // probe table is an honest error, never a blind row.
                let driver = self.active_driver();
                let Some(table_driver) = self.drv(&driver).probe_table() else {
                    anyhow::bail!(
                        "probe needs a driver with a probe table (relay runs {driver}) — switch drivers first"
                    );
                };
                let Some(found) = self.probe_desk(table_driver, &addr).await else {
                    anyhow::bail!(
                        "no reply from {addr} — check the IP (desks on another subnet never answer broadcast; unicast to this address got nothing back)"
                    );
                };
                // The verified row is visible even if the connect fails.
                {
                    let mut devs = self.devices.write().unwrap();
                    if !devs.iter().any(|d| d.ip == found.ip) {
                        devs.push(found.clone());
                    }
                }
                // Remember + persist the verified address under the table's
                // driver so later scans (and a restart) re-verify it by
                // directed unicast and re-find its /24 instead of dropping
                // the row, broadcast never crosses subnets.
                self.remember_desk(table_driver, &found.ip);
                // Claim the driver exactly like Cmd::ConnectDevice.
                let mut claimed = "sim";
                for name in &self.driver_order {
                    if self.drv(name).claims_device(&found.id, &found.model) {
                        claimed = name.as_str();
                        break;
                    }
                }
                match self.connect(claimed, Some(&found.ip)).await {
                    Ok(()) => {
                        self.set_link_connected(Some(&found.id), &found.ip).await;
                        self.post_switch(writer, out, false).await?;
                        out.push(self.devices_message());
                        Ok(None)
                    }
                    Err(e) => {
                        // Verified but unusable: keep the row, leave the old
                        // link alone, and tell THIS client why (a plain `?`
                        // would drop the devices frame we just queued).
                        out.push(self.devices_message());
                        self.unicast(
                            writer,
                            Event::Error {
                                message: format!(
                                    "{} answered but the connect failed: {e}",
                                    found.ip
                                ),
                                locked_by: None,
                                ch: None,
                            },
                        );
                        Ok(None)
                    }
                }
            }
            Cmd::ForgetSavedMixers { request_id } => {
                // Pure-local saved-mixer forget: no driver/query/connect/
                // disconnect/probe/scan/native-view work, no broadcasts —
                // the correlated `device_operation_complete` ack is the
                // unicast direct reply (`apply_cmd` unicasts it).
                Ok(Some(self.forget_saved_mixers(request_id)))
            }
            Cmd::Disconnect => {
                // A missing readback cannot prevent local disconnect cleanup.
                let _ = self.active_drv().set_spectrum_target("", None).await;
                self.invalidate_connection();
                self.reads_suspended.store(true, Ordering::Release);
                let _ = self.active_drv().disconnect().await;
                self.param_cache.write().unwrap().clear();
                self.fx_cache.write().unwrap().clear();
                // Drop the meter subscription guard so the next connect
                // resubscribes from clean state (see `connect`).
                *self.meter_guard.write().unwrap() = None;
                *self.meter_driver.write().unwrap() = None;
                self.health_strikes.store(0, Ordering::Release);
                {
                    let mut link = self.link.write().unwrap();
                    link.state = "disconnected".to_string();
                }
                out.push(self.link_message_cached());
                // The desk is gone: native slots read unavailable (the
                // driver reset its view on disconnect), never stale.
                out.push(self.native_snapshots_message());
                self.record_native_view();
                Ok(None)
            }
            Cmd::Info => Ok(Some(self.info_message().await?)),
            Cmd::SceneSave { n, name } => {
                // Numbered save through the SAME serialized candidate
                // commit: capture, validate, durably commit, then swap.
                let action = LibraryAction::Save { n, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneRecall { n, fade_ms, scope } => {
                policy.allow_recall()?;
                let n = validate_scene_n(n)?;
                let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
                // r58 P2(1): the numbered recall holds the existing async
                // `lib_lock` from the slot clone through the current-marker
                // publication, so a racing Clear/Delete/Reset (which commit
                // under the same lock) cannot interleave between the clone
                // and the marker and resurrect `current` on a deleted slot.
                // The fade itself runs under the lock — mutations waiting
                // out a recall fade is the accepted serialization (no new
                // audio controls). No `std` lock is held across `.await`;
                // write provenance and audio-echo ordering below are exact.
                let _lib_guard = self.lib_lock.lock().await;
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
                let action = LibraryAction::Rename { n, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneDelete { n } => {
                let action = LibraryAction::Delete { n };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneClear => {
                let action = LibraryAction::ClearNumbered;
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::LocalLibraryStatus => Ok(Some(self.local_library_status_message())),
            Cmd::LocalLibraryMutate {
                request_id,
                expected_revision,
                action,
            } => {
                self.library_mutate(request_id, expected_revision, action, writer, out)
                    .await;
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
                if drv.fx_flavor() == FxFlavor::Design {
                    openfader_core::fx::fx_type_meta(&fx_type)
                        .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
                } else if drv.fx_flavor() == FxFlavor::Numbered {
                    // Desk-native rack: names come from the desk type
                    // table behind `caps.fx`, the short code (`id`, e.g.
                    // `HALL`) or the full display name (`label`, e.g.
                    // `Hall Reverb`); the UI sends the code. Unknown here
                    // means unknown there.
                    if drv.fx_type_index_of(&fx_type).is_none() {
                        anyhow::bail!("unknown fx type {fx_type:?}");
                    }
                } else {
                    anyhow::bail!(
                        "{driver}: fx type select not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = drv.fx_get_type(slot).unwrap_or_default();
                drv.fx_set_type(slot, &fx_type).await?;
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
                self.invalidate_connection();
                self.reads_suspended.store(false, Ordering::Release);
                *self.active.write().unwrap() = "sim".to_string();
                self.health_strikes.store(0, Ordering::Release);
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
                let value = openfader_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                // Preserve the sibling for read failures. As in
                // `refresh_param`, a disagreeing immediate read can be the
                // desk's pre-write value; echo the submitted value instead.
                // This bus display frame does not establish verified proof.
                let muted = self.active_drv().get_bus_mute(bus).await.ok();
                self.active_drv().set_bus_fader(bus, value).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.bus_message(bus).await, muted) {
                    (Ok(mut m), _) => {
                        match &mut m {
                            Event::Bus { fader, .. }
                                if !refresh_agrees(&json!(*fader), &json!(value)) =>
                            {
                                *fader = value;
                            }
                            _ => {}
                        }
                        m
                    }
                    (Err(_), Some(muted)) => Event::Bus {
                        bus,
                        fader: value,
                        muted,
                        key: Some(openfader_core::strip_key("bus", bus.saturating_add(1) as u16)),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            Cmd::BusMute { bus, muted } => {
                policy.allow_master_mute()?;
                let fader = self.active_drv().get_bus_fader(bus).await.ok();
                self.active_drv().set_bus_mute(bus, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.bus_message(bus).await, fader) {
                    (Ok(mut m), _) => {
                        // Bool agreement is exact; retain the accepted
                        // write on a stale read, without caching it as proof.
                        if let Event::Bus { muted: read, .. } = &mut m {
                            *read = muted;
                        }
                        m
                    }
                    (Err(_), Some(fader)) => Event::Bus {
                        bus,
                        fader,
                        muted,
                        key: Some(openfader_core::strip_key("bus", bus.saturating_add(1) as u16)),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            // R2 subgroup masters (0-based `group` over `0..subgroup_count()`).
            Cmd::SubgroupFader { group, value } => {
                let value = openfader_core::validate_fader(value)?;
                policy.allow_master_fader(value)?;
                let muted = self.active_drv().get_subgroup_mute(group).await.ok();
                self.active_drv().set_subgroup_fader(group, value).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.subgroup_message(group).await, muted) {
                    (Ok(m), _) => m,
                    (Err(_), Some(muted)) => Event::Subgroup {
                        group,
                        fader: value,
                        muted,
                        key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                    },
                    (Err(e), None) => return Err(e),
                });
                Ok(None)
            }
            Cmd::SubgroupMute { group, muted } => {
                policy.allow_master_mute()?;
                let fader = self.active_drv().get_subgroup_fader(group).await.ok();
                self.active_drv().set_subgroup_mute(group, muted).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(match (self.subgroup_message(group).await, fader) {
                    (Ok(m), _) => m,
                    (Err(_), Some(fader)) => Event::Subgroup {
                        group,
                        fader,
                        muted,
                        key: Some(format!("subgroup/{:02}", group.saturating_add(1))),
                    },
                    (Err(e), None) => return Err(e),
                });
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
                // Display-continuity echo: the inclusive getter already
                // carries own writes, so fall back to the submitted value
                // only when nothing was ever pushed (no fabrication: this IS
                // the submitted value, and these frames carry no `authority`
                // field, see the `param` event).
                let muted = self.active_drv().get_mute_group(group).await.unwrap_or(muted);
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
            // only). No desk echo, the desk ACKs by silence, so fan out
            // explicit unsoloed frames for every known-soloed strip and drop
            // them from the cache; otherwise the UI would stick on soloed.
            // (A surface clear arrives the same way, via `apply_inbound`
            // solosw broadcasts.)
            Cmd::ClearSolo => {
                self.active_drv().clear_solo().await?;
                let cleared: Vec<u16> = {
                    let mut sync = self.sync_state.lock().unwrap();
                    let mut cache = self.param_cache.write().unwrap();
                    let mut cleared = Vec::new();
                    for (ch, map) in cache.iter_mut() {
                        if map.get("solo") == Some(&json!(true)) {
                            cleared.push(*ch);
                            map.remove("solo");
                            // Attach must not replay superseded Solo(true).
                            // The false echo below is display continuity,
                            // not a desk-verified replacement cache entry.
                            sync.typed.remove(&format!("{ch}/solo"));
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
/// `fx`, `routing`, then one `params` frame per cached channel,
/// `native_snapshots`, then `local_library_status` in device mode only)
/// in that order, then every live broadcast. Unicast frames (`you` replies,
/// `list_shows` replies, denials, `recalled` reports, command errors) arrive
/// only in the offending/sender's mailbox.
///
/// Dropping the handle detaches the client (best-effort synchronous;
/// prefer an explicit [`detach`](ClientHandle::detach) so the roster update
/// and lock auto-release are guaranteed to run).
/// A client's live event mailbox: the receiving half of the engine
/// fan-out with latest-only realtime coalescing.
///
/// `recv`/`try_recv` mirror [`mpsc::UnboundedReceiver`], so adapters drain
/// exactly as before. Yielding a realtime frame clears that mailbox's
/// in-flight flag and promotes every coalesced slot back into the channel
/// (meters first, then per-key families in key order), so an actively
/// draining client observes every stream's latest frame and loses nothing
/// but superseded same-key frames. `len` covers both the queued channel
/// backlog and the coalesced slots (what an edge fan-out still owes this
/// client); control frames are always counted, never coalesced.
pub struct ClientMailbox {
    rx: mpsc::UnboundedReceiver<SharedEvent>,
    /// Promotes coalesced slots back into `rx` as the client drains (an
    /// extra sender on the same channel: unbounded sends never block).
    tx: mpsc::UnboundedSender<SharedEvent>,
    rt: Arc<std::sync::Mutex<RtSlots>>,
    queued: Arc<AtomicUsize>,
}

impl ClientMailbox {
    fn new(
        rx: mpsc::UnboundedReceiver<SharedEvent>,
        tx: mpsc::UnboundedSender<SharedEvent>,
        rt: Arc<std::sync::Mutex<RtSlots>>,
        queued: Arc<AtomicUsize>,
    ) -> Self {
        Self { rx, tx, rt, queued }
    }

    /// Next frame, waiting if empty (returns `None` once the engine has
    /// detached and the queue has drained).
    pub async fn recv(&mut self) -> Option<SharedEvent> {
        let frame = self.rx.recv().await?;
        self.after_yield(&frame);
        Some(frame)
    }

    /// Next frame without waiting.
    pub fn try_recv(&mut self) -> Result<SharedEvent, mpsc::error::TryRecvError> {
        let frame = self.rx.try_recv()?;
        self.after_yield(&frame);
        Ok(frame)
    }

    /// Frames this client is still owed: queued channel backlog plus
    /// coalesced realtime slots.
    pub fn len(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
            + self.rt.lock().map(|rt| rt.occupancy()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Account for one yielded frame; on realtime frames, clear in-flight
    /// and promote the coalesced latest frames behind it.
    fn after_yield(&mut self, frame: &SharedEvent) {
        self.queued.fetch_sub(1, Ordering::Relaxed);
        if rt_slot_key(frame).is_none() {
            return;
        }
        let pending = match self.rt.lock() {
            Ok(mut rt) => {
                rt.inflight = false;
                rt.take_all()
            }
            Err(_) => Vec::new(),
        };
        if pending.is_empty() {
            return;
        }
        let mut promoted = false;
        for frame in pending {
            if self.tx.send(frame).is_ok() {
                self.queued.fetch_add(1, Ordering::Relaxed);
                promoted = true;
            }
        }
        if promoted {
            if let Ok(mut rt) = self.rt.lock() {
                rt.inflight = true;
            }
        }
    }
}

pub struct ClientHandle {
    /// Server-side id (`client-N`); matches later `by` tags.
    pub id: String,
    engine: Arc<Engine>,
    /// Live event mailbox: an unbounded channel plus per-mailbox
    /// latest-only realtime slots (see [`RtSlots`]). Slow clients never
    /// apply backpressure, but they no longer accumulate meter-class frames
    /// without bound: while a realtime frame sits queued, newer `meters` /
    /// `taps` / `spectrum` / `audio_spectrum` / `gr` frames supersede the
    /// older same-key frame in the slots instead of queueing, so a stalled
    /// client holds at most one queued realtime frame plus one latest frame
    /// per realtime stream key. Every other frame (state changes, params,
    /// roster, locks, errors, …) queues reliably in order and is never
    /// dropped or coalesced. Each frame carries its wire JSON serialized
    /// once at broadcast time and shared across clients via `Arc` (see
    /// [`SharedEvent`]); the sweeper prunes detached mailboxes.
    pub events: ClientMailbox,
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

/// Write-through [`openfader_core::show::FadeDriver`]: numbered-scene recall
/// shares the single [`openfader_core::show::fade_strips`] implementation with
/// named-show recall (which fades through the driver blanket impl).
impl openfader_core::show::FadeDriver for Engine {
    async fn fade_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.drv_set_mute(strip, muted).await
    }

    async fn fade_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        // Inclusive display getter (continuity: own writes included, no proof
        // claimed), never the stale pre-write cache entry, which would
        // start a named-show fade from the wrong pre-move state.
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
                key: openfader_core::strip_key_for_index(&topo, s.id.0 as usize),
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
/// else, empty sim slots (`no value yet`), unpushed dl32s cache
/// (`no value pushed yet`), own-write-pending dl32s words with no desk
/// proof (`no desk-confirmed value yet`), `not connected`, transient
/// I/O, keeps polling.
fn is_unsupported_error(e: &anyhow::Error) -> bool {
    let m = format!("{e:#}");
    m.contains("not supported") || m.contains("unknown param") || m.contains("timed out")
}

/// Timeout-class poll errors (a subset of [`is_unsupported_error`]): the
/// desk never answered. Transient on wifi, [`Engine::note_poll_error`]
/// evicts only after [`POLL_TIMEOUT_STRIKES`] in a row.
fn is_timeout_error(e: &anyhow::Error) -> bool {
    format!("{e:#}").contains("timed out")
}

/// Connection-fill priority only; steady-state polling retains its lanes.
fn initial_load_priority(id: &str) -> u8 {
    match id {
        "fader" => 0,
        "mute" => 1,
        "name" => 2,
        "icon" | "color" => 3,
        id if id.starts_with("eq") || id.starts_with("hpf_") => 4,
        _ => 5,
    }
}

/// R2 essentials-lane ids: strip basics + pan + look. Everything deeper
/// rides focus (visible strips) or the background sweep, see
/// [`Engine::core_poll_list`].
fn is_essentials_id(id: &str) -> bool {
    matches!(id, "fader" | "mute" | "name" | "pan" | "balance" | "color")
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
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 2e-3,
            _ => false,
        },
        _ => read == written,
    }
}

/// Map a sim slot's positional param vector onto design param ids.
fn param_vec_to_map(fx_type: &str, values: &[f32]) -> BTreeMap<String, f32> {
    match openfader_core::fx::fx_type_meta(fx_type) {
        Some(meta) => meta
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id.to_string(),
                    values.get(i).copied().unwrap_or(p.default),
                )
            })
            .collect(),
        None => values
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("p{i}"), *v))
            .collect(),
    }
}


/// Parse a patch label (`Lcl 01`, `Net 12`, `USB 03`) into (group, 1-based
/// index). The design's unpatched tile (` `) and anything else → `None`.
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
/// wave, `name`+`pan` second, see [`Engine::request_focus`]). `first`
/// selects the fader/mute wave; `false` the name/pan wave.
fn is_core_id(id: &str, first: bool) -> bool {
    if first {
        id == "fader" || id == "mute"
    } else {
        id == "name" || id == "pan" || id == "balance"
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
    } else if id == "pan" || id == "balance" {
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

/// Known desk addresses file (`desks.json`) under the data dir.
fn desk_file(data_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join("desks.json"))
}

/// Load the remembered driver -> desk address map (best-effort: a missing
/// or corrupt file reads back empty, persistence must never block start).
/// Broadcast discovery never crosses subnets, so without this a manually
/// added desk vanishes from Scan after an app restart.
fn load_desks(data_dir: Option<&std::path::Path>) -> HashMap<String, String> {
    let Some(path) = desk_file(data_dir) else {
        return HashMap::new();
    };
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    if raw.trim().is_empty() {
        return HashMap::new();
    }
    let v: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let mut out = HashMap::new();
    if let Some(obj) = v.get("desks").and_then(Value::as_object) {
        for (driver, ip) in obj {
            let Some(ip) = ip.as_str() else {
                continue;
            };
            let ip = ip.trim();
            if !ip.is_empty() {
                out.insert(driver.clone(), ip.to_string());
            }
        }
    }
    out
}

/// Persist the remembered driver -> desk address map (best-effort:
/// failures are logged and swallowed — a read-only data dir must never
/// break connect/probe). Atomic temp-file + rename (no direct truncate)
/// so a racing `forget_saved_mixers` clear never sees a torn file.
/// Untouched by the local-library kernel, which owns `scenes.json`
/// alone.
fn persist_desks(data_dir: Option<&std::path::Path>, addrs: &HashMap<String, String>) {
    let Some(dir) = data_dir else {
        return;
    };
    match write_desks_atomic(dir, addrs) {
        Ok(DesksWriteOutcome::Committed) => {}
        Ok(DesksWriteOutcome::Uncertain(error)) | Err(error) => {
            eprintln!("engine: desks persist failed: {error}");
        }
    }
}

enum DesksWriteOutcome {
    Committed,
    Uncertain(String),
}

/// Atomic durable write of the remembered desk map (`desks.json`):
/// serialize, `create_new` a unique owned sibling temp
/// (`.desks.json.tmp-<pid>-<nanos>-<attempt>`), hold that handle through
/// `write_all` + `flush` + `sync_all`, rename over the target, then sync
/// the directory where supported. Pre-replace failures return `Err` with
/// map and disk preserved (temp cleanup errors are appended, never
/// silently dropped); the single post-replace failure (directory sync)
/// returns an explicit `Uncertain` outcome (the rename landed — no
/// rollback claim). No caller infers write provenance from error text.
fn write_desks_atomic(
    dir: &std::path::Path,
    addrs: &HashMap<String, String>,
) -> Result<DesksWriteOutcome, String> {
    let doc = json!({ "version": 1, "desks": addrs });
    let bytes = serde_json::to_vec_pretty(&doc)
        .map_err(|e| format!("saved mixers serialize failed: {e}"))?;
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Err(format!(
            "saved mixers dir {} not creatable: {e}",
            dir.display()
        ));
    }
    let target = dir.join("desks.json");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let mut temp_path: Option<PathBuf> = None;
    let mut temp_file: Option<std::fs::File> = None;
    for attempt in 0..8u64 {
        let candidate = dir.join(format!(".desks.json.tmp-{pid}-{nanos}-{attempt}"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(f) => {
                temp_path = Some(candidate);
                temp_file = Some(f);
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(format!(
                    "saved mixers temp {} not creatable: {e}",
                    candidate.display()
                ));
            }
        }
    }
    let (Some(temp), Some(mut f)) = (temp_path, temp_file) else {
        return Err("saved mixers temp file not creatable (name collision)".to_string());
    };
    let remove_temp = |temp: &std::path::Path| {
        if let Err(e) = std::fs::remove_file(temp) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return format!("; temp cleanup {} failed: {e}", temp.display());
            }
        }
        String::new()
    };
    if let Err(e) = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        f.write_all(&bytes)?;
        f.flush()?;
        f.sync_all()?;
        Ok(())
    })() {
        let tail = remove_temp(&temp);
        return Err(format!(
            "saved mixers temp write {} failed: {e}{tail}",
            temp.display()
        ));
    }
    drop(f);
    if let Err(e) = std::fs::rename(&temp, &target) {
        let tail = remove_temp(&temp);
        return Err(format!(
            "saved mixers rename {} -> {} failed: {e}{tail}",
            temp.display(),
            target.display()
        ));
    }
    if let Err(e) = sync_desks_dir(dir) {
        return Ok(DesksWriteOutcome::Uncertain(format!(
            "saved mixers dir sync {} failed (saved mixers may have cleared): {e}",
            dir.display()
        )));
    }
    Ok(DesksWriteOutcome::Committed)
}

#[cfg(unix)]
fn sync_desks_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let f = std::fs::File::open(dir)?;
    f.sync_all()
}

#[cfg(not(unix))]
fn sync_desks_dir(_dir: &std::path::Path) -> std::io::Result<()> {
    Ok(())
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
    fn dial_addr_skips_sweep_only_for_bare_provider_ips() {
        // Discovered-device dial: a bare IP gains an explicit port when
        // the provider answers there (the sweep is skipped); everything
        // else passes through untouched.
        let xair = openfader_proto_xair::XAirDriver::new();
        let sim = openfader_sim::SimDriver::new();
        assert_eq!(
            crate::drivers::DynDriver::dial_addr(&xair, "192.168.1.120"),
            "192.168.1.120:10024"
        );
        assert_eq!(
            crate::drivers::DynDriver::dial_addr(&xair, "192.168.1.120:10024"),
            "192.168.1.120:10024"
        );
        assert_eq!(
            crate::drivers::DynDriver::dial_addr(&xair, "desk.local"),
            "desk.local"
        );
        assert_eq!(
            crate::drivers::DynDriver::dial_addr(&sim, "192.168.1.120"),
            "192.168.1.120"
        );
    }

    #[test]
    fn poller_yield_rule() {
        // MR1 write priority: the poller skips its tick while ANY command
        // is inside apply (writes_in_flight > 0), and only then.
        assert!(!Engine::poller_yields(0));
        assert!(Engine::poller_yields(1));
        assert!(Engine::poller_yields(u64::MAX));
    }

    /// Remembered desk addresses survive a restart: `desks.json` under
    /// `data_dir` reloads into `desk_addrs` (scan re-verify + /24 sweep
    /// need it, broadcast never crosses subnets). Missing/no data dir and
    /// a corrupt file all degrade to empty, never to a boot failure.
    #[test]
    fn desks_persist_across_restart() {
        let dir = std::env::temp_dir().join(format!(
            "openfader-desks-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        {
            let engine = Engine::new(EngineConfig {
                data_dir: Some(dir.clone()),
                ..EngineConfig::default()
            });
            assert!(engine.desk_addrs.read().unwrap().is_empty());
            engine.remember_desk("xair", "192.168.1.120");
            assert!(dir.join("desks.json").exists(), "desk file written");
        }
        let engine = Engine::new(EngineConfig {
            data_dir: Some(dir.clone()),
            ..EngineConfig::default()
        });
        assert_eq!(
            engine
                .desk_addrs
                .read()
                .unwrap()
                .get("xair")
                .map(String::as_str),
            Some("192.168.1.120"),
            "restart reloads the remembered address"
        );
        // No data dir: in-memory only, never panics.
        let mem = Engine::new(EngineConfig::default());
        assert!(mem.desk_addrs.read().unwrap().is_empty());
        // Corrupt file: empty map, engine still boots.
        std::fs::write(dir.join("desks.json"), "{ not json").unwrap();
        let broken = Engine::new(EngineConfig {
            data_dir: Some(dir.clone()),
            ..EngineConfig::default()
        });
        assert!(broken.desk_addrs.read().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn new_param_id_shapes_need_no_allowlist() {
        // New CATALOGUE ids (lr_assign, solo_*, sendN_grpon, automix_weight,
        // fx_insert, snap/scope, routing src) flow through the generic strip
        // param path (`Cmd::Param` passes `field` straight to the driver 
        // no match-list gates it). These pure helpers must already classify
        // the new id shapes correctly so zero engine changes are needed:
        // `sendN_grpon` rides the send slot-number rule into the bus/FX
        // send recall groups; desk-global ids fall back to `config` (never
        // silently dropped from a partial recall).
        assert_eq!(send_slot_number("send1_grpon"), Some(1));
        assert_eq!(send_slot_number("send10_grpon"), Some(10));
        assert_eq!(send_slot_number("aux3_grpon"), Some(3));
        assert_eq!(send_slot_number("fx2_grpon"), Some(2));
        assert_eq!(send_slot_number("lr_assign"), None);
        assert_eq!(param_scope_group("send1_grpon"), "sends_bus");
        assert_eq!(param_scope_group("send1_tap"), "sends_bus");
        assert_eq!(param_scope_group("send7_grpon"), "sends_fx");
        assert_eq!(param_scope_group("send10_grpon"), "sends_fx");
        assert_eq!(param_scope_group("lr_assign"), "config");
        assert_eq!(param_scope_group("automix_weight"), "config");
        assert!(is_priority_id("send1_grpon"));
        assert!(!is_priority_id("lr_assign"));
        assert!(!is_priority_id("automix_weight"));
        assert_eq!(safe_order_key("send1_grpon").0, 7);
        assert_eq!(safe_order_key("lr_assign").0, 9);
        // Channel inserts (rig-verified 2026-10-20) need no allowlist either:
        // they fall back to the config recall group, drain on the background
        // lane (never the essentials/focus fast path, first paint stays
        // faders/names), and sort last in the safe recall order.
        assert_eq!(param_scope_group("insert_on"), "config");
        assert_eq!(param_scope_group("insert_sel"), "config");
        assert!(!is_priority_id("insert_on"));
        assert!(!is_priority_id("insert_sel"));
        assert!(!is_essentials_id("insert_on"));
        assert!(!is_essentials_id("insert_sel"));
        assert_eq!(safe_order_key("insert_on").0, 9);
        assert_eq!(safe_order_key("insert_sel").0, 9);
    }

    #[test]
    fn numbered_type_names_resolve_like_caps() {
        // `FxSetType` on a desk-native rack validates against the same
        // table `caps.fx` advertises (short code `id` or full display
        // `label`), not the design list: design names are unknown there.
        let xair = openfader_proto_xair::XAirDriver::new();
        let idx = |name: &str| crate::drivers::DynDriver::fx_type_index_of(&xair, name);
        assert_eq!(idx("HALL"), Some(0));
        assert_eq!(idx("VRM"), Some(7));
        assert_eq!(idx("PIT"), Some(60));
        assert_eq!(idx("Hall Reverb"), Some(0));
        assert_eq!(idx("Vintage Room"), Some(7));
        assert_eq!(idx("Stereo Pitch Shifter"), Some(60));
        assert_eq!(idx("Nope"), None);
        assert_eq!(idx("Hall"), None);
        assert_eq!(idx(""), None);
    }

    fn event_json(event: Event) -> Value {
        serde_json::to_value(event).unwrap()
    }

    /// Parse a mailbox frame's shared wire JSON (see [`SharedEvent`]):
    /// what the adapters put on the socket, byte-identical to
    /// [`event_json`] for the same event, without re-serializing.
    fn shared_json(frame: &SharedEvent) -> Value {
        serde_json::from_str(&frame.json).unwrap()
    }

    #[test]
    fn probe_cmd_parses() {
        let cmd: Cmd =
            serde_json::from_value(json!({"cmd": "probe", "addr": "192.168.1.120"})).unwrap();
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

    /// Mailbox coalescing harness: a detached entry + mailbox pair with no
    /// ticker attached, so the bound assertions are exact.
    fn stalled_pair() -> (ClientEntry, ClientMailbox) {
        let (tx, rx) = mpsc::unbounded_channel();
        let rt = Arc::new(std::sync::Mutex::new(RtSlots::default()));
        let queued = Arc::new(AtomicUsize::new(0));
        let entry = ClientEntry {
            mailbox: tx.clone(),
            rt: Arc::clone(&rt),
            queued: Arc::clone(&queued),
            label: "test".to_string(),
            connected_at: Instant::now(),
            last_write: None,
            policy: Policy::admin(),
            binary_meters: false,
            taps: false,
            caps_full: false,
        };
        (entry, ClientMailbox::new(rx, tx, rt, queued))
    }

    fn meters_frame(v: f32) -> SharedEvent {
        SharedEvent::new(Event::Meters { levels: vec![v] })
    }

    #[tokio::test]
    async fn stalled_mailbox_coalesces_meter_flood_to_latest() {
        let (entry, mut mailbox) = stalled_pair();
        for i in 0..50u32 {
            Engine::push_to(&entry, meters_frame(i as f32));
        }
        // Bound: one queued frame plus one slot, however long the stall.
        assert_eq!(mailbox.len(), 2, "flood must collapse to queued + slot");
        // The first delivery is the frame that was already queued; draining
        // it promotes the latest behind it.
        let first = mailbox.try_recv().expect("queued meters frame");
        assert!(
            matches!(&*first.event, Event::Meters { levels } if levels == &vec![0.0]),
            "queued frame is delivered first"
        );
        let mut last = first;
        while let Ok(frame) = mailbox.try_recv() {
            last = frame;
        }
        assert!(
            matches!(&*last.event, Event::Meters { levels } if levels == &vec![49.0]),
            "latest frame must survive the flood"
        );
        assert!(mailbox.is_empty(), "slots must drain, not linger");
    }

    #[tokio::test]
    async fn stalled_mailbox_never_drops_control_under_meter_flood() {
        let (entry, mut mailbox) = stalled_pair();
        for i in 0..20u16 {
            Engine::push_to(&entry, meters_frame(i as f32));
            Engine::push_to(
                &entry,
                SharedEvent::new(Event::Param {
                    ch: i,
                    field: "fader".to_string(),
                    value: json!(0.5),
                    key: None,
                    by: None,
                    authority: None,
                }),
            );
        }
        Engine::push_to(
            &entry,
            SharedEvent::new(Event::Error {
                message: "boom".to_string(),
                locked_by: None,
                ch: None,
            }),
        );
        let mut params = Vec::new();
        let mut errors = 0u32;
        let mut meters = Vec::new();
        while let Ok(frame) = mailbox.try_recv() {
            match &*frame.event {
                Event::Param { ch, .. } => params.push(*ch),
                Event::Error { .. } => errors += 1,
                Event::Meters { levels } => meters.push(levels.clone()),
                other => panic!("unexpected frame in drain: {}", other.tag()),
            }
        }
        assert_eq!(
            params,
            (0..20u16).collect::<Vec<_>>(),
            "control order must survive the flood"
        );
        assert_eq!(errors, 1, "errors are never coalesced");
        assert!(
            meters.len() <= 2,
            "meters collapse to queued + latest, got {}",
            meters.len()
        );
        assert_eq!(meters.last().unwrap(), &vec![19.0], "latest meters frame must survive");
        assert!(mailbox.is_empty());
    }

    #[tokio::test]
    async fn stalled_mailbox_keeps_latest_per_realtime_key() {
        let (entry, mut mailbox) = stalled_pair();
        let gr = |ch: u16, gate: f32| {
            SharedEvent::new(Event::Gr {
                ch,
                gate,
                comp: 0.0,
            })
        };
        // First frame queues; the rest collapse per channel key.
        Engine::push_to(&entry, gr(0, 1.0));
        Engine::push_to(&entry, gr(1, 1.0));
        Engine::push_to(&entry, gr(0, 2.0));
        Engine::push_to(
            &entry,
            SharedEvent::new(Event::Spectrum {
                ch: 3,
                bins: vec![0.5],
            }),
        );
        Engine::push_to(
            &entry,
            SharedEvent::new(Event::Taps {
                tap: "in".to_string(),
                levels: vec![0.25],
            }),
        );
        // 1 queued + 4 slots (gr/0, gr/1, spectrum/3, taps/in).
        assert_eq!(mailbox.len(), 5);
        let mut gates = Vec::new();
        let mut taps = 0u32;
        let mut spectra = 0u32;
        while let Ok(frame) = mailbox.try_recv() {
            match &*frame.event {
                Event::Gr { gate, .. } => gates.push(*gate),
                Event::Taps { .. } => taps += 1,
                Event::Spectrum { .. } => spectra += 1,
                other => panic!("unexpected frame in drain: {}", other.tag()),
            }
        }
        assert_eq!(gates, vec![1.0, 2.0, 1.0], "per-channel latest, queued first");
        assert_eq!((taps, spectra), (1, 1));
        assert!(mailbox.is_empty());
    }

    #[tokio::test]
    async fn caps_sim_lists_full_catalogue() {
        let engine = Engine::new(EngineConfig::default());
        let caps = event_json(engine.caps_message());
        assert_eq!(caps.get("type"), Some(&json!("caps")));
        assert_eq!(caps.get("driver"), Some(&json!("sim")));
        let params = caps.get("params").and_then(Value::as_array).unwrap();
        assert_eq!(params.len(), engine.registry("sim").len());
        let ids: Vec<&str> = params
            .iter()
            .map(|p| p.get("id").and_then(Value::as_str).unwrap())
            .collect();
        for def in openfader_core::params::CATALOGUE {
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
        assert_eq!(topo.len(), openfader_sim::SIM_TOPOLOGY.len());
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
        // Wave-1 verified set (see `openfader_proto_dl32s::DL32S_CAPABILITIES`).
        assert_eq!(
            engine.driver_capabilities("sim").len(),
            engine.registry("sim").len()
        );
        assert_eq!(
            engine.driver_capabilities("xair"),
            openfader_proto_xair::XAIR_CAPABILITIES
        );
        assert_eq!(
            engine.driver_capabilities("dl32s"),
            openfader_proto_dl32s::DL32S_CAPABILITIES
        );
        assert_eq!(engine.driver_capabilities("nope"), engine.driver_capabilities("sim"));
        assert_eq!(engine.driver_channels("xair"), 16);
        assert_eq!(engine.driver_channels("sim"), 32);
        // Strip totals (topology): sim 77, xair 36, dl32s 32.
        assert_eq!(engine.driver_strips("sim"), openfader_sim::SIM_STRIPS);
        assert_eq!(engine.driver_strips("xair"), openfader_proto_xair::XAIR_STRIPS);
        assert_eq!(engine.driver_strips("dl32s"), 32);
        assert_eq!(engine.driver_topology("sim").len(), openfader_sim::SIM_TOPOLOGY.len());
        assert_eq!(engine.driver_topology("xair").len(), openfader_proto_xair::XAIR_TOPOLOGY.len());
        assert_eq!(engine.driver_meter_taps("sim"), vec!["in", "post", "gr_gate", "gr_comp"]);
    }

    #[test]
    fn registries_merge_canonical_plus_provider_rows() {
        use openfader_core::params::ParamRegistry;
        let engine = Engine::new(EngineConfig::default());
        // Provider rows append without replacing the canonical table.
        for driver in ["sim", "xair", "dl32s"] {
            let reg = engine.registry(driver);
            assert_eq!(reg.len(), openfader_core::params::CATALOGUE.len() + engine.drv(driver).param_defs().len(), "{driver}");
            for def in openfader_core::params::CATALOGUE {
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
        assert_eq!(reg.len(), openfader_core::params::CATALOGUE.len());
    }

    /// Resume path: `push_snapshot` queues the attach frame set into an
    /// already-attached client's mailbox (fresh state, no `you`, no roster
    /// churn, no lock/role change). This is what the native shell calls when
    /// the app comes back from the background, so a UI that missed minutes
    /// of frames catches up in one batch.
    #[tokio::test]
    async fn push_snapshot_refreshes_without_reattaching() {
        let engine = Engine::new(EngineConfig::default());
        let mut client = engine.attach(Some("resume-ui".to_string())).await;
        let id = client.id.clone();
        // Drain the attach snapshot, then warm the param cache (a fresh
        // snapshot only carries `params` for cached channels).
        while client.events.try_recv().is_ok() {}
        for _ in 0..20 {
            engine.poll_once().await;
        }
        while client.events.try_recv().is_ok() {}
        engine.push_snapshot(&id).await;
        let mut tags: Vec<String> = Vec::new();
        while let Ok(event) = client.events.try_recv() {
            tags.push(
                shared_json(&event)
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
                    .to_string(),
            );
        }
        for want in [
            "state", "shows", "clients", "locks", "caps", "devices", "link", "scenes", "fx",
            "routing", "params",
        ] {
            assert!(
                tags.contains(&want.to_string()),
                "resume snapshot carries `{want}` (got {tags:?})"
            );
        }
        assert!(
            !tags.contains(&"you".to_string()),
            "resume snapshot must not re-identify the client (got {tags:?})"
        );
        // No re-attach: same id, exactly one roster entry.
        assert_eq!(client.id, id);
        {
            let clients = engine.clients.read().unwrap();
            assert_eq!(clients.len(), 1, "resume must not duplicate the client");
            assert!(clients.contains_key(&id));
        }
        // Unknown id: no-op (no panic, no mailbox created).
        engine.push_snapshot("client-does-not-exist").await;
        assert_eq!(engine.clients.read().unwrap().len(), 1);
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
            tags.push(shared_json(&event).get("type").and_then(Value::as_str).unwrap_or("?").to_string());
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
        // Timeout-class read: tolerated twice (one lost datagram on venue
        // wifi must not evict a live param), dropped on the third
        // consecutive strike and reported.
        while client.events.try_recv().is_ok() {} // membership false rows follow the legacy prefix
        let err = anyhow::anyhow!("/ch/01/eq/on: timed out after 2s");
        assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
        assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
        assert!(engine.note_poll_error("xair", "eq_on", &err).await);
        assert!(
            engine
                .param_dropped
                .read()
                .unwrap()
                .contains(&("xair".to_string(), "eq_on".to_string()))
        );
        // A post-drop timeout starts a fresh count (strikes cleared on
        // eviction): no caps re-broadcast, no double report.
        assert!(!engine.note_poll_error("xair", "eq_on", &err).await);
        // A success between strikes resets the count: two strikes, one ok,
        // two more strikes must NOT drop.
        assert!(!engine.note_poll_error("xair", "gain", &err).await);
        assert!(!engine.note_poll_error("xair", "gain", &err).await);
        engine.note_poll_ok("xair", "gain");
        assert!(!engine.note_poll_error("xair", "gain", &err).await);
        assert!(!engine.note_poll_error("xair", "gain", &err).await);
        assert!(
            !engine
                .param_dropped
                .read()
                .unwrap()
                .contains(&("xair".to_string(), "gain".to_string()))
        );
        // Unknown params still drop on the first failure (no tolerance for
        // addresses the desk can never answer).
        let unknown = anyhow::anyhow!("xair: unknown param \"nope\"");
        assert!(engine.note_poll_error("xair", "nope", &unknown).await);
        // Other drivers are unaffected.
        assert!(
            !engine
                .param_dropped
                .read()
                .unwrap()
                .contains(&("sim".to_string(), "eq_on".to_string()))
        );
        // Two caps updates went out on the fanout (one per eviction).
        for _ in 0..2 {
            let msg = tokio::time::timeout(Duration::from_secs(2), client.events.recv())
                .await
                .expect("caps update in time")
                .expect("mailbox open");
            assert_eq!(shared_json(&msg).get("type"), Some(&json!("caps")));
        }
        // No-data errors never drop.
        let nodata = anyhow::anyhow!("sim: gain has no value yet");
        assert!(!engine.note_poll_error("sim", "gain", &nodata).await);
        assert!(engine.param_dropped.read().unwrap().len() == 2);
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
                            .map(|def| openfader_core::params::supports_kind(def, fam.kind))
                            .unwrap_or(true)
                    })
                    .count();
                // Solo rides the poll set as its own id on strips with a
                // provider extra (see `static_poll_list`): one extra entry
                // per strip of the matching families.
                let n = n + engine.drv(driver).essentials_extra_ids(fam.kind).len();
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
                                .map(|def| openfader_core::params::supports_kind(def, fam.kind))
                                .unwrap_or(true)
                        })
                        .copied()
                        .collect();
                    want.extend(engine.drv(driver).essentials_extra_ids(fam.kind).iter().copied());
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
        // poll id (bank re-read via `read_param`); nothing else does 
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
        use openfader_core::{strip_index_for_key, strip_key_for_index};
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
                Ok(frame) => match &*frame {
                    Event::Param { ch, key, field, .. } if field == "fader" => {
                        saw = Some((*ch, key.clone()));
                        break;
                    }
                    _ => continue,
                },
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
                Ok(frame) => match &*frame {
                    Event::Error { message, .. } if message.contains("grp/01") => {
                        erred = true;
                        break;
                    }
                    _ => continue,
                },
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
                .get_fader(openfader_core::StripId(1))
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
                .get_fader(openfader_core::StripId(1))
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
        // clients as `Event::Solo`, the channel the UI solos view
        // renders, while other params still ride `Params` frames.
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
                Ok(frame) => match &*frame {
                    Event::Solo { ch, soloed, .. } => {
                        found = Some((*ch, *soloed));
                        break;
                    }
                    _ => continue,
                },
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert_eq!(found, Some((2, true)), "solo-in must fan out Solo");
        // A repeat of the same state stays quiet (poll diff semantics 
        // no Solo echo storm every sweep).
        engine.apply_inbound(2, "solo", &ParamValue::Bool(true));
        let mut dup = false;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(frame) => match &*frame {
                    Event::Solo { .. } => {
                        dup = true;
                        break;
                    }
                    _ => continue,
                },
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert!(!dup, "duplicate solo state must stay quiet");
        // Unsolo fans out, and the param plane is untouched by solos.
        engine.apply_inbound(2, "solo", &ParamValue::Bool(false));
        let mut found = None;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(frame) => match &*frame {
                    Event::Solo { ch, soloed, .. } => {
                        found = Some((*ch, *soloed));
                        break;
                    }
                    _ => continue,
                },
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        assert_eq!(found, Some((2, false)), "solo-out must fan out Solo");
        engine.apply_inbound(2, "fader", &ParamValue::Float(0.5));
        let mut saw_params = false;
        for _ in 0..64 {
            match handle.events.try_recv() {
                Ok(frame) => match &*frame {
                    Event::Params { ch: 2, .. } => {
                        saw_params = true;
                        break;
                    }
                    _ => continue,
                },
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
    /// deterministic even under workspace-parallel load. The immediate tick
    /// issues one full batch across the lanes (focus/visible/essentials/
    /// background), so the total of the lane cursors, not the background
    /// cursor alone, marks the fired tick.
    async fn settle_parked_poller(engine: &Engine, client: &mut ClientHandle) {
        let want = engine.effective_inflight(&engine.active_driver());
        let deadline = Instant::now() + Duration::from_secs(10);
        while engine.poll_cursor.load(Ordering::Relaxed) + engine.core_cursor.load(Ordering::Relaxed)
            < want as u64
        {
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
        // Advance partway into channel 0's block. The parked tick already
        // warmed ch0's essentials through the core lane (fader/mute/name),
        // so 2 of these 10 round-robin reads are cache hits and stay
        // quiet, 8 frames land.
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
        // Diff frames land in the mailbox in poll order: 8 round-robin
        // reads (10 minus the 2 core-warmed dups), then the whole focus
        // block on ch5.
        let mut chs: Vec<u16> = Vec::new();
        while let Ok(event) = client.events.try_recv() {
            if let Event::Params { ch, .. } = &*event {
                chs.push(*ch);
            }
        }
        assert_eq!(chs.len(), 8 + nparams, "8 round-robin + {nparams} focus reads");
        assert!(
            chs[8..].iter().all(|&c| c == 5),
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
        // value for every supported scoped id, the poller never silently
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
                        .map(|def| openfader_core::params::supports_kind(def, fam.kind))
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
        // focus strip N, N's full set drains first (most-recent-first),
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
            if let Event::Params { ch, .. } = &*event {
                chs.push(*ch);
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
