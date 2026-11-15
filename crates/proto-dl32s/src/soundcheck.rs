//! DL32S USB soundcheck routing, using native A/B selectors and fresh receipts.
//! Mic Pre USB sends can be replayed before channel processing. Post-DSP sends
//! are deliberately excluded to avoid applying processing twice.
use openfader_core::StripId;
use openfader_transport::mackie::{MackieSession, REQUEST_TIMEOUT, VALUE_TYPE_PARAM};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::atomic::Ordering};

pub struct Profile {
    pub capture_sources: Vec<String>,
    pub output_mapping: Vec<Option<usize>>,
    pub output_mask: Vec<bool>,
    pub changes: Vec<(String, i32)>,
    pub keys: Vec<String>,
    pub state: Value,
}

fn key(pid: u32) -> String {
    format!("dl32s/parameter/{pid}")
}
fn address(key: &str) -> anyhow::Result<u32> {
    let pid: u32 = key
        .strip_prefix("dl32s/parameter/")
        .ok_or_else(|| anyhow::anyhow!("Invalid DL32S routing key"))?
        .parse()?;
    anyhow::ensure!(
        routing_addresses()?.contains(&pid),
        "Unknown DL32S routing parameter"
    );
    Ok(pid)
}
fn routing_addresses() -> anyhow::Result<Vec<u32>> {
    let mut addresses = Vec::new();
    for channel in 1..=crate::addrs::INPUT_COUNT {
        for field in ["source_a", "source_b", "source_select", "stereo_link"] {
            addresses.push(crate::addrs::dl32s_inputs().address(channel, field)?);
        }
    }
    for slot in 1..=32 {
        addresses.push(crate::output_routing::usb_send_address(slot)?);
    }
    Ok(addresses)
}
fn guard(session: &MackieSession, state: &Value, generation: u64) -> anyhow::Result<()> {
    anyhow::ensure!(
        session.is_alive() && state["generation"].as_u64() == Some(generation),
        "DL32S routing connection changed"
    );
    let rows = state["rows"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Missing DL32S routing guard"))?;
    for pid in routing_addresses()? {
        let row = rows
            .get(&pid.to_string())
            .ok_or_else(|| anyhow::anyhow!("Incomplete DL32S routing guard"))?;
        let (raw, revision, _) = session
            .value_cache()
            .verified(pid)
            .ok_or_else(|| anyhow::anyhow!("DL32S routing awaits independent readback: {pid}"))?;
        anyhow::ensure!(
            row["raw"].as_u64() == Some(u64::from(raw))
                && row["revision"].as_u64() == Some(revision),
            "DL32S routing changed: {pid}"
        );
    }
    Ok(())
}

/// One raw parameter and the transport sequence of its most recent receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingReceipt {
    pub pid: u32,
    pub value: u32,
    pub revision: u64,
    pub receipt_sequence: u64,
}

/// Two complete, matching fresh passes; this is not an atomic hardware snapshot.
#[derive(Debug, Clone)]
pub struct FreshRoutingSnapshot {
    pub generation: u64,
    /// Uninterpreted GeneralInfo 3 reply body, identical before and after.
    pub device_info: Vec<u8>,
    pub first_pass: Vec<RoutingReceipt>,
    pub second_pass: Vec<RoutingReceipt>,
}

impl crate::Dl32sDriver {
    /// Read two matching sets of desk-originated routing receipts. Each pass
    /// is bounded independently; an ACK or a previously cached value is not evidence.
    pub async fn fresh_routing_snapshot(
        &self,
        pass_timeout: std::time::Duration,
    ) -> anyhow::Result<FreshRoutingSnapshot> {
        use openfader_transport::mackie::REQUEST_TIMEOUT;
        use std::sync::atomic::Ordering;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("session checked");
        let generation = self.generation.load(Ordering::Acquire);
        let check = || -> anyhow::Result<()> {
            anyhow::ensure!(
                session.is_alive(),
                "dl32s: transport closed during routing readback"
            );
            anyhow::ensure!(
                self.generation.load(Ordering::Acquire) == generation,
                "dl32s: generation changed during routing readback"
            );
            Ok(())
        };
        let info = session.request_general_info(3, REQUEST_TIMEOUT).await?.body;
        anyhow::ensure!(
            info.starts_with(&3u32.to_be_bytes()) && info.len() >= 8,
            "dl32s: invalid GeneralInfo 3 reply"
        );
        check()?;
        let addresses = routing_addresses()?;
        let cache = session.value_cache();
        let mut passes = Vec::new();
        for pass in 1..=2 {
            check()?;
            let deadline = tokio::time::Instant::now() + pass_timeout;
            let barrier = cache.receipt_barrier();
            let request = tokio::time::timeout_at(deadline, session.request_value_snapshot()).await;
            match request {
                Ok(Ok(())) => (),
                Ok(Err(error)) => {
                    return Err(error.context(format!("dl32s: routing pass {pass} request")))
                }
                Err(_) => (),
            }
            loop {
                check()?;
                match cache.received_since(&barrier, &addresses) {
                    Ok(values) => {
                        passes.push(
                            addresses
                                .iter()
                                .copied()
                                .zip(values)
                                .map(
                                    |(pid, (value, revision, receipt_sequence))| RoutingReceipt {
                                        pid,
                                        value,
                                        revision,
                                        receipt_sequence,
                                    },
                                )
                                .collect::<Vec<_>>(),
                        );
                        break;
                    }
                    Err(missing) if tokio::time::Instant::now() >= deadline => anyhow::bail!(
                        "dl32s: routing pass {pass} timed out; missing PIDs: {missing:?}"
                    ),
                    Err(_) => {
                        tokio::time::sleep_until(
                            (tokio::time::Instant::now() + std::time::Duration::from_millis(50))
                                .min(deadline),
                        )
                        .await
                    }
                }
            }
        }
        let after = session.request_general_info(3, REQUEST_TIMEOUT).await?.body;
        check()?;
        anyhow::ensure!(
            after == info,
            "dl32s: GeneralInfo 3 identity changed during routing readback"
        );
        anyhow::ensure!(
            passes[0]
                .iter()
                .zip(&passes[1])
                .all(|(a, b)| a.value == b.value && a.revision == b.revision),
            "dl32s: fresh routing passes differ"
        );
        let second_pass = passes.pop().expect("two passes");
        let first_pass = passes.pop().expect("two passes");
        Ok(FreshRoutingSnapshot {
            generation,
            device_info: info,
            first_pass,
            second_pass,
        })
    }
}

impl crate::Dl32sDriver {
    pub async fn soundcheck_profile(&self) -> anyhow::Result<Profile> {
        let snapshot = self
            .fresh_routing_snapshot(std::time::Duration::from_millis(800))
            .await?;
        let values: BTreeMap<_, _> = snapshot
            .second_pass
            .iter()
            .map(|r| (r.pid, r.value))
            .collect();
        let table = crate::addrs::dl32s_inputs();
        let mut capture_sources = Vec::new();
        let mut captured = BTreeMap::new();
        for slot in 1..=32 {
            let raw = values[&crate::output_routing::usb_send_address(slot)?];
            capture_sources.push(format!("dl32s/usb-send/source/{raw}"));
            if (1000..=1031).contains(&raw) {
                captured.entry(raw).or_insert((slot - 1) as usize);
            }
        }
        let mut changes = Vec::new();
        let mut output_mask = vec![false; 32];
        let mut output_mapping = vec![None; 32];
        let mut reserved = [false; 32];
        for channel in 1..=crate::addrs::INPUT_COUNT {
            let a = values[&table.address(channel, "source_a")?];
            let b = values[&table.address(channel, "source_b")?];
            let selected = values[&table.address(channel, "source_select")?];
            let linked = values[&table.address(channel, "stereo_link")?];
            anyhow::ensure!(
                selected <= 1 && linked <= 1,
                "Input {channel}: unknown A/B or stereo link selector"
            );
            let live = if selected == 1 { a } else { b };
            if (2000..=2031).contains(&live) {
                reserved[(live - 2000) as usize] = true;
            }
        }
        // Route controls are explicitly independent of stereo processing links:
        // leave both native link words untouched and guard them throughout.
        // Allocate one output per captured physical Mic Pre, allowing duplicate
        // input assignments to share that return without duplicating audio.
        let mut outputs = BTreeMap::new();
        for channel in 1..=crate::addrs::INPUT_COUNT {
            let a = values[&table.address(channel, "source_a")?];
            let b = values[&table.address(channel, "source_b")?];
            let selected = values[&table.address(channel, "source_select")?];
            let live = if selected == 1 { a } else { b };
            if (2000..=2031).contains(&live) {
                continue;
            }
            anyhow::ensure!(
                (1000..=1031).contains(&live),
                "Input {channel}: live source cannot be replayed as a physical Mic Pre"
            );
            let track = *captured.get(&live).ok_or_else(|| {
                anyhow::anyhow!(
                    "Input {channel}: Mic Pre {} is not captured by a USB send",
                    live - 999
                )
            })?;
            let output = if let Some(output) = outputs.get(&live) {
                *output
            } else {
                let preferred = b.checked_sub(2000).filter(|v| *v < 32).map(|v| v as usize);
                let output = preferred
                    .filter(|v| !reserved[*v] && !output_mask[*v])
                    .or_else(|| (0..32).find(|v| !reserved[*v] && !output_mask[*v]))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No free USB return for input {channel}; live USB returns are reserved"
                        )
                    })?;
                output_mask[output] = true;
                output_mapping[track] = Some(output);
                outputs.insert(live, output);
                output
            };
            // If B is already live, repatch the active selector in place;
            // journal restoration reverses this write before restoring A/B.
            if b != 2000 + output as u32 {
                changes.push((
                    key(table.address(channel, "source_b")?),
                    2000 + output as i32,
                ));
            }
            if selected != 0 {
                changes.push((key(table.address(channel, "source_select")?), 0));
            }
        }
        anyhow::ensure!(
            output_mask.iter().any(|v| *v),
            "No physical inputs available for soundcheck"
        );
        let rows: serde_json::Map<String, Value> = snapshot
            .second_pass
            .iter()
            .map(|r| {
                (
                    r.pid.to_string(),
                    json!({"raw":r.value,"revision":r.revision}),
                )
            })
            .collect();
        Ok(Profile {
            capture_sources,
            output_mapping,
            output_mask,
            changes,
            keys: snapshot.second_pass.iter().map(|r| key(r.pid)).collect(),
            state: json!({"generation":snapshot.generation,"rows":rows}),
        })
    }

    pub async fn soundcheck_route_read(
        &self,
        key: &str,
        state: &Value,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<i32> {
        let pid = address(key)?;
        if state.is_null() {
            let members: Vec<_> = (0..crate::addrs::INPUT_COUNT)
                .map(|ch| StripId(ch as u16))
                .collect();
            check(&members)?;
            let snapshot = self
                .fresh_routing_snapshot(std::time::Duration::from_millis(800))
                .await?;
            check(&members)?;
            return snapshot
                .second_pass
                .iter()
                .find(|r| r.pid == pid)
                .map(|r| r.value as i32)
                .ok_or_else(|| anyhow::anyhow!("Missing fresh routing parameter"));
        }
        let members: Vec<_> = (0..crate::addrs::INPUT_COUNT)
            .map(|ch| StripId(ch as u16))
            .collect();
        check(&members)?;
        let session_guard = self.session().await?;
        let session = session_guard.as_ref().expect("session checked");
        let generation = self.generation.load(Ordering::Acquire);
        guard(session, state, generation)?;
        check(&members)?;
        Ok(session
            .cached_verified(pid)
            .ok_or_else(|| anyhow::anyhow!("Routing readback unavailable"))? as i32)
    }

    pub async fn soundcheck_route_write(
        &self,
        key: &str,
        state: &Value,
        expected: i32,
        desired: i32,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<Value> {
        let pid = address(key)?;
        let recovered_state;
        let state = if state.is_null() {
            let snapshot = self
                .fresh_routing_snapshot(std::time::Duration::from_millis(800))
                .await?;
            let rows: serde_json::Map<String, Value> = snapshot
                .second_pass
                .iter()
                .map(|r| {
                    (
                        r.pid.to_string(),
                        json!({"raw":r.value,"revision":r.revision}),
                    )
                })
                .collect();
            recovered_state = json!({"generation":snapshot.generation,"rows":rows});
            &recovered_state
        } else {
            state
        };
        let table = crate::addrs::dl32s_inputs();
        let mut writable = false;
        for ch in 1..=crate::addrs::INPUT_COUNT {
            if pid == table.address(ch, "source_b")? {
                writable = desired == 0
                    || desired == 17000
                    || (1000..=1031).contains(&desired)
                    || (2000..=2031).contains(&desired);
            }
            if pid == table.address(ch, "source_select")? {
                writable = (0..=1).contains(&desired);
            }
        }
        anyhow::ensure!(
            writable,
            "DL32S soundcheck cannot write this selector/value"
        );
        let _lane = self.output_write.lock().await;
        let members: Vec<_> = (0..crate::addrs::INPUT_COUNT)
            .map(|ch| StripId(ch as u16))
            .collect();
        let session_guard = self.session().await?;
        let session = session_guard.as_ref().expect("session checked");
        let generation = self.generation.load(Ordering::Acquire);
        check(&members)?;
        guard(session, state, generation)?;
        let before = session
            .value_cache()
            .verified(pid)
            .ok_or_else(|| anyhow::anyhow!("Routing readback unavailable"))?;
        anyhow::ensure!(
            i64::from(expected) == i64::from(before.0),
            "DL32S routing conflict"
        );
        let authorize = || {
            check(&members).is_ok()
                && self.generation.load(Ordering::Acquire) == generation
                && guard(session, state, generation).is_ok()
        };
        session
            .write_int_guarded(pid, desired as u32, VALUE_TYPE_PARAM, &authorize)
            .await?;
        session.request_value_snapshot().await?;
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        loop {
            check(&members)?;
            anyhow::ensure!(
                generation == self.generation.load(Ordering::Acquire) && session.is_alive(),
                "DL32S routing connection changed; recovery required"
            );
            if let Some((raw, revision, receipt)) = session.value_cache().verified(pid) {
                if receipt != before.2 {
                    anyhow::ensure!(
                        raw == desired as u32
                            && revision == before.1.wrapping_add(u64::from(raw != before.0)),
                        "DL32S routing readback differs; recovery required"
                    );
                    let mut next = state.clone();
                    next["rows"][pid.to_string()] = json!({"raw":raw,"revision":revision});
                    guard(session, &next, generation)?;
                    return Ok(next);
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "DL32S routing accepted without independent readback; recovery required"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
