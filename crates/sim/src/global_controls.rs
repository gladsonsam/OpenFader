//! Reference global-control simulation; no hardware routing or signal generation.
use crate::SimDriver;
use openfader_core::destinations::DestinationControl;
use std::sync::atomic::Ordering;

impl SimDriver {
    pub(crate) fn global_controls(&self) -> Vec<DestinationControl> {
        let generation = self.processor_generation.load(Ordering::Acquire);
        self.globals
            .read()
            .expect("sim globals poisoned")
            .iter()
            .map(|row| {
                let mut row = row.clone();
                row.generation = Some(generation);
                row
            })
            .collect()
    }
    pub(crate) fn write_global(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: Option<&[DestinationControl]>,
    ) -> anyhow::Result<()> {
        let mut rows = self.globals.write().expect("sim globals poisoned");
        if let Some(snapshot) = snapshot {
            let generation = self.processor_generation.load(Ordering::Acquire);
            let block: Vec<_> = rows
                .iter()
                .filter(|row| row.target == target)
                .map(|row| {
                    let mut row = row.clone();
                    row.generation = Some(generation);
                    row
                })
                .collect();
            anyhow::ensure!(
                snapshot.len() == block.len() && snapshot.iter().all(|row| block.contains(row)),
                "sim: global configuration changed"
            );
        }
        let row = rows
            .iter_mut()
            .find(|row| row.target == target && row.id == control)
            .ok_or_else(|| anyhow::anyhow!("sim: global control unavailable"))?;
        anyhow::ensure!(
            row.allows_value(value)
                && row
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == expected),
            "sim: global value changed or outside domain"
        );
        row.current = Some(openfader_core::destinations::DestinationValue {
            value,
            verified: true,
        });
        row.revision = row.revision.map(|r| r.wrapping_add(1));
        Ok(())
    }
}
