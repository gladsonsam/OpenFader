//! openfader-engine: the single transport-agnostic mixer session.
//!
//! All mixer capability lives here (driver sessions, strip state, param
//! plane, meters/spectrum/GR, scenes/shows, locks, per-client policy,
//! roster). A feature that is not in the engine does not exist.
//!
//! The engine speaks exactly one protocol: [`Cmd`] in, [`Event`] out (see
//! [`protocol`], specified in the private engine protocol notes). Adapters
//! (`crates/serve` for WebSocket, `crates/flutter_bridge` for the native
//! app) only move `Cmd`/`Event` JSON; they contain no mixer logic.
//!
//! ```no_run
//! use std::sync::Arc;
//! use openfader_engine::{Engine, EngineConfig, Cmd};
//!
//! # async fn run() {
//! let engine = Engine::new(EngineConfig::default());
//! engine.connect("sim", None).await.unwrap();
//! let mut client = engine.attach(Some("ui".to_string())).await;
//! client.send(Cmd::Fader { ch: 0, key: None, value: 0.5 }).await.unwrap();
//! // `client.events` yields `SharedEvent`s: the initial snapshot first
//! // (`you`, `state`, `shows`, `clients`, `locks`, `caps`, `devices`,
//! // `link`, `info`?, `scenes`, `fx`, `routing`, `params…`,
//! // `native_snapshots`, plus `local_library_status` in device mode),
//! // then every live broadcast. Dropping `client` detaches it.
//! # }
//! ```

mod connection_lifecycle;
pub mod drivers;
pub mod engine;
pub mod local_library;
pub mod protocol;

pub use drivers::SoundcheckProfile;
pub use engine::{ClientHandle, ClientMailbox, Engine, EngineConfig, RecallScope};
pub use local_library::LibraryStatusView;
pub use protocol::{parse_cmd, Cmd, Event, LibraryAction, SharedEvent};

/// Default per-strip lock TTL (see [`EngineConfig::lock_ttl`]).
pub use engine::LOCK_TTL;
/// Default param-plane poll tick: one driver query per tick = 20 queries/s.
pub use engine::PARAM_POLL_INTERVAL;
/// Legacy simulator envelope-activity budget (audio spectra are uncapped).
pub use engine::SPECTRUM_CHANNELS;

/// Legacy simulator GR fanout budget (hardware uses its whole topology).
pub use engine::GR_CHANNELS;

/// XR18 soundcheck routing assessment supplied by its protocol driver.
pub use openfader_proto_xair::soundcheck_profile as xair_soundcheck_profile;
