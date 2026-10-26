//! Bounded connect-time host/port probing.
use super::*;

/// True when `addr` carries an explicit `:port` (or parses as a full
/// `SocketAddr`): the connect sweep leaves those alone and dials as-is.
/// Mirrors [`openfader_transport::session::resolve_peer`]'s host/port split
/// (bare IPv6 without brackets is treated as a bare host, same as there).
pub(super) fn has_explicit_port(addr: &str) -> bool {
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
pub(super) async fn probe_port(host: &str, port: u16) -> bool {
    use openfader_transport::session::resolve_peer;
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
