//! Offline fake mixer for UI dev: 32 channels + 8 buses, fake meters.
use openmix_core::{validate_fader, ChannelStrip, MeterFrame, MixerDriver, MixerSnapshot, ParamValue, StripId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// 32 input channels.
pub const SIM_CHANNELS: usize = 32;
/// 8 buses (FX / aux / LR-adjacent), ids 32..40.
pub const SIM_BUSES: usize = 8;
/// Total strips.
pub const SIM_STRIPS: usize = SIM_CHANNELS + SIM_BUSES;

/// Named scene preset for [`SimDriver`]: realistic channel/bus scribbles
/// for the two supported hardware desks, plus a `Studio` clone of the
/// owner's live XR18 desk (see `docs/xr18-baseline.md` §2). Geometry stays
/// 32ch + 8 buses (the sim is a stand-in, not a channel-count model); only
/// names change (plus the reported model string in [`MixerDriver::query_info`]).
/// `Generic` is the historical default (`CH01..`, `BUS1..`) and is what
/// `SimDriver::new()` / `Default` produce, so existing snapshots stay valid.
/// `Studio` additionally mirrors the desk-as-found faders/mutes on strips
/// 1..16, buses 1..6 and the main LR fader; strips past the 16-channel desk
/// (17..32, buses 7..8) are parked spares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimPreset {
    /// Historical default: `CH01..CH32`, `BUS1..BUS8`.
    #[default]
    Generic,
    /// Behringer XR18 layout: 18 real inputs + `CH19..CH32` spare.
    Xr18,
    /// Mackie DL32S layout: 32 real inputs.
    Dl32s,
    /// Owner's XR18 desk as found (`docs/xr18-baseline.md` §2): real
    /// scribbles/order (`BASS`, `VOX`, `GTR L/R`, `DRUM L/R`, `KEYS L/R`,
    /// `AC GTR`, `Abelton` — desk spelling kept), bus scribbles
    /// (`COMP L/R`, `DRUM`, `KEYS`, `VOX`) and baseline faders/mutes.
    Studio,
}

impl SimPreset {
    /// Parse a `--preset` flag value (`generic|xr18|dl32s|studio`,
    /// case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "generic" | "gen" | "default" | "sim" => Some(SimPreset::Generic),
            "xr18" | "xair" | "x-air" => Some(SimPreset::Xr18),
            "dl32s" | "dl32-s" | "dl" => Some(SimPreset::Dl32s),
            "studio" | "rig" | "home" => Some(SimPreset::Studio),
            _ => None,
        }
    }

    /// CLI-facing name (`--preset <name>`).
    pub fn name(self) -> &'static str {
        match self {
            SimPreset::Generic => "generic",
            SimPreset::Xr18 => "xr18",
            SimPreset::Dl32s => "dl32s",
            SimPreset::Studio => "studio",
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

/// Sim bus scribbles, shared by the generic-desk presets: front mix, four
/// monitor sends, two FX returns, one spare. The LR mains live on the
/// separate main fader (`set_main_fader`), as on the real desks.
pub const PRESET_BUSES: [&str; 8] = [
    "MixBus", "Mon 1", "Mon 2", "Mon 3", "Mon 4", "Reverb", "Delay", "FX",
];

/// Owner's XR18 desk scribbles, in channel order (`docs/xr18-baseline.md`
/// §2). `""` is an empty desk scribble, not a spare: channels 02/03/06/14
/// read back empty on the hardware. Desk spelling kept (`Abelton`).
/// Strips 17..=32 keep their `CHxx` spare labels (XR18 is a 16-channel desk).
pub const STUDIO_CHANNELS: [&str; 16] = [
    "BASS", "", "", "VOX", "VOX", "", "GTR L", "GTR R", "DRUM L", "DRUM R", "KEYS L",
    "KEYS R", "AC GTR", "", "Abelton", "Abelton",
];

/// Owner's desk-as-found channel faders, same order (`docs/xr18-baseline.md`
/// §2; `/ch/NN/mix/fader`).
pub const STUDIO_FADERS: [f32; 16] = [
    0.197458, 0.0, 0.337243, 0.348974, 0.375367, 0.0, 0.434995, 0.434995, 0.0, 0.482893,
    0.438905, 0.438905, 0.339198, 0.0, 0.524927, 0.524927,
];

/// Owner's desk-as-found mute state, same order: `true` = muted.
/// Baseline column is `mix/on` (1 = ON/unmuted), so mute = `on == 0`.
pub const STUDIO_MUTES: [bool; 16] = [
    true, true, true, true, false, true, false, false, false, false, true, true, true,
    true, true, true,
];

/// Owner's bus scribbles (`/bus/1..6/config/name`; bus 3 is empty on the
/// desk). Buses 7..8 have no desk equivalent and stay parked spares.
pub const STUDIO_BUSES: [&str; 8] = [
    "COMP L", "COMP R", "", "DRUM", "KEYS", "VOX", "BUS7", "BUS8",
];

/// Owner's desk-as-found bus faders (`/bus/1..6/mix/fader` all ON/unmuted);
/// buses 7..8 are parked spares at 0.75.
pub const STUDIO_BUS_FADERS: [f32; 8] = [
    0.749756, 0.749756, 0.749756, 1.0, 1.0, 1.0, 0.75, 0.75,
];

/// Owner's desk-as-found main LR fader (`/lr/mix/fader`, ON/unmuted).
pub const STUDIO_MAIN: f32 = 0.749756;

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
            SimPreset::Studio => STUDIO_CHANNELS
                .get(i)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("CH{:02}", i + 1)),
        };
        let mut s = ChannelStrip::new(i as u16, name);
        match preset {
            SimPreset::Studio => {
                // Desk-as-found scene (§2): baseline fader/mute on strips
                // 1..16, parked spares past the 16-channel desk.
                if let (Some(f), Some(m)) = (STUDIO_FADERS.get(i), STUDIO_MUTES.get(i)) {
                    s.fader = *f;
                    s.mute = *m;
                } else {
                    s.fader = 0.0;
                    s.mute = true;
                }
            }
            _ => {
                // A pleasant default scene: unity-ish faders with variation.
                s.fader = 0.7 + 0.1 * ((i % 5) as f32 / 4.0);
            }
        }
        v.push(s);
    }
    for b in 0..SIM_BUSES {
        let bus_name = match preset {
            SimPreset::Generic => format!("BUS{}", b + 1),
            SimPreset::Studio => STUDIO_BUSES
                .get(b)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("BUS{}", b + 1)),
            _ => PRESET_BUSES
                .get(b)
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("BUS{}", b + 1)),
        };
        let mut s = ChannelStrip::new((SIM_CHANNELS + b) as u16, bus_name);
        s.fader = match preset {
            SimPreset::Studio => STUDIO_BUS_FADERS.get(b).copied().unwrap_or(0.75),
            _ => 0.75,
        };
        // Desk buses read back ON/unmuted; spares park unmuted.
        s.mute = false;
        v.push(s);
    }
    v
}

/// Expected value kind for one canonical detail-panel field.
///
/// Vocabulary mirrors the `openmix-proto-dl32s` input-strip address table:
/// `gain`, `phantom`, `pan`, `hpf_*`, `gate_*`, `comp_*`, `eq{1..4}_*`,
/// `aux{1..8}_*`, `fx{1..4}_*`, plus mute-group/view/subgroup/VCA membership.
/// Enum/int wire fields accept [`ParamValue::Float`] (whole numbers), the same
/// convention as the DL32S driver; sim stores everything bit-exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParamKind {
    Float,
    Bool,
    Text,
}

fn canonical_param_kind(param: &str) -> Option<ParamKind> {
    match param {
        "fader" | "pan" | "trim" | "gain" | "hpf_freq" | "gate_threshold" | "gate_attack"
        | "gate_release" | "gate_hold" | "gate_range" | "comp_gain" | "comp_threshold"
        | "comp_attack" | "comp_release" | "comp_ratio" => Some(ParamKind::Float),
        "mute" | "polarity" | "main_assign" | "stereo_link" | "phantom" | "hpf_on"
        | "gate_on" | "comp_on" | "eq_on" => Some(ParamKind::Bool),
        "name" => Some(ParamKind::Text),
        // Enum/int wire fields (accepted as whole-number floats).
        "source_a" | "source_b" | "source_select" | "icon" | "colour" | "unknown_16"
        | "gate_mode" | "comp_mode" | "comp_knee" | "eq_mode" | "eq_bands" => {
            Some(ParamKind::Float)
        }
        _ => patterned_param_kind(param),
    }
}

/// Indexed families of the canonical field set: `eq{1..4}_*`,
/// `aux{1..8}_*`, `fx{1..4}_*`, mute-group/view/subgroup/VCA membership.
/// Out-of-family indices (`eq5_gain`, `aux9_level`, …) are NOT canonical and
/// return `None` so they keep failing as unknown params.
fn patterned_param_kind(param: &str) -> Option<ParamKind> {
    // EQ bands: eq{1..4}_{gain,freq,q} floats; eq{2..4}_type enum-as-float
    // (band 1 has no filter-type word on the wire).
    if let Some(rest) = param.strip_prefix("eq") {
        let (band, suffix) = rest.split_once('_')?;
        let band: u32 = band.parse().ok()?;
        if !(1..=4).contains(&band) {
            return None;
        }
        return match suffix {
            "gain" | "freq" | "q" => Some(ParamKind::Float),
            "type" if band != 1 => Some(ParamKind::Float),
            _ => None,
        };
    }
    // Sends: aux{1..8}_{level,mute,unknown}, fx{1..4}_{level,mute}.
    for (prefix, count) in [("aux", 8u32), ("fx", 4u32)] {
        if let Some(rest) = param.strip_prefix(prefix) {
            let (n, suffix) = rest.split_once('_')?;
            let n: u32 = n.parse().ok()?;
            if !(1..=count).contains(&n) {
                return None;
            }
            return match suffix {
                "level" | "unknown" => Some(ParamKind::Float),
                "mute" => Some(ParamKind::Bool),
                _ => None,
            };
        }
    }
    // Membership rows: mutegroup/view/sub/vca{1..6} bools.
    for prefix in ["mutegroup", "view", "sub", "vca"] {
        if let Some(n) = param.strip_prefix(prefix) {
            let n: u32 = n.parse().ok()?;
            return if (1..=6).contains(&n) {
                Some(ParamKind::Bool)
            } else {
                None
            };
        }
    }
    None
}

pub struct SimDriver {
    strips: RwLock<Vec<ChannelStrip>>,
    /// Per-strip detail-panel state (`gain`, `phantom`, `eq1_gain`, …): the
    /// canonical DL32S field set, stored bit-exact (no wire quantization in
    /// sim). Strip-backed params (`fader`, `mute`, `name`, `pan`) live in
    /// [`ChannelStrip`] and are intentionally NOT mirrored here; `get_param`
    /// reads those from the strip so the two views cannot disagree.
    params: RwLock<Vec<HashMap<String, ParamValue>>>,
    main: RwLock<f32>,
    tick: AtomicU64,
    connected: RwLock<bool>,
    preset: SimPreset,
}

impl SimDriver {
    pub fn new() -> Self {
        Self::with_preset(SimPreset::Generic)
    }

    /// Named desk preset (`--preset xr18|dl32s|studio` in `openmix-cli`).
    /// Geometry is unchanged (32ch + 8 buses); only scribbles differ
    /// (studio additionally mirrors desk-as-found faders/mutes + main).
    pub fn with_preset(preset: SimPreset) -> Self {
        let main = match preset {
            SimPreset::Studio => STUDIO_MAIN,
            _ => 0.8,
        };
        Self {
            strips: RwLock::new(default_strips_for(preset)),
            params: RwLock::new(vec![HashMap::new(); SIM_STRIPS]),
            main: RwLock::new(main),
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

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        let i = self.idx(strip)?;
        // Strip-backed params stay the single source of truth in ChannelStrip.
        match (param, &value) {
            ("fader", ParamValue::Float(v)) => return self.set_fader(strip, *v).await,
            ("mute", ParamValue::Bool(m)) => return self.set_mute(strip, *m).await,
            ("name", ParamValue::Text(n)) => return self.set_name(strip, n).await,
            ("pan", ParamValue::Float(v)) => {
                if !v.is_finite() {
                    anyhow::bail!("sim: pan must be finite, got {v}");
                }
                self.write()[i].pan = *v;
                return Ok(());
            }
            _ => {}
        }
        let kind = canonical_param_kind(param)
            .ok_or_else(|| anyhow::anyhow!("sim: unknown param {param:?}"))?;
        let kind_ok = matches!(
            (kind, &value),
            (ParamKind::Float, ParamValue::Float(_))
                | (ParamKind::Bool, ParamValue::Bool(_))
                | (ParamKind::Text, ParamValue::Text(_))
        );
        if !kind_ok {
            anyhow::bail!("sim: {param} expects {kind:?}, got {value:?}");
        }
        if let ParamValue::Float(v) = &value {
            if !v.is_finite() {
                anyhow::bail!("sim: {param} must be finite, got {v}");
            }
        }
        // Bit-exact store: no dB conversion, no quantization in sim.
        self.params.write().expect("sim params poisoned")[i].insert(param.to_string(), value);
        Ok(())
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        let i = self.idx(strip)?;
        match param {
            "fader" => return self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => return self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => return self.get_name(strip).await.map(ParamValue::Text),
            "pan" => return Ok(ParamValue::Float(self.read()[i].pan)),
            _ => {}
        }
        canonical_param_kind(param)
            .ok_or_else(|| anyhow::anyhow!("sim: unknown param {param:?}"))?;
        self.params.read().expect("sim params poisoned")[i]
            .get(param)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("sim: {param} has no value yet"))
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        // Strip-backed params (fader/mute/name/pan) ride along in the strips.
        // Extended detail params (gain, phantom, …) are live driver state in
        // the per-strip map: `MixerSnapshot` (a core type) carries strips
        // only, so `recall` restores strips and leaves the map intact rather
        // than clobbering detail state it cannot represent.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn studio_preset_mirrors_rig() {
        // `--preset studio` parses (case-insensitive) and round-trips.
        assert_eq!(SimPreset::parse("studio"), Some(SimPreset::Studio));
        assert_eq!(SimPreset::parse("STUDIO"), Some(SimPreset::Studio));
        assert_eq!(SimPreset::Studio.name(), "studio");
        // Existing presets keep their names.
        assert_eq!(SimPreset::parse("xr18"), Some(SimPreset::Xr18));
        assert_eq!(SimPreset::parse("dl32s"), Some(SimPreset::Dl32s));
        assert_eq!(SimPreset::parse("generic"), Some(SimPreset::Generic));

        let d = SimDriver::with_preset(SimPreset::Studio);
        assert_eq!(d.preset(), SimPreset::Studio);

        // Channel scribbles in desk order (baseline §2; "" = empty desk
        // scribble on 02/03/06/14; desk spelling `Abelton` kept).
        let names = async {
            let mut out = Vec::with_capacity(SIM_CHANNELS);
            for i in 0..SIM_CHANNELS {
                out.push(d.get_name(StripId(i as u16)).await.unwrap());
            }
            out
        }
        .await;
        let expected: Vec<String> = STUDIO_CHANNELS
            .iter()
            .map(|s| s.to_string())
            .chain((STUDIO_CHANNELS.len()..SIM_CHANNELS).map(|i| format!("CH{:02}", i + 1)))
            .collect();
        assert_eq!(names, expected);
        // Spot names a demo viewer would recognise.
        assert_eq!(names[0], "BASS");
        assert_eq!(names[3], "VOX");
        assert_eq!(names[4], "VOX");
        assert_eq!(names[6], "GTR L");
        assert_eq!(names[8], "DRUM L");
        assert_eq!(names[12], "AC GTR");
        assert_eq!(names[14], "Abelton");

        // Desk-as-found faders/mutes on strips 1..16.
        for (i, (f, m)) in STUDIO_FADERS.iter().zip(STUDIO_MUTES.iter()).enumerate() {
            let got = d.get_fader(StripId(i as u16)).await.unwrap();
            assert!((got - f).abs() < 1e-6, "ch{} fader {got} != {f}", i + 1);
            assert_eq!(d.get_mute(StripId(i as u16)).await.unwrap(), *m, "ch{}", i + 1);
        }
        // Spares past the 16-channel desk park silent.
        assert_eq!(names[16], "CH17");
        assert_eq!(d.get_fader(StripId(16)).await.unwrap(), 0.0);
        assert!(d.get_mute(StripId(16)).await.unwrap());

        // Bus scribbles + faders as observed (bus 3 empty on the desk).
        for (b, name) in STUDIO_BUSES.iter().enumerate() {
            let strip = StripId((SIM_CHANNELS + b) as u16);
            assert_eq!(d.get_name(strip).await.unwrap(), *name, "bus{}", b + 1);
            let got = d.get_bus_fader(b as u8).await.unwrap();
            assert!((got - STUDIO_BUS_FADERS[b]).abs() < 1e-6, "bus{} {got}", b + 1);
            assert!(!d.get_mute(strip).await.unwrap(), "bus{}", b + 1);
        }

        // Main LR fader as observed.
        assert!((d.get_main_fader().await.unwrap() - STUDIO_MAIN).abs() < 1e-6);
        // Reported model carries the preset name.
        let info = d.query_info().await.unwrap();
        assert!(info.model.contains("studio"), "model: {}", info.model);
    }

    #[tokio::test]
    async fn desk_presets_untouched() {
        // Generic default is unchanged (CHxx/BUSx, unity-ish scene).
        let g = SimDriver::new();
        assert_eq!(g.preset(), SimPreset::Generic);
        assert_eq!(g.get_name(StripId(0)).await.unwrap(), "CH01");
        assert_eq!(g.get_name(StripId(32)).await.unwrap(), "BUS1");
        assert!((g.get_main_fader().await.unwrap() - 0.8).abs() < 1e-6);

        // XR18/DL32S scribbles unchanged.
        let x = SimDriver::with_preset(SimPreset::Xr18);
        assert_eq!(x.get_name(StripId(0)).await.unwrap(), "Kick");
        assert_eq!(x.get_name(StripId(18)).await.unwrap(), "CH19");
        let dl = SimDriver::with_preset(SimPreset::Dl32s);
        assert_eq!(dl.get_name(StripId(0)).await.unwrap(), "Kick");
        assert_eq!(dl.get_name(StripId(31)).await.unwrap(), "Spare 6");
    }

    #[tokio::test]
    async fn param_roundtrip_float_gain() {
        let d = SimDriver::new();
        // Bit-exact: values that dB quantization would mangle survive intact.
        for v in [12.5f32, 0.1, -19.95, 60.0] {
            d.set_param(StripId(0), "gain", ParamValue::Float(v))
                .await
                .unwrap();
            assert_eq!(
                d.get_param(StripId(0), "gain").await.unwrap(),
                ParamValue::Float(v),
                "gain {v}"
            );
        }
    }

    #[tokio::test]
    async fn param_roundtrip_bool_phantom() {
        let d = SimDriver::new();
        d.set_param(StripId(3), "phantom", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(3), "phantom").await.unwrap(),
            ParamValue::Bool(true)
        );
        d.set_param(StripId(3), "phantom", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(3), "phantom").await.unwrap(),
            ParamValue::Bool(false)
        );
    }

    #[tokio::test]
    async fn param_roundtrip_text_name() {
        let d = SimDriver::new();
        d.set_param(StripId(0), "name", ParamValue::Text("Kick".into()))
            .await
            .unwrap();
        assert_eq!(
            d.get_param(StripId(0), "name").await.unwrap(),
            ParamValue::Text("Kick".into())
        );
        // Strip-backed: the generic param and the dedicated accessor agree.
        assert_eq!(d.get_name(StripId(0)).await.unwrap(), "Kick");
    }

    #[tokio::test]
    async fn param_covers_detail_panel_families() {
        let d = SimDriver::new();
        let s = StripId(1);
        // One field per detail-panel family: hpf, gate, comp, eq, aux, fx,
        // plus pan and an enum-as-float (eq2_type).
        let fields: &[(&str, ParamValue)] = &[
            ("pan", ParamValue::Float(-0.25)),
            ("hpf_on", ParamValue::Bool(true)),
            ("hpf_freq", ParamValue::Float(80.0)),
            ("gate_on", ParamValue::Bool(true)),
            ("gate_threshold", ParamValue::Float(-30.0)),
            ("comp_on", ParamValue::Bool(true)),
            ("comp_ratio", ParamValue::Float(4.0)),
            ("eq_on", ParamValue::Bool(true)),
            ("eq1_gain", ParamValue::Float(3.0)),
            ("eq4_freq", ParamValue::Float(12000.0)),
            ("eq2_q", ParamValue::Float(1.4)),
            ("eq2_type", ParamValue::Float(1.0)),
            ("aux1_level", ParamValue::Float(-10.0)),
            ("aux1_mute", ParamValue::Bool(true)),
            ("fx2_level", ParamValue::Float(-20.0)),
            ("fx2_mute", ParamValue::Bool(false)),
            ("mutegroup1", ParamValue::Bool(true)),
        ];
        for (f, v) in fields {
            d.set_param(s, f, v.clone())
                .await
                .unwrap_or_else(|e| panic!("set {f}: {e}"));
        }
        for (f, v) in fields {
            assert_eq!(
                d.get_param(s, f).await.unwrap(),
                *v,
                "roundtrip {f}"
            );
        }
    }

    #[tokio::test]
    async fn param_snapshot_includes_params() {
        let d = SimDriver::new();
        let s = StripId(0);
        d.set_param(s, "gain", ParamValue::Float(12.5))
            .await
            .unwrap();
        d.set_param(s, "phantom", ParamValue::Bool(true))
            .await
            .unwrap();
        d.set_param(s, "name", ParamValue::Text("Kick".into()))
            .await
            .unwrap();
        d.set_param(s, "pan", ParamValue::Float(-0.25))
            .await
            .unwrap();
        let snap = d.snapshot().await.unwrap();
        // Strip-backed params ride along in the snapshot strips, bit-exact.
        assert_eq!(snap.strips[0].name, "Kick");
        assert_eq!(snap.strips[0].pan, -0.25);
        // A snapshot/recall cycle keeps detail params readable and exact
        // (recall restores strips without clobbering the param map).
        d.recall(&snap).await.unwrap();
        assert_eq!(
            d.get_param(s, "gain").await.unwrap(),
            ParamValue::Float(12.5)
        );
        assert_eq!(
            d.get_param(s, "phantom").await.unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            d.get_param(s, "name").await.unwrap(),
            ParamValue::Text("Kick".into())
        );
        assert_eq!(
            d.get_param(s, "pan").await.unwrap(),
            ParamValue::Float(-0.25)
        );
    }

    #[tokio::test]
    async fn param_rejects_unknown_and_wrong_kind() {
        let d = SimDriver::new();
        let s = StripId(0);
        for bad in ["nope", "eq5_gain", "aux9_level", "fx5_mute", "eq1_type"] {
            let err = d
                .set_param(s, bad, ParamValue::Float(1.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{bad}: {err}");
            let err = d.get_param(s, bad).await.unwrap_err().to_string();
            assert!(err.contains("unknown"), "{bad}: {err}");
        }
        // Wrong value kind per field kind.
        let err = d
            .set_param(s, "gain", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Float"), "{err}");
        let err = d
            .set_param(s, "phantom", ParamValue::Float(1.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Bool"), "{err}");
        // Non-finite floats rejected; unset params read back as missing.
        assert!(d
            .set_param(s, "gain", ParamValue::Float(f32::NAN))
            .await
            .is_err());
        let err = d
            .get_param(StripId(5), "gain")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("no value yet"), "{err}");
        // Out-of-range strips still error.
        assert!(d
            .set_param(StripId(99), "gain", ParamValue::Float(1.0))
            .await
            .is_err());
        assert!(d.get_param(StripId(99), "gain").await.is_err());
    }
}
