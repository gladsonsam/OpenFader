//! Verified output EQ scene capture and scoped recall.

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

pub(super) struct PreparedOutputEq {
    saved: StoredOutputEqTarget,
    strip: StripId,
    members: Vec<StripId>,
}

type RecallCheck<'a> = &'a (dyn Fn() -> anyhow::Result<()> + Send + Sync);

fn validate_target_recall(
    saved: &StoredOutputEqTarget,
    block: &[DestinationControl],
) -> anyhow::Result<StoredOutputEqTarget> {
    let live = output_eq_target_from_controls(&saved.target, block)?;
    anyhow::ensure!(
        live.gains_db.len() == saved.gains_db.len(),
        "output EQ roster changed"
    );
    for (id, value) in std::iter::once(("eq_mode".to_string(), f32::from(saved.mode))).chain(
        saved
            .gains_db
            .iter()
            .enumerate()
            .map(|(i, value)| (format!("geq{:02}_gain", i + 1), *value)),
    ) {
        anyhow::ensure!(
            block
                .iter()
                .any(|c| c.id == id && scene_recall::snapshot_control_allows(c, value)),
            "snapshot output EQ value not advertised: {id}"
        );
    }
    Ok(live)
}

fn target_matches(saved: &StoredOutputEqTarget, actual: &StoredOutputEqTarget) -> bool {
    saved.target == actual.target
        && saved.mode == actual.mode
        && saved.gains_db.len() == actual.gains_db.len()
        && saved
            .gains_db
            .iter()
            .zip(&actual.gains_db)
            .all(|(a, b)| (a - b).abs() <= 0.0001)
}

impl Engine {
    /// Select output processing through the same EQ group and stable strip
    /// selectors as strip PEQ. Shared monitor/RTA preferences have separate scopes.
    pub(super) async fn prepare_scene_output_eq(
        &self,
        saved: Option<&StoredOutputEq>,
        scope: Option<&RecallScope>,
        driver: &str,
        check: RecallCheck<'_>,
    ) -> anyhow::Result<Vec<PreparedOutputEq>> {
        check()?;
        let Some(saved) = saved
            .filter(|eq| eq.provider == driver)
            .filter(|_| scope.is_none_or(|s| s.allows_group("eq")))
        else {
            return Ok(Vec::new());
        };
        crate::local_library::validate_stored_output_eq(saved, "output_eq")
            .map_err(anyhow::Error::msg)?;
        let drv = self.drv(driver);
        let topology = self.driver_topology(driver);
        let advertised: BTreeSet<_> = drv
            .destination_controls()
            .into_iter()
            .filter(|c| c.surface == "output_geq")
            .map(|c| c.target)
            .collect();
        let stored: BTreeSet<_> = saved.targets.iter().map(|t| t.target.clone()).collect();
        anyhow::ensure!(
            advertised == stored,
            "snapshot output EQ target roster changed"
        );
        let mut prepared = Vec::new();
        for target in &saved.targets {
            let index = openfader_core::strip_index_for_key(&topology, &target.target).ok_or_else(
                || anyhow::anyhow!("snapshot output EQ target unavailable: {}", target.target),
            )?;
            let strip = StripId(u16::try_from(index)?);
            if scope.is_some_and(|s| !s.matches_strip(strip.0, &topology)) {
                continue;
            }
            check()?;
            let block = drv
                .read_output_geq(&target.target)
                .await?
                .ok_or_else(|| anyhow::anyhow!("snapshot output EQ readback unavailable"))?;
            check()?;
            validate_target_recall(target, &block)?;
            let members = drv.control_members(strip, "eq_mode").await?;
            check()?;
            anyhow::ensure!(
                members.contains(&strip)
                    && members.iter().all(|m| openfader_core::strip_key_for_index(
                        &topology,
                        usize::from(m.0)
                    )
                    .is_some()),
                "snapshot output EQ members unavailable"
            );
            prepared.push(PreparedOutputEq {
                saved: target.clone(),
                strip,
                members,
            });
        }
        Ok(prepared)
    }

    pub(super) async fn recall_scene_output_eq(
        &self,
        prepared: &[PreparedOutputEq],
        driver: &str,
        check: RecallCheck<'_>,
        affected: &std::sync::Mutex<BTreeSet<u16>>,
    ) -> anyhow::Result<()> {
        let drv = self.drv(driver);
        let topology = self.driver_topology(driver);
        for target in prepared {
            check()?;
            let mut block = drv
                .read_output_geq(&target.saved.target)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("snapshot output EQ unavailable; mixer may be partly changed")
                })?;
            check()?;
            validate_target_recall(&target.saved, &block)?;
            // Restore dormant gains first; switch the requested EQ mode last.
            let desired = target
                .saved
                .gains_db
                .iter()
                .enumerate()
                .map(|(i, value)| (format!("geq{:02}_gain", i + 1), *value))
                .chain(std::iter::once((
                    "eq_mode".into(),
                    f32::from(target.saved.mode),
                )));
            for (id, value) in desired {
                check()?;
                let expected = block
                    .iter()
                    .find(|c| c.id == id)
                    .unwrap()
                    .current
                    .as_ref()
                    .unwrap()
                    .value;
                if (expected - value).abs() <= 0.0001 {
                    continue;
                }
                let context = || {
                    (|| -> anyhow::Result<()> {
                        check()?;
                        let members = drv
                            .current_control_members(target.strip, &id)?
                            .unwrap_or_else(|| target.members.clone());
                        anyhow::ensure!(
                            members.contains(&target.strip)
                                && members.iter().all(|m| openfader_core::strip_key_for_index(
                                    &topology,
                                    usize::from(m.0)
                                )
                                .is_some()),
                            "snapshot output EQ members changed"
                        );
                        // Numbered recalls explicitly override locks. Record the
                        // current companions again at the native dispatch check.
                        affected.lock().unwrap().extend(members.iter().map(|m| m.0));
                        Ok(())
                    })()
                    .is_ok()
                };
                let receipt = drv
                    .set_output_processor_guarded(
                        &target.saved.target,
                        &id,
                        value,
                        expected,
                        &block,
                        &context,
                    )
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "snapshot output EQ recall unconfirmed; mixer may be partly changed"
                        )
                    })?;
                check()?;
                anyhow::ensure!(
                    receipt.target == target.saved.target
                        && receipt.surface == "output_geq"
                        && receipt.id == id
                        && receipt
                            .current
                            .as_ref()
                            .is_some_and(|v| v.verified && (v.value - value).abs() <= 0.0001),
                    "snapshot output EQ recall unconfirmed; mixer may be partly changed"
                );
                block = drv
                    .read_output_geq(&target.saved.target)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "snapshot output EQ readback unavailable; mixer may be partly changed"
                        )
                    })?;
                check()?;
                validate_target_recall(&target.saved, &block)?;
            }
        }
        // Verify every selected output after the complete write sequence,
        // including unchanged bands and outputs that needed no commands.
        for target in prepared {
            check()?;
            let block = drv.read_output_geq(&target.saved.target).await?
                .ok_or_else(|| anyhow::anyhow!("snapshot output EQ final readback unavailable; mixer may be partly changed"))?;
            check()?;
            let actual = validate_target_recall(&target.saved, &block)?;
            anyhow::ensure!(
                target_matches(&target.saved, &actual),
                "snapshot output EQ final readback changed for {}; mixer may be partly changed",
                target.saved.target
            );
        }
        Ok(())
    }
}
