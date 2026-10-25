//! UI-free mixer state. Implemented by proto-xair, proto-dl32s, sim.
//!
//! Shared logic (OSC encode/decode, TCP framing, keepalive, meter
//! ring-buffer, reconnect, discovery) lives in `openfader-transport`, never in
//! driver crates. A new provider adds one crate implementing [`MixerDriver`]
//! plus an address-table row; core stays untouched.
//!
//! The trait is async (`&self` + interior mutability) because drivers do
//! tokio I/O. All methods past the core four have default bodies returning
//! "unsupported", so stub drivers keep compiling while they grow.

pub mod access;
pub mod fx;
pub mod params;
pub mod presets;
pub mod rta;
pub mod show;
pub mod sync;

/// 0-based channel/bus strip index, uniform across every driver.
///
/// All drivers (`sim`, `xair`, `dl32s`, …) take 0-based [`StripId`]s; a driver
/// whose wire protocol numbers channels from 1 (X-Air `/ch/01/…`, the DL32S
/// address table) adds 1 internally. `bus: u8` parameters follow the same
/// rule: 0-based at the trait level, +1 on 1-based wires. User-facing layers
/// (the CLI) display channels 1-based and subtract 1 before calling in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StripId(pub u16);

/// Audio-frequency data with provider provenance. Bins retain their measured
/// dB values; explicit band edges avoid inventing/resampling a frequency axis.
#[derive(Clone, Debug)]
pub struct AudioSpectrum {
    pub strip: Option<StripId>,
    pub bins_db: Vec<f32>,
    pub band_edges_hz: Vec<f32>,
    pub source: String,
    pub position: String,
    pub epoch: u64,
}

/// Strip family kind, as carried in `caps.topology[].kind`
/// (`in|ret|bus|fxsnd|mtx|main|dca`). Catalogue entries declare which kinds
/// they apply to via [`params::ParamDef::scopes`].
///
/// The enum is [`non_exhaustive`](https://doc.rust-lang.org/reference/attributes/type_system.html#the-non_exhaustive-attribute):
/// future desks (Wing/X32/M32, …) contribute new families through
/// [`StripKind::Custom`] instead of editing core. Downstream matches must
/// carry a wildcard arm. The seven historical wire names are byte-identical
/// in the `caps` JSON; [`StripKind::as_str`] returns the inner name for
/// [`StripKind::Custom`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StripKind {
    In,
    Ret,
    Bus,
    FxSnd,
    Mtx,
    Main,
    Dca,
    /// Provider-extended family kind. The inner value is the wire name
    /// (convention: lowercase, e.g. `"grp"`); it renders verbatim in
    /// `caps.topology[].kind` and `scopes`.
    Custom(&'static str),
}

impl StripKind {
    /// Wire name used in the `caps` frame (`in|ret|bus|fxsnd|mtx|main|dca`,
    /// or the provider name for [`StripKind::Custom`]).
    pub fn as_str(self) -> &'static str {
        match self {
            StripKind::In => "in",
            StripKind::Ret => "ret",
            StripKind::Bus => "bus",
            StripKind::FxSnd => "fxsnd",
            StripKind::Mtx => "mtx",
            StripKind::Main => "main",
            StripKind::Dca => "dca",
            StripKind::Custom(name) => name,
        }
    }

    /// Parse a wire kind name back (caps round-trip / tests). Unknown names
    /// return `None`, use [`StripKind::kind_from_str`] when provider kinds
    /// must resolve instead of failing.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "in" => Some(StripKind::In),
            "ret" => Some(StripKind::Ret),
            "bus" => Some(StripKind::Bus),
            "fxsnd" => Some(StripKind::FxSnd),
            "mtx" => Some(StripKind::Mtx),
            "main" => Some(StripKind::Main),
            "dca" => Some(StripKind::Dca),
            _ => None,
        }
    }

    /// Extensible kind resolution: the seven canonical wire names map to
    /// their variants, anything else becomes [`StripKind::Custom`] carrying
    /// the name verbatim (so a future desk's `"grp"` family round-trips
    /// through caps without a core edit). The input must be `'static`
    /// because [`StripKind::Custom`] stores it by reference, string
    /// literals from provider topology tables qualify.
    pub fn kind_from_str(s: &'static str) -> Self {
        Self::parse(s).unwrap_or(StripKind::Custom(s))
    }
}

/// One ordered strip family in the global strip index space
/// (`caps.topology`). Families are contiguous, non-overlapping and ordered
/// by `first`; the UI builds banks from this table for any desk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StripFamily {
    /// Stable family key (`ch`, `mix`, `fxret`, `fxsnd`, `mtx`, `main`, `dca` …).
    pub key: &'static str,
    pub kind: StripKind,
    /// Display label (`Ch 1-32`, `Mix 1-8`, `Main LR`, …).
    pub label: &'static str,
    /// First global strip index (0-based [`StripId`]).
    pub first: usize,
    /// Strip count in this family.
    pub count: usize,
    /// Name prefix for default scribbles (`Ch`, `FxRet`, `Mix`, `Grp`, `FxSnd`, `Mtx`, `Main`, `DCA`).
    pub name_prefix: &'static str,
    /// Default desk channel colour (0..=15 enum, bank colour).
    pub color_default: u8,
}

impl StripFamily {
    pub const fn end(self) -> usize {
        self.first + self.count
    }

    pub const fn contains(self, i: usize) -> bool {
        i >= self.first && i < self.first + self.count
    }
}

/// Opaque stable strip key for one strip in a family (`family_key/index`).
///
/// Keys survive desk switching where positional [`StripId`]s do not (sim 77
/// strips, xair 36, dl32s 32): profiles, banks and scenes store keys, and
/// the engine resolves them against the active desk's topology.
///
/// Format: `{family_key}/{nn}` with the 1-based index in the family padded
/// to 2 digits (`ch/01`, `bus/01`, `fxret/01`, `dca/02`). The single-strip
/// `main` family is the bare key (`main`). `idx` is 1-based; `0` yields
/// `"{family_key}/00"` (callers should reject it via [`parse_strip_key`]).
/// The `mix` legacy alias is normalised to `bus` (the sim bus family key)
/// so sim-captured banks resolve on xair/dl32s surfaces.
pub fn strip_key(family_key: &str, idx: u16) -> String {
    let fam = strip_family_alias(family_key);
    if fam == "main" {
        return "main".to_string();
    }
    format!("{fam}/{:02}", idx.min(99))
}

/// Legacy family-key alias normalisation for strip keys: the sim bus family
/// is keyed `mix` while xair keys it `bus`, both are kind `Bus` and the
/// same masters. Keys always use the canonical `bus` spelling; the parser
/// accepts both.
pub fn strip_family_alias(family_key: &str) -> &str {
    match family_key.trim() {
        "mix" => "bus",
        other => other,
    }
}

/// Parse a stable strip key back into (family key, 1-based index).
///
/// Accepts `main` (single-strip family → `("main", 1)`), `main/1`,
/// `ch/01` / `ch/1`, and the legacy `mix/01` alias (normalised to `bus`).
/// Returns `None` for empty families, index `0`, or malformed keys
/// (empty input, extra slashes, non-numeric index).
pub fn parse_strip_key(key: &str) -> Option<(String, u16)> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if key == "main" {
        return Some(("main".to_string(), 1));
    }
    let (fam, num) = key.split_once('/')?;
    let fam = strip_family_alias(fam.trim());
    if fam.is_empty() || fam.contains('/') || fam.contains(char::is_whitespace) {
        return None;
    }
    if num.trim().is_empty() {
        return None;
    }
    if num.contains('/') {
        return None;
    }
    let idx: u16 = num.trim().parse().ok()?;
    if idx == 0 {
        return None;
    }
    Some((fam.to_string(), idx))
}

/// Canonical family key behind [`strip_key_for_index`]: bus-kind families
/// always key as `bus` (sim `mix` vs xair `bus`); every other family keys
/// as its topology key.
pub fn canonical_family_key(fam: &StripFamily) -> &str {
    if fam.kind == StripKind::Bus {
        "bus"
    } else {
        strip_family_alias(fam.key)
    }
}

/// Stable key for a global strip index under `topology` (`None` when the
/// index belongs to no family). Single-strip families key as the bare
/// family key (`main`); the rest key as `family/nn` (see [`strip_key`]).
/// Bus-kind families normalise to `bus` (see [`canonical_family_key`]).
pub fn strip_key_for_index(topology: &[StripFamily], index: usize) -> Option<String> {
    let fam = topology.iter().find(|f| f.contains(index))?;
    let canon = canonical_family_key(fam);
    if fam.count == 1 {
        return Some(canon.to_string());
    }
    let local = (index - fam.first + 1).min(99);
    Some(strip_key(canon, local as u16))
}

/// Global strip index for a stable key under `topology` (`None` when the
/// key names no strip here). Accepts the `mix/..` legacy alias and bare
/// `main`; single-strip families also accept `key/1`.
pub fn strip_index_for_key(topology: &[StripFamily], key: &str) -> Option<usize> {
    let (fam_key, idx) = parse_strip_key(key)?;
    // Exact family-key match first (covers `ch2/01`, `fxsnd/..`, …), then
    // the canonical bus fallback (`bus/01` on the sim `mix` family).
    let fam = topology
        .iter()
        .find(|f| strip_family_alias(f.key) == fam_key.as_str())
        .or_else(|| {
            topology
                .iter()
                .find(|f| canonical_family_key(f) == fam_key.as_str())
        })?;
    if fam.count == 1 {
        return if idx == 1 { Some(fam.first) } else { None };
    }
    let n = idx as usize;
    if n == 0 || n > fam.count {
        return None;
    }
    Some(fam.first + n - 1)
}

/// Per-tick meter levels, one entry per strip (linear 0.0..=1.0).
#[derive(Clone, Debug, PartialEq)]
pub struct MeterFrame {
    pub levels: Vec<f32>,
}

/// Measured processor telemetry, independent of fader/input meter taps.
/// Reduction is positive dB; input/output are measured dBFS, never inferred.
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicsMeterFrame {
    pub ch: u16,
    pub gate_in: f32,
    pub gate_reduction: f32,
    pub gate_out: f32,
    pub comp_in: f32,
    pub comp_reduction: f32,
    pub comp_out: f32,
}

impl MeterFrame {
    pub fn silent(channels: usize) -> Self {
        Self {
            levels: vec![0.0; channels],
        }
    }
}

/// Identity block returned by [`MixerDriver::query_info`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MixerInfo {
    pub name: String,
    pub model: String,
    pub firmware: String,
    pub addr: String,
}

/// Generic parameter value for [`MixerDriver::set_param`]/[`MixerDriver::get_param`].
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Float(f32),
    Bool(bool),
    Text(String),
}

impl From<f32> for ParamValue {
    fn from(v: f32) -> Self {
        ParamValue::Float(v)
    }
}

impl From<bool> for ParamValue {
    fn from(v: bool) -> Self {
        ParamValue::Bool(v)
    }
}

impl From<String> for ParamValue {
    fn from(v: String) -> Self {
        ParamValue::Text(v)
    }
}

/// One desk-originated param change: the desk broadcast a new value for
/// one strip param (another client moved it, or the desk echoes another
/// client's query reply to every `/xremote` subscriber). Drivers that can
/// observe the desk push these; the engine applies them to its param cache
/// and fans them out like poller diffs. Engineering units, same as
/// [`MixerDriver::get_param`].
#[derive(Clone, Debug, PartialEq)]
pub struct ParamUpdate {
    /// Monotonic socket receive time; cache replay never advances this clock.
    pub received_at: std::time::Instant,
    pub strip: StripId,
    pub param: String,
    pub value: ParamValue,
}

impl From<&str> for ParamValue {
    fn from(v: &str) -> Self {
        ParamValue::Text(v.to_string())
    }
}

/// One channel strip of mixer state.
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelStrip {
    pub id: StripId,
    pub name: String,
    pub fader: f32,
    pub mute: bool,
    pub pan: f32,
}

impl ChannelStrip {
    pub fn new(id: u16, name: impl Into<String>) -> Self {
        Self {
            id: StripId(id),
            name: name.into(),
            fader: 0.0,
            mute: false,
            pan: 0.5,
        }
    }
}

/// Point-in-time copy of all strips, for scene snapshot/recall.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MixerSnapshot {
    pub strips: Vec<ChannelStrip>,
}

/// One native desk snapshot slot: the desk's own slot id intact (raw `u32`,
/// never remapped, never zero-defaulted) plus the desk-provided name. No
/// base, bounds, labels or counts are invented, native `0` may exist
/// (Default?) but that is unverified, so no slot is assumed to be first.
/// Slots exist ONLY from desk push evidence (see
/// [`MixerDriver::native_snapshots`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSnapshotSlot {
    pub slot: u32,
    pub name: String,
}

/// Read-only view of a desk's native snapshot slots, distinct from the
/// engine's app-local numbered scene library (`scenes_lib`, slots
/// `1..=999`).
///
/// Provenance contract: every entry is desk-observed push metadata, a
/// snapshot recall or rename relay received on the live session. Own-write
/// optimism (a local save accept, a delete) never populates this view, so
/// it can never launder an unconfirmed write into a confirmed slot.
/// `received` stays true once ANY genuine snapshot push has been observed
/// this session, even when `slots` is empty, that is PARTIAL observed
/// metadata only, never a full catalogue and never proof of an empty desk.
/// `ready` therefore means "observed something", not "complete inventory":
/// `slots` empty with `current: Some(n)` proves only the last reported
/// recall, never a non-empty inventory. `connected` reports whether a
/// live desk session backs this view; `current` is the last
/// desk-observed recall slot, `None` means no recall observed, never a
/// slot-0 default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeSnapshots {
    /// Push-evidenced slots in slot-id order.
    pub slots: Vec<NativeSnapshotSlot>,
    /// Last desk-evidenced recall slot (`None` = no recall observed).
    pub current: Option<u32>,
    /// Any genuine snapshot push observed this session (even empty).
    pub received: bool,
    /// A live desk session backs this view.
    pub connected: bool,
}

pub fn clamp_fader(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

pub fn validate_fader(v: f32) -> anyhow::Result<f32> {
    if !v.is_finite() {
        anyhow::bail!("fader must be finite, got {v}");
    }
    if !(0.0..=1.0).contains(&v) {
        anyhow::bail!("fader {v} out of range 0.0..=1.0");
    }
    Ok(v)
}

/// Piecewise-linear fader scale shared by drivers whose mixers use
/// engineering units (e.g. dB) on the wire instead of 0.0..=1.0.
///
/// The table holds unit values at evenly spaced levels
/// (`level[i] = i / (n-1)`), as in DigiMixer's `DbFaderScale`. Per-driver
/// crates own their table (address-mapping data); the interpolation math
/// lives here so every dB-fader provider reuses it.
#[derive(Clone, Copy, Debug)]
pub struct FaderScale {
    units: &'static [f32],
}

impl FaderScale {
    /// Needs >= 2 entries (unchecked).
    pub const fn from_uniform(units: &'static [f32]) -> Self {
        Self { units }
    }

    /// Level to engineering units; out-of-range clamps.
    pub fn forward(&self, level: f32) -> f32 {
        let pts = self.units;
        if pts.is_empty() {
            return 0.0;
        }
        if level <= 0.0 {
            return pts[0];
        }
        if level >= 1.0 {
            return pts[pts.len() - 1];
        }
        let segs = (pts.len() - 1) as f32;
        let pos = level * segs;
        let idx = (pos as usize).min(pts.len() - 2);
        let frac = pos - idx as f32;
        let (a, b) = (pts[idx], pts[idx + 1]);
        a + (b - a) * frac
    }

    /// Engineering units back to level; out-of-range clamps.
    pub fn inverse(&self, units: f32) -> f32 {
        let pts = self.units;
        if pts.is_empty() {
            return 0.0;
        }
        if units <= pts[0] {
            return 0.0;
        }
        if units >= pts[pts.len() - 1] {
            return 1.0;
        }
        let segs = (pts.len() - 1) as f32;
        for i in 0..pts.len() - 1 {
            let (a, b) = (pts[i], pts[i + 1]);
            if (a <= units && units <= b) || (b <= units && units <= a) {
                let frac = if b == a { 0.0 } else { (units - a) / (b - a) };
                return (i as f32 + frac) / segs;
            }
        }
        0.0
    }
}

/// Async-trait style is intentional: every driver method is async and the
/// desugar to `fn ... -> impl Future` would be a breaking API change across
/// all driver crates, so the `async_fn_in_trait` lint is allowed here.
#[allow(async_fn_in_trait)]
pub trait MixerDriver: Send + Sync {
    // ---- identity ----
    fn name(&self) -> &'static str;

    /// Total number of addressable channel strips (excl. buses/main).
    fn channel_count(&self) -> usize {
        32
    }

    // ---- connection ----
    async fn connect(&self, addr: &str) -> anyhow::Result<()>;
    /// Release the current transport and its subscription tasks.
    async fn disconnect(&self) -> anyhow::Result<()> { Ok(()) }
    /// Renew existing telemetry subscriptions after host suspension.
    async fn refresh_streams(&self) -> anyhow::Result<()> { Ok(()) }
    /// Socket receipt stamps for level and GR samples, when the provider
    /// can prove their provenance. Replaying a cache does not advance them.
    fn telemetry_timestamps(&self) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> { None }

    // ---- channel strips ----
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()>;
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let _ = strip;
        anyhow::bail!("{}: get_fader not supported", self.name())
    }
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()>;
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let _ = strip;
        anyhow::bail!("{}: get_mute not supported", self.name())
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let _ = (strip, name);
        anyhow::bail!("{}: set_name not supported", self.name())
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let _ = strip;
        anyhow::bail!("{}: get_name not supported", self.name())
    }

    // ---- buses / main (default: unsupported) ----
    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let _ = (bus, value);
        anyhow::bail!("{}: bus faders not supported", self.name())
    }
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let _ = bus;
        anyhow::bail!("{}: bus faders not supported", self.name())
    }
    /// 0-based bus over `0..bus_count()`.
    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        let _ = (bus, muted);
        anyhow::bail!("{}: bus mutes not supported", self.name())
    }
    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        let _ = bus;
        anyhow::bail!("{}: bus mutes not supported", self.name())
    }
    /// Mix-bus master count (0-based range `0..bus_count()`). Default 0 =
    /// no bus masters (honest `unsupported`, never faked).
    fn bus_count(&self) -> u8 {
        0
    }
    // ---- subgroups (default: unsupported) ----
    /// 0-based `0..subgroup_count()`.
    fn subgroup_count(&self) -> u8 {
        0
    }
    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
        let _ = (group, value);
        anyhow::bail!("{}: subgroup faders not supported", self.name())
    }
    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
        let _ = group;
        anyhow::bail!("{}: subgroup faders not supported", self.name())
    }
    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let _ = (group, muted);
        anyhow::bail!("{}: subgroup mutes not supported", self.name())
    }
    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
        let _ = group;
        anyhow::bail!("{}: subgroup mutes not supported", self.name())
    }
    /// Channel -> subgroup membership (DL32S `subN` leaf). Default: unsupported.
    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let _ = (ch, group, member);
        anyhow::bail!("{}: subgroup assign not supported", self.name())
    }
    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let _ = (ch, group);
        anyhow::bail!("{}: subgroup assign not supported", self.name())
    }
    // ---- mute groups (default: unsupported) ----
    /// 0-based `0..mute_group_count()`.
    fn mute_group_count(&self) -> u8 {
        0
    }
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let _ = (group, muted);
        anyhow::bail!("{}: mute groups not supported", self.name())
    }
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let _ = group;
        anyhow::bail!("{}: mute groups not supported", self.name())
    }
    /// Channel -> mute-group membership. Default: unsupported.
    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let _ = (ch, group, member);
        anyhow::bail!("{}: mute-group assign not supported", self.name())
    }
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let _ = (ch, group);
        anyhow::bail!("{}: mute-group assign not supported", self.name())
    }
    // ---- file transfer (default: unsupported) ----
    /// Desk file transfer (DL32S `Show.zip` up/download).
    fn file_transfer(&self) -> bool {
        false
    }
    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        let _ = filename;
        anyhow::bail!("{}: file transfer not supported", self.name())
    }
    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        let _ = (filename, data, file_type);
        anyhow::bail!("{}: file transfer not supported", self.name())
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        let _ = value;
        anyhow::bail!("{}: main fader not supported", self.name())
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        anyhow::bail!("{}: main fader not supported", self.name())
    }
    /// Set the Main LR master mute. Default: unsupported (never faked).
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let _ = muted;
        anyhow::bail!("{}: main mute not supported", self.name())
    }
    /// Read the Main LR master mute. Default: unsupported.
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        anyhow::bail!("{}: main mute not supported", self.name())
    }
    /// Set per-strip solo (monitor bus; never touches the FOH mix).
    /// Default: unsupported (never faked).
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let _ = (strip, soloed);
        anyhow::bail!("{}: solo not supported", self.name())
    }
    /// Read per-strip solo. Default: unsupported.
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        let _ = strip;
        anyhow::bail!("{}: solo not supported", self.name())
    }

    // ---- generic params (default: thin dispatch to the above) ----
    /// Well-known ids: `"fader"`, `"mute"`, `"name"`, `"pan"`.
    /// Driver-specific ids pass through to the driver override.
    ///
    /// [`capabilities`](MixerDriver::capabilities) advertises which
    /// [`params`](crate::params) catalogue ids this driver implements;
    /// the relay `caps` frame renders the catalogue filtered by it.
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match (param, value) {
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("pan", ParamValue::Float(_)) => {
                anyhow::bail!("{}: pan not supported", self.name())
            }
            (p, _) => anyhow::bail!("{}: unknown param {p:?}", self.name()),
        }
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            _ => anyhow::bail!("{}: unknown param {param:?}", self.name()),
        }
    }

    // ---- capabilities ----
    /// Catalogue ids ([`crate::params::CATALOGUE`]) this driver implements
    /// for `get_param`/`set_param`. The relay polls and advertises exactly
    /// this set; anything else on the wire is hidden from the UI (never
    /// shown as unknown). Default is the always-available triple; drivers
    /// override with their loopback-tested set.
    fn capabilities(&self) -> &'static [&'static str] {
        &["fader", "mute", "name"]
    }

    /// Provider-contributed param definitions merged with the canonical
    /// [`crate::params::CATALOGUE`] at engine boot (see
    /// [`crate::params::ParamRegistry`]): rows for desk-specific DSP that
    /// has no canonical id. Default is empty (the driver serves canonical
    /// ids only); a driver that adds DSP returns its table here AND lists
    /// every served id (canonical + contributed) in
    /// [`capabilities`](MixerDriver::capabilities) so the engine polls and
    /// advertises it. Convention for contributed ids is
    /// `vendor:param` namespacing (e.g. `"wing:stagebox_gain"`) so tables
    /// never collide; on an id clash the canonical row wins and the
    /// provider row is ignored.
    fn param_defs(&self) -> &'static [crate::params::ParamDef] {
        &[]
    }

    /// Driver-specific engineering-unit range overrides, applied over the
    /// canonical [`crate::params::CATALOGUE`] in this provider's merged
    /// [`crate::params::ParamRegistry`] (see
    /// [`crate::params::ParamRegistry::override_range`]): each entry is
    /// `(canonical id, min, max)`; unknown ids are ignored. Default is
    /// empty (the driver serves canonical ranges). A driver whose desk
    /// documents different native bounds returns them here, ranges only
    /// (kinds, units, labels, scopes and defaults never change), so the
    /// canonical merge stays stable for every other driver.
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        &[]
    }

    /// Provider-owned labels and conditional choices for model-specific DSP.
    fn param_presentations(&self) -> &'static [crate::params::ParamPresentation] {
        &[]
    }

    /// Desk-confirmed display labels for advertised parameters, such as
    /// named View memberships. Empty/unknown labels retain the static label.
    /// Cache-only; providers must clear labels across connection changes.
    fn param_label_overrides(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    fn param_swatches(&self, _param: &str) -> &'static [crate::params::ParamSwatch] {
        &[]
    }

    fn param_symbols(&self, _param: &str) -> &'static [crate::params::ParamSymbol] {
        &[]
    }

    fn param_text_rules(&self, _param: &str) -> Option<crate::params::ParamTextRules> {
        None
    }

    /// Ordered strip families in the global strip index space
    /// (`caps.topology`). Default is one `in` family covering
    /// [`MixerDriver::channel_count`] so drivers that have not grown
    /// topology yet keep working; the poller treats an empty topology the
    /// same way. Families must be contiguous, non-overlapping and ordered
    /// by `first`; the first `channel_count` strips keep their historic
    /// indices.
    fn topology(&self) -> Vec<StripFamily> {
        vec![StripFamily {
            key: "ch",
            kind: StripKind::In,
            label: "Ch",
            first: 0,
            count: self.channel_count(),
            name_prefix: "Ch",
            color_default: 2,
        }]
    }

    /// Total addressable strips (sum of [`MixerDriver::topology`]).
    /// Defaults to [`MixerDriver::channel_count`] so old drivers keep their
    /// poll/snapshot geometry; drivers with extra families (sim, xair)
    /// override with the topology total.
    fn strip_count(&self) -> usize {
        self.channel_count()
    }

    /// Declared meter taps (`caps.meter_taps`): per-strip `in` (pre-fader
    /// input level), `post` (post-fader), `gr_gate` / `gr_comp` (dB
    /// reduction), plus main LR L/R. Transport carries them (MR formats);
    /// the param poller is never used for meters.
    fn meter_taps(&self) -> &'static [&'static str] {
        &["in", "post", "gr_gate", "gr_comp"]
    }

    /// None: unsupported; Some(empty): supported but no fresh measurement.
    fn poll_dynamics_meters(&self) -> Option<Vec<DynamicsMeterFrame>> {
        None
    }

    // ---- snapshot / info ----
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        anyhow::bail!("{}: snapshot not supported", self.name())
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        let _ = snapshot;
        anyhow::bail!("{}: recall not supported", self.name())
    }

    /// Recall a [`show::Scene`] with a fader fade over `fade_ms`
    /// (see [`show::Scene::recall_to`]). Default suits every driver.
    async fn recall_scene(&self, scene: &crate::show::Scene, fade_ms: u64) -> anyhow::Result<()>
    where
        Self: Sync,
    {
        crate::show::recall_scene(self, scene, fade_ms).await
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        anyhow::bail!("{}: query_info not supported", self.name())
    }

    // ---- FX params (openfader_core::fx; X-Air/DL32S map these later) ----
    /// Set one numbered FX param: `slot` 1..=4, `index` 0..64, value 0..1.
    /// Default: unsupported (validate with [`fx::FxRack`] first).
    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        let _ = (slot, param, value);
        anyhow::bail!("{}: fx params not supported", self.name())
    }

    /// Read one numbered FX param. Default: unsupported.
    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        let _ = (slot, param);
        anyhow::bail!("{}: fx params not supported", self.name())
    }

    // ---- meters ----
    /// Shared broadcast hub lives in `openfader-transport`; drivers hand out
    /// receivers. Default: unsupported (use [`MixerDriver::poll_meters`]).
    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<MeterFrame>> {
        anyhow::bail!("{}: subscribe_meters not supported", self.name())
    }

    /// Desk-originated param changes (broadcasts the desk sends to every
    /// subscribed client when another client moves something). Default:
    /// `None` (the driver learns desk-side changes by polling only).
    /// Drivers that tap the transport update feed return a receiver here;
    /// values are engineering units, same as `get_param`.
    fn subscribe_updates(
        &self,
    ) -> Option<tokio::sync::broadcast::Receiver<ParamUpdate>> {
        None
    }

    /// Cache-only audio spectra. Empty means unavailable/stale. A global desk
    /// source without a verified strip association carries `strip: None`.
    fn poll_spectrum(&self) -> Vec<AudioSpectrum> {
        Vec::new()
    }

    /// Acquire/change/release the desk's shared RTA selector. An empty owner
    /// releases the current lease (disconnect); a named release affects only
    /// that client. Providers without a selector can ignore the hint.
    async fn set_spectrum_target(&self, _owner: &str, _strip: Option<StripId>) -> anyhow::Result<()> {
        Ok(())
    }

    /// Non-blocking single-frame poll (fake meters, tests, headless use).
    fn poll_meters(&self) -> MeterFrame {
        MeterFrame::silent(self.channel_count())
    }

    /// Read-only view of the desk's native snapshot slots (see
    /// [`NativeSnapshots`]): push-evidenced names plus the current-slot
    /// marker from the current session. Default `None` = the driver does
    /// not model native slots (honest `unsupported`, never faked, in
    /// particular no simulator invents desk slots it does not model).
    ///
    /// There is deliberately NO write counterpart: native recall/save/
    /// delete, `Show.zip` transfer, free-slot allocation and probing stay
    /// out of this surface. The view is sync and lock-local (session-cache
    /// reads only, no I/O) so the engine can sample it on its existing
    /// ticker without a new task or timer.
    fn native_snapshots(&self) -> Option<NativeSnapshots> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTH: FaderScale = FaderScale::from_uniform(&[-60.0, 0.0, 10.0]);

    #[test]
    fn endpoints_clamp() {
        assert_eq!(SYNTH.forward(0.0), -60.0);
        assert_eq!(SYNTH.forward(1.0), 10.0);
        assert_eq!(SYNTH.forward(-0.5), -60.0);
        assert_eq!(SYNTH.forward(2.0), 10.0);
        assert_eq!(SYNTH.inverse(-120.0), 0.0);
        assert_eq!(SYNTH.inverse(99.0), 1.0);
    }

    #[test]
    fn segments_interpolate() {
        assert!((SYNTH.forward(0.25) - -30.0).abs() < 1e-6);
        assert!((SYNTH.forward(0.75) - 5.0).abs() < 1e-6);
        assert!((SYNTH.inverse(-30.0) - 0.25).abs() < 1e-6);
        assert!((SYNTH.inverse(5.0) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn roundtrip() {
        for i in 0..=20 {
            let level = i as f32 / 20.0;
            let back = SYNTH.inverse(SYNTH.forward(level));
            assert!((back - level).abs() < 1e-5, "level {level}");
        }
    }

    #[test]
    fn strip_key_format_and_parse() {
        assert_eq!(super::strip_key("ch", 1), "ch/01");
        assert_eq!(super::strip_key("bus", 1), "bus/01");
        assert_eq!(super::strip_key("dca", 2), "dca/02");
        assert_eq!(super::strip_key("fxret", 1), "fxret/01");
        assert_eq!(super::strip_key("main", 1), "main");
        assert_eq!(super::parse_strip_key("ch/01"), Some(("ch".to_string(), 1)));
        assert_eq!(super::parse_strip_key("bus/1"), Some(("bus".to_string(), 1)));
        assert_eq!(super::parse_strip_key("dca/2"), Some(("dca".to_string(), 2)));
        assert_eq!(super::parse_strip_key("main"), Some(("main".to_string(), 1)));
        assert_eq!(super::parse_strip_key("main/1"), Some(("main".to_string(), 1)));
        assert_eq!(super::strip_key("mix", 1), "bus/01");
        assert_eq!(super::parse_strip_key("mix/01"), Some(("bus".to_string(), 1)));
        // Malformed keys fail.
        for bad in ["", " ", "ch/", "ch/0", "/01", "ch/01/x", "ch/x", "main/2x"] {
            assert_eq!(super::parse_strip_key(bad), None, "key {bad:?}");
        }
        // Round-trip across the padded range.
        for n in 1..=48u16 {
            let k = super::strip_key("ch", n);
            assert_eq!(super::parse_strip_key(&k), Some(("ch".to_string(), n.min(99))), "key {k}");
        }
    }

    #[test]
    fn strip_key_index_roundtrip_bus_alias() {
        use super::{strip_index_for_key, strip_key_for_index, StripFamily, StripKind};
        let sim = [
            StripFamily { key: "ch", kind: StripKind::In, label: "Ch", first: 0, count: 2, name_prefix: "Ch", color_default: 2 },
            StripFamily { key: "mix", kind: StripKind::Bus, label: "Mix", first: 2, count: 2, name_prefix: "Mix", color_default: 6 },
            StripFamily { key: "main", kind: StripKind::Main, label: "Main", first: 4, count: 1, name_prefix: "Main", color_default: 11 },
        ];
        let xair = [
            StripFamily { key: "ch", kind: StripKind::In, label: "Ch", first: 0, count: 2, name_prefix: "Ch", color_default: 2 },
            StripFamily { key: "bus", kind: StripKind::Bus, label: "Bus", first: 2, count: 2, name_prefix: "Mix", color_default: 6 },
        ];
        assert_eq!(strip_key_for_index(&sim, 2), Some("bus/01".to_string()));
        assert_eq!(strip_key_for_index(&xair, 2), Some("bus/01".to_string()));
        assert_eq!(strip_key_for_index(&sim, 4), Some("main".to_string()));
        assert_eq!(strip_index_for_key(&xair, "bus/01"), Some(2));
        assert_eq!(strip_index_for_key(&xair, "mix/01"), Some(2));
        assert_eq!(strip_index_for_key(&sim, "bus/02"), Some(3));
        assert_eq!(strip_index_for_key(&xair, "bus/03"), None);
        assert_eq!(strip_index_for_key(&xair, "dca/01"), None);
        assert_eq!(strip_key_for_index(&xair, 99), None);
    }
}
