//! Existing 60-word family prefix only; no subscriptions or invented tap positions.
use crate::Dl32sDriver;
use openfader_core::{OutputMeterFrame, OutputMeterSnapshot};
use openfader_transport::mackie::{PushEvent, VALUE_TYPE_METERS};
use std::{sync::atomic::Ordering, time::Duration};

pub(super) fn targets() -> Vec<String> {
    (1..=8)
        .map(|n| format!("bus/{n:02}"))
        .chain((1..=6).map(|n| format!("subgroup/{n:02}")))
        .chain((1..=4).map(|n| format!("fx/{n:02}")))
        .chain(["main".into()])
        .collect()
}

/// Structural evidence: dl32_family_meter_addresses explicitly orders fourteen
/// 318+4z master words, four 278+2z FX-out words, then 310/311 Main L/R.
/// Eight Aux followed by six subgroup labels match the output inventory but
/// remain inferred until independently correlated. FX-out slot order and Main
/// L/R also need live correlation; none of these claim a post-fader tap.
/// Returns and FX-in words are deliberately outside this output contract.
pub(super) fn decode(push: &PushEvent) -> Option<Vec<OutputMeterFrame>> {
    let PushEvent::Values {
        value_type: VALUE_TYPE_METERS,
        start: 1,
        raws,
    } = push
    else {
        return None;
    };
    if !matches!(raws.len(), 60 | 91 | 283) {
        return None;
    }
    let levels: Option<Vec<f32>> = raws[40..60]
        .iter()
        .map(|raw| {
            let db = f32::from_bits(*raw);
            db.is_finite().then(|| 10f32.powf(db.min(0.0) / 20.0))
        })
        .collect();
    let levels = levels?;
    Some(
        targets()
            .into_iter()
            .enumerate()
            .map(|(i, target)| OutputMeterFrame {
                target,
                levels: if i == 18 {
                    levels[18..20].to_vec()
                } else {
                    vec![levels[i]]
                },
                inferred: true,
            })
            .collect(),
    )
}

impl Dl32sDriver {
    pub(super) fn telemetry_poll_outputs(&self) -> OutputMeterSnapshot {
        let generation = self.generation.load(Ordering::Acquire);
        let mut snapshot = OutputMeterSnapshot {
            generation,
            age_ms: 0,
            frames: Vec::new(),
        };
        let fresh = self.meters.lock().ok().and_then(|m| {
            if m.generation != generation {
                return None;
            }
            let age = m.outputs_at?.elapsed();
            if age > Duration::from_secs(1) {
                return None;
            }
            Some((age, m.outputs.clone()))
        });
        if let Some((age, frames)) = fresh {
            snapshot.age_ms = age.as_millis() as u64;
            snapshot.frames = frames;
        }
        snapshot
    }
}
