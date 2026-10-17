//! Offline fake mixer for UI dev: 32 channels + 8 buses, fake meters.
use openmix_core::{validate_fader, ChannelStrip, MeterFrame, MixerDriver, MixerSnapshot, StripId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 32 input channels.
pub const SIM_CHANNELS: usize = 32;
/// 8 buses (FX / aux / LR-adjacent), ids 32..40.
pub const SIM_BUSES: usize = 8;
/// Total strips.
pub const SIM_STRIPS: usize = SIM_CHANNELS + SIM_BUSES;

/// Named scene preset for [`SimDriver`]: realistic channel/bus scribbles
/// for the two supported hardware desks. Geometry stays 32ch + 8 buses
/// (the sim is a stand-in, not a channel-count model); only names change
/// (plus the reported model string in [`MixerDriver::query_info`]).
/// `Generic` is the historical default (`CH01..`, `BUS1..`) and is what
/// `SimDriver::new()` / `Default` produce, so existing snapshots stay valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimPreset {
    /// Historical default: `CH01..CH32`, `BUS1..BUS8`.
    #[default]
    Generic,
    /// Behringer XR18 layout: 18 real inputs + `CH19..CH32` spare.
    Xr18,
    /// Mackie DL32S layout: 32 real inputs.
    Dl32s,
}

impl SimPreset {
    /// Parse a `--preset` flag value (`generic|xr18|dl32s`, case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "generic" | "gen" | "default" | "sim" => Some(SimPreset::Generic),
            "xr18" | "xair" | "x-air" => Some(SimPreset::Xr18),
            "dl32s" | "dl32-s" | "dl" => Some(SimPreset::Dl32s),
            _ => None,
        }
    }

    /// CLI-facing name (`--preset <name>`).
    pub fn name(self) -> &'static str {
        match self {
            SimPreset::Generic => "generic",
            SimPreset::Xr18 => "xr18",
            SimPreset::Dl32s => "dl32s",
        }
    }
}

/// XR18 input scribbles (18 physical inputs of a typical live band layout).
/// Strips 19..=32 keep their `CHxx` spare labels.
pub const XR18_CHANNELS: [&str; 18] = [
    "Kick", "Snare", "Hat", "Tom 1", "Tom 2", "Floor", "OH L", "OH R", "Bass", "Gtr L",
    "Gtr R", "Keys L", "Keys R", "Vox 1", "Vox 2", "Vox 3", "Play L", "Play R",
];

/// DL32S input scribbles (32 physical inputs, typical live band layout).
pub const DL32S_CHANNELS: [&str; 32] = [
    "Kick", "Snare", "Hat", "Tom 1", "Tom 2", "Tom 3", "Floor", "OH L", "OH R", "Bass",
    "Gtr 1", "Gtr 2", "Keys L", "Keys R", "Ac Gtr", "Perc", "Vox 1", "Vox 2", "Vox 3",
    "Vox 4", "Choir 1", "Choir 2", "Play L", "Play R", "Click", "Talkbk", "Spare 1",
    "Spare 2", "Spare 3", "Spare 4", "Spare 5", "Spare 6",
];

/// Sim bus scribbles, shared by both desk presets: front mix, four
/// monitor sends, two FX returns, one spare. The LR mains live on the
/// separate main fader (`set_main_fader`), as on the real desks.
pub const PRESET_BUSES: [&str; 8] = [
    "MixBus", "Mon 1", "Mon 2", "Mon 3", "Mon 4", "Reverb", "Delay", "FX",
];

fn default_strips_for(preset: SimPreset) -> Vec<ChannelStrip> {
    let mut v = Vec::with_capacity(SIM_STRIPS);
    for i in 0..SIM_CHANNELS {
        let name = match preset {
            SimPreset::Generic => format!("CH{:02}", i + 1),
            SimPreset::Xr18 => XR18_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
            SimPreset::Dl32s => DL32S_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
        };
        let mut s = ChannelStrip::new(i as u16, name);
        // A pleasant default scene: unity-ish faders with variation.
        s.fader = 0.7 + 0.1 * ((i % 5) as f32 / 4.0);
        v.push(s);
    }
    for b in 0..SIM_BUSES {
        let bus_name = match preset {
            SimPreset::Generic => format!("BUS{}", b + 1),
            _ => PRESET_BUSES
                .get(b)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("BUS{}", b + 1)),
        };
        let mut s = ChannelStrip::new((SIM_CHANNELS + b) as u16, bus_name);
        s.fader = 0.75;
        v.push(s);
    }
    v
}

pub struct SimDriver {
    strips: RwLock<Vec<ChannelStrip>>,
    main: RwLock<f32>,
    tick: AtomicU64,
    connected: RwLock<bool>,
    preset: SimPreset,
}

impl SimDriver {
    pub fn new() -> Self {
        Self::with_preset(SimPreset::Generic)
    }

    /// Named desk preset (`--preset xr18|dl32s` in `openmix-cli`).
    /// Geometry is unchanged (32ch + 8 buses); only scribbles differ.
    pub fn with_preset(preset: SimPreset) -> Self {
        Self {
            strips: RwLock::new(default_strips_for(preset)),
            main: RwLock::new(0.8),
            tick: AtomicU64::new(0),
            connected: RwLock::new(false),
            preset,
        }
    }

    /// The preset this driver was built with.
    pub fn preset(&self) -> SimPreset {
        self.preset
    }

    fn read(&self) -> RwLockReadGuard<'_, Vec<ChannelStrip>> {
        self.strips.read().expect("sim strips poisoned")
    }

    fn write(&self) -> RwLockWriteGuard<'_, Vec<ChannelStrip>> {
        self.strips.write().expect("sim strips poisoned")
    }

    fn idx(&self, strip: StripId) -> anyhow::Result<usize> {
        let i = strip.0 as usize;
        if i < self.read().len() {
            Ok(i)
        } else {
            anyhow::bail!("sim: strip {} out of range 0..{}", i, SIM_STRIPS)
        }
    }

    /// Advance the fake-meter phase (1 tick == one 50ms frame).
    pub fn advance(&self, ticks: u64) {
        self.tick.fetch_add(ticks, Ordering::Relaxed);
    }

    fn meter_level(&self, i: usize, tick: u64) -> f32 {
        // Deterministic pseudo-live meters: layered sines per strip.
        let t = tick as f32 * 0.15;
        let ph = i as f32 * 0.7;
        let v = 0.55 + 0.25 * (t + ph).sin() + 0.15 * (t * 2.3 + ph * 1.7).sin();
        let muted = self.read().get(i).map(|s| s.mute).unwrap_or(false);
        if muted {
            0.0
        } else {
            v.clamp(0.0, 1.0)
        }
    }
}

impl Default for SimDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerDriver for SimDriver {
    fn name(&self) -> &'static str {
        "sim"
    }

    fn channel_count(&self) -> usize {
        SIM_CHANNELS
    }

    async fn connect(&self, _addr: &str) -> anyhow::Result<()> {
        *self.connected.write().expect("sim flag poisoned") = true;
        Ok(())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        let i = self.idx(strip)?;
        self.write()[i].fader = v;
        Ok(())
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].fader)
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let i = self.idx(strip)?;
        self.write()[i].mute = muted;
        Ok(())
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].mute)
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        if name.is_empty() || name.len() > 16 {
            anyhow::bail!("sim: name must be 1..=16 chars");
        }
        let i = self.idx(strip)?;
        self.write()[i].name = name.to_string();
        Ok(())
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let i = self.idx(strip)?;
        Ok(self.read()[i].name.clone())
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        let i = SIM_CHANNELS + bus as usize;
        if i >= SIM_STRIPS {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        self.write()[i].fader = v;
        Ok(())
    }

    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let i = SIM_CHANNELS + bus as usize;
        if i >= SIM_STRIPS {
            anyhow::bail!("sim: bus {bus} out of range 0..{}", SIM_BUSES);
        }
        Ok(self.read()[i].fader)
    }

    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        let v = validate_fader(value)?;
        *self.main.write().expect("sim main poisoned") = v;
        Ok(())
    }

    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        Ok(*self.main.read().expect("sim main poisoned"))
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        Ok(MixerSnapshot {
            strips: self.read().clone(),
        })
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        if snapshot.strips.len() != SIM_STRIPS {
            anyhow::bail!(
                "sim: snapshot has {} strips, need {SIM_STRIPS}",
                snapshot.strips.len()
            );
        }
        *self.write() = snapshot.strips.clone();
        Ok(())
    }

    async fn query_info(&self) -> anyhow::Result<openmix_core::MixerInfo> {
        let model = match self.preset {
            SimPreset::Generic => "SimDriver 32+8".to_string(),
            p => format!("SimDriver 32+8 ({})", p.name()),
        };
        Ok(openmix_core::MixerInfo {
            name: "sim".into(),
            model,
            firmware: "0.1.0".into(),
            addr: "sim://local".into(),
        })
    }

    fn poll_meters(&self) -> MeterFrame {
        let tick = self.tick.fetch_add(1, Ordering::Relaxed);
        let n = self.read().len();
        let levels = (0..n).map(|i| self.meter_level(i, tick)).collect();
        MeterFrame { levels }
    }
}
