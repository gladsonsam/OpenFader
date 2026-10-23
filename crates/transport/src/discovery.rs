//! Discovery helpers: generic UDP broadcast probe + thin per-driver tables.
//!
//! Modularity rule: all socket loops, timeouts, and DNS/OSC byte handling
//! live here. A new provider adds one [`ProbeSpec`] row (address table) and
//! zero changes to core. No per-driver bespoke loops.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::session::UdpPeer;

/// X-Air OSC port (XR12/16/18, MR18).
pub const XAIR_PORT: u16 = 10024;
/// mDNS multicast endpoint.
pub const MDNS_ADDR: &str = "224.0.0.251:5353";

/// Probe sends per socket inside one discovery budget. One lost datagram
/// (or a desk whose network stack is still waking) must not produce an
/// empty scan.
pub const PROBE_ATTEMPTS: usize = 3;
/// Spacing between probe attempts (see [`attempt_spacing`] for budgets too
/// small to fit it).
pub const PROBE_ATTEMPT_SPACING: Duration = Duration::from_millis(300);
/// Default bounded /24 sweep budget: the engine scan re-find path.
pub const SWEEP_TIMEOUT: Duration = Duration::from_millis(1200);

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
/// Sockets and destinations, cheapest-first:
/// 1. one ephemeral socket that owns the limited broadcast
///    (`255.255.255.255`) plus every local subnet's directed broadcast
///    (`192.168.1.255`-style, from [`subnet_broadcasts`]);
/// 2. one socket bound to EACH local IPv4 address (port 0, broadcast
///    enabled) that owns that interface's directed broadcast (and the
///    limited broadcast again). A phone with VPN/cellular/Wi-Fi interfaces
///    may route `255.255.255.255` out the wrong radio — a socket pinned to
///    the interface address can only leave through that interface, so the
///    datagram reaches the Wi-Fi subnet the desk is actually on.
///
/// Every socket repeats its sends [`PROBE_ATTEMPTS`] times (spaced
/// [`PROBE_ATTEMPT_SPACING`], see [`attempt_spacing`]) inside the same
/// timeout and keeps collecting replies in between, so one lost datagram or
/// a slow desk does not end a scan early. All sockets run concurrently and
/// rows are deduped by `addr`; one unreachable interface (VPN down, no IPv4
/// route) never aborts the scan.
///
/// Replies are unicast to our ephemeral ports, so no multicast lock or
/// extra permission is needed to receive them — plain `INTERNET` covers
/// send + receive on Android.
pub async fn probe(spec: &ProbeSpec, timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let spec = *spec;
    let mut set = tokio::task::JoinSet::new();

    // 1. Shared socket: limited broadcast + every local directed broadcast.
    let peer = UdpPeer::ephemeral().await?;
    let global_targets = probe_targets(&subnet_broadcasts_v4(), spec.port);
    set.spawn(async move { probe_on(&peer, &global_targets, &spec, timeout).await });

    // 2. One pinned socket per local IPv4 address.
    for iface in local_ipv4_interfaces() {
        let Ok(sock) = UdpPeer::bind(&format!("{}:0", iface.ip)).await else {
            continue; // interface cannot send (VPN alias, gone) — best-effort
        };
        let broadcasts: Vec<Ipv4Addr> = iface.broadcast().into_iter().collect();
        let targets = probe_targets(&broadcasts, spec.port);
        set.spawn(async move { probe_on(&sock, &targets, &spec, timeout).await });
    }

    let mut out: Vec<DiscoveredDevice> = Vec::new();
    while let Some(joined) = set.join_next().await {
        let rows = joined.map_err(|e| anyhow::anyhow!("discovery probe task failed: {e}"))?;
        for dev in rows {
            if !out.iter().any(|d| d.addr == dev.addr) {
                out.push(dev);
            }
        }
    }
    Ok(out)
}

/// One socket's probe loop: `PROBE_ATTEMPTS` send rounds spaced by
/// [`attempt_spacing`], collecting matching replies the whole time (between
/// sends and until `timeout`). Deduped by sender address.
async fn probe_on(
    peer: &UdpPeer,
    targets: &[String],
    spec: &ProbeSpec,
    timeout: Duration,
) -> Vec<DiscoveredDevice> {
    let deadline = tokio::time::Instant::now() + timeout;
    let spacing = attempt_spacing(timeout);
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    let mut buf = [0u8; 2048];
    let mut sent = 0usize;
    while sent < PROBE_ATTEMPTS {
        // Best-effort per target: one unreachable interface must not abort
        // the whole scan (VPNs, down links, no-IPv4 routes).
        for t in targets {
            let _ = peer.send_to(spec.payload, t).await;
        }
        sent += 1;
        let next_send = tokio::time::Instant::now() + spacing;
        // Collect until the next send round or the deadline.
        loop {
            let now = tokio::time::Instant::now();
            let until = next_send.min(deadline);
            if now >= until {
                break;
            }
            match peer.recv_from_timeout(&mut buf, until - now).await {
                Ok((len, from)) => collect_reply(&mut out, spec, &buf[..len], from),
                Err(_) => break, // next send round, or the deadline
            }
        }
    }
    // All attempts are out: keep collecting until the deadline (a desk may
    // answer the last attempt a moment later).
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match peer.recv_from_timeout(&mut buf, deadline - now).await {
            Ok((len, from)) => collect_reply(&mut out, spec, &buf[..len], from),
            Err(_) => break,
        }
    }
    out
}

/// Parse one datagram and append a row, deduped by sender address (a desk
/// answers every attempt; the first reply per address wins).
fn collect_reply(
    out: &mut Vec<DiscoveredDevice>,
    spec: &ProbeSpec,
    buf: &[u8],
    from: SocketAddr,
) {
    if out.iter().any(|d| d.addr == from) {
        return;
    }
    if let Some((model, raw)) = parse_info_reply(buf, spec.reply_prefix) {
        out.push(DiscoveredDevice {
            driver: spec.driver,
            addr: from,
            model,
            raw,
        });
    }
}

/// Spacing between probe attempts for `timeout`: the configured
/// [`PROBE_ATTEMPT_SPACING`] when all attempts fit, otherwise the budget
/// split evenly so short test/scan budgets still send every attempt.
/// Pure: unit-tested below.
pub fn attempt_spacing(timeout: Duration) -> Duration {
    let needed = PROBE_ATTEMPT_SPACING * PROBE_ATTEMPTS as u32;
    if timeout >= needed {
        PROBE_ATTEMPT_SPACING
    } else {
        timeout / PROBE_ATTEMPTS as u32
    }
}

/// Broadcast destinations for one socket: the limited broadcast first,
/// then each directed broadcast (deduped, order preserved). Pure:
/// unit-tested below.
pub fn probe_targets(broadcasts: &[Ipv4Addr], port: u16) -> Vec<String> {
    let mut out = vec![format!("255.255.255.255:{port}")];
    for b in broadcasts {
        let t = format!("{b}:{port}");
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
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
                collect_reply(&mut out, spec, &buf[..len], from);
                // One directed target answers once: stop at the first valid
                // reply, so Add-by-IP connects in one round trip instead of
                // waiting out the verify timeout after the desk answered.
                if !out.is_empty() {
                    break;
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

/// One local IPv4 interface: address + netmask. Pure data (no I/O) so the
/// per-interface fan-out is testable without touching the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalIfaceV4 {
    pub ip: Ipv4Addr,
    pub netmask: Ipv4Addr,
}

impl LocalIfaceV4 {
    /// Directed broadcast of this interface; `None` for /31 and /32
    /// (point-to-point, no broadcast address).
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        if prefix_len(self.netmask) >= 31 {
            None
        } else {
            Some(directed_broadcast(self.ip, self.netmask))
        }
    }
}

/// Netmask popcount (`255.255.255.0` -> 24).
pub fn prefix_len(mask: Ipv4Addr) -> u32 {
    mask.octets().iter().map(|o| o.count_ones()).sum()
}

/// Every local IPv4 interface (deduped, sorted by address), loopback
/// included so loopback self-tests exercise the same per-interface fan-out
/// the phone uses. Enumeration failure yields an empty list; discovery
/// degrades (limited broadcast only), never dies.
pub fn local_ipv4_interfaces() -> Vec<LocalIfaceV4> {
    let mut out: Vec<LocalIfaceV4> = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return out;
    };
    for iface in ifaces {
        if let if_addrs::IfAddr::V4(v4) = iface.addr {
            let row = LocalIfaceV4 {
                ip: v4.ip,
                netmask: v4.netmask,
            };
            if !out.contains(&row) {
                out.push(row);
            }
        }
    }
    out.sort_by_key(|r| u32::from(r.ip));
    out
}

/// Every local subnet's directed broadcast address (deduped, sorted):
/// one `ip | !mask` per up IPv4 interface, loopback included (so
/// loopback self-tests exercise the same fan-out the phone uses).
/// Interfaces that fail to enumerate yield an empty list and the caller
/// still sends the limited broadcast — discovery degrades, never dies.
pub fn subnet_broadcasts() -> Vec<std::net::IpAddr> {
    subnet_broadcasts_v4()
        .into_iter()
        .map(std::net::IpAddr::V4)
        .collect()
}

/// [`subnet_broadcasts`] without the `IpAddr` wrapping (the `probe` fan-out
/// builds target strings from these).
fn subnet_broadcasts_v4() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = Vec::new();
    for iface in local_ipv4_interfaces() {
        if let Some(b) = iface.broadcast() {
            if !out.contains(&b) {
                out.push(b);
            }
        }
    }
    out.sort();
    out
}

/// The /24 prefix of an address: `192.168.1.123` -> `192.168.1.0`.
/// Accepts a known desk IP or an already-masked prefix (idempotent).
/// Pure: unit-tested below.
pub fn subnet24(ip: Ipv4Addr) -> Ipv4Addr {
    Ipv4Addr::from(u32::from(ip) & 0xFFFF_FF00)
}

/// Hosts `.1`..=`.254` of a /24, minus `exclude` (this host's own
/// addresses). Pure: unit-tested below.
pub fn sweep_targets(prefix: Ipv4Addr, exclude: &[Ipv4Addr]) -> Vec<Ipv4Addr> {
    let base = u32::from(prefix) & 0xFFFF_FF00;
    (1..=254u32)
        .map(|host| Ipv4Addr::from(base | host))
        .filter(|ip| !exclude.contains(ip))
        .collect()
}

/// Whether a /24 is worth sweeping at all: not the unspecified block,
/// not loopback, not multicast/reserved (`>= 224.0.0.0`), not the
/// limited-broadcast address. Link-local (169.254/16) is kept — a desk
/// with a failed DHCP lease can live there. Pure: unit-tested below.
pub fn sweepable_prefix(prefix: Ipv4Addr) -> bool {
    let o = prefix.octets();
    if o[0] == 0 || o[0] == 127 || o[0] >= 224 {
        return false;
    }
    prefix != Ipv4Addr::new(255, 255, 255, 0)
}

/// Bounded unicast /24 sweep: given a known desk IP (or its /24 prefix),
/// probe hosts `.1`..=`.254` with every `specs` payload over ONE ephemeral
/// socket and collect matching replies until `timeout`
/// ([`SWEEP_TIMEOUT`] for scans).
///
/// This is how a scan re-finds a desk whose DHCP lease moved (same /24,
/// different host) and finds sibling desks on a known desk subnet — even
/// when broadcast discovery cannot cross the router. It is deliberately
/// NEVER called without a known desk address: no arbitrary range scans.
/// IPv6 seeds, loopback/reserved prefixes and hostnames yield nothing.
/// Unicast only (no broadcast datagrams), so it stays read-only on desks.
pub async fn sweep_known_subnet(
    specs: &[ProbeSpec],
    seed: IpAddr,
    timeout: Duration,
) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let IpAddr::V4(v4) = seed else {
        return Ok(Vec::new());
    };
    let prefix = subnet24(v4);
    if !sweepable_prefix(prefix) || specs.is_empty() {
        return Ok(Vec::new());
    }
    let own: Vec<Ipv4Addr> = local_ipv4_interfaces().iter().map(|i| i.ip).collect();
    let hosts = sweep_targets(prefix, &own);
    sweep_hosts(specs, &hosts, timeout).await
}

/// The wire half of [`sweep_known_subnet`]: one ephemeral socket, paced
/// unicast sends to `hosts` (every payload), then collect replies until
/// `timeout`. Split out so the loopback test can pass an explicit host list
/// (the gate above never sweeps loopback).
async fn sweep_hosts(
    specs: &[ProbeSpec],
    hosts: &[Ipv4Addr],
    timeout: Duration,
) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let peer = UdpPeer::ephemeral().await?;
    // Send in small paced batches: 254 hosts x N specs is a burst on a
    // phone radio, and a slow stack must not drop the tail of the sweep.
    for chunk in hosts.chunks(16) {
        for ip in chunk {
            for spec in specs {
                let _ = peer.send_to(spec.payload, &format!("{ip}:{}", spec.port)).await;
            }
        }
        tokio::task::yield_now().await;
    }
    let deadline = tokio::time::Instant::now() + timeout;
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    let mut buf = [0u8; 2048];
    while tokio::time::Instant::now() < deadline {
        let now = tokio::time::Instant::now();
        let until = deadline - now;
        match peer.recv_from_timeout(&mut buf, until).await {
            Ok((len, from)) => {
                // A desk may answer BOTH payloads: match whichever spec's
                // prefix the reply carries (rows dedupe by address).
                if let Some(spec) = specs
                    .iter()
                    .find(|s| parse_info_reply(&buf[..len], s.reply_prefix).is_some())
                {
                    collect_reply(&mut out, spec, &buf[..len], from);
                }
            }
            Err(_) => break, // deadline
        }
    }
    Ok(out)
}

/// X-Air discovery: run every X-Air row of [`PROBE_TABLE`] concurrently,
/// dedupe by addr. Rows run in parallel (one task each) so a two-row table
/// costs one timeout, not two — a phone scan budget is seconds, and a slow
/// row must not double it. Row errors propagate (bind failure); a row that
/// answers nothing simply contributes nothing.
pub async fn discover_xair(timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    let mut set = tokio::task::JoinSet::new();
    for spec in PROBE_TABLE.iter().filter(|s| s.driver == "xair") {
        set.spawn(probe(spec, timeout));
    }
    while let Some(joined) = set.join_next().await {
        let devs = joined
            .map_err(|e| anyhow::anyhow!("discovery probe task failed: {e}"))??;
        for dev in devs {
            if !out.iter().any(|d| d.addr == dev.addr) {
                out.push(dev);
            }
        }
    }
    Ok(out)
}

/// Generic discovery: run every row of [`PROBE_TABLE`] concurrently,
/// dedupe by addr.
/// A new provider adds its rows to the table; this loop needs no edits.
pub async fn discover_all(timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let mut out: Vec<DiscoveredDevice> = Vec::new();
    let mut set = tokio::task::JoinSet::new();
    for spec in PROBE_TABLE {
        set.spawn(probe(spec, timeout));
    }
    while let Some(joined) = set.join_next().await {
        let devs = joined
            .map_err(|e| anyhow::anyhow!("discovery probe task failed: {e}"))??;
        for dev in devs {
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

    /// Real captured replies (XR18 FW 1.20, console named "Studio"):
    /// the model is the second-to-last string arg — positional `nth(2)`
    /// would read the IP ("192.168.1.123") or version ("V0.04") instead.
    #[test]
    fn parse_info_reply_reads_model_not_ip() {
        let xinfo = b"/xinfo\x00\x00,ssss\x00\x00\x00192.168.1.123\x00\x00Studio\x00XR18\x00\x00\x00\x001.20\x00\x00\x00\x00";
        let (model, raw) = parse_info_reply(xinfo, "/xinfo").unwrap();
        assert_eq!(model, "XR18");
        assert!(raw.starts_with("/xinfo"));
        let info = b"/info\x00\x00\x00,ssss\x00\x00\x00V0.04\x00\x00\x00Studio\x00XR18\x00\x00\x00\x001.20\x00\x00\x00\x00";
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

    /// Per-socket broadcast list: limited broadcast always first, then each
    /// directed broadcast (deduped). This is the exact fan-out `probe`
    /// sends on the shared socket and on every pinned interface socket.
    #[test]
    fn probe_targets_shape() {
        let targets = probe_targets(
            &[
                Ipv4Addr::new(192, 168, 20, 255),
                Ipv4Addr::new(192, 168, 40, 255),
                Ipv4Addr::new(192, 168, 20, 255),
            ],
            10024,
        );
        assert_eq!(
            targets,
            vec![
                "255.255.255.255:10024".to_string(),
                "192.168.20.255:10024".to_string(),
                "192.168.40.255:10024".to_string(),
            ]
        );
        // Per-interface list (one directed broadcast) is the same shape.
        assert_eq!(
            probe_targets(&[Ipv4Addr::new(10, 0, 5, 255)], 9000),
            vec![
                "255.255.255.255:9000".to_string(),
                "10.0.5.255:9000".to_string()
            ]
        );
        // /31 and /32 interfaces contribute no directed broadcast.
        let p2p = LocalIfaceV4 {
            ip: Ipv4Addr::new(10, 9, 0, 1),
            netmask: Ipv4Addr::new(255, 255, 255, 254),
        };
        assert_eq!(p2p.broadcast(), None);
    }

    /// Repeat-send rule: 300 ms apart when the budget fits all attempts,
    /// else the budget split evenly (short budgets still send all three).
    #[test]
    fn attempt_spacing_rule() {
        assert_eq!(
            attempt_spacing(Duration::from_secs(2)),
            PROBE_ATTEMPT_SPACING
        );
        assert_eq!(attempt_spacing(Duration::from_millis(900)), Duration::from_millis(300));
        assert_eq!(attempt_spacing(Duration::from_millis(300)), Duration::from_millis(100));
        assert_eq!(attempt_spacing(Duration::from_millis(30)), Duration::from_millis(10));
    }

    /// /24 sweep target generation: a known desk IP maps to its /24, hosts
    /// run `.1`..=`.254` (no network/broadcast address) and this host's own
    /// address is excluded.
    #[test]
    fn sweep_targets_shape() {
        let desk: Ipv4Addr = "192.168.1.123".parse().unwrap();
        let prefix = subnet24(desk);
        assert_eq!(prefix, Ipv4Addr::new(192, 168, 1, 0));
        // Idempotent on an already-masked prefix.
        assert_eq!(subnet24(prefix), prefix);
        let own = [Ipv4Addr::new(192, 168, 1, 10)];
        let targets = sweep_targets(prefix, &own);
        assert_eq!(targets.len(), 253, ".1-.254 minus this host");
        assert_eq!(targets.first(), Some(&Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(targets.last(), Some(&Ipv4Addr::new(192, 168, 1, 254)));
        assert!(!targets.contains(&own[0]), "own address excluded");
        assert!(!targets.contains(&Ipv4Addr::new(192, 168, 1, 0)));
        assert!(!targets.contains(&Ipv4Addr::new(192, 168, 1, 255)));
        // An address from another /24 never leaks into the sweep.
        assert!(!targets.contains(&Ipv4Addr::new(192, 168, 20, 5)));
    }

    /// Sweep safety gate: only real host subnets are swept (never an
    /// arbitrary reserved block, and the caller must pass a known desk).
    #[test]
    fn sweepable_prefix_gate() {
        assert!(sweepable_prefix(Ipv4Addr::new(192, 168, 40, 0)));
        assert!(sweepable_prefix(Ipv4Addr::new(10, 0, 5, 0)));
        assert!(sweepable_prefix(Ipv4Addr::new(169, 254, 3, 0)));
        assert!(!sweepable_prefix(Ipv4Addr::new(0, 0, 0, 0)));
        assert!(!sweepable_prefix(Ipv4Addr::new(127, 0, 0, 0)));
        assert!(!sweepable_prefix(Ipv4Addr::new(224, 0, 0, 0)));
        assert!(!sweepable_prefix(Ipv4Addr::new(255, 255, 255, 0)));
    }

    /// Bounded sweep over loopback: the fake desk answers on the loopback
    /// host while the sweep target list came from a /24 (a moved DHCP
    /// lease) — the sweep still finds it. This is the cross-subnet re-find
    /// path, minus the radio.
    #[tokio::test]
    async fn sweep_finds_desk_on_same_24_as_moved_seed() {
        let sock = tokio::net::UdpSocket::bind("127.0.0.1:10026")
            .await
            .unwrap();
        let task = tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            // Answer only the first datagram: the reply lands on the sweep
            // socket regardless of which host in the /24 was probed.
            let res = tokio::time::timeout(Duration::from_secs(5), sock.recv_from(&mut buf)).await;
            let Ok(Ok((n, peer))) = res else {
                return;
            };
            let text = String::from_utf8_lossy(&buf[..n]).replace('\0', " ");
            assert!(text.starts_with("/xinfo"), "sweep uses the probe payload");
            let _ = sock.send_to(b"/xinfo 127.0.0.1 XR18 1.22", peer).await;
        });
        let spec = ProbeSpec {
            driver: "xair",
            port: 10026,
            payload: XINFO_PROBE,
            reply_prefix: "/xinfo",
        };
        // The gate above refuses loopback seeds (real scans never sweep
        // 127/8), so exercise the wire half with the same host list the
        // /24 generator would produce.
        let hosts = [Ipv4Addr::new(127, 0, 0, 1)];
        let found = sweep_hosts(&[spec], &hosts, Duration::from_millis(800))
            .await
            .unwrap();
        task.abort();
        assert_eq!(found.len(), 1, "one reply, one row (got {found:?})");
        assert_eq!(found[0].model, "XR18");
        assert_eq!(found[0].addr.port(), 10026);
    }

    /// IPv6 and reserved seeds are honest no-ops: the sweep never scans
    /// random ranges.
    #[tokio::test]
    async fn sweep_refuses_reserved_seeds() {
        let spec = ProbeSpec {
            driver: "xair",
            port: 10027,
            payload: XINFO_PROBE,
            reply_prefix: "/xinfo",
        };
        for seed in ["::1".parse().unwrap(), "224.0.0.1".parse().unwrap()] {
            let found = sweep_known_subnet(&[spec], seed, Duration::from_millis(50))
                .await
                .unwrap();
            assert!(found.is_empty(), "seed {seed} must not sweep");
        }
    }
}
