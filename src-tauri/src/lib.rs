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
//! Event fan-out is push-based: after subscribing, the backend forwards
//! every Event batch over an ordered Tauri [`Channel`](tauri::ipc::Channel)
//! — JSON control frames as one `InvokeResponseBody::Json` batch (array of
//! `Event::to_json` strings, the exact texts the WS relay puts on the
//! socket) plus hot meter frames as `InvokeResponseBody::Raw` binary
//! (`Event::to_binary`: meters/GR/taps, same layout the WS relay uses;
//! spectrum stays JSON, like WS) — once inline in `engine_cmd`/
//! `engine_poll` for the frames the call itself produced, and from a
//! background task for ticker frames (meters/spectrum/GR/poller) that no
//! command produced. The task waits on the engine's `tick_notify`
//! (notified on every mailbox write) with the 50 ms [`PUSH_TICK`] only as
//! a fallback ceiling, so ticker frames go out within ~ms instead of
//! riding up to 50 ms of phase-added age against the engine's own 50 ms
//! meter tick. Drained frames collapse through a latest-only [`IpcOutbox`]
//! (slot-of-1 per meter stream, drops counted, `engine_stats` — the same
//! shape as the WS `Outbox`), so a stalled shell gets the freshest frame
//! per stream, never a stale-burst catch-up. Channels are ordered and
//! binary skips JSON entirely, which is exactly the meter ticker's case (20 Hz + fan-out,
//! [`ENGINE_DRAIN_CAP`] batches). The UI subscribes instead of polling,
//! so an idle phone holds no 10 Hz IPC timer. The legacy `openmix-event`
//! emit path (`engine_subscribe_push`) stays as the fallback for clients
//! that cannot pass a Channel (tests, foreign shells): JSON-only, no
//! binary. `engine_poll` stays as the fallback drain for clients that
//! never subscribed (it returns the same JSON frames it would push, never
//! binary).
//!
//! Encoding (both `to_json` and `to_binary`) happens AFTER the slot lock
//! is released: the lock only drains `Event`s and snapshots targets, so
//! serialization never serializes command handling against the 50 ms
//! ticker. When push is active `engine_cmd`/`engine_poll` return empty
//! (the frames already went over the Channel/event) to avoid a wasted
//! clone + serialization; poll-only clients get the frames as the return
//! value, exactly as before.
use tauri::async_runtime::Mutex;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter, Manager};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Tauri event carrying engine Event JSON frames (backend -> UI).
/// Legacy/fallback path — see `engine_subscribe_push`. Payload: array of
/// JSON strings, the exact `Event::to_json` texts the WS relay would put on
/// the socket (text-only; binary meters stay WS + Channel-only). Channel
/// subscribers (`engine_subscribe_channel`) never receive this event.
pub const OPENMIX_EVENT: &str = "openmix-event";

/// Tauri event announcing an available app update (backend -> UI).
/// Payload: `{ version, current_version }`. Emitted at most once per
/// process, after the startup check finds a newer signed release. No
/// frontend listener is required; without one the event is a no-op.
pub const UPDATER_EVENT: &str = "openmix-updater";

/// Fallback ceiling for the notify-driven push task: the engine wakes the
/// task on every mailbox write, so this only fires when the engine is
/// silent (or a wakeup was missed).
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

/// Per-shell latest-only outbox (P1), mirroring `crates/serve`'s `Outbox`
/// (slot-of-1 per meter stream, drops counted, stats).
///
/// Meter-class events collapse into bounded slots of 1 (`meters`, one slot
/// per tap id, per-channel `spectrum` / `gr`): storing a frame while the
/// previous one is still unsent replaces it and counts a drop. Everything
/// else queues in the reliable `control` FIFO. The push task / inline
/// command paths take control first, then one snapshot of the stream
/// slots — so a stalled shell never gets a stale-burst catch-up of old
/// meter frames, only the freshest per stream.
#[derive(Default)]
struct IpcOutbox {
    meters: Option<openmix_engine::Event>,
    meters_sent: u64,
    meters_dropped: u64,
    taps: HashMap<String, openmix_engine::Event>,
    spectrum: HashMap<u16, openmix_engine::Event>,
    spectrum_sent: u64,
    spectrum_dropped: u64,
    gr: HashMap<u16, openmix_engine::Event>,
    gr_sent: u64,
    gr_dropped: u64,
    control: VecDeque<openmix_engine::Event>,
}

impl IpcOutbox {
    /// Store one drained mailbox event: meter-class frames collapse into
    /// their slot (replacement counts a drop), control frames queue FIFO.
    fn store(&mut self, event: openmix_engine::Event) {
        use openmix_engine::Event as E;
        match event {
            E::Meters { .. } => {
                if self.meters.replace(event).is_some() {
                    self.meters_dropped += 1;
                }
            }
            E::Taps { ref tap, .. } => {
                if self.taps.insert(tap.clone(), event).is_some() {
                    // Taps fold into the meters counters (the `stats`
                    // shape is pinned, same as `crates/serve`).
                    self.meters_dropped += 1;
                }
            }
            E::Spectrum { ch, .. } => {
                if self.spectrum.insert(ch, event).is_some() {
                    self.spectrum_dropped += 1;
                }
            }
            E::Gr { ch, .. } => {
                if self.gr.insert(ch, event).is_some() {
                    self.gr_dropped += 1;
                }
            }
            other => self.control.push_back(other),
        }
    }

    /// Take the next deliverable batch: the whole control FIFO first
    /// (reliable order), then the freshest snapshot of each occupied
    /// stream slot. Sent counters advance at take time (delivery is
    /// best-effort, same as the Channel send below).
    fn take_batch(&mut self) -> Vec<openmix_engine::Event> {
        let mut out = Vec::with_capacity(
            self.control.len()
                + usize::from(self.meters.is_some())
                + self.taps.len()
                + self.spectrum.len()
                + self.gr.len(),
        );
        while let Some(ev) = self.control.pop_front() {
            out.push(ev);
        }
        if let Some(ev) = self.meters.take() {
            out.push(ev);
            self.meters_sent += 1;
        }
        if !self.taps.is_empty() {
            // Deterministic order for tests/encoding: sort tap ids.
            let mut keys: Vec<String> = self.taps.keys().cloned().collect();
            keys.sort();
            for k in keys {
                if let Some(ev) = self.taps.remove(&k) {
                    out.push(ev);
                    self.meters_sent += 1;
                }
            }
        }
        if !self.spectrum.is_empty() {
            let mut keys: Vec<u16> = self.spectrum.keys().copied().collect();
            keys.sort_unstable();
            for k in keys {
                if let Some(ev) = self.spectrum.remove(&k) {
                    out.push(ev);
                    self.spectrum_sent += 1;
                }
            }
        }
        if !self.gr.is_empty() {
            let mut keys: Vec<u16> = self.gr.keys().copied().collect();
            keys.sort_unstable();
            for k in keys {
                if let Some(ev) = self.gr.remove(&k) {
                    out.push(ev);
                    self.gr_sent += 1;
                }
            }
        }
        out
    }

    fn queue_depth(&self) -> usize {
        self.control.len()
            + usize::from(self.meters.is_some())
            + self.taps.len()
            + self.spectrum.len()
            + self.gr.len()
    }

    fn stats_json(&self) -> String {
        serde_json::json!({
            "type": "ipc_stats",
            "meters_sent": self.meters_sent,
            "meters_dropped": self.meters_dropped,
            "gr_sent": self.gr_sent,
            "gr_dropped": self.gr_dropped,
            "spectrum_sent": self.spectrum_sent,
            "spectrum_dropped": self.spectrum_dropped,
            "queue_depth": self.queue_depth(),
        })
        .to_string()
    }
}

/// In-process engine session behind the IPC commands: one shared `Engine`
/// (booted on sim so the app works with no desk) plus the single UI
/// client's handle. `ClientHandle.events` needs `&mut` for `try_recv`,
/// hence the async mutex.
struct EngineSlot {
    engine: Option<Arc<openmix_engine::Engine>>,
    client: Option<openmix_engine::ClientHandle>,
    /// Push fan-out targets. Set by the subscribe commands; `None` until
    /// then, in which case this shell behaves exactly as before (every
    /// `engine_cmd`/`engine_poll` returns its frames, nothing is pushed).
    /// The ordered [`Channel`](tauri::ipc::Channel) is preferred (set by
    /// `engine_subscribe_channel`); `app` is the legacy emit target (set by
    /// `engine_subscribe_push`, `openmix-event`). Only one is ever used per
    /// batch — channel wins when both are set. The channel carries
    /// `InvokeResponseBody`: `Json(batch)` for control frames (array of
    /// `Event::to_json` strings) plus one `Raw(bytes)` per hot meter frame
    /// (`Event::to_binary`), ordered.
    app: Option<AppHandle>,
    channel: Option<Channel<InvokeResponseBody>>,
    /// True once a UI client subscribed: drained frames are pushed (push
    /// clients ignore the `engine_cmd`/`engine_poll` return value, which is
    /// empty when push is active); poll-only clients never set this flag,
    /// so they can never observe a frame twice.
    push: bool,
    /// Per-client monotonic binary frame counter (`seq` in the
    /// `Event::to_binary` header; `t_ms` comes from `Engine::now_ms` at
    /// encode time). `Arc` so encoding can increment it AFTER the slot
    /// lock is released (lock-free across concurrent `engine_cmd` +
    /// ticker encodes).
    seq: Arc<AtomicU32>,
    /// Whether the background ticker task is running (spawned once).
    push_task: bool,
    /// Latest-only edge buffer (P1): drained mailbox frames collapse here
    /// before encode/deliver, so push bursts carry the freshest meters,
    /// not stale catch-up. See [`IpcOutbox`].
    outbox: IpcOutbox,
}

impl Default for EngineSlot {
    fn default() -> Self {
        Self {
            engine: None,
            client: None,
            app: None,
            channel: None,
            push: false,
            seq: Arc::new(AtomicU32::new(0)),
            push_task: false,
            outbox: IpcOutbox::default(),
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

/// Drain all currently-queued Event frames for the UI client as owned
/// [`Event`](openmix_engine::Event)s — NO encoding here. The caller encodes
/// (`to_json` / `to_binary`) AFTER the slot lock is released, so the
/// 50 ms ticker's serialization never blocks command handling.
fn drain_client_events(
    client: &mut openmix_engine::ClientHandle,
) -> Vec<openmix_engine::Event> {
    let mut out = Vec::new();
    while out.len() < ENGINE_DRAIN_CAP {
        match client.events.try_recv() {
            Ok(ev) => out.push(ev),
            Err(_) => break,
        }
    }
    out
}

/// Encode drained events for the legacy paths (emit + poll fallback): all
/// JSON (`Event::to_json`), byte-identical to the WS relay. Never binary.
fn encode_legacy(events: &[openmix_engine::Event]) -> Vec<String> {
    events.iter().map(|ev| ev.to_json()).collect()
}

/// Encode drained events for the ordered Channel: control frames (and
/// `spectrum`, whose `to_binary` is `None` like WS) as a JSON batch, hot
/// meter frames (`meters`/`gr` when `binary_meters`, `taps` when `taps`)
/// as one binary frame each (`Event::to_binary`, same layout WS uses).
/// `seq` is the per-client monotonic counter (fetch-add per binary frame),
/// `t_ms` the engine monotonic clock. Mirrors `crates/serve`'s
/// `encode_stream` split (binary flag vs taps flag).
fn encode_channel(
    events: &[openmix_engine::Event],
    binary_meters: bool,
    taps: bool,
    seq: &AtomicU32,
    t_ms: u32,
) -> (Vec<String>, Vec<Vec<u8>>) {
    let mut json = Vec::new();
    let mut bin = Vec::new();
    for ev in events {
        let want_bin = match ev {
            openmix_engine::Event::Meters { .. }
            | openmix_engine::Event::Gr { .. } => binary_meters,
            openmix_engine::Event::Taps { .. } => taps,
            _ => false,
        };
        if want_bin {
            let s = seq.fetch_add(1, Ordering::Relaxed);
            if let Some(bytes) = ev.to_binary(s, t_ms) {
                bin.push(bytes);
                continue;
            }
        }
        json.push(ev.to_json());
    }
    (json, bin)
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

/// Snapshot the push targets while the slot lock is held. The caller
/// delivers outside the lock (webview eval must not serialize the 50 ms
/// ticker against command handling). Also clones the binary `seq` counter
/// (`Arc`, lock-free outside) and the engine + client id for `t_ms` /
/// hello-preference reads (engine is internally synchronized).
fn push_targets(
    slot: &EngineSlot,
) -> (
    Option<Channel<InvokeResponseBody>>,
    Option<AppHandle>,
    Arc<AtomicU32>,
) {
    if slot.push {
        (slot.channel.clone(), slot.app.clone(), Arc::clone(&slot.seq))
    } else {
        (None, None, Arc::clone(&slot.seq))
    }
}

/// Deliver one drained batch over the ordered Channel: the JSON batch as a
/// single `Json` message (array of `Event::to_json` strings), then one
/// `Raw` message per binary meter frame (each arrives in JS as an
/// `ArrayBuffer`). Ordered — the JS side observes sends in call order.
/// Empty batches send nothing.
fn deliver_channel(
    channel: &Channel<InvokeResponseBody>,
    json_batch: &[String],
    binaries: Vec<Vec<u8>>,
) {
    if !json_batch.is_empty() {
        if let Ok(payload) = serde_json::to_string(json_batch) {
            let _ = channel.send(InvokeResponseBody::Json(payload));
        }
    }
    for bytes in binaries {
        let _ = channel.send(InvokeResponseBody::Raw(bytes));
    }
}

/// Deliver one drained batch: ordered [`Channel`](tauri::ipc::Channel)
/// when a channel subscriber exists (JSON batch + binary frames),
/// otherwise the legacy `openmix-event` emit (JSON batch only — legacy
/// listeners cannot receive binary). Never both — push clients read
/// exactly one path.
fn deliver_push(
    channel: Option<Channel<InvokeResponseBody>>,
    app: Option<AppHandle>,
    json_batch: &[String],
    binaries: Vec<Vec<u8>>,
) {
    if json_batch.is_empty() && binaries.is_empty() {
        return;
    }
    if let Some(ch) = channel {
        deliver_channel(&ch, json_batch, binaries);
        return;
    }
    if let Some(app) = app {
        emit_push(&app, json_batch);
    }
}

/// Drain events under the slot lock, collapse meter-class frames through
/// the latest-only [`IpcOutbox`], and snapshot push targets + engine
/// handle for post-lock encoding. Returns `(batch, channel, app, engine,
/// client_id, seq)` where `batch` is control FIFO + freshest stream
/// snapshot (never a stale-burst catch-up). Callers encode AFTER the lock
/// is released, then deliver and (when push is active) return empty to
/// avoid a wasted clone/serialization.
#[allow(clippy::type_complexity)]
fn drain_for_push(
    slot: &mut EngineSlot,
) -> (
    Vec<openmix_engine::Event>,
    Option<Channel<InvokeResponseBody>>,
    Option<AppHandle>,
    Option<Arc<openmix_engine::Engine>>,
    Option<String>,
    Arc<AtomicU32>,
) {
    let drained = match slot.client.as_mut() {
        Some(client) => drain_client_events(client),
        None => Vec::new(),
    };
    for ev in drained {
        slot.outbox.store(ev);
    }
    let batch = slot.outbox.take_batch();
    let (channel, app, seq) = push_targets(slot);
    let engine = slot.engine.clone();
    let id = slot.client.as_ref().map(|c| c.id.clone());
    (batch, channel, app, engine, id, seq)
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
/// call). Poll-only clients get all Event JSON frames it produced, plus any
/// queued background frames, as the return value. Push subscribers get the
/// same batch over their Channel (JSON batch + binary meter frames) or
/// legacy event and this returns empty (no wasted clone/serialization).
#[tauri::command]
async fn engine_cmd(eslot: EngineState<'_>, cmd: String) -> Result<Vec<String>, String> {
    // Ensure + identify under the lock, then RELEASE it across the engine
    // call: scans/probes/connects block on the LAN (seconds), and holding
    // the slot mutex that long starves the 50 ms push ticker — its backlog
    // then buries the very devices/link frames the command produced.
    // The engine is internally synchronized (the WS relay already drives
    // it from many concurrent sockets), so concurrent commands are safe.
    let (engine, id) = {
        let mut slot = eslot.lock().await;
        let id = ensure_engine_client(&mut slot).await?;
        (
            slot.engine.clone().expect("engine just ensured"),
            id,
        )
    };
    // `handle_json` never fails the call itself (rejections arrive as
    // unicast `error` Events); it only skips unknown writers, and the
    // stored handle detaches solely via `engine_detach`.
    engine.handle_json(&id, &cmd).await;
    // A concurrent `engine_detach` drops the client: nothing to drain.
    // Events are drained under the lock; encoding + delivery happen after
    // it is released (serialization must not block the next command).
    // ENCODING HAPPENS OUTSIDE THE LOCK (P1): `drain_for_push` only moves
    // `Event`s out of the mailbox and snapshots targets.
    let (events, channel, app, _engine, _cid, seq) = {
        let mut slot = eslot.lock().await;
        if slot.client.as_ref().is_some_and(|c| c.id == id) {
            drain_for_push(&mut slot)
        } else {
            (
                Vec::new(),
                None,
                None,
                None,
                None,
                Arc::clone(&slot.seq),
            )
        }
    };
    let push_active = channel.is_some() || app.is_some();
    if let Some(ch) = channel {
        let binary_meters = engine.client_binary_meters(&id);
        let taps = engine.client_taps(&id);
        let t_ms = engine.now_ms();
        let (json_batch, binaries) =
            encode_channel(&events, binary_meters, taps, &seq, t_ms);
        deliver_push(Some(ch), None, &json_batch, binaries);
        // Push consumed the batch: return empty, skip the return-value
        // clone/serialization entirely.
        return Ok(Vec::new());
    }
    // Legacy emit + poll fallback: JSON-only (legacy listeners cannot
    // receive binary; poll returns `Vec<String>` by shape).
    let frames = encode_legacy(&events);
    if let Some(app) = app {
        deliver_push(None, Some(app), &frames, Vec::new());
        return Ok(Vec::new());
    }
    debug_assert!(!push_active, "push without a target");
    Ok(frames)
}

/// Drain background Event frames (meter ticker, param poller, discovery)
/// queued since the last `engine_cmd`/`engine_poll`. Poll fallback for
/// clients that never subscribed (and for tests): returns the JSON frames.
/// Push subscribers get background frames over their Channel (JSON +
/// binary) or legacy event instead, and this returns empty.
#[tauri::command]
async fn engine_poll(eslot: EngineState<'_>) -> Result<Vec<String>, String> {
    let (events, channel, app, engine_opt, id_opt, seq) = {
        let mut slot = eslot.lock().await;
        ensure_engine_client(&mut slot).await?;
        drain_for_push(&mut slot)
    };
    if let Some(ch) = channel {
        let (binary_meters, taps, t_ms) = match (&engine_opt, &id_opt) {
            (Some(engine), Some(id)) => (
                engine.client_binary_meters(id),
                engine.client_taps(id),
                engine.now_ms(),
            ),
            _ => (false, false, 0),
        };
        let (json_batch, binaries) =
            encode_channel(&events, binary_meters, taps, &seq, t_ms);
        deliver_push(Some(ch), None, &json_batch, binaries);
        return Ok(Vec::new());
    }
    let frames = encode_legacy(&events);
    if let Some(app) = app {
        deliver_push(None, Some(app), &frames, Vec::new());
        return Ok(Vec::new());
    }
    Ok(frames)
}

/// Subscribe this shell to push fan-out over the legacy `openmix-event`
/// Tauri event: from now on every drained batch (command replies inline,
/// ticker frames from the background task) is emitted. Starts the 50 ms
/// ticker task once. Idempotent — repeat calls just refresh the stored
/// handle. Prefer `engine_subscribe_channel` (ordered, no eval); this stays
/// for clients that cannot pass a Channel.
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

/// Subscribe this shell to push fan-out over an ordered Tauri Channel:
/// from now on every drained batch (command replies inline, ticker frames
/// from the background task) is sent as one `Json` batch (control frames,
/// array of `Event::to_json` strings) plus one `Raw` message per hot meter
/// frame (`Event::to_binary`: meters/GR/taps) — ordered, binary skips JSON
/// entirely, the fast path for the 20 Hz meter ticker. Starts the 50 ms
/// ticker task once. Idempotent — repeat calls replace the stored channel.
/// The channel outranks the legacy emit target: when both are set, batches
/// go over the channel only (legacy emit is JSON-only).
#[allow(non_snake_case)]
#[tauri::command]
async fn engine_subscribe_channel(
    app: AppHandle,
    eslot: EngineState<'_>,
    onEvent: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let mut slot = eslot.lock().await;
    ensure_engine_client(&mut slot).await?;
    slot.channel = Some(onEvent);
    slot.push = true;
    if !slot.push_task {
        slot.push_task = true;
        tauri::async_runtime::spawn(push_task(app));
    }
    Ok(())
}

/// Background ticker (P0): notify-driven drain of frames the commands did
/// not produce (meter/spectrum/GR ticker, poller, discovery), pushed to
/// subscribed clients. Poll-only shells never set `push`, so this task
/// never runs for them (the commands exist only to start it). Draining
/// happens under the slot lock; encoding + delivery happen outside it.
///
/// Wakeups come from the engine's [`tick_notify`](openmix_engine::Engine::tick_notify)
/// (notified on every mailbox write: broadcasts, unicasts, attach
/// snapshot), so ticker frames go out within ~ms instead of riding up to
/// 50 ms of phase-added age against the engine's own 50 ms meter tick.
/// [`PUSH_TICK`] stays as a fallback ceiling (missed wakeups, detach
/// races) and keeps the cadence bounded when the engine is silent.
async fn push_task(app: AppHandle) {
    let mut tick = tokio::time::interval(PUSH_TICK);
    // First wake immediately so the attach snapshot goes out without
    // waiting for the first meter tick.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        // Snapshot the notify handle without holding the slot lock across
        // the wait. `Notify` stores one permit while no waiter is parked,
        // so a broadcast between drain and wait still wakes the next wait
        // (coalesced, never lost — and we always drain fully anyway).
        let notify = {
            let state = app.state::<Mutex<EngineSlot>>();
            let slot = state.lock().await;
            if !slot.push {
                None
            } else {
                slot.engine.as_ref().map(|e| e.tick_notify())
            }
        };
        match notify {
            Some(n) => {
                tokio::select! {
                    _ = n.notified() => {},
                    _ = tick.tick() => {},
                }
            }
            None => {
                tick.tick().await;
            }
        }
        let (events, channel, target, engine_opt, id_opt, seq) = {
            let state = app.state::<Mutex<EngineSlot>>();
            let mut slot = state.lock().await;
            if !slot.push {
                continue;
            }
            drain_for_push(&mut slot)
        };
        if events.is_empty() {
            continue;
        }
        if let Some(ch) = channel {
            let (binary_meters, taps, t_ms) = match (&engine_opt, &id_opt) {
                (Some(engine), Some(id)) => (
                    engine.client_binary_meters(id),
                    engine.client_taps(id),
                    engine.now_ms(),
                ),
                _ => (false, false, 0),
            };
            let (json_batch, binaries) =
                encode_channel(&events, binary_meters, taps, &seq, t_ms);
            deliver_push(Some(ch), None, &json_batch, binaries);
            continue;
        }
        // Legacy emit path: JSON-only.
        let frames = encode_legacy(&events);
        deliver_push(None, target, &frames, Vec::new());
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

/// App version (single source of truth: root `VERSION`, fanned out into
/// every manifest by `scripts/sync-version.mjs`). `CARGO_PKG_VERSION` is
/// the src-tauri copy, so this always matches the Tauri About dialog and
/// the updater's `current_version`.
#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Latest-only edge stats (P1): cumulative IPC outbox counters behind the
/// push fan-out (`meters_sent`, `meters_dropped`, `gr_*`, `spectrum_*`,
/// `queue_depth`), same shape as the WS `stats` reply minus the transport
/// flag. Cheap (one lock, no engine call): poll it to confirm the edge is
/// collapsing instead of bursting.
#[tauri::command]
async fn engine_stats(eslot: EngineState<'_>) -> Result<String, String> {
    let slot = eslot.lock().await;
    Ok(slot.outbox.stats_json())
}

#[cfg(test)]
mod engine_ipc_tests {
    use super::*;

    /// Drive the REAL embedded stack (Engine + sim, no network): attach,
    /// snapshot prefix, one write round-trip, one rejection. Guards the
    /// exact path the Tauri commands wrap (`ensure` + `handle_json` +
    /// `drain` + post-lock encode), minus the Tauri `State` shell.
    #[tokio::test]
    async fn embedded_engine_cmd_event_round_trip() {
        let engine = openmix_engine::Engine::new(openmix_engine::EngineConfig::default());
        engine.connect("sim", None).await.expect("sim boot");
        let mut client = engine.attach(Some("test-ui".to_string())).await;

        // Attach snapshot: `you` first, then the world (state/caps/...).
        // Drain returns `Event`s (no encoding under the lock); the type
        // tag read goes through `Event::tag` instead of JSON parsing.
        let snap = drain_client_events(&mut client);
        assert!(!snap.is_empty(), "attach must queue a snapshot");
        let types: Vec<String> = snap.iter().map(|e| e.tag().to_string()).collect();
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
        // Legacy encode path (poll fallback) stays all-JSON.
        assert!(!encode_legacy(&after_write).is_empty());

        // Unknown command spelling: unicast `error`, connection stays up.
        engine
            .handle_json(&client.id, r#"{"cmd":"bogus_command"}"#)
            .await;
        let errs = drain_client_events(&mut client);
        let err_json = encode_legacy(&errs).join("\n");
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
    /// Encoding happens outside the lock; the cap applies to drained
    /// `Event`s, so the bound holds before any serialization.
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

    /// `drain_for_push` on a fresh slot (no engine, no client, push off)
    /// returns empty with no targets — the flag-off path that keeps
    /// poll-only clients on the exact pre-push behaviour. Encoding is the
    /// caller's job (outside the lock), so this only asserts the drain.
    #[tokio::test]
    async fn drain_for_push_empty_slot_is_empty() {
        let mut slot = EngineSlot::default();
        assert!(!slot.push);
        let (events, channel, app, engine, id, _seq) = drain_for_push(&mut slot);
        assert!(events.is_empty());
        assert!(channel.is_none());
        assert!(app.is_none());
        assert!(engine.is_none());
        assert!(id.is_none());
    }

    /// Channel outranks the legacy emit target: `deliver_push` sends over
    /// the channel and never touches the event path. A Rust-side channel
    /// (`Channel::new`) lets the test observe this without a webview.
    /// JSON batches arrive as `Json`, binary meter frames as `Raw`.
    #[tokio::test]
    async fn deliver_push_prefers_channel_over_emit() {
        use std::sync::Mutex as StdMutex;
        let seen: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let seen_clone = seen.clone();
        let seen_bin: Arc<StdMutex<Vec<Vec<u8>>>> = Arc::new(StdMutex::new(Vec::new()));
        let seen_bin_clone = seen_bin.clone();
        let channel: Channel<InvokeResponseBody> = Channel::new(move |body| {
            match body {
                tauri::ipc::InvokeResponseBody::Json(json) => {
                    let batch: Vec<String> =
                        serde_json::from_str(&json).expect("channel JSON batch");
                    seen_clone.lock().unwrap().extend(batch);
                }
                tauri::ipc::InvokeResponseBody::Raw(bytes) => {
                    seen_bin_clone.lock().unwrap().push(bytes);
                }
            }
            Ok(())
        });
        let frames = vec![r#"{"type":"meters"}"#.to_string()];
        deliver_push(Some(channel), None, &frames, vec![vec![1u8, 1, 0]]);
        assert_eq!(*seen.lock().unwrap(), frames);
        assert_eq!(seen_bin.lock().unwrap().len(), 1);
        // Empty batches send nothing on any path (no panic, no emit).
        deliver_push(None, None, &[], Vec::new());
    }

    /// Binary channel split: `meters`/`gr`/`taps` encode via `to_binary`
    /// when opted in (hello flags), everything else stays JSON — the same
    /// split the WS relay uses. `seq` increments once per binary frame,
    /// `t_ms` rides the header (SPEC 2a: monotonic sequence + timestamp).
    #[tokio::test]
    async fn encode_channel_splits_binary_and_json() {
        use openmix_engine::protocol::Event;
        let seq = AtomicU32::new(7);
        let events = vec![
            Event::Meters { levels: vec![0.5, 0.25] },
            Event::Gr { ch: 1, gate: 2.0, comp: 3.0 },
            Event::Taps { tap: "in".to_string(), levels: vec![0.5] },
            Event::Spectrum { ch: 0, bins: vec![0.1] },
            Event::Error { message: "x".to_string(), locked_by: None, ch: None },
        ];
        let (json, bin) = encode_channel(&events, true, true, &seq, 1234);
        // meters + gr + taps -> binary; spectrum + error -> JSON.
        assert_eq!(bin.len(), 3, "meters/gr/taps go binary");
        assert_eq!(json.len(), 2, "spectrum + control stay JSON");
        assert_eq!(seq.load(Ordering::Relaxed), 10);
        // Binary headers carry seq/t_ms (first frame seq 7, then 8, 9).
        for (i, b) in bin.iter().enumerate() {
            assert!(b.len() >= openmix_engine::protocol::BIN_HEADER_LEN);
            let seq_le = u32::from_le_bytes(b[4..8].try_into().unwrap());
            let t_le = u32::from_le_bytes(b[8..12].try_into().unwrap());
            assert_eq!(seq_le, 7 + i as u32);
            assert_eq!(t_le, 1234);
        }
        // Opt-out: everything stays JSON, seq untouched.
        let seq2 = AtomicU32::new(0);
        let (json2, bin2) = encode_channel(&events, false, false, &seq2, 0);
        assert!(bin2.is_empty());
        assert_eq!(json2.len(), events.len());
        assert_eq!(seq2.load(Ordering::Relaxed), 0);
    }

    /// Latest-only edge buffer (P1): repeated meter-class frames collapse
    /// to the freshest per slot (drop counted), control stays FIFO and
    /// reliable, and `take_batch` emits control first, then the stream
    /// snapshot — no stale-burst catch-up.
    #[test]
    fn ipc_outbox_collapses_meter_streams() {
        use openmix_engine::Event;
        let mut o = IpcOutbox::default();
        // Two meter frames with no take in between: one drop, depth 1.
        o.store(Event::Meters { levels: vec![0.1] });
        o.store(Event::Meters { levels: vec![0.2] });
        assert_eq!(o.meters_dropped, 1);
        assert_eq!(o.queue_depth(), 1);
        // Per-channel spectrum/gr slots are independent per channel.
        o.store(Event::Spectrum { ch: 3, bins: vec![] });
        o.store(Event::Gr { ch: 3, gate: 1.0, comp: 0.0 });
        assert_eq!(o.queue_depth(), 3);
        o.store(Event::Spectrum { ch: 3, bins: vec![] });
        assert_eq!(o.spectrum_dropped, 1);
        o.store(Event::Gr { ch: 4, gate: 0.0, comp: 0.0 });
        assert_eq!(o.gr_dropped, 0, "other channels do not collide");
        // Same-tap replacement folds into the meters drop counter.
        o.store(Event::Taps { tap: "in".into(), levels: vec![0.1] });
        o.store(Event::Taps { tap: "in".into(), levels: vec![0.3] });
        assert_eq!(o.meters_dropped, 2);
        // Control first, then streams; sent counters advance at take.
        o.store(Event::Error { message: "x".into(), locked_by: None, ch: None });
        let batch = o.take_batch();
        assert!(matches!(batch[0], Event::Error { .. }), "control first");
        assert_eq!(o.meters_sent, 2, "meters + taps");
        assert_eq!(o.spectrum_sent, 1);
        assert_eq!(o.gr_sent, 2);
        assert_eq!(o.queue_depth(), 0);
        // Stats shape mirrors the WS `stats` reply (minus transport flag).
        let v: serde_json::Value =
            serde_json::from_str(&o.stats_json()).expect("stats is JSON");
        assert_eq!(v.get("type"), Some(&serde_json::json!("ipc_stats")));
        assert_eq!(v.get("meters_dropped"), Some(&serde_json::json!(2)));
        assert_eq!(v.get("spectrum_dropped"), Some(&serde_json::json!(1)));
    }

    /// Engine notify (P0): a command broadcast wakes `tick_notify`
    /// waiters — the hook the push task waits on instead of a second
    /// 50 ms poll.
    #[tokio::test]
    async fn engine_tick_notify_fires_on_command() {
        let engine = openmix_engine::Engine::new(openmix_engine::EngineConfig::default());
        engine.connect("sim", None).await.expect("sim boot");
        let client = engine.attach(Some("notify-test".to_string())).await;
        let id = client.id.clone();
        // Drain the attach snapshot so the mailbox is idle.
        let mut slot = EngineSlot {
            client: Some(client),
            engine: Some(engine.clone()),
            ..EngineSlot::default()
        };
        let _ = drain_client_events(slot.client.as_mut().unwrap());
        let notify = engine.tick_notify();
        engine
            .handle_json(&id, r#"{"cmd":"fader","ch":0,"value":0.5}"#)
            .await;
        // The future is created after the broadcast; `Notify` stores one
        // permit while no waiter is parked, so this still completes
        // (coalesced, never lost).
        tokio::time::timeout(std::time::Duration::from_secs(2), notify.notified())
            .await
            .expect("tick_notify fires on broadcast");
        // The broadcast actually landed in the mailbox (drain finds it).
        let batch = drain_for_push(&mut slot).0;
        assert!(
            batch.iter().any(|e| e.tag() == "params" || e.tag() == "state"),
            "write broadcast reaches the IPC drain (got {:?})",
            batch.iter().map(|e| e.tag().to_string()).collect::<Vec<_>>()
        );
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
            engine_subscribe_channel,
            engine_stats,
            engine_detach,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
