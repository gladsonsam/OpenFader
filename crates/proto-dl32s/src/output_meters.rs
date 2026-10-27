//! Native DL32S explicit-list output levels; no invented tap positions.
use crate::Dl32sDriver;
use openfader_core::{OutputMeterFrame, OutputMeterSnapshot, output_meters::OutputCompressorGr};
use openfader_transport::mackie::{PushEvent, VALUE_TYPE_METERS, dl32s_meters as layout};
use std::{sync::atomic::Ordering, time::Duration};

pub(super) fn targets() -> Vec<String> {
    (1..=layout::AUX_COUNT)
        .map(|n| format!("bus/{n:02}"))
        .chain((1..=layout::SUBGROUP_COUNT).map(|n| format!("subgroup/{n:02}")))
        .chain((1..=layout::FX_COUNT).map(|n| format!("fx/{n:02}")))
        .chain(["main".into()])
        .chain((1..=layout::RETURN_COUNT).map(|n| format!("ret/{n:02}")))
        .chain((1..=layout::FX_COUNT).map(|n| format!("fxret/{n:02}")))
        .collect()
}

/// Native DL32S OUT/SEND getters map the explicit list, never the legacy
/// family list. FX identities here are sends (tap unknown); the separate
/// stereo FX-return PREFADER pairs at FX_RETURN_OFFSET are not send levels.
/// Native semantics plus passive raw-f32 evidence are not physical calibration.
pub(super) fn decode(push: &PushEvent) -> Option<Vec<OutputMeterFrame>> {
    let PushEvent::Values {
        value_type: VALUE_TYPE_METERS,
        start: 1,
        raws,
    } = push
    else {
        return None;
    };
    if !layout::is_explicit_shape(raws.len()) {
        return None;
    }
    // Ordered exactly like targets(); Main alone owns an L/R pair.
    let positions = (layout::AUX_OFFSET..layout::FX_RETURN_OFFSET)
        .map(|i| (i, 1))
        .chain((layout::SUBGROUP_OFFSET..layout::MAIN_OFFSET).map(|i| (i, 1)))
        .chain((layout::FX_SEND_OFFSET..layout::AUX_OFFSET).map(|i| (i, 1)))
        .chain([(layout::MAIN_OFFSET, layout::STEREO_WIDTH)])
        .chain((0..layout::RETURN_COUNT).map(|n| (layout::RETURN_OFFSET + n, 1)))
        .chain((0..layout::FX_COUNT).map(|n| (layout::FX_RETURN_OFFSET + n * 2, 2)));
    targets()
        .into_iter()
        .zip(positions)
        .map(|(target, (offset, width))| {
            let levels = raws[offset..offset + width]
                .iter()
                .map(|raw| {
                    let db = f32::from_bits(*raw);
                    db.is_finite().then(|| 10f32.powf(db.min(0.0) / 20.0))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(OutputMeterFrame {
                compressor_gr: compressor_gr_indices(&target).map(|_| OutputCompressorGr {
                    reduction_db: None,
                    inferred: true,
                }),
                target,
                levels,
                inferred: true,
            })
        })
        .collect()
}

/// Native meter-wire-type-1 addresses, NOT type-2 parameters/raw offsets.
/// Our 62/93/285-word lists omit output GR. All 18 GR slots in the supplied
/// 73 passive explicit-116-word frames were zero: static positive-attenuation
/// parity is not nonzero output scale calibration. Emit absence/inferred until
/// that calibration exists; do not reinterpret a subgroup GR as an FX level.
fn compressor_gr_indices(target: &str) -> Option<Vec<u32>> {
    if target == "main" {
        return Some(vec![306, 307]);
    }
    if let Some(n) = target
        .strip_prefix("bus/")
        .and_then(|n| n.parse::<u32>().ok())
    {
        return (1..=layout::AUX_COUNT as u32)
            .contains(&n)
            .then(|| vec![312 + 4 * (n - 1)]);
    }
    if let Some(n) = target
        .strip_prefix("subgroup/")
        .and_then(|n| n.parse::<u32>().ok())
    {
        return (1..=layout::SUBGROUP_COUNT as u32)
            .contains(&n)
            .then(|| vec![280 + 4 * (n - 1)]);
    }
    None
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
