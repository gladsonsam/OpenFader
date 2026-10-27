//! `openfader-flutter-bridge`: FRB (flutter_rust_bridge v2) binding over the
//! embedded `openfader-engine` session for the native app.
//!
//! The `frb` macro expands to `cfg(frb_expand)`-gated items the stable
//! toolchain does not know; allowed crate-wide (the macro owns them).
#![allow(unexpected_cfgs)]
//!
//! The full `openfader-engine` mixer session runs in this process (in-process
//! engine, no IPC hop): the Dart side sends one Cmd JSON
//! per [`cmd`] and drains Event JSON frames via [`poll`]. No mixer logic
//! lives here, this crate only moves Cmd/Event JSON, exactly like the WS
//! relay.
//!
//! FRB-compatible signatures only (`String`, `Vec<String>`, `Vec<u8>`,
//! `u32`, `()`); no engine types are exposed. [`boot`] loads device-local
//! storage asynchronously on the bridge-owned runtime; [`cmd`]/[`poll`]/
//! [`poll_binary`]/[`resync`] are async so Dart gets Futures without
//! blocking the UI thread.
//!
//! Binary meter fan-out ([`api::poll_binary`]): meter-class events ride
//! the compact `Event::to_binary` encoding (protocol.rs `BIN_VERSION`
//! layout) as length-prefixed frames (`u32 LE len` + frame bytes,
//! concatenated); every other event is set aside as JSON text in the
//! client's pending queue, which [`poll`](api::poll) drains first, so
//! the JSON control path is a lossless fallback for non-meter events.
//!

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use openfader_engine::{ClientHandle, Engine};

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
            .thread_name("openfader-flutter")
            .build()
            .expect("openfader_flutter_bridge: tokio runtime")
    })
}

static ENGINE: OnceLock<Arc<Engine>> = OnceLock::new();

pub(crate) fn engine() -> Option<Arc<Engine>> {
    ENGINE.get().cloned()
}

pub(crate) fn set_engine(engine: Arc<Engine>) {
    let _ = ENGINE.set(engine);
}

pub(crate) fn clients() -> &'static Mutex<HashMap<String, BridgeClient>> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, BridgeClient>>> = OnceLock::new();
    CLIENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One attached Dart client: its live event mailbox plus the binary
/// meter-poll cursor.
///
/// `bin_seq` is the per-client monotonic frame counter stamped into the
/// `seq` field of every `Event::to_binary` frame vended by
/// [`poll_binary`](api::poll_binary) (adapter-owned, per the protocol.rs
/// contract); `pending_json` holds non-meter JSON frames set aside by
/// `poll_binary`, drained first by [`poll`](api::poll) so no control
/// frame is ever lost on the binary path.
pub(crate) struct BridgeClient {
    pub(crate) handle: ClientHandle,
    pub(crate) bin_seq: u32,
    pub(crate) pending_json: VecDeque<String>,
    // Output meters are JSON-only realtime telemetry, never reliable backlog.
    pub(crate) pending_output_json: Option<String>,
}

impl BridgeClient {
    pub(crate) fn new(handle: ClientHandle) -> Self {
        Self {
            handle,
            bin_seq: 0,
            pending_json: VecDeque::new(),
            pending_output_json: None,
        }
    }
}
