//! Behringer X-Air OSC driver (XR12/16/18, MR18).
//!
//! Thin mapping over [`openmix_transport::OscSession`]: this crate owns ONLY the
//! X-Air address table + arg mapping. Sockets, keepalive, meter parsing, and
//! request/response plumbing all live in `openmix-transport`.
use openmix_core::{MixerDriver, MixerInfo, ParamValue, StripId};
use openmix_transport::{KeepaliveConfig, OscSession, osc};
use rosc::{OscMessage, OscType};
use std::sync::OnceLock;
use tokio::sync::{Mutex, broadcast};

pub mod dyn_gr;
pub mod fx;
pub mod headamp;
pub mod input;

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
    /// Main LR mute, by analogy with `/ch/XX/mix/on` (not yet live-queried).
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

/// Live XR18-class mixer on UDP 10024.
pub struct XAirDriver {
    session: Mutex<Option<OscSession>>,
    peer: Mutex<String>,
    model: addrs::XAirModel,
    /// Last decoded `/meters/6` ALL DYN frame (`None` until the first blob
    /// is ingested). Serves the `gate_gr` / `comp_gr` param reads — the
    /// desk has no GR query address, so there is nothing else to read.
    dyn_cache: Mutex<Option<dyn_gr::DynMeters>>,
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
        Self {
            session: Mutex::new(None),
            peer: Mutex::new(String::new()),
            model,
            dyn_cache: Mutex::new(None),
        }
    }

    /// 0-based strip index to 1-based wire channel, validated against this
    /// driver's model bound (XR18: strips 0..=15; `/ch/17+` is silent on
    /// hardware, T3).
    fn check_channel(&self, strip: u16) -> anyhow::Result<u16> {
        addrs::channel_in(strip, self.model.channels())
    }

    async fn session(&self) -> anyhow::Result<tokio::sync::MutexGuard<'_, Option<OscSession>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        Ok(guard)
    }

    /// Stereo aux return strip (`/rtn/aux`, hardware-verified present,
    /// baseline §2). Kept as explicit methods — not a `StripId` — so the
    /// 0..16 channel range maps 1:1 to wire `/ch/01..16` with no phantom
    /// strips (T3).
    pub async fn get_aux_fader(&self) -> anyhow::Result<f32> {
        let guard = self.session().await?;
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

    /// Map a raw `/info` reply to [`MixerInfo`]. Hardware order (baseline
    /// §1, XR18 FW 1.20 `/info` → `[V0.04, Example Mixer, XR18, 1.20]`):
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
        self.dyn_cache.lock().await.clone()
    }

    /// Meter subscribe with an explicit renewal interval (tests pass a short
    /// interval; production uses [`addrs::METER_RENEW_INTERVAL`]).
    pub async fn subscribe_meters_with_interval(
        &self,
        interval: std::time::Duration,
    ) -> anyhow::Result<broadcast::Receiver<openmix_core::MeterFrame>> {
        let rx = self.subscribe_meters_once().await?;
        self.spawn_meter_renewal(interval).await?;
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
        // `/xremote` keepalive (every ~8 s, already running via `connect`)
        // alongside. Owner-rig step: capture a real meter blob and confirm
        // the LE-i16 run shape (xr-meters skips 4 header bytes before the
        // samples — transport parses from offset 0; reconcile on hardware).
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
        let sock = std::sync::Arc::clone(guard.as_ref().unwrap().socket());
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
        tokio::spawn(async move {
            let mut ticker =
                openmix_transport::session::keepalive_ticker(interval);
            // `keepalive_ticker` fires immediately; consume it so the first
            // renewal lands one full `interval` after the initial subscribe.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _ = sock.send(&xremote).await;
                let _ = sock.send(&sub_levels).await;
                let _ = sock.send(&sub_dyn).await;
            }
        });
        Ok(())
    }

    /// Ingest one raw `/meters/6` OSC message into the GR cache (the
    /// documented ALL DYN shape — see [`crate::dyn_gr`]). Any other address
    /// (or a non-blob payload) bails: no guessing at unknown meter layouts.
    ///
    /// Ingestion lives here — not in the session recv loop — because the
    /// shared transport hub only forwards level-only `MeterFrame`s and this
    /// crate cannot change that routing. A future transport raw-frame hook
    /// (or the relay slice) calls this per DYN blob; until then the cache
    /// fills in tests, and [`MixerDriver::get_param`] reports no-data.
    pub async fn ingest_dyn_message(&self, msg: &OscMessage) -> anyhow::Result<()> {
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
        let dyn_meters = dyn_gr::parse_dyn_blob(blob)?;
        *self.dyn_cache.lock().await = Some(dyn_meters);
        Ok(())
    }

    /// GR read for one strip from the DYN cache. Errors when no blob has
    /// been ingested yet (caller keeps the static scale) or the strip is
    /// out of range — never a fabricated zero.
    async fn cached_gr(&self, strip: StripId, gate: bool) -> anyhow::Result<f32> {
        let cache = self.dyn_cache.lock().await;
        let Some(dyn_meters) = cache.as_ref() else {
            anyhow::bail!("xair: no DYN meter data yet (desk not streaming /meters/6)");
        };
        if gate {
            dyn_meters.gate_gr(strip.0)
        } else {
            dyn_meters.comp_gr(strip.0)
        }
    }
}

impl MixerDriver for XAirDriver {
    fn name(&self) -> &'static str {
        "xair"
    }

    /// Addressable channel strips for this model (XR18: 16; `/rtn/aux` and
    /// buses/main are separate methods, not strips). Keeps `list`/sweeps
    /// off the phantom `/ch/17+` range the desk never answers (T3).
    fn channel_count(&self) -> usize {
        self.model.channels() as usize
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        let peer = openmix_transport::session::resolve_peer(addr, addrs::PORT)?;
        let session = OscSession::connect(peer, Some(KeepaliveConfig::xremote())).await?;
        // Best-effort discovery ping; failure still leaves the session usable.
        let _ = session
            .query_timeout(addrs::INFO, std::time::Duration::from_millis(800))
            .await;
        *self.session.lock().await = Some(session);
        *self.peer.lock().await = peer.to_string();
        Ok(())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_fader(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_fader(ch);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_on(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::on_arg(muted)])
            .await
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_on(ch);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::mute_is_muted(&reply)
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_name(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::String(name.to_string())])
            .await
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = self.check_channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_name(ch);
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
        let addr = fx::fx_param(slot, param)?;
        let guard = self.session().await?;
        let session = guard.as_ref().unwrap();
        // Loaded effect selects the par def (enum pars reply as INT option
        // indices on hardware — see `crate::fx`). A missing/unparseable
        // type falls back to raw decode (`None` def); plain floats stay
        // bit-exact either way (baseline §6 row 5).
        let def = match session.query(&fx::fx_type(slot)?).await {
            Ok(treply) => match fx::parse_fx_type(&treply.args) {
                Ok(t) => fx::fx_param_def(t, param + 1),
                Err(_) => None,
            },
            Err(_) => None,
        };
        let reply = session.query(&addr).await?;
        fx::parse_fx_param(&reply.args, def.as_ref())
    }

    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openmix_core::MeterFrame>> {
        self.subscribe_meters_with_interval(addrs::METER_RENEW_INTERVAL)
            .await
    }

    /// GR reads ride the param path (least invention: no trait change, no
    /// relay change). `fader`/`mute`/`name` arms repeat the trait default
    /// verbatim (defaults are not inheritable); `gate_gr`/`comp_gr` serve
    /// dB reduction `>= 0` from the last `/meters/6` blob and error when no
    /// blob has arrived yet. No GR query address exists (see
    /// [`crate::dyn_gr`]), so `set_param` for these ids stays unsupported.
    ///
    /// `gain` (dB) and `phantom` (bool) drive the channel preamps
    /// (`/headamp/NN/…`, community-documented via xair-api-python — see
    /// [`crate::headamp`]; NOT owner-rig verified). `set_param` mirrors
    /// these plus the trait-default arms (defaults are not inheritable).
    ///
    /// Channel input routing (`crate::input`, owner-rig verified
    /// 2026-10-18 on XR18 FW 1.20): `usbreturn` (whole-number float
    /// `0..=17`, `/ch/NN/config/rtnsrc`), `trim` (dB `−18..=18`,
    /// `/ch/NN/preamp/rtntrim`), `polarity` (bool, `/ch/NN/preamp/invert`),
    /// `hpf_on` (bool, `/ch/NN/preamp/hpon`) and `hpf_freq` (Hz `20..=400`,
    /// `/ch/NN/preamp/hpf`). `inputsource` (`/ch/NN/config/insrc`) and
    /// channel `usbinput` (`/ch/NN/preamp/rtnsw`) stay unsupported — both
    /// are live-silent on this rig (see [`crate::input`).
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match (param, value) {
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("pan", ParamValue::Float(_)) => {
                anyhow::bail!("{}: pan not supported", self.name())
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
                        vec![headamp::phantom_arg(on)],
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
                    .send(&input::preamp_invert(ch), vec![input::input_bool_arg(on)])
                    .await
            }
            ("hpf_on", ParamValue::Bool(on)) => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                guard
                    .as_ref()
                    .unwrap()
                    .send(&input::preamp_hpf_on(ch), vec![input::input_bool_arg(on)])
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
            (p, _) => anyhow::bail!("{}: unknown param {p:?}", self.name()),
        }
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            "gain" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&headamp::headamp_gain(ch))
                    .await?;
                headamp::wire_to_db(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "phantom" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&headamp::headamp_phantom(ch))
                    .await?;
                headamp::phantom_is_on(&reply).map(ParamValue::Bool)
            }
            "usbreturn" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::config_rtnsrc(ch))
                    .await?;
                input::usbreturn_from_msg(&reply).map(|i| ParamValue::Float(i as f32))
            }
            "trim" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_rtntrim(ch))
                    .await?;
                input::rtntrim_wire_to_db(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "polarity" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_invert(ch))
                    .await?;
                input::input_bool_is_on(&reply).map(ParamValue::Bool)
            }
            "hpf_on" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf_on(ch))
                    .await?;
                input::input_bool_is_on(&reply).map(ParamValue::Bool)
            }
            "hpf_freq" => {
                let ch = self.check_channel(strip.0)?;
                let guard = self.session().await?;
                let reply = guard
                    .as_ref()
                    .unwrap()
                    .query(&input::preamp_hpf(ch))
                    .await?;
                input::hpf_wire_to_hz(osc::fader_value(&reply)?).map(ParamValue::Float)
            }
            "gate_gr" => self.cached_gr(strip, true).await.map(ParamValue::Float),
            "comp_gr" => self.cached_gr(strip, false).await.map(ParamValue::Float),
            _ => anyhow::bail!("{}: unknown param {param:?}", self.name()),
        }
    }
}

/// Codec-level fader helpers re-exported for tests (wire logic lives in transport).
pub fn encode_fader(addr: &str, value: f32) -> anyhow::Result<Vec<u8>> {
    osc::encode_message(addr, vec![OscType::Float(osc::clamp01(value))])
}

pub fn keepalive_packet() -> anyhow::Result<Vec<u8>> {
    static CACHED: OnceLock<Vec<u8>> = OnceLock::new();
    let bytes = CACHED
        .get_or_init(|| osc::query_message(addrs::XREMOTE).expect("keepalive encodes"))
        .clone();
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openmix_transport::meter::build_meter_blob;
    use rosc::OscType;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::UdpSocket;

    #[test]
    fn fader_encode_decode_roundtrip() {
        let bytes = encode_fader("/ch/01/mix/fader", 0.75).unwrap();
        let packet = osc::decode_packet(&bytes).unwrap();
        let msgs = osc::flatten(packet);
        assert_eq!(msgs[0].addr, "/ch/01/mix/fader");
        assert!((osc::fader_value(&msgs[0]).unwrap() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn keepalive_shape() {
        let bytes = keepalive_packet().unwrap();
        let msgs = osc::flatten(osc::decode_packet(&bytes).unwrap());
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].addr, "/xremote");
        assert!(msgs[0].args.is_empty());
    }

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
        meter_subscribe: StdMutex<Option<rosc::OscMessage>>,
        dyn_subscribe: StdMutex<Option<rosc::OscMessage>>,
        /// Resubscribe tolerance counters: the renewal loop re-sends
        /// `/meters` subscribes (and `/xremote`) on schedule, so the fake
        /// counts every hit instead of asserting exactly-one.
        meter_count: StdMutex<u64>,
        dyn_count: StdMutex<u64>,
        xremote_count: StdMutex<u64>,
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
                meter_subscribe: StdMutex::new(None),
                dyn_subscribe: StdMutex::new(None),
                meter_count: StdMutex::new(0),
                dyn_count: StdMutex::new(0),
                xremote_count: StdMutex::new(0),
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
                            (a, Some(OscType::Float(f))) if a.ends_with("/mix/fader") => {
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
                            (a, Some(OscType::Int(i))) if a.ends_with("/mix/on") => {
                                worker.ons.lock().unwrap().insert(a.to_string(), *i);
                                None
                            }
                            (a, Some(OscType::String(s))) if a.ends_with("/config/name") => {
                                worker.names.lock().unwrap().insert(a.to_string(), s.clone());
                                None
                            }
                            (a, None) if a.ends_with("/mix/fader") => {
                                let v = worker
                                    .faders
                                    .lock()
                                    .unwrap()
                                    .get(a)
                                    .copied()
                                    .unwrap_or(0.75);
                                Some(osc::encode_message(a, vec![OscType::Float(v)]).unwrap())
                            }
                            (a, None) if a.ends_with("/mix/on") => {
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
                            ("/info", _) => None,
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
        *driver.session.lock().await = Some(session);
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
        *driver.session.lock().await = Some(session);

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
        *driver.session.lock().await = Some(session);

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
        // `/ch/17+` (strips 16+) are phantom on the XR18: reject locally
        // instead of querying addresses the desk never answers (T3).
        assert!(driver.set_fader(StripId(15), 0.5).await.is_ok());
        assert!(driver.set_fader(StripId(16), 0.5).await.is_err());
        assert!(driver.get_mute(StripId(99)).await.is_err());
        assert!(driver.set_bus_fader(6, 0.5).await.is_err());
        assert!(driver.get_bus_fader(6).await.is_err());
    }

    #[tokio::test]
    async fn driver_rejects_out_of_range_strips() {
        let (driver, _) = connected_driver().await;
        assert!(driver.set_fader(StripId(32), 0.5).await.is_err());
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
        assert!(driver.get_param(StripId(0), "pan").await.is_err());
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

        // Phantom strips stay local (T3); live-silent leaves stay errors.
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
        // Session keepalive (`/xremote` every 8 s) fires immediately too —
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
}
