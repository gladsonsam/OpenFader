//! `openmix-flutter-bridge`: FRB (flutter_rust_bridge v2) binding over the
//! embedded `openmix-engine` session for the native app.
//!
//! The `frb` macro expands to `cfg(frb_expand)`-gated items the stable
//! toolchain does not know; allowed crate-wide (the macro owns them).
#![allow(unexpected_cfgs)]
//!
//! The full `openmix-engine` mixer session runs in this process (in-process
//! engine, no IPC hop): the Dart side sends one Cmd JSON
//! per [`cmd`] and drains Event JSON frames via [`poll`]. No mixer logic
//! lives here — this crate only moves Cmd/Event JSON, exactly like the WS
//! relay.
//!
//! FRB-compatible signatures only (`String`, `Vec<String>`, `u32`, `()`);
//! no engine types are exposed. [`boot`] is sync (it block_on's the
//! runtime it owns); [`cmd`]/[`poll`]/[`resync`] are async so Dart gets
//! Futures without blocking the UI thread.
//!
//! TODO (before real use): meter collapse latest-only. [`poll`] currently
//! drains everything up to `cap`; a stalled Dart poll loop gets a stale
//! burst instead of the freshest frame per stream. Port the slot-of-1
//! per-meter-stream outbox pattern (drops counted) before raising the
//! poll rate past a smoke test.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use openmix_engine::{ClientHandle, Engine};

mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */

pub mod api;

/// Fallback drain bound when Dart passes `cap == 0` (mirrors the bridge's
/// `ENGINE_DRAIN_CAP`: one batch, overflow stays queued).
pub const DRAIN_CAP_DEFAULT: usize = 512;
/// Hard ceiling so a bad `cap` cannot blow one JNI call's payload.
pub const DRAIN_CAP_MAX: usize = 4096;

pub(crate) fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("openmix-flutter")
            .build()
            .expect("openmix_flutter_bridge: tokio runtime")
    })
}

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();

pub(crate) fn engine() -> Option<Arc<Engine>> {
    ENGINE.get().cloned()
}

pub(crate) fn set_engine(engine: Arc<Engine>) {
    let _ = ENGINE.set(engine);
}

pub(crate) fn clients() -> &'static Mutex<HashMap<String, ClientHandle>> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, ClientHandle>>> = OnceLock::new();
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

// FRB entry points live in [`api`] (`crate::api`): `boot`, `cmd`, `poll`,
// `resync`, `stats`.
