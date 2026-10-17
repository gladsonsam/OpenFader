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

/// 0-based channel/bus strip index, uniform across every driver.
///
/// All drivers (`sim`, `xair`, `dl32s`, …) take 0-based [`StripId`]s; a driver
/// whose wire protocol numbers channels from 1 (X-Air `/ch/01/…`, the DL32S
/// address table) adds 1 internally. `bus: u8` parameters follow the same
/// rule: 0-based at the trait level, +1 on 1-based wires. User-facing layers
/// (the CLI) display channels 1-based and subtract 1 before calling in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StripId(pub u16);

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
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        let _ = value;
        anyhow::bail!("{}: main fader not supported", self.name())
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        anyhow::bail!("{}: main fader not supported", self.name())
    }

    // ---- generic params (default: thin dispatch to the above) ----
    /// Well-known ids: `"fader"`, `"mute"`, `"name"`, `"pan"`.
    /// Driver-specific ids pass through to the driver override.
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

    // ---- snapshot / info ----
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        anyhow::bail!("{}: snapshot not supported", self.name())
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        let _ = snapshot;
        anyhow::bail!("{}: recall not supported", self.name())
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        anyhow::bail!("{}: query_info not supported", self.name())
    }

    // ---- meters ----
    /// Shared broadcast hub lives in `openmix-transport`; drivers hand out
    /// receivers. Default: unsupported (use [`MixerDriver::poll_meters`]).
    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<MeterFrame>> {
        anyhow::bail!("{}: subscribe_meters not supported", self.name())
    }

    /// Non-blocking single-frame poll (fake meters, tests, headless use).
    fn poll_meters(&self) -> MeterFrame {
        MeterFrame::silent(self.channel_count())
    }
}
