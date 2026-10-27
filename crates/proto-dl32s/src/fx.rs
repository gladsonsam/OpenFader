//! Provider FX catalogue and evidence. No optimistic values or algorithm defaults.
mod banks;
mod source;
mod specs;
mod writes;
use super::Dl32sDriver;
use openfader_core::{
    destinations::{DestinationControl, DestinationValue},
    fx::dl32s,
};
use openfader_transport::mackie::{MackieSession, MackieValueCache};
use std::sync::atomic::Ordering;

#[derive(Debug, Default)]
struct SlotState {
    algorithm: Option<(u32, u64)>,
    fresh: [bool; 31],
    operation: u64,
    // Cancellation leaves this freshness barrier in place. Only incoming desk
    // evidence clears it; dropping a future never schedules a control replay.
    after: Vec<(u32, u64)>,
}
#[derive(Debug, Default)]
pub(super) struct FxState {
    cache: Option<MackieValueCache>,
    slots: [SlotState; 4],
}
impl FxState {
    pub(super) fn seed(&mut self, session: &MackieSession) {
        self.cache = Some(session.value_cache());
        self.slots = std::array::from_fn(|i| {
            let bank = banks::BANKS[i];
            let observed = session.value_cache().verified(bank[0]);
            SlotState {
                algorithm: observed.map(|v| (v.0, v.1)),
                fresh: bank.map(|a| session.cached_verified(a).is_some()),
                ..SlotState::default()
            }
        });
    }
    pub(super) fn ingest(&mut self, start: u32, raws: &[u32]) {
        let Some(cache) = self.cache.as_ref() else {
            return;
        };
        for (i, slot) in self.slots.iter_mut().enumerate() {
            let bank = banks::BANKS[i];
            let current = cache.verified(bank[0]).map(|v| (v.0, v.1));
            if current != slot.algorithm {
                slot.algorithm = current;
                slot.fresh = [false; 31];
            }
            for (word, address) in bank.iter().enumerate() {
                if address
                    .checked_sub(start)
                    .is_some_and(|n| (n as usize) < raws.len())
                {
                    slot.fresh[word] = cache.verified(*address).is_some();
                }
            }
        }
    }
    fn observed(&self, slot: u16) -> Option<Vec<(u32, u64, u64)>> {
        self.cache
            .as_ref()?
            .verified_snapshot(&banks::addresses(slot))
    }
    fn ready(&self, slot: u16) -> bool {
        let s = &self.slots[slot as usize - 1];
        let Some(cache) = &self.cache else {
            return false;
        };
        s.fresh.iter().all(|v| *v)
            && cache
                .verified(banks::BANKS[slot as usize - 1][0])
                .map(|v| (v.0, v.1))
                == s.algorithm
            && s.after
                .iter()
                .all(|(a, seq)| cache.verified(*a).is_some_and(|v| v.2 > *seq))
    }
    fn rows(&self, slot: u16, generation: u64) -> Vec<DestinationControl> {
        let s = &self.slots[slot as usize - 1];
        let addresses = banks::addresses(slot);
        let observed = self.observed(slot);
        let revision = observed
            .as_ref()
            .map(|v| v.iter().fold(s.operation, |r, v| r.wrapping_add(v.1)))
            .unwrap_or(s.operation);
        let raw = |a| {
            addresses
                .iter()
                .position(|p| *p == a)
                .and_then(|i| observed.as_ref().map(|v| v[i].0))
        };
        let bank = banks::BANKS[slot as usize - 1];
        let current = raw(bank[0]).filter(|_| self.ready(slot));
        let mut rows = vec![dl32s::selector(slot, current, revision, generation)];
        let Some(algorithm) = current.and_then(dl32s::algorithm) else {
            return rows;
        };
        // A complete raw bank can still contain an unsupported encoding. Keep
        // all algorithm fields hidden until the entire active schema decodes.
        let values: Option<Vec<_>> = algorithm
            .fields
            .iter()
            .map(|f| raw(bank[f.word]).and_then(|v| f.decode(v)))
            .collect();
        if let Some(values) = values {
            for (field, value) in algorithm.fields.iter().zip(values) {
                let mut row = field.control(slot, algorithm, Some(value), revision, generation);
                // Preserve the original 55 descriptor domains/labels/precision.
                if let Some(legacy) = specs::FX_PARAMETERS
                    .iter()
                    .find(|s| s.slot == slot && s.algorithm == algorithm.id && s.id == field.id)
                {
                    debug_assert_eq!(legacy.algorithm_address, bank[0]);
                    row.target_name = Some(legacy.algorithm_name.to_string());
                    row.label = legacy.label.to_string();
                    row.range = Some(openfader_core::destinations::DestinationRange {
                        min: legacy.min,
                        max: legacy.max,
                        unit: legacy.unit.to_string(),
                        decimals: legacy.decimals,
                    });
                }
                rows.push(row);
            }
        }
        for spec in banks::common(slot) {
            let mut row = dl32s::selector(slot, current, revision, generation);
            row.id = spec.id.to_string();
            row.label = spec.label.to_string();
            row.description = format!("Changes {} for FX {slot}. Algorithm, mute, returns and routing remain independent.", spec.label.to_lowercase());
            row.choices.clear();
            row.range = None;
            row.current = raw(spec.address).and_then(|v| spec.read_value(v));
            if spec.id == "send_source" {
                let source = source::addresses(slot);
                row.current = source
                    .first()
                    .and_then(|a| raw(*a))
                    .filter(|v| *v <= 1)
                    .filter(|v| source.iter().all(|a| raw(*a) == Some(*v)))
                    .map(|v| DestinationValue {
                        value: v as f32,
                        verified: true,
                    });
            }
            if spec.is_binary() {
                row.choices = [
                    (
                        0.0,
                        if spec.id == "send_source" {
                            "Post Fader"
                        } else if spec.id == "mute" {
                            "Unmuted"
                        } else {
                            "Off"
                        },
                    ),
                    (
                        1.0,
                        if spec.id == "send_source" {
                            "Pre Fader"
                        } else if spec.id == "mute" {
                            "Muted"
                        } else {
                            "On"
                        },
                    ),
                ]
                .into_iter()
                .map(
                    |(value, label)| openfader_core::destinations::DestinationChoice {
                        value,
                        label: label.to_string(),
                    },
                )
                .collect();
            } else {
                row.range = Some(openfader_core::destinations::DestinationRange {
                    min: spec.min,
                    max: spec.max,
                    unit: spec.unit.to_string(),
                    decimals: spec.decimals,
                });
            }
            rows.push(row);
        }
        rows
    }
    pub(super) fn controls(&self, controls: &mut Vec<DestinationControl>, generation: u64) {
        for slot in 1..=4 {
            controls.extend(self.rows(slot, generation));
        }
    }
}
