//! UI-free mixer state. Implemented by proto-xair, proto-dl32s, sim.
//!
//! Shared logic (OSC encode/decode, TCP framing, keepalive, meter
//! ring-buffer, reconnect, discovery) lives in `openmix-transport`, never in
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

/// Strip family kind, as carried in `caps.topology[].kind`
/// (`in|ret|bus|fxsnd|mtx|main|dca`). Catalogue entries declare which kinds
/// they apply to via [`params::ParamDef::scopes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StripKind {
    In,
    Ret,
    Bus,
    FxSnd,
    Mtx,
    Main,
    Dca,
}

impl StripKind {
    /// Wire name used in the `caps` frame (`in|ret|bus|fxsnd|mtx|main|dca`).
    pub const fn as_str(self) -> &'static str {
        match self {
            StripKind::In => "in",
            StripKind::Ret => "ret",
            StripKind::Bus => "bus",
            StripKind::FxSnd => "fxsnd",
            StripKind::Mtx => "mtx",
            StripKind::Main => "main",
            StripKind::Dca => "dca",
        }
    }

    /// Parse a wire kind name back (caps round-trip / tests).
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
    /// One past the last strip index in this family.
    pub const fn end(self) -> usize {
        self.first + self.count
    }

    /// Whether global strip index `i` belongs to this family.
    pub const fn contains(self, i: usize) -> bool {
        i >= self.first && i < self.first + self.count
    }
}

/// Per-tick meter levels, one entry per strip (linear 0.0..=1.0).
#[derive(Clone, Debug, PartialEq)]
pub struct MeterFrame {
    pub levels: Vec<f32>,
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

/// Clamp a fader value into the canonical 0.0..=1.0 range.
pub fn clamp_fader(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

/// Validate a fader value; rejects NaN and out-of-range values.
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
    /// Build a scale from unit values at levels `i / (n-1)`. Needs >= 2 entries.
    pub const fn from_uniform(units: &'static [f32]) -> Self {
        Self { units }
    }

    /// Canonical 0.0..=1.0 level to engineering units. Clamps out-of-range.
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

    /// Engineering units back to canonical 0.0..=1.0 level. Clamps out-of-range.
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
    /// Set a mix-bus master mute (0-based bus, `0..bus_count()`). Default: unsupported.
    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        let _ = (bus, muted);
        anyhow::bail!("{}: bus mutes not supported", self.name())
    }
    /// Read a mix-bus master mute. Default: unsupported.
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
    /// Subgroup master count (0-based `0..subgroup_count()`). Default 0.
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
    /// Mute-group master count (0-based `0..mute_group_count()`). Default 0.
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
    /// Desk file transfer (DL32S `Show.zip` up/download). Default false =
    /// honest `unsupported`, never faked.
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

    // ---- FX params (openmix_core::fx; X-Air/DL32S map these later) ----
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
    /// Shared broadcast hub lives in `openmix-transport`; drivers hand out
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

    /// Non-blocking single-frame poll (fake meters, tests, headless use).
    fn poll_meters(&self) -> MeterFrame {
        MeterFrame::silent(self.channel_count())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Small synthetic table: levels 0.0/0.5/1.0 map to -60/0/+10 units.
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
        // First segment: level 0.25 -> -30.0; second: level 0.75 -> +5.0.
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
}
