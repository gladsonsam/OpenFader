//! Mackie DL32S driver: thin mapping over [`MackieSession`].
//!
//! This crate owns ONLY the DL32S address table ([`addrs`]) plus argument
//! mapping. All sockets, framing, keepalive, caching and the init handshake
//! live in `openfader-transport::mackie`. New providers add a crate shaped
//! exactly like this one: table + mapping, zero core changes.

pub mod addrs;
mod assignments;
mod commands;
mod destination;
mod driver;
mod fx;
mod input;
mod lifecycle;
mod metadata;
mod native;
mod output_processing;
mod input_processors;
mod preamp;
mod telemetry;
mod output_meters;

pub use metadata::DL32S_CAPABILITIES;
use native::NativeCache;
use openfader_core::ParamUpdate;
use openfader_transport::mackie::MackieSession;
use std::sync::{atomic::AtomicU64, Arc};
use telemetry::{MeterState, SpectrumState};
use tokio::sync::{broadcast, Mutex};

/// Live DL32S-class mixer on TCP 50001.
pub struct Dl32sDriver {
    session: Arc<Mutex<Option<MackieSession>>>,
    /// Serialize input/output processor writes through independent readback.
    /// Output readback releases the session for native ingestion; input
    /// readback consumes transport provenance directly while holding it.
    output_write: Arc<Mutex<()>>,
    /// Session-tagged trailing input DSP refresh; never carries protocol writes.
    input_refresh: std::sync::Mutex<Option<(u64, tokio::sync::watch::Sender<()>)>>,
    addr: Mutex<Option<String>>,
    update_tx: broadcast::Sender<ParamUpdate>,
    generation: Arc<AtomicU64>,
    meters: Arc<std::sync::Mutex<MeterState>>,
    spectrum: Arc<std::sync::Mutex<SpectrumState>>,
    /// Read-only native snapshot view (see [`NativeCache`]): seeded from
    /// the session's push-only verified stores on connect (covering pushes
    /// that arrived before this connection's ingest subscription), then
    /// updated from genuine `SnapshotRecall`/`SnapshotRename` pushes by
    /// the ingest task. Own saves/deletes never touch it. Reset on
    /// disconnect so a dead session reads unavailable, never the last desk.
    native: Arc<std::sync::Mutex<NativeCache>>,
}

impl Default for Dl32sDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Dl32sDriver {
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(256);
        Self {
            session: Arc::new(Mutex::new(None)),
            output_write: Arc::new(Mutex::new(())),
            input_refresh: std::sync::Mutex::new(None),
            addr: Mutex::new(None),
            update_tx,
            generation: Arc::new(AtomicU64::new(0)),
            meters: Arc::new(std::sync::Mutex::new(MeterState::default())),
            spectrum: Arc::new(std::sync::Mutex::new(SpectrumState::default())),
            native: Arc::new(std::sync::Mutex::new(NativeCache::default())),
        }
    }
}

#[cfg(test)]
mod tests;
