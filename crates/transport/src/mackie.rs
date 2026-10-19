//! Mackie DL-series TCP wire protocol (DL16S/DL32S/DL32R family).
//!
//! Black-box port of the proven Python client (`mackie_dl/client.py`) and its
//! spec (`SPEC.md`): one client-initiated TCP connection, default port 50001,
//! all values big-endian. This module owns everything on the wire — framing,
//! checksums, value-block encode/decode, the init handshake, the receive loop
//! with request/response correlation, and the push-driven caches.
//!
//! It deliberately owns NO per-model address tables: strip layout (base,
//! stride, field offsets) lives in the driver crate (`proto-dl32s`). Session
//! methods take explicit addresses; the driver maps fields to addresses.
//! Mute-group masters and the meter/output-strip lists are fixed protocol
//! addresses shared by the whole DL family, so they live here.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// Frame codec
// ---------------------------------------------------------------------------

/// First header byte of every frame.
pub const HEADER_MARKER: u8 = 0xAB;
/// Default mixer TCP port.
pub const DEFAULT_PORT: u16 = 50001;
/// Bytes of the fixed frame header (marker/seq/chunks/type/command).
pub const HEADER_LEN: usize = 6;
/// Header + header checksum.
pub const FRAME_PREFIX_LEN: usize = 8;
/// Keepalive period: the mixer expects a ping roughly every 2.5 s.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(2500);
/// Request timeout for correlated request/response exchanges.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Frame direction/kind on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Request = 0,
    Response = 1,
    Error = 5,
    Broadcast = 8,
}

impl TryFrom<u8> for MessageType {
    type Error = FrameError;

    fn try_from(v: u8) -> Result<Self, FrameError> {
        match v {
            0 => Ok(Self::Request),
            1 => Ok(Self::Response),
            5 => Ok(Self::Error),
            8 => Ok(Self::Broadcast),
            _ => Err(FrameError::UnknownType(v)),
        }
    }
}

/// Command byte. Snapshot/file commands come from wire observation; the OSC/
/// meter commands match the family-wide values both sources agree on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    KeepAlive = 0x01,
    Reboot = 0x05,
    ClientHandshake = 0x03,
    FirmwareInfo = 0x04,
    ChannelInfoControl = 0x06,
    ShowSnapshot = 0x07,
    FileInfo = 0x09,
    FileOpen = 0x0A,
    FileClose = 0x0B,
    FileRead = 0x0C,
    PostFile = 0x0D,
    GeneralInfo = 0x0E,
    SetSystemState = 0x0F,
    ShowStatus = 0x12,
    ChannelValues = 0x13,
    BroadcastControl = 0x15,
    MeterLayout = 0x16,
    ChannelNames = 0x18,
}

impl TryFrom<u8> for Command {
    type Error = FrameError;

    fn try_from(v: u8) -> Result<Self, FrameError> {
        match v {
            0x01 => Ok(Self::KeepAlive),
            0x05 => Ok(Self::Reboot),
            0x03 => Ok(Self::ClientHandshake),
            0x04 => Ok(Self::FirmwareInfo),
            0x06 => Ok(Self::ChannelInfoControl),
            0x07 => Ok(Self::ShowSnapshot),
            0x09 => Ok(Self::FileInfo),
            0x0A => Ok(Self::FileOpen),
            0x0B => Ok(Self::FileClose),
            0x0C => Ok(Self::FileRead),
            0x0D => Ok(Self::PostFile),
            0x0E => Ok(Self::GeneralInfo),
            0x0F => Ok(Self::SetSystemState),
            0x12 => Ok(Self::ShowStatus),
            0x13 => Ok(Self::ChannelValues),
            0x15 => Ok(Self::BroadcastControl),
            0x16 => Ok(Self::MeterLayout),
            0x18 => Ok(Self::ChannelNames),
            _ => Err(FrameError::UnknownCommand(v)),
        }
    }
}

/// Frame-level decode/encode failures.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    BadMarker(u8),
    UnknownType(u8),
    UnknownCommand(u8),
    BadBodyLen,
    BadHeaderChecksum { want: u16, got: u16 },
    Truncated,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadMarker(b) => write!(f, "bad header marker {b:#04x}"),
            Self::UnknownType(t) => write!(f, "unknown message type {t:#04x}"),
            Self::UnknownCommand(c) => write!(f, "unknown command {c:#04x}"),
            Self::BadBodyLen => write!(f, "body length not a multiple of 4"),
            Self::BadHeaderChecksum { want, got } => {
                write!(f, "header checksum mismatch want={want:#06x} got={got:#06x}")
            }
            Self::Truncated => write!(f, "truncated frame"),
        }
    }
}

impl std::error::Error for FrameError {}

/// One decoded frame. `body` is in network order, length a multiple of 4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub seq: u8,
    pub msg_type: MessageType,
    pub command: Command,
    pub body: Vec<u8>,
}

impl Message {
    pub fn new(
        seq: u8,
        msg_type: MessageType,
        command: Command,
        body: Vec<u8>,
    ) -> Result<Self, FrameError> {
        if !body.len().is_multiple_of(4) {
            return Err(FrameError::BadBodyLen);
        }
        Ok(Self {
            seq,
            msg_type,
            command,
            body,
        })
    }

    pub fn chunk_count(&self) -> usize {
        self.body.len() / 4
    }

    /// `0xFFFF` minus the sum of the 6 header bytes.
    pub fn header_checksum(header: &[u8; HEADER_LEN]) -> u16 {
        let sum: u32 = header.iter().map(|b| *b as u32).sum();
        (0xFFFF - (sum & 0xFFFF)) as u16
    }

    /// `0xFFFFFFFF` minus the sum of the body bytes.
    pub fn body_checksum(body: &[u8]) -> u32 {
        let sum: u64 = body.iter().map(|b| *b as u64).sum();
        (0xFFFFFFFF - (sum & 0xFFFFFFFF)) as u32
    }

    pub fn encode(&self) -> Vec<u8> {
        debug_assert_eq!(self.body.len() % 4, 0);
        let mut header = [0u8; HEADER_LEN];
        header[0] = HEADER_MARKER;
        header[1] = self.seq;
        header[2..4].copy_from_slice(&(self.chunk_count() as u16).to_be_bytes());
        header[4] = self.msg_type as u8;
        header[5] = self.command as u8;
        let checksum = Self::header_checksum(&header);

        let mut out = Vec::with_capacity(FRAME_PREFIX_LEN + self.body.len() + 4);
        out.extend_from_slice(&header);
        out.extend_from_slice(&checksum.to_be_bytes());
        if self.body.is_empty() {
            return out;
        }
        out.extend_from_slice(&self.body);
        out.extend_from_slice(&Self::body_checksum(&self.body).to_be_bytes());
        out
    }
}

/// Decode one frame from the front of `buf`.
///
/// Returns the message plus the total bytes consumed (header + body +
/// checksum), or `Ok(None)` when `buf` does not hold a full frame yet.
/// The header checksum is validated; the body checksum is computed by
/// [`Message::body_checksum`] but not enforced, matching the reference
/// implementation (which ignores it like DigiMixer does).
pub fn decode_one(buf: &[u8]) -> Result<Option<(Message, usize)>, FrameError> {
    if buf.len() < FRAME_PREFIX_LEN {
        return Ok(None);
    }
    if buf[0] != HEADER_MARKER {
        return Err(FrameError::BadMarker(buf[0]));
    }
    let seq = buf[1];
    let chunks = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    let msg_type = MessageType::try_from(buf[4])?;
    let command = Command::try_from(buf[5])?;
    let want = u16::from_be_bytes([buf[6], buf[7]]);
    let header: &[u8; HEADER_LEN] = buf[0..HEADER_LEN].try_into().expect("prefix checked");
    let got = Message::header_checksum(header);
    if want != got {
        return Err(FrameError::BadHeaderChecksum { want, got });
    }
    if chunks == 0 {
        let msg = Message {
            seq,
            msg_type,
            command,
            body: Vec::new(),
        };
        return Ok(Some((msg, FRAME_PREFIX_LEN)));
    }
    let total = FRAME_PREFIX_LEN + chunks * 4 + 4;
    if buf.len() < total {
        return Ok(None);
    }
    let body = buf[FRAME_PREFIX_LEN..FRAME_PREFIX_LEN + chunks * 4].to_vec();
    let msg = Message {
        seq,
        msg_type,
        command,
        body,
    };
    Ok(Some((msg, total)))
}

/// Reverse the bytes inside each 4-byte word.
///
/// The mixer stores text as little-endian characters inside an otherwise
/// big-endian protocol, so `"MGZU"` travels as `55 5A 47 4D`. Applies to
/// channel names, snapshot names and file payloads. Trailing bytes past the
/// last whole word are dropped: the protocol always pads to a word.
pub fn word_swap(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() - data.len() % 4);
    for word in data.as_chunks::<4>().0 {
        out.extend_from_slice(&[word[3], word[2], word[1], word[0]]);
    }
    out
}

// ---------------------------------------------------------------------------
// Value space
// ---------------------------------------------------------------------------

/// `type` byte selecting the CHANNEL_VALUES address space: normal parameters.
pub const VALUE_TYPE_PARAM: u8 = 5;
/// Mute-group master address space (kept in a separate cache: address 11 is
/// input 1's `main_assign` under type 5 and mute group 1's master under type 2).
pub const VALUE_TYPE_MUTE_GROUP: u8 = 2;
/// Meter address space (pushed periodically; value encoding unconfirmed, ignored).
pub const VALUE_TYPE_METERS: u8 = 1;
/// Show-parameter address space (set_show_parameter uses type 3).
pub const VALUE_TYPE_SHOW: u8 = 3;
/// File-CRC address space (set_file_crc uses type 4).
pub const VALUE_TYPE_FILE: u8 = 4;

/// Number of mute groups on the DL family.
pub const MUTE_GROUP_COUNT: u32 = 6;
/// Mute group N master mute, verified for all six on a DL32S.
pub const MUTE_GROUP_BASE_ADDRESS: u32 = 10;
/// Mute group N name slot in the flat CHANNEL_NAMES table.
pub const MUTE_GROUP_NAME_SLOT_BASE: u32 = 63;

/// Mute group N (1-based) master-mute address.
pub fn mute_group_master_address(group_1_based: u32) -> anyhow::Result<u32> {
    if !(1..=MUTE_GROUP_COUNT).contains(&group_1_based) {
        anyhow::bail!("mute group must be 1..={MUTE_GROUP_COUNT}");
    }
    Ok(MUTE_GROUP_BASE_ADDRESS + group_1_based)
}

/// Mute group N (1-based) name slot.
pub fn mute_group_name_slot(group_1_based: u32) -> anyhow::Result<u32> {
    if !(1..=MUTE_GROUP_COUNT).contains(&group_1_based) {
        anyhow::bail!("mute group must be 1..={MUTE_GROUP_COUNT}");
    }
    Ok(MUTE_GROUP_NAME_SLOT_BASE + group_1_based)
}

/// A decoded CHANNEL_VALUES / CHANNEL_NAMES payload: chunk 0 is the start
/// address, chunk 1 packs `count:16 | type:8 | unknown:8`, then one word per
/// value. The `count == 0 means "as many as fit"` rule is applied here, so
/// bulk pushes (meta `0x00000500`) decode to their full implied length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueBlock {
    pub start: u32,
    pub value_type: u8,
    pub raws: Vec<u32>,
}

pub fn parse_value_block(body: &[u8]) -> Option<ValueBlock> {
    if body.len() < 8 || !body.len().is_multiple_of(4) {
        return None;
    }
    let start = u32::from_be_bytes(body[0..4].try_into().ok()?);
    let meta = u32::from_be_bytes(body[4..8].try_into().ok()?);
    let count = ((meta >> 16) & 0xFFFF) as usize;
    let value_type = ((meta >> 8) & 0xFF) as u8;
    let available = body.len() / 4 - 2;
    let n = if count == 0 { available } else { count.min(available) };
    let mut raws = Vec::with_capacity(n);
    for i in 0..n {
        let off = 8 + i * 4;
        raws.push(u32::from_be_bytes(body[off..off + 4].try_into().ok()?));
    }
    Some(ValueBlock {
        start,
        value_type,
        raws,
    })
}

/// Raw wire value for a single-address write: integers/bools travel as u32,
/// faders and other continuous parameters as big-endian IEEE-754 floats (dB).
#[derive(Clone, Copy, Debug)]
pub enum RawValue {
    Int(u32),
    Float(f32),
}

impl RawValue {
    fn to_be_bytes(self) -> [u8; 4] {
        match self {
            Self::Int(v) => v.to_be_bytes(),
            Self::Float(v) => v.to_be_bytes(),
        }
    }
}

/// Decoded parameter word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DecodedWord {
    F(f32),
    I(u32),
}

/// Split a raw word into its float or integer reading.
pub fn decode_word(raw: u32, is_float: bool) -> DecodedWord {
    if is_float {
        DecodedWord::F(f32::from_bits(raw))
    } else {
        DecodedWord::I(raw)
    }
}

/// Build a single-address CHANNEL_VALUES write body:
/// `[address, count(1)|type|0, value]`.
pub fn build_single_set(address: u32, value: RawValue, value_type: u8) -> Vec<u8> {
    let meta = (1u32 << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&address.to_be_bytes());
    out.extend_from_slice(&meta.to_be_bytes());
    out.extend_from_slice(&value.to_be_bytes());
    out
}

/// Build a multi-address CHANNEL_VALUES write the way the mixer app does:
/// `(address, value)` pairs with the meta word carrying a `0x2000` flag in
/// the count field, placed after the first address.
pub fn build_multi_set(pairs: &[(u32, u32)], value_type: u8) -> anyhow::Result<Vec<u8>> {
    if pairs.is_empty() {
        anyhow::bail!("multi-set needs at least one (address, value) pair");
    }
    let count = pairs.len() as u32;
    let meta = (((count & 0xFFFF) | 0x2000) << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(pairs.len() * 8 + 4);
    let mut first = true;
    for (addr, raw) in pairs {
        out.extend_from_slice(&addr.to_be_bytes());
        if first {
            out.extend_from_slice(&meta.to_be_bytes());
            first = false;
        }
        out.extend_from_slice(&raw.to_be_bytes());
    }
    Ok(out)
}

/// Build a read request body: the two header chunks only (start + meta).
/// The mixer answers read requests with an empty body — polling does not
/// work, only listening to pushes does — so this is used for probing only.
pub fn build_read_request(start: u32, count: u32, value_type: u8) -> Vec<u8> {
    let meta = ((count & 0xFFFF) << 16) | ((value_type as u32) << 8);
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&start.to_be_bytes());
    out.extend_from_slice(&meta.to_be_bytes());
    out
}

// ---------------------------------------------------------------------------
// Meter layout / broadcast control (init sequence data)
// ---------------------------------------------------------------------------

/// Meter address list for the 32-input DL family (inputs, returns, FX in,
/// aux masters, FX out, mains). The DL32S needs this full table even though
/// the DL16S profile lists only 16 inputs.
pub fn dl32_family_meter_addresses() -> Vec<i32> {
    let mut addrs = Vec::with_capacity(62);
    for z in 0..32 {
        addrs.push(34 + z * 7);
    }
    for z in 0..4 {
        addrs.push(258 + z * 4 + 1);
    }
    for z in 0..4 {
        addrs.push(274 + z);
    }
    for z in 0..14 {
        addrs.push(318 + z * 4);
    }
    for z in 0..4 {
        addrs.push(278 + z * 2);
    }
    addrs.extend_from_slice(&[310, 311]);
    addrs
}

/// METER_LAYOUT body: a flag word with byte 3 set, then each address as BE i32.
pub fn build_meter_layout(addresses: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + addresses.len() * 4);
    out.extend_from_slice(&[0, 0, 0, 1]);
    for a in addresses {
        out.extend_from_slice(&a.to_be_bytes());
    }
    out
}

/// BROADCAST_CONTROL body enabling periodic reports (family-wide constant).
pub const BROADCAST_CONTROL_BODY: [u8; 12] =
    [0x00, 0x00, 0x00, 0x01, 0x10, 0x00, 0x01, 0x00, 0x00, 0x5A, 0x00, 0x01];

// ---------------------------------------------------------------------------
// Replies the client owes the mixer (mixer -> client REQUESTs)
// ---------------------------------------------------------------------------

/// Answer to CLIENT_HANDSHAKE (the real MAC would be ideal; zeroes work).
pub const HANDSHAKE_REPLY: [u8; 8] = [0x10, 0x40, 0, 0, 0, 0, 0, 0];
/// Answer to GENERAL_INFO.
pub const GENERAL_INFO_REPLY: [u8; 8] = [0, 0, 0, 2, 0, 0, 0x40, 0];

// ---------------------------------------------------------------------------
// Names + snapshots
// ---------------------------------------------------------------------------

/// Parse a CHANNEL_NAMES payload into `(slot, name)` pairs.
///
/// The payload mirrors CHANNEL_VALUES framing (start slot + meta + a
/// word-swapped stream of NUL-terminated names). An unnamed slot is a bare
/// NUL and still consumes an index, so blanks are kept as empty strings to
/// keep later slots numbered correctly.
///
/// The meta count bounds how many slots the push covers when nonzero: a
/// single-slot rename then touches exactly its slot instead of also blanking
/// the neighbour with padding NULs. A zero count (bulk table dump) takes
/// every chunk, padding included — those land past the real 75-slot table
/// and never surface through name lookups.
pub fn parse_name_table(body: &[u8]) -> Option<Vec<(u32, String)>> {
    if body.len() < 8 || !body.len().is_multiple_of(4) {
        return None;
    }
    let start = u32::from_be_bytes(body[0..4].try_into().ok()?);
    let meta = u32::from_be_bytes(body[4..8].try_into().ok()?);
    let count = ((meta >> 16) & 0xFFFF) as usize;
    let swapped = word_swap(&body[8..]);
    let chunks: Vec<&[u8]> = swapped.split(|b| *b == 0).collect();
    let n = if count == 0 { chunks.len() } else { count.min(chunks.len()) };
    let mut out = Vec::with_capacity(n);
    for (i, chunk) in chunks.iter().take(n).enumerate() {
        let name = String::from_utf8_lossy(chunk).trim().to_string();
        out.push((start + i as u32, name));
    }
    Some(out)
}

/// A snapshot event relayed by the mixer (`SHOW_SNAPSHOT` op 1/5 pushes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotEvent {
    Recall(u32),
    Rename(u32, String),
}

/// Body `<op> <number> ...`: op 1 is a recall relay, op 5 carries
/// `<unix epoch> <word-swapped name>`.
pub fn parse_snapshot_push(body: &[u8]) -> Option<SnapshotEvent> {
    if body.len() < 8 {
        return None;
    }
    let op = u32::from_be_bytes(body[0..4].try_into().ok()?);
    let number = u32::from_be_bytes(body[4..8].try_into().ok()?);
    if number == 0 {
        return None;
    }
    match op {
        1 => Some(SnapshotEvent::Recall(number)),
        5 => {
            if body.len() < 12 {
                return None;
            }
            let raw = word_swap(&body[12..]);
            let name = String::from_utf8_lossy(raw.split(|b| *b == 0).next().unwrap_or(&[]))
                .trim()
                .to_string();
            if name.is_empty() {
                return None;
            }
            Some(SnapshotEvent::Rename(number, name))
        }
        _ => None,
    }
}

/// Recall body pair, the way the mixer app does it: REQUEST `[1, snapshot]`
/// followed by RESPONSE `[1]`.
pub fn build_snapshot_recall_request(snapshot: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&snapshot.to_be_bytes());
    out
}

pub fn build_snapshot_save(snapshot: u32, epoch_secs: u64, name: &str) -> Vec<u8> {
    let text = name.as_bytes();
    let clipped = &text[..text.len().min(63)];
    let mut padded = clipped.to_vec();
    // NUL-terminate and pad to a whole word.
    padded.push(0);
    while !padded.len().is_multiple_of(4) {
        padded.push(0);
    }
    let mut out = Vec::with_capacity(12 + padded.len());
    out.extend_from_slice(&5u32.to_be_bytes());
    out.extend_from_slice(&snapshot.to_be_bytes());
    out.extend_from_slice(&(epoch_secs as u32).to_be_bytes());
    out.extend_from_slice(&word_swap(&padded));
    out
}

// ---------------------------------------------------------------------------
// Session: TCP connection + init + receive loop + push caches
// ---------------------------------------------------------------------------

/// Correlated-request failure delivered to the waiter.
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
struct Caches {
    values: StdMutex<HashMap<u32, u32>>,
    type2_values: StdMutex<HashMap<u32, u32>>,
    names: StdMutex<HashMap<u32, String>>,
    snapshot_names: StdMutex<HashMap<u32, String>>,
    current_snapshot: StdMutex<Option<u32>>,
}

impl Caches {
    fn fresh() -> Self {
        Self {
            values: StdMutex::new(HashMap::new()),
            type2_values: StdMutex::new(HashMap::new()),
            names: StdMutex::new(HashMap::new()),
            snapshot_names: StdMutex::new(HashMap::new()),
            current_snapshot: StdMutex::new(None),
        }
    }
}

/// Live Mackie TCP session. Cloneable via Arc internals through `&self`
/// methods, mirroring [`crate::session::OscSession`].
pub struct MackieSession {
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
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let peer: SocketAddr = crate::session::resolve_peer(addr, DEFAULT_PORT)?;
        let stream = TcpStream::connect(peer).await?;
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
        if !body.len().is_multiple_of(4) {
            anyhow::bail!("request body length must be a multiple of 4");
        }
        let (seq, rx) = {
            let (tx, rx) = oneshot::channel();
            let mut guard = self.write.lock().await;
            let seq = self.next_seq();
            self.pending
                .lock()
                .map(|mut p| p.insert(seq, tx))
                .expect("pending poisoned");
            let msg = Message::new(seq, MessageType::Request, command, body)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
                Err(anyhow::anyhow!("{e}"))
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
        let msg = Message::new(seq, msg_type, command, body).map_err(|e| anyhow::anyhow!("{e}"))?;
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
            .send_request(Command::GeneralInfo, 2u32.to_be_bytes().to_vec(), REQUEST_TIMEOUT)
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

    // ---- raw value access (driver maps fields to addresses) ----

    /// Write one address as a raw u32 (bools, enums, ints, mute group).
    pub async fn write_int(&self, address: u32, value: u32, value_type: u8) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Int(value), value_type);
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        // The mixer never echoes our own writes back; cache them locally.
        if value_type == VALUE_TYPE_MUTE_GROUP {
            self.caches.type2_values
                .lock()
                .map(|mut c| c.insert(address, value))
                .expect("type2 cache poisoned");
        } else {
            self.caches.values
                .lock()
                .map(|mut c| c.insert(address, value))
                .expect("value cache poisoned");
        }
        Ok(())
    }

    /// Write one address as a big-endian float (dB faders, gains, freqs).
    pub async fn write_float(&self, address: u32, value: f32) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Float(value), VALUE_TYPE_PARAM);
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        self.caches.values
            .lock()
            .map(|mut c| c.insert(address, value.to_bits()))
            .expect("value cache poisoned");
        Ok(())
    }

    /// App-style multi-address write of raw u32 pairs.
    pub async fn write_multi(&self, pairs: &[(u32, u32)], value_type: u8) -> anyhow::Result<()> {
        let body = build_multi_set(pairs, value_type)?;
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        if value_type == VALUE_TYPE_MUTE_GROUP {
            if let Ok(mut c) = self.caches.type2_values.lock() {
                for (addr, raw) in pairs {
                    c.insert(*addr, *raw);
                }
            }
        } else if let Ok(mut c) = self.caches.values.lock() {
            for (addr, raw) in pairs {
                c.insert(*addr, *raw);
            }
        }
        Ok(())
    }

    /// Probe read (the mixer answers reads with an empty body; the live
    /// value space only arrives via pushes).
    pub async fn read_values(&self, start: u32, count: u32) -> anyhow::Result<Vec<u32>> {
        let body = build_read_request(start, count, VALUE_TYPE_PARAM);
        let resp = self
            .send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        Ok(parse_value_block(&resp.body).map(|b| b.raws).unwrap_or_default())
    }

    /// Cached type-5 word, if a push (or our own write) has populated it.
    pub fn cached(&self, address: u32) -> Option<u32> {
        self.caches.values.lock().ok()?.get(&address).copied()
    }

    /// Cached type-2 word (mute-group masters).
    pub fn cached_type2(&self, address: u32) -> Option<u32> {
        self.caches.type2_values.lock().ok()?.get(&address).copied()
    }

    /// Cached channel name slot (empty/unknown slots report `None`).
    pub fn name(&self, slot: u32) -> Option<String> {
        let name = self.caches.names.lock().ok()?.get(&slot).cloned()?;
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    pub fn snapshot_names(&self) -> HashMap<u32, String> {
        self.caches.snapshot_names.lock().map(|c| c.clone()).unwrap_or_default()
    }

    pub fn current_snapshot(&self) -> Option<u32> {
        self.caches.current_snapshot.lock().ok().and_then(|g| *g)
    }

    // ---- mute groups (fixed protocol addresses) ----

    pub async fn set_mute_group(&self, group_1_based: u32, muted: bool) -> anyhow::Result<()> {
        let address = mute_group_master_address(group_1_based)?;
        self.write_int(address, u32::from(muted), VALUE_TYPE_MUTE_GROUP)
            .await
    }

    pub fn cached_mute_group(&self, group_1_based: u32) -> Option<bool> {
        let address = mute_group_master_address(group_1_based).ok()?;
        self.cached_type2(address).map(|v| v != 0)
    }

    // ---- show snapshots (command-level, no address table needed) ----

    /// Recall a show snapshot the way the mixer app does: REQUEST `[1, n]`
    /// then RESPONSE `[1]`, fire-and-forget (no correlated reply).
    pub async fn recall_show_snapshot(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        if snapshot_1_based == 0 {
            anyhow::bail!("snapshot number must be >= 1");
        }
        let req = build_snapshot_recall_request(snapshot_1_based);
        self.send_frame_no_reply(MessageType::Request, Command::ShowSnapshot, req)
            .await?;
        self.send_frame_no_reply(
            MessageType::Response,
            Command::ShowSnapshot,
            1u32.to_be_bytes().to_vec(),
        )
        .await?;
        Ok(())
    }

    /// Save current state into a snapshot slot under `name` (op 5).
    pub async fn save_show_snapshot(&self, snapshot_1_based: u32, name: &str) -> anyhow::Result<()> {
        if snapshot_1_based == 0 {
            anyhow::bail!("snapshot number must be >= 1");
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let body = build_snapshot_save(snapshot_1_based, epoch, name);
        self.send_request(Command::ShowSnapshot, body, REQUEST_TIMEOUT)
            .await?;
        // No echo comes back for our own save; cache it like the relay would.
        if let Ok(mut c) = self.caches.snapshot_names.lock() {
            c.insert(snapshot_1_based, name.trim().to_string());
        }
        Ok(())
    }

    // ---- channel names (rename) ----

    pub async fn set_channel_name(&self, slot: u32, name: &str) -> anyhow::Result<()> {
        if slot == 0 {
            anyhow::bail!("slot must be >= 1");
        }
        let text = name.as_bytes();
        let clipped = &text[..text.len().min(63)];
        let mut padded = clipped.to_vec();
        padded.push(0);
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = Vec::with_capacity(8 + padded.len());
        body.extend_from_slice(&slot.to_be_bytes());
        body.extend_from_slice(&0x00010500u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::ChannelNames, body, REQUEST_TIMEOUT)
            .await?;
        if let Ok(mut c) = self.caches.names.lock() {
            c.insert(slot, name.trim().to_string());
        }
        Ok(())
    }

    // ---- mute groups already in separate methods; expose alias ----

    // ---- meters ----

    pub async fn enable_meter_reports(&self) -> anyhow::Result<()> {
        let addrs = dl32_family_meter_addresses();
        let _ = self
            .send_request(Command::MeterLayout, build_meter_layout(&addrs), REQUEST_TIMEOUT)
            .await;
        let _ = self
            .send_request(Command::BroadcastControl, BROADCAST_CONTROL_BODY.to_vec(), REQUEST_TIMEOUT)
            .await;
        Ok(())
    }

    // ---- multi-address write (fan-out to singles per Python) ----
    // Python's set_values sends one single write per address because the pairs
    // layout is misparsed by DL32S firmware. We keep build_multi_set for
    // compatibility but implement as fan-out singles.

    pub async fn set_values(&self, values: &std::collections::HashMap<u32, u32>, value_type: u8) -> anyhow::Result<()> {
        for (addr, raw) in values {
            self.write_int(*addr, *raw, value_type).await?;
        }
        Ok(())
    }

    // ---- delete snapshot ----

    pub async fn delete_snapshot(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        if !(1..=99).contains(&snapshot_1_based) {
            anyhow::bail!("snapshot number must be 1..99");
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0) as u32;
        let mut body = Vec::with_capacity(12);
        body.extend_from_slice(&3u32.to_be_bytes());
        body.extend_from_slice(&snapshot_1_based.to_be_bytes());
        body.extend_from_slice(&epoch.to_be_bytes());
        self.send_request(Command::ShowSnapshot, body, REQUEST_TIMEOUT)
            .await?;
        if let Ok(mut c) = self.caches.snapshot_names.lock() {
            c.remove(&snapshot_1_based);
        }
        Ok(())
    }

    // ---- reboot ----

    pub async fn reboot(&self, mode: u32) -> anyhow::Result<()> {
        if mode != 1 && mode != 2 {
            anyhow::bail!("reboot mode must be 1 or 2");
        }
        self.send_request(Command::Reboot, mode.to_be_bytes().to_vec(), REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    // ---- recall snapshot with address ----

    pub async fn recall_snapshot(&self, address: u32, snapshot_1_based: u32) -> anyhow::Result<()> {
        if snapshot_1_based == 0 {
            anyhow::bail!("snapshot number must be >= 1");
        }
        if address != 0 {
            // legacy single write
            self.write_int(address, snapshot_1_based, VALUE_TYPE_PARAM).await?;
            return Ok(());
        }
        self.recall_show_snapshot(snapshot_1_based).await
    }

    // ---- disconnect notice ----

    pub async fn disconnect_notice(&self) -> anyhow::Result<()> {
        self.send_request(Command::ChannelInfoControl, [1u32.to_be_bytes(), 4u32.to_be_bytes()].concat(), REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    // ---- device admin (0x0E / 0x0F) ----

    pub async fn request_general_info(&self, info_type: u32, timeout: std::time::Duration) -> anyhow::Result<Message> {
        self.send_request(Command::GeneralInfo, info_type.to_be_bytes().to_vec(), timeout)
            .await
    }

    pub async fn request_firmware_info(&self, timeout: std::time::Duration) -> anyhow::Result<Message> {
        self.send_request(Command::FirmwareInfo, vec![], timeout).await
    }

    pub fn info_string(body: &[u8]) -> String {
        if body.len() < 8 {
            return String::new();
        }
        let swapped = word_swap(&body[4..]);
        String::from_utf8_lossy(swapped.split(|b| *b == 0).next().unwrap_or(&[]))
            .to_string()
    }

    pub fn info_ipv4(word: u32) -> String {
        use std::net::Ipv4Addr;
        Ipv4Addr::from(word).to_string()
    }

    pub async fn get_device_name(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(3, timeout).await?.body;
        Ok(Self::info_string(&body))
    }

    pub async fn get_subdevice_name(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(7, timeout).await?.body;
        Ok(Self::info_string(&body))
    }

    pub async fn get_ip_address(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(4, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("IP address reply too short");
        }
        let word = u32::from_be_bytes(body[4..8].try_into().unwrap());
        Ok(Self::info_ipv4(word))
    }

    pub async fn get_lock_status(&self, timeout: std::time::Duration) -> anyhow::Result<u32> {
        let body = self.request_general_info(5, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("Lock status reply too short");
        }
        Ok(u32::from_be_bytes(body[4..8].try_into().unwrap()))
    }

    pub async fn get_static_ip_config(&self, timeout: std::time::Duration) -> anyhow::Result<(bool, String, String, String)> {
        let body = self.request_general_info(6, timeout).await?.body;
        if body.len() < 20 {
            anyhow::bail!("Static IP config reply too short");
        }
        let flag = u32::from_be_bytes(body[4..8].try_into().unwrap());
        let ip = u32::from_be_bytes(body[8..12].try_into().unwrap());
        let mask = u32::from_be_bytes(body[12..16].try_into().unwrap());
        let gw = u32::from_be_bytes(body[16..20].try_into().unwrap());
        Ok((flag != 0, Self::info_ipv4(ip), Self::info_ipv4(mask), Self::info_ipv4(gw)))
    }

    pub async fn get_max_message_words(&self, timeout: std::time::Duration) -> anyhow::Result<u32> {
        let body = self.request_general_info(2, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("Max message words reply too short");
        }
        Ok(u32::from_be_bytes(body[4..8].try_into().unwrap()))
    }

    pub async fn get_network_config(&self, timeout: std::time::Duration) -> anyhow::Result<Vec<u32>> {
        let body = self.request_general_info(0x12, timeout).await?.body;
        if body.len() < 4 + 65 * 4 {
            anyhow::bail!("Network config reply too short");
        }
        let mut out = Vec::with_capacity(65);
        for i in 0..65 {
            let off = 4 + i * 4;
            out.push(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()));
        }
        Ok(out)
    }

    pub async fn site_survey(&self, timeout: std::time::Duration) -> anyhow::Result<Vec<[u32; 11]>> {
        let body = self.request_general_info(0x13, timeout).await?.body;
        if body.len() < 8 {
            return Ok(Vec::new());
        }
        let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
        let mut out = Vec::new();
        let max = count.min((body.len().saturating_sub(8)) / 44);
        for i in 0..max {
            let mut entry = [0u32; 11];
            for (j, slot) in entry.iter_mut().enumerate() {
                let off = 8 + i * 44 + j * 4;
                *slot = u32::from_be_bytes(body[off..off + 4].try_into().unwrap());
            }
            out.push(entry);
        }
        Ok(out)
    }

    pub async fn set_device_name(&self, name: &str, timeout: std::time::Duration) -> anyhow::Result<()> {
        let text = name.as_bytes();
        let clipped = &text[..text.len().min(1019)];
        let mut padded = clipped.to_vec();
        // pad to word boundary with NUL
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        if padded.len() == clipped.len() {
            padded.push(0);
            while !padded.len().is_multiple_of(4) {
                padded.push(0);
            }
        }
        let mut body = Vec::with_capacity(4 + padded.len());
        body.extend_from_slice(&3u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::SetSystemState, body, timeout).await?;
        Ok(())
    }

    pub async fn set_static_ip_config(&self, is_static: bool, address: &str, mask: &str, gateway: &str, timeout: std::time::Duration) -> anyhow::Result<()> {
        fn pack_ip(s: &str) -> anyhow::Result<u32> {
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() != 4 {
                anyhow::bail!("bad ip");
            }
            let mut v = 0u32;
            for p in parts {
                v = (v << 8) | p.parse::<u32>().map_err(|_| anyhow::anyhow!("bad ip"))?;
            }
            Ok(v)
        }
        let body_bytes: [u8; 20] = {
            let mut b = [0u8; 20];
            b[0..4].copy_from_slice(&6u32.to_be_bytes());
            b[4..8].copy_from_slice(&(if is_static { 1u32 } else { 0u32 }).to_be_bytes());
            b[8..12].copy_from_slice(&pack_ip(address)?.to_be_bytes());
            b[12..16].copy_from_slice(&pack_ip(mask)?.to_be_bytes());
            b[16..20].copy_from_slice(&pack_ip(gateway)?.to_be_bytes());
            b
        };
        self.send_request(Command::SetSystemState, body_bytes.to_vec(), timeout).await?;
        Ok(())
    }

    pub async fn get_show_info(&self, timeout: std::time::Duration) -> anyhow::Result<Message> {
        self.send_request(Command::FileInfo, 5u32.to_be_bytes().to_vec(), timeout).await
    }

    pub async fn show_ok(&self) -> anyhow::Result<()> {
        self.send_frame_no_reply(MessageType::Request, Command::ShowSnapshot, 10u32.to_be_bytes().to_vec()).await
    }

    pub async fn get_show_crc(&self, pid: u32, timeout: std::time::Duration) -> anyhow::Result<Message> {
        let mut body = Vec::with_capacity(8);
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010400u32.to_be_bytes());
        self.send_request(Command::ShowStatus, body, timeout).await
    }

    pub async fn set_show_parameter(&self, pid: u32, value: u32) -> anyhow::Result<()> {
        let mut body = Vec::with_capacity(12);
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010300u32.to_be_bytes());
        body.extend_from_slice(&value.to_be_bytes());
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT).await?;
        Ok(())
    }

    pub async fn set_show_name(&self, pid: u32, name: &str) -> anyhow::Result<()> {
        let text = name.as_bytes();
        let clipped = &text[..text.len().min(63)];
        let mut padded = clipped.to_vec();
        padded.push(0);
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = Vec::with_capacity(8 + padded.len());
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010300u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::ChannelNames, body, REQUEST_TIMEOUT).await?;
        Ok(())
    }

    // ---- file transfer ----

    pub async fn download_file(&self, filename: &str, timeout: std::time::Duration) -> anyhow::Result<Vec<u8>> {
        let name = filename.as_bytes();
        let mut padded = name.to_vec();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut open_body = Vec::with_capacity(4 + padded.len());
        open_body.extend_from_slice(&0x80000004u32.to_be_bytes());
        open_body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::FileOpen, open_body, timeout).await?;

        // file info
        let info = self.send_request(Command::FileInfo, [2u32.to_be_bytes(), 0u32.to_be_bytes()].concat(), timeout).await?;
        if info.body.len() < 12 {
            anyhow::bail!("File info response too short");
        }
        let size = u32::from_be_bytes(info.body[8..12].try_into().unwrap()) as usize;
        if size == 0 {
            let _ = self.send_request(Command::FileClose, 0u32.to_be_bytes().to_vec(), timeout).await;
            return Ok(Vec::new());
        }

        let mut out = Vec::with_capacity(size);
        const CHUNK: usize = 0x2FEC;
        while out.len() < size {
            let length = CHUNK.min(size - out.len());
            let mut req = Vec::with_capacity(16);
            req.extend_from_slice(&0u32.to_be_bytes());
            req.extend_from_slice(&(size as u32).to_be_bytes());
            req.extend_from_slice(&(out.len() as u32).to_be_bytes());
            req.extend_from_slice(&(length as u32).to_be_bytes());
            let resp = self.send_request(Command::FileRead, req, timeout).await?;
            if resp.body.len() <= 16 {
                anyhow::bail!("File read returned no data");
            }
            let chunk = word_swap(&resp.body[16..]);
            let take = length.min(chunk.len());
            if take == 0 {
                anyhow::bail!("File read returned an empty chunk");
            }
            out.extend_from_slice(&chunk[..take]);
        }
        let _ = self.send_request(Command::FileClose, 0u32.to_be_bytes().to_vec(), timeout).await;
        out.truncate(size);
        Ok(out)
    }

    pub async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32, timeout: std::time::Duration) -> anyhow::Result<()> {
        let payload = data;
        let total = payload.len();
        let name = filename.as_bytes();
        let mut padded_name = name.to_vec();
        while !padded_name.len().is_multiple_of(4) {
            padded_name.push(0);
        }
        let mut open_body = Vec::with_capacity(4 + padded_name.len());
        open_body.extend_from_slice(&file_type.to_be_bytes());
        open_body.extend_from_slice(&word_swap(&padded_name));
        let resp = self.send_request(Command::FileOpen, open_body, timeout).await?;
        if resp.body.len() < 8 || u32::from_be_bytes(resp.body[0..4].try_into().unwrap()) == 0 {
            anyhow::bail!("File open for upload refused");
        }
        let file_id = u32::from_be_bytes(resp.body[4..8].try_into().unwrap());

        const CHUNK: usize = 0x2FEC;
        let mut offset = 0usize;
        while offset < total {
            let length = CHUNK.min(total - offset);
            let chunk_raw = &payload[offset..offset + length];
            let mut chunk = word_swap(chunk_raw);
            while !chunk.len().is_multiple_of(4) {
                chunk.push(0);
            }
            let mut body = Vec::with_capacity(16 + chunk.len());
            body.extend_from_slice(&file_id.to_be_bytes());
            body.extend_from_slice(&(total as u32).to_be_bytes());
            body.extend_from_slice(&(offset as u32).to_be_bytes());
            body.extend_from_slice(&(length as u32).to_be_bytes());
            body.extend_from_slice(&chunk);
            let ack = self.send_request(Command::PostFile, body, timeout).await?;
            if ack.body.len() < 4 || u32::from_be_bytes(ack.body[0..4].try_into().unwrap()) == 0 {
                anyhow::bail!("POST chunk not acknowledged");
            }
            offset += length;
        }
        // zero-length END chunk
        let mut end_body = Vec::with_capacity(16);
        end_body.extend_from_slice(&file_id.to_be_bytes());
        end_body.extend_from_slice(&(total as u32).to_be_bytes());
        end_body.extend_from_slice(&(total as u32).to_be_bytes());
        end_body.extend_from_slice(&0u32.to_be_bytes());
        let end = self.send_request(Command::PostFile, end_body, timeout).await?;
        if end.body.len() < 4 || u32::from_be_bytes(end.body[0..4].try_into().unwrap()) != 0 {
            anyhow::bail!("END of POST not accepted");
        }
        let _ = self.send_request(Command::FileClose, file_id.to_be_bytes().to_vec(), timeout).await;
        Ok(())
    }

    pub async fn set_file_crc(&self, address: u32, crc32: u32) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Int(crc32), VALUE_TYPE_FILE);
        // build_single_set uses meta count 1 | type; but type must be 4. build_single_set takes value_type param. Good.
        // However build_single_set sets meta = (1<<16) | (value_type<<8). For file crc we need 0x00010400 = count 1 type 4. Good.
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT).await?;
        Ok(())
    }

    pub async fn process_files(&self) -> anyhow::Result<()> {
        self.send_request(Command::ChannelInfoControl, 10u32.to_be_bytes().to_vec(), REQUEST_TIMEOUT).await?;
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

// ---------------------------------------------------------------------------
// Receive loop
// ---------------------------------------------------------------------------

/// Read frames until the peer closes, dispatching each to [`dispatch`].
/// Garbage bytes (failed decodes) are skipped one at a time; short reads
/// wait for more data.
async fn rx_loop(
    mut read: OwnedReadHalf,
    write: Arc<Mutex<OwnedWriteHalf>>,
    pending: Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    caches: Arc<Caches>,
) {
    let mut buf = Vec::with_capacity(65536);
    let mut tmp = [0u8; 4096];
    loop {
        // Drain every complete frame before reading more.
        loop {
            match decode_one(&buf) {
                Ok(Some((msg, used))) => {
                    buf.drain(..used);
                    dispatch(msg, &write, &pending, &caches).await;
                }
                Ok(None) => break, // truncated: read more
                Err(_) => {
                    if buf.is_empty() {
                        break;
                    }
                    buf.drain(..1); // garbage: resync one byte at a time
                }
            }
        }
        match read.read(&mut tmp).await {
            Ok(0) => break, // peer closed
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(_) => break,
        }
    }
    // Connection lost: fail every waiter.
    let senders: Vec<oneshot::Sender<PendingResult>> = pending
        .lock()
        .map(|mut p| p.drain().map(|(_, tx)| tx).collect())
        .unwrap_or_default();
    for tx in senders {
        let _ = tx.send(Err(RequestFailed("connection lost".into())));
    }
}

async fn send_frame(
    write: &Arc<Mutex<OwnedWriteHalf>>,
    seq: u8,
    msg_type: MessageType,
    command: Command,
    body: Vec<u8>,
) -> anyhow::Result<()> {
    let msg = Message::new(seq, msg_type, command, body).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut guard = write.lock().await;
    guard.write_all(&msg.encode()).await?;
    guard.flush().await?;
    Ok(())
}

fn ingest_values(cache: &StdMutex<HashMap<u32, u32>>, block: &ValueBlock) {
    if block.value_type != VALUE_TYPE_PARAM {
        return;
    }
    if let Ok(mut c) = cache.lock() {
        for (i, raw) in block.raws.iter().enumerate() {
            c.insert(block.start + i as u32, *raw);
        }
    }
}

fn ingest_type2(cache: &StdMutex<HashMap<u32, u32>>, block: &ValueBlock) {
    if block.value_type != VALUE_TYPE_MUTE_GROUP {
        return;
    }
    if let Ok(mut c) = cache.lock() {
        for (i, raw) in block.raws.iter().enumerate() {
            c.insert(block.start + i as u32, *raw);
        }
    }
}

fn ingest_names(cache: &StdMutex<HashMap<u32, String>>, body: &[u8]) {
    if let Some(pairs) = parse_name_table(body) {
        if let Ok(mut c) = cache.lock() {
            for (slot, name) in pairs {
                c.insert(slot, name);
            }
        }
    }
}

fn ingest_snapshot(
    names: &StdMutex<HashMap<u32, String>>,
    current: &StdMutex<Option<u32>>,
    body: &[u8],
) {
    match parse_snapshot_push(body) {
        Some(SnapshotEvent::Recall(n)) => {
            if let Ok(mut c) = current.lock() {
                *c = Some(n);
            }
        }
        Some(SnapshotEvent::Rename(n, name)) => {
            if let Ok(mut c) = names.lock() {
                c.insert(n, name);
            }
        }
        None => {}
    }
}

fn complete_pending(
    pending: &Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    seq: u8,
    result: PendingResult,
) {
    let tx = pending.lock().map(|mut p| p.remove(&seq)).unwrap_or(None);
    if let Some(tx) = tx {
        let _ = tx.send(result);
    }
}

async fn dispatch(
    msg: Message,
    write: &Arc<Mutex<OwnedWriteHalf>>,
    pending: &Arc<StdMutex<HashMap<u8, oneshot::Sender<PendingResult>>>>,
    caches: &Caches,
) {
    let values = &caches.values;
    let type2_values = &caches.type2_values;
    let names = &caches.names;
    let snapshot_names = &caches.snapshot_names;
    let current_snapshot = &caches.current_snapshot;

    match msg.msg_type {
        MessageType::Request => {
            let reply_body: Vec<u8> = match msg.command {
                Command::ClientHandshake => HANDSHAKE_REPLY.to_vec(),
                Command::GeneralInfo => GENERAL_INFO_REPLY.to_vec(),
                Command::ChannelInfoControl => msg.body.get(..4).unwrap_or(&[]).to_vec(),
                Command::ShowSnapshot => {
                    ingest_snapshot(snapshot_names, current_snapshot, &msg.body);
                    msg.body.get(..4).unwrap_or(&[]).to_vec()
                }
                Command::ChannelValues => {
                    if let Some(block) = parse_value_block(&msg.body) {
                        ingest_values(values, &block);
                        ingest_type2(type2_values, &block);
                    }
                    Vec::new()
                }
                Command::ChannelNames => {
                    ingest_names(names, &msg.body);
                    Vec::new()
                }
                _ => Vec::new(),
            };
            let _ = send_frame(write, msg.seq, MessageType::Response, msg.command, reply_body).await;
        }
        MessageType::Broadcast => {
            if msg.command == Command::ChannelValues {
                if let Some(block) = parse_value_block(&msg.body) {
                    ingest_values(values, &block);
                    ingest_type2(type2_values, &block);
                }
            }
        }
        MessageType::Response | MessageType::Error => {
            if msg.command == Command::ChannelValues {
                if let Some(block) = parse_value_block(&msg.body) {
                    ingest_values(values, &block);
                    ingest_type2(type2_values, &block);
                }
            }
            if msg.msg_type == MessageType::Error {
                complete_pending(
                    pending,
                    msg.seq,
                    Err(RequestFailed("mixer returned ERROR".into())),
                );
            } else {
                complete_pending(pending, msg.seq, Ok(msg));
            }
        }
    }
}

/// Test helper: bind a loopback listener for fake-mixer tests.
pub async fn loopback_listener() -> anyhow::Result<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    Ok((listener, addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keepalive_request(seq: u8) -> Vec<u8> {
        vec![HEADER_MARKER, seq, 0, 0, 0, 0x01, 0xFF, 0x52]
    }

    #[test]
    fn keepalive_frame_vector() {
        // Hand-computed: header AB 01 00 00 00 01 sums to 0xAD,
        // checksum 0xFFFF-0xAD = 0xFF52, no body.
        let msg = Message::new(1, MessageType::Request, Command::KeepAlive, vec![]).unwrap();
        assert_eq!(msg.encode(), keepalive_request(1));
        let (back, used) = decode_one(&keepalive_request(1)).unwrap().unwrap();
        assert_eq!(used, FRAME_PREFIX_LEN);
        assert_eq!(back, msg);
    }

    #[test]
    fn single_set_frame_vector() {
        // Mute input ch1 (address 8) to 1: body 00000008|00010500|00000001.
        // Body bytes sum to 15, checksum 0xFFFFFFF0. Header AB 01 00 03 00 13
        // sums to 0xC2, checksum 0xFF3D.
        let body = build_single_set(8, RawValue::Int(1), VALUE_TYPE_PARAM);
        assert_eq!(
            body,
            vec![0, 0, 0, 8, 0, 1, 5, 0, 0, 0, 0, 1]
        );
        let msg = Message::new(1, MessageType::Request, Command::ChannelValues, body).unwrap();
        let bytes = msg.encode();
        assert_eq!(
            &bytes[..FRAME_PREFIX_LEN],
            &[HEADER_MARKER, 1, 0, 3, 0, 0x13, 0xFF, 0x3D]
        );
        assert_eq!(&bytes[FRAME_PREFIX_LEN + 12..], &[0xFF, 0xFF, 0xFF, 0xF0]);
        let (back, used) = decode_one(&bytes).unwrap().unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(back, msg);
    }

    #[test]
    fn header_checksum_rejects_corruption() {
        let mut bad = keepalive_request(1);
        bad[7] ^= 0xFF;
        match decode_one(&bad) {
            Err(FrameError::BadHeaderChecksum { .. }) => {}
            other => panic!("expected checksum error, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(matches!(
            decode_one(&[0x00, 0x01, 0x02, 0x03, 0, 0, 0, 0]),
            Err(FrameError::BadMarker(0x00))
        ));
        assert!(matches!(
            decode_one(&[HEADER_MARKER, 1, 0, 0, 0x42, 0x01, 0, 0]),
            Err(FrameError::UnknownType(0x42))
        ));
        // Truncated body: header claims 1 chunk, only half present.
        let mut short = keepalive_request(1);
        short[2] = 0;
        short[3] = 1; // chunk count 1, no body bytes follow
        let recomputed = {
            let h: [u8; 6] = short[..6].try_into().unwrap();
            Message::header_checksum(&h).to_be_bytes()
        };
        short[6] = recomputed[0];
        short[7] = recomputed[1];
        assert_eq!(decode_one(&short).unwrap(), None);
    }

    #[test]
    fn word_swap_vector() {
        // SPEC: "MGZU" travels as 55 5A 47 4D.
        assert_eq!(word_swap(&[0x55, 0x5A, 0x47, 0x4D]), b"MGZU");
        assert_eq!(word_swap(b"MGZU"), vec![0x55, 0x5A, 0x47, 0x4D]);
        assert!(word_swap(&[]).is_empty());
        // Trailing partial word is dropped.
        assert_eq!(word_swap(&[1, 2, 3, 4, 5]), vec![4, 3, 2, 1]);
    }

    #[test]
    fn bulk_push_count_zero_means_all() {
        // Bulk push shape: meta 0x00000500 (type 5, count 0), 3 values.
        let mut body = vec![0, 0, 0, 8, 0, 0, 5, 0];
        for v in [1u32, 0xC1E00000, 0u32] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        let block = parse_value_block(&body).unwrap();
        assert_eq!(block.start, 8);
        assert_eq!(block.value_type, VALUE_TYPE_PARAM);
        assert_eq!(block.raws, vec![1, 0xC1E00000, 0]);
    }

    #[test]
    fn explicit_count_is_honoured() {
        let mut body = vec![0, 0, 0, 8, 0, 2, 5, 0];
        for v in [1u32, 2, 3] {
            body.extend_from_slice(&v.to_be_bytes());
        }
        let block = parse_value_block(&body).unwrap();
        assert_eq!(block.raws, vec![1, 2]);
    }

    #[test]
    fn multi_set_flag_shape() {
        let body = build_multi_set(&[(8, 1), (9, 0xC1E00000)], VALUE_TYPE_PARAM).unwrap();
        // chunk0 = addr0, chunk1 = meta with 0x2000 flag, chunk2 = val0,
        // then (addrN, valN) pairs.
        let meta = u32::from_be_bytes(body[4..8].try_into().unwrap());
        assert_eq!(meta, ((2 | 0x2000) << 16) | ((VALUE_TYPE_PARAM as u32) << 8));
        assert_eq!(&body[..4], &8u32.to_be_bytes());
        assert_eq!(&body[8..12], &1u32.to_be_bytes());
        assert_eq!(&body[12..16], &9u32.to_be_bytes());
        assert!(build_multi_set(&[], VALUE_TYPE_PARAM).is_err());
    }

    #[test]
    fn mute_group_helpers() {
        assert_eq!(mute_group_master_address(1).unwrap(), 11);
        assert_eq!(mute_group_master_address(6).unwrap(), 16);
        assert!(mute_group_master_address(0).is_err());
        assert!(mute_group_master_address(7).is_err());
        assert_eq!(mute_group_name_slot(1).unwrap(), 64);
        assert_eq!(mute_group_name_slot(6).unwrap(), 69);
    }

    #[test]
    fn meter_layout_shape() {
        let addrs = dl32_family_meter_addresses();
        assert_eq!(addrs.len(), 32 + 4 + 4 + 14 + 4 + 2);
        assert_eq!(addrs[0], 34);
        assert_eq!(addrs[31], 34 + 31 * 7);
        let body = build_meter_layout(&addrs);
        assert_eq!(body.len(), 4 + addrs.len() * 4);
        assert_eq!(&body[..4], &[0, 0, 0, 1]);
        assert_eq!(&body[4..8], &34i32.to_be_bytes());
        assert_eq!(BROADCAST_CONTROL_BODY.len(), 12);
    }

    #[test]
    fn name_table_keeps_blank_slots() {
        // Slots 1-3: "Kick", "", "Snare" — the blank still consumes index 2.
        let text = b"Kick\x00\x00Snare\x00\x00\x00";
        let mut padded = text.to_vec();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = vec![];
        body.extend_from_slice(&1u32.to_be_bytes());
        body.extend_from_slice(&0x00030000u32.to_be_bytes()); // count = 3 slots
        body.extend_from_slice(&word_swap(&padded));
        let pairs = parse_name_table(&body).unwrap();
        assert_eq!(
            pairs,
            vec![
                (1, "Kick".to_string()),
                (2, String::new()),
                (3, "Snare".to_string()),
            ]
        );
    }

    #[test]
    fn name_table_count_bounds_single_rename() {
        // A one-slot rename must not blank the neighbour with padding NULs.
        let mut padded = b"Vox\x00".to_vec();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = vec![];
        body.extend_from_slice(&5u32.to_be_bytes());
        body.extend_from_slice(&0x00010000u32.to_be_bytes()); // count = 1 slot
        body.extend_from_slice(&word_swap(&padded));
        assert_eq!(parse_name_table(&body).unwrap(), vec![(5, "Vox".to_string())]);
    }

    #[test]
    fn snapshot_push_events() {
        let mut recall = vec![];
        recall.extend_from_slice(&1u32.to_be_bytes());
        recall.extend_from_slice(&7u32.to_be_bytes());
        assert_eq!(parse_snapshot_push(&recall), Some(SnapshotEvent::Recall(7)));

        let mut save = vec![];
        save.extend_from_slice(&5u32.to_be_bytes());
        save.extend_from_slice(&2u32.to_be_bytes());
        save.extend_from_slice(&0u32.to_be_bytes()); // epoch placeholder
        let mut name = b"Main\x00".to_vec();
        while !name.len().is_multiple_of(4) {
            name.push(0);
        }
        save.extend_from_slice(&word_swap(&name));
        assert_eq!(
            parse_snapshot_push(&save),
            Some(SnapshotEvent::Rename(2, "Main".to_string()))
        );
        assert_eq!(parse_snapshot_push(&[0; 4]), None);
    }

    #[test]
    fn snapshot_save_body_shape() {
        let body = build_snapshot_save(3, 1_700_000_000, "Main");
        assert_eq!(&body[..12], &[0, 0, 0, 5, 0, 0, 0, 3, 0x65, 0x53, 0xF1, 0x00][..]);
        // Name rides word-swapped after the 12-byte header.
        assert_eq!(word_swap(&body[12..])[..4], [b'M', b'a', b'i', b'n']);
    }

    // ---- loopback session test: fake mixer over TCP, no hardware ----

    /// Minimal fake mixer: answers init requests with empty responses, then
    /// after the second CHANNEL_INFO_CONTROL pushes a bulk value block, a
    /// name table and a snapshot rename as REQUESTs (client must answer).
    /// Records every CHANNEL_VALUES write it receives.
    struct FakeMixer {
        writes: StdMutex<Vec<(u32, u32)>>,
    }

    async fn read_frame(stream: &mut TcpStream) -> anyhow::Result<Message> {
        let mut prefix = [0u8; FRAME_PREFIX_LEN];
        stream.read_exact(&mut prefix).await?;
        let chunks = u16::from_be_bytes([prefix[2], prefix[3]]) as usize;
        let mut rest = vec![0u8; chunks * 4 + if chunks == 0 { 0 } else { 4 }];
        if !rest.is_empty() {
            stream.read_exact(&mut rest).await?;
        }
        let mut full = prefix.to_vec();
        full.extend_from_slice(&rest);
        let (msg, _) = decode_one(&full)?.ok_or_else(|| anyhow::anyhow!("short frame"))?;
        Ok(msg)
    }

    async fn write_msg(stream: &mut TcpStream, msg: &Message) -> anyhow::Result<()> {
        stream.write_all(&msg.encode()).await?;
        Ok(())
    }

    fn response_to(req: &Message, body: Vec<u8>) -> Message {
        Message {
            seq: req.seq,
            msg_type: MessageType::Response,
            command: req.command,
            body,
        }
    }

    #[tokio::test]
    async fn session_init_push_write_loopback() {
        let (listener, addr) = loopback_listener().await.unwrap();
        let fake = std::sync::Arc::new(FakeMixer {
            writes: StdMutex::new(Vec::new()),
        });
        let fake_task = {
            let fake = fake.clone();
            tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut cic_count = 0;
                let mut seq: u8 = 40;
                let mut next_seq = move || {
                    seq = seq.wrapping_add(1);
                    if seq == 0 {
                        seq = 1;
                    }
                    seq
                };
                loop {
                    let req = match read_frame(&mut stream).await {
                        Ok(m) => m,
                        Err(_) => break,
                    };
                    match (req.command, req.msg_type) {
                        (Command::KeepAlive, MessageType::Request) => {}
                        (Command::ChannelValues, MessageType::Request)
                            if req.body.len() > 8 =>
                        {
                            // A write: record (address, raw) pairs.
                            if let Some(block) = parse_value_block(&req.body) {
                                let mut w = fake.writes.lock().unwrap();
                                for (i, raw) in block.raws.iter().enumerate() {
                                    w.push((block.start + i as u32, *raw));
                                }
                            }
                            let resp = response_to(&req, vec![]);
                            write_msg(&mut stream, &resp).await.unwrap();
                        }
                        _ => {
                            let resp = response_to(&req, vec![]);
                            write_msg(&mut stream, &resp).await.unwrap();
                            if req.command == Command::ChannelInfoControl {
                                cic_count += 1;
                                if cic_count == 2 {
                                    // Bulk push: ch1 mute=1 (addr 8),
                                    // ch1 fader=-15 dB float (addr 9).
                                    let mut body = vec![];
                                    body.extend_from_slice(&8u32.to_be_bytes());
                                    body.extend_from_slice(&0x00000500u32.to_be_bytes());
                                    body.extend_from_slice(&1u32.to_be_bytes());
                                    body.extend_from_slice(&(-15.0f32).to_be_bytes());
                                    let push = Message {
                                        seq: next_seq(),
                                        msg_type: MessageType::Request,
                                        command: Command::ChannelValues,
                                        body,
                                    };
                                    write_msg(&mut stream, &push).await.unwrap();
                                    // Name table: slot 1 "Kick", slot 2 blank,
                                    // slot 3 "Snare".
                                    let text = b"Kick\x00\x00Snare\x00\x00\x00";
                                    let mut padded = text.to_vec();
                                    while !padded.len().is_multiple_of(4) {
                                        padded.push(0);
                                    }
                                    let mut nbody = vec![];
                                    nbody.extend_from_slice(&1u32.to_be_bytes());
                                    nbody.extend_from_slice(&0x00030000u32.to_be_bytes());
                                    nbody.extend_from_slice(&word_swap(&padded));
                                    let names = Message {
                                        seq: next_seq(),
                                        msg_type: MessageType::Request,
                                        command: Command::ChannelNames,
                                        body: nbody,
                                    };
                                    write_msg(&mut stream, &names).await.unwrap();
                                    // Snapshot rename relay: slot 2 "Main".
                                    let mut sbody = vec![];
                                    sbody.extend_from_slice(&5u32.to_be_bytes());
                                    sbody.extend_from_slice(&2u32.to_be_bytes());
                                    sbody.extend_from_slice(&0u32.to_be_bytes());
                                    let mut nm = b"Main\x00".to_vec();
                                    while !nm.len().is_multiple_of(4) {
                                        nm.push(0);
                                    }
                                    sbody.extend_from_slice(&word_swap(&nm));
                                    let snap = Message {
                                        seq: next_seq(),
                                        msg_type: MessageType::Request,
                                        command: Command::ShowSnapshot,
                                        body: sbody,
                                    };
                                    write_msg(&mut stream, &snap).await.unwrap();
                                }
                            }
                        }
                    }
                }
            })
        };

        let session = MackieSession::connect(&addr.to_string()).await.unwrap();

        // Pushes are processed by the rx task; wait for the cache to fill.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if session.cached(8) == Some(1) {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("bulk push never ingested");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(session.cached(8), Some(1));
        assert_eq!(session.cached(9), Some((-15.0f32).to_bits()));
        // No echo for our own writes yet; unknown address reads None.
        assert_eq!(session.cached(10), None);

        assert_eq!(session.name(1).as_deref(), Some("Kick"));
        assert_eq!(session.name(2), None); // blank slot
        assert_eq!(session.name(3).as_deref(), Some("Snare"));
        assert_eq!(
            session.snapshot_names().get(&2).map(String::as_str),
            Some("Main")
        );

        // Writes go out as single-set frames and land in the fake's log.
        session.write_int(8, 0, VALUE_TYPE_PARAM).await.unwrap();
        session
            .write_float(9, -15.0)
            .await
            .unwrap();
        session.set_mute_group(1, true).await.unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if fake.writes.lock().unwrap().len() >= 3 {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                panic!("writes never reached the fake");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let writes = fake.writes.lock().unwrap().clone();
        assert!(writes.contains(&(8, 0)));
        assert!(writes.contains(&(9, (-15.0f32).to_bits())));
        assert!(writes.contains(&(11, 1))); // mute group 1 master
        // Own writes are cached locally without an echo.
        assert_eq!(session.cached(8), Some(0));
        assert_eq!(session.cached_type2(11), Some(1));

        session.close();
        fake_task.abort();
    }
}
