//! Metadata operations.

use super::*;

impl Engine {
    /// Supported catalogue ids of the named driver (its
    /// [`DynDriver::capabilities`]).
    pub(super) fn driver_capabilities(&self, driver: &str) -> Vec<&'static str> {
        self.drivers
            .get(driver)
            .map_or_else(Vec::new, |registered| {
                registered.driver.capabilities().to_vec()
            })
    }

    /// Merged param registry of the named driver (canonical catalogue +
    /// that provider's contributed rows). Names must be registered before dispatch.
    pub(super) fn registry(&self, driver: &str) -> &ParamRegistry {
        self.registries
            .get(driver)
            .expect("registered drivers have a parameter registry")
    }

    /// Channel-strip count of the named driver (legacy `channel_count`;
    /// poll/attach geometry now uses [`Engine::driver_strips`]).
    #[allow(dead_code)]
    pub(super) fn driver_channels(&self, driver: &str) -> usize {
        self.drv(driver).channel_count()
    }

    /// Total addressable strips of the named driver (topology total).
    pub(super) fn driver_strips(&self, driver: &str) -> usize {
        self.drv(driver).strip_count()
    }

    /// Ordered strip families of the named driver (`caps.topology`).
    pub(super) fn driver_topology(&self, driver: &str) -> Vec<openfader_core::StripFamily> {
        self.drv(driver).topology()
    }

    /// Stable key for one strip on the named driver (`None` off-topology).
    pub(super) fn strip_key(&self, driver: &str, strip: u16) -> Option<String> {
        openfader_core::strip_key_for_index(&self.driver_topology(driver), strip as usize)
    }

    /// Stable key for one strip on the ACTIVE driver (`None` off-topology).
    pub(super) fn active_strip_key(&self, strip: u16) -> Option<String> {
        let driver = self.active_driver();
        self.strip_key(&driver, strip)
    }

    /// Resolve a strip-targeted command's `ch` + optional `key` against the
    /// active topology: a resolvable key wins (desk-switch survival
    /// profiles replay keys, not positions); otherwise `ch` is validated in
    /// range. An unresolvable key is an honest error naming the key.
    pub(super) fn resolve_strip(&self, ch: u16, key: Option<&str>) -> anyhow::Result<u16> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        if let Some(k) = key.filter(|k| !k.trim().is_empty()) {
            if let Some(idx) = openfader_core::strip_index_for_key(&topo, k) {
                let max = u16::try_from(self.driver_strips(&driver)).unwrap_or(u16::MAX);
                if (idx as u16) < max {
                    return Ok(idx as u16);
                }
            }
            anyhow::bail!("unknown strip key {k:?} on {driver}");
        }
        if (ch as usize) < self.driver_strips(&driver) {
            Ok(ch)
        } else {
            anyhow::bail!("strip {ch} out of range 0..{}", self.driver_strips(&driver))
        }
    }

    /// Resolve a `focus` strip list mixing indices and stable keys
    /// (`chs` arrives as `Vec<Value>` so keys ride the same frame).
    pub(super) fn resolve_focus_list(
        &self,
        items: &[serde_json::Value],
    ) -> anyhow::Result<Vec<u16>> {
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        let n = self.driver_strips(&driver);
        let mut out = Vec::new();
        for item in items {
            if let Some(k) = item.as_str() {
                let idx = openfader_core::strip_index_for_key(&topo, k)
                    .ok_or_else(|| anyhow::anyhow!("focus: unknown strip key {k:?} on {driver}"))?;
                if idx >= n {
                    anyhow::bail!("focus: strip key {k:?} out of range 0..{n}");
                }
                out.push(idx as u16);
            } else if let Some(v) = item.as_u64() {
                let v: u16 = v
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("focus channel {v} out of range"))?;
                out.push(v);
            } else {
                anyhow::bail!("focus \"chs\" entries must be integers or strip keys, got {item}");
            }
        }
        Ok(out)
    }

    /// Declared meter taps of the named driver (`caps.meter_taps`).
    pub(super) fn driver_meter_taps(&self, driver: &str) -> Vec<String> {
        self.drv(driver)
            .meter_taps()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}
