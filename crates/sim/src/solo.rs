//! Guarded hard-solo simulation shares destination descriptors and legacy strips.
use crate::{SimDriver, SIM_TOPOLOGY};
use openfader_core::destinations::{DestinationControl, DestinationValue};
use std::sync::atomic::Ordering;

fn strip_identity(ch: u16) -> Option<(String, &'static str)> {
    SIM_TOPOLOGY
        .iter()
        .find(|f| f.contains(ch as usize))
        .and_then(|f| {
            let n = ch as usize - f.first + 1;
            let (family, id) = match f.key {
                "ch" => ("ch", "solo"),
                "ch2" => ("ch2", "solo"),
                "mtx" => ("mtx", "solo"),
                "mix" => ("bus", "solo"),
                "fxsnd" => ("fx", "send_solo"),
                "fxret" => ("fx", "return_solo"),
                "dca" => ("vca", "solo"),
                "ret" => ("return", "solo"),
                _ => return None,
            };
            Some((format!("{family}/{n:02}"), id))
        })
}
pub(crate) fn controls() -> Vec<DestinationControl> {
    let mut rows = openfader_core::solo::controls();
    // Keep the simulation's existing extra input/matrix strips in its honest
    // Clear All coverage; DL32S still advertises exactly its native 62 bits.
    for family in SIM_TOPOLOGY {
        for offset in 0..family.count {
            let Some((target, id)) = strip_identity((family.first + offset) as u16) else {
                continue;
            };
            if rows.iter().any(|r| r.target == target && r.id == id) {
                continue;
            }
            let mut row = rows[0].clone();
            row.target = target.clone();
            row.members = vec![target];
            row.target_label = format!("{} {}", family.name_prefix, offset + 1);
            rows.push(row);
        }
    }
    rows
}
impl SimDriver {
    pub(crate) fn legacy_solo_descriptor(&self, ch: u16, value: bool) {
        if let Some((target, id)) = strip_identity(ch) {
            let mut rows = self.globals.write().expect("sim globals poisoned");
            if let Some(row) = rows
                .iter_mut()
                .find(|r| r.surface == "solo" && r.target == target && r.id == id)
            {
                row.current = Some(DestinationValue {
                    value: f32::from(value),
                    verified: true,
                });
                row.revision = row.revision.map(|r| r.wrapping_add(1));
            }
        }
    }
    pub(crate) fn write_sim_solo(
        &self,
        target: &str,
        id: &str,
        value: f32,
        expected: f32,
        snapshot: &[DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<DestinationControl> {
        let mut rows = self.globals.write().expect("sim globals poisoned");
        let generation = self.processor_generation.load(Ordering::Acquire);
        let block: Vec<_> = rows
            .iter()
            .filter(|r| r.surface == "solo" && r.target == target)
            .map(|r| {
                let mut r = r.clone();
                r.generation = Some(generation);
                r
            })
            .collect();
        anyhow::ensure!(
            snapshot == block && context(),
            "sim: solo scope or authority changed"
        );
        let row = rows
            .iter_mut()
            .find(|r| r.surface == "solo" && r.target == target && r.id == id)
            .ok_or_else(|| anyhow::anyhow!("sim: solo target unsupported"))?;
        anyhow::ensure!(
            row.allows_value(value)
                && row
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "sim: solo value changed"
        );
        row.current = Some(DestinationValue {
            value,
            verified: true,
        });
        row.revision = row.revision.map(|r| r.wrapping_add(1));
        for ch in 0..crate::SIM_STRIPS as u16 {
            if strip_identity(ch).is_some_and(|(t, c)| t == target && c == id) {
                self.solos
                    .write()
                    .expect("sim solos poisoned")
                    .insert(ch, value == 1.);
            }
        }
        let mut receipt = row.clone();
        receipt.generation = Some(generation);
        Ok(receipt)
    }
}
