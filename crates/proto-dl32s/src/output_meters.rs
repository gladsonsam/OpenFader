//! DL32S explicit-list output levels and provider-normalized compressor GR.
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
                compressor_gr: compressor_gr(&target, raws),
                target,
                levels,
                inferred: true,
            })
        })
        .collect()
}

/// Positive network-f32 dB attenuation. Aux1 native312 is physically
/// calibrated (2026-10-27, thresholds -80/-78 dB; IN311-OUT313 agrees within
/// 0.052 dB). Other outputs have static native getter evidence only.
/// Short explicit shapes publish absence, replacing any prior 301-word GR.
/// Invalid readings stay unavailable; never clamp them to zero or merge L/R.
fn compressor_gr(target: &str, raws: &[u32]) -> Option<OutputCompressorGr> {
    let (offset, width) = if target == "main" {
        (layout::MAIN_GR_OFFSET, layout::STEREO_WIDTH)
    } else if let Some(n) = target
        .strip_prefix("bus/")
        .and_then(|n| n.parse::<usize>().ok())
    {
        if !(1..=layout::AUX_COUNT).contains(&n) {
            return None;
        }
        (layout::OUTPUT_GR_OFFSET + n - 1, 1)
    } else {
        let n = target
            .strip_prefix("subgroup/")
            .and_then(|n| n.parse::<usize>().ok())?;
        if !(1..=layout::SUBGROUP_COUNT).contains(&n) {
            return None;
        }
        (layout::SUBGROUP_GR_OFFSET + n - 1, 1)
    };
    let reduction_db = (raws.len() == layout::OUTPUT_GR_FRAME_LEN)
        .then(|| {
            raws[offset..offset + width]
                .iter()
                .map(|raw| {
                    let db = f32::from_bits(*raw);
                    (db.is_finite() && db >= 0.0).then_some(db)
                })
                .collect::<Option<Vec<_>>>()
        })
        .flatten();
    Some(OutputCompressorGr {
        reduction_db,
        inferred: target != "bus/01",
    })
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
