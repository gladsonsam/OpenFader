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
use tauri::async_runtime::Mutex;
use std::sync::Arc;
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

// ---------------------------------------------------------------------------
// Embedded engine (phone independence): the full `openmix-engine` session
// runs in this process, so the app needs no relay/server/LAN box — the
// desk is reached by the engine's own discovery (broadcast + unicast on
// the phone's WiFi) and UDP drivers, same as `openmix-serve` does on a
// relay host. These commands carry the SAME Cmd/Event JSON the WS relay
// speaks (`openmix_engine::protocol` serialization — no new protocol):
// the UI's `TauriTransport` sends one Cmd JSON per `engine_cmd` and
// receives the resulting Event JSON frames; `engine_poll` drains
// background frames (meters/spectrum/GR ticker, poller) between writes.
// The WS path in `crates/serve` is untouched (byte-identical for browsers).
// ---------------------------------------------------------------------------

/// Max Event frames returned per `engine_cmd`/`engine_poll` call: bounds
/// one IPC payload (the phone polls at ~10 Hz; the meter ticker alone can
/// queue dozens of frames per interval at 20 Hz + GR + spectrum fan-out).
/// Overflow stays queued for the next poll — control frames are never
/// dropped, only deferred by one interval.
const ENGINE_DRAIN_CAP: usize = 512;

/// In-process engine session behind the IPC commands: one shared `Engine`
/// (booted on sim so the app works with no desk) plus the single UI
/// client's handle. `ClientHandle.events` needs `&mut` for `try_recv`,
/// hence the async mutex.
struct EngineSlot {
    engine: Option<Arc<openmix_engine::Engine>>,
    client: Option<openmix_engine::ClientHandle>,
}

impl Default for EngineSlot {
    fn default() -> Self {
        Self {
            engine: None,
            client: None,
        }
    }
}

type EngineState<'r> = tauri::State<'r, Mutex<EngineSlot>>;

/// Ensure the embedded engine exists (booted on `sim`, like `serve`
/// with default options) and the UI client is attached. Idempotent:
/// repeat calls reuse the session.
async fn ensure_engine_client(slot: &mut EngineSlot) -> Result<String, String> {
    if slot.engine.is_none() {
        let engine = openmix_engine::Engine::new(openmix_engine::EngineConfig::default());
        // Sim boot must succeed; a failure here means the engine itself is
        // broken (sim is in-process, no network). Propagate loudly.
        engine
            .connect("sim", None)
            .await
            .map_err(|e| format!("embedded engine sim boot failed: {e:#}"))?;
        slot.engine = Some(engine);
    }
    let engine = slot.engine.clone().expect("engine just ensured");
    if slot.client.is_none() {
        let handle = engine.attach(Some("tauri-ui".to_string())).await;
        let id = handle.id.clone();
        slot.client = Some(handle);
        return Ok(id);
    }
    Ok(slot.client.as_ref().expect("client just ensured").id.clone())
}

/// Drain all currently-queued Event frames for the UI client as wire JSON
/// (`Event::to_json` — the exact strings the WS relay puts on the socket).
fn drain_client_events(client: &mut openmix_engine::ClientHandle) -> Vec<String> {
    let mut out = Vec::new();
    while out.len() < ENGINE_DRAIN_CAP {
        match client.events.try_recv() {
            Ok(ev) => out.push(ev.to_json()),
            Err(_) => break,
        }
    }
    out
}

/// Attach the UI client to the embedded engine (idempotent) and return its
/// client id. The attach snapshot (`you`, `state`, `caps`, …) is already
/// queued — call `engine_poll` (or any `engine_cmd`) to collect it.
#[tauri::command]
async fn engine_attach(eslot: EngineState<'_>, label: Option<String>) -> Result<String, String> {
    let _ = label;
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await
}

/// Send one engine Cmd JSON (same `{"cmd":…}` the WS relay accepts;
/// parse failures come back as unicast `error` Events, never as a failed
/// call) and return all Event JSON frames it produced, plus any queued
/// background frames, in socket order.
#[tauri::command]
async fn engine_cmd(eslot: EngineState<'_>, cmd: String) -> Result<Vec<String>, String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    let engine = slot.engine.clone().expect("engine just ensured");
    let client = slot.client.as_mut().expect("client just ensured");
    let id = client.id.clone();
    // `handle_json` never fails the call itself (rejections arrive as
    // unicast `error` Events); it only skips unknown writers, and the
    // stored handle detaches solely via `engine_detach`.
    engine.handle_json(&id, &cmd).await;
    Ok(drain_client_events(client))
}

/// Drain background Event frames (meter ticker, param poller, discovery)
/// queued since the last `engine_cmd`/`engine_poll`. The UI polls this at
/// ~10 Hz; control writes go through `engine_cmd`.
#[tauri::command]
async fn engine_poll(eslot: EngineState<'_>) -> Result<Vec<String>, String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    let client = slot.client.as_mut().expect("client just ensured");
    Ok(drain_client_events(client))
}

/// Detach the UI client (engine keeps running; next `engine_cmd` /
/// `engine_poll` / `engine_attach` re-attaches with a fresh snapshot).
#[tauri::command]
async fn engine_detach(eslot: EngineState<'_>) -> Result<(), String> {
    let mut slot = eslot.lock().await;
    if let Some(client) = slot.client.take() {
        client.detach().await;
    }
    Ok(())
}

#[cfg(test)]
mod engine_ipc_tests {
    use super::*;

    /// Drive the REAL embedded stack (Engine + sim, no network): attach,
    /// snapshot prefix, one write round-trip, one rejection. Guards the
    /// exact path the Tauri commands wrap (`ensure` + `handle_json` +
    /// `drain`), minus the Tauri `State` shell.
    #[tokio::test]
    async fn embedded_engine_cmd_event_round_trip() {
        let engine = openmix_engine::Engine::new(openmix_engine::EngineConfig::default());
        engine.connect("sim", None).await.expect("sim boot");
        let mut client = engine.attach(Some("test-ui".to_string())).await;

        // Attach snapshot: `you` first, then the world (state/caps/...).
        let snap = drain_client_events(&mut client);
        assert!(!snap.is_empty(), "attach must queue a snapshot");
        let types: Vec<String> = snap
            .iter()
            .filter_map(|s| {
                serde_json::from_str::<serde_json::Value>(s)
                    .ok()
                    .and_then(|v| v.get("type")?.as_str().map(str::to_string))
            })
            .collect();
        assert_eq!(types[0], "you", "snapshot opens with `you`");
        for want in ["state", "caps", "devices", "link"] {
            assert!(
                types.contains(&want.to_string()),
                "snapshot carries `{want}` (got {types:?})"
            );
        }

        // Write round-trip through the WS command spelling.
        engine
            .handle_json(&client.id, r#"{"cmd":"fader","ch":0,"value":0.5}"#)
            .await;
        let after_write = drain_client_events(&mut client);
        assert!(
            !after_write.is_empty(),
            "fader write must produce broadcast frame(s)"
        );

        // Unknown command spelling: unicast `error`, connection stays up.
        engine
            .handle_json(&client.id, r#"{"cmd":"bogus_command"}"#)
            .await;
        let errs = drain_client_events(&mut client);
        let err_json = errs.join("\n");
        assert!(
            err_json.contains(r#""type":"error""#) && err_json.contains("unknown cmd"),
            "unknown cmd must unicast an error event (got {err_json:?})"
        );

        // Client still live: a follow-up write still produces frames.
        engine
            .handle_json(&client.id, r#"{"cmd":"mute","ch":1,"muted":true}"#)
            .await;
        assert!(
            !drain_client_events(&mut client).is_empty(),
            "client survives the rejected command"
        );
    }

    /// The drain helper never errors and stays capped on an idle client.
    #[tokio::test]
    async fn embedded_engine_drain_stays_capped() {
        let engine = openmix_engine::Engine::new(openmix_engine::EngineConfig::default());
        engine.connect("sim", None).await.expect("sim boot");
        let mut client = engine.attach(Some("idle-ui".to_string())).await;
        let _ = drain_client_events(&mut client);
        // Poller/meter ticks may queue frames at any moment; drain thrice
        // and only assert the bound (freshness, not emptiness).
        for _ in 0..3 {
            let frames = drain_client_events(&mut client);
            assert!(frames.len() <= ENGINE_DRAIN_CAP, "drain is capped");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_haptics::init())
        .plugin(tauri_plugin_edge_to_edge::init())
        .plugin(tauri_plugin_keep_screen_on::init())
        .manage(Mutex::new(Inner::default()))
        .manage(Mutex::new(EngineSlot::default()))
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
            get_gr,
            engine_attach,
            engine_cmd,
            engine_poll,
            engine_detach
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
