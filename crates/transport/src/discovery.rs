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
/// `/xinfo <ip> <model> <fw> ...`.
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
pub fn parse_info_reply(buf: &[u8], reply_prefix: &str) -> Option<(String, String)> {
    let text: String = String::from_utf8_lossy(buf)
        .chars()
        .map(|c| {
            if c == '\0' || c == ',' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !compact.starts_with(reply_prefix) {
        return None;
    }
    let model = compact
        .split_whitespace()
        .nth(2)
        .unwrap_or("unknown")
        .to_string();
    Some((model, compact))
}

/// Generic broadcast probe loop shared by all drivers: send `spec.payload`
/// to `<broadcast>:port`, collect matching replies until `timeout`.
pub async fn probe(spec: &ProbeSpec, timeout: Duration) -> anyhow::Result<Vec<DiscoveredDevice>> {
    let peer = UdpPeer::ephemeral().await?;
    let target = format!("255.255.255.255:{}", spec.port);
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
