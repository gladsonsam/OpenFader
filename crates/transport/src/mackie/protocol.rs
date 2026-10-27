//! Shared value spaces, meter layouts, and handshake reply constants.

use super::dl32s_meters;
use std::time::Duration;

/// Default mixer TCP port.
pub const DEFAULT_PORT: u16 = 50001;

/// Keepalive period: the mixer expects a ping roughly every 2.5 s.
pub const KEEPALIVE_INTERVAL: Duration = Duration::from_millis(2500);
/// Request timeout for correlated request/response exchanges. Generous for
/// a ~500 ms relay path (6x headroom per round trip); the init handshake
/// runs several of these in sequence, so a relayed connect takes seconds.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
/// TCP dial timeout: a blackhole route (dead relay leg) must fail fast
/// instead of hanging `Cmd::Connect` on OS SYN retries. Generous for a
/// ~500 ms relay path, far below platform defaults.
pub const DIAL_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Value space
// ---------------------------------------------------------------------------

/// `type` byte selecting the CHANNEL_VALUES address space: normal parameters.
pub const VALUE_TYPE_PARAM: u8 = 5;
/// Mute-group master address space (kept in a separate cache: address 11 is
/// input 1's `main_assign` under type 5 and mute group 1's master under type 2).
pub const VALUE_TYPE_MUTE_GROUP: u8 = 2;
/// Meter address space, pushed periodically once the meter-layout /
/// broadcast-control init runs. Words are big-endian IEEE-754 dB; on the
/// DL32S the first 32 words of a `start == 1` block are the ordered input
/// meters (rig-verified; decoded in `proto-dl32s`, not here). Transport
/// stays encoding-agnostic and forwards every block via `PushEvent::Values`.
pub const VALUE_TYPE_METERS: u8 = 1;
/// Show-parameter address space (set_show_parameter uses type 3).
pub const VALUE_TYPE_SHOW: u8 = 3;
/// File-CRC address space (set_file_crc uses type 4).
pub const VALUE_TYPE_FILE: u8 = 4;

pub const MUTE_GROUP_COUNT: u32 = 6;
/// Mute group N master mute, verified for all six on a DL32S.
pub const MUTE_GROUP_BASE_ADDRESS: u32 = 10;
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

// ---------------------------------------------------------------------------
// Meter layout / broadcast control (init sequence data)
// ---------------------------------------------------------------------------

/// Legacy 60-word family list retained for the shared init handshake and
/// other models. Its output aliases are NOT DL32S geometry; the DL32S driver
/// explicitly replaces it with its native list after handshake.
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

/// DL32S strip prefix plus passive RTA (93 words).
pub fn dl32s_meter_addresses() -> Vec<i32> {
    let mut addrs = dl32s_meters::STRIP_ADDRESSES.to_vec();
    addrs.extend(3..3 + dl32s_meters::RTA_BIN_COUNT as i32);
    addrs
}

/// DL32S input/RTA prefix plus gate IN/GR/OUT, compressor IN/GR/OUT
/// per input. Historical input probes pin this six-word order and scale.
pub fn dl32s_dynamics_meter_addresses() -> Vec<i32> {
    let mut addrs = dl32s_meter_addresses();
    for z in 0..dl32s_meters::INPUT_COUNT as i32 {
        let first = 35 + z * 7;
        addrs.extend(first..first + dl32s_meters::DYNAMICS_WIDTH as i32);
    }
    addrs
}

/// DL32S 285-word dynamics prefix plus 16 output compressor GR samples.
/// Aux1 positive f32 dB attenuation is physically calibrated; other outputs
/// share native getter semantics but remain inferred by the provider.
pub fn dl32s_output_meter_addresses() -> Vec<i32> {
    let mut addrs = dl32s_dynamics_meter_addresses();
    addrs.extend_from_slice(&dl32s_meters::OUTPUT_GR_ADDRESSES);
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
pub const BROADCAST_CONTROL_BODY: [u8; 12] = [
    0x00, 0x00, 0x00, 0x01, 0x10, 0x00, 0x01, 0x00, 0x00, 0x5A, 0x00, 0x01,
];

// ---------------------------------------------------------------------------
// Replies the client owes the mixer (mixer -> client REQUESTs)
// ---------------------------------------------------------------------------

/// Answer to CLIENT_HANDSHAKE (the real MAC would be ideal; zeroes work).
pub const HANDSHAKE_REPLY: [u8; 8] = [0x10, 0x40, 0, 0, 0, 0, 0, 0];
/// Answer to GENERAL_INFO.
pub const GENERAL_INFO_REPLY: [u8; 8] = [0, 0, 0, 2, 0, 0, 0x40, 0];
