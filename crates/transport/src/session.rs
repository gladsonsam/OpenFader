//! Generic OSC-over-UDP session. Drivers call this; none spawn their own
//! socket loops. Covers: bind+connect, serialized request/response (`query`),
//! fire-and-forget `send`, `/xremote`-style keepalive task, meter broadcast,
//! and `/info`-style string discovery.
use crate::meters::parse_meter_message;
use crate::osc;
use openfader_core::MeterFrame;
use rosc::{OscMessage, OscType};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, broadcast, oneshot};
use tokio::task::JoinHandle;

pub const DEFAULT_QUERY_TIMEOUT: Duration = Duration::from_secs(2);
const RECV_BUF: usize = 65535;

/// Connect-burst query budget (connect/verify + first-paint reads only):
/// one lost datagram here must not cost the steady-state 2 s tail above.
/// Steady-state poller traffic keeps `DEFAULT_QUERY_TIMEOUT` (venue-wifi
/// safety: a slow gig reply must not drop a poll read).
pub const CONNECT_BURST_TIMEOUT: Duration = Duration::from_millis(300);

/// Keepalive config: re-send `addr` (usually `/xremote`) every `interval`.
#[derive(Clone, Debug)]
pub struct KeepaliveConfig {
    pub addr: String,
    pub interval: Duration,
}

impl KeepaliveConfig {
    pub fn xremote() -> Self {
        // ~10 s desk-side expiry is inferred (meters dribble without
        // renewals); 5 s gives two lost datagrams of margin before the
        // desk stops broadcasting change notifications to this session.
        // One extra tiny packet per 5 s is negligible next to the meter
        // stream.
        Self {
            addr: "/xremote".into(),
            interval: Duration::from_secs(5),
        }
    }
}

/// Shared keepalive ticker: the first tick fires immediately, and missed ticks
/// use `Delay` so a stalled loop resumes with one ping instead of a catch-up
/// burst. Used by [`OscSession`] (`/xremote`) and by `MackieSession`
/// (KeepAlive frames); the 50ms meter hub keeps its own default-behavior
/// interval, so it is deliberately not routed through here.
pub fn keepalive_ticker(interval: Duration) -> tokio::time::Interval {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker
}

/// Parse `host[:port]` with a default port (X-Air: 10024).
pub fn resolve_peer(addr: &str, default_port: u16) -> anyhow::Result<SocketAddr> {
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return Ok(sa);
    }
    let host = addr.trim().trim_matches(['[', ']']);
    if host.is_empty() {
        anyhow::bail!("empty mixer address");
    }
    // Try host:port split; fall back to bare host + default port.
    if let Some((h, p)) = host.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            let candidate = format!("{h}:{port}");
            if let Ok(sa) = candidate.parse::<SocketAddr>() {
                return Ok(sa);
            }
        }
    }
    let candidate = format!("{host}:{default_port}");
    candidate
        .parse::<SocketAddr>()
        .map_err(|e| anyhow::anyhow!("bad mixer address {addr:?}: {e}"))
}

/// Shared UDP session. Cloneable via Arc internals through &self methods.
pub struct OscSession {
    socket: Arc<UdpSocket>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<OscMessage>>>>,
    query_lock: Mutex<()>,
    meter_tx: broadcast::Sender<MeterFrame>,
    /// Desk-originated traffic: every non-meter message that matches no
    /// outstanding query (change broadcasts to all `/xremote` subscribers,
    /// other clients' query replies the desk echoes to everyone, stale
    /// late replies). Drivers subscribe here to learn desk-side changes
    /// without polling. Bounded (256); slow readers lag, never block.
    update_tx: broadcast::Sender<OscMessage>,
    received_tx: broadcast::Sender<(std::time::Instant, OscMessage)>,
    latest_meter: Arc<std::sync::Mutex<Option<(std::time::Instant, MeterFrame)>>>,
    _recv: JoinHandle<()>,
    _keepalive: Option<JoinHandle<()>>,
}

impl OscSession {
    /// Bind an ephemeral local port and start the recv loop + optional keepalive.
    pub async fn connect(
        peer: SocketAddr,
        keepalive: Option<KeepaliveConfig>,
    ) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(peer).await?;
        Ok(Self::from_connected_socket(Arc::new(socket), keepalive))
    }

    /// Wrap an already-connected socket (used by tests with loopback pairs).
    pub fn from_connected_socket(
        socket: Arc<UdpSocket>,
        keepalive: Option<KeepaliveConfig>,
    ) -> Self {
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<OscMessage>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (meter_tx, _) = broadcast::channel::<MeterFrame>(64);
        let (update_tx, _) = broadcast::channel::<OscMessage>(256);
        let (received_tx, _) = broadcast::channel(2048);
        let latest_meter = Arc::new(std::sync::Mutex::new(None));

        let recv = {
            let socket = socket.clone();
            let pending = pending.clone();
            let meter_tx = meter_tx.clone();
            let update_tx = update_tx.clone();
            let received_tx = received_tx.clone();
            let latest_meter = latest_meter.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; RECV_BUF];
                loop {
                    let n = match socket.recv(&mut buf).await {
                        Ok(n) => n,
                        Err(error) => {
                            eprintln!("OSC receive error: {error}; retrying same connected peer in 250ms");
                            tokio::time::sleep(Duration::from_millis(250)).await;
                            continue;
                        }
                    };
                    let packet = match osc::decode_packet(&buf[..n]) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    let received = std::time::Instant::now();
                    for msg in osc::flatten(packet) {
                        if let Some(frame) = parse_meter_message(&msg) {
                            *latest_meter.lock().unwrap() = Some((received, frame.clone()));
                            let _ = meter_tx.send(frame);
                            continue;
                        }
                        // Query replies also enter the ordered receive plane. A
                        // late query caller must not hide a newer desk change.
                        let _ = received_tx.send((received, msg.clone()));
                        let mut guard = pending.lock().await;
                        if let Some(tx) = guard.remove(&msg.addr) {
                            let _ = tx.send(msg);
                        } else {
                            // No outstanding query: desk-originated traffic.
                            // Forward a clone to update subscribers (drivers
                            // ingest these into live param caches); drops
                            // when nobody listens, lags slow readers.
                            drop(guard);
                            let _ = update_tx.send(msg);
                        }
                    }
                }
            })
        };

        let keep = keepalive.map(|cfg| {
            let socket = socket.clone();
            tokio::spawn(async move {
                let bytes = osc::query_message(&cfg.addr).unwrap_or_default();
                let mut ticker = keepalive_ticker(cfg.interval);
                loop {
                    ticker.tick().await;
                    let _ = socket.send(&bytes).await;
                }
            })
        });

        Self {
            socket,
            pending,
            query_lock: Mutex::new(()),
            meter_tx,
            update_tx,
            received_tx,
            latest_meter,
            _recv: recv,
            _keepalive: keep,
        }
    }

    /// Fire-and-forget send.
    pub async fn send(&self, addr: &str, args: Vec<OscType>) -> anyhow::Result<()> {
        let bytes = osc::encode_message(addr, args)?;
        self.socket.send(&bytes).await?;
        Ok(())
    }

    /// Send a no-arg query and await the mixer's reply to the same address.
    /// Serialized so concurrent queries never steal each other's reply.
    pub async fn query(&self, addr: &str) -> anyhow::Result<OscMessage> {
        self.query_timeout(addr, DEFAULT_QUERY_TIMEOUT).await
    }

    pub async fn query_timeout(
        &self,
        addr: &str,
        timeout: Duration,
    ) -> anyhow::Result<OscMessage> {
        let _guard = self.query_lock.lock().await;
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(addr.to_string(), tx);
        }
        let bytes = osc::query_message(addr)?;
        self.socket.send(&bytes).await?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(_)) => anyhow::bail!("{addr}: reply channel closed"),
            Err(_) => {
                // Stale entry would steal a later reply; drop it.
                self.pending.lock().await.remove(addr);
                anyhow::bail!("{addr}: timed out after {timeout:?}")
            }
        }
    }

    /// Meter broadcast receiver (fed by the recv loop).
    pub fn subscribe_meters(&self) -> broadcast::Receiver<MeterFrame> {
        self.meter_tx.subscribe()
    }

    /// Desk-originated message receiver (fed by the recv loop): every
    /// non-meter message that matches no outstanding query. Drivers use
    /// this to ingest desk-side change broadcasts without polling.
    pub fn subscribe_updates(&self) -> broadcast::Receiver<OscMessage> {
        self.update_tx.subscribe()
    }

    /// Ordered control receives, timestamped at the socket, including replies.
    pub fn subscribe_received_updates(&self) -> broadcast::Receiver<(std::time::Instant, OscMessage)> {
        self.received_tx.subscribe()
    }

    /// Most recent socket-received level sample; never refresh its age on replay.
    pub fn latest_meter_sample(&self) -> Option<(std::time::Instant, MeterFrame)> {
        self.latest_meter.lock().unwrap().clone()
    }

    /// App-style per-param push subscribe (VERIFIED live 2026-10-19, XR18
    /// FW 1.20 — mixer control software protocol map §4): sends `/subscribe`
    /// sends `/subscribe` with `sub_slot` (string, e.g. `"meters/9"`),
    /// `path` (string) and `long(1)`. The desk then streams that param's
    /// value at ~20/s addressed as the param itself; other-param changes
    /// do not leak in. Stop the stream with
    /// [`unsubscribe`](Self::unsubscribe).
    ///
    /// NOTE: `/subscribe` with one string arg, or with a `/meters/N` path,
    /// elicits NO meter flow — meter blobs need [`batchsubscribe`](Self::batchsubscribe).
    pub async fn subscribe_param(&self, sub_slot: &str, path: &str) -> anyhow::Result<()> {
        self.send(
            "/subscribe",
            vec![
                OscType::String(sub_slot.to_string()),
                OscType::String(path.to_string()),
                OscType::Long(1),
            ],
        )
        .await
    }

    /// App-style meter blob subscribe (VERIFIED live 2026-10-19, XR18 FW
    /// 1.20 — mixer control software protocol map §4): sends `/batchsubscribe`
    /// sends `/batchsubscribe` with `sub_slot` (string, e.g.
    /// `"meters/1"`), `meter_path` (string, e.g. `"/meters/1"`) and `long`
    /// ×3 (zeros; trailing values do not gate the flow, their meaning is
    /// unknown). The desk streams blobs addressed `meters/N` (**no leading
    /// slash**) at ~20/s for ~10 s. Extend a live subscription with
    /// [`renew`](Self::renew); stopping is
    /// [`unsubscribe`](Self::unsubscribe). The no-slash blobs route into
    /// the existing meter parse path (`meters::parse_meter_message`).
    ///
    /// NOT implemented (static-only shapes, live-untested): the multi-path
    /// `/batchsubscribe` variant (n ≤ 64 paths + 3 longs in one message)
    /// and `/formatsubscribe` (several messages per packet).
    pub async fn batchsubscribe(&self, sub_slot: &str, meter_path: &str) -> anyhow::Result<()> {
        self.send(
            "/batchsubscribe",
            vec![
                OscType::String(sub_slot.to_string()),
                OscType::String(meter_path.to_string()),
                OscType::Long(0),
                OscType::Long(0),
                OscType::Long(0),
            ],
        )
        .await
    }

    /// App-style subscription renewal (VERIFIED live 2026-10-19, XR18 FW
    /// 1.20 — mixer control software protocol map §4): sends `/renew` +
    /// `sub_slot` (string). Sent while the subscription is live it EXTENDS
    /// the stream (alive at t+14 s); sent after death it does NOT
    /// resurrect — resubscribe instead.
    pub async fn renew(&self, sub_slot: &str) -> anyhow::Result<()> {
        self.send(
            "/renew",
            vec![OscType::String(sub_slot.to_string())],
        )
        .await
    }

    /// App-style subscription stop (VERIFIED live 2026-10-19, XR18 FW 1.20 —
    /// mixer control software protocol map §4): sends `/unsubscribe` +
    /// `sub_slot` (string). Expect a few in-flight residuals after the stop.
    pub async fn unsubscribe(&self, sub_slot: &str) -> anyhow::Result<()> {
        self.send(
            "/unsubscribe",
            vec![OscType::String(sub_slot.to_string())],
        )
        .await
    }

    /// Raw socket access for driver-specific fire-and-forget flows.
    pub fn socket(&self) -> &Arc<UdpSocket> {
        &self.socket
    }
}

/// Join-handle discipline: dropping a session aborts its recv + keepalive
/// loops. Without this every reconnect (gig wifi flapping) leaks two tasks
/// that hold the old socket `Arc` alive and keep pinging a dead peer.
impl Drop for OscSession {
    fn drop(&mut self) {
        self._recv.abort();
        if let Some(keep) = &self._keepalive {
            keep.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// UDP peer: bound socket with send/recv + timeouts. All socket setup lives
// next to the session that wraps it; import via `crate::session::UdpPeer`
// (or the crate root).
//

/// A bound UDP socket with convenience send/recv helpers.
pub struct UdpPeer {
    socket: UdpSocket,
}

impl UdpPeer {
    /// Bind to `bind_addr` (e.g. `"0.0.0.0:0"` for an ephemeral port).
    /// Set `broadcast(true)` so discovery probes can use `<broadcast>:port`.
    pub async fn bind(bind_addr: &str) -> anyhow::Result<Self> {
        let socket = UdpSocket::bind(bind_addr).await?;
        socket.set_broadcast(true)?;
        Ok(Self { socket })
    }

    /// Bind an ephemeral port with broadcast enabled (discovery default).
    pub async fn ephemeral() -> anyhow::Result<Self> {
        Self::bind("0.0.0.0:0").await
    }

    pub fn local_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(self.socket.local_addr()?)
    }

    pub async fn send_to(&self, data: &[u8], target: &str) -> anyhow::Result<usize> {
        Ok(self.socket.send_to(data, target).await?)
    }

    /// Receive one datagram, returning `(len, from)`. Data lands in `buf`.
    pub async fn recv_from(&self, buf: &mut [u8]) -> anyhow::Result<(usize, SocketAddr)> {
        Ok(self.socket.recv_from(buf).await?)
    }

    /// Receive one datagram with a timeout.
    pub async fn recv_from_timeout(
        &self,
        buf: &mut [u8],
        timeout: Duration,
    ) -> anyhow::Result<(usize, SocketAddr)> {
        Ok(tokio::time::timeout(timeout, self.socket.recv_from(buf)).await??)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    async fn loopback_pair() -> (OscSession, Arc<UdpSocket>) {
        let mixer = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let mixer_addr = mixer.local_addr().unwrap();
        let client_sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        client_sock.connect(mixer_addr).await.unwrap();
        let session = OscSession::from_connected_socket(client_sock, None);
        (session, mixer)
    }

    #[tokio::test]
    async fn query_response_roundtrip() {
        let (session, mixer) = loopback_pair().await;
        let mixer_addr = session.socket().peer_addr().unwrap();
        // Fake mixer: reply to /info with two strings.
        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let (n, peer) = mixer.recv_from(&mut buf).await.unwrap();
            let _ = mixer.connect(peer).await;
            let packet = osc::decode_packet(&buf[..n]).unwrap();
            for msg in osc::flatten(packet) {
                assert_eq!(msg.addr, "/info");
                let _ = mixer_addr;
                let reply = osc::encode_message(
                    "/info",
                    vec![
                        OscType::String("XR18".into()),
                        OscType::String("1.22".into()),
                    ],
                )
                .unwrap();
                mixer.send(&reply).await.unwrap();
            }
        });
        let reply = session
            .query_timeout("/info", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(osc::string_args(&reply), vec!["XR18", "1.22"]);
    }

    #[tokio::test]
    async fn keepalive_config_shape() {
        let cfg = KeepaliveConfig::xremote();
        assert_eq!(cfg.addr, "/xremote");
        assert_eq!(cfg.interval, Duration::from_secs(5));
        // Wire shape of the keepalive packet itself.
        let bytes = osc::query_message(&cfg.addr).unwrap();
        let msgs = osc::flatten(osc::decode_packet(&bytes).unwrap());
        assert_eq!(msgs[0].addr, "/xremote");
        assert!(msgs[0].args.is_empty());
    }

    /// Gig wifi: nobody answers. The query must fail fast with a timeout
    /// error, never hang the caller.
    #[tokio::test]
    async fn query_timeout_is_clean_error() {
        let (session, _mixer) = loopback_pair().await;
        // Nothing reads `_mixer`, so no reply ever arrives.
        let res = tokio::time::timeout(
            Duration::from_secs(5),
            session.query_timeout("/no/reply", Duration::from_millis(50)),
        )
        .await
        .expect("query must not hang");
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("timed out"),
            "unexpected error: {err}"
        );
    }

    /// A timed-out query must drop its pending entry: the late reply to
    /// attempt #1 must not steal the reply slot of attempt #2 (which would
    /// hang it forever on a flaky link).
    #[tokio::test]
    async fn query_timeout_cleans_stale_entry() {
        let (session, mixer) = loopback_pair().await;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            let mut seen = 0u32;
            loop {
                let Ok((n, peer)) = mixer.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    if msg.addr == "/info" && msg.args.is_empty() {
                        seen += 1;
                        if seen >= 2 {
                            let reply = osc::encode_message(
                                "/info",
                                vec![rosc::OscType::String("late".into())],
                            )
                            .unwrap();
                            let _ = mixer.send_to(&reply, peer).await;
                        }
                    }
                }
            }
        });
        let err = session
            .query_timeout("/info", Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("timed out"));
        // Attempt #2 must get its own reply, not hang on a stale slot.
        let reply = session
            .query_timeout("/info", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(osc::string_args(&reply), vec!["late"]);
    }

    /// `UdpPeer` timeout reads fail fast with an error, never block.
    #[tokio::test]
    async fn udp_peer_recv_timeout_is_clean_error() {
        let peer = UdpPeer::ephemeral().await.unwrap();
        let mut buf = vec![0u8; 2048];
        let start = std::time::Instant::now();
        let res = tokio::time::timeout(
            Duration::from_secs(5),
            peer.recv_from_timeout(&mut buf, Duration::from_millis(50)),
        )
        .await
        .expect("recv must not hang");
        assert!(res.is_err(), "expected timeout error, got a datagram");
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// Dropping a session must terminate its recv + keepalive tasks: the
    /// socket `Arc` refcount (session + recv task + keepalive task clones)
    /// must fall back to just the test's own handle.
    #[tokio::test]
    async fn dropped_session_aborts_background_tasks() {
        let mixer = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let mixer_addr = mixer.local_addr().unwrap();
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(mixer_addr).await.unwrap();
        {
            let session = OscSession::from_connected_socket(
                sock.clone(),
                Some(KeepaliveConfig {
                    addr: "/xremote".into(),
                    interval: Duration::from_millis(10),
                }),
            );
            // Ours + session's + recv task's + keepalive task's.
            assert_eq!(Arc::strong_count(&sock), 4);
            drop(session);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if Arc::strong_count(&sock) == 1 {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background tasks leaked: strong_count={}",
                Arc::strong_count(&sock)
            );
            tokio::task::yield_now().await;
        }
    }

    /// After drop, the keepalive task must stop pinging: drain in-flight
    /// pings, wait several intervals, then assert radio silence.
    #[tokio::test]
    async fn dropped_session_stops_keepalive() {
        let mixer = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let mixer_addr = mixer.local_addr().unwrap();
        let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        sock.connect(mixer_addr).await.unwrap();
        let session = OscSession::from_connected_socket(
            sock,
            Some(KeepaliveConfig {
                addr: "/xremote".into(),
                interval: Duration::from_millis(10),
            }),
        );
        // Warm up: at least two pings arrive while the session lives.
        let mut buf = vec![0u8; 2048];
        let mut got = 0;
        while got < 2 {
            tokio::time::timeout(Duration::from_millis(500), mixer.recv(&mut buf))
                .await
                .expect("keepalive should fire while session lives")
                .unwrap();
            got += 1;
        }
        drop(session);
        // Drain pings already in flight, then require silence.
        loop {
            match mixer.try_recv(&mut buf) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        match mixer.try_recv(&mut buf) {
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            other => panic!("keepalive task survived drop: {other:?}"),
        }
    }

    /// Reconnect storm: rapid connect/drop cycles (flapping venue wifi)
    /// must neither hang nor poison the next session.
    #[tokio::test]
    async fn reconnect_storm_stays_usable() {
        let blackhole = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let peer = blackhole.local_addr().unwrap();
        let start = std::time::Instant::now();
        for _ in 0..30 {
            let sock = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
            sock.connect(peer).await.unwrap();
            let session = OscSession::from_connected_socket(
                sock,
                Some(KeepaliveConfig {
                    addr: "/xremote".into(),
                    interval: Duration::from_millis(5),
                }),
            );
            drop(session);
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "reconnect storm hung"
        );
        // A fresh session on a answering mixer still round-trips.
        let (session, mixer) = loopback_pair().await;
        tokio::spawn(async move {
            let mut buf = vec![0u8; 2048];
            let Ok((n, peer)) = mixer.recv_from(&mut buf).await else {
                return;
            };
            let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                return;
            };
            for msg in osc::flatten(packet) {
                if msg.addr == "/info" {
                    let reply = osc::encode_message(
                        "/info",
                        vec![rosc::OscType::String("back".into())],
                    )
                    .unwrap();
                    let _ = mixer.send_to(&reply, peer).await;
                }
            }
        });
        let reply = session
            .query_timeout("/info", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(osc::string_args(&reply), vec!["back"]);
    }

    /// Slow meter consumer must not kill the relay: flood 200 blobs (past
    /// the 64-frame broadcast buffer) with nobody reading, then prove a
    /// query still round-trips and the lagging reader sees `Lagged`
    /// (skip-and-continue, the semantics the serve relay relies on).
    #[tokio::test]
    async fn meter_flood_does_not_block_queries() {
        let (session, mixer) = loopback_pair().await;
        let mut meters = session.subscribe_meters();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65535];
            loop {
                let Ok((n, peer)) = mixer.recv_from(&mut buf).await else {
                    break;
                };
                let Ok(packet) = osc::decode_packet(&buf[..n]) else {
                    continue;
                };
                for msg in osc::flatten(packet) {
                    if msg.addr == "/info" && msg.args.is_empty() {
                        for _ in 0..200 {
                            let blob = crate::meters::build_meter_blob(&[0.1, 0.5]);
                            let bytes = osc::encode_message(
                                "/meters/1",
                                vec![rosc::OscType::Blob(blob)],
                            )
                            .unwrap();
                            let _ = mixer.send_to(&bytes, peer).await;
                        }
                        let reply = osc::encode_message(
                            "/info",
                            vec![rosc::OscType::String("ok".into())],
                        )
                        .unwrap();
                        let _ = mixer.send_to(&reply, peer).await;
                    }
                }
            }
        });
        let reply = session
            .query_timeout("/info", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(osc::string_args(&reply), vec!["ok"]);
        match meters.try_recv() {
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            other => panic!("expected lagged meter backlog, got {other:?}"),
        }
    }

    /// Read one datagram off the fake mixer and flatten it to a single message.
    async fn capture_one(mixer: &Arc<UdpSocket>) -> OscMessage {
        let mut buf = vec![0u8; 65535];
        let (n, _) = tokio::time::timeout(Duration::from_secs(2), mixer.recv_from(&mut buf))
            .await
            .expect("desk datagram must arrive")
            .unwrap();
        let mut msgs = osc::flatten(osc::decode_packet(&buf[..n]).unwrap());
        assert_eq!(msgs.len(), 1);
        msgs.pop().unwrap()
    }

    /// §4 VERIFIED wire form: `/subscribe meters/9 /ch/14/mix/fader 1`
    /// (sub + path strings, `long` count).
    #[tokio::test]
    async fn subscribe_param_wire_shape() {
        let (session, mixer) = loopback_pair().await;
        session
            .subscribe_param("meters/9", "/ch/14/mix/fader")
            .await
            .unwrap();
        let msg = capture_one(&mixer).await;
        assert_eq!(msg.addr, "/subscribe");
        assert_eq!(
            msg.args,
            vec![
                OscType::String("meters/9".into()),
                OscType::String("/ch/14/mix/fader".into()),
                OscType::Long(1),
            ]
        );
    }

    /// §4 VERIFIED wire form: `/batchsubscribe meters/1 /meters/1 0 0 0`
    /// (sub + path strings, three `long` zeros).
    #[tokio::test]
    async fn batchsubscribe_wire_shape() {
        let (session, mixer) = loopback_pair().await;
        session
            .batchsubscribe("meters/1", "/meters/1")
            .await
            .unwrap();
        let msg = capture_one(&mixer).await;
        assert_eq!(msg.addr, "/batchsubscribe");
        assert_eq!(
            msg.args,
            vec![
                OscType::String("meters/1".into()),
                OscType::String("/meters/1".into()),
                OscType::Long(0),
                OscType::Long(0),
                OscType::Long(0),
            ]
        );
    }

    /// §4 VERIFIED wire forms: `/renew` + sub, `/unsubscribe` + sub
    /// (single string arg each).
    #[tokio::test]
    async fn renew_and_unsubscribe_wire_shapes() {
        let (session, mixer) = loopback_pair().await;
        session.renew("meters/1").await.unwrap();
        let msg = capture_one(&mixer).await;
        assert_eq!(msg.addr, "/renew");
        assert_eq!(msg.args, vec![OscType::String("meters/1".into())]);
        session.unsubscribe("meters/9").await.unwrap();
        let msg = capture_one(&mixer).await;
        assert_eq!(msg.addr, "/unsubscribe");
        assert_eq!(msg.args, vec![OscType::String("meters/9".into())]);
    }

    /// Desk-addressed blobs WITHOUT the leading slash (`meters/1`, the
    /// `/batchsubscribe` reply form) must reach the meter channel exactly
    /// like the slashed form — both through the session recv loop.
    #[tokio::test]
    async fn no_slash_meter_blob_reaches_meter_channel() {
        let (session, mixer) = loopback_pair().await;
        let mut meters = session.subscribe_meters();
        let client = session.socket().local_addr().unwrap();
        for addr in ["meters/1", "/meters/1"] {
            let blob = crate::meters::build_meter_blob(&[0.1, 0.5]);
            let bytes =
                osc::encode_message(addr, vec![rosc::OscType::Blob(blob)]).unwrap();
            mixer.send_to(&bytes, client).await.unwrap();
            let frame = tokio::time::timeout(Duration::from_secs(2), meters.recv())
                .await
                .expect("meter frame must arrive")
                .unwrap();
            assert_eq!(frame.levels.len(), 2, "{addr}");
        }
    }
}
