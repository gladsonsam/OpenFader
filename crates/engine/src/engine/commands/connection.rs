//! Connection command arms.

use super::*;

impl Engine {
    pub(super) async fn apply_connection(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            Cmd::Connect { driver, addr } => {
                self.connect(&driver, addr.as_deref()).await?;
                // A fresh explicit connect clears offline editing (the desk
                // is authoritative again) unless a change list is pending,
                // in which case post_switch raises offline_diff.
                let ip = addr.unwrap_or_else(|| self.default_addr(&driver).to_string());
                self.set_link_connected(None, &ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            // ================= R2 =================
            Cmd::RefreshDevices { request_id, hosts } => {
                let mut results = Vec::new();
                let error = if hosts.len() > 64 {
                    Some("Refresh accepts at most 64 hosts".to_string())
                } else {
                    let mut seen = HashSet::new();
                    let hosts: Vec<String> = hosts
                        .into_iter()
                        .filter(|host| seen.insert(host.trim().to_ascii_lowercase()))
                        .collect();
                    // Completion order streams (one `devices` frame per
                    // host); the slots below restore input order for the
                    // final correlated complete.
                    let mut slots: Vec<Option<DeviceCheck>> =
                        (0..hosts.len()).map(|_| None).collect();
                    let mut pending: futures::stream::FuturesUnordered<_> = hosts
                        .into_iter()
                        .enumerate()
                        .map(|(index, addr)| async move {
                            let result = match tokio::time::timeout(
                                Duration::from_secs(3),
                                self.check_recent_host(addr.clone()),
                            )
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => DeviceCheck {
                                    addr,
                                    status: "no_reply".into(),
                                    device: None,
                                },
                            };
                            (index, result)
                        })
                        .collect();
                    while let Some((index, result)) = futures::StreamExt::next(&mut pending).await {
                        self.apply_device_check(&result);
                        self.scanned_at.store(now_unix(), Ordering::Relaxed);
                        self.unicast(writer, self.devices_message());
                        out.push(self.devices_message());
                        slots[index] = Some(result);
                    }
                    results = slots
                        .into_iter()
                        .map(|slot| slot.expect("refresh slot fills on completion"))
                        .collect();
                    None
                };
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                self.unicast(writer, self.devices_message());
                out.push(self.devices_message());
                Ok(Some(Event::DeviceOperationComplete {
                    request_id,
                    operation: "refresh".into(),
                    results,
                    error,
                }))
            }
            Cmd::DiscoverDevices { request_id } => {
                let display = |k: &str, m: &str| self.drv(k).display_name(m);
                let remembered: Vec<String> =
                    self.desk_addrs.read().unwrap().values().cloned().collect();
                // Stream live rows: the 2 s broadcast scan merges and
                // publishes first, then each remembered-host identify
                // publishes on completion. The op keeps its 4 s-class
                // bound: the remembered phase gets whatever the scan
                // left of it, and a hit there reports the same timeout
                // with the rows gathered so far.
                let started = Instant::now();
                let (mut rows, mut error) = match tokio::time::timeout(
                    Duration::from_secs(4),
                    self.scanner.scan(&display, Duration::from_secs(2)),
                )
                .await
                {
                    Ok(scan_rows) => (scan_rows, None),
                    Err(_) => (Vec::new(), Some("Discovery timed out".to_string())),
                };
                if error.is_none() {
                    self.merge_device_rows(rows.iter().cloned());
                    self.scanned_at.store(now_unix(), Ordering::Relaxed);
                    self.unicast(writer, self.devices_message());
                    out.push(self.devices_message());
                    let remaining = Duration::from_secs(4)
                        .checked_sub(started.elapsed())
                        .unwrap_or(Duration::ZERO);
                    let mut pending: futures::stream::FuturesUnordered<_> = remembered
                        .iter()
                        .map(|host| self.identify_registered(host, Duration::from_secs(3)))
                        .collect();
                    if !pending.is_empty() {
                        match tokio::time::timeout(remaining, async {
                            while let Some(found) = futures::StreamExt::next(&mut pending).await {
                                if let Some(row) = found {
                                    push_row(&mut rows, row.clone());
                                    self.merge_device_rows([row]);
                                    self.scanned_at.store(now_unix(), Ordering::Relaxed);
                                    self.unicast(writer, self.devices_message());
                                    out.push(self.devices_message());
                                }
                            }
                        })
                        .await
                        {
                            Ok(()) => {}
                            Err(_) => {
                                error = Some("Discovery timed out".to_string());
                            }
                        }
                    }
                }
                let results = rows
                    .iter()
                    .map(|row| DeviceCheck {
                        addr: row.ip.clone(),
                        status: "available".into(),
                        device: Some(row.clone()),
                    })
                    .collect();
                // Rows merged as they streamed (scan first, then each
                // identify); the final store + publish below are unchanged.
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                self.unicast(writer, self.devices_message());
                out.push(self.devices_message());
                Ok(Some(Event::DeviceOperationComplete {
                    request_id,
                    operation: "discover".into(),
                    results,
                    error,
                }))
            }
            Cmd::Scan => {
                // Real LAN discovery behind the trait (fake in tests),
                // on EVERY driver: a sim-backed relay has no desks of its
                // own, so scanning from sim must run the same broadcast
                // discovery the phone uses, never the design seed rows.
                //
                // Broadcast discovery and remembered-address verification
                // run CONCURRENTLY: the UI budget is ~10 s, and serialising
                // them (2 s broadcast, then one ~1 s probe per remembered
                // desk) runs out of budget with two desks.
                //
                // Remembered desks (previous connects + manual probes):
                // (a) directed unicast verify of each address (broadcast
                // never crosses subnets) with the linked-desk fast path,
                // (b) ONE bounded /24 sweep per distinct remembered subnet
                // so a desk whose DHCP lease moved, or a sibling desk on
                // that subnet, is re-found across the router. Sweeps run
                // in parallel and are capped, so they never delay the
                // broadcast rows past the scan budget.
                //
                // The list is still REPLACED: an address that verifies
                // nothing adds no row; stale entries are never kept.
                let driver = self.active_driver();
                let remembered: Vec<(String, String)> = self
                    .desk_addrs
                    .read()
                    .unwrap()
                    .iter()
                    .map(|(d, a)| (d.clone(), a.clone()))
                    .collect();

                let display = |k: &str, m: &str| self.drv(k).display_name(m);
                let scan_fut = self.scanner.scan(&display, Duration::from_secs(2));
                let verify_fut = async {
                    // (a) Linked desk + directed verify, all in parallel.
                    // Each item is capped: a connected-but-silent driver
                    // answers nothing and must not stretch the scan.
                    let active_linked = async {
                        if self.drv(&driver).is_hardware() {
                            tokio::time::timeout(
                                SCAN_VERIFY_TIMEOUT,
                                self.linked_desk_device(&driver),
                            )
                            .await
                            .ok()
                            .flatten()
                        } else {
                            None
                        }
                    };
                    let mut probes = Vec::new();
                    for (drv_name, ip) in &remembered {
                        // Hardware desks only: a sim "address" is not a
                        // desk (sim ignores it), so nothing to verify.
                        if !self.drv(drv_name).is_hardware() {
                            continue;
                        }
                        probes.push(async move {
                            tokio::time::timeout(SCAN_VERIFY_TIMEOUT, async {
                                if let Some(linked) = self.linked_desk_device(drv_name).await {
                                    return Some(linked);
                                }
                                self.probe_desk(drv_name, ip).await
                            })
                            .await
                            .ok()
                            .flatten()
                        });
                    }

                    // (b) Bounded /24 re-find sweep, one plan per distinct
                    // remembered /24 (specs merged when two drivers share
                    // it), all plans in parallel.
                    let sweep_fut = async {
                        use openfader_transport::discovery::{
                            subnet24, sweep_known_subnet, ProbeSpec, PROBE_TABLE,
                        };
                        let mut plans: Vec<(std::net::Ipv4Addr, Vec<ProbeSpec>)> = Vec::new();
                        for (drv_name, ip) in &remembered {
                            if !self.drv(drv_name).is_hardware() {
                                continue;
                            }
                            let Some(table) = self.drv(drv_name).probe_table() else {
                                continue;
                            };
                            // Hostname addresses have no /24 to sweep; the
                            // directed verify above still covers them.
                            let Ok(seed) = ip.parse::<std::net::Ipv4Addr>() else {
                                continue;
                            };
                            let specs: Vec<ProbeSpec> = PROBE_TABLE
                                .iter()
                                .filter(|s| s.driver == table)
                                .copied()
                                .collect();
                            if specs.is_empty() {
                                continue;
                            }
                            let prefix = subnet24(seed);
                            match plans.iter_mut().find(|(p, _)| *p == prefix) {
                                Some((_, existing)) => {
                                    for s in specs {
                                        if !existing
                                            .iter()
                                            .any(|e| e.port == s.port && e.payload == s.payload)
                                        {
                                            existing.push(s);
                                        }
                                    }
                                }
                                None => plans.push((prefix, specs)),
                            }
                        }
                        let sweeps = plans.into_iter().map(|(prefix, specs)| async move {
                            let budget = openfader_transport::discovery::SWEEP_TIMEOUT
                                + Duration::from_millis(300);
                            match tokio::time::timeout(
                                budget,
                                sweep_known_subnet(
                                    &specs,
                                    std::net::IpAddr::V4(prefix),
                                    openfader_transport::discovery::SWEEP_TIMEOUT,
                                ),
                            )
                            .await
                            {
                                Ok(Ok(devs)) => devs
                                    .into_iter()
                                    .map(|d| {
                                        let name = self.drv(d.driver).display_name(&d.model);
                                        device_row(d, "manual", name)
                                    })
                                    .collect(),
                                _ => Vec::new(), // a slow sweep is dropped, never blocks
                            }
                        });
                        let mut rows: Vec<DeviceDesc> = Vec::new();
                        for group in futures::future::join_all(sweeps).await {
                            for row in group {
                                push_row(&mut rows, row);
                            }
                        }
                        rows
                    };

                    let (linked, probed, swept) =
                        tokio::join!(active_linked, futures::future::join_all(probes), sweep_fut);
                    let mut rows: Vec<DeviceDesc> = Vec::new();
                    if let Some(row) = linked {
                        push_row(&mut rows, row);
                    }
                    for row in probed.into_iter().flatten() {
                        push_row(&mut rows, row);
                    }
                    for row in swept {
                        push_row(&mut rows, row);
                    }
                    rows
                };

                let (broadcast_rows, verified_rows) = tokio::join!(scan_fut, verify_fut);
                let mut merged = broadcast_rows;
                for row in verified_rows {
                    push_row(&mut merged, row);
                }
                *self.devices.write().unwrap() = merged;
                self.scanned_at.store(now_unix(), Ordering::Relaxed);
                out.push(self.devices_message());
                Ok(None)
            }
            Cmd::ConnectDevice { id } => {
                let entry = self
                    .devices
                    .read()
                    .unwrap()
                    .iter()
                    .find(|d| d.id == id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("unknown device {id:?} (scan first)"))?;
                if entry.kind == "io" {
                    anyhow::bail!("{:?} is I/O (in use) — connect a console", entry.name);
                }
                // Discovered devices dial their claiming driver
                // (UNVERIFIED-ON-RIG for dl32s); anything else re-links
                // the local sim model (dev fixture ids, old rows).
                let mut claimed = "sim";
                for name in &self.driver_order {
                    if self.drv(name).claims_device(&id, &entry.model) {
                        claimed = name.as_str();
                        break;
                    }
                }
                // Discovered-device dial carries an explicit port when the
                // driver answers there (the row already proved it, so the
                // bare-host sweep is skipped). Other drivers pass through
                // untouched.
                let dial = self.drv(claimed).dial_addr(&entry.ip);
                self.connect(claimed, Some(&dial)).await?;
                self.set_link_connected(Some(&id), &entry.ip).await;
                self.post_switch(writer, out, false).await?;
                Ok(Some(Event::You {
                    id: writer.to_string(),
                }))
            }
            Cmd::Probe { addr } => {
                // Ask every registered hardware provider; never infer the
                // protocol from whichever desk happened to be active before.
                let Some(found) = self
                    .identify_registered(&addr, DIRECT_IDENTIFY_TIMEOUT)
                    .await
                else {
                    anyhow::bail!("no reply from {addr} — no supported mixer identified; check the IP and local network access");
                };
                // The verified row is visible even if the connect fails.
                {
                    let mut devs = self.devices.write().unwrap();
                    if !devs.iter().any(|d| d.ip == found.ip) {
                        devs.push(found.clone());
                    }
                }
                // Remember + persist the verified address under the table's
                // driver so later scans (and a restart) re-verify it by
                // directed unicast and re-find its /24 instead of dropping
                // the row, broadcast never crosses subnets.
                let table_driver = found.id.split('-').next().unwrap_or("");
                self.remember_desk(table_driver, &found.ip);
                // Claim the driver exactly like Cmd::ConnectDevice.
                let mut claimed = "sim";
                for name in &self.driver_order {
                    if self.drv(name).claims_device(&found.id, &found.model) {
                        claimed = name.as_str();
                        break;
                    }
                }
                match self.connect(claimed, Some(&found.ip)).await {
                    Ok(()) => {
                        self.set_link_connected(Some(&found.id), &found.ip).await;
                        self.post_switch(writer, out, false).await?;
                        out.push(self.devices_message());
                        Ok(None)
                    }
                    Err(e) => {
                        // Verified but unusable: keep the row, leave the old
                        // link alone, and tell THIS client why (a plain `?`
                        // would drop the devices frame we just queued).
                        out.push(self.devices_message());
                        self.unicast(
                            writer,
                            Event::Error {
                                message: format!(
                                    "{} answered but the connect failed: {e}",
                                    found.ip
                                ),
                                locked_by: None,
                                ch: None,
                            },
                        );
                        Ok(None)
                    }
                }
            }
            Cmd::ForgetSavedMixers { request_id } => {
                // Pure-local saved-mixer forget: no driver/query/connect/
                // disconnect/probe/scan/native-view work, no broadcasts —
                // the correlated `device_operation_complete` ack is the
                // unicast direct reply (`apply_cmd` unicasts it).
                Ok(Some(self.forget_saved_mixers(request_id)))
            }
            Cmd::Disconnect => {
                let had_parameter_labels = !self.active_drv().param_label_overrides().is_empty();
                // A missing readback cannot prevent local disconnect cleanup.
                let _ = self.active_drv().set_spectrum_target("", None).await;
                self.invalidate_connection();
                self.reads_suspended.store(true, Ordering::Release);
                let _ = self.active_drv().disconnect().await;
                self.param_cache.write().unwrap().clear();
                self.fx_cache.write().unwrap().clear();
                // Drop the meter subscription guard so the next connect
                // resubscribes from clean state (see `connect`).
                *self.meter_guard.write().unwrap() = None;
                *self.meter_driver.write().unwrap() = None;
                self.health_strikes.store(0, Ordering::Release);
                {
                    let mut link = self.link.write().unwrap();
                    link.state = "disconnected".to_string();
                }
                out.push(self.link_message_cached());
                if had_parameter_labels {
                    out.push(self.caps_message());
                }
                // The desk is gone: native slots read unavailable (the
                // driver reset its view on disconnect), never stale.
                out.push(self.native_snapshots_message());
                out.extend(self.destination_controls_message());
                self.record_native_view();
                Ok(None)
            }
            Cmd::Info => Ok(Some(self.info_message().await?)),

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
