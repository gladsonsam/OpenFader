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
mod diagnostics;
mod driver;
mod fx;
mod global_controls;
mod input;
mod input_processors;
mod lifecycle;
mod metadata;
mod native;
mod native_library;
mod output_meters;
mod output_processing;
pub mod output_routing;
mod preamp;
mod push;
mod push_masters;
mod solo;
pub mod soundcheck;
pub mod strips;
mod telemetry;

pub use metadata::DL32S_CAPABILITIES;
use native::NativeCache;
use openfader_core::ParamUpdate;
use openfader_transport::mackie::MackieSession;
use std::sync::{atomic::AtomicU64, Arc};
use telemetry::{MeterState, SpectrumState};
use tokio::sync::{broadcast, Mutex};

/// Live DL32S-class mixer on TCP 50001.
pub struct Dl32sDriver {
    session: Arc<Mutex<Option<Arc<MackieSession>>>>,
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
    /// the ingest task. A bounded read-only stored-show fetch publishes the complete
    /// catalogue after identity/CRC/session validation. Own saves/deletes never
    /// fabricate entries. Reset on
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
        // A read-only value-space refresh can fan out the complete 32-input
        // catalogue in one non-yielding ingest burst. The former 256 slots
        // dropped early-channel confirmations before Engine could consume
        // them, leaving accepted UI edits unverified until a slow repair sweep.
        // Bound the queue to two complete input catalogues (including aliases).
        let (update_tx, _) = broadcast::channel(2 * 32 * DL32S_CAPABILITIES.len());
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

/// Lossless native file inspection; does not authorize a mixer restore.
pub mod native_archive;
mod native_inflate;
pub mod native_show;
