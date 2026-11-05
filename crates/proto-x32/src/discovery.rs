//! Stateless X32/M32 discovery over UDP 10023.
use crate::{addrs, X32Driver};
use openfader_transport::discovery::{DiscoveredDevice, ProbeSpec};
use std::time::Duration;

const X32_PROBE: ProbeSpec = ProbeSpec {
    driver: "x32",
    port: addrs::PORT,
    payload: b"/info\0\0\0,",
    reply_prefix: "/info",
};

impl X32Driver {
    pub async fn identify(ip: std::net::IpAddr, budget: Duration) -> Option<DiscoveredDevice> {
        let mut rows = openfader_transport::discovery::probe_addr(&X32_PROBE, ip, budget)
            .await
            .ok()?;
        rows.retain(|d| addrs::is_x32_model(&d.model));
        rows.into_iter().next()
    }

    pub async fn discover(budget: Duration) -> Vec<DiscoveredDevice> {
        let rows = openfader_transport::discovery::probe(&X32_PROBE, budget)
            .await
            .unwrap_or_default();
        rows.into_iter()
            .filter(|d| addrs::is_x32_model(&d.model))
            .collect()
    }

    pub fn discovered_name(device: &DiscoveredDevice) -> String {
        if device.model.to_uppercase().starts_with("M32") {
            format!("Midas {}", device.model)
        } else {
            format!("Behringer {}", device.model)
        }
    }
}
