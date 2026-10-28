//! Verified output EQ scene capture and metadata parsing.

use super::*;
use openfader_core::destinations::DestinationControl;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn output_eq_target_from_controls(
    target: &str,
    block: &[DestinationControl],
) -> anyhow::Result<StoredOutputEqTarget> {
    anyhow::ensure!(
        block.len() >= 2 && block.len() <= 129,
        "output EQ block incomplete"
    );
    let generation = block[0]
        .generation
        .ok_or_else(|| anyhow::anyhow!("output EQ generation unavailable"))?;
    let mut values = BTreeMap::new();
    for control in block {
        let current = control
            .current
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("output EQ readback unavailable"))?;
        anyhow::ensure!(
            control.target == target
                && control.surface == "output_geq"
                && control.generation == Some(generation)
                && control.revision.is_some()
                && current.verified
                && scene_recall::snapshot_control_allows(control, current.value),
            "invalid output EQ control {}",
            control.id
        );
        if control.id != "eq_mode" && control.choices.is_empty() {
            anyhow::ensure!(
                control.range.as_ref().is_some_and(|r| r.unit == "dB"),
                "output EQ gain requires dB metadata"
            );
        }
        anyhow::ensure!(
            values.insert(control.id.as_str(), current.value).is_none(),
            "duplicate output EQ control"
        );
    }
    let mode = values
        .remove("eq_mode")
        .ok_or_else(|| anyhow::anyhow!("output EQ mode missing"))?;
    anyhow::ensure!(
        mode.fract() == 0.0
            && (0.0..=255.0).contains(&mode)
            && block
                .iter()
                .find(|c| c.id == "eq_mode")
                .is_some_and(|c| c.choices.iter().any(|choice| choice.value == mode)),
        "invalid output EQ mode"
    );
    let mut gains_db = Vec::with_capacity(block.len() - 1);
    for index in 1..block.len() {
        gains_db.push(
            values
                .remove(format!("geq{index:02}_gain").as_str())
                .ok_or_else(|| anyhow::anyhow!("output EQ gain {index} missing"))?,
        );
    }
    anyhow::ensure!(values.is_empty(), "unknown output EQ control");
    Ok(StoredOutputEqTarget {
        target: target.to_string(),
        mode: mode as u8,
        gains_db,
    })
}

impl Engine {
    pub(super) async fn capture_scene_output_eq(
        &self,
        driver: &str,
        generation: u64,
    ) -> anyhow::Result<Option<StoredOutputEq>> {
        let check = || {
            anyhow::ensure!(
                self.active_driver() == driver
                    && self.connection_gen.load(Ordering::Acquire) == generation
                    && !self.reads_suspended.load(Ordering::Acquire),
                "desk changed during output EQ capture"
            );
            Ok(())
        };
        check()?;
        let drv = self.drv(driver);
        let targets: BTreeSet<_> = drv
            .destination_controls()
            .into_iter()
            .filter(|c| c.surface == "output_geq")
            .map(|c| c.target)
            .collect();
        check()?;
        let mut rows = Vec::new();
        let mut missing = 0;
        for target in &targets {
            check()?;
            let result = drv.read_output_geq(target).await;
            check()?;
            match result? {
                Some(block) => rows.push(output_eq_target_from_controls(target, &block)?),
                None => missing += 1,
            }
        }
        if rows.is_empty() {
            return Ok(None);
        }
        anyhow::ensure!(missing == 0, "output EQ capture incomplete");
        let eq = StoredOutputEq {
            provider: driver.to_string(),
            targets: rows,
        };
        crate::local_library::validate_stored_output_eq(&eq, "output_eq")
            .map_err(anyhow::Error::msg)?;
        Ok(Some(eq))
    }
}
