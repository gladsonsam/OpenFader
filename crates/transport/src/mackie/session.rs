//! Session ownership, pending requests, push stores, and initialization.

use super::*;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;

mod controls;
mod device;
mod diagnostics;
pub use diagnostics::ReceivedState;
mod files;
mod receive;
mod values;

use receive::{rx_loop, send_frame};

#[derive(Debug)]
struct RequestFailed(String);

impl std::fmt::Display for RequestFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mixer request failed: {}", self.0)
    }
}

impl std::error::Error for RequestFailed {}

type PendingResult = Result<Message, RequestFailed>;

/// Push caches shared between the session struct and its rx task.
/// Value words with own-write provenance under ONE lock (R2 TOCTOU fix):
/// the cached word and its pending mark update/read atomically, so a
/// verified reader can never observe our own just-written value with the
/// pending mark absent, and a write can never interleave between another
/// thread's provenance check and value read. Push ingest clears pending in
/// the same critical section it inserts desk values. No second connection,
/// no invented wire fields, provenance only, values unchanged.
#[derive(Default)]
struct ValueStore {
    observed: HashMap<u32, (u32, u64, u64)>,
    sequence: u64,
    values: HashMap<u32, u32>,
    pending: HashSet<u32>,
}

/// Channel-name slots with own-write provenance under ONE lock (same
/// atomicity contract as [`ValueStore`]).
struct NameStore {
    received: HashMap<u32, String>,
    names: HashMap<u32, String>,
    pending: HashSet<u32>,
}

struct Caches {
    values: StdMutex<ValueStore>,
    type2_values: StdMutex<ValueStore>,
    names: StdMutex<NameStore>,
    extra_values: StdMutex<std::collections::BTreeMap<(u8, u32), u32>>,
    meter_blocks: StdMutex<std::collections::BTreeMap<u32, ValueBlock>>,
    info_frames: StdMutex<std::collections::BTreeMap<(u8, u8, u32), Message>>,
    snapshot_names: StdMutex<HashMap<u32, String>>,
    current_snapshot: StdMutex<Option<u32>>,
    /// Push-only native snapshot record (verified desk evidence): written
    /// EXCLUSIVELY by the receive loop when a genuine desk snapshot push
    /// (recall relay / rename relay) arrives. Own-write paths
    /// (`save_show_snapshot`'s optimistic insert, `delete_snapshot`'s
    /// removal) touch `snapshot_names` only and never these stores, so no
    /// readback path can mistake our own optimism for desk confirmation
    ///, no future pending-cache laundering by construction (there is no
    /// pending mark here at all: push writes are proof, everything else
    /// cannot write). A fresh session starts empty (a new connect means a
    /// new `Caches`), so stale desks never survive a session change.
    snapshot_names_pushed: StdMutex<HashMap<u32, String>>,
    current_snapshot_pushed: StdMutex<Option<u32>>,
    /// Own-write provenance: words/slots whose cached entry came from a
    /// local `write_*`/`set_channel_name` instead of a desk push live in the
    /// same [`ValueStore`]/[`NameStore`] lock as the value itself (see the
    /// store docs). The mixer never echoes our own writes back, so a desk
    /// push covering the address (someone else moved the same leaf, or a
    /// fresh bulk) is the only thing that clears an entry. Until then the
    /// word is accepted-but-unverified and the `*_verified` accessors
    /// withhold it, so no readback path can mistake our own optimism for
    /// desk confirmation.
    push_tx: broadcast::Sender<PushEvent>,
}

/// Desk-originated push, emitted for every inbound value/name/snapshot
/// frame: the mixer's unprompted requests, its broadcasts, AND our own
/// query replies (the desk relays state to every client alike). Drivers
/// decode these into `ParamUpdate`s; the engine dedups them against its
/// cache, so own-write echoes converge instead of looping.
#[derive(Clone, Debug)]
pub enum PushEvent {
    /// One `ChannelValues` block: `value_type` 5 = params, 2 = mute-group
    /// masters, 1 = meters (BE-f32 dB; the DL32S driver decodes the first
    /// 32 words as input meters, other drivers skip it).
    Values {
        value_type: u8,
        start: u32,
        raws: Vec<u32>,
    },
    /// One `ChannelNames` table (`(slot, name)` pairs, 1-based slots).
    Names { pairs: Vec<(u32, String)> },
    /// Snapshot recall relay (desk state changed wholesale, the poller
    /// converges; drivers do not fan this out as strip updates).
    SnapshotRecall(u32),
    /// Snapshot slot rename relay.
    SnapshotRename { slot: u32, name: String },
}

impl Caches {
    fn fresh() -> Self {
        let (push_tx, _) = broadcast::channel(256);
        Self {
            values: StdMutex::new(ValueStore::default()),
            type2_values: StdMutex::new(ValueStore::default()),
            extra_values: StdMutex::default(),
            meter_blocks: StdMutex::default(),
            info_frames: StdMutex::default(),
            names: StdMutex::new(NameStore {
                received: HashMap::new(),
                names: HashMap::new(),
                pending: HashSet::new(),
            }),
            snapshot_names: StdMutex::new(HashMap::new()),
            current_snapshot: StdMutex::new(None),
            snapshot_names_pushed: StdMutex::new(HashMap::new()),
            current_snapshot_pushed: StdMutex::new(None),
            push_tx,
        }
    }
}

/// Cloneable, read-only access to transport-received parameter evidence.
/// Reads never wait for a driver's session lock or its delayed push consumer.
#[derive(Clone)]
pub struct MackieValueCache {
    caches: Arc<Caches>,
}

/// Receipt baseline bound to the transport cache that created it.
pub struct ValueReceiptBarrier {
    caches: Arc<Caches>,
    sequence: u64,
}

impl std::fmt::Debug for MackieValueCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MackieValueCache").finish_non_exhaustive()
    }
}

impl MackieValueCache {
    /// Capture the receive baseline before issuing a read-only request.
    pub fn receipt_barrier(&self) -> ValueReceiptBarrier {
        let sequence = self
            .caches
            .values
            .lock()
            .expect("value cache poisoned")
            .sequence;
        ValueReceiptBarrier {
            caches: self.caches.clone(),
            sequence,
        }
    }

    /// Return all values received since the barrier, or the missing addresses.
    /// A barrier from another session cannot establish freshness.
    pub fn received_since(
        &self,
        barrier: &ValueReceiptBarrier,
        addresses: &[u32],
    ) -> Result<Vec<(u32, u64, u64)>, Vec<u32>> {
        if !Arc::ptr_eq(&self.caches, &barrier.caches) {
            return Err(addresses.to_vec());
        }
        let store = self.caches.values.lock().expect("value cache poisoned");
        let mut missing = Vec::new();
        let mut values = Vec::with_capacity(addresses.len());
        for address in addresses {
            match store.observed.get(address) {
                Some(value) if value.2 > barrier.sequence && !store.pending.contains(address) => {
                    values.push(*value)
                }
                _ => missing.push(*address),
            }
        }
        if missing.is_empty() {
            Ok(values)
        } else {
            Err(missing)
        }
    }
    pub fn verified_snapshot(&self, addresses: &[u32]) -> Option<Vec<(u32, u64, u64)>> {
        let store = self.caches.values.lock().ok()?;
        addresses
            .iter()
            .map(|a| {
                (!store.pending.contains(a))
                    .then(|| store.observed.get(a).copied())
                    .flatten()
            })
            .collect()
    }
    /// Value, observed-change revision, receipt sequence; own ACKs are withheld.
    /// Revisions describe received transitions, not invisible/same-value peers.
    pub fn verified(&self, address: u32) -> Option<(u32, u64, u64)> {
        let store = self.caches.values.lock().ok()?;
        if store.pending.contains(&address) {
            return None;
        }
        store.observed.get(&address).copied()
    }
}

/// Live Mackie TCP session. Cloneable via Arc internals through `&self`
/// methods, mirroring [`crate::session::OscSession`].
pub struct MackieSession {
    // File operations share a device-side handle; never interleave transfers.
    file_lane: Mutex<()>,
    write: Arc<Mutex<OwnedWriteHalf>>,
    seq: StdMutex<u8>,
    pending: Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    caches: Arc<Caches>,
    rx_task: JoinHandle<()>,
    keepalive_task: JoinHandle<()>,
}

impl MackieSession {
    /// Connect to `host[:port]` (default port 50001) and run the init
    /// handshake. Resolves via the shared [`crate::session::resolve_peer`].
    /// The dial itself is bounded by [`DIAL_TIMEOUT`]: without it a
    /// blackhole relay leg hangs the connect on OS SYN retries.
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let peer: SocketAddr = crate::session::resolve_peer(addr, DEFAULT_PORT)?;
        let stream = tokio::time::timeout(DIAL_TIMEOUT, TcpStream::connect(peer))
            .await
            .map_err(|_| anyhow::anyhow!("connect to {peer} timed out after {DIAL_TIMEOUT:?}"))??;
        stream.set_nodelay(true)?;
        Self::from_connected_stream(stream).await
    }

    /// Wrap an already-connected stream (used by tests with loopback pairs).
    pub async fn from_connected_stream(stream: TcpStream) -> anyhow::Result<Self> {
        let (read, write) = stream.into_split();
        let pending: Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let caches = Arc::new(Caches::fresh());
        let write = Arc::new(Mutex::new(write));

        let rx_task = {
            let write = write.clone();
            let pending = pending.clone();
            let caches = caches.clone();
            tokio::spawn(async move { rx_loop(read, write, pending, caches).await })
        };

        let keepalive_task = {
            let write = write.clone();
            tokio::spawn(async move {
                let mut ticker = crate::session::keepalive_ticker(KEEPALIVE_INTERVAL);
                loop {
                    ticker.tick().await;
                    if send_frame(&write, 0, MessageType::Request, Command::KeepAlive, vec![])
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            })
        };

        let this = Self {
            file_lane: Mutex::new(()),
            write,
            seq: StdMutex::new(0),
            pending,
            caches,
            rx_task,
            keepalive_task,
        };
        this.initialize().await?;
        Ok(this)
    }

    /// Best-effort shutdown: stop the background tasks. Also runs on drop.
    pub fn close(&self) {
        self.rx_task.abort();
        self.keepalive_task.abort();
        self.fail_all_pending("session closed");
    }

    /// Task-liveness probe: true while BOTH background tasks are still
    /// running. Reports only that the rx and keepalive tasks have not
    /// finished, no ACK, no RTT, no wire probe. Both tasks finish solely on
    /// fatal/close/drop paths (rx EOF/read error, keepalive send failure, or
    /// explicit close/drop abort), never on a transient healthy-init
    /// completion, so a finished handle means the transport is closed.
    pub fn is_alive(&self) -> bool {
        !self.rx_task.is_finished() && !self.keepalive_task.is_finished()
    }

    /// Tap the desk-originated push feed: every inbound value/name/snapshot
    /// frame is forwarded here as well as into the caches. Drivers decode
    /// these into `ParamUpdate`s for live ingest (another client's moves
    /// arrive without a poll sweep). Lagged receivers skip, never block.
    pub fn subscribe_pushes(&self) -> broadcast::Receiver<PushEvent> {
        self.caches.push_tx.subscribe()
    }

    fn fail_all_pending(&self, reason: &str) {
        let senders: Vec<oneshot::Sender<PendingResult>> = {
            self.pending
                .lock()
                .map(|mut p| p.drain().map(|(_, tx)| tx).collect())
                .unwrap_or_default()
        };
        for tx in senders {
            let _ = tx.send(Err(RequestFailed(reason.to_string())));
        }
    }

    fn next_seq(&self) -> u8 {
        let mut seq = self.seq.lock().expect("seq poisoned");
        *seq = seq.wrapping_add(1);
        if *seq == 0 {
            *seq = 1;
        }
        *seq
    }

    /// Send a request and await the correlated response.
    pub async fn send_request(
        &self,
        command: Command,
        body: Vec<u8>,
        timeout: Duration,
    ) -> anyhow::Result<Message> {
        self.send_request_guarded(command, body, timeout, &|| true)
            .await
    }

    /// Recheck caller authority after acquiring the socket write lane, before
    /// allocating a request or sending bytes. The synchronous callback must
    /// not acquire this session's write lock or call session I/O. A rejection
    /// sends nothing, creates no pending request and is never replayed.
    ///
    /// This is a dispatch boundary, not a transaction: revocation after bytes
    /// start leaving cannot retract them. Cancellation while waiting for the
    /// lane sends nothing; cancellation after dispatch remains uncertain.
    pub async fn send_request_guarded(
        &self,
        command: Command,
        body: Vec<u8>,
        timeout: Duration,
        authorize: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<Message> {
        if !body.len().is_multiple_of(4) {
            anyhow::bail!("request body length must be a multiple of 4");
        }
        let (seq, rx) = {
            let (tx, rx) = oneshot::channel();
            let mut guard = self.write.lock().await;
            anyhow::ensure!(authorize(), "Request authority changed before dispatch");
            let seq = self.next_seq();
            self.pending
                .lock()
                .map(|mut p| p.insert(seq, tx))
                .expect("pending poisoned");
            let msg = Message::new(seq, MessageType::Request, command, body)?;
            let bytes = msg.encode();
            guard.write_all(&bytes).await?;
            guard.flush().await?;
            (seq, rx)
        };
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(msg))) => Ok(msg),
            Ok(Ok(Err(e))) => {
                self.pending
                    .lock()
                    .map(|mut p| p.remove(&seq))
                    .unwrap_or(None);
                Err(e.into())
            }
            Ok(Err(_)) => anyhow::bail!("request channel closed"),
            Err(_) => {
                self.pending
                    .lock()
                    .map(|mut p| p.remove(&seq))
                    .unwrap_or(None);
                anyhow::bail!("{command:?} request timed out after {timeout:?}")
            }
        }
    }

    /// Send a frame without awaiting a response (snapshot-recall pair leg).
    async fn send_frame_no_reply(
        &self,
        msg_type: MessageType,
        command: Command,
        body: Vec<u8>,
    ) -> anyhow::Result<()> {
        if !body.len().is_multiple_of(4) {
            anyhow::bail!("frame body length must be a multiple of 4");
        }
        let mut guard = self.write.lock().await;
        let seq = self.next_seq();
        let msg = Message::new(seq, msg_type, command, body)?;
        guard.write_all(&msg.encode()).await?;
        guard.flush().await?;
        Ok(())
    }

    /// Minimal init sequence from observation: keepalive, channel-info
    /// control (large-message permit), handshake, general-info query, meter
    /// layout + broadcast control, then the channel-data request that makes
    /// the mixer push its whole value space. Each step is best-effort.
    async fn initialize(&self) -> anyhow::Result<()> {
        let _ = self
            .send_frame_no_reply(MessageType::Request, Command::KeepAlive, vec![])
            .await;
        let _ = self
            .send_request(Command::ChannelInfoControl, vec![0; 8], REQUEST_TIMEOUT)
            .await;
        let _ = self
            .send_request(Command::ClientHandshake, vec![], REQUEST_TIMEOUT)
            .await;
        let _ = self
            .send_request(
                Command::GeneralInfo,
                2u32.to_be_bytes().to_vec(),
                REQUEST_TIMEOUT,
            )
            .await;
        let _ = self
            .send_request(
                Command::MeterLayout,
                build_meter_layout(&dl32_family_meter_addresses()),
                REQUEST_TIMEOUT,
            )
            .await;
        let _ = self
            .send_request(
                Command::BroadcastControl,
                BROADCAST_CONTROL_BODY.to_vec(),
                REQUEST_TIMEOUT,
            )
            .await;
        let _ = self
            .send_request(
                Command::ChannelInfoControl,
                6u32.to_be_bytes().to_vec(),
                REQUEST_TIMEOUT,
            )
            .await;
        Ok(())
    }
}

impl Drop for MackieSession {
    fn drop(&mut self) {
        self.rx_task.abort();
        self.keepalive_task.abort();
        self.fail_all_pending("session dropped");
    }
}
