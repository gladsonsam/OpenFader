//! Tauri 2 stub shell over the openmix drivers. Sim by default; xair/dl32s attach
//! when an address is given.
//!
//! Static dispatch (`AnyDriver` enum, like `openmix-cli`): `MixerDriver` has
//! async methods and is not dyn-safe, so `Box<dyn MixerDriver>` does not
//! compile. Commands are `async` and `.await` the driver; the `std::sync::Mutex`
//! guard is held across the await (brief critical sections — a production
//! shell would switch to `tokio::sync::Mutex`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use openmix_core::{MixerDriver, ParamValue, StripId};
use openmix_proto_dl32s::Dl32sDriver;
use openmix_proto_xair::XAirDriver;
use openmix_sim::SimDriver;

const CHANNELS: usize = 8;
/// Minimum interval between fresh meter frames; faster polls get the cached frame.
const METER_THROTTLE: Duration = Duration::from_millis(50);

/// Static dispatch over drivers (`MixerDriver` is not dyn-compatible due to
/// async methods). New providers add one variant + one delegation arm.
enum AnyDriver {
    Sim(SimDriver),
    XAir(XAirDriver),
    Dl32s(Dl32sDriver),
}

impl MixerDriver for AnyDriver {
    fn name(&self) -> &'static str {
        match self {
            AnyDriver::Sim(d) => d.name(),
            AnyDriver::XAir(d) => d.name(),
            AnyDriver::Dl32s(d) => d.name(),
        }
    }
    fn channel_count(&self) -> usize {
        match self {
            AnyDriver::Sim(d) => d.channel_count(),
            AnyDriver::XAir(d) => d.channel_count(),
            AnyDriver::Dl32s(d) => d.channel_count(),
        }
    }
    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.connect(addr).await,
            AnyDriver::XAir(d) => d.connect(addr).await,
            AnyDriver::Dl32s(d) => d.connect(addr).await,
        }
    }
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_fader(strip, value).await,
            AnyDriver::XAir(d) => d.set_fader(strip, value).await,
            AnyDriver::Dl32s(d) => d.set_fader(strip, value).await,
        }
    }
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        match self {
            AnyDriver::Sim(d) => d.get_fader(strip).await,
            AnyDriver::XAir(d) => d.get_fader(strip).await,
            AnyDriver::Dl32s(d) => d.get_fader(strip).await,
        }
    }
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_mute(strip, muted).await,
            AnyDriver::XAir(d) => d.set_mute(strip, muted).await,
            AnyDriver::Dl32s(d) => d.set_mute(strip, muted).await,
        }
    }
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        match self {
            AnyDriver::Sim(d) => d.get_mute(strip).await,
            AnyDriver::XAir(d) => d.get_mute(strip).await,
            AnyDriver::Dl32s(d) => d.get_mute(strip).await,
        }
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_name(strip, name).await,
            AnyDriver::XAir(d) => d.set_name(strip, name).await,
            AnyDriver::Dl32s(d) => d.set_name(strip, name).await,
        }
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        match self {
            AnyDriver::Sim(d) => d.get_name(strip).await,
            AnyDriver::XAir(d) => d.get_name(strip).await,
            AnyDriver::Dl32s(d) => d.get_name(strip).await,
        }
    }
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_param(strip, param, value).await,
            AnyDriver::XAir(d) => d.set_param(strip, param, value).await,
            AnyDriver::Dl32s(d) => d.set_param(strip, param, value).await,
        }
    }
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match self {
            AnyDriver::Sim(d) => d.get_param(strip, param).await,
            AnyDriver::XAir(d) => d.get_param(strip, param).await,
            AnyDriver::Dl32s(d) => d.get_param(strip, param).await,
        }
    }
    async fn snapshot(&self) -> anyhow::Result<openmix_core::MixerSnapshot> {
        match self {
            AnyDriver::Sim(d) => d.snapshot().await,
            AnyDriver::XAir(d) => d.snapshot().await,
            AnyDriver::Dl32s(d) => d.snapshot().await,
        }
    }
    async fn recall(&self, snapshot: &openmix_core::MixerSnapshot) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.recall(snapshot).await,
            AnyDriver::XAir(d) => d.recall(snapshot).await,
            AnyDriver::Dl32s(d) => d.recall(snapshot).await,
        }
    }
    async fn query_info(&self) -> anyhow::Result<openmix_core::MixerInfo> {
        match self {
            AnyDriver::Sim(d) => d.query_info().await,
            AnyDriver::XAir(d) => d.query_info().await,
            AnyDriver::Dl32s(d) => d.query_info().await,
        }
    }
    fn poll_meters(&self) -> openmix_core::MeterFrame {
        match self {
            AnyDriver::Sim(d) => d.poll_meters(),
            AnyDriver::XAir(d) => d.poll_meters(),
            AnyDriver::Dl32s(d) => d.poll_meters(),
        }
    }
}

struct Inner {
    kind: String,
    connected: bool,
    driver: AnyDriver,
    faders: [f32; CHANNELS],
    mutes: [bool; CHANNELS],
    last_meters: Vec<f32>,
    last_emit: Option<Instant>,
    tick: u64,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            kind: "sim".into(),
            connected: false,
            driver: AnyDriver::Sim(SimDriver::new()),
            faders: [0.75; CHANNELS],
            mutes: [false; CHANNELS],
            last_meters: vec![0.0; CHANNELS],
            last_emit: None,
            tick: 0,
        }
    }
}

type State<'r> = tauri::State<'r, Mutex<Inner>>;

/// 0-based strip index, matching `StripId` (the UI adds 1 for display).
fn check_strip(ch: u16) -> Result<StripId, String> {
    if (ch as usize) < CHANNELS {
        Ok(StripId(ch))
    } else {
        Err(format!("channel {ch} out of range 0..{}", CHANNELS - 1))
    }
}

#[tauri::command]
async fn connect(state: State<'_>, driver: String, addr: String) -> Result<String, String> {
    let kind = driver.to_lowercase();
    let mut inner = state.lock().map_err(|e| e.to_string())?;
    match kind.as_str() {
        "sim" => {
            let d = AnyDriver::Sim(SimDriver::new());
            d.connect(&addr).await.map_err(|e| e.to_string())?;
            inner.driver = d;
        }
        "xair" => {
            if addr.trim().is_empty() {
                return Err("xair needs a mixer address, e.g. 192.168.1.10".into());
            }
            let d = AnyDriver::XAir(XAirDriver::new());
            d.connect(&addr).await.map_err(|e| e.to_string())?;
            inner.driver = d;
        }
        "dl32s" => {
            if addr.trim().is_empty() {
                return Err("dl32s needs a mixer address, e.g. 192.168.1.120".into());
            }
            let d = AnyDriver::Dl32s(Dl32sDriver::new());
            d.connect(&addr).await.map_err(|e| e.to_string())?;
            inner.driver = d;
        }
        other => return Err(format!("unknown driver '{other}' (sim|xair|dl32s)")),
    }
    inner.kind = kind.clone();
    inner.connected = true;
    Ok(format!("connected via {kind}"))
}

#[tauri::command]
async fn set_fader(state: State<'_>, ch: u16, value: f32) -> Result<(), String> {
    let strip = check_strip(ch)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("fader {value} out of range 0.0..=1.0"));
    }
    let mut inner = state.lock().map_err(|e| e.to_string())?;
    inner
        .driver
        .set_fader(strip, value)
        .await
        .map_err(|e| e.to_string())?;
    inner.faders[ch as usize] = value;
    Ok(())
}

#[tauri::command]
async fn set_mute(state: State<'_>, ch: u16, muted: bool) -> Result<(), String> {
    let strip = check_strip(ch)?;
    let mut inner = state.lock().map_err(|e| e.to_string())?;
    inner
        .driver
        .set_mute(strip, muted)
        .await
        .map_err(|e| e.to_string())?;
    inner.mutes[ch as usize] = muted;
    Ok(())
}

/// Throttled meter snapshot. Poll faster than 50ms and you get the cached frame.
#[tauri::command]
fn get_meters(state: State) -> Result<Vec<f32>, String> {
    let mut inner = state.lock().map_err(|e| e.to_string())?;
    let now = Instant::now();
    if let Some(t) = inner.last_emit {
        if now.duration_since(t) < METER_THROTTLE {
            return Ok(inner.last_meters.clone());
        }
    }
    inner.tick += 1;
    let t = inner.tick as f32 * 0.15;
    let mut frame = Vec::with_capacity(CHANNELS);
    for i in 0..CHANNELS {
        if inner.mutes[i] {
            frame.push(0.0);
        } else {
            let wobble = 0.6 + 0.4 * (t + i as f32 * 0.9).sin();
            frame.push((inner.faders[i] * wobble).clamp(0.0, 1.0));
        }
    }
    inner.last_meters = frame.clone();
    inner.last_emit = Some(now);
    Ok(frame)
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(Inner::default()))
        .invoke_handler(tauri::generate_handler![connect, set_fader, set_mute, get_meters])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
