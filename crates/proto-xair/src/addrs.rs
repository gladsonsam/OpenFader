//! X-Air address table. The wire is 1-based with zero-padded two-digit
//! channel numbers (`/ch/01/…`); [`StripId`](openfader_core::StripId) is
//! 0-based everywhere, so [`channel`] adds 1. Buses are the exception:
//! the desk answers single-digit `/bus/1/…` and stays silent on
//! zero-padded `/bus/01/…` (hardware-verified XR18 FW 1.20, T2), so
//! [`bus_fader`] does NOT pad.
//! The `ch_*` builders below take 1-based *wire* numbers; `bus_fader`
//! takes a 1-based wire bus number.
pub const PORT: u16 = 10024;
pub const XREMOTE: &str = "/xremote";
pub const INFO: &str = "/info";
/// Clear-solo action leaf (fire-and-forget). Rig-verified answering
/// (int) on XR18 FW 1.20; no undo leaf answers (probed 2026-10-18).
pub const CLEAR_SOLO: &str = "/-action/clearsolo";
/// Solo switch bank: the desk answers queries here with one int per
/// channel (1 = soloed), and 1-based `/-stat/solosw/NN` leaves take
/// fire-and-forget sets. The desk broadcasts leaf updates to
/// `/xremote` subscribers on solo changes (surface or another OSC client —
/// decoded by [`crate::parse_inbound`]). Rig-verified XR18 FW 1.20.
pub const SOLO_SW: &str = "/-stat/solosw";
/// Meter stream id requested from the mixer (`/meters/1` carries the
/// 16-ch + aux + FX + bus + main + monitor blob; see xr-meters).
pub const METER_SUBSCRIBE: &str = "/meters/1";
/// Subscribe *address*: the desk streams only after receiving `/meters`
/// with the stream id as a string arg (NOT a bare/empty query to
/// `/meters/1` — the desk is silent there, baseline T4). Form is
/// community-documented (xr-meters working code, x-Air Remote Protocol
/// doc `/meters ,si "/meters/0" 8`); `/meters/1` is now owner-rig
/// verified (xair-protocol.md), the `/meters/6` form below is not.
pub const METER_REQUEST: &str = "/meters";
/// ALL DYN stream id: one blob of 16 gate + 16 dyn + 6 bus + 1 LR GR
/// samples (see [`crate::dyn_gr`]). Behringer *X AIR Mixer OSC Document*
/// fw 1.11, community-documented — NOT owner-rig verified. Do not
/// confuse with X32 `/meters/6 <channel_id>` (protocol reference doc), which is a
/// per-channel strip meter on that desk.
pub const METER_DYN_SUBSCRIBE: &str = "/meters/6";
/// Per-channel meter stream id (`/meters/0` + channel id arg → 8 meters
/// incl. gate + comp GR for ONE channel). Documented in the same fw 1.11
/// doc; the driver does NOT subscribe it — one `/meters/6` subscribe
/// covers all 16 channels, so no parser is implemented here either.
/// Hardware fader readback tolerance. The desk quantizes fader writes to
/// ~10-bit steps (writing 0.5 reads back 0.49951124, off by ~5e-4), so
/// fader round-trips must assert with ~1e-3 epsilon (baseline T8).
/// Send levels and FX params round-trip bit-exact; keep exact asserts
/// there.
pub const FADER_READBACK_EPSILON: f32 = 1e-3;
/// Main LR fader. X32 uses `/main/st/mix/fader`; X-Air uses `/lr/*`.
/// Hardware-verified: `/main/st/mix/fader` gets NO reply on XR18 FW 1.20.
pub const MAIN_FADER: &str = "/lr/mix/fader";
/// Main LR mute (`/lr/mix/on`, int `1` = unmuted / `0` = muted).
/// Hardware-verified answering on XR18 FW 1.20 (2026-10-18).
pub const MAIN_ON: &str = "/lr/mix/on";
/// Main LR name, by analogy with `/ch/XX/config/name` (not yet live-queried).
pub const MAIN_NAME: &str = "/lr/config/name";
/// Stereo aux return strip. Hardware-verified (baseline §2): `/rtn/aux`
/// answers fader/on/name like a channel. The XR18 is a 16-channel desk
/// plus this return — `/ch/17/*` and `/ch/18/*` get NO reply (T3).
pub const RTN_AUX_FADER: &str = "/rtn/aux/mix/fader";
/// Aux return mute (hardware-verified present, baseline §2).
pub const RTN_AUX_ON: &str = "/rtn/aux/mix/on";
/// Aux return name (hardware-verified present, baseline §2).
pub const RTN_AUX_NAME: &str = "/rtn/aux/config/name";

/// XR18 channel count, hardware-verified (T3): `/ch/01..16` reply,
/// `/ch/17..18` are silent. Stereo aux returns live at `/rtn/aux`.
pub const XR18_CHANNELS: u16 = 16;
/// X32-kind channel count (NOT hardware-verified on X-Air; kept so
/// 32-strip maps still validate when the model is explicitly selected).
pub const X32KIND_CHANNELS: u16 = 32;
pub const MAX_BUS: u8 = 6;

/// Fallback UDP port for the connect-time sweep: some firmware/network
/// setups answer on 10023 instead of 10024. Owner rig (XR18 FW 1.20)
/// is silent on 10023 — the fallback is a no-op there by design.
pub const FALLBACK_PORT: u16 = 10023;

/// Per-probe timeout for the connect-time port sweep. Connect path only;
/// steady-state queries keep the 2 s `DEFAULT_QUERY_TIMEOUT` (venue-wifi
/// safety: a slow gig reply must not drop a poll read).
pub const CONNECT_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(400);

/// Mute-group master count, rig-verified (XR18 FW 1.20, 2026-10-18):
/// `/config/mute/1..4` answer int, `/config/mute/5..8` are silent.
/// (`/config/mute/N/name` and per-channel membership leaves are silent
/// — masters only, no assign path.)
pub const MUTE_GROUPS: u8 = 4;

/// OSC address of one mute-group master (`/config/mute/N`, 1-based,
/// `1..=MUTE_GROUPS`). Out-of-range groups error locally instead of
/// hitting a silent address.
pub fn mute_group(group: u8) -> anyhow::Result<String> {
    if group >= MUTE_GROUPS {
        anyhow::bail!("mute group {group} out of range 0..{MUTE_GROUPS} (0-based)");
    }
    Ok(format!("/config/mute/{}", group + 1))
}

/// Meter renewal interval: the desk stops streaming ~10 s after a
/// subscribe (INFERRED timing — owner rig saw only ~5 blobs/12 s without
/// renewals; 10 s expiry suspected, unconfirmed until rig confirms).
/// The driver re-sends `/xremote` + both `/meters` subscribes on this
/// schedule to keep levels (and GR) flowing. Overrides (e.g. tests) pass
/// an explicit [`std::time::Duration`] to
/// [`crate::XAirDriver::subscribe_meters_with_interval`].
pub const METER_RENEW_INTERVAL: std::time::Duration = std::time::Duration::from_secs(8);

/// Mixer geometry: selects the channel-count validation bound.
/// Defaults to [`XAirModel::Xr18`] (the hardware-verified desk).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum XAirModel {
    /// XR18 (also XR16): 16 channels + `/rtn/aux`. Hardware-verified (T3).
    #[default]
    Xr18,
    /// X32-kind maps: 32 channels. NOT hardware-verified on X-Air.
    X32Kind,
}

impl XAirModel {
    pub const fn channels(self) -> u16 {
        match self {
            XAirModel::Xr18 => XR18_CHANNELS,
            XAirModel::X32Kind => X32KIND_CHANNELS,
        }
    }
}

pub fn ch_fader(ch: u16) -> String {
    format!("/ch/{ch:02}/mix/fader")
}
pub fn ch_on(ch: u16) -> String {
    format!("/ch/{ch:02}/mix/on")
}
pub fn ch_name(ch: u16) -> String {
    format!("/ch/{ch:02}/config/name")
}
pub fn bus_fader(bus: u8) -> String {
    // Single-digit on purpose: the desk is silent on `/bus/01/…` (T2).
    format!("/bus/{bus}/mix/fader")
}

/// 0-based strip index to 1-based wire channel (`StripId(0)` → ch 1),
/// validated against the XR18 default (16 channels, T3). Use
/// [`channel_in`] for an explicit model bound.
pub fn channel(strip: u16) -> anyhow::Result<u16> {
    channel_in(strip, XR18_CHANNELS)
}

/// 0-based strip index to 1-based wire channel with an explicit
/// per-model bound (`XAirModel::channels`).
pub fn channel_in(strip: u16, max: u16) -> anyhow::Result<u16> {
    if strip >= max {
        anyhow::bail!(
            "channel strip {strip} out of range 0..={} (0-based)",
            max - 1
        );
    }
    Ok(strip + 1)
}

/// 0-based bus index to 1-based wire bus (bus 0 → `/bus/1/…`, single
/// digit — the desk is silent on zero-padded `/bus/01/…`, T2).
pub fn bus(strip: u8) -> anyhow::Result<u8> {
    if strip >= MAX_BUS {
        anyhow::bail!(
            "bus strip {strip} out of range 0..={} (0-based)",
            MAX_BUS - 1
        );
    }
    Ok(strip + 1)
}

// NOTE (T7): per-send on/off (`/ch/NN/mix/MM/on`) is silent on XR18
// FW 1.20 while `/ch/NN/mix/MM/level` replies — per-send mute likely
// does not exist on X-Air (a pre/post tap address was never confirmed).
// No send-mute API is exposed here, deliberately: no guessing.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_shapes() {
        assert_eq!(ch_fader(1), "/ch/01/mix/fader");
        assert_eq!(ch_fader(16), "/ch/16/mix/fader");
        assert_eq!(ch_on(3), "/ch/03/mix/on");
        assert_eq!(ch_name(5), "/ch/05/config/name");
        // Buses are single-digit: `/bus/2/…`, never `/bus/02/…` (T2).
        assert_eq!(bus_fader(1), "/bus/1/mix/fader");
        assert_eq!(bus_fader(6), "/bus/6/mix/fader");
        assert_eq!(RTN_AUX_FADER, "/rtn/aux/mix/fader");
        assert_eq!(RTN_AUX_ON, "/rtn/aux/mix/on");
        assert_eq!(RTN_AUX_NAME, "/rtn/aux/config/name");
        assert_eq!(METER_REQUEST, "/meters");
        assert_eq!(METER_SUBSCRIBE, "/meters/1");
        assert_eq!(METER_DYN_SUBSCRIBE, "/meters/6");
        assert_eq!(MAIN_FADER, "/lr/mix/fader");
        assert_eq!(MAIN_ON, "/lr/mix/on");
        assert_eq!(MAIN_NAME, "/lr/config/name");
        // 0-based mapping: strip 0 is wire ch 1; XR18 stops at ch 16 (T3).
        assert_eq!(channel(0).unwrap(), 1);
        assert_eq!(channel(15).unwrap(), 16);
        assert!(channel(16).is_err());
        // X32-kind maps keep the 32-channel bound (not HW-verified).
        assert_eq!(channel_in(31, X32KIND_CHANNELS).unwrap(), 32);
        assert!(channel_in(32, X32KIND_CHANNELS).is_err());
        assert_eq!(XAirModel::default(), XAirModel::Xr18);
        assert_eq!(XAirModel::Xr18.channels(), 16);
        assert_eq!(XAirModel::X32Kind.channels(), 32);
        assert_eq!(bus(0).unwrap(), 1);
        assert_eq!(bus(5).unwrap(), 6);
        assert!(bus(6).is_err());
    }
}
