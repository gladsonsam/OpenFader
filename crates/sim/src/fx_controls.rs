//! DL32S catalogue simulation. Native defaults are simulation seeds only.
use crate::SimDriver;
use openfader_core::{
    destinations::{DestinationControl, DestinationValue},
    fx::dl32s,
};
use std::{collections::BTreeMap, sync::atomic::Ordering};
#[derive(Debug)]
pub(crate) struct FxSlot {
    algorithm: u32,
    revision: u64,
    values: BTreeMap<String, f32>,
}
impl FxSlot {
    fn select(&mut self, id: u32) -> anyhow::Result<()> {
        let a =
            dl32s::algorithm(id).ok_or_else(|| anyhow::anyhow!("sim: FX algorithm unavailable"))?;
        // Model firmware resets across different bank geometries. Common mute,
        // return and source controls live outside the algorithm parameter bank.
        self.values.retain(|id, _| {
            matches!(
                id.as_str(),
                "mute" | "send_mute" | "main_return" | "send_master" | "send_source"
            )
        });
        self.values
            .extend(a.fields.iter().map(|f| (f.id.to_string(), f.default)));
        self.algorithm = id;
        Ok(())
    }
    fn rows(&self, slot: u16, generation: u64) -> Vec<DestinationControl> {
        let mut rows = vec![dl32s::selector(
            slot,
            Some(self.algorithm),
            self.revision,
            generation,
        )];
        let a = dl32s::algorithm(self.algorithm).expect("sim algorithm");
        rows.extend(a.fields.iter().map(|f| {
            f.control(
                slot,
                a,
                self.values.get(f.id).copied(),
                self.revision,
                generation,
            )
        }));
        for (id, label, binary) in [
            ("mute", "Mute", true),
            ("send_mute", "Send mute", true),
            ("send_source", "Send source", true),
            ("main_return", "Main return", false),
            ("send_master", "Send master", false),
        ] {
            let mut r = rows[0].clone();
            r.id = id.to_string();
            r.label = label.to_string();
            r.description = format!("Simulated FX {slot} {label}");
            r.choices.clear();
            if binary {
                r.choices = [
                    (
                        0.0,
                        if id == "send_source" {
                            "Post Fader"
                        } else {
                            "Off"
                        },
                    ),
                    (
                        1.0,
                        if id == "send_source" {
                            "Pre Fader"
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
                r.range = Some(openfader_core::destinations::DestinationRange {
                    min: -120.0,
                    max: 10.0,
                    unit: "dB".to_string(),
                    decimals: 1,
                });
            }
            r.current = self.values.get(id).map(|value| DestinationValue {
                value: *value,
                verified: true,
            });
            rows.push(r);
        }
        rows
    }
}
pub(crate) fn initial() -> Vec<FxSlot> {
    [2, 12, 7, 10]
        .into_iter()
        .map(|id| {
            let mut slot = FxSlot {
                algorithm: id,
                revision: 0,
                values: BTreeMap::from([
                    ("mute".to_string(), 1.0),
                    ("send_mute".to_string(), 0.0),
                    ("send_source".to_string(), 0.0),
                    ("main_return".to_string(), -12.0),
                    ("send_master".to_string(), -6.0),
                ]),
            };
            slot.select(id).expect("sim algorithm");
            slot
        })
        .collect()
}
impl SimDriver {
    pub(crate) fn fx_controls(&self) -> Vec<DestinationControl> {
        let generation = self.processor_generation.load(Ordering::Acquire);
        self.fx_controls
            .read()
            .expect("sim FX poisoned")
            .iter()
            .enumerate()
            .flat_map(|(i, s)| s.rows(i as u16 + 1, generation))
            .collect()
    }
    pub(crate) fn write_sim_fx(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let i = (0..4)
            .find(|i| target == format!("fx/{:02}", i + 1))
            .ok_or_else(|| anyhow::anyhow!("sim: FX target unavailable"))?;
        let mut slots = self.fx_controls.write().expect("sim FX poisoned");
        let generation = self.processor_generation.load(Ordering::Acquire);
        let slot = &mut slots[i];
        let rows = slot.rows(i as u16 + 1, generation);
        anyhow::ensure!(
            context() && rows == snapshot,
            "sim: FX algorithm/value/revision/session changed"
        );
        let row = rows
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("sim: FX field unavailable"))?;
        anyhow::ensure!(
            row.allows_value(value) && row.current.as_ref().is_some_and(|v| v.value == expected),
            "sim: FX value outside domain or changed"
        );
        if id == "algorithm" {
            slot.select(value as u32)?;
        } else {
            if let Some(f) =
                dl32s::algorithm(slot.algorithm).and_then(|a| a.fields.iter().find(|f| f.id == id))
            {
                anyhow::ensure!(f.encode(value).is_some(), "sim: FX encoding unavailable");
            }
            slot.values.insert(id.to_string(), value);
        }
        slot.revision = slot.revision.wrapping_add(1);
        slot.rows(i as u16 + 1, generation)
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("sim: FX result unavailable"))
    }
}
