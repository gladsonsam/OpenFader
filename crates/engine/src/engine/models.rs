//! Session configuration, stored scene values, recall scope, and shared state models.

use super::*;

/// Link re-broadcast interval (R2): the `link` event goes out on every
/// change plus on this cadence with a fresh round-trip measurement.
pub const LINK_INTERVAL: Duration = Duration::from_secs(2);

/// R2 file-transfer upload cap (16 MiB): bounds one WS JSON frame's base64
/// payload so a bad client cannot blow the engine's memory.
pub const FILE_MAX_BYTES: usize = 16 << 20;

/// Per-desk verify cap inside [`Cmd::Scan`]: the linked-desk info query plus
/// the unicast probe fallback. Keeps a connected-but-unresponsive driver from
/// stretching a scan past the broadcast budget (the UI allows ~10 s).
pub(super) const SCAN_VERIFY_TIMEOUT: Duration = Duration::from_millis(2200);

// A directed address may traverse a relay. Budget the complete protocol
// identification (TCP dial plus correlated reply), not just one local RTT.
// All registered providers share the bound; port reachability is never proof.
pub(super) const DIRECT_IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);

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
/// writable value supported by each strip + main + FX + routing).
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
    /// Optional provider-owned mute group state; old scenes omit it.
    pub mute_groups: Option<StoredMuteGroups>,
    pub automix: Option<StoredAutomix>,
    pub output_taps: Option<StoredOutputTaps>,
}

#[derive(Clone, Debug)]
pub struct StoredOutputTaps {
    pub provider: String,
    pub values: Vec<openfader_core::routing::OutputTapState>,
}

#[derive(Clone, Debug)]
pub struct StoredAutomix {
    pub provider: String,
    pub values: [bool; 4],
}

#[derive(Clone, Debug)]
pub struct StoredMuteGroups {
    pub provider: String,
    pub masters: Vec<bool>,
    pub memberships: Vec<StoredMuteGroupMembership>,
}

#[derive(Clone, Debug)]
pub struct StoredMuteGroupMembership {
    pub strip: u16,
    pub key: String,
    pub mask: u32,
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
    /// Input target, or full `output/.../NN` destination ID for a non-strip
    /// output. Output rows have no strip key and recall with a fresh guard.
    pub target: String,
    pub strip: u16,
    pub key: Option<String>,
    pub group: String,
    pub index: u8,
}

/// Outcome of a serialized library commit (see
/// [`Engine::commit_locked`]): memory was swapped in both cases —
/// committed durably, or renamed-but-unproven. Callers publish success
/// frames only on `Committed`.
pub(super) enum CommitDisposition {
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
    /// Param-group allowlist (see [`param_scope_group`]). Providers may override
    /// a parameter's group via `caps.params[].presentations[].recall_group`.
    /// WING adds `sends_matrix` and `sends_main`. `None` = all.
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    /// Global allowlist (`main`, `fx`, `routing`, `mute_groups`). `None` = all.
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
    pub fn matches_strip(&self, strip: u16, topology: &[openfader_core::StripFamily]) -> bool {
        if self.strips.is_none() && self.strip_keys.is_none() {
            return true;
        }
        if self.strips.as_ref().is_some_and(|s| s.contains(&strip)) {
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
        self.groups
            .as_ref()
            .is_none_or(|g| g.iter().any(|x| x == group))
    }
    pub fn allows_param(&self, id: &str) -> bool {
        self.allows_group(param_scope_group(id))
    }
    /// Apply the active provider's local-scene classification when present.
    /// The legacy helper above remains available to existing API callers.
    pub fn allows_param_for(&self, driver: &dyn DynDriver, id: &str) -> bool {
        self.allows_group(
            driver
                .param_recall_group(id)
                .unwrap_or_else(|| param_scope_group(id)),
        )
    }
    pub fn allows_global(&self, global: &str) -> bool {
        self.globals
            .as_ref()
            .is_none_or(|g| g.iter().any(|x| x == global))
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
pub(super) fn send_slot_number(id: &str) -> Option<u8> {
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
    /// Optional provider-specific FX insert state. Legacy scenes omit it.
    pub insert: Option<StoredFxInsert>,
}

#[derive(Clone, Debug)]
pub struct StoredFxInsert {
    pub provider: String,
    pub enabled: bool,
}

pub(super) fn pv_bool(value: &ParamValue) -> Option<bool> {
    if let ParamValue::Bool(v) = value {
        Some(*v)
    } else {
        None
    }
}

pub(super) type ReadFailures = HashMap<(String, u16, String), (u8, Instant)>;

/// Receive ordering and snapshot publication share one short, non-async lock.
pub(super) struct SyncState {
    pub(super) epoch: Instant,
    pub(super) revision: u64,
    pub(super) received: HashMap<(u16, String), (Instant, u64)>,
    pub(super) typed: BTreeMap<String, Event>,
}
impl Default for SyncState {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            revision: 0,
            received: HashMap::new(),
            typed: BTreeMap::new(),
        }
    }
}

/// Engine-side FX rack row (runtime mirror of the driver's rack).
#[derive(Clone, Debug)]
pub(super) struct FxSlotRt {
    pub(super) slot: u8,
    pub(super) fx_type: String,
    pub(super) on: bool,
    pub(super) bypass: bool,
    pub(super) params: BTreeMap<String, f32>,
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

/// Canonicalize XR18 legacy bus-send levels for admission and dispatch.
pub(super) fn canonical_send_field<'a>(driver: &str, field: &'a str) -> std::borrow::Cow<'a, str> {
    if driver == "xair" {
        if let Some(n) = field
            .strip_prefix("aux")
            .and_then(|s| s.strip_suffix("_level"))
        {
            if matches!(n, "1" | "2" | "3" | "4" | "5" | "6") {
                return std::borrow::Cow::Owned(format!("send{n}_level"));
            }
        }
    }
    std::borrow::Cow::Borrowed(field)
}
