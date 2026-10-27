use super::*;
use openfader_core::input_processors::{domain, member, value, writable};
use serde_json::{json, Value};
impl SimDriver {
    pub(super) fn guarded_input_processor(
        &self,
        strip: StripId,
        processor: &str,
        write: Option<(&str, ParamValue)>,
        expected: Option<&Value>,
        check: openfader_core::input_processors::RecallCheck<'_>,
    ) -> anyhow::Result<Value> {
        let i = self.idx(strip)?;
        let kind = kind_of_strip(i)?;
        anyhow::ensure!(
            ["eq", "gate", "compressor", "channel_processing"].contains(&processor),
            "Input processor unavailable"
        );
        anyhow::ensure!(processor != "channel_processing" || matches!(kind,
            StripKind::In | StripKind::Ret | strip_schema::RETURN_KIND),
            "Channel DSP clipboard unavailable on this family");
        let mut params = self.params.write().expect("sim params poisoned");
        let ids: Vec<_> = self
            .capabilities()
            .iter()
            .copied()
            .filter(|id| {
                member(processor, id)
                    && openfader_core::params::find(id)
                        .is_some_and(|d| strip_schema::supports(d, kind))
            })
            .collect();
        anyhow::ensure!(!ids.is_empty(), "Processor unavailable on this family");
        let pair = self.stereo_pairs().into_iter().find(|pair| pair.contains(&strip.0));
        let snapshot = |params: &Vec<HashMap<String, ParamValue>>| -> anyhow::Result<Value> {
            let mut links = Vec::new();
            for ch in pair.into_iter().flatten().map(usize::from) {
                anyhow::ensure!(
                    params[ch].get("link") == Some(&ParamValue::Bool(false)),
                    "Input presets require verified unlinked channels"
                );
                links.push(json!({"ch":ch,"value":false}));
            }
            let rows = ids.iter().map(|id| Ok(json!({"id":id,"value":value(params[i].get(*id).ok_or_else(|| anyhow::anyhow!("Input value unavailable"))?)}))).collect::<anyhow::Result<Vec<_>>>()?;
            Ok(
                json!({"provider":"sim","generation":self.processor_generation.load(Ordering::Acquire),"ch":i,"processor":processor,"domain":domain(&ids),"revision":self.processor_revision.load(Ordering::Acquire),"rows":rows,"links":links}),
            )
        };
        check()?;
        let before = snapshot(&params)?;
        let generation = before["generation"].as_u64().expect("snapshot generation");
        if let Some(expected) = expected {
            anyhow::ensure!(
                *expected == before,
                "Input processor changed; reopen presets"
            );
        }
        if let Some((id, pv)) = write {
            anyhow::ensure!(
                expected.is_some() && writable(processor, id) && ids.contains(&id),
                "Input setting cannot be recalled"
            );
            openfader_core::params::validate(id, &pv)?;
            if params[i].get(id) != Some(&pv) {
                self.processor_revision.fetch_add(1, Ordering::AcqRel);
            }
            params[i].insert(id.into(), pv);
        }
        // Sim owns the authoritative state; locked reread is independent of
        // the frontend optimistic map, with no hardware acceptance ambiguity.
        anyhow::ensure!(
            self.processor_generation.load(Ordering::Acquire) == generation,
            "Sim connection changed during recall; result uncertain"
        );
        check()?;
        snapshot(&params)
    }
}
