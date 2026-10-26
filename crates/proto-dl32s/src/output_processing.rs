//! Output DSP geometry inferred from the 2026-10-26 full raw baseline.
//!
//! Main native movement anchors and inferred family strides are documented in
//! OUTPUT_PROCESSING.md. Readback provenance does not prove inferred mappings.
//! Input-like compressor and reversed four-band PEQ records recur in all 15
//! output strips. Main/Aux add 31 GEQ gains and HP/LP filters. Keep this map
//! separate from the hardware-proven input table. See OUTPUT_PROCESSING.md.
use std::collections::BTreeMap;

use openfader_core::destinations::{
    DestinationChoice, DestinationControl, DestinationRange, DestinationValue,
};
use openfader_transport::mackie::MackieSession;

use crate::Dl32sDriver;
use std::sync::atomic::Ordering;

mod map;
use map::{specs, Bank, Spec};

#[derive(Debug, Default)]
struct Entry {
    current: Option<DestinationValue>,
    revision: u64,
}

#[derive(Debug, Default)]
pub(crate) struct OutputProcessingCache {
    parameters: BTreeMap<(Bank, u32), Entry>,
    modes: BTreeMap<u32, u32>,
    names: BTreeMap<u32, String>,
}

impl OutputProcessingCache {
    pub(crate) fn seed(&mut self, session: &MackieSession) {
        *self = Self::default();
        for spec in specs() {
            if let Some(address) = spec.mode {
                if let Some(raw) = session.cached_verified(address) {
                    self.modes.insert(address, raw);
                }
            }
            if let Some(slot) = spec.name_slot {
                if let Some(name) = session.name_slot_verified(slot) {
                    self.names.insert(slot, name);
                }
            }
            let current = spec
                .bank
                .verified(session, spec.address)
                .and_then(|raw| spec.decode(raw));
            self.parameters.insert(
                (spec.bank, spec.address),
                Entry {
                    current,
                    revision: 0,
                },
            );
        }
    }

    pub(crate) fn ingest(&mut self, value_type: u8, start: u32, raws: &[u32]) {
        let raw_at = |a: u32| {
            a.checked_sub(start)
                .and_then(|i| raws.get(i as usize))
                .copied()
        };
        let mut changed = std::collections::BTreeSet::new();
        for address in specs()
            .iter()
            .filter(|spec| spec.bank.value_type() == value_type)
            .filter_map(|spec| spec.mode)
        {
            if let Some(mode) = raw_at(address) {
                if self.modes.insert(address, mode) != Some(mode) {
                    changed.insert(address);
                }
            }
        }
        for spec in specs()
            .iter()
            .filter(|spec| spec.bank.value_type() == value_type)
        {
            let entry = self
                .parameters
                .entry((spec.bank, spec.address))
                .or_default();
            if spec.mode.is_some_and(|address| changed.contains(&address)) {
                entry.revision = entry.revision.wrapping_add(1);
                entry.current = None;
            }
            if let Some(raw) = raw_at(spec.address) {
                entry.revision = entry.revision.wrapping_add(1);
                entry.current = spec.decode(raw);
            }
        }
    }

    pub(crate) fn rename(&mut self, slot: u32, name: &str) {
        self.names.insert(slot, name.to_string());
    }

    fn ready(&self, spec: &Spec) -> bool {
        spec.mode
            .is_none_or(|address| self.modes.get(&address) == Some(&0))
    }

    pub(crate) fn controls(&self) -> Vec<DestinationControl> {
        specs()
            .iter()
            .map(|spec| DestinationControl {
                target: spec.target.clone(),
                target_label: spec.target_label.clone(),
                target_name: spec
                    .name_slot
                    .and_then(|slot| self.names.get(&slot))
                    .filter(|name| !name.is_empty())
                    .cloned(),
                send_slot: None,
                surface: spec.surface.into(),
                id: spec.id.clone(),
                label: spec.label.clone(),
                tap_tempo: false,
                description: if spec.bank == Bank::System {
                    "Shared temperature compensation for alignment delays on all outputs. Changing this changes delay times while preserving speaker distances.".into()
                } else {
                    format!("Changes {} processing for everything feeding {}.",
                        spec.label.to_lowercase(), spec.target_label)
                },
                choices: spec
                    .choices
                    .iter()
                    .map(|(value, label)| DestinationChoice {
                        value: *value as f32,
                        label: (*label).into(),
                    })
                    .collect(),
                range: spec.choices.is_empty().then(|| DestinationRange {
                    min: spec.min,
                    max: spec.max,
                    unit: spec.unit.into(),
                    decimals: 2,
                }),
                current: self
                    .ready(spec)
                    .then(|| {
                        self.parameters
                            .get(&(spec.bank, spec.address))
                            .and_then(|entry| entry.current.clone())
                    })
                    .flatten(),
            })
            .collect()
    }
}

impl Dl32sDriver {
    pub(crate) async fn write_output_processing(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
    ) -> anyhow::Result<bool> {
        let Some(spec) = specs()
            .iter()
            .find(|spec| spec.target == target && spec.id == control)
        else {
            return Ok(false);
        };
        anyhow::ensure!(
            spec.allows(value) && spec.allows(expected),
            "Output parameter outside documented range"
        );
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        anyhow::ensure!(session.is_alive(), "Output processing session unavailable");
        let generation = self.generation.load(Ordering::Acquire);
        let revision = {
            let mut native = self
                .native
                .lock()
                .map_err(|_| anyhow::anyhow!("Output cache unavailable"))?;
            anyhow::ensure!(
                native.generation == generation && native.view.connected,
                "Output connection changed"
            );
            let cache = &mut native.output_processing;
            anyhow::ensure!(
                cache.ready(spec)
                    && spec
                        .mode
                        .is_none_or(|a| session.cached_verified(a) == Some(0)),
                "Output processor changed; reopen the control"
            );
            let entry = cache
                .parameters
                .get_mut(&(spec.bank, spec.address))
                .ok_or_else(|| anyhow::anyhow!("Output value unavailable"))?;
            anyhow::ensure!(
                entry.current.as_ref().is_some_and(|v| v.value == expected),
                "Output value changed; reopen the control"
            );
            if let Some(raw) = spec.bank.verified(session, spec.address) {
                anyhow::ensure!(
                    raw == spec.raw(expected),
                    "Output value changed; reopen the control"
                );
            }
            entry.current = None;
            entry.revision
        };
        session
            .write_int(spec.address, spec.raw(value), spec.bank.value_type())
            .await?;
        let mut native = self
            .native
            .lock()
            .map_err(|_| anyhow::anyhow!("Output cache unavailable"))?;
        anyhow::ensure!(
            self.generation.load(Ordering::Acquire) == generation
                && native.generation == generation
                && native.view.connected,
            "Output connection changed while writing; result uncertain"
        );
        let cache = &mut native.output_processing;
        anyhow::ensure!(
            cache.ready(spec),
            "Output processor changed while writing; result uncertain"
        );
        let entry = cache
            .parameters
            .get_mut(&(spec.bank, spec.address))
            .ok_or_else(|| anyhow::anyhow!("Output unavailable"))?;
        if entry.revision == revision {
            entry.current = Some(DestinationValue {
                value,
                verified: false,
            });
        }
        Ok(true)
    }
}
