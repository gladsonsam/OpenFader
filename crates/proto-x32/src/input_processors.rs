//! Guarded canonical input processor snapshots. OSC observations are not desk CAS.
use crate::{
    mapping::{mapping, Mapping},
    presentation, X32Driver, QUERY_TIMEOUT,
};
use anyhow::{ensure, Context};
use openfader_core::{
    input_processors::{domain, member, value, writable, RecallCheck},
    ParamValue, StripId,
};
use openfader_transport::OscSession;
use rosc::OscMessage;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock, Mutex,
    },
};

struct Observation {
    wire: String,
    revision: u64,
}
static OBSERVED: LazyLock<Mutex<HashMap<(u64, String), Observation>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT: AtomicU64 = AtomicU64::new(1);

/// Track unsolicited observations as well as independent query replies.
/// A matching A→B→A push advances the revision; unseen transitions cannot be detected.
pub(crate) fn observe(generation: u64, msg: &OscMessage) -> anyhow::Result<u64> {
    let wire = format!("{:?}", msg.args);
    let mut cache = OBSERVED
        .lock()
        .map_err(|_| anyhow::anyhow!("x32: processor observation unavailable"))?;
    if cache.len() > 8192 {
        cache.retain(|(g, _), _| *g == generation);
    }
    let entry = cache
        .entry((generation, msg.addr.clone()))
        .or_insert_with(|| Observation {
            wire: wire.clone(),
            revision: NEXT.fetch_add(1, Ordering::AcqRel),
        });
    if entry.wire != wire {
        entry.wire = wire;
        entry.revision = NEXT.fetch_add(1, Ordering::AcqRel);
    }
    Ok(entry.revision)
}

fn fields(strip: StripId, processor: &str) -> anyhow::Result<Vec<&'static str>> {
    ensure!(
        ["eq", "gate", "compressor", "channel_processing"].contains(&processor),
        "Unknown input processor"
    );
    let family = crate::X32_TOPOLOGY
        .iter()
        .find(|f| f.contains(strip.0 as usize))
        .context("Invalid strip")?;
    let scopes = presentation::scopes();
    let defs = presentation::defs();
    let ids = presentation::capabilities()
        .iter()
        .copied()
        .filter(|id| {
            if !member(processor, id) || matches!(*id, "gate_gr" | "comp_gr") {
                return false;
            }
            let scope = scopes
                .iter()
                .find(|(name, _)| name == id)
                .map(|(_, scopes)| *scopes)
                .or_else(|| defs.iter().find(|d| d.id == *id).map(|d| d.scopes))
                .or_else(|| openfader_core::params::find(id).map(|d| d.scopes));
            scope.is_some_and(|s| s.contains(&family.kind)) && mapping(strip, id).is_ok()
        })
        .collect::<Vec<_>>();
    ensure!(!ids.is_empty(), "Input processor unavailable");
    Ok(ids)
}

async fn read(
    session: &Arc<OscSession>,
    generation: u64,
    m: &Mapping,
) -> anyhow::Result<(ParamValue, u64)> {
    let reply = session.query_timeout(&m.path, QUERY_TIMEOUT).await?;
    let value = m.decode(&reply)?;
    Ok((value, observe(generation, &reply)?))
}

fn snapshot_still_observed(snapshot: &Value, generation: u64) -> anyhow::Result<()> {
    let cache = OBSERVED
        .lock()
        .map_err(|_| anyhow::anyhow!("Processor observations unavailable"))?;
    for row in snapshot["rows"]
        .as_array()
        .context("Missing processor rows")?
        .iter()
        .chain(
            snapshot["links"]
                .as_array()
                .context("Missing link witness")?,
        )
    {
        let path = row["native"].as_str().context("Invalid processor path")?;
        let revision = row["revision"]
            .as_u64()
            .context("Invalid processor revision")?;
        ensure!(
            cache
                .get(&(generation, path.to_owned()))
                .is_some_and(|entry| entry.revision == revision),
            "Input processor changed; refresh before dispatch"
        );
    }
    Ok(())
}

impl X32Driver {
    /// Static processor-plane applicability, independent of live hydration.
    pub fn guarded_processor_applicable(&self, strip: StripId, processor: &str) -> bool {
        fields(strip, processor).is_ok()
    }

    async fn processor_snapshot(
        &self,
        session: &Arc<OscSession>,
        generation: u64,
        strip: StripId,
        processor: &str,
    ) -> anyhow::Result<Value> {
        let console_revision = self.console_epoch.load(Ordering::Acquire);
        let ids = fields(strip, processor)?;
        let link = if crate::link::STEREO_PAIRS.iter().any(|pair| pair.contains(&strip.0)) {
            let mapping = crate::link::link_mapping(strip)?;
            let (value, revision) = read(session, generation, &mapping).await?;
            ensure!(value == ParamValue::Bool(false), "Processor requires a verified unlinked stereo pair");
            Some((mapping, revision))
        } else {
            None
        };
        let mut rows = Vec::with_capacity(ids.len());
        for id in &ids {
            let m = mapping(strip, id)?;
            let (observed, revision) = read(session, generation, &m).await?;
            rows.push(json!({"id":id,"value":value(&observed),"revision":revision,"native":m.path,"encoding":format!("{:?}",m.kind)}));
        }
        if let Some((mapping, revision)) = &link {
            let (after, after_revision) = read(session, generation, mapping).await?;
            ensure!(after == ParamValue::Bool(false) && after_revision == *revision,
                "Stereo link changed during processor capture");
        }
        self.ensure_current(session)?;
        {
            let observed = OBSERVED
                .lock()
                .map_err(|_| anyhow::anyhow!("Processor observations unavailable"))?;
            for row in &rows {
                let path = row["native"].as_str().context("Invalid processor path")?;
                let revision = row["revision"]
                    .as_u64()
                    .context("Invalid processor revision")?;
                ensure!(
                    observed
                        .get(&(generation, path.to_owned()))
                        .is_some_and(|entry| entry.revision == revision),
                    "Input processor changed during capture"
                );
            }
            if let Some((mapping, revision)) = &link {
                ensure!(observed.get(&(generation, mapping.path.clone()))
                    .is_some_and(|entry| entry.revision == *revision),
                    "Stereo link changed during processor capture");
            }
        }
        ensure!(
            self.console_epoch.load(Ordering::Acquire) == console_revision,
            "Console controls changed during processor capture"
        );
        let links = link.map(|(mapping, revision)| vec![json!({"value":false,"revision":revision,"native":mapping.path})]).unwrap_or_default();
        let link_mode = if links.is_empty() { "non_linkable" } else { "unlinked" };
        Ok(json!({"provider":"x32","generation":generation,"console_revision":console_revision,"ch":strip.0,"processor":processor,"domain":domain(&ids),"rows":rows,"links":links,"link_mode":link_mode}))
    }

    pub(crate) async fn guarded_input_processor(
        &self,
        strip: StripId,
        processor: &str,
        write: Option<(&str, ParamValue)>,
        expected: Option<&Value>,
        check: RecallCheck<'_>,
    ) -> anyhow::Result<Value> {
        let session = self.session()?;
        let generation = crate::routing::generation_for_session(&session)?;
        let _lane = self.writes.lock().await;
        self.ensure_current(&session)?;
        check()?;
        let before = self
            .processor_snapshot(&session, generation, strip, processor)
            .await?;
        ensure!(
            self.processor_snapshot(&session, generation, strip, processor)
                .await?
                == before,
            "Input processor changed during capture"
        );
        check()?;
        if let Some(expected) = expected {
            ensure!(
                *expected == before,
                "Input processor changed; refresh before dispatch"
            );
        }
        let Some((id, desired)) = write else {
            return Ok(before);
        };
        ensure!(
            expected.is_some()
                && writable(processor, id)
                && fields(strip, processor)?.contains(&id),
            "Input setting cannot be recalled"
        );
        let m = mapping(strip, id)?;
        let arg = m.encode(desired.clone())?;
        let final_before = self
            .processor_snapshot(&session, generation, strip, processor)
            .await?;
        ensure!(
            before == final_before,
            "Input processor changed before dispatch"
        );
        self.ensure_current(&session)?;
        check()?;
        snapshot_still_observed(&before, generation)?;
        session
            .send(&m.path, vec![arg])
            .await
            .map_err(|e| anyhow::anyhow!("Input write attempted: {e}; result uncertain"))?;
        let after = self
            .processor_snapshot(&session, generation, strip, processor)
            .await
            .map_err(|e| anyhow::anyhow!("Input write attempted: {e}; result uncertain"))?;
        self.ensure_current(&session)
            .map_err(|e| anyhow::anyhow!("{e}; result uncertain"))?;
        check().map_err(|e| anyhow::anyhow!("{e}; result uncertain"))?;
        ensure!(
            before["links"] == after["links"]
                && before["domain"] == after["domain"]
                && before["console_revision"] == after["console_revision"],
            "Input link or domain changed; result uncertain"
        );
        let before_rows = before["rows"]
            .as_array()
            .context("Missing processor rows")?;
        let after_rows = after["rows"].as_array().context("Missing processor rows")?;
        ensure!(
            before_rows.len() == after_rows.len()
                && before_rows
                    .iter()
                    .filter(|r| r["id"] != id)
                    .all(|r| after_rows.contains(r)),
            "Input sibling changed; result uncertain"
        );
        let actual = after_rows
            .iter()
            .find(|r| r["id"] == id)
            .context("Input readback missing; result uncertain")?;
        ensure!(
            actual["value"] == value(&desired),
            "Input readback differs; result uncertain"
        );
        snapshot_still_observed(&after, generation)
            .map_err(|error| anyhow::anyhow!("{error}; result uncertain"))?;
        Ok(after)
    }
}
