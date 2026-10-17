//! Generic OSC-over-UDP session. Drivers call this; none spawn their own
//! socket loops. Covers: bind+connect, serialized request/response (`query`),
//! fire-and-forget `send`, `/xremote`-style keepalive task, meter broadcast,
//! and `/info`-style string discovery.
use crate::meters::parse_meter_message;
use crate::osc;
use openmix_core::MeterFrame;
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

/// Keepalive config: re-send `addr` (usually `/xremote`) every `interval`.
#[derive(Clone, Debug)]
pub struct KeepaliveConfig {
    pub addr: String,
    pub interval: Duration,
}

impl KeepaliveConfig {
    pub fn xremote() -> Self {
        Self {
            addr: "/xremote".into(),
            interval: Duration::from_secs(8),
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

        let recv = {
            let socket = socket.clone();
            let pending = pending.clone();
            let meter_tx = meter_tx.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; RECV_BUF];
                loop {
                    let n = match socket.recv(&mut buf).await {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    let packet = match osc::decode_packet(&buf[..n]) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    for msg in osc::flatten(packet) {
                        if let Some(frame) = parse_meter_message(&msg) {
                            let _ = meter_tx.send(frame);
                            continue;
                        }
                        let mut guard = pending.lock().await;
                        if let Some(tx) = guard.remove(&msg.addr) {
                            let _ = tx.send(msg);
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

    /// Raw socket access for driver-specific fire-and-forget flows.
    pub fn socket(&self) -> &Arc<UdpSocket> {
        &self.socket
    }
}

// ---------------------------------------------------------------------------
// UDP peer: bound socket with send/recv + timeouts.
// Merged here from the old `udp` module so all socket setup lives next to the
// session that wraps it; `crate::udp::UdpPeer` still resolves via the shim.
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
        assert_eq!(cfg.interval, Duration::from_secs(8));
        // Wire shape of the keepalive packet itself.
        let bytes = osc::query_message(&cfg.addr).unwrap();
        let msgs = osc::flatten(osc::decode_packet(&bytes).unwrap());
        assert_eq!(msgs[0].addr, "/xremote");
        assert!(msgs[0].args.is_empty());
    }
}
