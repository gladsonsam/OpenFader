//! Shared transport: tokio UDP/TCP peers, generic OSC codec, OSC session
//! (query/keepalive/meters), LAN discovery probes, meter parsing + ring
//! buffers, reconnect backoff. No UI.
//!
//! Driver crates use these helpers plus thin mapping tables; no socket or
//! codec code is duplicated per provider.
pub mod backoff;
pub mod discovery;
pub mod mackie;
pub mod meters;
pub mod osc;
pub mod session;
pub mod tcp;

pub use backoff::{backoff_delays, connect_with_backoff, Backoff};
pub use discovery::{
    discover_mdns, discover_xair, mdns_query, parse_info_reply, probe, DiscoveredDevice,
    ProbeSpec, INFO_PROBE, MDNS_ADDR, PROBE_TABLE, XAIR_PORT, XINFO_PROBE,
};
pub use meters::{
    build_meter_blob, is_meter_addr, parse_meter_blob, parse_meter_message, MeterHub, MeterRing,
    METER_CAPACITY, METER_INTERVAL, METER_PREFIX,
};
pub use mackie::{
    build_meter_layout, build_multi_set, build_read_request, build_single_set,
    build_snapshot_recall_request, build_snapshot_save, decode_one, decode_word,
    dl32_family_meter_addresses, mute_group_master_address, mute_group_name_slot,
    parse_name_table, parse_snapshot_push, parse_value_block, word_swap, Command, DecodedWord,
    MackieSession, Message, MessageType, RawValue, SnapshotEvent, ValueBlock,
    BROADCAST_CONTROL_BODY, DEFAULT_PORT, FRAME_PREFIX_LEN, GENERAL_INFO_REPLY, HANDSHAKE_REPLY,
    HEADER_LEN, HEADER_MARKER, KEEPALIVE_INTERVAL, MUTE_GROUP_BASE_ADDRESS, MUTE_GROUP_COUNT,
    MUTE_GROUP_NAME_SLOT_BASE, REQUEST_TIMEOUT, VALUE_TYPE_METERS, VALUE_TYPE_MUTE_GROUP,
    VALUE_TYPE_PARAM,
};
pub use osc::{
    clamp01, decode_packet, encode_message, fader_arg, fader_value, flatten, int_arg, is_on,
    mute_is_muted, on_arg, query_message, string_args,
};
pub use session::{
    resolve_peer, keepalive_ticker, KeepaliveConfig, OscSession, UdpPeer, DEFAULT_QUERY_TIMEOUT,
};
pub use tcp::{encode_frame, split_frames};
