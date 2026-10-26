//! Discovery operations.

use super::*;

impl Engine {
    /// Current `devices` event (link-aware states: the linked console
    /// reads `connected`, other consoles `available`, I/O `in_use`).
    pub(super) fn devices_message(&self) -> Event {
        let link = self.link.read().unwrap();
        let list = self
            .devices
            .read()
            .unwrap()
            .iter()
            .map(|d| {
                let state = if d.kind == "io" {
                    "in_use".to_string()
                } else if link.state == "connected"
                    && link.device_id.as_deref() == Some(d.id.as_str())
                {
                    "connected".to_string()
                } else {
                    d.state.clone()
                };
                DeviceDesc {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    model: d.model.clone(),
                    ip: d.ip.clone(),
                    kind: d.kind.clone(),
                    state,
                    fw: d.fw.clone(),
                    via: d.via.clone(),
                }
            })
            .collect();
        Event::Devices {
            list,
            scanned_at: self.scanned_at.load(Ordering::Relaxed),
        }
    }

    /// Verified device entry for the relay's linked desk (hardware drivers
    /// only): the stored connect address answers the driver's own info
    /// query. `None` when never connected, unconfigured, or unreachable
    /// a scan then honestly reports only what broadcast discovery found.
    pub(super) async fn linked_desk_device(&self, driver: &str) -> Option<DeviceDesc> {
        if !self.drv(driver).is_hardware() {
            return None;
        }
        let ip = self.desk_addrs.read().unwrap().get(driver).cloned()?;
        let info = self.drv(driver).query_info().await.ok()?;
        if info.model.trim().is_empty() {
            return None;
        }
        let name = if info.name.trim().is_empty() {
            info.model.clone()
        } else {
            info.name.clone()
        };
        Some(DeviceDesc {
            id: format!("{driver}-{ip}"),
            name,
            model: info.model,
            ip,
            kind: "console".to_string(),
            state: "available".to_string(),
            fw: if info.firmware.trim().is_empty() {
                None
            } else {
                Some(info.firmware)
            },
            via: Some("linked".to_string()),
        })
    }

    /// Directed verify of one desk address WITHOUT switching the link:
    /// unicast `/xinfo` (+ legacy `/info` fallback) at `addr`. Returns the
    /// device row (`via: "manual"`) when the desk answers, `None` on
    /// silence. Broadcast discovery never crosses subnets; this is the
    /// manual-add and cross-subnet path (read-only on the desk).
    pub(super) async fn probe_desk(&self, driver: &str, addr: &str) -> Option<DeviceDesc> {
        let ip = Self::discovery_ip(addr).await?;
        let reply = self
            .drv(driver)
            .identify(ip, Duration::from_millis(1800))
            .await?;
        let name = self.drv(driver).discovered_name(&reply);
        Some(device_row(reply, "manual", name))
    }

    pub(super) async fn discovery_ip(raw: &str) -> Option<IpAddr> {
        if raw.is_empty() || raw.len() > 255 || raw.chars().any(char::is_whitespace) {
            return None;
        }
        if let Ok(ip) = raw.parse() {
            return Some(ip);
        }
        if let Ok(socket) = raw.parse::<std::net::SocketAddr>() {
            return Some(socket.ip());
        }
        let host = raw.rsplit_once(':').map_or(raw, |(h, _)| h);
        let mut ips =
            tokio::time::timeout(Duration::from_secs(2), tokio::net::lookup_host((host, 0)))
                .await
                .ok()?
                .ok()?;
        ips.find(|s| s.is_ipv4())
            .or_else(|| ips.next())
            .map(|s| s.ip())
    }

    pub(super) async fn identify_registered(
        &self,
        addr: &str,
        budget: Duration,
    ) -> Option<DeviceDesc> {
        use futures::{stream::FuturesUnordered, StreamExt};
        let ip = Self::discovery_ip(addr).await?;
        let mut probes: FuturesUnordered<_> = self
            .driver_order
            .iter()
            .map(|name| self.drv(name))
            .filter(|driver| driver.is_hardware())
            .map(|driver| async move {
                tokio::time::timeout(budget, driver.identify(ip, budget))
                    .await
                    .ok()
                    .flatten()
            })
            .collect();
        while let Some(result) = probes.next().await {
            if let Some(reply) = result {
                let name = self.drv(reply.driver).discovered_name(&reply);
                return Some(device_row(reply, "manual", name));
            }
        }
        None
    }

    /// Probe a supplied host with only verified discovery protocols. DNS and
    /// socket waits are async and bounded by the caller; no driver is linked.
    pub(super) async fn check_recent_host(&self, addr: String) -> DeviceCheck {
        let raw = addr.trim();
        if raw.is_empty()
            || raw.len() > 255
            || raw.chars().any(char::is_whitespace)
            || Self::discovery_ip(raw).await.is_none()
        {
            return DeviceCheck {
                addr,
                status: "invalid".into(),
                device: None,
            };
        }
        let device = self.identify_registered(raw, DIRECT_IDENTIFY_TIMEOUT).await;
        DeviceCheck {
            addr,
            status: if device.is_some() {
                "available"
            } else {
                "no_reply"
            }
            .into(),
            device,
        }
    }

    pub(super) fn merge_device_rows(&self, rows: impl IntoIterator<Item = DeviceDesc>) {
        let mut devices = self.devices.write().unwrap();
        for row in rows {
            if let Some(existing) = devices.iter_mut().find(|d| {
                d.id == row.id
                    || (d.id.split('-').next() == row.id.split('-').next()
                        && d.ip.parse::<IpAddr>().ok().is_some()
                        && d.ip.parse::<IpAddr>().ok() == row.ip.parse::<IpAddr>().ok())
            }) {
                *existing = row;
            } else {
                devices.push(row);
            }
        }
    }
}

/// R2 LAN discovery behind a trait: production uses [`RealScanner`]
/// (`crates/transport` mDNS/OSC broadcast discovery); tests inject a fake
/// so no test ever touches the network.
pub trait DeviceScanner: Send + Sync {
    fn scan<'a>(
        &'a self,
        display: &'a (dyn Fn(&str, &str) -> String + Send + Sync),
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + 'a>>;
}

/// Production scanner: every registered hardware provider contributes its
/// read-only discovery method. Searches share a bounded deadline.
pub struct RealScanner {
    pub(super) drivers: Vec<Arc<dyn DynDriver>>,
}

impl DeviceScanner for RealScanner {
    fn scan<'a>(
        &'a self,
        display: &'a (dyn Fn(&str, &str) -> String + Send + Sync),
        timeout: Duration,
    ) -> Pin<Box<dyn Future<Output = Vec<DeviceDesc>> + Send + 'a>> {
        Box::pin(async move {
            let mut out = Vec::new();
            let searches = self.drivers.iter().map(|d| async move {
                tokio::time::timeout(timeout, d.discover(timeout))
                    .await
                    .unwrap_or_default()
            });
            for devs in futures::future::join_all(searches).await {
                for d in devs {
                    let name = self
                        .drivers
                        .iter()
                        .find(|driver| driver.name() == d.driver)
                        .map_or_else(
                            || display(d.driver, &d.model),
                            |driver| driver.discovered_name(&d),
                        );
                    push_row(&mut out, device_row(d, "broadcast", name));
                }
            }
            out
        })
    }
}

/// One discovery reply as a device row. Same mapping for broadcast, the
/// unicast verify (manual add), the linked desk and the /24 sweep; only
/// `via` differs. No sim rows ever come from here.
pub(super) fn device_row(
    d: openfader_transport::discovery::DiscoveredDevice,
    via: &str,
    display_name: String,
) -> DeviceDesc {
    let ip = d.addr.ip().to_string();
    DeviceDesc {
        id: format!("{}-{ip}", d.driver),
        name: display_name,
        model: d.model,
        ip,
        kind: "console".to_string(),
        state: "available".to_string(),
        fw: None,
        via: Some(via.to_string()),
    }
}

/// Append `row` unless a row for the same IP is already listed (first row
/// wins: broadcast beats linked beats manual beats sweep).
pub(super) fn push_row(out: &mut Vec<DeviceDesc>, row: DeviceDesc) {
    if !out.iter().any(|d| d.ip == row.ip) {
        out.push(row);
    }
}
