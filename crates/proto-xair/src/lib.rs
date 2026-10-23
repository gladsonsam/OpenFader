//! Behringer X-Air OSC driver (XR12/16/18, MR18).
//!
//! Thin mapping over [`openmix_transport::OscSession`]: this crate owns ONLY the
//! X-Air address table + arg mapping. Sockets, keepalive, meter parsing, and
//! request/response plumbing all live in `openmix-transport`.
use openmix_core::{MixerDriver, MixerInfo, ParamValue, StripId};
use openmix_transport::{KeepaliveConfig, OscSession, osc};
use rosc::{OscMessage, OscType};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, broadcast};

pub mod automix;
pub mod rta;
pub mod dyn_gr;
pub mod codec;
pub mod comp;
pub mod eq;
pub mod fx;
pub mod gate;
pub mod groups;
pub mod headamp;
pub mod input;
pub mod insert;
pub mod look;
pub mod mix;
pub mod solo;
mod sync_decode;

pub mod addrs {
    //! X-Air address table. The wire is 1-based with zero-padded two-digit
    //! channel numbers (`/ch/01/…`); [`StripId`](openmix_core::StripId) is
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
    /// `/xremote` subscribers on solo changes (surface or X-Air Edit —
    /// decoded by [`parse_inbound`]). Rig-verified XR18 FW 1.20.
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
    /// confuse with X32 `/meters/6 <channel_id>` (Maillot doc), which is a
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
        /// 0-based channel-strip bound for this model.
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
}

/// XR18 strip index map (global, 36 strips). Channels 0..16 keep their
/// historic meaning; buses stay single-digit on the wire (T2); the FX-return
/// family lives at `/rtn/N` (the firmware map has no `/fxrtn` root —
/// `/fxrtn/N/*` is silent on FW 1.20); the stereo aux input lives at
/// `/rtn/aux` (NOT `/aux/*` or `/auxin/*` — the desk is silent there,
/// owner-rig verified 2026-10-20); DCA strips address fader/on directly
/// under `/dca/N` (no `/mix` segment — `/dca/N/mix/fader` is silent).
/// Every family below is live-verified on the owner rig except `/rtn/N`
/// fader/on (verified present 2026-10-18: fader + on answer) — see
/// `docs/xr18-baseline.md`:
/// | indices | family | kind  | root       |
/// |---------|--------|-------|------------|
/// | 0..16   | ch     | in    | `/ch/NN`   |
/// | 16..20  | fxret  | ret   | `/rtn/N`   |
/// | 20..26  | bus    | bus   | `/bus/N`   |
/// | 26..30  | fxsnd  | fxsnd | `/fxsend/N`|
/// | 30..31  | main   | main  | `/lr`      |
/// | 31..35  | dca    | dca   | `/dca/N`   |
/// | 35..36  | aux    | ret   | `/rtn/aux` |
pub const XAIR_TOPOLOGY: [openmix_core::StripFamily; 7] = [
    openmix_core::StripFamily { key: "ch", kind: openmix_core::StripKind::In, label: "Ch 1-16", first: 0, count: 16, name_prefix: "Ch", color_default: 2 },
    openmix_core::StripFamily { key: "fxret", kind: openmix_core::StripKind::Ret, label: "FxRet 1-4", first: 16, count: 4, name_prefix: "FxRet", color_default: 6 },
    openmix_core::StripFamily { key: "bus", kind: openmix_core::StripKind::Bus, label: "Bus 1-6", first: 20, count: 6, name_prefix: "Mix", color_default: 6 },
    openmix_core::StripFamily { key: "fxsnd", kind: openmix_core::StripKind::FxSnd, label: "FxSnd 1-4", first: 26, count: 4, name_prefix: "FxSnd", color_default: 4 },
    openmix_core::StripFamily { key: "main", kind: openmix_core::StripKind::Main, label: "Main LR", first: 30, count: 1, name_prefix: "Main", color_default: 11 },
    openmix_core::StripFamily { key: "dca", kind: openmix_core::StripKind::Dca, label: "DCA 1-4", first: 31, count: 4, name_prefix: "DCA", color_default: 0 },
    openmix_core::StripFamily { key: "aux", kind: openmix_core::StripKind::Ret, label: "Aux", first: 35, count: 1, name_prefix: "Aux", color_default: 2 },
];

/// Total addressable strips on the XR18 map (16 + 4 + 6 + 4 + 1 + 4 + 1).
pub const XAIR_STRIPS: usize = 36;

/// Loopback-tested param-plane capabilities: the ids this driver implements
/// with `get_param`/`set_param` (each covered by a strict fake-desk loopback
/// test below, and every address confirmed against the owner XR18 FW 1.20 —
/// see `docs/xr18-baseline.md`). Everything else in
/// [`openmix_core::params::CATALOGUE`] is reported in `caps.unsupported`
/// so the UI hides it. Honest exclusions: `delay_on`/`delay_ms`
/// (X32-only), `hpf_slope` (X32-only), `source` (`insrc` live-silent on FW
/// 1.20), `icon` (no such leaf in the firmware map), `link` (stereo link is
/// the global `/config/chlink/1-2…`, not a per-channel leaf — silent),
/// `send1..10_on` + all `auxN_mute`/`fxN_mute` (the firmware send block has
/// no on/off leaf and `/ch/NN/mix/MM/on` is live-silent — baseline T7),
/// `send11..16_*` (only 6+4 send slots), per-channel `eq5_*` (channels and
/// aux returns carry 4 bands; `eq5/6_*` ride buses + main only),
/// `solo_dim`/`solo_mute`/`solo_level`/`solo_source` (global
/// `/config/solo/*`, driven via driver methods, not strip params) and
/// `fx_insert` (slot-addressed `/fx/N/insert`, likewise a driver method).
pub const XAIR_CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "gain",
    "phantom",
    "usbreturn",
    "trim",
    "polarity",
    "pan",
    "lr_assign",
    "dca1_assign",
    "dca2_assign",
    "dca3_assign",
    "dca4_assign",
    "hpf_on",
    "hpf_freq",
    "automix_on",
    "automix_weight",
    "insert_on",
    "insert_sel",
    "color",
    "gate_on",
    "gate_thr",
    "gate_range",
    "gate_att",
    "gate_hold",
    "gate_rel",
    "gate_mode",
    "gate_key_src",
    "gate_filter_on",
    "gate_filter_type",
    "gate_filter_freq",
    "comp_on",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_makeup",
    "comp_att",
    "comp_hold",
    "comp_rel",
    "comp_mode",
    "comp_det",
    "comp_env",
    "comp_mix",
    "comp_auto",
    "comp_key_src",
    "comp_filter_on",
    "comp_filter_type",
    "comp_filter_freq",
    "eq_on",
    "eq1_gain",
    "eq1_freq",
    "eq1_q",
    "eq1_type",
    "eq2_gain",
    "eq2_freq",
    "eq2_q",
    "eq2_type",
    "eq3_gain",
    "eq3_freq",
    "eq3_q",
    "eq3_type",
    "eq4_gain",
    "eq4_freq",
    "eq4_q",
    "eq4_type",
    "eq5_gain",
    "eq5_freq",
    "eq5_q",
    "eq5_type",
    "eq6_gain",
    "eq6_freq",
    "eq6_q",
    "eq6_type",
    "aux1_level",
    "aux2_level",
    "aux3_level",
    "aux4_level",
    "aux5_level",
    "aux6_level",
    "fx1_level",
    "fx2_level",
    "fx3_level",
    "fx4_level",
    "send1_level",
    "send1_tap",
    "send1_pan",
    "send1_grpon",
    "send2_level",
    "send2_tap",
    "send2_grpon",
    "send3_level",
    "send3_tap",
    "send3_pan",
    "send3_grpon",
    "send4_level",
    "send4_tap",
    "send4_grpon",
    "send5_level",
    "send5_tap",
    "send5_pan",
    "send5_grpon",
    "send6_level",
    "send6_tap",
    "send6_grpon",
    "send7_level",
    "send7_tap",
    "send7_grpon",
    "send8_level",
    "send8_tap",
    "send8_grpon",
    "send9_level",
    "send9_tap",
    "send9_grpon",
    "send10_level",
    "send10_tap",
    "send10_grpon",
];

/// Parse an `eq{b}_{leaf}` param id into its 1-based band and leaf.
/// Bands 1..=6 parse here; the per-kind band cap (4 on channels/returns,
/// 6 on buses/main) is enforced by [`check_eq_band`] after the kind is
/// known, so out-of-range bands on a desk family error instead of
/// hitting a silent address.
fn eq_leaf(param: &str) -> anyhow::Result<(u8, &str)> {
    let rest = param
        .strip_prefix("eq")
        .ok_or_else(|| anyhow::anyhow!("xair: not an eq param {param:?}"))?;
    let (band, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let band: u8 = band
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    eq::check_band_wide(band)?;
    Ok((band, leaf))
}

/// Enforce the firmware band count for one strip kind: channels and aux
/// returns carry 4 bands, buses and main LR 6 (`1..n: 4 4 6 6`).
fn check_eq_band(kind: openmix_core::StripKind, band: u8) -> anyhow::Result<()> {
    use openmix_core::StripKind as K;
    let max = match kind {
        K::In | K::Ret => 4,
        _ => 6,
    };
    if band < 1 || band > max {
        anyhow::bail!("xair: eq band {band} out of range 1..={max} for {kind:?} strips");
    }
    Ok(())
}

/// Parse an `aux{n}_level` param id into its 1-based send number.
fn aux_send(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("aux")
        .ok_or_else(|| anyhow::anyhow!("xair: not an aux param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "level" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::AUX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n)
}

/// Parse a generic `send{N}_{leaf}` param id into its send number (1..=10)
/// and leaf (`level|tap|pan|grpon|on`).
fn generic_send(param: &str) -> anyhow::Result<(u8, &str)> {
    let rest = param
        .strip_prefix("send")
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::AUX_SENDS + mix::FX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    if !matches!(leaf, "level" | "tap" | "pan" | "grpon" | "on") {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok((n, leaf))
}

/// Parse a `dcaN_assign` param id into its 0-based DCA group index.
fn dca_assign(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("dca")
        .ok_or_else(|| anyhow::anyhow!("xair: not a dca param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "assign" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=groups::DCA_GROUPS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n - 1)
}

/// Global strip index to (family kind, OSC root). Channels keep the
/// `/ch/NN` root (T3); FX returns live at `/rtn/N` (NOT `/fxrtn/N` — the
/// desk is silent there); the stereo aux input lives at `/rtn/aux` (NOT
/// `/aux/*` or `/auxin/*` — the desk is silent there, owner-rig verified
/// 2026-10-20); buses are single-digit `/bus/N` (T2); DCA strips
/// are `/dca/N` with fader/on directly underneath (no `/mix` segment).
/// Every root below is live-verified on the owner rig (FW 1.20).
fn strip_root(strip: u16) -> anyhow::Result<(openmix_core::StripKind, String)> {    use openmix_core::StripKind as K;
    let s = strip as usize;
    if s < 16 {
        return Ok((K::In, format!("/ch/{:02}", s + 1)));
    }
    if (16..20).contains(&s) {
        return Ok((K::Ret, format!("/rtn/{}", s - 16 + 1)));
    }
    if (20..26).contains(&s) {
        return Ok((K::Bus, format!("/bus/{}", s - 20 + 1)));
    }
    if (26..30).contains(&s) {
        return Ok((K::FxSnd, format!("/fxsend/{}", s - 26 + 1)));
    }
    if s == 30 {
        return Ok((K::Main, "/lr".to_string()));
    }
    if (31..35).contains(&s) {
        return Ok((K::Dca, format!("/dca/{}", s - 31 + 1)));
    }
    if s == 35 {
        return Ok((K::Ret, "/rtn/aux".to_string()));
    }
    anyhow::bail!("channel strip {strip} out of range 0..=35 (0-based)")
}

/// Scope-check one catalogue id against a strip kind: out-of-scope is a
/// NON-drop-class error (the engine pre-filters its poll set by scopes, so
/// this only fires on direct misuse and must never shrink `caps`).
fn check_scope(param: &str, kind: openmix_core::StripKind) -> anyhow::Result<()> {
    let def = openmix_core::params::find(param)
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !openmix_core::params::supports_kind(def, kind)
        || (param == "comp_key_src" && kind == openmix_core::StripKind::Main)
    {
        anyhow::bail!("xair: {param} out of scope for {kind:?} strips");
    }
    Ok(())
}

/// Mix fader address for one strip root (`/lr` keeps its dedicated leaf;
/// DCA strips address `/dca/N/fader` directly — there is no `/mix`
/// segment there and the desk stays silent on it).
fn strip_mix_fader(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_FADER.to_string();
    }
    if root.starts_with("/dca/") {
        return format!("{root}/fader");
    }
    format!("{root}/mix/fader")
}

/// Mix on/mute address for one strip root (DCA: `/dca/N/on` directly).
fn strip_mix_on(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_ON.to_string();
    }
    if root.starts_with("/dca/") {
        return format!("{root}/on");
    }
    format!("{root}/mix/on")
}

/// Config name address for one strip root.
fn strip_config_name(root: &str) -> String {
    if root == "/lr" {
        return addrs::MAIN_NAME.to_string();
    }
    format!("{root}/config/name")
}

/// True when `addr` carries an explicit `:port` (or parses as a full
/// `SocketAddr`): the connect sweep leaves those alone and dials as-is.
/// Mirrors [`openmix_transport::session::resolve_peer`]'s host/port split
/// (bare IPv6 without brackets is treated as a bare host, same as there).
fn has_explicit_port(addr: &str) -> bool {
    if addr.parse::<SocketAddr>().is_ok() {
        return true;
    }
    let host = addr.trim().trim_matches(['[', ']']);
    if let Some((_, p)) = host.rsplit_once(':') {
        return p.parse::<u16>().is_ok();
    }
    false
}

/// One connect-time `/info` ping to `host:port`: any datagram back counts
/// as alive. Bounded by [`addrs::CONNECT_PROBE_TIMEOUT`]; never used on
/// the steady-state path.
async fn probe_port(host: &str, port: u16) -> bool {
    use openmix_transport::session::resolve_peer;
    let peer = match resolve_peer(host, port) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let sock = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(_) => return false,
    };
    if sock.connect(peer).await.is_err() {
        return false;
    }
    let bytes = match osc::query_message(addrs::INFO) {
        Ok(b) => b,
        Err(_) => return false,
    };
    if sock.send(&bytes).await.is_err() {
        return false;
    }
    let mut buf = vec![0u8; 65535];
    matches!(
        tokio::time::timeout(addrs::CONNECT_PROBE_TIMEOUT, sock.recv(&mut buf)).await,
        Ok(Ok(_))
    )
}

/// Reverse of [`strip_root`]: an OSC root back to its global strip index./// Returns `None` for roots outside the XR18 map. The stereo aux input
/// (`/rtn/aux`) is strip 35; numeric `/rtn/N` covers the four FX returns.
fn strip_of_root(root: &str) -> Option<u16> {
    if root == "/rtn/aux" {
        return Some(35);
    }
    let path = root.strip_prefix('/')?;
    let (fam, n) = path.split_once('/')?;
    let n: usize = n.parse().ok()?;
    match fam {
        "ch" if (1..=16).contains(&n) => Some((n - 1) as u16),
        "rtn" if (1..=4).contains(&n) => Some((15 + n) as u16),
        "bus" if (1..=6).contains(&n) => Some((19 + n) as u16),
        "fxsend" if (1..=4).contains(&n) => Some((25 + n) as u16),
        "dca" if (1..=4).contains(&n) => Some((30 + n) as u16),
        _ => None,
    }
}

/// Strip index of the `/lr` root (the main-LR family, one strip).
const LR_STRIP: u16 = 30;

/// Canonical catalogue id per send slot (bus slots 1..=6 ride `auxN_level`,
/// FX slots 7..=10 ride `fxN_level` — the same ids `get_param` decodes).
const AUX_LEVEL_IDS: [&str; 6] = [
    "aux1_level",
    "aux2_level",
    "aux3_level",
    "aux4_level",
    "aux5_level",
    "aux6_level",
];
const FX_LEVEL_IDS: [&str; 4] = ["fx1_level", "fx2_level", "fx3_level", "fx4_level"];
/// Canonical catalogue ids per DCA-assign bit (`dca1..4_assign`, the same
/// ids the `dcaN_assign` get_param arms decode — for `parse_inbound`).
const DCA_ASSIGN_IDS: [&str; 4] = [
    "dca1_assign",
    "dca2_assign",
    "dca3_assign",
    "dca4_assign",
];

/// Canonical catalogue ids per EQ band leaf (bands 1..=6 index 0..=5) —
/// the same ids `get_param`'s eq arms decode, for `parse_inbound`.
const EQ_GAIN_IDS: [&str; 6] = [
    "eq1_gain",
    "eq2_gain",
    "eq3_gain",
    "eq4_gain",
    "eq5_gain",
    "eq6_gain",
];
const EQ_FREQ_IDS: [&str; 6] = [
    "eq1_freq",
    "eq2_freq",
    "eq3_freq",
    "eq4_freq",
    "eq5_freq",
    "eq6_freq",
];
const EQ_Q_IDS: [&str; 6] = ["eq1_q", "eq2_q", "eq3_q", "eq4_q", "eq5_q", "eq6_q"];
const EQ_TYPE_IDS: [&str; 6] = [
    "eq1_type",
    "eq2_type",
    "eq3_type",
    "eq4_type",
    "eq5_type",
    "eq6_type",
];

/// Decode advertised XR18 controls in engineering units, including query
/// replies and captured typed containers. Dynamics/input/send leaves share
/// their query codecs. One send level fans out to canonical and legacy IDs.
/// Private group IDs are consumed by the engine's typed library fanout;
/// FX is stateful (its type selects each parameter codec) and is decoded by
/// the per-session receive actor. Unknown/unmapped paths remain absent.
pub fn parse_inbound(msg: &OscMessage) -> Vec<(u16, &'static str, ParamValue)> {
    let shared = sync_decode::inbound(msg);
    if !shared.is_empty() {
        let mut shared = shared;
        if let Some((strip, id, value)) = shared.first().cloned() {
            if let Ok((slot, "level")) = generic_send(id) {
                let alias = if slot <= 6 { AUX_LEVEL_IDS[slot as usize - 1] } else { FX_LEVEL_IDS[slot as usize - 7] };
                // Keep the legacy update first for existing consumers.
                shared.insert(0, (strip, alias, value));
            }
        }
        return shared;
    }
    let expanded = sync_decode::expand(msg);
    if !expanded.is_empty() { return expanded.iter().flat_map(parse_inbound).collect(); }
    let mut updates = parse_inbound_go(msg).unwrap_or_default();
    // One wire level has both canonical and compatibility IDs.
    if let Some((strip, id, value)) = updates.first().cloned() {
        let slot = AUX_LEVEL_IDS.iter().position(|p| *p == id).map(|i| i + 1)
            .or_else(|| FX_LEVEL_IDS.iter().position(|p| *p == id).map(|i| i + 7));
        if let Some(slot) = slot {
            const IDS: [&str; 10] = ["send1_level","send2_level","send3_level","send4_level","send5_level","send6_level","send7_level","send8_level","send9_level","send10_level"];
            updates.push((strip, IDS[slot - 1], value));
        }
    }
    updates
}

fn parse_inbound_go(msg: &OscMessage) -> Option<Vec<(u16, &'static str, ParamValue)>> {
    let addr = msg.addr.strip_prefix('/')?;
    if let Some(group) = addr.strip_prefix("config/mute/").and_then(|s| s.parse::<u16>().ok()).filter(|n| (1..=4).contains(n)) {
        return Some(vec![(group - 1, "_mute_group", ParamValue::Bool(osc::is_on(msg).ok()?))]);
    }
    if let Some(root) = addr.strip_suffix("/grp/mute") {
        let strip = strip_of_root(&format!("/{root}"))?;
        if !matches!(strip_root(strip).ok()?.0, openmix_core::StripKind::In | openmix_core::StripKind::Ret | openmix_core::StripKind::Bus | openmix_core::StripKind::FxSnd) { return None; }
        let mask = groups::mask_from_msg(msg).ok()?;
        const IDS: [&str; 4] = ["_mg1", "_mg2", "_mg3", "_mg4"];
        return Some(IDS.iter().enumerate().map(|(i, id)| (strip, *id, ParamValue::Bool(mask & (1 << i) != 0))).collect());
    }
    if addr == "-stat/solosw" && msg.args.len() >= addrs::XR18_CHANNELS as usize {
        return Some(msg.args.iter().take(16).enumerate().filter_map(|(i, arg)| match arg {
            OscType::Int(v) => Some((i as u16, "solo", ParamValue::Bool(*v != 0))), _ => None,
        }).collect());
    }

    // Solo switches (`/-stat/solosw/NN`, 1-based channel bank): the desk
    // broadcasts these on solo changes (surface, X-Air Edit, or our own
    // `set_solo` — all drive the same bank). Decoded to the `solo` poll
    // id so the engine can fan them out as `Solo` events; out-of-range
    // leaves stay silent (XR18: channels 1..=16 only).
    if let Some(rest) = addr.strip_prefix("-stat/solosw/") {
        let Ok(n): Result<u16, _> = rest.parse() else {
            return None;
        };
        if !(1..=addrs::XR18_CHANNELS).contains(&n) {
            return None;
        }
        let Ok(on) = osc::int_arg(msg) else {
            return None;
        };
        return Some(vec![(n - 1, "solo", ParamValue::Bool(on != 0))]);
    }
    // Headamp gain (`/headamp/NN/gain`, 1-based wire channel): the only
    // non-strip-root leaf covered here, decoded like `get_param`'s `gain`
    // arm (linear wire `0..1` <-> `-12..=+60` dB).
    if let Some(rest) = addr.strip_prefix("headamp/") {
        let (n, leaf) = rest.split_once('/')?;
        if leaf != "gain" {
            return None;
        }
        let ch: u16 = n.parse().ok()?;
        if !(1..=addrs::XR18_CHANNELS).contains(&ch) {
            return None;
        }
        let db = headamp::wire_to_db(osc::fader_value(msg).ok()?).ok()?;
        return Some(vec![(ch - 1, "gain", ParamValue::Float(db))]);
    }
    // DCA direct forms first (`/dca/N/fader`, `/dca/N/on` — no `/mix`).
    if let Some(rest) = addr.strip_prefix("dca/") {
        let (n, leaf) = rest.split_once('/')?;
        let strip = strip_of_root(&format!("/dca/{n}"))?;
        if leaf == "config/name" {
            return Some(vec![(strip, "name", ParamValue::Text(osc::string_args(msg).into_iter().next()?))]);
        }
        if leaf == "config/color" {
            let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
            return Some(vec![(strip, "color", ParamValue::Float(i as f32))]);
        }
        return match leaf {
            "fader" => Some(vec![(strip, "fader", ParamValue::Float(osc::fader_value(msg).ok()?))]),
            "on" => Some(vec![(strip, "mute", ParamValue::Bool(osc::mute_is_muted(msg).ok()?))]),
            _ => None,
        };
    }
    // Main LR (`/lr/mix/fader`, `/lr/mix/on`, `/lr/config/name`).
    if addr == "lr/mix/fader" {
        return Some(vec![(LR_STRIP, "fader", ParamValue::Float(osc::fader_value(msg).ok()?))]);
    }
    if addr == "lr/mix/on" {
        return Some(vec![(LR_STRIP, "mute", ParamValue::Bool(osc::mute_is_muted(msg).ok()?))]);
    }
    if addr == "lr/config/name" {
        return Some(vec![(LR_STRIP, "name", ParamValue::Text(osc::string_args(msg).into_iter().next()?))]);
    }
    if addr == "lr/config/color" {
        let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
        return Some(vec![(LR_STRIP, "color", ParamValue::Float(i as f32))]);
    }
    // Family roots: `/ch/NN`, `/rtn/N`, `/bus/N`, `/fxsend/N`, with the
    // mix block (`…/mix/fader|on|pan`), a send slot (`…/mix/MM/level`) or a
    // config name/color (`…/config/name|color`).
    let (head, leaf) = addr.rsplit_once('/')?;
    // Send level (`/ch/NN/mix/MM/level`, slots 1..=10): channel roots only
    // (the `get_param` send arms are channel-scoped), decoded like the
    // `aux/fx/send…_level` arms (X-Air fader law, dB).
    if leaf == "level" {
        let (mix_root, slot) = head.rsplit_once('/')?;
        let (root, block) = mix_root.rsplit_once('/')?;
        if block != "mix" {
            return None;
        }
        let ch: u16 = root.strip_prefix("ch/")?.parse().ok()?;
        if !(1..=addrs::XR18_CHANNELS).contains(&ch) {
            return None;
        }
        let slot: usize = slot.parse().ok()?;
        let id = match slot {
            1..=6 => AUX_LEVEL_IDS[slot - 1],
            7..=10 => FX_LEVEL_IDS[slot - 7],
            _ => return None,
        };
        let db = mix::wire_to_send_db(mix::send_level_value(msg).ok()?).ok()?;
        return Some(vec![(ch - 1, id, ParamValue::Float(db))]);
    }
    // Input HPF leaves broadcast independently of EQ bypass. Decode with
    // the same input codecs as get_param; do not wait for a cached poll.
    if matches!(leaf, "hpon" | "hpf") {
        let (root, block) = head.rsplit_once('/')?;
        if block != "preamp" { return None; }
        let ch: u16 = root.strip_prefix("ch/")?.parse().ok()?;
        if !(1..=addrs::XR18_CHANNELS).contains(&ch) { return None; }
        let (id, value) = if leaf == "hpon" {
            ("hpf_on", ParamValue::Bool(osc::is_on(msg).ok()?))
        } else {
            ("hpf_freq", ParamValue::Float(input::hpf_wire_to_hz(osc::fader_value(msg).ok()?).ok()?))
        };
        return Some(vec![(ch - 1, id, value)]);
    }
    // EQ on/off (`{root}/eq/on`): channels, FX returns, buses and main LR
    // — decoded like the `get_param` `eq_on` arm. FX-send/DCA roots carry
    // no EQ and stay silent.
    if let Some(eq_root) = addr.strip_suffix("/eq/on") {
        if eq_root != "lr"
            && !eq_root.starts_with("ch/")
            && !eq_root.starts_with("rtn/")
            && !eq_root.starts_with("bus/")
        {
            return None;
        }
        let strip = if eq_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{eq_root}"))?
        };
        return Some(vec![(strip, "eq_on", ParamValue::Bool(osc::is_on(msg).ok()?))]);
    }
    // Insert bypass/select (`{root}/insert/on|sel`): channels, buses and
    // main LR only — returns, FX sends and masters carry no insert leaf
    // (the desk is silent there). Decoded like the `get_param`
    // `insert_on`/`insert_sel` arms (raw `0..=8` sel, no slot mapping).
    if let Some(ins_root) = addr.strip_suffix("/insert/on") {
        if ins_root != "lr" && !ins_root.starts_with("ch/") && !ins_root.starts_with("bus/") {
            return None;
        }
        let strip = if ins_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{ins_root}"))?
        };
        return Some(vec![(strip, "insert_on", ParamValue::Bool(osc::is_on(msg).ok()?))]);
    }
    if let Some(ins_root) = addr.strip_suffix("/insert/sel") {
        if ins_root != "lr" && !ins_root.starts_with("ch/") && !ins_root.starts_with("bus/") {
            return None;
        }
        let strip = if ins_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{ins_root}"))?
        };
        let sel = insert::sel_from_msg(msg).ok()? as f32;
        return Some(vec![(strip, "insert_sel", ParamValue::Float(sel))]);
    }
    // EQ band leaves (`{root}/eq/{band}/{g|f|q|type}`): decoded like the
    // matching `get_param` eq arms (same codecs, engineering units).
    // Channels and FX returns carry 4 bands, buses and main LR 6
    // (mirroring `check_eq_band`); FX-send/DCA roots carry no EQ and stay
    // silent, as do out-of-range bands on a 4-band strip.
    if matches!(leaf, "g" | "f" | "q" | "type") {
        let (eq_parent, band_s) = head.rsplit_once('/')?;
        let eq_root = eq_parent.strip_suffix("/eq")?;
        let max_band = if eq_root == "lr" {
            6
        } else if eq_root.starts_with("ch/") || eq_root.starts_with("rtn/") {
            4
        } else if eq_root.starts_with("bus/") {
            6
        } else {
            return None;
        };
        let band: u8 = band_s.parse().ok()?;
        if !(1..=max_band).contains(&band) {
            return None;
        }
        let strip = if eq_root == "lr" {
            LR_STRIP
        } else {
            strip_of_root(&format!("/{eq_root}"))?
        };
        let idx = band as usize - 1;
        let id = match leaf {
            "g" => EQ_GAIN_IDS[idx],
            "f" => EQ_FREQ_IDS[idx],
            "q" => EQ_Q_IDS[idx],
            _ => EQ_TYPE_IDS[idx],
        };
        let value = match leaf {
            "g" => ParamValue::Float(eq::wire_to_gain_db(osc::fader_value(msg).ok()?).ok()?),
            "f" => ParamValue::Float(eq::wire_to_freq_hz(osc::fader_value(msg).ok()?).ok()?),
            "q" => ParamValue::Float(eq::wire_to_q(osc::fader_value(msg).ok()?).ok()?),
            _ => ParamValue::Float(eq::eqtype_from_msg(msg).ok()? as f32),
        };
        return Some(vec![(strip, id, value)]);
    }
    // Group assignment (`{root}/grp/dca`, int bitmask): one update per DCA
    // catalogue id (same decode as the `dcaN_assign` get_param arms).
    // `grp/mute` carries no catalogue ids (the engine tracks mute-group
    // membership through `MuteGroupAssign` events) and stays out — see the
    // fn docs. DCA/main roots carry no `grp` leaf on the desk; a message
    // from anywhere else stays silent.
    if let Some(grp_root) = addr.strip_suffix("/grp/dca") {
        let strip = strip_of_root(&format!("/{grp_root}"))?;
        let (kind, _) = strip_root(strip).ok()?;
        if !matches!(
            kind,
            openmix_core::StripKind::In
                | openmix_core::StripKind::Ret
                | openmix_core::StripKind::Bus
                | openmix_core::StripKind::FxSnd
        ) {
            return None;
        }
        let mask = groups::mask_from_msg(msg).ok()?;
        return Some(
            DCA_ASSIGN_IDS
                .iter()
                .enumerate()
                .map(|(i, id)| (strip, *id, ParamValue::Bool(mask & (1 << i) != 0)))
                .collect(),
        );
    }
    // Automix container (`{root}/automix`, `(int, float)` = enable +
    // weight): one update per leaf (same decode as the `automix_on` /
    // `automix_weight` get_param arms). Channel strips only — the desk is
    // silent on `automix` everywhere else. (`…/automix/weight` does not
    // match this suffix and keeps its existing handling: none — weight
    // queries ride `get_param`; desk-side weight moves arrive as container
    // broadcasts, decoded here.)
    if let Some(am_root) = addr.strip_suffix("/automix") {
        let strip = strip_of_root(&format!("/{am_root}"))?;
        let (kind, _) = strip_root(strip).ok()?;
        if kind != openmix_core::StripKind::In {
            return None;
        }
        let (on, w) = automix::automix_from_msg(msg).ok()?;
        return Some(vec![
            (strip, "automix_on", ParamValue::Bool(on)),
            (strip, "automix_weight", ParamValue::Float(w)),
        ]);
    }
    let (root, leaf) = match leaf {
        "fader" | "on" | "pan" | "lr" => {
            let (root, block) = head.rsplit_once('/')?;
            if block != "mix" {
                return None;
            }
            (root, leaf)
        }
        "name" | "color" => {
            let (root, block) = head.rsplit_once('/')?;
            if block != "config" {
                return None;
            }
            (root, leaf)
        }
        _ => return None,
    };
    let strip = strip_of_root(&format!("/{root}"))?;
    match leaf {
        "fader" => Some(vec![(strip, "fader", ParamValue::Float(osc::fader_value(msg).ok()?))]),
        "on" => Some(vec![(strip, "mute", ParamValue::Bool(osc::mute_is_muted(msg).ok()?))]),
        "pan" => {
            let pan = mix::wire_to_pan(osc::fader_value(msg).ok()?).ok()?;
            Some(vec![(strip, "pan", ParamValue::Float(pan))])
        }
        // LR assign (`{root}/mix/lr`, int; same decode as `get_param`).
        // Main LR never reaches here (`/lr` has no `lr` field, and
        // `strip_of_root("/lr")` is `None` anyway).
        "lr" => Some(vec![(strip, "lr_assign", ParamValue::Bool(osc::is_on(msg).ok()?))]),
        "name" => Some(vec![(strip, "name", ParamValue::Text(osc::string_args(msg).into_iter().next()?))]),
        "color" => {
            let i = look::int_from_msg(msg, look::COLOR_MIN, look::COLOR_MAX, "color").ok()?;
            Some(vec![(strip, "color", ParamValue::Float(i as f32))])
        }
        _ => None,
    }
}

/// Parse an `fx{n}_level` param id into its 1-based send number.
fn fx_send(param: &str) -> anyhow::Result<u8> {
    let rest = param
        .strip_prefix("fx")
        .ok_or_else(|| anyhow::anyhow!("xair: not an fx param {param:?}"))?;
    let (n, leaf) = rest
        .split_once('_')
        .ok_or_else(|| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if leaf != "level" {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    let n: u8 = n
        .parse()
        .map_err(|_| anyhow::anyhow!("xair: unknown param {param:?}"))?;
    if !(1..=mix::FX_SENDS).contains(&n) {
        anyhow::bail!("xair: unknown param {param:?}");
    }
    Ok(n)
}

type LevelSample = Option<(std::time::Instant, Vec<f32>)>;

/// Live XR18-class mixer on UDP 10024.
pub struct XAirDriver {
    receive_since: std::sync::Arc<std::sync::Mutex<std::time::Instant>>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>> ,
    connection_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    rta: std::sync::Mutex<Option<std::sync::Arc<rta::RtaStream>>>,
    session: Mutex<Option<std::sync::Arc<OscSession>>>,
    peer: Mutex<String>,
    model: addrs::XAirModel,
    /// Last decoded `/meters/6` ALL DYN frame (`None` until the first blob
    /// is ingested). Serves the `gate_gr` / `comp_gr` param reads — the
    /// desk has no GR query address, so there is nothing else to read.
    /// Shared with the per-connection ingest task (see `connect`), which
    /// routes every streamed DYN blob here; the `/meters/6` subscribe in
    /// `subscribe_meters_once` starts that stream.
    dyn_cache: std::sync::Arc<tokio::sync::Mutex<Option<(std::time::Instant, dyn_gr::DynMeters)>>>,
    /// Desk-originated mix-surface updates (see [`parse_inbound`]): stable
    /// across reconnects, fed by one ingest task per connection. The engine
    /// subscribes once and applies these to its param cache live.
    update_tx: broadcast::Sender<openmix_core::ParamUpdate>,
    /// Latest `/meters/1` levels in decoded blob order (40 linear levels:
    /// input bank `0..16`, aux-return post at idx 26 — rig-verified
    /// 2026-10-20; families past the channels are NOT mapped to strips —
    /// see [`XAirDriver::poll_meters`]). `None` until the first blob
    /// arrives. Filled by the cache-drain task that
    /// [`MixerDriver::subscribe_meters`] installs; read by `poll_meters`.
    meter_latest: std::sync::Arc<std::sync::Mutex<LevelSample>>,
}

impl Default for XAirDriver {
    fn default() -> Self {
        Self::with_model(addrs::XAirModel::Xr18)
    }
}

impl XAirDriver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a driver for an explicit geometry. Default ([`XAirDriver::new`])
    /// is XR18 (16 channels, hardware-verified); pass
    /// [`addrs::XAirModel::X32Kind`] only for 32-strip maps (not
    /// hardware-verified on X-Air).
    pub fn with_model(model: addrs::XAirModel) -> Self {
        let (update_tx, _) = broadcast::channel(256);
        Self {
            receive_since: std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
            tasks: std::sync::Mutex::new(Vec::new()),
            connection_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            rta: std::sync::Mutex::new(None),
            session: Mutex::new(None),
            peer: Mutex::new(String::new()),
            model,
            dyn_cache: std::sync::Arc::new(Mutex::new(None)),
            update_tx,
            meter_latest: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// 0-based strip index to 1-based wire channel, validated against this
    /// driver's model bound (XR18: strips 0..=15; `/ch/17+` is silent on
    /// hardware, T3).
    fn check_channel(&self, strip: u16) -> anyhow::Result<u16> {
        addrs::channel_in(strip, self.model.channels())
    }

    /// Strip root for group-assignment leaves (`grp/dca`, `grp/mute`):
    /// channels, FX returns, buses and FX-send masters answer (rig-verified
    /// XR18 FW 1.20); main LR and DCA strips are live-silent there and bail
    /// locally so callers never burn a 2 s query timeout on hardware.
    fn assign_root(&self, strip: u16, what: &str) -> anyhow::Result<(openmix_core::StripKind, String)> {
        let (kind, root) = strip_root(strip)?;
        match kind {
            openmix_core::StripKind::In
            | openmix_core::StripKind::Ret
            | openmix_core::StripKind::Bus
            | openmix_core::StripKind::FxSnd => Ok((kind, root)),
            _ => anyhow::bail!("xair: {what} not supported on {kind:?} strips (no grp leaf)"),
        }
    }

    /// Connect-time port sweep for a bare host (no `:port`): ping `/info`
    /// on 10024, then the 10023 fallback, each bounded by
    /// [`addrs::CONNECT_PROBE_TIMEOUT`](crate::addrs::CONNECT_PROBE_TIMEOUT);
    /// dial the first responder, preferring 10024. If neither answers,
    /// dial 10024 anyway — venue wifi may drop the ping but pass the
    /// session (same best-effort posture as the old unconditional dial).
    async fn sweep_peer(host: &str) -> anyhow::Result<SocketAddr> {
        use openmix_transport::session::resolve_peer;
        if probe_port(host, addrs::PORT).await {
            return resolve_peer(host, addrs::PORT);
        }
        if probe_port(host, addrs::FALLBACK_PORT).await {
            return resolve_peer(host, addrs::FALLBACK_PORT);
        }
        resolve_peer(host, addrs::PORT)
    }

    async fn session(&self) -> anyhow::Result<tokio::sync::MutexGuard<'_, Option<std::sync::Arc<OscSession>>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        Ok(guard)
    }

    /// Clear all solos (`/-action/clearsolo`, fire-and-forget int `1` =
    /// button press). Monitoring-side only — main LR untouched. The desk
    /// answers queries on this leaf with int `0` (nothing soloed); the
    /// trigger semantic itself is UNVERIFIED-ON-RIG (no solo was active to
    /// clear during the single live fire, 2026-10-18).
    pub async fn clear_solo(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::CLEAR_SOLO, vec![OscType::Int(1)])
            .await
    }

    /// Stereo aux return strip (`/rtn/aux`, hardware-verified present,
    /// baseline §2). Kept as explicit methods — not a `StripId` — so the
    /// 0..16 channel range maps 1:1 to wire `/ch/01..16` with no phantom
    /// strips (T3).
    pub async fn get_aux_fader(&self) -> anyhow::Result<f32> {        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(addrs::RTN_AUX_FADER)
            .await?;
        osc::fader_value(&reply)
    }

    pub async fn set_aux_fader(&self, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::RTN_AUX_FADER, vec![osc::fader_arg(value)])
            .await
    }

    pub async fn get_aux_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::RTN_AUX_ON).await?;
        osc::mute_is_muted(&reply)
    }

    pub async fn set_aux_mute(&self, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::RTN_AUX_ON, vec![osc::on_arg(muted)])
            .await
    }

    pub async fn get_aux_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::RTN_AUX_NAME).await?;
        Ok(osc::string_args(&reply).first().cloned().unwrap_or_default())
    }

    pub async fn set_aux_name(&self, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(
                addrs::RTN_AUX_NAME,
                vec![OscType::String(name.to_string())],
            )
            .await
    }

    /// Single-query FX param read when the loaded effect index is already
    /// known (connect burst / poller fast path): skips the per-param
    /// `/fx/N/type` re-query in the trait `get_fx_param`. `None`
    /// (unknown/unreadable type) falls back to raw decode, exactly like
    /// the two-query path.
    pub async fn get_fx_param_with_type(
        &self,
        slot: u8,
        param: u8,
        effect: Option<u8>,
    ) -> anyhow::Result<f32> {
        let addr = fx::fx_param(slot, param)?;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        let def = effect.and_then(|t| fx::fx_param_def(t, param + 1));
        let reply = session.query(&addr).await?;
        fx::parse_fx_param(&reply.args, def.as_ref())
    }

    /// Live solo bank in ONE query (`/-stat/solosw` int args, 1 per
    /// channel): index `i` set = channel `i + 1` soloed. Backs the engine
    /// snapshot fan-out so a connect burst costs 1 query, not 16.
    pub async fn solo_bank(&self) -> anyhow::Result<Vec<bool>> {
        let n = self.model.channels() as usize;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(addrs::SOLO_SW)
            .await?;
        Ok((0..n)
            .map(|i| matches!(reply.args.get(i), Some(OscType::Int(v)) if *v != 0))
            .collect())
    }

    /// Map a raw `/info` reply to [`MixerInfo`]. Hardware order (baseline
    /// §1, XR18 FW 1.20 `/info` → `[V0.04, Studio, XR18, 1.20]`):
    /// index 1 = name, 2 = model, 3 = firmware (index 0 is the protocol
    /// revision). Short replies yield empty fields rather than guesses.
    fn info_from_strings(strings: Vec<String>, peer: String) -> MixerInfo {
        MixerInfo {
            name: strings.get(1).cloned().unwrap_or_default(),
            model: strings.get(2).cloned().unwrap_or_default(),
            firmware: strings.get(3).cloned().unwrap_or_default(),
            addr: peer,
        }
    }

    /// Latest decoded ALL DYN frame, if any blob has been ingested yet.
    pub async fn last_dyn(&self) -> Option<dyn_gr::DynMeters> {
        self.dyn_cache.lock().await.as_ref().filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500)).map(|(_, meters)| meters.clone())
    }

    /// Loaded effect-type index for one FX slot (`/fx/N/type`, int
    /// `0..=60`; see [`crate::fx`]). The type-select write path is
    /// rig-verified (owner XR18 FW 1.20, 2026-10-19: slot 4 swept
    /// 60→0/7/26/59/30/56→60 round-trip) — see [`Self::set_fx_type`].
    /// Out-of-range slots bail locally, never hitting the desk.
    pub async fn get_fx_type(&self, slot: u8) -> anyhow::Result<u8> {
        let addr = fx::fx_type(slot)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        fx::parse_fx_type(&reply.args)
    }

    /// Select the loaded effect for one FX slot (`/fx/N/type`, int
    /// `0..=60`): 61 firmware types, one more than the app table's 0..=59.
    /// Rig-verified on the owner XR18 (FW 1.20, 2026-10-19 — slot-4 sweep,
    /// restored bit-exact). Type swaps are state-changing on a live desk;
    /// loopback-only callers treat this like any other verified write.
    /// Out-of-range slots/types bail locally, never hitting the desk.
    pub async fn set_fx_type(&self, slot: u8, effect: u8) -> anyhow::Result<()> {
        let addr = fx::fx_type(slot)?;
        let arg = fx::fx_type_to_arg(effect)?;
        let guard = self.session().await?;
        guard.as_ref().unwrap().send(&addr, vec![arg]).await
    }

    /// FX slot insert switch (`/fx/N/insert`, int `1` = inserted).
    /// Rig-verified answering on the owner XR18 (FW 1.20, 2026-10-19:
    /// `/fx/4/insert` = 1 matches its container). Out-of-range slots bail
    /// locally, never hitting the desk.
    pub async fn get_fx_insert(&self, slot: u8) -> anyhow::Result<bool> {
        let addr = fx::fx_insert(slot)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::is_on(&reply)
    }

    /// Insert/bypass one FX slot. Same verified leaf as
    /// [`Self::get_fx_insert`]; out-of-range slots bail locally.
    pub async fn set_fx_insert(&self, slot: u8, inserted: bool) -> anyhow::Result<()> {
        let addr = fx::fx_insert(slot)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![codec::bool_arg(inserted)])
            .await
    }

    /// Solo monitoring globals (`/config/solo/*`, see [`crate::solo`]).
    /// Rig-verified on the owner XR18 (FW 1.20, 2026-10-19 — dim/mute/level
    /// flips round-trip bit-exact, source reads 3). Monitoring-side only:
    /// main LR untouched. These are globals, not strip params (out of
    /// strip caps, never polled).
    pub async fn get_solo_dim(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_DIM).await?;
        osc::is_on(&reply)
    }

    /// Solo dim switch. Same verified leaf as [`Self::get_solo_dim`].
    pub async fn set_solo_dim(&self, dim: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_DIM, vec![codec::bool_arg(dim)])
            .await
    }

    /// Solo mute switch (`/config/solo/mute`). Same verification as
    /// [`Self::get_solo_dim`].
    pub async fn get_solo_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_MUTE).await?;
        osc::is_on(&reply)
    }

    /// Solo mute switch. Same verified leaf as [`Self::get_solo_mute`].
    pub async fn set_solo_mute(&self, mute: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_MUTE, vec![codec::bool_arg(mute)])
            .await
    }

    /// Solo headphone level (`/config/solo/level`, wire float `0..1`).
    /// Same verification as [`Self::get_solo_dim`].
    pub async fn get_solo_level(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_LEVEL).await?;
        solo::wire_to_level(osc::fader_value(&reply)?)
    }

    /// Solo headphone level. Validated locally (`0..=1`) before any I/O.
    pub async fn set_solo_level(&self, level: f32) -> anyhow::Result<()> {
        let wire = solo::level_to_wire(level)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(solo::SOLO_LEVEL, vec![OscType::Float(wire)])
            .await
    }

    /// Solo headphone source (`/config/solo/source`, int `0..=14`).
    /// Same verification as [`Self::get_solo_dim`].
    pub async fn get_solo_source(&self) -> anyhow::Result<u8> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(solo::SOLO_SOURCE).await?;
        solo::source_from_msg(&reply)
    }

    /// Solo headphone source. Validated locally (`0..=14`) before any I/O.
    pub async fn set_solo_source(&self, source: u8) -> anyhow::Result<()> {
        let arg = solo::source_to_arg(source)?;
        let guard = self.session().await?;
        guard.as_ref().unwrap().send(solo::SOLO_SOURCE, vec![arg]).await
    }

    /// Meter subscribe with an explicit renewal interval (tests pass a short
    /// interval; production uses [`addrs::METER_RENEW_INTERVAL`]).
    pub async fn subscribe_meters_with_interval(
        &self,
        interval: std::time::Duration,
    ) -> anyhow::Result<broadcast::Receiver<openmix_core::MeterFrame>> {
        let rx = self.session().await?.as_ref().unwrap().subscribe_meters();
        self.spawn_meter_renewal(interval).await?;
        // Keep the receiver/drain and watchdog installed even after an
        // initial send failure; its same-session retry can then recover.
        if let Err(error) = self.subscribe_meters_once().await { eprintln!("xair: initial stream subscribe failed: {error}; watchdog will retry"); }
        Ok(rx)
    }

    /// One-shot subscribe (no renewal loop). Shared by
    /// [`MixerDriver::subscribe_meters`] and
    /// [`Self::subscribe_meters_with_interval`].
    async fn subscribe_meters_once(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openmix_core::MeterFrame>> {
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        // Documented working form (xr-meters against real XR hardware, x-Air
        // Remote Protocol doc `/meters ,si <id> <n>`): subscribe by sending
        // the stream id AS A STRING ARG to bare `/meters` — NOT an empty
        // query to `/meters/1` (desk is silent there, baseline T4).
        //
        // INFERRED, not owner-rig verified: the subscription reportedly
        // expires after ~10 s (xr-meters re-issues every 9 s) and needs the
        // `/xremote` keepalive (every ~5 s, already running via `connect`)
        // alongside. Rig-verified 2026-10-20: every `/meters/N` blob is a
        // 4-byte LE i32 sample count + that many LE i16 dBFSx256 samples
        // (`/meters/1` = 84 B = 40 samples); transport's
        // `parse_xair_meter_blob` strips the count word and decodes
        // dB→linear. The xr-meters 4-header-byte note describes this same
        // count word, not a second header.
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("meter subscribe: {e}"))?;
        // ALL DYN (gate/comp GR) stream, same subscribe form with the
        // `/meters/6` id (Behringer X AIR OSC doc fw 1.11 — NOT owner-rig
        // verified). A send failure here is local socket I/O (UDP sends
        // cannot fail remotely), so it propagates like the levels
        // subscribe above. The DYN blobs the desk then streams decode via
        // `ingest_dyn_message` into the `gate_gr` / `comp_gr` param reads;
        // the shared hub only carries levels, so nothing else changes here.
        session
            .send(
                addrs::METER_REQUEST,
                vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())],
            )
            .await
            .map_err(|e| anyhow::anyhow!("dyn meter subscribe: {e}"))?;
        Ok(session.subscribe_meters())
    }

    /// Spawn the meter renewal loop: re-send `/xremote` + both `/meters`
    /// subscribes every `interval` so the desk (suspected ~10 s expiry,
    /// INFERRED) keeps streaming. Fire-and-forget detached task over a
    /// cloned [`std::sync::Arc`] socket — no driver/session lock held, so
    /// renewals never block queries and tolerate resubscribes.
    async fn spawn_meter_renewal(&self, interval: std::time::Duration) -> anyhow::Result<()> {
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap().clone();
        let sock = std::sync::Arc::clone(session.socket());
        let dyn_cache = self.dyn_cache.clone();
        let generation = self.connection_generation.clone();
        let connection = generation.load(std::sync::atomic::Ordering::Acquire);
        drop(guard);
        let xremote = osc::query_message(addrs::XREMOTE)?;
        let sub_levels = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())],
        )?;
        let sub_dyn = osc::encode_message(
            addrs::METER_REQUEST,
            vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())],
        )?;
        let task = tokio::spawn(async move {
            let mut ticker =
                openmix_transport::session::keepalive_ticker(interval.min(std::time::Duration::from_secs(1)));
            let mut renewed = std::time::Instant::now();
            // `keepalive_ticker` fires immediately; consume it so the first
            // renewal lands one full `interval` after the initial subscribe.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if generation.load(std::sync::atomic::Ordering::Acquire) != connection { break; }
                let stale_levels = session.latest_meter_sample().is_none_or(|(at, _)| at.elapsed() > std::time::Duration::from_millis(500));
                let stale_dyn = dyn_cache.lock().await.as_ref().is_none_or(|(at, _)| at.elapsed() > std::time::Duration::from_millis(500));
                if renewed.elapsed() < interval && !stale_levels && !stale_dyn { continue; }
                renewed = std::time::Instant::now();
                for bytes in [&xremote, &sub_levels, &sub_dyn] {
                    if let Err(error) = sock.send(bytes).await { eprintln!("xair: subscription renewal failed: {error}"); }
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        Ok(())
    }

    /// Decode one raw `/meters/6` OSC message into [`DynMeters`]
    /// (shared by [`Self::ingest_dyn_message`] and the per-connection
    /// ingest task in `connect`).
    fn decode_dyn_message(msg: &OscMessage) -> anyhow::Result<dyn_gr::DynMeters> {
        if msg.addr != addrs::METER_DYN_SUBSCRIBE {
            anyhow::bail!("ingest_dyn_message: unsupported meter {}", msg.addr);
        }
        let blob = msg.args.iter().find_map(|a| match a {
            OscType::Blob(b) => Some(b.as_slice()),
            _ => None,
        });
        let Some(blob) = blob else {
            anyhow::bail!("ingest_dyn_message: /meters/6 without blob payload");
        };
        dyn_gr::parse_dyn_blob(blob)
    }

    /// Ingest one raw `/meters/6` OSC message into the GR cache (the
    /// documented ALL DYN shape — see [`crate::dyn_gr`]). Any other address
    /// (or a non-blob payload) bails: no guessing at unknown meter layouts.
    ///
    /// Ingestion also runs live: the per-connection task in `connect`
    /// routes every streamed DYN blob through [`Self::decode_dyn_message`]
    /// into the same cache, so the `gate_gr` / `comp_gr` param reads serve
    /// live reduction once the desk streams (see `subscribe_meters_once`).
    pub async fn ingest_dyn_message(&self, msg: &OscMessage) -> anyhow::Result<()> {
        let dyn_meters = Self::decode_dyn_message(msg)?;
        *self.dyn_cache.lock().await = Some((std::time::Instant::now(), dyn_meters));
        Ok(())
    }

    /// GR read for one strip from the DYN cache. Errors when no blob has
    /// been ingested yet (caller keeps the static scale) or the strip is
    /// out of range — never a fabricated zero.
    async fn cached_gr(&self, strip: StripId, gate: bool) -> anyhow::Result<f32> {
        let cache = self.dyn_cache.lock().await;
        let Some((_, dyn_meters)) = cache.as_ref().filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500)) else {
            anyhow::bail!("xair: no DYN meter data yet (desk not streaming /meters/6)");
        };
        if gate {
            dyn_meters.gate_gr(strip.0)
        } else {
            // Resolve family-local slots from topology; returns, FX sends,
            // aux and DCAs have no slot in the ALL DYN payload.
            let family = XAIR_TOPOLOGY.iter().find(|f|
                usize::from(strip.0) >= f.first && usize::from(strip.0) < f.first + f.count);
            match family.map(|f| (f.key, usize::from(strip.0) - f.first)) {
                Some(("ch", i)) => Ok(dyn_meters.comp[i]),
                Some(("bus", i)) => Ok(dyn_meters.bus[i]),
                Some(("main", 0)) => Ok(dyn_meters.lr),
                _ => anyhow::bail!("xair: no compressor GR slot for strip {}", strip.0),
            }
        }
    }
}

impl MixerDriver for XAirDriver {
    fn meter_taps(&self) -> &'static [&'static str] {
        &["post", "gr_gate", "gr_comp"]
    }

    fn name(&self) -> &'static str {
        "xair"
    }

    /// Loopback-tested param-plane set (see [`XAIR_CAPABILITIES`]).
    fn capabilities(&self) -> &'static [&'static str] {
        XAIR_CAPABILITIES
    }

    /// XR18 strip map (16 in + 4 FX ret + 6 bus + 4 FX send + LR + 4 DCA).
    fn topology(&self) -> Vec<openmix_core::StripFamily> {
        XAIR_TOPOLOGY.to_vec()
    }

    /// Total addressable strips (36 on the XR18 map).
    fn strip_count(&self) -> usize {
        XAIR_STRIPS
    }

    /// Addressable channel strips for this model (XR18: 16; `/rtn/N`,
    /// `/rtn/aux`, buses/main are separate strips, not channels). Keeps
    /// `list`/sweeps off the phantom `/ch/17+` range the desk never
    /// answers (T3).
    fn channel_count(&self) -> usize {
        self.model.channels() as usize
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        // Connect-time port sweep: bare hosts try 10024 then the 10023
        // fallback (sub-second probes each); an explicit `:port` is dialed
        // as-is with no sweep. Neither path touches the steady-state 2 s
        // query timeout.
        let peer = if has_explicit_port(addr) {
            openmix_transport::session::resolve_peer(addr, addrs::PORT)?
        } else {
            Self::sweep_peer(addr).await?
        };
        let session = std::sync::Arc::new(OscSession::connect(peer, Some(KeepaliveConfig::xremote())).await?);
        // Authoritative discovery ping: a UDP "connect" always succeeds, so
        // without this a silent desk would link as connected and then time
        // out every query (phantom link, tap-to-connect looks dead with no
        // message). Two attempts absorb one lost datagram on flaky venue
        // wifi; a desk that answers nothing fails fast with the address in
        // the message so the UI can offer a retry. Attempts use the
        // connect-burst budget (not the steady-state 2 s query timeout).
        let mut verified = false;
        for _ in 0..2 {
            if session
                .query_timeout(
                    addrs::INFO,
                    openmix_transport::session::CONNECT_BURST_TIMEOUT,
                )
                .await
                .is_ok()
            {
                verified = true;
                break;
            }
        }
        if !verified {
            anyhow::bail!(
                "no reply from {peer} — check the desk is powered, on this network, and the IP is correct"
            );
        }
        for task in self.tasks.lock().unwrap().drain(..) { task.abort(); }
        let connection = self.connection_generation.fetch_add(1, std::sync::atomic::Ordering::AcqRel) + 1;
        *self.receive_since.lock().unwrap() = std::time::Instant::now();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        // Desk-broadcast ingest: parse every unmatched inbound message into
        // the stable update channel (one task per connection; it exits when
        // the next connect drops this session). `/meters/6` ALL DYN blobs
        // ride along here too (the transport hub only forwards levels, so
        // the session files DYN blobs as unmatched traffic): route them
        // into the GR cache backing the `gate_gr` / `comp_gr` param reads.
        let mut rx = session.subscribe_received_updates();
        let tx = self.update_tx.clone();
        let dyn_cache = self.dyn_cache.clone();
        let receive_since = self.receive_since.clone();
        let generation = self.connection_generation.clone();
        let task = tokio::spawn(async move {
            let mut fx_types = [None; 4];
            loop {
                match rx.recv().await {
                    Ok((received_at, msg)) => {
                        if received_at < *receive_since.lock().unwrap() { continue; }
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection { break; }
                        if msg.addr == addrs::METER_DYN_SUBSCRIBE {
                            if let Ok(dyn_m) = Self::decode_dyn_message(&msg) {
                                *dyn_cache.lock().await = Some((received_at, dyn_m));
                            }
                            continue;
                        }
                        if let Some(rest) = msg.addr.strip_prefix("/fx/") {
                            if let Some((slot, leaf)) = rest.split_once('/') {
                                if let Ok(slot @ 1..=4) = slot.parse::<u16>() {
                                    let type_args = if leaf == "type" { Some(msg.args.as_slice()) } else { None };
                                    if let Some(args) = type_args {
                                        if let Ok(index) = fx::parse_fx_type(args) {
                                            fx_types[slot as usize - 1] = Some(index);
                                            let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(slot), param: "_fx_type".into(), value: ParamValue::Float(index as f32) });
                                        }
                                    } else if let Some(index) = leaf.strip_prefix("par/").and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=64).contains(n)) {
                                        if let Some(effect) = fx_types[slot as usize - 1] {
                                            if let Some(def) = fx::fx_param_def(effect, index) {
                                                if let Ok(value) = fx::parse_fx_param(&msg.args, Some(&def)) {
                                                    let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(slot), param: format!("_fx_par{index:02}"), value: ParamValue::Float(value) });
                                                }
                                            }
                                        } else {
                                            let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(slot), param: "_fx_dirty".into(), value: ParamValue::Bool(true) });
                                        }
                                    } else if leaf == "par" {
                                        if let Some(effect) = fx_types[slot as usize - 1] {
                                            for (i, arg) in msg.args.iter().take(64).enumerate() {
                                                if let Some(def) = fx::fx_param_def(effect, i as u8 + 1) {
                                                    if let Ok(value) = fx::parse_fx_param(std::slice::from_ref(arg), Some(&def)) {
                                                        let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(slot), param: format!("_fx_par{:02}", i + 1), value: ParamValue::Float(value) });
                                                    }
                                                }
                                            }
                                        }
                                    } else if leaf.is_empty() {
                                        // No positional guessing for unknown FX containers.
                                    }
                                }
                            } else if let Ok(slot @ 1..=4) = rest.parse::<u16>() {
                                if let Ok(index) = fx::parse_fx_type(&msg.args) {
                                    fx_types[slot as usize - 1] = Some(index);
                                    let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(slot), param: "_fx_type".into(), value: ParamValue::Float(index as f32) });
                                }
                            }
                            continue;
                        }
                        if msg.addr == "/node" {
                            // Display-unit node text is never decoded as normalized
                            // OSC. Re-read a captured typed container at normal budget.
                            if let Some(OscType::String(text)) = msg.args.first() {
                                for line in text.lines().take(256) {
                                    let Some(path) = line.split_whitespace().next() else { continue; };
                                    let route = XAIR_TOPOLOGY.iter().flat_map(|f| f.first..f.first + f.count).find_map(|i| {
                                        let (_, root) = strip_root(i as u16).ok()?;
                                        if path == root { return Some((i as u16, None)); }
                                        path.strip_prefix(&(root + "/")).map(|suffix| (i as u16, Some(suffix.to_string())))
                                    });
                                    let (strip, value) = match route {
                                        Some((strip, Some(suffix))) => (strip, ParamValue::Text(suffix)),
                                        Some((strip, None)) => (strip, ParamValue::Bool(true)),
                                        None if path.starts_with("/fx/") => {
                                            let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(0), param: "_fx_dirty".into(), value: ParamValue::Bool(true) });
                                            continue;
                                        }
                                        None => (u16::MAX, ParamValue::Bool(true)),
                                    };
                                    let _ = tx.send(openmix_core::ParamUpdate { received_at, strip: StripId(strip), param: "_sync_dirty".into(), value });
                                }
                            }
                            continue;
                        }
                        for (strip, param, value) in parse_inbound(&msg) {
                            let _ = tx.send(openmix_core::ParamUpdate {
                                received_at,
                                strip: StripId(strip),
                                param: param.to_string(),
                                value,
                            });
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        eprintln!("xair: control receive lagged {n}; requesting bounded reconciliation");
                        let _ = tx.send(openmix_core::ParamUpdate { received_at: std::time::Instant::now(), strip: StripId(u16::MAX), param: "_sync_dirty".into(), value: ParamValue::Bool(true) });
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        let old_rta = self.rta.lock().unwrap().take();
        if let Some(old) = old_rta { let _ = old.target("", None, true).await; }
        // Same OSC session as the selector: XR18 meter provenance is session-bound.
        // The actor runs concurrently; no RTA round trip blocks hydration.
        let stream = std::sync::Arc::new(rta::RtaStream::start(session.clone(), connection));
        *self.rta.lock().unwrap() = Some(stream);
        *self.session.lock().await = Some(session);
        *self.peer.lock().await = peer.to_string();
        Ok(())
    }

    fn telemetry_timestamps(&self) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> {
        let levels = self.meter_latest.lock().unwrap().as_ref().map(|(at, _)| *at);
        let gr = self.dyn_cache.try_lock().ok().and_then(|cache| cache.as_ref().map(|(at, _)| *at));
        Some((levels, gr))
    }

    async fn refresh_streams(&self) -> anyhow::Result<()> {
        let session = self.session().await?.as_ref().unwrap().clone();
        // Drain pre-suspend socket traffic through a lightweight reply
        // barrier, then require new samples. No cached replay becomes fresh.
        let _ = session.query_timeout(addrs::INFO, openmix_transport::session::CONNECT_BURST_TIMEOUT).await;
        *self.receive_since.lock().unwrap() = std::time::Instant::now();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        session.send(addrs::XREMOTE, Vec::new()).await?;
        self.subscribe_meters_once().await?;
        Ok(())
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        self.connection_generation.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let old_rta = self.rta.lock().unwrap().take();
        if let Some(old) = old_rta { let _ = old.target("", None, true).await; }
        for task in self.tasks.lock().unwrap().drain(..) { task.abort(); }
        self.session.lock().await.take();
        self.peer.lock().await.clear();
        *self.meter_latest.lock().unwrap() = None;
        *self.dyn_cache.lock().await = None;
        Ok(())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_fader(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_fader(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_on(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::on_arg(muted)])
            .await
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_mix_on(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::mute_is_muted(&reply)
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_config_name(&root);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::String(name.to_string())])
            .await
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let (_, root) = strip_root(strip.0)?;
        let guard = self.session().await?;
        let addr = strip_config_name(&root);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        Ok(osc::string_args(&reply).first().cloned().unwrap_or_default())
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let wire = addrs::bus(bus)?;
        let guard = self.session().await?;
        let addr = addrs::bus_fader(wire);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let wire = addrs::bus(bus)?;
        let guard = self.session().await?;
        let addr = addrs::bus_fader(wire);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    /// XR18 bus masters are strips 20..25 (`/bus/N`): mute rides the same
    /// `/bus/N/mix/on` leaf as any strip (verified present on FW 1.20).
    fn bus_count(&self) -> u8 {
        addrs::MAX_BUS
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        if bus >= addrs::MAX_BUS {
            anyhow::bail!("bus {bus} out of range 0..{} (0-based)", addrs::MAX_BUS);
        }
        self.set_mute(StripId(20 + bus as u16), muted).await
    }

    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        if bus >= addrs::MAX_BUS {
            anyhow::bail!("bus {bus} out of range 0..{} (0-based)", addrs::MAX_BUS);
        }
        self.get_mute(StripId(20 + bus as u16)).await
    }

    /// Mute-group master count, rig-verified (XR18 FW 1.20, 2026-10-18):
    /// `/config/mute/1..4` answer, `/config/mute/5..8` are silent.
    fn mute_group_count(&self) -> u8 {
        addrs::MUTE_GROUPS
    }

    /// Mute-group master mute (`/config/mute/N`, int). Polarity is
    /// INFERRED from the leaf name (`1` = group muted): only `0` has ever
    /// been observed live and the master has never been toggled on hardware
    /// (toggling could mute member channels audibly), so the `1` direction
    /// is UNVERIFIED-ON-RIG. Same-value writes round-trip bit-exact.
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(muted))])
            .await
    }

    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let addr = addrs::mute_group(group)?;
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        match reply.args.first() {
            Some(OscType::Int(i)) => Ok(*i != 0),
            other => anyhow::bail!("{addr}: mute group needs an int arg, got {other:?}"),
        }
    }

    /// Channel -> mute-group membership, bit `group` of the strip's
    /// `{root}/grp/mute` bitmask (rig-verified XR18 FW 1.20, 2026-10-20:
    /// write `1` → `1`, `2` → `2`, restore `0`; out-of-range writes
    /// saturate on the desk, so the group is validated locally first).
    /// Read-modify-write: the desk has no per-group leaf. Main-LR and DCA
    /// strips carry no `grp` leaf (live-silent) and bail locally instead
    /// of timing out on hardware — this also keeps the engine attach
    /// snapshot (which scans every strip) fast.
    async fn set_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        let (kind, root) = self.assign_root(ch.0, "mute-group assign")?;
        let _ = kind;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        let reply = session.query(&groups::grp_mute(&root)).await?;
        let mask = groups::mask_from_msg(&reply)?;
        let next = groups::mask_with(mask, group, member, "mute group")?;
        session
            .send(&groups::grp_mute(&root), vec![OscType::Int(next)])
            .await
    }

    /// Channel -> mute-group membership read (same leaf; groups with no
    /// desk leaf bail locally — see [`Self::set_mute_group_assign`]).
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let (kind, root) = self.assign_root(ch.0, "mute-group assign")?;
        let _ = kind;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(&groups::grp_mute(&root))
            .await?;
        let mask = groups::mask_from_msg(&reply)?;
        groups::mask_has(mask, group, "mute group")
    }

    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::MAIN_FADER, vec![osc::fader_arg(value)])
            .await
    }

    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(addrs::MAIN_FADER)
            .await?;
        osc::fader_value(&reply)
    }

    /// Main LR master mute (`/lr/mix/on`, int `1` = unmuted / `0` = muted,
    /// same polarity as channel `/mix/on`). The leaf answers queries
    /// (rig-verified XR18 FW 1.20, 2026-10-18: reads `1`).
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(addrs::MAIN_ON, vec![osc::on_arg(muted)])
            .await
    }

    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::MAIN_ON).await?;
        osc::mute_is_muted(&reply)
    }

    /// Per-channel solo via the `/-stat/solosw` switch bank: live-verified
    /// on the owner rig (XR18 FW 1.20, 2026-10-18) — writing `1` to the
    /// 1-based `/-stat/solosw/NN` sets bank bit `NN - 1` (soloing ch16 set
    /// bits 14+15: ch15/16 are stereo-linked on that desk), and
    /// `/-action/clearsolo` clears the bank. Only channel strips
    /// (`0..16`) map (1-based index `strip + 1`); buses/mains/DCAs have no
    /// verified solo leaf and bail out (never faked).
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let ch = self.check_channel(strip.0)?;
        let addr = format!("{}/{ch:02}", addrs::SOLO_SW);
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Int(i32::from(soloed))])
            .await
    }

    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let reply = guard
            .as_ref()
            .unwrap()
            .query(addrs::SOLO_SW)
            .await?;
        let idx = ch as usize - 1;
        match reply.args.get(idx) {
            Some(OscType::Int(i)) => Ok(*i != 0),
            other => anyhow::bail!("/-stat/solosw: solo bank needs an int at index {idx}, got {other:?}"),
        }
    }

    /// Strip-ordered meter levels (one entry per strip, [`XAIR_STRIPS`]).
    /// The `/meters/1` blob (the only level stream subscribed) decodes in
    /// transport to 40 linear levels: the input bank `0..16` feeds ch 1..16
    /// (pre-mute taps — live-verified pattern 2026-10-20: hot while the
    /// channels sit muted, ch15-anchored at idx 14, aux pre pair at 16/17),
    /// and idx 26 feeds the aux-return strip 35 (`/rtn/aux` mute-gated,
    /// live-verified 2026-10-20). Strips past the channels (FX/bus/main/
    /// DCA) and the remaining blob slots have no rig-verified strip
    /// mapping, so they read honest silence here — never a neighbor's
    /// level. `None` (no blob yet) reads back full-map silence, like the
    /// core default.
    fn poll_meters(&self) -> openmix_core::MeterFrame {
        let sample = self.meter_latest.lock().unwrap();
        let Some((_, latest)) = sample.as_ref().filter(|(at, _)| at.elapsed() <= std::time::Duration::from_millis(500)) else {
            return openmix_core::MeterFrame { levels: Vec::new() };
        };
        let mut levels = vec![0.0f32; XAIR_STRIPS];
        for (i, v) in latest.iter().take(16).enumerate() {
            levels[i] = *v;
        }
        if let Some(v) = latest.get(26) {
            levels[35] = *v;
        }
        openmix_core::MeterFrame { levels }
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        let guard = self.session().await?;
        let reply = guard.as_ref().unwrap().query(addrs::INFO).await?;
        let peer = self.peer.lock().await.clone();
        Ok(Self::info_from_strings(osc::string_args(&reply), peer))
    }

    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        // 0-based trait index; the wire is 1-based on both axes (`fx`).
        // Validated BEFORE any I/O so out-of-range never hits the desk.
        // Bit-exact on hardware (baseline §6 row 5) — no fader epsilon here.
        let addr = fx::fx_param(slot, param)?;
        let value = fx::check_value(value)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::Float(value)])
            .await
    }

    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        // Loaded effect selects the par def (enum pars reply as INT option
        // indices on hardware — see `crate::fx`). A missing/unparseable
        // type falls back to raw decode (`None` def); plain floats stay
        // bit-exact either way (baseline §6 row 5).
        let effect = match self.session().await?.as_ref().unwrap().query(&fx::fx_type(slot)?).await {
            Ok(treply) => fx::parse_fx_type(&treply.args).ok(),
            Err(_) => None,
        };
        self.get_fx_param_with_type(slot, param, effect).await
    }

    fn poll_spectrum(&self) -> Vec<openmix_core::AudioSpectrum> {
        self.rta.lock().unwrap().as_ref().map_or_else(Vec::new, |rta| rta.poll())
    }

    async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()> {
        let stream = self.rta.lock().unwrap().clone();
        if let Some(stream) = stream { stream.target(owner, strip, false).await?; }
        Ok(())
    }

    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openmix_core::MeterFrame>> {
        let rx = self
            .subscribe_meters_with_interval(addrs::METER_RENEW_INTERVAL)
            .await?;
        // Blob-order cache drain feeding `poll_meters`: a resubscribed
        // clone sees every frame alongside the caller's receiver.
        let mut drain = rx.resubscribe();
        let cache = self.meter_latest.clone();
        let receive_since = self.receive_since.clone();
        let session = self.session().await?.as_ref().unwrap().clone();
        let generation = self.connection_generation.clone();
        let connection = generation.load(std::sync::atomic::Ordering::Acquire);
        let task = tokio::spawn(async move {
            loop {
                match drain.recv().await {
                    Ok(_frame) => {
                        if generation.load(std::sync::atomic::Ordering::Acquire) != connection { break; }
                        if let Ok(mut guard) = cache.lock() {
                            *guard = session.latest_meter_sample().filter(|(at, _)| *at >= *receive_since.lock().unwrap()).map(|(at, frame)| (at, frame.levels));
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self.tasks.lock().unwrap().push(task);
        Ok(rx)
    }

    /// Desk-originated mix-surface updates (see [`parse_inbound`]): the
    /// stable channel fed by the per-connection ingest task. The engine
    /// subscribes once and applies these live; reconnects never disturb
    /// the receiver.
    fn subscribe_updates(
        &self,
    ) -> Option<broadcast::Receiver<openmix_core::ParamUpdate>> {
        Some(self.update_tx.subscribe())
    }

    /// Full param plane (see [`XAIR_CAPABILITIES`]). Strip roots come from
    /// [`strip_root`]: channels (`/ch/NN`) carry the whole set; FX returns
    /// (`/rtn/N`), buses and LR carry fader/mute/name/color plus the
    /// kind-scoped gate/comp/EQ sets; FX-send masters and DCAs carry
    /// fader/mute/name/color only (catalogue scopes). Every address/codec is
    /// confirmed against the owner XR18 (FW 1.20) — see
    /// `docs/xr18-baseline.md` for the readback table.
    ///
    /// GR reads ride the param path (`gate_gr`/`comp_gr` serve dB reduction
    /// `>= 0` from the last `/meters/6` blob; no GR query address exists).
    /// `delay_on`/`delay_ms` (X32-only), `hpf_slope` (X32-only), `source`
    /// (`insrc` live-silent, FW 1.20), `icon` (no firmware leaf), `link`
    /// (global `/config/chlink`, no per-channel leaf), per-channel `eq5_*`
    /// (channels/returns carry 4 bands; `eq5/6_*` ride buses + main),
    /// `aux7/8_*`, `send11..16_*` and all legacy `auxN_mute`/`fxN_mute` plus
    /// the v2 `send1..10_on` (the send block has no on/off leaf and
    /// `/ch/NN/mix/MM/on` is live-silent — baseline T7) stay unsupported.
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        let (kind, root) = strip_root(strip.0)?;
        if openmix_core::params::find(param).is_some() {
            check_scope(param, kind)?;
        }
        // Channel wire number for channel-only leaves (headamp/input: the
        // scope check above already rejected non-input strips).
        let ch = || self.check_channel(strip.0);
        match (param, value) {
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("pan", ParamValue::Float(pan)) => {
                let wire = mix::pan_to_wire(pan)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&format!("{root}/mix/pan"), vec![OscType::Float(wire)])
                    .await
            }
            ("lr_assign", ParamValue::Bool(assigned)) => {
                // No leaf on `/lr` (the `/lr/mix` container has no `lr`
                // field) — the scope check above already confines this to
                // channel/FX-return/bus strips.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&mix::mix_lr(&root), vec![codec::bool_arg(assigned)])
                    .await
            }
            ("color", ParamValue::Float(v)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&look::strip_color(&root), vec![look::color_to_arg(v)?])
                    .await
            }
            ("icon", ParamValue::Float(v)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&look::strip_icon(&root), vec![look::icon_to_arg(v)?])
                    .await
            }
            ("link", ParamValue::Bool(on)) => {
                let _ = ch()?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&look::strip_link(&root), vec![codec::bool_arg(on)])
                    .await
            }
            ("gate_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&format!("{root}/gate/on"), vec![codec::bool_arg(on)])
                    .await
            }
            ("comp_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&format!("{root}/dyn/on"), vec![codec::bool_arg(on)])
                    .await
            }
            ("eq_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&eq::eq_on_r(&root), vec![codec::bool_arg(on)])
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("eq") => {
                let (band, leaf) = eq_leaf(p)?;
                check_eq_band(kind, band)?;
                let (addr, arg) = match leaf {
                    "gain" => (eq::eq_gain_r(&root, band)?, OscType::Float(eq::gain_db_to_wire(v)?)),
                    "freq" => (eq::eq_freq_r(&root, band)?, OscType::Float(eq::freq_hz_to_wire(v)?)),
                    "q" => (eq::eq_q_r(&root, band)?, OscType::Float(eq::q_to_wire(v)?)),
                    "type" => (eq::eq_type_r(&root, band)?, eq::eqtype_to_arg(v)?),
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&addr, vec![arg]).await
            }
            (p, ParamValue::Float(v))
                if matches!(p, "gate_mode" | "gate_key_src" | "gate_filter_type") =>
            {
                let (addr, arg) = match p {
                    "gate_mode" => (gate::gate_mode(&root), gate::int_leaf_to_arg(v, gate::GATE_MODE_MIN, gate::GATE_MODE_MAX, "mode")?),
                    "gate_key_src" => (gate::gate_key_src(&root), gate::int_leaf_to_arg(v, gate::GATE_KEY_SRC_MIN, gate::GATE_KEY_SRC_MAX, "keysrc")?),
                    _ => (gate::gate_filter_type(&root), gate::int_leaf_to_arg(v, gate::GATE_FILTER_TYPE_MIN, gate::GATE_FILTER_TYPE_MAX, "filter type")?),
                };
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&addr, vec![arg]).await
            }
            ("gate_filter_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&gate::gate_filter_on(&root), vec![codec::bool_arg(on)]).await
            }
            ("gate_filter_freq", ParamValue::Float(v)) => {
                let wire = gate::filter_hz_to_wire(v)?;
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&gate::gate_filter_freq(&root), vec![OscType::Float(wire)]).await
            }
            (p, ParamValue::Float(v)) if p.starts_with("gate_") => {
                let addr = match p {
                    "gate_thr" => (gate::gate_thr(&root), gate::lin_to_wire(v, gate::GATE_THR_MIN_DB, gate::GATE_THR_MAX_DB, "thr")?),
                    "gate_range" => (gate::gate_range(&root), gate::lin_to_wire(v, gate::GATE_RANGE_MIN_DB, gate::GATE_RANGE_MAX_DB, "range")?),
                    "gate_att" => (gate::gate_attack(&root), gate::lin_to_wire(v, gate::GATE_ATT_MIN_MS, gate::GATE_ATT_MAX_MS, "attack")?),
                    "gate_hold" => (gate::gate_hold(&root), gate::log_to_wire(v, gate::GATE_HOLD_MIN_MS, gate::GATE_HOLD_MAX_MS, "hold")?),
                    "gate_rel" => (gate::gate_release(&root), gate::log_to_wire(v, gate::GATE_REL_MIN_MS, gate::GATE_REL_MAX_MS, "release")?),
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&addr.0, vec![OscType::Float(addr.1)]).await
            }
            (p, ParamValue::Float(v))
                if matches!(p, "comp_thr" | "comp_knee" | "comp_makeup" | "comp_att" | "comp_hold" | "comp_rel" | "comp_mix") =>
            {
                let (addr, wire) = match p {
                    "comp_thr" => (comp::comp_thr(&root), comp::lin_to_wire(v, comp::COMP_THR_MIN_DB, comp::COMP_THR_MAX_DB, "thr")?),
                    "comp_knee" => (comp::comp_knee(&root), comp::lin_to_wire(v, comp::COMP_KNEE_MIN, comp::COMP_KNEE_MAX, "knee")?),
                    "comp_makeup" => (comp::comp_makeup(&root), comp::lin_to_wire(v, comp::COMP_MAKEUP_MIN_DB, comp::COMP_MAKEUP_MAX_DB, "makeup")?),
                    "comp_att" => (comp::comp_attack(&root), comp::lin_to_wire(v, comp::COMP_ATT_MIN_MS, comp::COMP_ATT_MAX_MS, "attack")?),
                    "comp_hold" => (comp::comp_hold(&root), comp::log_to_wire(v, comp::COMP_HOLD_MIN_MS, comp::COMP_HOLD_MAX_MS, "hold")?),
                    "comp_rel" => (comp::comp_release(&root), comp::log_to_wire(v, comp::COMP_REL_MIN_MS, comp::COMP_REL_MAX_MS, "release")?),
                    _ => (comp::comp_mix(&root), comp::lin_to_wire(v, comp::COMP_MIX_MIN, comp::COMP_MIX_MAX, "mix")?),
                };
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&addr, vec![OscType::Float(wire)]).await
            }
            ("comp_ratio", ParamValue::Float(v)) => {
                // Firmware step table: the UI ratio number must be an exact
                // table entry; the wire is the INT index.
                let idx = comp::ratio_to_index(v)?;
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&comp::comp_ratio(&root), vec![OscType::Int(idx)]).await
            }
            (p, ParamValue::Float(v))
                if matches!(p, "comp_mode" | "comp_det" | "comp_env" | "comp_key_src" | "comp_filter_type") =>
            {
                let (addr, arg) = match p {
                    "comp_mode" => (comp::comp_mode(&root), comp::int_leaf_to_arg(v, comp::COMP_MODE_MIN, comp::COMP_MODE_MAX, "mode")?),
                    "comp_det" => (comp::comp_det(&root), comp::int_leaf_to_arg(v, comp::COMP_DET_MIN, comp::COMP_DET_MAX, "det")?),
                    "comp_env" => (comp::comp_env(&root), comp::int_leaf_to_arg(v, comp::COMP_ENV_MIN, comp::COMP_ENV_MAX, "env")?),
                    "comp_key_src" => (comp::comp_key_src(&root), comp::int_leaf_to_arg(v, 0, 22, "keysrc")?),
                    _ => (comp::comp_filter_type(&root), comp::int_leaf_to_arg(v, 0, 8, "filter type")?),
                };
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&addr, vec![arg]).await
            }
            ("comp_auto", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&comp::comp_auto(&root), vec![codec::bool_arg(on)]).await
            }
            ("comp_filter_on", ParamValue::Bool(on)) => {
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&comp::comp_filter_on(&root), vec![codec::bool_arg(on)]).await
            }
            ("comp_filter_freq", ParamValue::Float(v)) => {
                let wire = gate::filter_hz_to_wire(v)?;
                let guard = self.session().await?;
                guard.as_ref().unwrap().send(&comp::comp_filter_freq(&root), vec![OscType::Float(wire)]).await
            }
            (p, ParamValue::Float(v)) if p.starts_with("aux") => {
                let n = aux_send(p)?;
                let ch = ch()?;
                let wire = mix::send_db_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&mix::send_level(ch, mix::aux_slot(n)?)?, vec![OscType::Float(wire)])
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("fx") => {
                let n = fx_send(p)?;
                let ch = ch()?;
                let wire = mix::send_db_to_wire(v)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&mix::send_level(ch, mix::fx_slot(n)?)?, vec![OscType::Float(wire)])
                    .await
            }
            (p, ParamValue::Float(v)) if p.starts_with("send") => {
                let (n, leaf) = generic_send(p)?;
                let ch = ch()?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                match leaf {
                    "level" => {
                        let wire = mix::send_db_to_wire(v)?;
                        session.send(&mix::send_level(ch, slot)?, vec![OscType::Float(wire)]).await
                    }
                    "tap" => session.send(&mix::send_tap(&root, slot)?, vec![mix::tap_to_arg(v)?]).await,
                    "pan" => {
                        let wire = mix::pan_to_wire(v)?;
                        session.send(&mix::send_pan(&root, slot)?, vec![OscType::Float(wire)]).await
                    }
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                }
            }
            (p, ParamValue::Bool(on)) if p.starts_with("send") => {
                // `sendN_on` and `sendN_grpon` (level/tap/pan are floats):
                // wrong-leaf bools fall through to the unknown-param error
                // below. The addresses are root-based; the scope check above
                // already confined this to input strips.
                let (n, leaf) = generic_send(p)?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let addr = match leaf {
                    "on" => mix::send_on(&root, slot)?,
                    "grpon" => mix::send_grpon(&root, slot)?,
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                };
                guard
                    .as_ref()
                    .unwrap()
                    .send(&addr, vec![codec::bool_arg(on)])
                    .await
            }
            ("gain", ParamValue::Float(db)) => {
                let ch = self.check_channel(strip.0)?;
                let wire = headamp::db_to_wire(db)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&headamp::headamp_gain(ch), vec![OscType::Float(wire)])
                    .await
            }
            ("phantom", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(
                        &headamp::headamp_phantom(ch),
                        vec![codec::bool_arg(on)],
                    )
                    .await
            }
            ("usbreturn", ParamValue::Float(v)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::config_rtnsrc(ch), vec![input::usbreturn_to_arg(v)?])
                    .await
            }
            ("trim", ParamValue::Float(db)) => {
                let ch = self.check_channel(strip.0)?;
                let wire = input::rtntrim_db_to_wire(db)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_rtntrim(ch), vec![OscType::Float(wire)])
                    .await
            }
            ("polarity", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_invert(ch), vec![codec::bool_arg(on)])
                    .await
            }
            ("hpf_on", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_hpf_on(ch), vec![codec::bool_arg(on)])
                    .await
            }
            ("hpf_freq", ParamValue::Float(hz)) => {
                let ch = self.check_channel(strip.0)?;
                let wire = input::hpf_hz_to_wire(hz)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_hpf(ch), vec![OscType::Float(wire)])
                    .await
            }
            ("automix_weight", ParamValue::Float(w)) => {
                // Channels only (the scope check above already rejected
                // buses/returns/mains); no engineering curve is verified,
                // so the wire float rides verbatim.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&automix::automix_weight(&root), vec![OscType::Float(automix::weight_to_wire(w)?)])
                    .await
            }
            ("automix_on", ParamValue::Bool(on)) => {
                // No standalone leaf: read the container (keeping the
                // current weight) and rewrite it with the new enable bit.
                // Two queries, one write — the enable survives verbatim.
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let reply = session.query(&automix::automix(&root)).await?;
                let (_, w) = automix::automix_from_msg(&reply)?;
                session
                    .send(&automix::automix(&root), automix::automix_to_args(on, w)?)
                    .await
            }
            (p, ParamValue::Bool(member)) if p.starts_with("dca") => {
                // Read-modify-write on the `{root}/grp/dca` bitmask (the
                // desk has no per-group leaf). Out-of-range groups and
                // upper bits never reach the desk (see `groups`).
                let group = dca_assign(p)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                let reply = session.query(&groups::grp_dca(&root)).await?;
                let mask = groups::mask_from_msg(&reply)?;
                let next = groups::mask_with(mask, group, member, "dca")?;
                session
                    .send(&groups::grp_dca(&root), vec![OscType::Int(next)])
                    .await
            }
            ("insert_on", ParamValue::Bool(on)) => {
                // Insert bypass switch (`{root}/insert/on`, rig-verified
                // 2026-10-20). The scope check above already confined this
                // to channel/bus/main strips (returns/sends/masters have no
                // insert leaf — the desk is silent there).
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&insert::insert_on(&root), vec![codec::bool_arg(on)])
                    .await
            }
            ("insert_sel", ParamValue::Float(v)) => {
                // Insert select (`{root}/insert/sel`, raw int `0..=8`,
                // rig-verified 2026-10-20 — NOT 1..4 FX slots, so no slot
                // mapping is asserted). Same scoping as `insert_on`.
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&insert::insert_sel(&root), vec![insert::sel_to_arg(v)?])
                    .await
            }
            (p, _) => anyhow::bail!("{}: unknown param {p:?}", self.name()),
        }
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        let (kind, root) = strip_root(strip.0)?;
        if openmix_core::params::find(param).is_some() {
            check_scope(param, kind)?;
        }
        let ch = || self.check_channel(strip.0);
        if let Some(suffix) = param.strip_prefix("_node/") {
            let allowed = matches!(suffix, "config" | "preamp" | "gate" | "gate/filter" | "dyn" | "dyn/filter" | "eq" | "mix" | "grp" | "insert" | "automix")
                || suffix.strip_prefix("eq/").and_then(|s| s.parse::<u8>().ok()).is_some_and(|n| (1..=6).contains(&n))
                || suffix.strip_prefix("mix/").and_then(|s| s.parse::<u8>().ok()).is_some_and(|n| (1..=10).contains(&n));
            anyhow::ensure!(allowed, "unsupported node query");
            let guard = self.session().await?;
            guard.as_ref().unwrap().query(&format!("{root}/{suffix}")).await?;
            // The ordered receive plane expands the complete typed reply.
            return Ok(ParamValue::Bool(true));
        }
        if let Some(addr) = sync_decode::address(strip.0, param) {
            let guard = self.session().await?;
            let reply = guard.as_ref().unwrap().query(&addr).await?;
            return sync_decode::decode(param, &reply);
        }
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            "color" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&look::strip_color(&root)).await?;
                look::int_from_msg(&reply, look::COLOR_MIN, look::COLOR_MAX, "color")
                    .map(|i| ParamValue::Float(i as f32))
            }
            "icon" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&look::strip_icon(&root)).await?;
                look::int_from_msg(&reply, look::ICON_MIN, look::ICON_MAX, "icon")
                    .map(|i| ParamValue::Float(i as f32))
            }
            "link" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&look::strip_link(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "gain" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&headamp::headamp_gain(ch))
                    .await?;
                headamp::wire_to_db(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "hpf_on" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf_on(ch))
                    .await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "hpf_freq" => {
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf(ch))
                    .await?;
                input::hpf_wire_to_hz(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "automix_on" => {
                // No standalone leaf (the desk is silent on
                // `{root}/automix/on`): read the container and take pos0.
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&automix::automix(&root)).await?;
                automix::automix_from_msg(&reply).map(|(on, _)| ParamValue::Bool(on))
            }
            p if p.starts_with("dca") => {
                // DCA membership bits of the `{root}/grp/dca` bitmask
                // (the scope check above already confined this to strips
                // whose root answers the leaf — channels, FX returns,
                // buses; main/DCA/FX-send masters are silent there).
                let group = dca_assign(p)?;
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&groups::grp_dca(&root)).await?;
                let mask = groups::mask_from_msg(&reply)?;
                groups::mask_has(mask, group, "dca").map(ParamValue::Bool)
            }
            "insert_on" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&insert::insert_on(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "insert_sel" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&insert::insert_sel(&root)).await?;
                insert::sel_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
            }
            "pan" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&format!("{root}/mix/pan")).await?;
                mix::wire_to_pan(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "lr_assign" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&mix::mix_lr(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            "eq_on" => {
                let guard = self.session().await?;
                let reply = guard.as_ref().unwrap().query(&eq::eq_on_r(&root)).await?;
                osc::is_on(&reply).map(ParamValue::Bool)
            }
            // GR reads ride the param path (DYN blob cache; no desk query
            // address). Placed before the `gate_`/`comp_` prefix arms.
            "gate_gr" => self.cached_gr(strip, true).await.map(ParamValue::Float),
            "comp_gr" => self.cached_gr(strip, false).await.map(ParamValue::Float),
            p if p.starts_with("eq") => {
                let (band, leaf) = eq_leaf(p)?;
                check_eq_band(kind, band)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                match leaf {
                    "gain" => {
                        let reply = session.query(&eq::eq_gain_r(&root, band)?).await?;
                        eq::wire_to_gain_db(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "freq" => {
                        let reply = session.query(&eq::eq_freq_r(&root, band)?).await?;
                        eq::wire_to_freq_hz(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "q" => {
                        let reply = session.query(&eq::eq_q_r(&root, band)?).await?;
                        eq::wire_to_q(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    "type" => {
                        let reply = session.query(&eq::eq_type_r(&root, band)?).await?;
                        eq::eqtype_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
                    }
                    _ => anyhow::bail!("{}: unknown param {p:?}", self.name()),
                }
            }
            p if p.starts_with("aux") => {
                let n = aux_send(p)?;
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&mix::send_level(ch, mix::aux_slot(n)?)?)
                    .await?;
                mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
            }
            p if p.starts_with("fx") => {
                let n = fx_send(p)?;
                let ch = ch()?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&mix::send_level(ch, mix::fx_slot(n)?)?)
                    .await?;
                mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
            }
            // Preserve the pre-existing direct helper for callers using
            // unadvertised leaves (loopback compatibility only). XR18 caps,
            // push and polling deliberately exclude send on/mute/dest.
            p if p.starts_with("send") => {
                let (n, leaf) = generic_send(p)?;
                let ch = ch()?;
                let slot = mix::generic_send_slot(n)?;
                let guard = self.session().await?;
                let session = guard.as_ref().unwrap();
                match leaf {
                    "level" => {
                        let reply = session.query(&mix::send_level(ch, slot)?).await?;
                        mix::wire_to_send_db(mix::send_level_value(&reply)?).map(ParamValue::Float)
                    }
                    "tap" => {
                        let reply = session.query(&mix::send_tap(&root, slot)?).await?;
                        mix::tap_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
                    }
                    "grpon" => {
                        let reply = session.query(&mix::send_grpon(&root, slot)?).await?;
                        osc::is_on(&reply).map(ParamValue::Bool)
                    }
                    "pan" => {
                        let reply = session.query(&mix::send_pan(&root, slot)?).await?;
                        mix::wire_to_pan(osc::fader_value(&reply)?).map(ParamValue::Float)
                    }
                    _ => {
                        let reply = session.query(&mix::send_on(&root, slot)?).await?;
                        osc::is_on(&reply).map(ParamValue::Bool)
                    }
                }
            }
            _ => anyhow::bail!("{}: unknown param {param:?}", self.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmix_transport::meters::build_meter_blob;
    use rosc::OscType;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::UdpSocket;

    /// In-process fake XR18: UDP loopback socket speaking real OSC via transport codecs.
    /// Fader stores quantize writes to 10-bit steps like the real desk
    /// (`(v*1023).floor()/1023`, so 0.5 stores as 0.49951124 — baseline T8),
    /// which is why fader asserts use `FADER_READBACK_EPSILON`, not exact
    /// equality. Meter streaming only starts after the documented
    /// `/meters` + stream-id-string subscribe (T4).
    struct FakeMixer {
        socket: Arc<UdpSocket>,
        faders: StdMutex<HashMap<String, f32>>,
        ons: StdMutex<HashMap<String, i32>>,
        names: StdMutex<HashMap<String, String>>,
        /// FX params answer bit-exact (no quantization, baseline §6 row 5).
        /// Stored verbatim (`Float` or `Int`) so enum/switch pars keep
        /// their hardware INT wire shape; untouched pars read back 0.0.
        fx: StdMutex<HashMap<String, OscType>>,
        /// Loaded effect per slot (`/fx/N/type` int `0..=60`); unset slots
        /// read back 0 (HALL) like a freshly-booted desk.
        fx_types: StdMutex<HashMap<u8, i32>>,
        /// Headamp gains store the verbatim normalized wire float
        /// (hardware quantization uncharacterized — see `crate::headamp`).
        gains: StdMutex<HashMap<String, f32>>,
        /// Headamp phantom switches store the verbatim int (`1`/`0`).
        phantoms: StdMutex<HashMap<String, i32>>,
        /// Input-routing int leaves (`/config/rtnsrc`, `/preamp/invert`,
        /// `/preamp/hpon`): verbatim ints. `insrc` / channel `rtnsw` have
        /// NO arms — the fake stays silent there like the live desk.
        input_ints: StdMutex<HashMap<String, i32>>,
        /// Input-routing float leaves (`/preamp/rtntrim`, `/preamp/hpf`):
        /// verbatim wire floats.
        input_floats: StdMutex<HashMap<String, f32>>,
        /// Param-plane int leaves (`/eq/on`, `/gate/on`, `/dyn/on`,
        /// `/eq/N/type`): verbatim ints, defaulting to 0 (off / first
        /// filter type) like an untouched desk strip.
        plane_ints: StdMutex<HashMap<String, i32>>,
        /// Param-plane float leaves (`/eq/N/{g,f,q}`, `/mix/pan`,
        /// `/mix/MM/level`): verbatim wire floats. Untouched defaults:
        /// eq `g`/`f`/`q` 0.5, `pan` 0.5 (centre), send `level` 0.0
        /// (−90 dB floor).
        plane_floats: StdMutex<HashMap<String, f32>>,
        meter_subscribe: StdMutex<Option<rosc::OscMessage>>,
        dyn_subscribe: StdMutex<Option<rosc::OscMessage>>,
        /// Resubscribe tolerance counters: the renewal loop re-sends
        /// `/meters` subscribes (and `/xremote`) on schedule, so the fake
        /// counts every hit instead of asserting exactly-one.
        meter_count: StdMutex<u64>,
        dyn_count: StdMutex<u64>,
        xremote_count: StdMutex<u64>,
        /// Most recently seen peer (any incoming datagram refreshes it):
        /// `broadcast` fires unsolicited datagrams there, simulating a
        /// desk-side change broadcast to `/xremote` subscribers.
        last_peer: StdMutex<Option<SocketAddr>>,
    }

    impl FakeMixer {
        async fn spawn() -> (SocketAddr, Arc<Self>) {
            let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            let addr = socket.local_addr().unwrap();
            let fake = Arc::new(Self {
                socket: socket.clone(),
                faders: StdMutex::new(HashMap::new()),
                ons: StdMutex::new(HashMap::from([(
                    "/ch/01/mix/on".to_string(),
                    1,
                )])),
                names: StdMutex::new(HashMap::new()),
                fx: StdMutex::new(HashMap::new()),
                fx_types: StdMutex::new(HashMap::new()),
                gains: StdMutex::new(HashMap::new()),
                phantoms: StdMutex::new(HashMap::new()),
                input_ints: StdMutex::new(HashMap::new()),
                input_floats: StdMutex::new(HashMap::new()),
                plane_ints: StdMutex::new(HashMap::new()),
                plane_floats: StdMutex::new(HashMap::new()),
                meter_subscribe: StdMutex::new(None),
                dyn_subscribe: StdMutex::new(None),
                meter_count: StdMutex::new(0),
                dyn_count: StdMutex::new(0),
                xremote_count: StdMutex::new(0),
                last_peer: StdMutex::new(None),
            });
            let worker = fake.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65535];
                // Preload defaults. Bus keys are single-digit: the real desk
                // is silent on `/bus/01/…` (T2), and so is this fake.
                worker.faders.lock().unwrap().insert("/ch/01/mix/fader".into(), 0.75);
                worker.faders.lock().unwrap().insert("/bus/1/mix/fader".into(), 0.7);
                worker.faders.lock().unwrap().insert(addrs::MAIN_FADER.into(), 0.8);
                loop {
                    let Ok((n, peer)) = worker.socket.recv_from(&mut buf).await else {
                        break;
                    };
                    *worker.last_peer.lock().unwrap() = Some(peer);
                    let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                        continue;
                    };
                    for msg in osc::flatten(packet) {
                        let reply: Option<Vec<u8>> = match (msg.addr.as_str(), msg.args.first()) {
                            ("/xremote", _) => {
                                *worker.xremote_count.lock().unwrap() += 1;
                                None
                            }
                            ("/meters", Some(OscType::String(id)))
                                if id == addrs::METER_SUBSCRIBE =>
                            {
                                // Documented subscribe form (T4): record it,
                                // then stream blobs at ~50ms like the desk.
                                // Resubscribes (renewal loop) just restart the
                                // burst — the desk tolerates them, so does this fake.
                                *worker.meter_subscribe.lock().unwrap() = Some(msg.clone());
                                *worker.meter_count.lock().unwrap() += 1;
                                let sock = worker.socket.clone();
                                tokio::spawn(async move {
                                    for _ in 0..4 {
                                        let blob = build_meter_blob(&[0.1, 0.5, 0.9]);
                                        let bytes = osc::encode_message(
                                            addrs::METER_SUBSCRIBE,
                                            vec![OscType::Blob(blob)],
                                        )
                                        .unwrap();
                                        let _ = sock.send_to(&bytes, peer).await;
                                        tokio::time::sleep(std::time::Duration::from_millis(50))
                                            .await;
                                    }
                                });
                                None
                            }
                            ("/meters", Some(OscType::String(id)))
                                if id == addrs::METER_DYN_SUBSCRIBE =>
                            {
                                // Documented ALL DYN subscribe (dyn_gr): record
                                // it, then stream the documented blob shape
                                // (ch1 gate 6 dB, ch1 comp 12 dB) like a desk
                                // with dynamics engaged would. Resubscribes
                                // restart the burst, like the levels stream.
                                *worker.dyn_subscribe.lock().unwrap() = Some(msg.clone());
                                *worker.dyn_count.lock().unwrap() += 1;
                                let sock = worker.socket.clone();
                                tokio::spawn(async move {
                                    let mut m = dyn_gr::DynMeters {
                                        gate: [0.0; dyn_gr::DYN_CHANNELS],
                                        comp: [0.0; dyn_gr::DYN_CHANNELS],
                                        bus: [0.0; dyn_gr::DYN_BUSES],
                                        lr: 0.0,
                                    };
                                    m.gate[0] = 6.0;
                                    m.comp[0] = 12.0;
                                    for _ in 0..2 {
                                        let blob = dyn_gr::build_dyn_blob(&m);
                                        let bytes = osc::encode_message(
                                            addrs::METER_DYN_SUBSCRIBE,
                                            vec![OscType::Blob(blob)],
                                        )
                                        .unwrap();
                                        let _ = sock.send_to(&bytes, peer).await;
                                        tokio::time::sleep(std::time::Duration::from_millis(50))
                                            .await;
                                    }
                                });
                                None
                            }
                            ("/info", _) if msg.args.is_empty() => Some(
                                osc::encode_message(
                                    "/info",
                                    vec![
                                        OscType::String("V0.04".into()),
                                        OscType::String("FakeXR".into()),
                                        OscType::String("XR18".into()),
                                        OscType::String("1.22".into()),
                                    ],
                                )
                                .unwrap(),
                            ),
                            // Faders quantize like hardware (T8); on/name are exact.
                            // DCA faders ride the same 1023-step law on the
                            // desk (uncharacterized — same treatment here).
                            (a, Some(OscType::Float(f)))
                                if a.ends_with("/mix/fader") || a.starts_with("/dca/") && a.ends_with("/fader") =>
                            {
                                let q = (*f * 1023.0).floor() / 1023.0;
                                worker.faders.lock().unwrap().insert(a.to_string(), q);
                                None
                            }
                            // FX type selects store verbatim (int form).
                            (a, Some(OscType::Int(i)))
                                if a.starts_with("/fx/") && a.ends_with("/type") =>
                            {
                                if let Some(slot) = a
                                    .split('/')
                                    .nth(2)
                                    .and_then(|s| s.parse::<u8>().ok())
                                {
                                    worker.fx_types.lock().unwrap().insert(slot, *i);
                                }
                                None
                            }
                            // FX params are bit-exact on hardware (baseline
                            // §6 row 5): store verbatim (int or float), no
                            // quantization — enum/switch pars keep INT shape.
                            (a, Some(arg @ (OscType::Float(_) | OscType::Int(_))))
                                if a.starts_with("/fx/")
                                    && a.contains("/par/") =>
                            {
                                worker.fx.lock().unwrap().insert(a.to_string(), arg.clone());
                                None
                            }
                            // Headamp gain stores the verbatim normalized
                            // wire float (quantization uncharacterized).
                            (a, Some(OscType::Float(f)))
                                if a.starts_with("/headamp/")
                                    && a.ends_with("/gain") =>
                            {
                                worker.gains.lock().unwrap().insert(a.to_string(), *f);
                                None
                            }
                            // Headamp phantom stores the verbatim int.
                            (a, Some(OscType::Int(i)))
                                if a.starts_with("/headamp/")
                                    && a.ends_with("/phantom") =>
                            {
                                worker.phantoms.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // Input-routing int leaves store verbatim (`rtnsrc`
                            // U01..U18 index, `invert`/`hpon` 1/0). Deliberately
                            // NO arms for `insrc` or channel `rtnsw`: the fake
                            // stays silent there exactly like the live desk.
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/config/rtnsrc")
                                    || a.ends_with("/preamp/invert")
                                    || a.ends_with("/preamp/hpon") =>
                            {
                                worker.input_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // Input-routing float leaves store the verbatim
                            // wire float (desk quantization uncharacterized;
                            // live ch16 reads rtntrim 0.0 / hpf 0.0).
                            (a, Some(OscType::Float(f)))
                                if a.ends_with("/preamp/rtntrim")
                                    || a.ends_with("/preamp/hpf") =>
                            {
                                worker.input_floats.lock().unwrap().insert(a.to_string(), *f);
                                None
                            }
                            // Param-plane int leaves store verbatim
                            // (`/eq/on`, `/gate/on`, `/dyn/on`, `/eq/N/type`
                            // all int 1/0 or 0..5). Deliberately NO arms for
                            // send mutes (`/ch/NN/mix/MM/on`): the fake stays
                            // silent there exactly like the live desk (T7).
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/eq/on")
                                    || a.ends_with("/gate/on")
                                    || a.ends_with("/dyn/on")
                                    || (a.contains("/eq/") && a.ends_with("/type")) =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // Param-plane float leaves store the verbatim
                            // wire float (`/eq/N/{g,f,q}`, `/mix/pan`,
                            // `/mix/MM/level`; desk quantization of these is
                            // uncharacterized — no fader-style rounding here).
                            (a, Some(OscType::Float(f)))
                                if (a.contains("/eq/")
                                    && (a.ends_with("/g")
                                        || a.ends_with("/f")
                                        || a.ends_with("/q")))
                                    || a.ends_with("/mix/pan")
                                    || (a.contains("/mix/") && a.ends_with("/level")) =>
                            {
                                worker.plane_floats.lock().unwrap().insert(a.to_string(), *f);
                                None
                            }
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/mix/on")
                                    || a.starts_with("/dca/") && a.ends_with("/on") =>
                            {
                                worker.ons.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, Some(OscType::String(s))) if a.ends_with("/config/name") => {
                                worker.names.lock().unwrap().insert(a.to_string(), s.clone());
                                None
                            }
                            // Group-assignment bitmasks (`{root}/grp/dca`,
                            // `{root}/grp/mute`): the live desk keeps only
                            // the low 4 bits (writes of 32/63 read back 15),
                            // so the fake masks on store — an unmasked store
                            // would pass a test the desk would fail.
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/grp/dca") || a.ends_with("/grp/mute") =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i & 0x0F);
                                None
                            }
                            (a, None)
                                if a.ends_with("/grp/dca") || a.ends_with("/grp/mute") =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // LR assign (`{root}/mix/lr`, int 1/0): store
                            // verbatim; untouched strips read back the live
                            // rig defaults (channels/FX returns assigned,
                            // buses not).
                            (a, Some(OscType::Int(i))) if a.ends_with("/mix/lr") => {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, None) if a.ends_with("/mix/lr") => {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or_else(|| {
                                        if a.starts_with("/bus/") {
                                            0
                                        } else {
                                            1
                                        }
                                    });
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // Automix container (`{root}/automix`, `(int,
                            // float)` = enable + weight): store both
                            // verbatim; untouched strips read back (0, 0.5)
                            // like the live rig.
                            (a, Some(_)) if a.ends_with("/automix") => {
                                if let [OscType::Int(on), OscType::Float(w)] = msg.args.as_slice()
                                {
                                    worker.plane_ints.lock().unwrap().insert(a.to_string(), *on);
                                    worker.plane_floats.lock().unwrap().insert(a.to_string(), *w);
                                }
                                None
                            }
                            (a, None) if a.ends_with("/automix") => {
                                let on = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                let w = worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.5);
                                Some(
                                    osc::encode_message(
                                        a,
                                        vec![OscType::Int(on), OscType::Float(w)],
                                    )
                                    .unwrap(),
                                )
                            }
                            (a, None)
                                if a.ends_with("/mix/fader")
                                    || a.starts_with("/dca/") && a.ends_with("/fader") =>
                            {
                                let v = worker
                                    .faders
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.75);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            (a, None)
                                if a.ends_with("/mix/on")
                                    || a.starts_with("/dca/") && a.ends_with("/on") =>
                            {
                                let v =
                                    worker.ons.lock().unwrap().get(a).copied().unwrap_or(1);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            (a, None) if a.ends_with("/config/name") => {
                                let v = worker
                                    .names
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .cloned()
                                    .unwrap_or_default();
                                Some(
                                    osc::encode_message(a, vec![OscType::String(v)]).unwrap(),
                                )
                            }
                            // `/fx/N/type` reads back the stored effect (unset → 0).
                            (a, None) if a.starts_with("/fx/") && a.ends_with("/type") => {
                                let slot = a
                                    .split('/')
                                    .nth(2)
                                    .and_then(|s| s.parse::<u8>().ok())
                                    .unwrap_or(0);
                                let v = worker
                                    .fx_types
                                    .lock()
                                    .unwrap()
                                    .get(&slot)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // Untouched FX params read back 0.0 float;
                            // stored pars answer verbatim (int stays int).
                            (a, None) if a.starts_with("/fx/") && a.contains("/par/") => {
                                let v = worker
                                    .fx
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .cloned()
                                    .unwrap_or(OscType::Float(0.0));
                                Some(osc::encode_message(a, vec![v]).unwrap())
                            }
                            // Untouched headamp gains read back 0.5 wire
                            // (= 24 dB); untouched phantom reads back off.
                            (a, None)
                                if a.starts_with("/headamp/")
                                    && a.ends_with("/gain") =>
                            {
                                let v = worker
                                    .gains
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.5);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            (a, None)
                                if a.starts_with("/headamp/")
                                    && a.ends_with("/phantom") =>
                            {
                                let v = worker
                                    .phantoms
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // Untouched input int leaves read back 0 (rtnsrc
                            // U01, invert/hpon off); untouched input floats
                            // read back 0.0 wire (rtntrim −18 dB, hpf 20 Hz).
                            (a, None)
                                if a.ends_with("/config/rtnsrc")
                                    || a.ends_with("/preamp/invert")
                                    || a.ends_with("/preamp/hpon") =>
                            {
                                let v = worker
                                    .input_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            (a, None)
                                if a.ends_with("/preamp/rtntrim")
                                    || a.ends_with("/preamp/hpf") =>
                            {
                                let v = worker
                                    .input_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.0);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            // Untouched param-plane switches read back 0
                            // (eq/gate/dyn off, first filter type).
                            (a, None)
                                if a.ends_with("/eq/on")
                                    || a.ends_with("/gate/on")
                                    || a.ends_with("/dyn/on")
                                    || (a.contains("/eq/") && a.ends_with("/type")) =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // Untouched param-plane floats: eq `g`/`f`/`q`
                            // read back mid-wire 0.5, pan centre 0.5, send
                            // levels the −90 dB floor (wire 0.0).
                            (a, None)
                                if (a.contains("/eq/")
                                    && (a.ends_with("/g")
                                        || a.ends_with("/f")
                                        || a.ends_with("/q")))
                                    || a.ends_with("/mix/pan")
                                    || (a.contains("/mix/") && a.ends_with("/level")) =>
                            {
                                let fallback = if a.ends_with("/level") { 0.0 } else { 0.5 };
                                let v = worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(fallback);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            // v2 int leaves (firmware-truth 2026-10-18): strip look
                            // (`/config/color`; `/config/icon` and
                            // `/config/link` have NO desk leaf — silent —
                            // but the fake answers them so the retained
                            // driver arms stay loopback-tested), gate/dyn
                            // int leaves (incl. the `filter/` sub-leaves and
                            // `dyn/ratio` step index), send tap + grpon and
                            // per-send on/off. NOTE the send-on conflict:
                            // this fake answers `/ch/NN/mix/MM/on` while the
                            // live desk is silent (T7); `sendN_on` is out of
                            // caps. Guards list int leaves EXACTLY: a broad
                            // `contains("/gate/")` here would shadow the
                            // float leaves' queries below (same `None`
                            // shape). Untouched reads answer 0.
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/config/color")
                                    || a.ends_with("/config/icon")
                                    || a.ends_with("/config/link")
                                    || a.ends_with("/gate/mode")
                                    || a.ends_with("/gate/keysrc")
                                    || a.ends_with("/gate/filter/on")
                                    || a.ends_with("/gate/filter/type")
                                    || a.ends_with("/dyn/ratio")
                                    || a.ends_with("/dyn/keysrc")
                                    || a.ends_with("/dyn/filter/on")
                                    || a.ends_with("/dyn/filter/type")
                                    || a.ends_with("/dyn/mode")
                                    || a.ends_with("/dyn/det")
                                    || a.ends_with("/dyn/env")
                                    || a.ends_with("/dyn/auto")
                                    || (a.contains("/mix/") && a.ends_with("/tap"))
                                    || (a.contains("/mix/") && a.ends_with("/grpon"))
                                    || (a.contains("/mix/")
                                        && a.ends_with("/on")
                                        && !a.ends_with("/mix/on")) =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // v2b: LR assign (`{root}/mix/lr`, ch/bus/rtn —
                            // the `/lr` root has no leaf and the driver
                            // never asks): verbatim ints, untouched reads
                            // default per rig (ch/rtn 1, bus 0).
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/mix/lr") =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // Strip insert (`{root}/insert/on|sel`, ch/bus/lr
                            // — rig-verified 2026-10-20; the driver never
                            // asks on rtn/fxsend/dca roots, which stay
                            // silent like the live desk): verbatim ints.
                            (a, Some(OscType::Int(i)))
                                if a.ends_with("/insert/on")
                                    || a.ends_with("/insert/sel") =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // v2b: solo monitoring globals (`/config/solo/*`):
                            // verbatim ints/float, untouched reads default
                            // per rig (dim 0, mute 0, level 0.75, source 3).
                            (a, Some(OscType::Int(i)))
                                if a == crate::solo::SOLO_DIM
                                    || a == crate::solo::SOLO_MUTE
                                    || a == crate::solo::SOLO_SOURCE =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, Some(OscType::Float(f)))
                                if a == crate::solo::SOLO_LEVEL =>
                            {
                                worker.plane_floats.lock().unwrap().insert(a.to_string(), *f);
                                None
                            }
                            // v2b: FX slot insert (`/fx/N/insert`): verbatim
                            // ints, untouched reads default 0 (bypassed).
                            // (Strip insert *containers* like `/ch/01/insert`
                            // stay silent — the driver addresses only the
                            // `/insert/on|sel` leaves.)
                            (a, Some(OscType::Int(i)))
                                if a.starts_with("/fx/") && a.ends_with("/insert") =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            // v2 float leaves: gate/dyn block (minus the int
                            // leaves above, which match first), key-filter
                            // corners (`filter/f`, log wire), per-send pan
                            // (`/mix/MM/pan`; channel pan keeps its own arm)
                            // and automix weight (`/automix/weight`, ch
                            // only). Untouched gate/dyn reads answer 0.0
                            // wire, send pan centre 0.5, automix 0.5.
                            (a, Some(OscType::Float(f)))
                                if (a.contains("/gate/")
                                    && !a.ends_with("/gate/mode")
                                    && !a.ends_with("/gate/keysrc")
                                    && !a.ends_with("/gate/filter/on")
                                    && !a.ends_with("/gate/filter/type"))
                                    || (a.contains("/dyn/")
                                        && !a.ends_with("/dyn/mode")
                                        && !a.ends_with("/dyn/det")
                                        && !a.ends_with("/dyn/env")
                                        && !a.ends_with("/dyn/auto")
                                        && !a.ends_with("/dyn/ratio")
                                        && !a.ends_with("/dyn/keysrc")
                                        && !a.ends_with("/dyn/filter/on")
                                        && !a.ends_with("/dyn/filter/type"))
                                    || (a.contains("/mix/")
                                        && a.ends_with("/pan")
                                        && !a.ends_with("/mix/pan"))
                                    || a.ends_with("/automix/weight") =>
                            {
                                worker.plane_floats.lock().unwrap().insert(a.to_string(), *f);
                                None
                            }
                            (a, None)
                                if a.ends_with("/config/color")
                                    || a.ends_with("/config/icon")
                                    || a.ends_with("/config/link")
                                    || a.ends_with("/gate/mode")
                                    || a.ends_with("/gate/keysrc")
                                    || a.ends_with("/gate/filter/on")
                                    || a.ends_with("/gate/filter/type")
                                    || a.ends_with("/dyn/ratio")
                                    || a.ends_with("/dyn/keysrc")
                                    || a.ends_with("/dyn/filter/on")
                                    || a.ends_with("/dyn/filter/type")
                                    || a.ends_with("/dyn/mode")
                                    || a.ends_with("/dyn/det")
                                    || a.ends_with("/dyn/env")
                                    || a.ends_with("/dyn/auto")
                                    || (a.contains("/mix/") && a.ends_with("/tap"))
                                    || (a.contains("/mix/") && a.ends_with("/grpon"))
                                    || (a.contains("/mix/")
                                        && a.ends_with("/on")
                                        && !a.ends_with("/mix/on")) =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // v2b: LR assign reads back the stored int, else
                            // the rig default (ch/rtn 1, bus 0).
                            (a, None) if a.ends_with("/mix/lr") => {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or_else(|| {
                                        if a.starts_with("/bus/") { 0 } else { 1 }
                                    });
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // v2b: solo globals read back stored values, else
                            // rig defaults (dim 0, mute 0, source 3).
                            (a, None)
                                if a == crate::solo::SOLO_DIM
                                    || a == crate::solo::SOLO_MUTE
                                    || a == crate::solo::SOLO_SOURCE =>
                            {
                                let fallback = if a == crate::solo::SOLO_SOURCE { 3 } else { 0 };
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(fallback);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // v2b: solo level reads back stored, else rig 0.75.
                            (a, None) if a == crate::solo::SOLO_LEVEL => {
                                let v = worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.75);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            // v2b: FX slot insert reads back stored, else 0.
                            (a, None)
                                if a.starts_with("/fx/") && a.ends_with("/insert") =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            // Strip insert reads back stored, else 0
                            // (bypassed, nothing selected — matches the rig
                            // defaults on untouched strips).
                            (a, None)
                                if a.ends_with("/insert/on")
                                    || a.ends_with("/insert/sel") =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            (a, None)
                                if (a.contains("/gate/")
                                    && !a.ends_with("/gate/mode")
                                    && !a.ends_with("/gate/keysrc")
                                    && !a.ends_with("/gate/filter/on")
                                    && !a.ends_with("/gate/filter/type"))
                                    || (a.contains("/dyn/")
                                        && !a.ends_with("/dyn/mode")
                                        && !a.ends_with("/dyn/det")
                                        && !a.ends_with("/dyn/env")
                                        && !a.ends_with("/dyn/auto")
                                        && !a.ends_with("/dyn/ratio")
                                        && !a.ends_with("/dyn/keysrc")
                                        && !a.ends_with("/dyn/filter/on")
                                        && !a.ends_with("/dyn/filter/type"))
                                    || (a.contains("/mix/")
                                        && a.ends_with("/pan")
                                        && !a.ends_with("/mix/pan")) =>
                            {
                                let fallback = if a.ends_with("/pan") { 0.5 } else { 0.0 };
                                let v = worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(fallback);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            // v2b: automix weight reads back stored, else 0.5.
                            (a, None) if a.ends_with("/automix/weight") => {
                                let v = worker
                                    .plane_floats
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.5);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            // Mute-group masters (`/config/mute/1..4`): verbatim
                            // ints, untouched reads answer 0. Groups 5+
                            // stay silent like the live desk (MUTE_GROUPS).
                            (a, Some(OscType::Int(i)))
                                if a.starts_with("/config/mute/")
                                    && matches!(a["/config/mute/".len()..].parse::<u8>(), Ok(n) if (1..=addrs::MUTE_GROUPS).contains(&n)) =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, None)
                                if a.starts_with("/config/mute/")
                                    && matches!(a["/config/mute/".len()..].parse::<u8>(), Ok(n) if (1..=addrs::MUTE_GROUPS).contains(&n)) =>
                            {
                                let v = worker
                                    .plane_ints
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0);
                                Some(osc::encode_message(a, vec![OscType::Int(v)]).unwrap())
                            }
                            ("/info", _) => None,
                            // Solo switches (`/-stat/solosw/NN`, 1-based):
                            // fire-and-forget sets store verbatim; the bank
                            // query reads back 54 ints (like the live desk,
                            // all 0 when nothing is soloed).
                            (a, Some(OscType::Int(i)))
                                if a.starts_with("/-stat/solosw/")
                                    && matches!(a["/-stat/solosw/".len()..].parse::<u8>(), Ok(n) if (1..=54).contains(&n)) =>
                            {
                                worker.plane_ints.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, None) if a == "/-stat/solosw" => {
                                let stored = worker.plane_ints.lock().unwrap();
                                let args: Vec<OscType> = (1..=54u8)
                                    .map(|n| {
                                        OscType::Int(
                                            stored
                                                .get(&format!("/-stat/solosw/{n:02}"))
                                                .copied()
                                                .unwrap_or(0),
                                        )
                                    })
                                    .collect();
                                Some(osc::encode_message(a, args).unwrap())
                            }
                            // Clear-solo action: fire-and-forget sets are
                            // ACKed by silence; queries read back int 0
                            // (nothing soloed), like the live desk.
                            (a, Some(OscType::Int(_)))
                                if a == addrs::CLEAR_SOLO =>
                            {
                                None
                            }
                            (a, None) if a == addrs::CLEAR_SOLO => {
                                Some(osc::encode_message(a, vec![OscType::Int(0)]).unwrap())
                            }
                            _ => None,
                        };
                        if let Some(bytes) = reply {
                            let _ = worker.socket.send_to(&bytes, peer).await;
                        }
                    }
                }
            });
            (addr, fake)
        }

        /// Fire an unsolicited datagram at the most recently seen peer,
        /// simulating a desk-side change broadcast to `/xremote`
        /// subscribers (no query asked for it — the driver's ingest task
        /// must route it to the update channel via `parse_inbound`).
        /// No-op until any datagram has arrived (the peer is unknown).
        async fn broadcast(&self, addr: &str, args: Vec<OscType>) {
            let peer = *self.last_peer.lock().unwrap();
            if let Some(peer) = peer {
                let bytes = osc::encode_message(addr, args).unwrap();
                let _ = self.socket.send_to(&bytes, peer).await;
            }
        }
    }

    async fn connected_driver() -> (XAirDriver, Arc<FakeMixer>) {
        let (addr, fake) = FakeMixer::spawn().await;
        // Bypass connect()'s best-effort /info ping timing by building the
        // session directly against the fake, then installing it.
        let peer: SocketAddr = addr;
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(peer).await.unwrap();
        let session =
            OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
        let driver = XAirDriver::new();
        *driver.session.lock().await = Some(std::sync::Arc::new(session));
        // Drain the fake's potential early datagrams by a tiny settle.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        (driver, fake)
    }

    #[tokio::test]
    async fn driver_fader_mute_name_roundtrip() {
        let (driver, _) = connected_driver().await;
        // StripId is 0-based: strip 0 drives wire `/ch/01/…`.
        // Fader assert uses the hardware quantization epsilon (T8): the fake
        // quantizes like the desk, so exact equality would fail here.
        driver.set_fader(StripId(0), 0.42).await.unwrap();
        let v = driver.get_fader(StripId(0)).await.unwrap();
        assert!(
            (v - 0.42).abs() < addrs::FADER_READBACK_EPSILON,
            "fader readback {v} outside hardware epsilon"
        );

        driver.set_mute(StripId(0), true).await.unwrap();
        assert!(driver.get_mute(StripId(0)).await.unwrap());
        driver.set_mute(StripId(0), false).await.unwrap();
        assert!(!driver.get_mute(StripId(0)).await.unwrap());

        driver.set_name(StripId(0), "Vox").await.unwrap();
        assert_eq!(driver.get_name(StripId(0)).await.unwrap(), "Vox");
    }

    #[tokio::test]
    async fn driver_bus_main_info_roundtrip() {
        let (driver, _) = connected_driver().await;
        // Buses are 0-based too: bus 0 drives wire `/bus/1/…` (single digit, T2).
        driver.set_bus_fader(0, 0.33).await.unwrap();
        let b = driver.get_bus_fader(0).await.unwrap();
        assert!(
            (b - 0.33).abs() < addrs::FADER_READBACK_EPSILON,
            "bus fader readback {b} outside hardware epsilon"
        );
        driver.set_main_fader(0.66).await.unwrap();
        let m = driver.get_main_fader().await.unwrap();
        assert!(
            (m - 0.66).abs() < addrs::FADER_READBACK_EPSILON,
            "main fader readback {m} outside hardware epsilon"
        );

        // `/info` order is [proto-rev, name, model, firmware] (T5, §1).
        let info = driver.query_info().await.unwrap();
        assert_eq!(info.name, "FakeXR");
        assert_eq!(info.model, "XR18");
        assert_eq!(info.firmware, "1.22");
    }

    /// Mute-group masters: `/config/mute/1..4` answer (strict fake stays
    /// silent on 5+ like the live desk), same-value writes round-trip
    /// bit-exact, out-of-range groups error locally.
    #[tokio::test]
    async fn driver_mute_group_roundtrip() {
        let (driver, _) = connected_driver().await;
        assert_eq!(driver.mute_group_count(), 4);
        for g in 0..4 {
            driver.set_mute_group(g, false).await.unwrap();
            assert!(!driver.get_mute_group(g).await.unwrap());
        }
        assert!(driver.set_mute_group(4, false).await.is_err());
    }

    /// Assign-plane loopback over UDP (`groups` + automix container, desk
    /// shapes rig-verified 2026-10-20): DCA/mute bitmask RMW, automix
    /// enable container RMW (weight preserved), LR round-trip. Out-of-range
    /// groups and silent roots (main/DCA) bail locally — the live desk
    /// saturates out-of-range mask writes, so an unchecked index would
    /// (mis)assign every group there.
    #[tokio::test]
    async fn driver_assign_plane_roundtrip() {
        let (driver, _) = connected_driver().await;
        // DCA bits on strip 0 (wire `/ch/01/grp/dca`, untouched reads 0).
        assert!(matches!(
            driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
            ParamValue::Bool(false)
        ));
        driver
            .set_param(StripId(0), "dca1_assign", ParamValue::Bool(true))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
            ParamValue::Bool(true)
        ));
        // Neighbouring bits stay clear; a second bit ORs in.
        assert!(matches!(
            driver.get_param(StripId(0), "dca2_assign").await.unwrap(),
            ParamValue::Bool(false)
        ));
        driver
            .set_param(StripId(0), "dca3_assign", ParamValue::Bool(true))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
            ParamValue::Bool(true)
        ));
        driver
            .set_param(StripId(0), "dca1_assign", ParamValue::Bool(false))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "dca1_assign").await.unwrap(),
            ParamValue::Bool(false)
        ));
        assert!(matches!(
            driver.get_param(StripId(0), "dca3_assign").await.unwrap(),
            ParamValue::Bool(true)
        ));
        // Unknown DCA ids and out-of-scope strips bail locally.
        assert!(driver.get_param(StripId(0), "dca5_assign").await.is_err());
        assert!(
            driver
                .set_param(StripId(30), "dca1_assign", ParamValue::Bool(true))
                .await
                .is_err()
        );
        // LR assign round-trips (untouched strips read assigned).
        assert!(matches!(
            driver.get_param(StripId(0), "lr_assign").await.unwrap(),
            ParamValue::Bool(true)
        ));
        driver
            .set_param(StripId(0), "lr_assign", ParamValue::Bool(false))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "lr_assign").await.unwrap(),
            ParamValue::Bool(false)
        ));
        driver
            .set_param(StripId(0), "lr_assign", ParamValue::Bool(true))
            .await
            .unwrap();
        // Mute-group membership via the driver methods (bit 1; bit 0
        // stays clear; out-of-range groups bail locally).
        assert!(!driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
        driver.set_mute_group_assign(StripId(0), 1, true).await.unwrap();
        assert!(driver.get_mute_group_assign(StripId(0), 1).await.unwrap());
        assert!(!driver.get_mute_group_assign(StripId(0), 0).await.unwrap());
        driver.set_mute_group_assign(StripId(0), 1, false).await.unwrap();
        assert!(!driver.get_mute_group_assign(StripId(0), 1).await.unwrap());
        assert!(driver.get_mute_group_assign(StripId(0), 4).await.is_err());
        assert!(driver.set_mute_group_assign(StripId(0), 4, true).await.is_err());
        // Main-LR (strip 30) and DCA (strip 31) strips carry no `grp`
        // leaf: local bail, never a hardware timeout.
        assert!(driver.get_mute_group_assign(StripId(30), 0).await.is_err());
        assert!(driver.set_mute_group_assign(StripId(31), 0, true).await.is_err());
        // Automix enable preserves the weight (container RMW).
        driver
            .set_param(StripId(0), "automix_weight", ParamValue::Float(0.75))
            .await
            .unwrap();
        driver
            .set_param(StripId(0), "automix_on", ParamValue::Bool(true))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "automix_on").await.unwrap(),
            ParamValue::Bool(true)
        ));
        assert!(matches!(
            driver.get_param(StripId(0), "automix_weight").await.unwrap(),
            ParamValue::Float(w) if (w - 0.75).abs() < 1e-6
        ));
        driver
            .set_param(StripId(0), "automix_on", ParamValue::Bool(false))
            .await
            .unwrap();
        assert!(matches!(
            driver.get_param(StripId(0), "automix_on").await.unwrap(),
            ParamValue::Bool(false)
        ));
        assert!(matches!(
            driver.get_param(StripId(0), "automix_weight").await.unwrap(),
            ParamValue::Float(w) if (w - 0.75).abs() < 1e-6
        ));
    }

    /// Clear-solo action: fire-and-forget set ACKed by silence, query reads
    /// back int 0 (nothing soloed) — the live-desk shape.
    #[tokio::test]
    async fn driver_clear_solo_fire_and_forget() {
        let (driver, _) = connected_driver().await;
        driver.clear_solo().await.unwrap();
    }

    /// Rig-gated solo write-path discovery (env `OPENMIX_RIG=1`, desk
    /// address via `OPENMIX_RIG_ADDR`; skipped otherwise so CI stays
    /// green). Uses muted
    /// ch16 only, read -> write -> readback -> clear -> verify. Reports
    /// which `/-stat/solosw/NN` numbering the desk honors (1-based `16`
    /// vs 0-based `15`) — run with `-- --nocapture` to see the verdict.
    /// PFL only, cleared immediately; never run while the rig is live.
    #[tokio::test]
    async fn rig_solo_write_path() {
        if std::env::var("OPENMIX_RIG").as_deref() != Ok("1") {
            return;
        }
        let driver = XAirDriver::new();
        let rig = std::env::var("OPENMIX_RIG_ADDR").unwrap_or_else(|_| "192.168.1.20".to_string());
        driver.connect(&format!("{rig}:10024")).await.unwrap();
        let bank = async || {
            let guard = driver.session().await.unwrap();
            let reply = guard
                .as_ref()
                .unwrap()
                .query("/-stat/solosw")
                .await
                .unwrap();
            reply
                .args
                .iter()
                .map(|a| match a {
                    rosc::OscType::Int(i) => *i,
                    _ => -999,
                })
                .collect::<Vec<_>>()
        };
        let set_idx = async |nn: u8, v: i32| {
            let guard = driver.session().await.unwrap();
            guard
                .as_ref()
                .unwrap()
                .send(&format!("/-stat/solosw/{nn:02}"), vec![rosc::OscType::Int(v)])
                .await
                .unwrap();
        };
        let b0 = bank().await;
        println!("rig solosw bank len: {}", b0.len());
        println!("baseline nonzero: {:?}", b0.iter().enumerate().filter(|(_, &v)| v != 0).collect::<Vec<_>>());
        // Hypothesis A: 1-based (ch16 -> /16, bank idx 15).
        set_idx(16, 1).await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let ba = bank().await;
        println!("after /16=1: {:?}", ba.iter().enumerate().filter(|(_, &v)| v != 0).collect::<Vec<_>>());
        driver.clear_solo().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        // Hypothesis B: 0-based (ch16 -> /15, bank idx 15).
        set_idx(15, 1).await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let bb = bank().await;
        println!("after /15=1: {:?}", bb.iter().enumerate().filter(|(_, &v)| v != 0).collect::<Vec<_>>());
        driver.clear_solo().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let b1 = bank().await;
        assert!(b1.iter().all(|&v| v == 0), "rig solos not cleared: {b1:?}");
    }

    /// Main LR mute (`/lr/mix/on`): same-value writes round-trip bit-exact
    /// against the strict fake (the live desk answers this leaf, rig-verified
    /// XR18 FW 1.20 2026-10-18: reads `1` unmuted).
    #[tokio::test]
    async fn driver_main_mute_roundtrip() {
        let (driver, _) = connected_driver().await;
        driver.set_main_mute(false).await.unwrap();
        assert!(!driver.get_main_mute().await.unwrap());
        driver.set_main_mute(true).await.unwrap();
        assert!(driver.get_main_mute().await.unwrap());
    }

    /// Per-channel solo (`/-stat/solosw/NN` set, `/-stat/solosw` bank
    /// readback): round-trips against the strict fake's 54-int bank. Non-
    /// channel strips bail locally (no verified desk leaf — never faked).
    #[tokio::test]
    async fn driver_solo_roundtrip() {
        let (driver, _) = connected_driver().await;
        assert!(!driver.get_solo(StripId(15)).await.unwrap());
        driver.set_solo(StripId(15), true).await.unwrap();
        assert!(driver.get_solo(StripId(15)).await.unwrap());
        assert!(!driver.get_solo(StripId(0)).await.unwrap());
        driver.clear_solo().await.unwrap();
        // Bus/main/DCA strips have no verified solo leaf.
        assert!(driver.set_solo(StripId(20), true).await.is_err());
        assert!(driver.set_solo(StripId(30), true).await.is_err());
        assert!(driver.get_solo(StripId(20)).await.is_err());
    }

    /// Meter subscription fills the cache that `poll_meters` serves
    /// strip-ordered: the strict fake streams `[0.1, 0.5, 0.9]` (a legacy
    /// blob with no count word, so transport keeps the plain LE-i16 path),
    /// which must land verbatim on strips 0..3 while the aux strip stays
    /// silent (a 3-level frame has no idx 26).
    #[tokio::test]
    async fn driver_meter_cache_fills() {
        let (driver, _) = connected_driver().await;
        assert!(driver.poll_meters().levels.iter().all(|l| *l == 0.0));
        let _rx = driver.subscribe_meters().await.unwrap();
        let t0 = std::time::Instant::now();
        loop {
            if driver.poll_meters().levels.iter().any(|l| *l > 0.0)
                || t0.elapsed() > std::time::Duration::from_secs(3)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        let levels = driver.poll_meters().levels;
        assert_eq!(levels.len(), XAIR_STRIPS);
        assert!((levels[0] - 0.1).abs() < 1e-4);
        assert!((levels[1] - 0.5).abs() < 1e-4);
        assert!((levels[2] - 0.9).abs() < 1e-4);
        assert_eq!(levels[35], 0.0);
    }

    /// Connect-time port sweep stays sub-second and honors explicit ports:
    /// `probe_port` sees the strict fake, `has_explicit_port` gates the
    /// sweep, and `connect()` with an explicit port dials it directly.
    #[test]
    fn explicit_port_detection() {
        assert!(has_explicit_port("192.168.1.20:10024"));
        assert!(has_explicit_port("192.168.1.20:10023"));
        assert!(!has_explicit_port("192.168.1.20"));
        assert!(!has_explicit_port("mixer.lan"));
    }

    #[tokio::test]
    async fn connect_probe_sees_fake_fast() {
        let (addr, _fake) = FakeMixer::spawn().await;
        let ip = addr.ip().to_string();
        let t0 = std::time::Instant::now();
        assert!(probe_port(&ip, addr.port()).await);
        assert!(t0.elapsed() < addrs::CONNECT_PROBE_TIMEOUT);
        // Unused loopback port: connection-refused (or silence on wifi) —
        // either way the probe fails fast, never the steady-state 2 s.
        let probe = async {
            let sock = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
            sock.local_addr().unwrap().port()
        };
        let free = probe.await;
        let t0 = std::time::Instant::now();
        assert!(!probe_port("127.0.0.1", free).await);
        assert!(t0.elapsed() < std::time::Duration::from_secs(2));
    }

    #[tokio::test]
    async fn connect_explicit_port_dials_directly() {
        let (addr, _fake) = FakeMixer::spawn().await;
        let driver = XAirDriver::new();
        driver.connect(&addr.to_string()).await.unwrap();
        let info = driver.query_info().await.unwrap();
        assert_eq!(info.model, "XR18");
    }
    /// like their `get_param` arms (same codecs, engineering units), and
    /// out-of-scope shapes stay silent. Pure unit test (no sockets): the
    /// messages below are the wire shapes the desk (and the strict fake)
    /// emits.
    #[test]
    fn inbound_covers_send_level_gain_color() {
        let msg = |addr: &str, args: Vec<OscType>| OscMessage {
            addr: addr.to_string(),
            args,
        };
        // Bus-send slot 1 -> `aux1_level` (dB via the X-Air fader law).
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/01/mix/01/level",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("send level parses");
        assert_eq!((strip, param), (0, "aux1_level"));
        assert!(matches!(value, ParamValue::Float(v) if (v - mix::wire_to_send_db(0.5).unwrap()).abs() < 1e-6));
        // FX-send slot 7 -> `fx1_level`.
        let (strip, param, _) = parse_inbound(&msg(
            "/ch/02/mix/07/level",
            vec![OscType::Float(0.25)],
        )).into_iter().next().expect("fx send level parses");
        assert_eq!((strip, param), (1, "fx1_level"));
        // Headamp gain -> dB like the `gain` arm.
        let (strip, param, value) = parse_inbound(&msg(
            "/headamp/01/gain",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("headamp gain parses");
        assert_eq!((strip, param), (0, "gain"));
        assert!(matches!(value, ParamValue::Float(v) if (v - 24.0).abs() < 1e-5));
        // Strip + LR color -> float enum like the `color` arm.
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/01/config/color",
            vec![OscType::Int(3)],
        )).into_iter().next().expect("strip color parses");
        assert_eq!((strip, param), (0, "color"));
        assert!(matches!(value, ParamValue::Float(v) if v == 3.0));
        let (strip, param, _) = parse_inbound(&msg(
            "/lr/config/color",
            vec![OscType::Int(7)],
        )).into_iter().next().expect("lr color parses");
        assert_eq!((strip, param), (LR_STRIP, "color"));
        // Out-of-scope shapes stay silent: bus roots carry no send slots,
        // headamp ends at ch16, slots end at 10, icon has no arm.
        assert!(parse_inbound(&msg("/bus/1/mix/01/level", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/headamp/17/gain", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/ch/01/mix/11/level", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/ch/01/config/icon", vec![OscType::Int(1)])).is_empty());
    }

    /// Desk-originated solo updates ride `/-stat/solosw/NN` (the same
    /// 1-based bank `set_solo`/`get_solo` drive): soloed ch1 decodes to
    /// strip 0 `solo` true, cleared ch16 to strip 15 false. Out-of-range
    /// leaves and the bare bank-query shape stay silent (XR18: channels
    /// 1..=16 only); the clear-solo action leaf is not a state frame.
    #[test]
    fn inbound_covers_solosw_bank() {
        let msg = |addr: &str, args: Vec<OscType>| OscMessage {
            addr: addr.to_string(),
            args,
        };
        let (strip, param, value) = parse_inbound(&msg(
            "/-stat/solosw/01",
            vec![OscType::Int(1)],
        )).into_iter().next().expect("solosw on parses");
        assert_eq!((strip, param), (0, "solo"));
        assert!(matches!(value, ParamValue::Bool(true)));
        let (strip, param, value) = parse_inbound(&msg(
            "/-stat/solosw/16",
            vec![OscType::Int(0)],
        )).into_iter().next().expect("solosw off parses");
        assert_eq!((strip, param), (15, "solo"));
        assert!(matches!(value, ParamValue::Bool(false)));
        assert!(parse_inbound(&msg("/-stat/solosw/17", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/-stat/solosw/00", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/-stat/solosw", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/-action/clearsolo", vec![OscType::Int(1)])).is_empty());
    }

    /// Desk-originated assign-plane updates (`{root}/mix/lr`,
    /// `{root}/grp/dca`, `{root}/automix`): decoded like the matching
    /// `get_param` arms. `grp/dca` fans out to one update per DCA id, the
    /// automix container to enable + weight; `grp/mute` stays silent (no
    /// catalogue ids — membership rides `MuteGroupAssign` events).
    #[test]
    fn inbound_covers_assign_plane() {
        let msg = |addr: &str, args: Vec<OscType>| OscMessage {
            addr: addr.to_string(),
            args,
        };
        // LR assign -> bool like the `lr_assign` arm.
        let updates = parse_inbound(&msg("/ch/01/mix/lr", vec![OscType::Int(0)]));
        assert_eq!(updates.len(), 1);
        assert_eq!((updates[0].0, updates[0].1), (0, "lr_assign"));
        assert!(matches!(updates[0].2, ParamValue::Bool(false)));
        // DCA bitmask -> one bool per DCA id (bits 0 + 2 set).
        let updates = parse_inbound(&msg("/ch/01/grp/dca", vec![OscType::Int(5)]));
        assert_eq!(updates.len(), 4);
        for (i, (strip, id, value)) in updates.iter().enumerate() {
            assert_eq!((*strip, *id), (0, DCA_ASSIGN_IDS[i]));
            assert!(matches!(value, ParamValue::Bool(b) if *b == (i == 0 || i == 2)));
        }
        // Automix container -> enable + weight.
        let updates = parse_inbound(&msg(
            "/ch/01/automix",
            vec![OscType::Int(1), OscType::Float(0.75)],
        ));
        assert_eq!(updates.len(), 2);
        assert_eq!((updates[0].0, updates[0].1), (0, "automix_on"));
        assert!(matches!(updates[0].2, ParamValue::Bool(true)));
        assert_eq!((updates[1].0, updates[1].1), (0, "automix_weight"));
        assert!(matches!(updates[1].2, ParamValue::Float(w) if (w - 0.75).abs() < 1e-6));
        // Mute-group bitmask: no catalogue ids, stays silent here.
        assert_eq!(parse_inbound(&msg("/ch/01/grp/mute", vec![OscType::Int(3)])).len(), 4);
        // Silent roots stay silent (no `grp`/`lr` leaf on the desk).
        assert!(parse_inbound(&msg("/lr/grp/dca", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/dca/1/grp/dca", vec![OscType::Int(1)])).is_empty());
    }

    /// Desk-originated EQ updates (`{root}/eq/on`, `{root}/eq/{band}/{g|f|q/type}`):
    /// decoded like the `get_param` eq arms (same codecs, engineering
    /// units). Channels/FX returns cap at 4 bands, buses/main LR at 6
    /// (mirroring `check_eq_band`); FX-send/DCA roots and out-of-range
    /// bands stay silent. Pure unit test (no sockets): the messages below
    /// are the wire shapes the desk broadcasts to `/xremote` subscribers.
    #[test]
    fn inbound_covers_eq_band_leaves() {
        let msg = |addr: &str, args: Vec<OscType>| OscMessage {
            addr: addr.to_string(),
            args,
        };
        // Channel band gain -> dB like the `eq{b}_gain` arm.
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/01/eq/1/g",
            vec![OscType::Float(0.75)],
        )).into_iter().next().expect("eq gain parses");
        assert_eq!((strip, param), (0, "eq1_gain"));
        assert!(matches!(value, ParamValue::Float(v) if (v - eq::wire_to_gain_db(0.75).unwrap()).abs() < 1e-6));
        // Channel band freq/q/type ride their own codecs.
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/02/eq/2/f",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("eq freq parses");
        assert_eq!((strip, param), (1, "eq2_freq"));
        assert!(matches!(value, ParamValue::Float(v) if (v - eq::wire_to_freq_hz(0.5).unwrap()).abs() < 1e-6));
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/03/eq/3/q",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("eq q parses");
        assert_eq!((strip, param), (2, "eq3_q"));
        assert!(matches!(value, ParamValue::Float(v) if (v - eq::wire_to_q(0.5).unwrap()).abs() < 1e-6));
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/04/eq/4/type",
            vec![OscType::Int(2)],
        )).into_iter().next().expect("eq type parses");
        assert_eq!((strip, param), (3, "eq4_type"));
        assert!(matches!(value, ParamValue::Float(v) if v == 2.0));
        // FX-return band gain (strip 16 = `/rtn/1`, 4-band family).
        let (strip, param, _) = parse_inbound(&msg(
            "/rtn/1/eq/1/g",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("fxret eq gain parses");
        assert_eq!((strip, param), (16, "eq1_gain"));
        // Bus bands 5+6 answer (strip 20 = `/bus/1`, 6-band family).
        let (strip, param, _) = parse_inbound(&msg(
            "/bus/1/eq/5/g",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("bus eq5 gain parses");
        assert_eq!((strip, param), (20, "eq5_gain"));
        let (strip, param, _) = parse_inbound(&msg(
            "/bus/1/eq/6/type",
            vec![OscType::Int(5)],
        )).into_iter().next().expect("bus eq6 type parses");
        assert_eq!((strip, param), (20, "eq6_type"));
        // Main LR EQ (strip 30 = `/lr`).
        let (strip, param, _) = parse_inbound(&msg(
            "/lr/eq/2/g",
            vec![OscType::Float(0.25)],
        )).into_iter().next().expect("lr eq gain parses");
        assert_eq!((strip, param), (LR_STRIP, "eq2_gain"));
        // EQ on/off decodes like the `eq_on` arm.
        let (strip, param, value) = parse_inbound(&msg(
            "/ch/01/eq/on",
            vec![OscType::Int(1)],
        )).into_iter().next().expect("eq on parses");
        assert_eq!((strip, param), (0, "eq_on"));
        assert!(matches!(value, ParamValue::Bool(true)));
        let (strip, param, _) = parse_inbound(&msg(
            "/lr/eq/on",
            vec![OscType::Int(0)],
        )).into_iter().next().expect("lr eq on parses");
        assert_eq!((strip, param), (LR_STRIP, "eq_on"));
        // Out-of-scope shapes stay silent: band 5+ on a 4-band channel,
        // band 0/7+, FX-send/DCA roots (no EQ there), non-EQ `/type`
        // leaves (FX type select), and garbage roots.
        assert!(parse_inbound(&msg("/ch/01/eq/5/g", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/ch/01/eq/0/g", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/ch/01/eq/7/g", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/fxsend/1/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/dca/1/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/dca/1/eq/on", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/fx/1/type", vec![OscType::Int(3)])).is_empty());
        assert!(!parse_inbound(&msg("/ch/01/gate/filter/type", vec![OscType::Int(1)])).is_empty());
        assert!(parse_inbound(&msg("/ch/99/eq/1/g", vec![OscType::Float(0.5)])).is_empty());
    }

    /// Desk-broadcast ingest for EQ over the real `connect()` path: an
    /// unsolicited `/ch/01/eq/1/g` datagram from the fake (no query asked
    /// for it — the desk-broadcast shape) must surface on the driver's
    /// update channel as `eq1_gain` in engineering units, like the polled
    /// `get_param` arm. Covers the connect-spawned ingest wiring, not just
    /// the pure `parse_inbound` decode above.
    #[tokio::test]
    async fn inbound_eq_gain_reaches_update_channel() {
        let (addr, fake) = FakeMixer::spawn().await;
        let driver = XAirDriver::new();
        driver.connect(&addr.to_string()).await.unwrap();
        let mut rx = driver.subscribe_updates().expect("driver has update channel");
        // One query so the fake learns our peer (connect's best-effort
        // /info ping already did, but be explicit), then fire the
        // unsolicited broadcast the desk would emit on a surface move.
        let _ = driver.get_fader(StripId(0)).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        fake.broadcast("/ch/01/eq/1/g", vec![OscType::Float(0.75)]).await;
        let deadline = std::time::Duration::from_secs(2);
        let t0 = std::time::Instant::now();
        loop {
            let update = tokio::time::timeout(deadline - t0.elapsed(), rx.recv())
                .await
                .expect("eq broadcast not ingested in time")
                .expect("update channel closed");
            if update.strip.0 == 0 && update.param == "eq1_gain" {
                assert!(
                    matches!(update.value, ParamValue::Float(v) if (v - 7.5).abs() < 1e-5),
                    "eq1_gain decoded off-codec: {:?}",
                    update.value
                );
                return;
            }
        }
    }

    /// Main LR must go through `/lr/mix/fader`: the strict fake below answers
    /// ONLY that address and stays silent on `/main/st/mix/fader`, mirroring
    /// live XR18 hardware (FW 1.20, 2026-10-17) where X32-style `/main/st`
    /// gets NO reply. A driver still querying `/main/st` would time out here.
    #[tokio::test]
    async fn main_fader_uses_lr_not_main_st() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let mixer_addr = socket.local_addr().unwrap();
        let seen = Arc::new(StdMutex::new(Vec::<String>::new()));
        let seen_worker = seen.clone();
        let worker = socket.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            let mut level = 0.8f32;
            loop {
                let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    if msg.addr == "/xremote" {
                        continue;
                    }
                    seen_worker.lock().unwrap().push(msg.addr.clone());
                    match (msg.addr.as_str(), msg.args.first()) {
                        ("/lr/mix/fader", Some(OscType::Float(f))) => level = *f,
                        ("/lr/mix/fader", None) => {
                            let bytes =
                                osc::encode_message("/lr/mix/fader", vec![OscType::Float(level)])
                                    .unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                        // Deliberately NO reply for /main/st/mix/fader: the
                        // real mixer stays silent there.
                        _ => {}
                    }
                }
            }
        });

        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(mixer_addr).await.unwrap();
        let session =
            OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
        let driver = XAirDriver::new();
        *driver.session.lock().await = Some(std::sync::Arc::new(session));

        driver.set_main_fader(0.66).await.unwrap();
        let v = tokio::time::timeout(std::time::Duration::from_secs(2), driver.get_main_fader())
            .await
            .expect("main fader reply in time")
            .unwrap();
        assert!(
            (v - 0.66).abs() < addrs::FADER_READBACK_EPSILON,
            "main fader readback {v} outside hardware epsilon"
        );

        let addrs = seen.lock().unwrap();
        assert!(
            addrs.iter().any(|a| a == "/lr/mix/fader"),
            "driver never queried /lr/mix/fader: {addrs:?}"
        );
        assert!(
            !addrs.iter().any(|a| a == "/main/st/mix/fader"),
            "driver queried X32-style /main/st/mix/fader: {addrs:?}"
        );
    }

    /// Buses must go through single-digit `/bus/1/…`: the strict fake below
    /// answers ONLY that address and stays silent on `/bus/01/…`, mirroring
    /// live XR18 hardware (FW 1.20, T2). A driver still zero-padding would
    /// time out here.
    #[tokio::test]
    async fn bus_fader_uses_single_digit_not_padded() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let mixer_addr = socket.local_addr().unwrap();
        let seen = Arc::new(StdMutex::new(Vec::<String>::new()));
        let seen_worker = seen.clone();
        let worker = socket.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            let mut level = 0.7f32;
            loop {
                let Ok((n, peer)) = worker.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    if msg.addr == "/xremote" {
                        continue;
                    }
                    seen_worker.lock().unwrap().push(msg.addr.clone());
                    match (msg.addr.as_str(), msg.args.first()) {
                        ("/bus/1/mix/fader", Some(OscType::Float(f))) => level = *f,
                        ("/bus/1/mix/fader", None) => {
                            let bytes =
                                osc::encode_message("/bus/1/mix/fader", vec![OscType::Float(level)])
                                    .unwrap();
                            let _ = worker.send_to(&bytes, peer).await;
                        }
                        // Deliberately NO reply for /bus/01/mix/fader: the
                        // real mixer stays silent there.
                        _ => {}
                    }
                }
            }
        });

        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(mixer_addr).await.unwrap();
        let session =
            OscSession::from_connected_socket(sock, Some(KeepaliveConfig::xremote()));
        let driver = XAirDriver::new();
        *driver.session.lock().await = Some(std::sync::Arc::new(session));

        driver.set_bus_fader(0, 0.33).await.unwrap();
        let v = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            driver.get_bus_fader(0),
        )
        .await
        .expect("bus fader reply in time")
        .unwrap();
        assert!(
            (v - 0.33).abs() < addrs::FADER_READBACK_EPSILON,
            "bus fader readback {v} outside hardware epsilon"
        );

        let addrs = seen.lock().unwrap();
        assert!(
            addrs.iter().any(|a| a == "/bus/1/mix/fader"),
            "driver never queried /bus/1/mix/fader: {addrs:?}"
        );
        assert!(
            !addrs.iter().any(|a| a == "/bus/01/mix/fader"),
            "driver queried zero-padded /bus/01/mix/fader: {addrs:?}"
        );
    }

    #[tokio::test]
    async fn driver_channel_count_is_per_model() {
        // Default driver is the hardware-verified XR18 geometry: 16.
        let xr18 = XAirDriver::new();
        assert_eq!(xr18.channel_count(), 16);
        // X32-kind maps keep the 32-channel bound (not hardware-verified).
        let x32kind = XAirDriver::with_model(addrs::XAirModel::X32Kind);
        assert_eq!(x32kind.channel_count(), 32);

        let (driver, _) = connected_driver().await;
        // v2 topology (36 strips): strips 0..16 stay channels (`/ch/01..16`,
        // rig T3); strips 16..35 are the FX-return (`/rtn/N`)/bus/FX-send/
        // LR/DCA roots, all live-verified; strip 35 is the stereo aux
        // input (`/rtn/aux`). The strip space now ends at 35 — not 15.
        assert!(driver.set_fader(StripId(15), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(16), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(34), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(35), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(36), 0.5).await.is_err());
        assert!(driver.get_mute(StripId(99)).await.is_err());
        assert!(driver.set_bus_fader(6, 0.5).await.is_err());
        assert!(driver.get_bus_fader(6).await.is_err());
    }

    #[tokio::test]
    async fn driver_rejects_out_of_range_strips() {
        let (driver, _) = connected_driver().await;
        // v2: the strip space ends at 35 (16 ch + 4 ret + 6 bus + 4 fxsnd +
        // LR + 4 DCA + aux); 32 is a DCA strip, 36+ is out of range.
        assert!(driver.set_fader(StripId(32), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(35), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(36), 0.5).await.is_err());
        assert!(driver.get_mute(StripId(99)).await.is_err());
        assert!(driver.set_bus_fader(6, 0.5).await.is_err());
        assert!(driver.get_bus_fader(6).await.is_err());
    }

    /// Aux return strip (`/rtn/aux`, the XR18's 17th strip, T3): fader, mute
    /// and name round-trip like a channel (baseline §2 values reply there).
    #[tokio::test]
    async fn driver_aux_strip_roundtrip() {
        let (driver, _) = connected_driver().await;
        driver.set_aux_fader(0.5).await.unwrap();
        let v = driver.get_aux_fader().await.unwrap();
        assert!(
            (v - 0.5).abs() < addrs::FADER_READBACK_EPSILON,
            "aux fader readback {v} outside hardware epsilon"
        );
        driver.set_aux_mute(true).await.unwrap();
        assert!(driver.get_aux_mute().await.unwrap());
        driver.set_aux_mute(false).await.unwrap();
        assert!(!driver.get_aux_mute().await.unwrap());
        driver.set_aux_name("COMP LR").await.unwrap();
        assert_eq!(driver.get_aux_name().await.unwrap(), "COMP LR");
    }

    /// Aux input as a first-class strip (strip 35 = `/rtn/aux`, owner-rig
    /// verified 2026-10-20): fader/mute/name/pan/color/eq ride the same
    /// `set_param`/`get_param` path as channels 1-16, and desk broadcasts
    /// decode via `parse_inbound` to strip 35.
    #[tokio::test]
    async fn driver_aux_strip_param_path() {
        let (driver, _) = connected_driver().await;
        let aux = StripId(35);
        driver
            .set_param(aux, "fader", ParamValue::Float(0.5))
            .await
            .unwrap();
        let ParamValue::Float(v) = driver.get_param(aux, "fader").await.unwrap() else {
            panic!("aux fader read back non-float");
        };
        assert!(
            (v - 0.5).abs() < addrs::FADER_READBACK_EPSILON,
            "aux strip fader readback {v} outside hardware epsilon"
        );
        driver
            .set_param(aux, "mute", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(aux, "mute").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(aux, "mute", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(aux, "mute").await.unwrap(),
            ParamValue::Bool(false)
        );
        driver
            .set_param(aux, "name", ParamValue::Text("AUX IN".to_string()))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(aux, "name").await.unwrap(),
            ParamValue::Text("AUX IN".to_string())
        );
        driver
            .set_param(aux, "pan", ParamValue::Float(0.0))
            .await
            .unwrap();
        let ParamValue::Float(pan) = driver.get_param(aux, "pan").await.unwrap() else {
            panic!("aux pan read back non-float");
        };
        assert!((pan - 0.0).abs() < 1e-4, "aux pan readback {pan}");
        driver
            .set_param(aux, "eq1_gain", ParamValue::Float(3.0))
            .await
            .unwrap();
        let ParamValue::Float(g) = driver.get_param(aux, "eq1_gain").await.unwrap() else {
            panic!("aux eq1_gain read back non-float");
        };
        assert!((g - 3.0).abs() < 1e-4, "aux eq1_gain readback {g}");
        // Aux carries 4 EQ bands like the FX returns: band 5+ is
        // scope-rejected driver-side, never hitting a silent address.
        assert!(
            driver
                .set_param(aux, "eq5_gain", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        // Input-scoped leaves stay out of scope on the aux return.
        assert!(driver.get_param(aux, "gain").await.is_err());
        assert!(driver.get_param(aux, "phantom").await.is_err());
        // Channel sends are channel-scoped: aux sends ride no leaf here.
        assert!(
            driver
                .set_param(aux, "aux1_level", ParamValue::Float(-10.0))
                .await
                .is_err()
        );
    }

    #[test]
    fn inbound_covers_rtn_aux() {
        let msg = |addr: &str, args: Vec<OscType>| OscMessage {
            addr: addr.to_string(),
            args,
        };
        let (strip, param, _) = parse_inbound(&msg(
            "/rtn/aux/mix/fader",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("aux fader parses");
        assert_eq!((strip, param), (35, "fader"));
        let (strip, param, value) = parse_inbound(&msg(
            "/rtn/aux/mix/on",
            vec![OscType::Int(0)],
        )).into_iter().next().expect("aux mute parses");
        assert_eq!((strip, param), (35, "mute"));
        assert!(matches!(value, ParamValue::Bool(true)));
        let (strip, param, _) = parse_inbound(&msg(
            "/rtn/aux/config/name",
            vec![OscType::String("COMP LR".to_string())],
        )).into_iter().next().expect("aux name parses");
        assert_eq!((strip, param), (35, "name"));
        let (strip, param, _) = parse_inbound(&msg(
            "/rtn/aux/eq/1/g",
            vec![OscType::Float(0.5)],
        )).into_iter().next().expect("aux eq gain parses");
        assert_eq!((strip, param), (35, "eq1_gain"));
        // No phantom `/aux/*` or `/auxin/*` roots: only `/rtn/aux` decodes.
        assert!(parse_inbound(&msg("/aux/mix/fader", vec![OscType::Float(0.5)])).is_empty());
        assert!(parse_inbound(&msg("/auxin/mix/fader", vec![OscType::Float(0.5)])).is_empty());
    }

    /// Headamp gain/phantom loopback over UDP (`crate::headamp`,
    /// community-documented via xair-api-python, NOT owner-rig verified):
    /// dB gain maps linearly onto the wire float, phantom rides int 1/0.
    #[tokio::test]
    async fn driver_headamp_gain_phantom_roundtrip() {
        let (driver, _) = connected_driver().await;
        // StripId is 0-based: strip 0 drives wire `/headamp/01/…`.
        driver
            .set_param(StripId(0), "gain", ParamValue::Float(24.0))
            .await
            .unwrap();
        let ParamValue::Float(db) = driver.get_param(StripId(0), "gain").await.unwrap()
        else {
            panic!("headamp gain read back non-float");
        };
        // The fake stores verbatim; epsilon covers f32 conversion only —
        // hardware quantization is uncharacterized (see `crate::headamp`).
        assert!((db - 24.0).abs() < 0.05, "headamp gain readback {db}");
        // Range and kind enforced locally, never sent.
        assert!(
            driver
                .set_param(StripId(0), "gain", ParamValue::Float(61.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "gain", ParamValue::Bool(true))
                .await
                .is_err()
        );

        driver
            .set_param(StripId(0), "phantom", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "phantom").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(StripId(0), "phantom", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "phantom").await.unwrap(),
            ParamValue::Bool(false)
        );
        assert!(
            driver
                .set_param(StripId(0), "phantom", ParamValue::Float(1.0))
                .await
                .is_err()
        );

        // Phantom strips stay local: no headamp past the model bound (T3).
        assert!(
            driver
                .set_param(StripId(16), "gain", ParamValue::Float(24.0))
                .await
                .is_err()
        );
        assert!(driver.get_param(StripId(16), "phantom").await.is_err());
        // Unknown params still rejected — including `inputsource`
        // (`/ch/NN/config/insrc`), which is live-silent on this rig
        // (see `crate::input`), and `usbinput` (`/ch/NN/preamp/rtnsw`).
        assert!(driver.get_param(StripId(0), "inputsource").await.is_err());
        assert!(driver.get_param(StripId(0), "usbinput").await.is_err());
        // Send mutes + aux7/8 stay unsupported: the firmware map has no
        // send-mute leaf and `/ch/NN/mix/MM/on` is live-silent (T7), and
        // only 6 bus sends exist — so these fail as unknown params.
        assert!(driver.get_param(StripId(0), "aux1_mute").await.is_err());
        assert!(driver.get_param(StripId(0), "fx1_mute").await.is_err());
        assert!(driver.get_param(StripId(0), "aux7_level").await.is_err());
        assert!(driver.get_param(StripId(0), "aux8_level").await.is_err());
        assert!(
            driver
                .set_param(StripId(0), "aux1_mute", ParamValue::Bool(true))
                .await
                .is_err()
        );
    }

    /// Input routing loopback over UDP (`crate::input`, owner-rig verified
    /// 2026-10-18 on XR18 FW 1.20): usb-return source, USB trim, phase
    /// invert, HPF on/off + corner — set/get plus local validation.
    /// `inputsource`/`usbinput` stay errors (live-silent, see `crate::input`).
    #[tokio::test]
    async fn driver_input_routing_roundtrip() {
        let (driver, _) = connected_driver().await;
        // StripId is 0-based: strip 0 drives wire `/ch/01/…`.
        // usbreturn: whole-number floats 0..=17 (firmware U01..U18).
        driver
            .set_param(StripId(0), "usbreturn", ParamValue::Float(17.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "usbreturn").await.unwrap(),
            ParamValue::Float(17.0)
        );
        driver
            .set_param(StripId(0), "usbreturn", ParamValue::Float(0.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "usbreturn").await.unwrap(),
            ParamValue::Float(0.0)
        );
        for bad in [18.0, -1.0, 3.5, f32::NAN, f32::INFINITY] {
            assert!(
                driver
                    .set_param(StripId(0), "usbreturn", ParamValue::Float(bad))
                    .await
                    .is_err(),
                "usbreturn {bad} accepted"
            );
        }
        assert!(
            driver
                .set_param(StripId(0), "usbreturn", ParamValue::Bool(true))
                .await
                .is_err()
        );

        // trim: dB −18..=18, linear wire (the fake stores verbatim, so
        // epsilon covers f32 conversion only).
        driver
            .set_param(StripId(0), "trim", ParamValue::Float(0.0))
            .await
            .unwrap();
        let ParamValue::Float(db) = driver.get_param(StripId(0), "trim").await.unwrap()
        else {
            panic!("trim read back non-float");
        };
        assert!((db - 0.0).abs() < 1e-4, "trim readback {db}");
        driver
            .set_param(StripId(0), "trim", ParamValue::Float(-18.0))
            .await
            .unwrap();
        let ParamValue::Float(db) = driver.get_param(StripId(0), "trim").await.unwrap()
        else {
            panic!("trim read back non-float");
        };
        assert!((db + 18.0).abs() < 1e-4, "trim readback {db}");
        assert!(
            driver
                .set_param(StripId(0), "trim", ParamValue::Float(18.1))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "trim", ParamValue::Bool(false))
                .await
                .is_err()
        );

        // polarity (phase invert): plain bool, NOT inverted like /mix/on.
        driver
            .set_param(StripId(0), "polarity", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "polarity").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(StripId(0), "polarity", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "polarity").await.unwrap(),
            ParamValue::Bool(false)
        );
        assert!(
            driver
                .set_param(StripId(0), "polarity", ParamValue::Float(1.0))
                .await
                .is_err()
        );

        // hpf_on + hpf_freq (log 20..400 Hz; 1 Hz epsilon for log/f32).
        driver
            .set_param(StripId(0), "hpf_on", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "hpf_on").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(StripId(0), "hpf_freq", ParamValue::Float(80.0))
            .await
            .unwrap();
        let ParamValue::Float(hz) = driver.get_param(StripId(0), "hpf_freq").await.unwrap()
        else {
            panic!("hpf_freq read back non-float");
        };
        assert!((hz - 80.0).abs() < 1.0, "hpf_freq readback {hz}");
        driver
            .set_param(StripId(0), "hpf_freq", ParamValue::Float(400.0))
            .await
            .unwrap();
        let ParamValue::Float(hz) = driver.get_param(StripId(0), "hpf_freq").await.unwrap()
        else {
            panic!("hpf_freq read back non-float");
        };
        assert!((hz - 400.0).abs() < 1.0, "hpf_freq readback {hz}");
        assert!(
            driver
                .set_param(StripId(0), "hpf_freq", ParamValue::Float(19.9))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "hpf_freq", ParamValue::Float(f32::NAN))
                .await
                .is_err()
        );

        // v2: strip 16 is an FX return (input-scoped leaves like trim stay
        // out-of-scope errors there); the strip space ends at 35.
        assert!(
            driver
                .set_param(StripId(16), "trim", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        assert!(driver.get_param(StripId(16), "polarity").await.is_err());
        assert!(driver.get_param(StripId(0), "inputsource").await.is_err());
        assert!(
            driver
                .set_param(StripId(0), "inputsource", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        assert!(driver.get_param(StripId(0), "usbinput").await.is_err());
    }

    /// FX slots (`/fx/N/par/MM`, baseline §6 row 5): bit-exact loopback
    /// through the core `FxRack` hooks — set/get plus local validation.
    /// Enum pars reply as INT on hardware, so the loopback seeds INT wire
    /// values (multi-option indices >1 fail the old raw float range check).
    #[tokio::test]
    async fn driver_fx_param_roundtrip() {
        let (driver, fake) = connected_driver().await;
        // Slot is 1-based, index 0-based: (1, 0) drives `/fx/1/par/01`.
        driver.set_fx_param(1, 0, 0.5).await.unwrap();
        assert_eq!(driver.get_fx_param(1, 0).await.unwrap(), 0.5);
        driver.set_fx_param(4, 63, 1.0).await.unwrap();
        assert_eq!(driver.get_fx_param(4, 63).await.unwrap(), 1.0);
        // Untouched params read back zero.
        assert_eq!(driver.get_fx_param(2, 10).await.unwrap(), 0.0);
        // Enum par with >2 options (DLY Mode ST/X/M on slot 1): INT wire
        // index 2 maps onto 1.0, not a range-check failure.
        fake.fx_types.lock().unwrap().insert(1, 10);
        fake.fx
            .lock()
            .unwrap()
            .insert("/fx/1/par/03".to_string(), OscType::Int(2));
        assert_eq!(driver.get_fx_param(1, 2).await.unwrap(), 1.0);
        // 14-option Pattern (D/FL par 02 on slot 3): index 1 → 1/13.
        fake.fx_types.lock().unwrap().insert(3, 26);
        fake.fx
            .lock()
            .unwrap()
            .insert("/fx/3/par/02".to_string(), OscType::Int(1));
        let v = driver.get_fx_param(3, 1).await.unwrap();
        assert!((v - 1.0 / 13.0).abs() < 1e-6, "pattern index 1 → {v}");
        // Out-of-range enum index bails, never a fabricated float.
        fake.fx
            .lock()
            .unwrap()
            .insert("/fx/3/par/02".to_string(), OscType::Int(14));
        assert!(driver.get_fx_param(3, 1).await.is_err());
        // Out-of-range addresses/values rejected locally, never sent.
        assert!(driver.set_fx_param(0, 0, 0.5).await.is_err());
        assert!(driver.set_fx_param(5, 0, 0.5).await.is_err());
        assert!(driver.set_fx_param(1, 64, 0.5).await.is_err());
        assert!(driver.set_fx_param(1, 0, 1.5).await.is_err());
        assert!(driver.set_fx_param(1, 0, f32::NAN).await.is_err());
        assert!(driver.get_fx_param(1, 64).await.is_err());
    }

    /// Meter subscription uses the documented `/meters` + stream-id-string
    /// form (T4): the fake streams blobs only after that subscribe, and the
    /// test asserts both the subscribe shape and the parsed frames.
    #[tokio::test]
    async fn driver_meter_subscribe_receives_blob() {
        let (driver, fake) = connected_driver().await;
        let mut rx = driver.subscribe_meters().await.unwrap();
        // The desk streams at ~50ms; expect at least two level frames. The
        // fake also streams ALL DYN blobs now (same subscribe call), which
        // the shared hub surfaces as 41-level frames — skipped here, the
        // DYN path has its own test below.
        let mut seen = 0;
        for _ in 0..8 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("meter frame in time")
                .unwrap();
            if frame.levels.len() != 3 {
                continue;
            }
            assert!((frame.levels[1] - 0.5).abs() < 0.01);
            seen += 1;
            if seen == 2 {
                break;
            }
        }
        assert_eq!(seen, 2, "expected two level frames");
        // The fake only streams after the documented subscribe shape.
        let sub = fake.meter_subscribe.lock().unwrap();
        let sub = sub.as_ref().expect("fake saw no meter subscribe");
        assert_eq!(sub.addr, addrs::METER_REQUEST);
        assert_eq!(
            sub.args,
            vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())]
        );
    }

    /// ALL DYN subscribe uses the same documented `/meters` + string-id
    /// form with the `/meters/6` id (Behringer X AIR OSC doc fw 1.11).
    #[tokio::test]
    async fn driver_meter_dyn_subscribe_shape() {
        let (driver, fake) = connected_driver().await;
        let _rx = driver.subscribe_meters().await.unwrap();
        // Let the fake observe the (two) subscribes.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let sub = fake.dyn_subscribe.lock().unwrap();
        let sub = sub.as_ref().expect("fake saw no dyn subscribe");
        assert_eq!(sub.addr, addrs::METER_REQUEST);
        assert_eq!(
            sub.args,
            vec![OscType::String(addrs::METER_DYN_SUBSCRIBE.to_string())]
        );
    }

    /// Renewal loop: `/xremote` + both `/meters` resubscribes go out on
    /// schedule (INFERRED ~8 s production interval;
    /// [`addrs::METER_RENEW_INTERVAL`]). Short 50 ms interval here keeps the
    /// test fast; the fake tolerates resubscribes by restarting its blob
    /// burst and counting every hit.
    #[tokio::test]
    async fn meter_renewal_resubscribes_on_schedule() {
        use std::time::Duration;
        assert_eq!(
            addrs::METER_RENEW_INTERVAL,
            Duration::from_secs(8),
            "production renewal stays ~8s (INFERRED until rig confirms)"
        );
        let (driver, fake) = connected_driver().await;
        // Session keepalive (`/xremote` every 5 s) fires immediately too —
        // reset its counter so only the renewal loop is measured below.
        tokio::time::sleep(Duration::from_millis(50)).await;
        *fake.xremote_count.lock().unwrap() = 0;
        *fake.meter_count.lock().unwrap() = 0;
        *fake.dyn_count.lock().unwrap() = 0;

        let _rx = driver
            .subscribe_meters_with_interval(Duration::from_millis(50))
            .await
            .unwrap();
        // Initial subscribe + ~4 renewals in 250 ms.
        tokio::time::sleep(Duration::from_millis(250)).await;
        let levels = *fake.meter_count.lock().unwrap();
        let dyns = *fake.dyn_count.lock().unwrap();
        let keepalives = *fake.xremote_count.lock().unwrap();
        assert!(
            levels >= 3,
            "expected >=3 /meters/1 subscribes (initial + renewals), saw {levels}"
        );
        assert!(
            dyns >= 3,
            "expected >=3 /meters/6 subscribes (initial + renewals), saw {dyns}"
        );
        assert!(
            keepalives >= 3,
            "expected >=3 /xremote renewals, saw {keepalives}"
        );
    }

    /// GR end-to-end at the codec level: the fake emits the documented
    /// `/meters/6` shape over UDP loopback, the driver ingests the decoded
    /// message and serves per-channel dB reduction via the param path.
    #[tokio::test]
    async fn driver_dyn_gr_param_loopback() {
        let (driver, fake) = connected_driver().await;
        // No blob yet: no-data error (the UI keeps the static scale), never
        // a fabricated zero.
        assert!(driver.get_param(StripId(0), "gate_gr").await.is_err());
        assert!(driver.get_param(StripId(0), "comp_gr").await.is_err());

        // Fake desk emits the documented shape: ch1 gate 6 dB, ch1 comp
        // 12 dB, ch16 comp 3.5 dB, silence elsewhere.
        let mut m = dyn_gr::DynMeters {
            gate: [0.0; dyn_gr::DYN_CHANNELS],
            comp: [0.0; dyn_gr::DYN_CHANNELS],
            bus: [0.0; dyn_gr::DYN_BUSES],
            lr: 0.0,
        };
        m.gate[0] = 6.0;
        m.comp[0] = 12.0;
        m.comp[15] = 3.5;
        let wire = osc::encode_message(
            addrs::METER_DYN_SUBSCRIBE,
            vec![OscType::Blob(dyn_gr::build_dyn_blob(&m))],
        )
        .unwrap();
        let rx_sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        fake.socket
            .send_to(&wire, rx_sock.local_addr().unwrap())
            .await
            .unwrap();
        let mut buf = vec![0u8; 65535];
        let (n, _) = rx_sock.recv_from(&mut buf).await.unwrap();
        let msgs = osc::flatten(osc::decode_packet(&buf[..n]).unwrap());
        assert_eq!(msgs.len(), 1);
        driver.ingest_dyn_message(&msgs[0]).await.unwrap();

        // Param path serves dB reduction >= 0 per channel.
        let gate = driver.get_param(StripId(0), "gate_gr").await.unwrap();
        let comp = driver.get_param(StripId(0), "comp_gr").await.unwrap();
        let comp16 = driver.get_param(StripId(15), "comp_gr").await.unwrap();
        let ParamValue::Float(gate) = gate else {
            panic!("gate_gr not a float");
        };
        let ParamValue::Float(comp) = comp else {
            panic!("comp_gr not a float");
        };
        let ParamValue::Float(comp16) = comp16 else {
            panic!("comp_gr not a float");
        };
        assert!((gate - 6.0).abs() < 0.01, "gate_gr {gate}");
        assert!((comp - 12.0).abs() < 0.01, "comp_gr {comp}");
        assert!((comp16 - 3.5).abs() < 0.01, "comp_gr ch16 {comp16}");
        // Silent channels read 0 reduction (real zero, post-blob).
        let ParamValue::Float(idle) = driver.get_param(StripId(1), "gate_gr").await.unwrap()
        else {
            panic!("gate_gr not a float");
        };
        assert_eq!(idle, 0.0);
        // Phantom strips (T3) and unknown ids still bail.
        assert!(driver.get_param(StripId(16), "gate_gr").await.is_err());
        assert!(driver.get_param(StripId(0), "nope").await.is_err());
        // Pre-existing arms survive the override.
        let ParamValue::Float(fader) = driver.get_param(StripId(0), "fader").await.unwrap()
        else {
            panic!("fader not a float");
        };
        assert!((fader - 0.75).abs() < addrs::FADER_READBACK_EPSILON);
    }

    /// Non-DYN messages never poison the GR cache: wrong address and
    /// blobless payloads bail, and the cache stays empty.
    #[tokio::test]
    async fn driver_dyn_ingest_rejects_other_shapes() {
        let (driver, _) = connected_driver().await;
        let other = OscMessage {
            addr: addrs::METER_SUBSCRIBE.to_string(),
            args: vec![OscType::Blob(vec![0u8; 8])],
        };
        assert!(driver.ingest_dyn_message(&other).await.is_err());
        let blobless = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args: vec![],
        };
        assert!(driver.ingest_dyn_message(&blobless).await.is_err());
        assert!(driver.last_dyn().await.is_none());
    }

    /// Garbage `/meters/6` payloads (truncated venue datagrams, corrupted
    /// blobs, non-blob args) must bail with `Err`, never panic and never
    /// poison the GR cache; a later valid blob still ingests cleanly.
    #[tokio::test]
    async fn driver_dyn_ingest_rejects_garbage_never_panics() {
        let (driver, _) = connected_driver().await;
        let mut m = dyn_gr::DynMeters {
            gate: [0.0; dyn_gr::DYN_CHANNELS],
            comp: [0.0; dyn_gr::DYN_CHANNELS],
            bus: [0.0; dyn_gr::DYN_BUSES],
            lr: 0.0,
        };
        m.gate[0] = 6.0;
        let good = dyn_gr::build_dyn_blob(&m);
        // Every truncation of a valid blob bails.
        for len in 0..good.len() {
            let msg = OscMessage {
                addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
                args: vec![OscType::Blob(good[..len].to_vec())],
            };
            assert!(
                driver.ingest_dyn_message(&msg).await.is_err(),
                "truncated len {len} ingested"
            );
        }
        // Garbage fills at exact length (wrong size word) bail.
        for fill in [0x00u8, 0xABu8, 0xFFu8] {
            let msg = OscMessage {
                addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
                args: vec![OscType::Blob(vec![fill; dyn_gr::DYN_BLOB_LEN])],
            };
            assert!(driver.ingest_dyn_message(&msg).await.is_err());
        }
        // Non-blob args (swapped typetags on the wire) bail.
        for args in [
            vec![OscType::Float(0.5)],
            vec![OscType::String("garbage".into())],
            vec![
                OscType::Int(-1),
                OscType::Blob(vec![0xABu8; 10]),
            ],
        ] {
            let msg = OscMessage {
                addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
                args,
            };
            assert!(driver.ingest_dyn_message(&msg).await.is_err());
        }
        // Float/string-only payloads must not have poisoned the cache…
        // (reset: only a valid blob fills it — proven below).
        let stringy = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args: vec![OscType::String("garbage".into())],
        };
        assert!(driver.ingest_dyn_message(&stringy).await.is_err());
        assert!(driver.last_dyn().await.is_none());
        // …and a later valid blob still ingests cleanly.
        let valid = OscMessage {
            addr: addrs::METER_DYN_SUBSCRIBE.to_string(),
            args: vec![OscType::Blob(good)],
        };
        driver.ingest_dyn_message(&valid).await.unwrap();
        assert_eq!(driver.last_dyn().await.unwrap().gate[0], 6.0);
    }

    #[test]
    fn capabilities_match_loopback_tested_set() {
        use openmix_core::params;
        // Every advertised id exists in the canonical catalogue…
        for id in XAIR_CAPABILITIES {
            assert!(
                params::find(id).is_some(),
                "xair capability {id} not in the catalogue"
            );
        }
        // …and the unsupported remainder is exactly the honest-no-address
        // set: delay + hpf_slope (X32-only), source (insrc live-silent),
        // icon (no firmware leaf), link (global chlink, no per-channel
        // leaf), aux7/8 (only 6 bus sends), send11..16 (only 6+4 send
        // slots), pan outside bus-pair slots 1/3/5, all twelve legacy
        // aux/fx send mutes plus the ten v2
        // sendN_on (no send on/off leaf, T7), per-channel eq5 stays out
        // (channels carry 4 bands; eq5/6 ride buses + main and ARE in caps),
        // plus the non-strip v2b ids: solo dim/mute/level/source (global
        // `/config/solo/*`, driver methods) and fx_insert (slot-addressed
        // `/fx/N/insert`, driver method).
        let mut unsupported: Vec<&str> = params::CATALOGUE
            .iter()
            .map(|d| d.id)
            .filter(|id| !XAIR_CAPABILITIES.contains(id))
            .collect();
        unsupported.sort_unstable();
        let mut expected = vec![
            "aux1_mute",
            "aux2_mute",
            "aux3_mute",
            "aux4_mute",
            "aux5_mute",
            "aux6_mute",
            "aux7_level",
            "aux7_mute",
            "aux8_level",
            "aux8_mute",
            "delay_ms",
            "delay_on",
            "fx1_mute",
            "fx2_mute",
            "fx3_mute",
            "fx4_mute",
            "fx_insert",
            "hpf_slope",
            "icon",
            "link",
            "send10_on",
            "send10_pan",
            "send11_level",
            "send11_on",
            "send11_pan",
            "send11_tap",
            "send12_level",
            "send12_on",
            "send12_pan",
            "send12_tap",
            "send13_level",
            "send13_on",
            "send13_pan",
            "send13_tap",
            "send14_level",
            "send14_on",
            "send14_pan",
            "send14_tap",
            "send15_level",
            "send15_on",
            "send15_pan",
            "send15_tap",
            "send16_level",
            "send16_on",
            "send16_pan",
            "send16_tap",
            "send1_on",
            "send2_on",
            "send2_pan",
            "send3_on",
            "send4_on",
            "send4_pan",
            "send5_on",
            "send6_on",
            "send6_pan",
            "send7_on",
            "send7_pan",
            "send8_on",
            "send8_pan",
            "send9_on",
            "send9_pan",
            "solo_dim",
            "solo_level",
            "solo_mute",
            "solo_source",
            "source",
        ];
        expected.sort_unstable();
        assert_eq!(unsupported, expected);
        assert_eq!(XAirDriver::new().capabilities(), XAIR_CAPABILITIES);
    }

    /// Pan loopback over UDP (`crate::mix`, UNVERIFIED-ON-RIG): engineering
    /// `-100..=+100` maps linearly onto the wire float, centre 0.5.
    #[tokio::test]
    async fn driver_pan_roundtrip() {
        let (driver, _) = connected_driver().await;
        // StripId is 0-based: strip 0 drives wire `/ch/01/mix/pan`.
        for (eng, wire) in [(-100.0, 0.0), (0.0, 0.5), (100.0, 1.0), (25.0, 0.625)] {
            driver
                .set_param(StripId(0), "pan", ParamValue::Float(eng))
                .await
                .unwrap();
            let ParamValue::Float(back) = driver.get_param(StripId(0), "pan").await.unwrap()
            else {
                panic!("pan read back non-float");
            };
            assert!((back - eng).abs() < 1e-4, "pan {eng} -> {back}");
            let _ = wire;
        }
        // Range and kind enforced locally, never sent.
        assert!(
            driver
                .set_param(StripId(0), "pan", ParamValue::Float(100.1))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "pan", ParamValue::Bool(true))
                .await
                .is_err()
        );
        // v2: strip 16 is an FX return (pan applies there); the strip space
        // ends at 35, so 40 is out of range.
        assert!(
            driver
                .set_param(StripId(40), "pan", ParamValue::Float(0.0))
                .await
                .is_err()
        );
    }

    /// EQ loopback over UDP (`crate::eq`, UNVERIFIED-ON-RIG): on/off plus
    /// all four bands of gain/freq/q/type, with the inverted Q wire.
    #[tokio::test]
    async fn driver_eq_roundtrip() {
        let (driver, _) = connected_driver().await;
        // eq_on is a plain bool switch.
        driver
            .set_param(StripId(1), "eq_on", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(1), "eq_on").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(StripId(1), "eq_on", ParamValue::Bool(false))
            .await
            .unwrap();
        assert!(
            driver
                .set_param(StripId(1), "eq_on", ParamValue::Float(1.0))
                .await
                .is_err()
        );

        // Band 2: gain dB, freq Hz, Q and type round-trip.
        driver
            .set_param(StripId(1), "eq2_gain", ParamValue::Float(6.0))
            .await
            .unwrap();
        let ParamValue::Float(g) = driver.get_param(StripId(1), "eq2_gain").await.unwrap()
        else {
            panic!("eq2_gain read back non-float");
        };
        assert!((g - 6.0).abs() < 1e-4, "eq2_gain {g}");

        driver
            .set_param(StripId(1), "eq2_freq", ParamValue::Float(1000.0))
            .await
            .unwrap();
        let ParamValue::Float(f) = driver.get_param(StripId(1), "eq2_freq").await.unwrap()
        else {
            panic!("eq2_freq read back non-float");
        };
        // Desk log quantization is 201 steps: 1 Hz epsilon, like hpf.
        assert!((f - 1000.0).abs() < 1.0, "eq2_freq {f}");

        driver
            .set_param(StripId(1), "eq2_q", ParamValue::Float(1.4))
            .await
            .unwrap();
        let ParamValue::Float(q) = driver.get_param(StripId(1), "eq2_q").await.unwrap()
        else {
            panic!("eq2_q read back non-float");
        };
        assert!((q - 1.4).abs() / 1.4 < 0.06, "eq2_q {q}");

        driver
            .set_param(StripId(1), "eq2_type", ParamValue::Float(2.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(1), "eq2_type").await.unwrap(),
            ParamValue::Float(2.0)
        );

        // Band 1 carries a type on X-Air (unlike the DL32S layout).
        driver
            .set_param(StripId(1), "eq1_type", ParamValue::Float(1.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(1), "eq1_type").await.unwrap(),
            ParamValue::Float(1.0)
        );

        // Range and kind enforced locally, never sent.
        assert!(
            driver
                .set_param(StripId(1), "eq2_gain", ParamValue::Float(15.1))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(1), "eq2_freq", ParamValue::Float(19.9))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(1), "eq2_q", ParamValue::Float(0.29))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(1), "eq2_type", ParamValue::Float(6.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(1), "eq5_gain", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(1), "eq2_gain", ParamValue::Bool(true))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(40), "eq1_gain", ParamValue::Float(0.0))
                .await
                .is_err()
        );
    }

    /// Gate/comp on/off loopback over UDP (`crate::dynamics`,
    /// UNVERIFIED-ON-RIG): plain int `1`/`0` switches.
    #[tokio::test]
    async fn driver_gate_comp_on_roundtrip() {
        let (driver, _) = connected_driver().await;
        for id in ["gate_on", "comp_on"] {
            driver
                .set_param(StripId(2), id, ParamValue::Bool(true))
                .await
                .unwrap();
            assert_eq!(
                driver.get_param(StripId(2), id).await.unwrap(),
                ParamValue::Bool(true),
                "{id}"
            );
            driver
                .set_param(StripId(2), id, ParamValue::Bool(false))
                .await
                .unwrap();
            assert_eq!(
                driver.get_param(StripId(2), id).await.unwrap(),
                ParamValue::Bool(false),
                "{id}"
            );
            assert!(
                driver
                    .set_param(StripId(2), id, ParamValue::Float(1.0))
                    .await
                    .is_err(),
                "{id} wrong kind accepted"
            );
        }
        assert!(
            driver
                .set_param(StripId(40), "gate_on", ParamValue::Bool(true))
                .await
                .is_err()
        );
    }

    /// Send-level loopback over UDP (`crate::mix`, UNVERIFIED-ON-RIG):
    /// `aux1..6` → slots `01..06`, `fx1..4` → slots `07..10`, dB via the
    /// X-Air fader law. Send mutes stay errors (no wire leaf, T7).
    #[tokio::test]
    async fn driver_send_level_roundtrip() {
        let (driver, _) = connected_driver().await;
        // StripId is 0-based: strip 0 drives wire `/ch/01/mix/MM/level`.
        driver
            .set_param(StripId(0), "aux1_level", ParamValue::Float(-10.0))
            .await
            .unwrap();
        let ParamValue::Float(db) =
            driver.get_param(StripId(0), "aux1_level").await.unwrap()
        else {
            panic!("aux1_level read back non-float");
        };
        assert!((db + 10.0).abs() < 1e-4, "aux1_level {db}");

        // FX slot: fx1 → wire slot 07.
        driver
            .set_param(StripId(0), "fx1_level", ParamValue::Float(0.0))
            .await
            .unwrap();
        let ParamValue::Float(fx) = driver.get_param(StripId(0), "fx1_level").await.unwrap()
        else {
            panic!("fx1_level read back non-float");
        };
        assert!(fx.abs() < 1e-4, "fx1_level {fx}");

        // Floor: wire 0 reads back the −90 dB −inf convention.
        driver
            .set_param(StripId(0), "aux6_level", ParamValue::Float(-90.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "aux6_level").await.unwrap(),
            ParamValue::Float(-90.0)
        );
        assert!(
            driver
                .set_param(StripId(0), "aux1_level", ParamValue::Float(10.1))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "aux7_level", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "fx5_level", ParamValue::Float(0.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "aux1_level", ParamValue::Bool(false))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(16), "aux1_level", ParamValue::Float(0.0))
                .await
                .is_err()
        );
    }

    /// Gate full-set loopback over UDP (`crate::gate`, firmware-truth):
    /// thr/range/att ride lin wires, hold/rel ride log wires, mode/keysrc/
    /// filter leaves ride ints. Live values from the owner rig (XR18 FW
    /// 1.20) decode through the same codecs.
    #[tokio::test]
    async fn driver_gate_full_roundtrip() {
        let (driver, _) = connected_driver().await;
        let s = StripId(2);
        for (id, v) in [
            ("gate_thr", -30.0f32),
            ("gate_range", 40.0),
            ("gate_att", 5.0),
            ("gate_hold", 120.0),
            ("gate_rel", 250.0),
            ("gate_filter_freq", 1000.0),
        ] {
            driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
            let ParamValue::Float(back) = driver.get_param(s, id).await.unwrap() else {
                panic!("{id} read back non-float");
            };
            assert!((back - v).abs() / v.abs().max(1.0) < 1e-2, "{id} {v} -> {back}");
        }
        for (id, v) in [("gate_mode", 4.0f32), ("gate_key_src", 22.0), ("gate_filter_type", 8.0)] {
            driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
            assert_eq!(driver.get_param(s, id).await.unwrap(), ParamValue::Float(v), "{id}");
        }
        driver.set_param(s, "gate_filter_on", ParamValue::Bool(true)).await.unwrap();
        assert_eq!(driver.get_param(s, "gate_filter_on").await.unwrap(), ParamValue::Bool(true));
        // Range and out-of-scope enforced locally, never sent.
        assert!(driver.set_param(s, "gate_thr", ParamValue::Float(-80.1)).await.is_err());
        assert!(driver.set_param(s, "gate_range", ParamValue::Float(2.9)).await.is_err());
        assert!(driver.set_param(s, "gate_mode", ParamValue::Float(5.0)).await.is_err());
        assert!(driver.set_param(s, "gate_key_src", ParamValue::Float(23.0)).await.is_err());
        assert!(driver.set_param(s, "gate_thr", ParamValue::Bool(true)).await.is_err());
        // DCA strips carry no gate block (catalogue scopes).
        assert!(driver.set_param(StripId(31), "gate_thr", ParamValue::Float(-30.0)).await.is_err());
        assert!(driver.get_param(StripId(31), "gate_mode").await.is_err());
        // Channels carry 4 EQ bands (bus/main carry 6).
        assert!(driver.get_param(s, "eq5_gain").await.is_err());
        assert!(driver.get_param(StripId(20), "eq5_gain").await.is_ok());
    }

    /// Comp full-set loopback over UDP (`crate::comp`, firmware-truth):
    /// ratio rides the INT step-table index (4.0 ↔ index 6), knee/att/makeup
    /// ride lin wires, hold/rel ride log wires. Live rig values (ratio
    /// `Int(5)` = 3.0 on ch01) decode through the same table.
    #[tokio::test]
    async fn driver_comp_full_roundtrip() {
        let (driver, _) = connected_driver().await;
        let s = StripId(2);
        for (id, v) in [
            ("comp_thr", -20.0f32),
            ("comp_ratio", 4.0),
            ("comp_knee", 3.0),
            ("comp_makeup", 6.0),
            ("comp_att", 10.0),
            ("comp_hold", 20.0),
            ("comp_rel", 100.0),
            ("comp_mix", 80.0),
            ("comp_filter_freq", 2000.0),
        ] {
            driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
            let ParamValue::Float(back) = driver.get_param(s, id).await.unwrap() else {
                panic!("{id} read back non-float");
            };
            assert!((back - v).abs() / v.abs().max(1.0) < 1e-2, "{id} {v} -> {back}");
        }
        for (id, v) in [("comp_mode", 1.0f32), ("comp_det", 1.0), ("comp_env", 0.0), ("comp_key_src", 22.0), ("comp_filter_type", 8.0)] {
            driver.set_param(s, id, ParamValue::Float(v)).await.unwrap();
            assert_eq!(driver.get_param(s, id).await.unwrap(), ParamValue::Float(v), "{id}");
        }
        driver.set_param(s, "comp_auto", ParamValue::Bool(true)).await.unwrap();
        assert_eq!(driver.get_param(s, "comp_auto").await.unwrap(), ParamValue::Bool(true));
        driver.set_param(s, "comp_filter_on", ParamValue::Bool(false)).await.unwrap();
        assert_eq!(driver.get_param(s, "comp_filter_on").await.unwrap(), ParamValue::Bool(false));
        assert!(driver.set_param(s, "comp_ratio", ParamValue::Float(3.5)).await.is_err());
        assert!(driver.set_param(s, "comp_ratio", ParamValue::Float(101.0)).await.is_err());
        assert!(driver.set_param(s, "comp_att", ParamValue::Float(121.0)).await.is_err());
        assert!(driver.set_param(s, "comp_auto", ParamValue::Float(1.0)).await.is_err());
        // FX-send masters carry no comp block (catalogue scopes).
        assert!(driver.get_param(StripId(26), "comp_thr").await.is_err());
    }

    /// Strip look loopback over UDP (`crate::look`): colour `0..=15` is a
    /// live desk leaf; `icon`/`link` have NO desk leaf (firmware map has
    /// neither; both silent on FW 1.20) and are out of caps — the fake
    /// still answers them so the retained driver arms stay loopback-tested.
    /// Note the resolved colour conflict: `/ch/16/config/color` answers on
    /// this rig (the old input-module silence note was a probe artifact).
    #[tokio::test]
    async fn driver_look_roundtrip() {
        let (driver, _) = connected_driver().await;
        driver.set_param(StripId(0), "color", ParamValue::Float(5.0)).await.unwrap();
        assert_eq!(driver.get_param(StripId(0), "color").await.unwrap(), ParamValue::Float(5.0));
        driver.set_param(StripId(0), "icon", ParamValue::Float(7.0)).await.unwrap();
        assert_eq!(driver.get_param(StripId(0), "icon").await.unwrap(), ParamValue::Float(7.0));
        driver.set_param(StripId(0), "link", ParamValue::Bool(true)).await.unwrap();
        assert_eq!(driver.get_param(StripId(0), "link").await.unwrap(), ParamValue::Bool(true));
        assert!(driver.set_param(StripId(0), "color", ParamValue::Float(16.0)).await.is_err());
        assert!(driver.set_param(StripId(0), "icon", ParamValue::Float(32.0)).await.is_err());
        // DCA strips carry colour (basics) but no link (input-only).
        driver.set_param(StripId(31), "color", ParamValue::Float(0.0)).await.unwrap();
        assert_eq!(driver.get_param(StripId(31), "color").await.unwrap(), ParamValue::Float(0.0));
        assert!(driver.get_param(StripId(20), "link").await.is_err());
    }

    /// Generic-send loopback over UDP (`crate::mix`, firmware-truth): `send1..6`
    /// alias the bus-send slots (same as `aux1..6`), `send7..10` the FX slots
    /// (same as `fx1..4`); tap rides the six-point enum
    /// (`0..=5 // IN, PREEQ, POSTEQ, PRE, POST, GRP`), pan rides floats.
    /// The `sendN_on` leaf has NO desk leaf (firmware send block lists no
    /// on/off; `/ch/NN/mix/MM/on` live-silent, T7) and is out of caps — the
    /// fake still answers it so the retained driver arm stays loopback-tested.
    #[tokio::test]
    async fn driver_send_tap_pan_on_roundtrip() {
        let (driver, _) = connected_driver().await;
        let s = StripId(0);
        // Alias check: same wire slot as the legacy id.
        driver.set_param(s, "send1_level", ParamValue::Float(-10.0)).await.unwrap();
        let ParamValue::Float(db) = driver.get_param(s, "aux1_level").await.unwrap() else {
            panic!("aux1_level read back non-float");
        };
        assert!((db + 10.0).abs() < 1e-4, "send1->aux1 alias {db}");
        driver.set_param(s, "send7_level", ParamValue::Float(-12.0)).await.unwrap();
        let ParamValue::Float(fx) = driver.get_param(s, "fx1_level").await.unwrap() else {
            panic!("fx1_level read back non-float");
        };
        assert!((fx + 12.0).abs() < 1e-4, "send7->fx1 alias {fx}");

        driver.set_param(s, "send1_tap", ParamValue::Float(4.0)).await.unwrap();
        assert_eq!(driver.get_param(s, "send1_tap").await.unwrap(), ParamValue::Float(4.0));
        assert!(driver.set_param(s, "send1_tap", ParamValue::Float(6.0)).await.is_err());
        driver.set_param(s, "send1_pan", ParamValue::Float(-20.0)).await.unwrap();
        let ParamValue::Float(pan) = driver.get_param(s, "send1_pan").await.unwrap() else {
            panic!("send1_pan read back non-float");
        };
        assert!((pan + 20.0).abs() < 1e-3, "send1_pan {pan}");
        driver.set_param(s, "send1_on", ParamValue::Bool(false)).await.unwrap();
        assert_eq!(driver.get_param(s, "send1_on").await.unwrap(), ParamValue::Bool(false));

        // Slots 11..16 have no wire; legacy send mutes stay errors (T7).
        assert!(driver.set_param(s, "send11_level", ParamValue::Float(0.0)).await.is_err());
        assert!(driver.set_param(s, "send2_level", ParamValue::Bool(true)).await.is_err());
        assert!(driver.get_param(s, "aux1_mute").await.is_err());
        assert!(driver.get_param(s, "fx1_mute").await.is_err());
    }

    /// LR-assign loopback over UDP (`crate::mix`, rig-verified 2026-10-19):
    /// `{root}/mix/lr` rides int `1`/`0` on channel/FX-return/bus strips;
    /// main LR has no `lr` leaf (its `/lr/mix` container is `(on, fader,
    /// pan)`), so main/DCA/FX-send strips stay out-of-scope errors.
    #[tokio::test]
    async fn driver_lr_assign_roundtrip() {
        let (driver, _) = connected_driver().await;
        // Rig defaults: channels and FX returns assigned (1), buses not (0).
        assert_eq!(
            driver.get_param(StripId(0), "lr_assign").await.unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            driver.get_param(StripId(16), "lr_assign").await.unwrap(),
            ParamValue::Bool(true)
        );
        assert_eq!(
            driver.get_param(StripId(20), "lr_assign").await.unwrap(),
            ParamValue::Bool(false)
        );
        driver
            .set_param(StripId(0), "lr_assign", ParamValue::Bool(false))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "lr_assign").await.unwrap(),
            ParamValue::Bool(false)
        );
        driver
            .set_param(StripId(20), "lr_assign", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(20), "lr_assign").await.unwrap(),
            ParamValue::Bool(true)
        );
        // Wrong kind and out-of-scope strips error locally, never sent.
        assert!(
            driver
                .set_param(StripId(0), "lr_assign", ParamValue::Float(1.0))
                .await
                .is_err()
        );
        assert!(driver.get_param(StripId(30), "lr_assign").await.is_err());
        assert!(
            driver
                .set_param(StripId(30), "lr_assign", ParamValue::Bool(true))
                .await
                .is_err()
        );
        assert!(driver.get_param(StripId(26), "lr_assign").await.is_err());
        assert!(driver.get_param(StripId(31), "lr_assign").await.is_err());
    }

    /// Send group-on loopback over UDP (`crate::mix`, rig-verified
    /// 2026-10-19): `{root}/mix/MM/grpon` rides int `1`/`0` next to tap;
    /// all slots read `0` on the owner rig. Slots 11+ have no wire.
    #[tokio::test]
    async fn driver_send_grpon_roundtrip() {
        let (driver, _) = connected_driver().await;
        let s = StripId(0);
        assert_eq!(
            driver.get_param(s, "send1_grpon").await.unwrap(),
            ParamValue::Bool(false)
        );
        driver
            .set_param(s, "send1_grpon", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(s, "send1_grpon").await.unwrap(),
            ParamValue::Bool(true)
        );
        // FX slot (slot 10) rides the same leaf shape.
        driver
            .set_param(s, "send10_grpon", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(s, "send10_grpon").await.unwrap(),
            ParamValue::Bool(true)
        );
        // Wrong kind and unknown slots error locally, never sent.
        assert!(
            driver
                .set_param(s, "send1_grpon", ParamValue::Float(1.0))
                .await
                .is_err()
        );
        assert!(driver.get_param(s, "send11_grpon").await.is_err());
        assert!(
            driver
                .set_param(s, "send11_grpon", ParamValue::Bool(true))
                .await
                .is_err()
        );
        // Sends originate from inputs only (catalogue scopes).
        assert!(
            driver
                .set_param(StripId(20), "send1_grpon", ParamValue::Bool(true))
                .await
                .is_err()
        );
    }

    /// Automix weight loopback over UDP (`crate::automix`, rig-verified
    /// 2026-10-19): `/ch/NN/automix/weight` rides the wire float verbatim
    /// (`0..1`, no verified engineering curve); channels only.
    #[tokio::test]
    async fn driver_automix_weight_roundtrip() {
        let (driver, _) = connected_driver().await;
        let s = StripId(2);
        // Rig default reads back 0.5.
        let ParamValue::Float(w) = driver.get_param(s, "automix_weight").await.unwrap()
        else {
            panic!("automix_weight read back non-float");
        };
        assert!((w - 0.5).abs() < 1e-6, "automix_weight default {w}");
        driver
            .set_param(s, "automix_weight", ParamValue::Float(0.7))
            .await
            .unwrap();
        let ParamValue::Float(back) = driver.get_param(s, "automix_weight").await.unwrap()
        else {
            panic!("automix_weight read back non-float");
        };
        assert!((back - 0.7).abs() < 1e-6, "automix_weight {back}");
        // Range and kind enforced locally, never sent.
        assert!(
            driver
                .set_param(s, "automix_weight", ParamValue::Float(1.1))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(s, "automix_weight", ParamValue::Bool(true))
                .await
                .is_err()
        );
        // Buses/returns carry no automix leaf (catalogue scopes).
        assert!(driver.get_param(StripId(20), "automix_weight").await.is_err());
        assert!(driver.get_param(StripId(16), "automix_weight").await.is_err());
    }

    /// Strip insert loopback over UDP (`crate::insert`, rig-verified
    /// 2026-10-20): `{root}/insert/on` rides int `1`/`0`, `{root}/insert/sel`
    /// rides the raw int `0..=8` (NOT 1..4 FX slots) on channel/bus/main
    /// strips. Untouched reads match the rig (bypassed, nothing selected).
    #[tokio::test]
    async fn driver_strip_insert_roundtrip() {
        let (driver, _) = connected_driver().await;
        // Channel, bus and main strips; untouched reads default off/0.
        for s in [StripId(0), StripId(20), StripId(30)] {
            assert_eq!(
                driver.get_param(s, "insert_on").await.unwrap(),
                ParamValue::Bool(false)
            );
            assert_eq!(
                driver.get_param(s, "insert_sel").await.unwrap(),
                ParamValue::Float(0.0)
            );
        }
        driver
            .set_param(StripId(0), "insert_on", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "insert_on").await.unwrap(),
            ParamValue::Bool(true)
        );
        driver
            .set_param(StripId(0), "insert_sel", ParamValue::Float(7.0))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(0), "insert_sel").await.unwrap(),
            ParamValue::Float(7.0)
        );
        // Main LR rides the same leaves.
        driver
            .set_param(StripId(30), "insert_on", ParamValue::Bool(true))
            .await
            .unwrap();
        assert_eq!(
            driver.get_param(StripId(30), "insert_on").await.unwrap(),
            ParamValue::Bool(true)
        );
        // Range and kind enforced locally, never sent.
        assert!(
            driver
                .set_param(StripId(0), "insert_on", ParamValue::Float(1.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "insert_sel", ParamValue::Float(9.0))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "insert_sel", ParamValue::Float(1.5))
                .await
                .is_err()
        );
        assert!(
            driver
                .set_param(StripId(0), "insert_sel", ParamValue::Bool(true))
                .await
                .is_err()
        );
        // Returns, FX sends and masters carry no insert leaf (the desk is
        // silent there — catalogue scopes reject before any I/O).
        for s in [StripId(16), StripId(26), StripId(31), StripId(35)] {
            assert!(driver.get_param(s, "insert_on").await.is_err());
            assert!(driver.get_param(s, "insert_sel").await.is_err());
        }
    }

    /// Solo monitoring globals loopback over UDP (`crate::solo`,
    /// rig-verified 2026-10-19): dim/mute/level/source round-trip through
    /// the global driver methods; untouched reads match the rig (dim 0,
    /// mute 0, level 0.75, source 3).
    #[tokio::test]
    async fn driver_solo_globals_roundtrip() {
        let (driver, _) = connected_driver().await;
        assert!(!driver.get_solo_dim().await.unwrap());
        assert!(!driver.get_solo_mute().await.unwrap());
        assert!((driver.get_solo_level().await.unwrap() - 0.75).abs() < 1e-6);
        assert_eq!(driver.get_solo_source().await.unwrap(), 3);
        driver.set_solo_dim(true).await.unwrap();
        assert!(driver.get_solo_dim().await.unwrap());
        driver.set_solo_dim(false).await.unwrap();
        assert!(!driver.get_solo_dim().await.unwrap());
        driver.set_solo_mute(true).await.unwrap();
        assert!(driver.get_solo_mute().await.unwrap());
        driver.set_solo_mute(false).await.unwrap();
        assert!(!driver.get_solo_mute().await.unwrap());
        driver.set_solo_level(0.5).await.unwrap();
        assert!((driver.get_solo_level().await.unwrap() - 0.5).abs() < 1e-6);
        // Range enforced locally, never sent.
        assert!(driver.set_solo_level(1.1).await.is_err());
        assert!(driver.set_solo_level(f32::NAN).await.is_err());
        driver.set_solo_source(0).await.unwrap();
        assert_eq!(driver.get_solo_source().await.unwrap(), 0);
        driver.set_solo_source(14).await.unwrap();
        assert_eq!(driver.get_solo_source().await.unwrap(), 14);
        assert!(driver.set_solo_source(15).await.is_err());
        // Globals are not strip params (out of strip caps, never polled).
        assert!(!XAIR_CAPABILITIES.contains(&"solo_dim"));
        assert!(!XAIR_CAPABILITIES.contains(&"solo_mute"));
        assert!(!XAIR_CAPABILITIES.contains(&"solo_level"));
        assert!(!XAIR_CAPABILITIES.contains(&"solo_source"));
        assert!(driver.get_param(StripId(0), "solo_dim").await.is_err());
        assert!(driver.get_param(StripId(30), "solo_dim").await.is_err());
    }

    /// FX slot insert loopback over UDP (`crate::fx`, rig-verified
    /// 2026-10-19): `/fx/N/insert` rides int `1`/`0` (slot 4 inserted on
    /// the rig, the rest bypassed). Out-of-range slots bail locally.
    #[tokio::test]
    async fn driver_fx_insert_roundtrip() {
        let (driver, _) = connected_driver().await;
        assert!(!driver.get_fx_insert(1).await.unwrap());
        driver.set_fx_insert(1, true).await.unwrap();
        assert!(driver.get_fx_insert(1).await.unwrap());
        driver.set_fx_insert(1, false).await.unwrap();
        assert!(!driver.get_fx_insert(1).await.unwrap());
        assert!(driver.set_fx_insert(0, true).await.is_err());
        assert!(driver.set_fx_insert(5, true).await.is_err());
        assert!(driver.get_fx_insert(0).await.is_err());
        assert!(driver.get_fx_insert(5).await.is_err());
        // Slot-addressed, not a strip param (out of strip caps).
        assert!(!XAIR_CAPABILITIES.contains(&"fx_insert"));
        assert!(driver.get_param(StripId(0), "fx_insert").await.is_err());
    }

    /// FX type-select write loopback over UDP (`crate::fx`, rig-verified
    /// 2026-10-19 — slot-4 sweep 60→0/7/26/59/30/56→60): `set_fx_type`
    /// drives the same `/fx/N/type` leaf `get_fx_type` reads. Range
    /// `0..=60` enforced locally (firmware holds 61 types, one more than
    /// the app table's 0..=59); out-of-range slots/types bail locally.
    #[tokio::test]
    async fn driver_fx_type_write_roundtrip() {
        let (driver, _) = connected_driver().await;
        // Untouched slots read back 0 (HALL), like a freshly-booted desk.
        assert_eq!(driver.get_fx_type(1).await.unwrap(), 0);
        driver.set_fx_type(4, 60).await.unwrap();
        assert_eq!(driver.get_fx_type(4).await.unwrap(), 60);
        // The rig-verified sweep shape: every written type reads back.
        for t in [0u8, 7, 26, 30, 56, 59, 60] {
            driver.set_fx_type(4, t).await.unwrap();
            assert_eq!(driver.get_fx_type(4).await.unwrap(), t, "type {t}");
        }
        assert!(driver.set_fx_type(1, 61).await.is_err());
        assert!(driver.set_fx_type(0, 7).await.is_err());
        assert!(driver.set_fx_type(5, 7).await.is_err());
        assert!(driver.get_fx_type(0).await.is_err());
        assert!(driver.get_fx_type(5).await.is_err());
        // FX params keep working after a type swap (slot-4 par write+read).
        driver.set_fx_param(4, 0, 0.25).await.unwrap();
        assert_eq!(driver.get_fx_param(4, 0).await.unwrap(), 0.25);
    }

    /// Non-channel strip loopback over UDP (firmware-truth): buses carry
    /// fader/mute/name/colour/EQ/dynamics, FX returns (`/rtn/N`) carry
    /// fader/mute/name/colour/EQ, FX-send masters and DCAs carry basics only.
    /// Gate stays channel-only (out of scope elsewhere); buses + main carry
    /// 6 EQ bands.
    #[tokio::test]
    async fn driver_bus_ret_fxsnd_main_dca_strips_roundtrip() {
        let (driver, _) = connected_driver().await;
        // Bus 1 (strip 20): fader/mute/name/colour/EQ/dynamics, no gate.
        let b = StripId(20);
        driver.set_fader(b, 0.6).await.unwrap();
        assert!((driver.get_fader(b).await.unwrap() - 0.6).abs() < addrs::FADER_READBACK_EPSILON);
        driver.set_mute(b, true).await.unwrap();
        assert!(driver.get_mute(b).await.unwrap());
        driver.set_name(b, "MON").await.unwrap();
        assert_eq!(driver.get_name(b).await.unwrap(), "MON");
        driver.set_param(b, "color", ParamValue::Float(6.0)).await.unwrap();
        driver.set_param(b, "eq_on", ParamValue::Bool(true)).await.unwrap();
        driver.set_param(b, "eq2_gain", ParamValue::Float(2.0)).await.unwrap();
        driver.set_param(b, "eq5_gain", ParamValue::Float(1.5)).await.unwrap();
        driver.set_param(b, "comp_ratio", ParamValue::Float(3.0)).await.unwrap();
        driver.set_param(b, "pan", ParamValue::Float(10.0)).await.unwrap();
        let ParamValue::Float(g) = driver.get_param(b, "eq2_gain").await.unwrap() else {
            panic!("bus eq2_gain non-float");
        };
        assert!((g - 2.0).abs() < 1e-3, "bus eq2_gain {g}");
        let ParamValue::Float(r) = driver.get_param(b, "comp_ratio").await.unwrap() else {
            panic!("bus comp_ratio non-float");
        };
        assert!((r - 3.0).abs() < 1e-6, "bus comp_ratio {r}");
        // Channel-only leaves stay out of scope on buses.
        assert!(driver.get_param(b, "gain").await.is_err());
        assert!(driver.get_param(b, "gate_thr").await.is_err());
        assert!(driver.get_param(b, "aux1_level").await.is_err());

        // FX return 1 (strip 16, `/rtn/1`): fader + EQ answer.
        let r = StripId(16);
        driver.set_fader(r, 0.7).await.unwrap();
        assert!((driver.get_fader(r).await.unwrap() - 0.7).abs() < addrs::FADER_READBACK_EPSILON);
        driver.set_param(r, "eq_on", ParamValue::Bool(true)).await.unwrap();
        assert_eq!(driver.get_param(r, "eq_on").await.unwrap(), ParamValue::Bool(true));

        // FX send master 1 (strip 26): basics only.
        let f = StripId(26);
        driver.set_fader(f, 0.5).await.unwrap();
        driver.set_param(f, "color", ParamValue::Float(4.0)).await.unwrap();
        assert!(driver.get_param(f, "eq_on").await.is_err());
        assert!(driver.get_param(f, "pan").await.is_err());

        // Main LR (strip 30): fader + EQ answer; pan does not exist.
        let m = StripId(30);
        driver.set_fader(m, 0.8).await.unwrap();
        driver.set_param(m, "eq2_gain", ParamValue::Float(1.0)).await.unwrap();
        assert!(driver.get_param(m, "pan").await.is_err());

        // DCA 1 (strip 31): basics only.
        let d = StripId(31);
        driver.set_fader(d, 0.75).await.unwrap();
        driver.set_mute(d, false).await.unwrap();
        assert!(driver.get_param(d, "gate_thr").await.is_err());
    }

    /// XR18 strip map shape: ordered, contiguous, 36 strips; roots resolve
    /// per family; out-of-range stays a local error.
    #[test]
    fn topology_matches_strip_roots() {
        assert_eq!(XAIR_STRIPS, 36);
        let mut next = 0usize;
        for fam in XAIR_TOPOLOGY {
            assert_eq!(fam.first, next, "gap/overlap before {}", fam.key);
            next = fam.end();
        }
        assert_eq!(next, XAIR_STRIPS);
        let (k, r) = strip_root(0).unwrap();
        assert_eq!((k, r.as_str()), (openmix_core::StripKind::In, "/ch/01"));
        let (k, r) = strip_root(15).unwrap();
        assert_eq!((k, r.as_str()), (openmix_core::StripKind::In, "/ch/16"));
        assert_eq!(strip_root(16).unwrap().1, "/rtn/1");
        assert_eq!(strip_root(19).unwrap().1, "/rtn/4");
        assert_eq!(strip_root(20).unwrap().1, "/bus/1");
        assert_eq!(strip_root(25).unwrap().1, "/bus/6");
        assert_eq!(strip_root(26).unwrap().1, "/fxsend/1");
        assert_eq!(strip_root(30).unwrap().1, "/lr");
        assert_eq!(strip_root(31).unwrap().1, "/dca/1");
        assert_eq!(strip_root(34).unwrap().1, "/dca/4");
        let (k, r) = strip_root(35).unwrap();
        assert_eq!((k, r.as_str()), (openmix_core::StripKind::Ret, "/rtn/aux"));
        assert!(strip_root(36).is_err());
        assert!(strip_root(99).is_err());
        assert_eq!(strip_of_root("/rtn/aux"), Some(35));
        assert_eq!(strip_of_root("/rtn/1"), Some(16));
    }
}
