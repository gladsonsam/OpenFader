//! Stateless WING broadcast/directed discovery helpers.
use crate::WingDriver;
use openfader_transport::discovery::{DiscoveredDevice, WING_PROBE};
use std::time::Duration;

impl WingDriver {
    pub async fn identify(ip: std::net::IpAddr, budget: Duration) -> Option<DiscoveredDevice> {
        openfader_transport::discovery::probe_addr(&WING_PROBE, ip, budget)
            .await
            .ok()?
            .into_iter()
            .next()
    }
    pub async fn discover(budget: Duration) -> Vec<DiscoveredDevice> {
        openfader_transport::discovery::probe(&WING_PROBE, budget)
            .await
            .unwrap_or_default()
    }
    pub fn discovered_name(device: &DiscoveredDevice) -> String {
        device
            .raw
            .split(',')
            .nth(2)
            .filter(|s| !s.is_empty())
            .unwrap_or("Behringer WING")
            .to_string()
    }
}
