//! FRB entry points (`crate::api`): thin Cmd/Event JSON movers over the
//! embedded `openmix-engine` session. No mixer logic lives here.
//!
//! TODO (before real use): meter collapse latest-only. [`poll`] currently
//! drains everything up to `cap`; a stalled Dart poll loop gets a stale
//! burst instead of the freshest frame per stream. Port the slot-of-1
//! per-meter-stream outbox pattern (drops counted) before raising the
//! poll rate past a smoke test.

use crate::{DRAIN_CAP_DEFAULT, DRAIN_CAP_MAX, clients, engine, rt, set_engine};
use openmix_engine::{Engine, EngineConfig};

/// Boot the embedded engine on `sim` (idempotent) and attach the
/// `app` client. Returns the client id Dart passes to every other
/// call. Repeat calls attach a NEW client on the same session.
#[flutter_rust_bridge::frb(sync)]
pub fn boot() -> String {
    let eng = match engine() {
        Some(e) => e,
        None => {
            let e = Engine::new(EngineConfig::default());
            // Sim boot has no network: a failure means the engine itself is
            // broken (loud-propagate: fail fast).
            rt().block_on(e.connect("sim", None))
                .expect("openmix_flutter_bridge: embedded sim boot");
            set_engine(e.clone());
            e
        }
    };
    let handle = rt().block_on(eng.attach(Some("app".to_string())));
    let id = handle.id.clone();
    clients().lock().unwrap().insert(id.clone(), handle);
    id
}

/// Send one engine Cmd JSON (`{"cmd":...}`, the WS relay spelling).
/// Rejections arrive as unicast `error` Events in the client's mailbox
/// (collect them with [`poll`]); unknown clients are ignored.
#[flutter_rust_bridge::frb]
pub async fn cmd(client_id: String, json: String) {
    if let Some(eng) = engine() {
        eng.handle_json(&client_id, &json).await;
    }
}

/// Drain queued Event JSON frames (`Event::to_json`, byte-identical to the
/// WS relay) for `client_id`, up to `cap` frames (`0` = default).
/// Overflow stays queued for the next call. Unknown clients get `[]`.
#[flutter_rust_bridge::frb]
pub async fn poll(client_id: String, cap: u32) -> Vec<String> {
    let cap = if cap == 0 {
        DRAIN_CAP_DEFAULT
    } else {
        (cap as usize).min(DRAIN_CAP_MAX)
    };
    let mut map = clients().lock().unwrap();
    let Some(handle) = map.get_mut(&client_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while out.len() < cap {
        match handle.events.try_recv() {
            Ok(ev) => out.push(ev.to_json()),
            Err(_) => break,
        }
    }
    out
}

/// Queue a fresh snapshot into an already-attached client's mailbox (the
/// resume path: same frame set as attach, minus `you`). No-op when the id
/// is not attached.
#[flutter_rust_bridge::frb]
pub async fn resync(client_id: String) {
    if let Some(eng) = engine() {
        eng.push_snapshot(&client_id).await;
    }
}

/// Edge stats: queue depth behind the push fan-out for `client_id`
/// (same shape as the engine's `ipc_stats`, minus the counters the
/// collapse layer will own — see the module TODO).
#[flutter_rust_bridge::frb(sync)]
pub fn stats(client_id: String) -> String {
    let depth = clients()
        .lock()
        .unwrap()
        .get(&client_id)
        .map(|h| h.events.len())
        .unwrap_or(0);
    serde_json::json!({
        "type": "ipc_stats",
        "queue_depth": depth,
    })
    .to_string()
}
