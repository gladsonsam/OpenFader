//! openmix-engine: the single transport-agnostic mixer session.
//!
//! Every frontend (browser via the relay, desktop/mobile via Tauri, CLI,
//! future apps) gets the FULL mixer capability through this crate: driver
//! sessions, strip state + param plane (caps, poller, focus, diff,
//! confirmed-write refresh, unsupported dropout), meters/spectrum/GR
//! aggregation, scenes/shows, strip locks + TTL sweeper, per-client access
//! policy, client roster/writers and `by` attribution. A feature that is not
//! in the engine does not exist.
//!
//! The engine speaks exactly one protocol: [`Cmd`] in, [`Event`] out (see
//! [`protocol`], specified in `crates/engine/README.md`). Adapters
//! (`crates/serve` for WebSocket + static files, `src-tauri` for IPC) only
//! move `Cmd`/`Event` JSON; they contain no mixer logic.
//!
//! ```no_run
//! use std::sync::Arc;
//! use openmix_engine::{Engine, EngineConfig, Cmd};
//!
//! # async fn run() {
//! let engine = Engine::new(EngineConfig::default());
//! engine.connect("sim", None).await.unwrap();
//! let mut client = engine.attach(Some("ui".to_string())).await;
//! client.send(Cmd::Fader { ch: 0, key: None, value: 0.5 }).await.unwrap();
//! // `client.events` yields `Event`s: the initial snapshot first
//! // (`you`, `state`, `shows`, `clients`, `locks`, `caps`, `devices`,
//! // `link`, `info`?, `scenes`, `fx`, `routing`, `params`…),
//! // then every live broadcast. Dropping `client` detaches it.
//! # }
//! ```

pub mod drivers;
pub mod engine;
pub mod protocol;

pub use engine::{ClientHandle, Engine, EngineConfig, RecallScope};
pub use protocol::{parse_cmd, Cmd, Event};

/// Default per-strip lock TTL (see [`EngineConfig::lock_ttl`]).
pub use engine::LOCK_TTL;
/// Default param-plane poll tick: one driver query per tick = 20 queries/s.
pub use engine::PARAM_POLL_INTERVAL;
/// Legacy simulator envelope-activity budget (audio spectra are uncapped).
pub use engine::SPECTRUM_CHANNELS;

/// Legacy simulator GR fanout budget (hardware uses its whole topology).
pub use engine::GR_CHANNELS;
