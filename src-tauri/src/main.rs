//! Tauri 2 shell over the openmix drivers. Sim by default; xair/dl32s attach
//! when an address is given.
//!
//! Command parity with the `openmix-serve` WebSocket API
//! (`crates/serve/src/lib.rs`) so the Svelte UI (`apps/web`) gets identical
//! capability over Tauri invoke:
//!
//! WS `connect`/`fader`/`mute`/`param` map to `connect`/`set_fader`/
//! `set_mute`/`set_param` with the same value encoding (number|bool|string).
//! The WS `state` push becomes a pull `get_state` returning the same shape
//! (`driver` + `strips` of `{ch,name,fader,muted}`), plus `connected` and
//! `main_fader` (`null` when the driver has none). The WS `meters` tick
//! becomes a pull `get_meters` returning live driver meters; the WS
//! per-tick `spectrum`/`gr` frames become pull `get_spectrum`/`get_gr`
//! (same 64 bins, same first-16-channel cap, same `RtaProcessor` approach
//! and `gate_gr`/`comp_gr` reads — but poll-based, so the UI polls at 5 Hz
//! instead of receiving the WS push).
//!
//! Beyond the WS API, the shell owns a server-side scene/show library
//! (`capture_scene`, `recall_scene`, `list_scenes`, `get_scene`,
//! `delete_scene`) and the main fader (`set_main_fader`, `get_main_fader`),
//! so desktop scenes survive UI reloads (no localStorage round-trip).
//!
//! Static dispatch (`AnyDriver` enum, like `openmix-cli`): `MixerDriver` has
//! async methods and is not dyn-safe, so `Box<dyn MixerDriver>` does not
//! compile. Commands are `async` and `.await` the driver; state lives in a
//! `tokio::sync::Mutex` (via `tauri::async_runtime::Mutex`) so the guard is
//! `Send` and may be held across `.await` — Tauri commands require `Send`
//! futures, which `std::sync::MutexGuard` is not.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::async_runtime::Mutex;
use std::time::{Duration, Instant};

use openmix_core::rta::RtaProcessor;
use openmix_core::show::{Scene, Show, DEFAULT_FADE_MS};
use openmix_core::{MixerDriver, ParamValue, StripId};
use openmix_proto_dl32s::Dl32sDriver;
use openmix_proto_xair::XAirDriver;
use openmix_sim::SimDriver;
use serde_json::json;

/// Minimum interval between fresh meter frames; faster polls get the cached frame.
const METER_THROTTLE: Duration = Duration::from_millis(50);

/// Max channels with spectrum output (first N channels only).
///
/// Mirrors `SPECTRUM_CHANNELS` in `crates/serve/src/lib.rs`: 16 covers a
/// full drum/band layer while keeping the per-tick fanout bounded. Channels
/// at/above this cap are rejected (serve simply never sends them).
const SPECTRUM_CHANNELS: usize = 16;

/// Max channels with GR output (first N channels only).
///
/// Mirrors `GR_CHANNELS` in `crates/serve/src/lib.rs`. Ticks at ~5 Hz over
/// WS (every second 10 Hz `meters` tick); over invoke the UI polls at 5 Hz
/// instead — same staleness contract, pull-based.
const GR_CHANNELS: usize = 16;

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
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        match self {
            AnyDriver::Sim(d) => d.set_main_fader(value).await,
            AnyDriver::XAir(d) => d.set_main_fader(value).await,
            AnyDriver::Dl32s(d) => d.set_main_fader(value).await,
        }
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        match self {
            AnyDriver::Sim(d) => d.get_main_fader().await,
            AnyDriver::XAir(d) => d.get_main_fader().await,
            AnyDriver::Dl32s(d) => d.get_main_fader().await,
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
    /// Server-side scene library; survives driver switches and UI reloads.
    show: Show,
    last_meters: Vec<f32>,
    last_emit: Option<Instant>,
    /// Trailing meter history feeding `get_spectrum`, mirroring serve's
    /// per-connection `RtaProcessor` (fed by its ~10 Hz tick there; fed by
    /// `get_meters`/`get_spectrum` polls here). Reset on `connect`.
    rta: RtaProcessor,
}

impl Default for Inner {
    fn default() -> Self {
        Self {
            kind: "sim".into(),
            connected: false,
            driver: AnyDriver::Sim(SimDriver::new()),
            show: Show::new("openmix"),
            last_meters: Vec::new(),
            last_emit: None,
            rta: RtaProcessor::new(0),
        }
    }
}

type State<'r> = tauri::State<'r, Mutex<Inner>>;

#[tauri::command]
async fn connect(state: State<'_>, driver: String, addr: String) -> Result<String, String> {
    let kind = driver.to_lowercase();
    let mut inner = state.lock().await;
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
    // Fresh driver, fresh meters and RTA; the scene library carries over.
    inner.last_meters.clear();
    inner.last_emit = None;
    inner.rta = RtaProcessor::new(0);
    Ok(format!("connected via {kind}"))
}

#[tauri::command]
async fn set_fader(state: State<'_>, ch: u16, value: f32) -> Result<(), String> {
    openmix_core::validate_fader(value).map_err(|e| e.to_string())?;
    let inner = state.lock().await;
    inner
        .driver
        .set_fader(StripId(ch), value)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn set_mute(state: State<'_>, ch: u16, muted: bool) -> Result<(), String> {
    let inner = state.lock().await;
    inner
        .driver
        .set_mute(StripId(ch), muted)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Generic param, mirroring the WS `param` command: JSON
/// number|boolean|string maps to `ParamValue::Float|Bool|Text`
/// (`"fader"`/`"mute"`/`"name"` dispatch to the strip setters).
#[tauri::command]
async fn set_param(
    state: State<'_>,
    ch: u16,
    field: String,
    value: serde_json::Value,
) -> Result<(), String> {
    let pv = match &value {
        serde_json::Value::Number(n) => ParamValue::Float(n.as_f64().unwrap_or(0.0) as f32),
        serde_json::Value::Bool(b) => ParamValue::Bool(*b),
        serde_json::Value::String(s) => ParamValue::Text(s.clone()),
        _ => return Err("param value must be number|boolean|string".into()),
    };
    let inner = state.lock().await;
    inner
        .driver
        .set_param(StripId(ch), &field, pv)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Full mixer state for the active driver: the WS `state` message shape
/// (`driver` + `strips` of `{ch,name,fader,muted}`), plus `connected` and
/// `main_fader` (`null` when the driver has none). Snapshot failure yields
/// empty strips, like `serve`.
#[tauri::command]
async fn get_state(state: State<'_>) -> Result<serde_json::Value, String> {
    let inner = state.lock().await;
    let strips = match inner.driver.snapshot().await {
        Ok(snap) => snap
            .strips
            .iter()
            .map(|s| {
                json!({
                    "ch": s.id.0,
                    "name": s.name,
                    "fader": s.fader,
                    "muted": s.mute,
                })
            })
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let main_fader = inner.driver.get_main_fader().await.ok();
    Ok(json!({
        "driver": inner.kind,
        "connected": inner.connected,
        "strips": strips,
        "main_fader": main_fader,
    }))
}

#[tauri::command]
async fn set_main_fader(state: State<'_>, value: f32) -> Result<(), String> {
    openmix_core::validate_fader(value).map_err(|e| e.to_string())?;
    let inner = state.lock().await;
    inner
        .driver
        .set_main_fader(value)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn get_main_fader(state: State<'_>) -> Result<f32, String> {
    let inner = state.lock().await;
    inner
        .driver
        .get_main_fader()
        .await
        .map_err(|e| e.to_string())
}

/// Capture the live mixer state as a named scene (upsert) in the
/// server-side show library, including the main fader when the driver has one.
#[tauri::command]
async fn capture_scene(state: State<'_>, name: String) -> Result<serde_json::Value, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("scene name must not be empty".into());
    }
    let mut inner = state.lock().await;
    let snap = inner.driver.snapshot().await.map_err(|e| e.to_string())?;
    let mut scene = Scene::capture(&name, &snap);
    scene.main_fader = inner.driver.get_main_fader().await.ok();
    let summary = json!({
        "name": scene.name,
        "strips": scene.strips.len(),
        "main_fader": scene.main_fader,
    });
    inner.show.add_scene(scene);
    Ok(summary)
}

/// Recall a captured scene with a fader fade (`fade_ms == 0` jumps;
/// default [`DEFAULT_FADE_MS`]). Pre-unmutes scene-unmuted strips so the
/// fade is audible, snaps to exact targets, then applies mutes and main.
#[tauri::command]
async fn recall_scene(
    state: State<'_>,
    name: String,
    fade_ms: Option<u64>,
) -> Result<String, String> {
    let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
    let inner = state.lock().await;
    let scene = inner
        .show
        .get(&name)
        .cloned()
        .ok_or_else(|| format!("unknown scene {name:?}"))?;
    inner
        .driver
        .recall_scene(&scene, fade_ms)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("recalled scene {name:?} (fade {fade_ms}ms)"))
}

/// Names of the captured scenes, in library order.
#[tauri::command]
async fn list_scenes(state: State<'_>) -> Result<Vec<String>, String> {
    let inner = state.lock().await;
    Ok(inner.show.names().into_iter().map(str::to_string).collect())
}

/// One captured scene with per-strip fader/mute, for slot display.
#[tauri::command]
async fn get_scene(state: State<'_>, name: String) -> Result<serde_json::Value, String> {
    let inner = state.lock().await;
    let scene = inner
        .show
        .get(&name)
        .ok_or_else(|| format!("unknown scene {name:?}"))?;
    Ok(json!({
        "name": scene.name,
        "main_fader": scene.main_fader,
        "strips": scene.strips.iter().map(|s| json!({
            "ch": s.strip.0,
            "name": s.name,
            "fader": s.fader,
            "muted": s.mute,
        })).collect::<Vec<_>>(),
    }))
}

/// Remove a scene from the library; `true` when something was removed.
#[tauri::command]
async fn delete_scene(state: State<'_>, name: String) -> Result<bool, String> {
    let mut inner = state.lock().await;
    Ok(inner.show.remove(&name))
}

/// Live meter frame from the active driver, throttled: polls faster than
/// 50 ms get the cached frame. Fresh frames also feed the shared RTA
/// history behind `get_spectrum` (return shape unchanged).
#[tauri::command]
async fn get_meters(state: State<'_>) -> Result<Vec<f32>, String> {
    let mut inner = state.lock().await;
    let now = Instant::now();
    if let Some(t) = inner.last_emit {
        if now.duration_since(t) < METER_THROTTLE {
            return Ok(inner.last_meters.clone());
        }
    }
    let frame = inner.driver.poll_meters();
    inner.rta.push_frame(&frame);
    let levels = frame.levels;
    inner.last_meters = levels.clone();
    inner.last_emit = Some(now);
    Ok(levels)
}

/// Live 64-bin spectrum for one channel: the pull-based mirror of serve's
/// WS `spectrum` tick (`{"type":"spectrum","ch":N,"bins":[...64 floats...]}`).
///
/// Same cap (`SPECTRUM_CHANNELS`) and same `RtaProcessor` approach as serve:
/// the trailing 100 ms of meter history (fed by `get_meters` polls plus the
/// fresh frame pushed here) is mean-removed, zero-padded to 128, and
/// returned peak-normalized (`spectrum` is empty for channels past the live
/// strip count, mirroring serve's per-tick bound).
///
/// Poll-based staleness contract: unlike the WS push (one message per
/// channel per ~10 Hz tick), invoke has no push — the UI polls this at 5 Hz
/// (200 ms), the same rate serve emits `gr` frames.
#[tauri::command]
async fn get_spectrum(state: State<'_>, ch: u16) -> Result<serde_json::Value, String> {
    if usize::from(ch) >= SPECTRUM_CHANNELS {
        return Err(format!(
            "spectrum capped at first {SPECTRUM_CHANNELS} channels, got ch{ch}"
        ));
    }
    let mut inner = state.lock().await;
    let frame = inner.driver.poll_meters();
    inner.rta.push_frame(&frame);
    let bins = inner.rta.spectrum(usize::from(ch));
    Ok(json!({"ch": ch, "bins": bins}))
}

/// Gate/comp gain reduction for one channel (dB reduction `>= 0`): the
/// pull-based mirror of serve's WS `gr` tick
/// (`{"type":"gr","ch":N,"gate":db,"comp":db}`).
///
/// Same semantics as serve's `gr_db`: xair reads the DYN blob cache
/// (`gate_gr`/`comp_gr` param path); no data yet (or out of range) falls
/// back to 0.0 so the poll never drops a frame — the UI treats 0.0 as "no
/// reduction", same as an idle gate. sim/dl32s have no GR source and always
/// read (0.0, 0.0). Same 16-channel cap as serve.
///
/// Poll-based staleness contract: serve pushes `gr` every second `meters`
/// tick (~5 Hz); invoke has no push, so the UI polls this at 5 Hz (200 ms)
/// for the same freshness.
#[tauri::command]
async fn get_gr(state: State<'_>, ch: u16) -> Result<serde_json::Value, String> {
    if usize::from(ch) >= GR_CHANNELS {
        return Err(format!(
            "gr capped at first {GR_CHANNELS} channels, got ch{ch}"
        ));
    }
    let inner = state.lock().await;
    if inner.kind != "xair" {
        return Ok(json!({"ch": ch, "gate": 0.0, "comp": 0.0}));
    }
    let id = StripId(ch);
    let gate = match inner.driver.get_param(id, "gate_gr").await {
        Ok(ParamValue::Float(v)) => v.max(0.0),
        _ => 0.0,
    };
    let comp = match inner.driver.get_param(id, "comp_gr").await {
        Ok(ParamValue::Float(v)) => v.max(0.0),
        _ => 0.0,
    };
    Ok(json!({"ch": ch, "gate": gate, "comp": comp}))
}

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(Inner::default()))
        .invoke_handler(tauri::generate_handler![
            connect,
            set_fader,
            set_mute,
            set_param,
            get_state,
            set_main_fader,
            get_main_fader,
            capture_scene,
            recall_scene,
            list_scenes,
            get_scene,
            delete_scene,
            get_meters,
            get_spectrum,
            get_gr
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
