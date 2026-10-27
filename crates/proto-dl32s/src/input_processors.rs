//! Guard existing input mappings with transport-observed revisions/fresh pushes.
//! Local serialization and observed-state checks are not atomic desk CAS.
use crate::{input::alias_target, Dl32sDriver, DL32S_CAPABILITIES};
use openfader_core::{
    input_processors::{domain, member, writable},
    ParamValue, StripId,
};
use openfader_transport::mackie::{MackieSession, REQUEST_TIMEOUT, VALUE_TYPE_PARAM};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

fn capture(
    session: &MackieSession,
    ch: u16,
    processor: &str,
    generation: u64,
) -> anyhow::Result<(Value, Vec<u64>)> {
    anyhow::ensure!(
        ["eq", "gate", "compressor"].contains(&processor),
        "Unknown input processor"
    );
    let (table, channel) = crate::strips::resolve(StripId(ch))?;
    let ids: Vec<_> = DL32S_CAPABILITIES
        .iter()
        .copied()
        .filter(|id| member(processor, id) && crate::strips::supports(table, id))
        .collect();
    anyhow::ensure!(!ids.is_empty(), "Input processor unavailable");
    let mut addresses = ids
        .iter()
        .map(|id| table.address(channel, alias_target(id).unwrap_or(id)))
        .collect::<anyhow::Result<Vec<_>>>()?;
    // Both native stereo words matter: left link state and right direction.
    if table.field("stereo_link").is_ok() {
        let left = channel - (channel - 1) % 2;
        addresses.push(table.address(left, "stereo_link")?);
        addresses.push(table.address(left + 1, "stereo_link")?);
    }
    let observed = session.verified_value_snapshot(&addresses).ok_or_else(|| {
        anyhow::anyhow!("Waiting for complete verified input processor and stereo link readback")
    })?;
    if addresses.len() > ids.len() {
        anyhow::ensure!(observed[ids.len()].0 == 0 && observed[ids.len() + 1].0 == 0,
            "Input presets require verified unlinked channels and known link direction");
    }
    let rows = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let native = alias_target(id).unwrap_or(id);
            let field = table.field(native)?;
            let (raw, revision, _) = observed[i];
            let v = if field.encoding.is_float() {
                json!(f32::from_bits(raw))
            } else if field.encoding == crate::addrs::Encoding::Bool {
                json!(raw != 0)
            } else {
                json!(raw)
            };
            Ok(json!({"id":id,"value":v,"revision":revision,"raw":raw,
            "native":native,"encoding":format!("{:?}",field.encoding),"limits":field.limits}))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let links = observed[ids.len()..]
        .iter()
        .map(|(raw, revision, _)| json!({"raw":raw,"revision":revision}))
        .collect::<Vec<_>>();
    Ok((
        json!({"provider":"dl32s","generation":generation,"ch":ch,"processor":processor,"domain":domain(&ids),"rows":rows,"links":links}),
        observed.iter().map(|r| r.2).collect(),
    ))
}
impl Dl32sDriver {
    pub(crate) async fn guarded_input_processor(
        &self,
        strip: StripId,
        processor: &str,
        write: Option<(&str, ParamValue)>,
        expected: Option<&Value>,
        check: openfader_core::input_processors::RecallCheck<'_>,
    ) -> anyhow::Result<Value> {
        let (table, channel) = crate::strips::resolve(strip)?;
        let generation = self.generation.load(Ordering::Acquire);
        let _write = self.output_write.lock().await;
        let session_guard = self.session().await?;
        let session = session_guard.as_ref().expect("checked session");
        let connected = || -> anyhow::Result<()> {
            anyhow::ensure!(
                session.is_alive() && generation == self.generation.load(Ordering::Acquire),
                "Input connection changed; result uncertain"
            );
            Ok(())
        };
        connected()?;
        check()?;
        let (before, _) = capture(session, strip.0, processor, generation)?;
        if let Some(expected) = expected {
            anyhow::ensure!(
                *expected == before,
                "Input processor changed; recall stopped before dispatch"
            );
        }
        let Some((id, pv)) = write else {
            return Ok(before);
        };
        anyhow::ensure!(
            expected.is_some() && writable(processor, id) && DL32S_CAPABILITIES.contains(&id),
            "Input setting cannot be recalled"
        );
        let mut registry = openfader_core::params::ParamRegistry::canonical();
        registry.register(<Self as openfader_core::MixerDriver>::param_defs(self));
        for (id, min, max) in <Self as openfader_core::MixerDriver>::param_range_overrides(self) {
            registry.override_range(id, *min, *max);
        }
        registry.validate(id, &pv)?;
        let native = alias_target(id).unwrap_or(id);
        let field = table.field(native)?;
        if matches!(id, "eq_mode" | "gate_mode" | "comp_mode" | "comp_knee") {
            anyhow::ensure!(
                matches!(pv,ParamValue::Float(v) if v == 0. || v == 1.),
                "Input native mode must be 0 or 1"
            );
        }
        if let Some(band) = id.strip_prefix("eq").and_then(|s| s.strip_suffix("_q")) {
            let shape = format!("eq{band}_type");
            anyhow::ensure!(
                before["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|r| r["id"] == shape && r["value"] == json!(2)),
                "Inactive DL32S Q is not recalled"
            );
        }
        // Same address table, alias map, encoding, and limits as set_field.
        let raw = match (&pv, field.encoding) {
            (ParamValue::Bool(v), crate::addrs::Encoding::Bool) => u32::from(*v),
            (ParamValue::Float(v), enc) if v.is_finite() => {
                if let Some((lo, hi)) = field.limits {
                    anyhow::ensure!((lo..=hi).contains(v), "Input value outside native limits");
                }
                if enc.is_float() {
                    v.to_bits()
                } else {
                    anyhow::ensure!(v.fract() == 0. && *v >= 0., "Invalid native enum");
                    *v as u32
                }
            }
            _ => anyhow::bail!("Invalid input value type"),
        };
        // Final complete transport capture immediately before dispatch.
        connected()?;
        check()?;
        anyhow::ensure!(
            capture(session, strip.0, processor, generation)?.0 == before,
            "Input processor changed before dispatch"
        );
        session
            .write_int(table.address(channel, native)?, raw, VALUE_TYPE_PARAM)
            .await?;
        let address = table.address(channel, native)?;
        // Accepted is not verified. Baseline the receipt counter after ACK;
        // only a subsequent independent push may complete this command.
        let sequence = session.observed_value_sequence(address).unwrap_or(0);
        session.request_value_snapshot().await.map_err(|e| {
            anyhow::anyhow!("Input write accepted; readback request failed: {e}; result uncertain")
        })?;
        let deadline = tokio::time::Instant::now() + REQUEST_TIMEOUT;
        let old_rows = before["rows"].as_array().expect("snapshot rows");
        let old = old_rows
            .iter()
            .find(|r| r["id"] == id)
            .expect("advertised field");
        let expected_revision =
            old["revision"].as_u64().unwrap() + u64::from(old["raw"] != json!(raw));
        loop {
            connected()?;
            let captured = capture(session, strip.0, processor, generation);
            if let Err(error) = &captured {
                anyhow::ensure!(
                    error.to_string().starts_with("Waiting for complete"),
                    "{error}; result uncertain"
                );
            }
            if let Ok((current, receipts)) = captured {
                let rows = current["rows"].as_array().expect("snapshot rows");
                anyhow::ensure!(
                    current["links"] == before["links"]
                        && current["domain"] == before["domain"]
                        && rows.len() == old_rows.len()
                        && old_rows
                            .iter()
                            .filter(|r| r["id"] != id)
                            .all(|r| rows.contains(r)),
                    "Input sibling or link changed during readback; result uncertain"
                );
                let index = rows
                    .iter()
                    .position(|r| r["id"] == id)
                    .expect("advertised field");
                let row = &rows[index];
                anyhow::ensure!(
                    row["revision"].as_u64().unwrap() <= expected_revision,
                    "Input changed again during readback; result uncertain"
                );
                if receipts[index] > sequence {
                    anyhow::ensure!(
                        row["raw"] == json!(raw) && row["revision"] == json!(expected_revision),
                        "Input write accepted but fresh mixer readback differs; result uncertain"
                    );
                    connected()?;
                    check()?;
                    return Ok(current);
                }
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "Input write accepted without matching fresh mixer readback; result uncertain"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}

impl Dl32sDriver {
    /// One worker per live session, one read-only request after a quiet DSP
    /// burst. A continuous drag postpones the bulk read instead of flooding
    /// the desk. No-op writes also need proof: ACK never clears pending words.
    pub(super) fn start_input_refresh(&self, generation: u64) {
        let (tx, mut rx) = tokio::sync::watch::channel(());
        *self.input_refresh.lock().unwrap() = Some((generation, tx));
        let sessions = std::sync::Arc::downgrade(&self.session);
        let gen = self.generation.clone();
        let writes = self.output_write.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                loop {
                    tokio::select! {
                        changed = rx.changed() => {
                            if changed.is_err() { return; }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => break,
                    }
                }
                // Same lane as manual/preamp and guarded processor writes.
                // Check again after waiting for both locks: old-session work
                // must never run on a replacement connection.
                let _write = writes.lock().await;
                let Some(sessions) = sessions.upgrade() else {
                    return;
                };
                let guard = sessions.lock().await;
                if gen.load(Ordering::Acquire) != generation {
                    return;
                }
                if rx.has_changed().unwrap_or(true) {
                    continue;
                }
                let Some(session) = guard.as_ref().filter(|s| s.is_alive()) else {
                    return;
                };
                if let Err(error) = session.request_value_snapshot().await {
                    // No replay/retry and no fabricated evidence. UI continues
                    // to report waiting for verified readback if this fails.
                    eprintln!("dl32s: input DSP readback request failed (edits remain unverified): {error:#}");
                }
            }
        });
    }

    pub(super) fn schedule_input_refresh(&self, native: &str) {
        let id = crate::input::canonical_for_table_key(native).unwrap_or(native);
        if !["eq", "gate", "compressor"].iter().any(|p| member(p, id)) {
            return;
        }
        if let Some((generation, tx)) = self.input_refresh.lock().unwrap().as_ref() {
            if *generation == self.generation.load(Ordering::Acquire) {
                tx.send_replace(());
            }
        }
    }
}
