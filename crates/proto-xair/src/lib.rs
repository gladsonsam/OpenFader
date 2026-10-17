//! Behringer X-Air OSC driver (XR12/16/18, MR18).
//!
//! Thin mapping over [`openmix_transport::OscSession`]: this crate owns ONLY the
//! X-Air address table + arg mapping. Sockets, keepalive, meter parsing, and
//! request/response plumbing all live in `openmix-transport`.
use openmix_core::{MixerDriver, MixerInfo, StripId};
use openmix_transport::{KeepaliveConfig, OscSession, osc};
use rosc::OscType;
use std::sync::OnceLock;
use tokio::sync::{Mutex, broadcast};

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
    /// doc `/meters ,si "/meters/0" 8`); NOT yet owner-rig verified.
    pub const METER_REQUEST: &str = "/meters";
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

    async fn subscribe_meters(
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
        Ok(session.subscribe_meters())
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
        meter_subscribe: StdMutex<Option<rosc::OscMessage>>,
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
                meter_subscribe: StdMutex::new(None),
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
                            ("/xremote", _) => None,
                            ("/meters", Some(OscType::String(id)))
                                if id == addrs::METER_SUBSCRIBE =>
                            {
                                // Documented subscribe form (T4): record it,
                                // then stream blobs at ~50ms like the desk.
                                *worker.meter_subscribe.lock().unwrap() = Some(msg.clone());
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

    /// Meter subscription uses the documented `/meters` + stream-id-string
    /// form (T4): the fake streams blobs only after that subscribe, and the
    /// test asserts both the subscribe shape and the parsed frames.
    #[tokio::test]
    async fn driver_meter_subscribe_receives_blob() {
        let (driver, fake) = connected_driver().await;
        let mut rx = driver.subscribe_meters().await.unwrap();
        // The desk streams at ~50ms; expect at least two frames.
        for _ in 0..2 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
                .await
                .expect("meter frame in time")
                .unwrap();
            assert_eq!(frame.levels.len(), 3);
            assert!((frame.levels[1] - 0.5).abs() < 0.01);
        }
        // The fake only streams after the documented subscribe shape.
        let sub = fake.meter_subscribe.lock().unwrap();
        let sub = sub.as_ref().expect("fake saw no meter subscribe");
        assert_eq!(sub.addr, addrs::METER_REQUEST);
        assert_eq!(
            sub.args,
            vec![OscType::String(addrs::METER_SUBSCRIBE.to_string())]
        );
    }
}
