//! Tauri 2 shell over the embedded `openmix-engine` session.
//!
//! The full `openmix-engine` mixer session runs in this process, so the app
//! needs no relay/server/LAN box — the desk is reached by the engine's own
//! discovery (broadcast + unicast on the phone's WiFi) and UDP drivers, same
//! as `openmix-serve` does on a relay host. These commands carry the SAME
//! Cmd/Event JSON the WS relay speaks (`openmix_engine::protocol`
//! serialization — no new protocol): the UI's `TauriTransport` sends one
//! Cmd JSON per `engine_cmd` and receives the resulting Event JSON frames;
//! `engine_poll` drains background frames (meters/spectrum/GR ticker,
//! poller) between writes. The WS path in `crates/serve` is untouched
//! (byte-identical for browsers).
//!
//! There is exactly one session stack here: the embedded engine. All mixer
//! logic lives in `openmix-engine`; this shell only moves Cmd/Event JSON.
//!
//! Event fan-out is push-based: after `engine_subscribe_push` the backend
//! emits every Event batch as an `openmix-event` Tauri event (JSON string
//! array payload) — once inline in `engine_cmd`/`engine_poll` for the frames
//! the call itself produced, and every 50 ms from a background task for
//! ticker frames (meters/spectrum/GR/poller) that no command produced. The
//! UI listens instead of polling, so an idle phone holds no 10 Hz IPC timer.
//! `engine_poll` stays as the fallback drain for tests and for clients that
//! never subscribed (it returns the same frames it would emit).
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use std::sync::Arc;

/// Tauri event carrying engine Event JSON frames (backend -> UI).
/// Payload: array of JSON strings, the exact `Event::to_json` texts the WS
/// relay would put on the socket (text-only; binary meters stay WS-only).
pub const OPENMIX_EVENT: &str = "openmix-event";

/// Tauri event announcing an available app update (backend -> UI).
/// Payload: `{ version, current_version }`. Emitted at most once per
/// process, after the startup check finds a newer signed release. No
/// frontend listener is required; without one the event is a no-op.
pub const UPDATER_EVENT: &str = "openmix-updater";

/// How often the background task forwards ticker frames to push clients.
const PUSH_TICK: std::time::Duration = std::time::Duration::from_millis(50);

/// Delay before the one-shot startup update check (lets the UI attach and
/// the desk link settle before spending a TLS round-trip).
const UPDATE_CHECK_DELAY: std::time::Duration = std::time::Duration::from_secs(10);

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
    /// Push fan-out target. Set by `engine_subscribe_push`; `None` until
    /// then, in which case this shell behaves exactly as before (every
    /// `engine_cmd`/`engine_poll` returns its frames, nothing is emitted).
    app: Option<AppHandle>,
    /// True once a UI client subscribed: drained frames are emitted as
    /// `openmix-event` in addition to being returned (push clients ignore
    /// the return value; poll-only clients never set this flag, so they
    /// can never observe a frame twice).
    push: bool,
    /// Whether the background ticker task is running (spawned once).
    push_task: bool,
}

impl Default for EngineSlot {
    fn default() -> Self {
        Self {
            engine: None,
            client: None,
            app: None,
            push: false,
            push_task: false,
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

/// Best-effort push of one drained batch to subscribed UI clients.
/// Empty batches send nothing; a missing listener (cold start) just drops
/// the batch — the frames were also returned to the caller, so nothing is
/// lost for clients that read return values.
fn emit_push(app: &AppHandle, frames: &[String]) {
    if frames.is_empty() {
        return;
    }
    let _ = app.emit(OPENMIX_EVENT, frames);
}

/// Drain plus conditional push: returns the frames (poll-fallback reads
/// the return value) and emits them when a push client subscribed.
fn drain_and_push(slot: &mut EngineSlot) -> Vec<String> {
    let frames = match slot.client.as_mut() {
        Some(client) => drain_client_events(client),
        None => Vec::new(),
    };
    if slot.push {
        if let Some(app) = slot.app.clone() {
            emit_push(&app, &frames);
        }
    }
    frames
}
/// Attach the UI client to the embedded engine (idempotent) and return its
/// client id. The attach snapshot (`you`, `state`, `caps`, …) is already
/// queued — push subscribers get it on the next tick, others call
/// `engine_poll` (or any `engine_cmd`) to collect it.
#[tauri::command]
async fn engine_attach(eslot: EngineState<'_>, label: Option<String>) -> Result<String, String> {
    let _ = label;
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await
}

/// Send one engine Cmd JSON (same `{"cmd":…}` the WS relay accepts;
/// parse failures come back as unicast `error` Events, never as a failed
/// call) and return all Event JSON frames it produced, plus any queued
/// background frames, in socket order. Push subscribers additionally get
/// the same batch as an `openmix-event` (and ignore this return value).
#[tauri::command]
async fn engine_cmd(eslot: EngineState<'_>, cmd: String) -> Result<Vec<String>, String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    let engine = slot.engine.clone().expect("engine just ensured");
    let id = slot
        .client
        .as_ref()
        .expect("client just ensured")
        .id
        .clone();
    // `handle_json` never fails the call itself (rejections arrive as
    // unicast `error` Events); it only skips unknown writers, and the
    // stored handle detaches solely via `engine_detach`.
    engine.handle_json(&id, &cmd).await;
    Ok(drain_and_push(&mut slot))
}

/// Drain background Event frames (meter ticker, param poller, discovery)
/// queued since the last `engine_cmd`/`engine_poll`. Poll fallback for
/// clients that never called `engine_subscribe_push` (and for tests);
/// push subscribers get background frames from the ticker task instead.
#[tauri::command]
async fn engine_poll(eslot: EngineState<'_>) -> Result<Vec<String>, String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    Ok(drain_and_push(&mut slot))
}

/// Subscribe this shell to push fan-out: from now on every drained batch
/// (command replies inline, ticker frames from the background task) is
/// emitted as `openmix-event`. Starts the 50 ms ticker task once.
/// Idempotent — repeat calls just refresh the stored handle.
#[tauri::command]
async fn engine_subscribe_push(app: AppHandle, eslot: EngineState<'_>) -> Result<(), String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    slot.app = Some(app.clone());
    slot.push = true;
    if !slot.push_task {
        slot.push_task = true;
        tauri::async_runtime::spawn(push_task(app));
    }
    Ok(())
}

/// Background ticker: every [`PUSH_TICK`] drain frames the commands did
/// not produce (meter/spectrum/GR ticker, poller, discovery) and push them
/// to subscribed clients. Poll-only shells never set `push`, so this task
/// never runs for them (and the command only exists to start it).
async fn push_task(app: AppHandle) {
    let mut tick = tokio::time::interval(PUSH_TICK);
    loop {
        tick.tick().await;
        let frames = {
            let state = app.state::<Mutex<EngineSlot>>();
            let mut slot = state.lock().await;
            if !slot.push {
                continue;
            }
            match slot.client.as_mut() {
                Some(client) => drain_client_events(client),
                None => Vec::new(),
            }
        };
        emit_push(&app, &frames);
    }
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

    /// `drain_and_push` on a fresh slot (no engine, no client, push off)
    /// returns empty without emitting — the flag-off path that keeps
    /// poll-only clients on the exact pre-push behaviour.
    #[tokio::test]
    async fn drain_and_push_empty_slot_is_empty() {
        let mut slot = EngineSlot::default();
        assert!(!slot.push);
        assert!(drain_and_push(&mut slot).is_empty());
    }
}

/// One-shot startup update check (best-effort, desktop only in practice —
/// the updater has no mobile backend). Unconfigured builds (empty
/// endpoints, the committed scaffold state) log once and skip; a found
/// update is logged and announced as [`UPDATER_EVENT`], never installed
/// automatically — mid-gig restarts are the operator's decision.
async fn check_for_updates(app: &AppHandle) {
    use tauri_plugin_updater::UpdaterExt;
    let updater = match app.updater_builder().build() {
        Ok(u) => u,
        Err(e) => {
            eprintln!("openmix: updater unavailable ({e:#}); skipping update check");
            return;
        }
    };
    match updater.check().await {
        Ok(Some(update)) => {
            eprintln!(
                "openmix: update available: {} (running {})",
                update.version, update.current_version
            );
            let _ = app.emit(
                UPDATER_EVENT,
                serde_json::json!({
                    "version": update.version,
                    "current_version": update.current_version,
                }),
            );
        }
        Ok(None) => eprintln!("openmix: up to date"),
        Err(e) => eprintln!("openmix: update check failed ({e:#})"),
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Mutex::new(EngineSlot::default()))
        .setup(|app| {
            // The update check is deliberately delayed and fire-and-forget:
            // boot latency and the first desk link matter more.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(UPDATE_CHECK_DELAY).await;
                check_for_updates(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_attach,
            engine_cmd,
            engine_poll,
            engine_subscribe_push,
            engine_detach
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
