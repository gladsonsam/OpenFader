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
    //! 0-based everywhere, so [`channel`] adds 1 (buses: [`bus`] adds 1).
    //! The `ch_*`/`bus_fader` builders below take 1-based *wire* numbers.
    pub const PORT: u16 = 10024;
    pub const XREMOTE: &str = "/xremote";
    pub const INFO: &str = "/info";
    pub const METER_SUBSCRIBE: &str = "/meters/1";
    pub const MAIN_FADER: &str = "/main/st/mix/fader";

    pub const MAX_CHANNEL: u16 = 32;
    pub const MAX_BUS: u8 = 6;

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
        format!("/bus/{bus:02}/mix/fader")
    }

    /// 0-based strip index to 1-based wire channel (`StripId(0)` → ch 1).
    pub fn channel(strip: u16) -> anyhow::Result<u16> {
        if strip >= MAX_CHANNEL {
            anyhow::bail!(
                "channel strip {strip} out of range 0..={} (0-based)",
                MAX_CHANNEL - 1
            );
        }
        Ok(strip + 1)
    }

    /// 0-based bus index to 1-based wire bus (bus 0 → `/bus/01/…`).
    pub fn bus(strip: u8) -> anyhow::Result<u8> {
        if strip >= MAX_BUS {
            anyhow::bail!(
                "bus strip {strip} out of range 0..={} (0-based)",
                MAX_BUS - 1
            );
        }
        Ok(strip + 1)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn table_shapes() {
            assert_eq!(ch_fader(1), "/ch/01/mix/fader");
            assert_eq!(ch_fader(16), "/ch/16/mix/fader");
            assert_eq!(ch_on(3), "/ch/03/mix/on");
            assert_eq!(ch_name(5), "/ch/05/config/name");
            assert_eq!(bus_fader(2), "/bus/02/mix/fader");
            assert_eq!(MAIN_FADER, "/main/st/mix/fader");
            // 0-based mapping: strip 0 is wire ch 1, strip 31 is ch 32.
            assert_eq!(channel(0).unwrap(), 1);
            assert_eq!(channel(31).unwrap(), 32);
            assert!(channel(32).is_err());
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
}

impl Default for XAirDriver {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            peer: Mutex::new(String::new()),
        }
    }
}

impl XAirDriver {
    pub fn new() -> Self {
        Self::default()
    }

    async fn session(&self) -> anyhow::Result<tokio::sync::MutexGuard<'_, Option<OscSession>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        Ok(guard)
    }

    fn info_from_strings(strings: Vec<String>, peer: String) -> MixerInfo {
        MixerInfo {
            name: strings.get(2).cloned().unwrap_or_default(),
            model: strings.first().cloned().unwrap_or_default(),
            firmware: strings.get(1).cloned().unwrap_or_default(),
            addr: peer,
        }
    }
}

impl MixerDriver for XAirDriver {
    fn name(&self) -> &'static str {
        "xair"
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
        let ch = addrs::channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_fader(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::fader_arg(value)])
            .await
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let ch = addrs::channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_fader(ch);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::fader_value(&reply)
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let ch = addrs::channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_on(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![osc::on_arg(muted)])
            .await
    }

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = addrs::channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_on(ch);
        let reply = guard.as_ref().unwrap().query(&addr).await?;
        osc::mute_is_muted(&reply)
    }

    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let ch = addrs::channel(strip.0)?;
        let guard = self.session().await?;
        let addr = addrs::ch_name(ch);
        guard
            .as_ref()
            .unwrap()
            .send(&addr, vec![OscType::String(name.to_string())])
            .await
    }

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = addrs::channel(strip.0)?;
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
        // Tell the mixer to start streaming blobs; recv loop broadcasts them.
        session
            .send(addrs::METER_SUBSCRIBE, vec![])
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
    struct FakeMixer {
        socket: Arc<UdpSocket>,
        faders: StdMutex<HashMap<String, f32>>,
        ons: StdMutex<HashMap<String, i32>>,
        names: StdMutex<HashMap<String, String>>,
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
            });
            let worker = fake.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65535];
                // Preload defaults.
                worker.faders.lock().unwrap().insert("/ch/01/mix/fader".into(), 0.75);
                worker.faders.lock().unwrap().insert("/bus/01/mix/fader".into(), 0.7);
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
                            ("/meters/1", _) => {
                                // Emit one meter blob back at the requester.
                                let blob = build_meter_blob(&[0.1, 0.5, 0.9]);
                                Some(
                                    osc::encode_message(
                                        "/meters/1",
                                        vec![OscType::Blob(blob)],
                                    )
                                    .unwrap(),
                                )
                            }
                            ("/info", _) if msg.args.is_empty() => Some(
                                osc::encode_message(
                                    "/info",
                                    vec![
                                        OscType::String("XR18".into()),
                                        OscType::String("1.22".into()),
                                        OscType::String("FakeXR".into()),
                                    ],
                                )
                                .unwrap(),
                            ),
                            (a, Some(OscType::Float(f))) if a.ends_with("/mix/fader") => {
                                worker.faders.lock().unwrap().insert(a.to_string(), *f);
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
        driver.set_fader(StripId(0), 0.42).await.unwrap();
        let v = driver.get_fader(StripId(0)).await.unwrap();
        assert!((v - 0.42).abs() < 1e-5);

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
        // Buses are 0-based too: bus 0 drives wire `/bus/01/…`.
        driver.set_bus_fader(0, 0.33).await.unwrap();
        assert!((driver.get_bus_fader(0).await.unwrap() - 0.33).abs() < 1e-5);
        driver.set_main_fader(0.66).await.unwrap();
        assert!((driver.get_main_fader().await.unwrap() - 0.66).abs() < 1e-5);

        let info = driver.query_info().await.unwrap();
        assert_eq!(info.model, "XR18");
        assert_eq!(info.firmware, "1.22");
    }

    #[tokio::test]
    async fn driver_rejects_out_of_range_strips() {
        let (driver, _) = connected_driver().await;
        assert!(driver.set_fader(StripId(32), 0.5).await.is_err());
        assert!(driver.get_mute(StripId(99)).await.is_err());
        assert!(driver.set_bus_fader(6, 0.5).await.is_err());
        assert!(driver.get_bus_fader(6).await.is_err());
    }

    #[tokio::test]
    async fn driver_meter_subscribe_receives_blob() {
        let (driver, _) = connected_driver().await;
        let mut rx = driver.subscribe_meters().await.unwrap();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("meter frame in time")
            .unwrap();
        assert_eq!(frame.levels.len(), 3);
        assert!((frame.levels[1] - 0.5).abs() < 0.01);
    }
}
