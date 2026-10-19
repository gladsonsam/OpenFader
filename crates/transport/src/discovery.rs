//! Discovery helpers: generic UDP broadcast probe + thin per-driver tables.
//!
//! Modularity rule: all socket loops, timeouts, and DNS/OSC byte handling
//! live here. A new provider adds one [`ProbeSpec`] row (address table) and
//! zero changes to core. No per-driver bespoke loops.
use std::net::SocketAddr;
use std::time::Duration;

use crate::session::UdpPeer;

/// X-Air OSC port (XR12/16/18, MR18).
pub const XAIR_PORT: u16 = 10024;
/// mDNS multicast endpoint.
pub const MDNS_ADDR: &str = "224.0.0.251:5353";

/// Thin per-driver mapping table: what to send, where, and what reply prefix
/// identifies the device. New providers add a row; the probe loop is shared.
#[derive(Clone, Copy, Debug)]
pub struct ProbeSpec {
    /// Driver key, e.g. `"xair"`.
    pub driver: &'static str,
    /// UDP destination port.
    pub port: u16,
    /// Raw datagram to broadcast.
    pub payload: &'static [u8],
    /// Expected reply prefix (matched against reply bytes as lossy UTF-8).
    pub reply_prefix: &'static str,
}

/// OSC `/xinfo` probe (no-arg call: addr + ","). X-Air replies with
/// `/xinfo` + `,ssss` + `[ip, name, model, fw]` (see [`parse_info_reply`]).
pub const XINFO_PROBE: &[u8] = b"/xinfo\0\0,";
/// Legacy `/info` probe (older X-Air firmware).
pub const INFO_PROBE: &[u8] = b"/info\0\0\0,";

/// Per-driver probe table. DL32S intentionally has no broadcast probe yet:
/// its protocol is closed pending own-LAN capture (see docs/dl32s-protocol.md).
pub const PROBE_TABLE: &[ProbeSpec] = &[
    ProbeSpec {
        driver: "xair",
        port: XAIR_PORT,
        payload: XINFO_PROBE,
        reply_prefix: "/xinfo",
    },
    ProbeSpec {
        driver: "xair",
        port: XAIR_PORT,
        payload: INFO_PROBE,
        reply_prefix: "/info",
    },
];

/// A discovered device on the LAN.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredDevice {
    pub driver: &'static str,
    pub addr: SocketAddr,
    pub model: String,
    pub raw: String,
}

/// Parse an X-Air `/xinfo` or `/info` reply into `(model, raw)`.
/// Replies are OSC-ish blobs; parse best-effort as lossy text and split on
/// NUL/comma boundaries. Returns `None` when the prefix does not match.
///
/// Wire shape (captured from an XR18, FW 1.20):
/// `/xinfo` + `,ssss` + `[ip, name, model, fw]`; `/info` + `,ssss` +
/// `[ver, name, model, fw]`. The console NAME may contain spaces ("Home
/// Studio"), so the model is the second-to-last string arg — positional
/// `nth(2)` reads the IP instead. Bare replies (one or zero args) fall
/// back to the single arg / `"unknown"`, the old behaviour.
pub fn parse_info_reply(buf: &[u8], reply_prefix: &str) -> Option<(String, String)> {
    let text: String = String::from_utf8_lossy(buf)
        .chars()
        .map(|c| if c == '\0' || c == ',' { ' ' } else { c })
        .collect();
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let tokens: Vec<&str> = compact.split_whitespace().collect();
    if tokens.first() != Some(&reply_prefix) {
        return None;
    }
    let mut args = &tokens[1..];
    // OSC type-tag token first (`ssss`): skip it, it is not an arg.
    if let Some((tag, rest)) = args.split_first() {
        if !tag.is_empty() && tag.chars().all(|c| c == 's') {
            args = rest;
        }
    }
    let model = if args.len() >= 2 {
        args[args.len() - 2]
    } else {
        args.first().copied().unwrap_or("unknown")
    }
    .to_string();
    Some((model, compact))
}

/// Generic broadcast probe loop shared by all drivers: send `spec.payload`
/// to `<broadcast>:port`, collect matching replies until `timeout`.
///
/// Two destinations, one loop: the limited broadcast (`255.255.255.255`)
/// plus every local subnet's directed broadcast (`192.168.1.255`-style,
/// from [`subnet_broadcasts`]). Phone WiFi stacks and venue APs often
/// swallow the limited broadcast while forwarding directed ones (or vice
/// versa); sending to both costs one extra datagram per interface and
/// keeps discovery working on either. Replies are unicast to our ephemeral
/// port, so no multicast lock or extra permission is needed to receive
/// them — plain `INTERNET` covers send + receive on Android.
pub async fn probe(spec: &ProbeSpec, timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let peer = UdpPeer::ephemeral().await?;
    let mut targets = vec![format!("255.255.255.255:{}", spec.port)];
    for bcast in subnet_broadcasts() {
        let t = format!("{bcast}:{}", spec.port);
        if !targets.contains(&t) {
            targets.push(t);
        }
    }
    for t in &targets {
        // Best-effort per target: one unreachable interface must not
        // abort the whole scan (VPNs, down links, no-IPv4 routes).
        let _ = peer.send_to(spec.payload, t).await;
    }
    let deadline = tokio::time::Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match peer.recv_from_timeout(&mut buf, deadline - now).await {
            Ok((len, from)) => {
                if let Some((model, raw)) = parse_info_reply(&buf[..len], spec.reply_prefix) {
                    out.push(DiscoveredDevice {
                        driver: spec.driver,
                        addr: from,
                        model,
                        raw,
                    });
                }
            }
            Err(_) => break, // timeout => done collecting
        }
    }
    Ok(out)
}

/// Directed unicast probe for a single address: send `spec.payload` to
/// `ip:port` (NOT broadcast) and collect matching replies until `timeout`.
/// Cross-subnet desks never answer broadcast discovery, but they DO answer
/// a unicast `/xinfo` — this is the manual-add and linked-desk verify path.
pub async fn probe_addr(
    spec: &ProbeSpec,
    ip: std::net::IpAddr,
    timeout: Duration,
) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let peer = UdpPeer::ephemeral().await?;
    let target = std::net::SocketAddr::new(ip, spec.port).to_string();
    peer.send_to(spec.payload, &target).await?;
    let deadline = tokio::time::Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = [0u8; 2048];
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match peer.recv_from_timeout(&mut buf, deadline - now).await {
            Ok((len, from)) => {
                if let Some((model, raw)) = parse_info_reply(&buf[..len], spec.reply_prefix) {
                    out.push(DiscoveredDevice {
                        driver: spec.driver,
                        addr: from,
                        model,
                        raw,
                    });
                }
            }
            Err(_) => break, // timeout => done collecting
        }
    }
    Ok(out)
}

/// Directed broadcast for one interface (`192.168.1.20` + `255.255.255.0`
/// -> `192.168.1.255`). Pure bit math, no I/O — unit-tested below.
pub fn directed_broadcast(ip: std::net::Ipv4Addr, mask: std::net::Ipv4Addr) -> std::net::Ipv4Addr {
    let b = u32::from(ip) | !u32::from(mask);
    std::net::Ipv4Addr::from(b)
}

/// Every local subnet's directed broadcast address (deduped, sorted):
/// one `ip | !mask` per up IPv4 interface, loopback included (so
/// loopback self-tests exercise the same fan-out the phone uses).
/// Interfaces that fail to enumerate yield an empty list and the caller
/// still sends the limited broadcast — discovery degrades, never dies.
pub fn subnet_broadcasts() -> Vec<std::net::IpAddr> {
    let mut out: Vec<std::net::IpAddr> = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        if let if_addrs::IfAddr::V4(v4) = iface.addr {
            // /31 and /32 have no broadcast address (point-to-point).
            let prefix = v4
                .netmask
                .octets()
                .iter()
                .map(|o| o.count_ones())
                .sum::<u32>();
            if prefix >= 31 {
                continue;
            }
            let bcast = std::net::IpAddr::V4(directed_broadcast(v4.ip, v4.netmask));
            if !out.contains(&bcast) {
                out.push(bcast);
            }
        }
    }
    out.sort();
    out
}

/// X-Air discovery: run every X-Air row of [`PROBE_TABLE`], dedupe by addr.
pub async fn discover_xair(timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    for spec in PROBE_TABLE.iter().filter(|s| s.driver == "xair") {
        for dev in probe(spec, timeout).await? {
            if !out.iter().any(|d| d.addr == dev.addr) {
                out.push(dev);
            }
        }
    }
    Ok(out)
}

/// Generic discovery: run every row of [`PROBE_TABLE`], dedupe by addr.
/// A new provider adds its rows to the table; this loop needs no edits.
pub async fn discover_all(timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    for spec in PROBE_TABLE {
        for dev in probe(spec, timeout).await? {
            if !out.iter().any(|d| d.addr == dev.addr) {
                out.push(dev);
            }
        }
    }
    Ok(out)
}

/// Build a minimal mDNS (DNS-SD) PTR query for `service`
/// (e.g. `"_xairstage._udp.local."`). Shared helper; providers only pass
/// their service string.
pub fn mdns_query(service: &str) -> Vec<u8> {
    let mut pkt = vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    for label in service.trim_end_matches('.').split('.') {
        pkt.push(label.len() as u8);
        pkt.extend_from_slice(label.as_bytes());
    }
    pkt.push(0x00);
    // QTYPE PTR (12), QCLASS IN (1).
    pkt.extend_from_slice(&[0x00, 0x0c, 0x00, 0x01]);
    pkt
}

/// Best-effort mDNS probe: multicast a PTR query, collect raw response
/// datagrams until `timeout`. Name decoding stays with the caller.
pub async fn discover_mdns(
    service: &str,
    timeout: Duration,
) -> anyhow::Result<Vec<(SocketAddr, Vec<u8>)>> {
    let peer = UdpPeer::bind("0.0.0.0:0").await?;
    let query = mdns_query(service);
    // Multicast send may fail on hosts without multicast; surface the error.
    peer.send_to(&query, MDNS_ADDR).await?;
    let deadline = tokio::time::Instant::now() + timeout;
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match peer.recv_from_timeout(&mut buf, deadline - now).await {
            Ok((len, from)) => out.push((from, buf[..len].to_vec())),
            Err(_) => break,
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    /// Subnet broadcast math: the exact addresses discovery fans out to.
    #[test]
    fn directed_broadcast_math() {
        let b = |ip: &str, mask: &str| {
            directed_broadcast(ip.parse().unwrap(), mask.parse().unwrap()).to_string()
        };
        assert_eq!(b("192.168.1.20", "255.255.255.0"), "192.168.1.255");
        assert_eq!(b("10.0.5.7", "255.255.0.0"), "10.0.255.255");
        assert_eq!(b("172.16.3.9", "255.255.255.252"), "172.16.3.11");
        assert_eq!(b("127.0.0.1", "255.0.0.0"), "127.255.255.255");
    }

    /// Enumeration never panics and never emits /31+/32 (no broadcast
    /// address there); loopback's /8 broadcast is always present on a
    /// normal host, which is what the loopback discovery test below
    /// relies on.
    #[test]
    fn subnet_broadcasts_shape() {
        let list = subnet_broadcasts();
        assert!(list.contains(&std::net::IpAddr::V4(Ipv4Addr::new(127, 255, 255, 255))));
        let mut sorted = list.clone();
        sorted.sort();
        assert_eq!(list, sorted, "broadcast list is deduped + sorted");
    }

    /// Real captured replies (XR18 FW 1.20, console named "Home Studio"):
    /// the model is the second-to-last string arg — positional `nth(2)`
    /// would read the IP ("192.168.40.123") or version ("V0.04") instead.
    #[test]
    fn parse_info_reply_reads_model_not_ip() {
        let xinfo = b"/xinfo\x00\x00,ssss\x00\x00\x00192.168.40.123\x00\x00Home Studio\x00XR18\x00\x00\x00\x001.20\x00\x00\x00\x00";
        let (model, raw) = parse_info_reply(xinfo, "/xinfo").unwrap();
        assert_eq!(model, "XR18");
        assert!(raw.starts_with("/xinfo"));
        let info = b"/info\x00\x00\x00,ssss\x00\x00\x00V0.04\x00\x00\x00Home Studio\x00XR18\x00\x00\x00\x001.20\x00\x00\x00\x00";
        let (model, _) = parse_info_reply(info, "/info").unwrap();
        assert_eq!(model, "XR18");
        // Wrong prefix never matches; bare replies keep the old fallback.
        assert!(parse_info_reply(xinfo, "/info").is_none());
        assert_eq!(
            parse_info_reply(b"/xinfo\x00\x00,", "/xinfo").unwrap().0,
            "unknown"
        );
    }

    /// End-to-end over loopback: a fake X-Air answering `/xinfo` is found
    /// by `discover_xair` through the broadcast fan-out — the same
    /// send/receive path the phone uses, minus the radio. The fake binds
    /// 0.0.0.0 (like a real desk): a 127.0.0.1-bound socket never sees
    /// broadcast datagrams, only unicast.
    #[tokio::test]
    async fn discover_xair_finds_loopback_desk() {
        let sock = tokio::net::UdpSocket::bind("0.0.0.0:10024").await.unwrap();
        let task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            // Answer up to a few probes (/xinfo + /info rows + fan-out).
            for _ in 0..8 {
                let res =
                    tokio::time::timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await;
                let Ok(Ok((n, peer))) = res else {
                    break;
                };
                let text = String::from_utf8_lossy(&buf[..n]).replace('\0', " ");
                let reply = if text.starts_with("/xinfo") {
                    "/xinfo 127.0.0.1 XR18 1.22"
                } else if text.starts_with("/info") {
                    "/info XR18 1.22"
                } else {
                    continue;
                };
                let _ = sock.send_to(reply.as_bytes(), peer).await;
            }
        });
        let found = discover_xair(Duration::from_secs(3)).await.unwrap();
        task.abort();
        assert!(
            found
                .iter()
                .any(|d| d.model == "XR18" || d.model == "127.0.0.1"),
            "loopback desk must answer the broadcast fan-out (got {found:?})"
        );
    }

    /// Directed unicast over loopback: `probe_addr` reaches one address
    /// without any broadcast — the manual add-by-IP path.
    #[tokio::test]
    async fn probe_addr_reaches_loopback_desk() {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:10025")
            .await
            .unwrap();
        let task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let res = tokio::time::timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await;
            let Ok(Ok((n, peer))) = res else {
                return;
            };
            let text = String::from_utf8_lossy(&buf[..n]).replace('\0', " ");
            assert!(text.starts_with("/xinfo"), "unicast probe spelling (got {text:?})");
            let _ = sock.send_to(b"/xinfo 127.0.0.1 XR18 1.22", peer).await;
        });
        let spec = ProbeSpec {
            driver: "xair",
            port: 10025,
            payload: XINFO_PROBE,
            reply_prefix: "/xinfo",
        };
        let found = probe_addr(&spec, "127.0.0.1".parse().unwrap(), Duration::from_secs(2))
            .await
            .unwrap();
        task.abort();
        assert_eq!(found.len(), 1, "one reply, one row");
        assert_eq!(found[0].model, "XR18");
    }
}
