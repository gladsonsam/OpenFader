//! Mackie DL-series TCP wire protocol (DL16S/DL32S/DL32R family).
//!
//! Black-box port of the proven Python client (`mackie_dl/client.py`) and its
//! spec (`SPEC.md`): one client-initiated TCP connection, default port 50001,
//! all values big-endian. This module owns everything on the wire, framing,
//! checksums, value-block encode/decode, the init handshake, the receive loop
//! with request/response correlation, and the push-driven caches.
//!
//! It deliberately owns NO per-model address tables: strip layout (base,
//! stride, field offsets) lives in the driver crate (`proto-dl32s`). Session
//! methods take explicit addresses; the driver maps fields to addresses.
//! Mute-group masters and the meter/output-strip lists are fixed protocol
//! addresses shared by the whole DL family, so they live here.

use std::net::SocketAddr;
use tokio::net::TcpListener;

#[cfg(test)]
use std::{sync::Mutex as StdMutex, time::Duration};
#[cfg(test)]
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
};

mod frame;
mod identify;
mod names;
mod protocol;
mod session;
mod values;

/// DL32S-only geometry and native meter addresses; shared handshake stays legacy.
pub mod dl32s_meters;

pub use frame::{
    decode_one, word_swap, Command, FrameError, Message, MessageType, FRAME_PREFIX_LEN, HEADER_LEN,
    HEADER_MARKER,
};
pub use identify::identify;
pub use names::{
    build_snapshot_delete, build_snapshot_store, build_snapshot_recall_request, build_snapshot_save, parse_name_table, parse_snapshot_push,
    SnapshotEvent,
};
pub use protocol::{
    build_meter_layout, dl32_family_meter_addresses, dl32s_dynamics_meter_addresses,
    dl32s_output_meter_addresses,
    dl32s_meter_addresses, mute_group_master_address, mute_group_name_slot, BROADCAST_CONTROL_BODY,
    DEFAULT_PORT, DIAL_TIMEOUT, GENERAL_INFO_REPLY, HANDSHAKE_REPLY, KEEPALIVE_INTERVAL,
    MUTE_GROUP_BASE_ADDRESS, MUTE_GROUP_COUNT, MUTE_GROUP_NAME_SLOT_BASE, REQUEST_TIMEOUT,
    VALUE_TYPE_FILE, VALUE_TYPE_METERS, VALUE_TYPE_MUTE_GROUP, VALUE_TYPE_PARAM, VALUE_TYPE_SHOW,
};
pub use session::{MackieSession, MackieValueCache, PushEvent};
pub use values::{
    build_multi_set, build_read_request, build_single_set, decode_word, parse_value_block,
    parse_value_blocks, DecodedWord, RawValue, ValueBlock,
};

/// Test helper: bind a loopback listener for fake-mixer tests.
pub async fn loopback_listener() -> anyhow::Result<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    Ok((listener, addr))
}

#[cfg(test)]
mod tests;
