//! Existing engine regression tests, grouped by responsibility.

use super::*;

mod catalogue;
mod mailbox;
mod polling;
mod session;
mod topology;

fn event_json(event: Event) -> Value {
    serde_json::to_value(event).unwrap()
}

/// Parse a mailbox frame's shared wire JSON (see [`SharedEvent`]):
/// what the adapters put on the socket, byte-identical to
/// [`event_json`] for the same event, without re-serializing.
fn shared_json(frame: &SharedEvent) -> Value {
    serde_json::from_str(&frame.json).unwrap()
}

/// Engine with the background poller parked for exact-sequence
/// tests: the interval is huge, so only the single immediate first
/// tick ever fires (tokio intervals tick once immediately, then
/// sleep). Pair with [`settle_parked_poller`] after attach.
fn parked_engine() -> Arc<Engine> {
    Engine::new(EngineConfig {
        poll_interval: Duration::from_secs(3600),
        ..EngineConfig::default()
    })
}

/// Wait out the parked poller's single immediate tick, then drain the
/// mailbox: after this returns `poll_once` is the only thing moving
/// the cursor/queue/cache, so exact broadcast-sequence assertions are
/// deterministic even under workspace-parallel load. The immediate tick
/// issues one full batch across the lanes (focus/visible/essentials/
/// background), so the total of the lane cursors, not the background
/// cursor alone, marks the fired tick.
async fn settle_parked_poller(engine: &Engine, client: &mut ClientHandle) {
    let want = engine.effective_inflight(&engine.active_driver());
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.poll_cursor.load(Ordering::Relaxed) + engine.core_cursor.load(Ordering::Relaxed)
        < want as u64
    {
        if Instant::now() > deadline {
            panic!("parked poller never ran its first tick");
        }
        tokio::task::yield_now().await;
    }
    // The cursor advances before the tick's reads/broadcasts land;
    // let them land, then drain so the manual phase starts clean.
    tokio::time::sleep(Duration::from_millis(100)).await;
    while client.events.try_recv().is_ok() {}
}
