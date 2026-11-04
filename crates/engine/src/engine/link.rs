//! Link operations.

use super::*;

impl Engine {
    /// Point the link at a connected desk (R2): canonical device name when
    /// the ip matches a discovered device, else the desk identity / driver.
    pub(super) async fn set_link_connected(&self, device_id: Option<&str>, ip: &str) {
        let entry = self
            .devices
            .read()
            .unwrap()
            .iter()
            .find(|d| device_id.is_some_and(|id| d.id == id) || d.ip == ip)
            .cloned();
        let (name, device) = match entry {
            Some(d) => (d.name.clone(), Some(d.id.clone())),
            None => {
                // Manual add-by-IP with no prior scan: resolve the steady-state
                // name from the desk `/info` reply (index 1 = console name,
                // e.g. `Studio`; index 2 = model, e.g. `XR18`, see
                // `info_from_strings`). The console name wins; the model is
                // only a degraded fallback, never the steady state when
                // `/info` answered. Unverified `/info` paths are NOT probed
                // this reuses the single supported `query_info` codec.
                let driver = self.active_driver();
                match self.drv(&driver).query_info().await {
                    Ok(info) if !info.name.is_empty() => {
                        eprintln!("engine: link name from console name {:?}", info.name);
                        (info.name, None)
                    }
                    Ok(info) if !info.model.is_empty() => {
                        eprintln!("engine: link name from model fallback {:?}", info.model);
                        (info.model, None)
                    }
                    _ => {
                        eprintln!(
                            "engine: link name from driver key {driver:?} (/info unanswered)"
                        );
                        (driver, None)
                    }
                }
            }
        };
        // A (re)connect is a fresh control path: clear any accumulated
        // health strikes so the new session starts `connected` (see
        // `measure_latency`).
        self.health_strikes.store(0, Ordering::Release);
        let sample_rate = self.active_drv().reported_sample_rate().await.unwrap_or(0);
        let mut link = self.link.write().unwrap();
        link.state = "connected".to_string();
        link.name = name;
        link.ip = ip.to_string();
        link.fs = sample_rate;
        link.device_id = device.or_else(|| device_id.map(str::to_string));
    }

    /// Pre-write value for the offline change list (`None` unless offline
    /// editing is active, avoids an extra desk query per write online).
    pub(super) async fn offline_from(&self, strip: StripId, id: &str) -> Option<Value> {
        if !self.offline.read().unwrap().active {
            return None;
        }
        let driver = self.active_driver();
        self.read_param(&driver, strip, id)
            .await
            .ok()
            .map(|pv| param_value_json(&pv))
    }
}

impl Engine {
    /// Cached `link` event (no measurement; the snapshot + change paths).
    pub(super) fn link_message_cached(&self) -> Event {
        let link = self.link.read().unwrap();
        Event::Link {
            state: link.state.clone(),
            name: link.name.clone(),
            ip: link.ip.clone(),
            fs: link.fs,
            latency_ms: link.latency_ms,
        }
    }

    /// Fresh `link` event with a new round-trip measurement stored.
    pub(super) async fn link_message(&self) -> Event {
        let latency_ms = self.measure_latency().await;
        {
            let mut link = self.link.write().unwrap();
            link.latency_ms = latency_ms;
        }
        self.link_message_cached()
    }

    /// Read-only control-health round trip through the provider hook. The
    /// default probes the display main fader; push-cache providers override
    /// it with a correlated wire query so a silent route cannot look healthy
    /// indefinitely from old values. It never verifies a mixer write and
    /// never opens a second session. Failures retain the last RTT sample.
    ///
    /// Bounded failure policy (matches the poller's
    /// [`POLL_TIMEOUT_STRIKES`] tolerance): a single failed query only arms
    /// the bounded `repair_dirty` reconciliation and keeps `connected`, one
    /// lost datagram must not flap the status UI while the independent UDP
    /// meter stream stays live. The `link` flips to `reconnecting` only
    /// after [`POLL_TIMEOUT_STRIKES`] consecutive failures (cadence depends
    /// on the provider's bounded query timeout and [`LINK_INTERVAL`]).
    /// The strike count resets on any success and on
    /// every link reset (connect/disconnect/offline), and fresh meters
    /// NEVER reset it, so a dead control path always surfaces after ~6 s
    /// and cannot be masked indefinitely by live levels.
    pub(super) async fn measure_latency(&self) -> f32 {
        let last = self.link.read().unwrap().latency_ms;
        if self.reads_suspended.load(Ordering::Acquire) {
            return last;
        }
        if self.active_drv().is_hardware() {
            let generation = self.connection_gen.load(Ordering::Acquire);
            let t0 = Instant::now();
            let target = self.active_drv().control_health().await;
            if generation != self.connection_gen.load(Ordering::Acquire)
                || self.reads_suspended.load(Ordering::Acquire)
            {
                return last;
            }
            match target {
                Ok(_) => {
                    self.health_strikes.store(0, Ordering::Release);
                    let mut link = self.link.write().unwrap();
                    if link.state == "reconnecting" {
                        link.state = "connected".into();
                        eprintln!("engine: control health recovered; link connected");
                    }
                    t0.elapsed().as_secs_f32() * 1000.0
                }
                Err(error) => {
                    self.repair_dirty.store(true, Ordering::Release);
                    let strikes = self
                        .health_strikes
                        .load(Ordering::Acquire)
                        .saturating_add(1);
                    self.health_strikes.store(strikes, Ordering::Release);
                    if strikes >= POLL_TIMEOUT_STRIKES {
                        let mut link = self.link.write().unwrap();
                        if !self.reads_suspended.load(Ordering::Acquire) {
                            link.state = "reconnecting".into();
                        }
                        eprintln!("engine: control health query failed ({strikes} in a row): {error}; link reconnecting, bounded repair armed");
                    } else {
                        eprintln!("engine: control health query failed ({strikes} in a row, tolerating up to {}): {error}; link holds connected, bounded repair armed", POLL_TIMEOUT_STRIKES);
                    }
                    last
                }
            }
        } else {
            1.0 + ((self.now_ms() / 100) % 5) as f32 * 0.1
        }
    }

    /// Desk identity block for `info` (connect + on request). Errors when
    /// the driver cannot answer (hardware drivers while unconnected).
    pub(super) async fn info_message(&self) -> anyhow::Result<Event> {
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active_driver();
        let drv = self.drv(&driver);
        let info = drv.query_info().await?;
        let sample_rate = drv.reported_sample_rate().await.unwrap_or(0);
        anyhow::ensure!(
            self.connection_gen.load(Ordering::Acquire) == generation,
            "connection changed during identity read"
        );
        Ok(Event::Info {
            model: info.model,
            firmware: info.firmware,
            sample_rate,
            console_name: info.name,
            driver,
        })
    }
}
