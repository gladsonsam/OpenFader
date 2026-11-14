//! Generic orchestration for typed provider-owned native processor blocks.
use super::super::*;
use openfader_core::native_processor::{NativeProcessorDomain, NativeProcessorSnapshot};

struct ProcessorAttempt<'a> {
    engine: &'a Engine,
    driver: &'a str,
    generation: u64,
}
impl Drop for ProcessorAttempt<'_> {
    fn drop(&mut self) {
        self.engine.invalidate_after_native_attempt(self.driver, self.generation);
        let _sync = self.engine.sync_state.lock().unwrap();
        self.engine.native_control_in_flight.store(false, Ordering::Release);
    }
}

pub(in crate::engine) fn covers(snapshot: &NativeProcessorSnapshot, key: &str, field: &str) -> bool {
    let domain = if field.starts_with("gate_") {
        Some(NativeProcessorDomain::Gate)
    } else if field.starts_with("comp_") {
        Some(NativeProcessorDomain::Dynamics)
    } else if field == "eq_mode" || field == "eq_on" ||
        field.strip_prefix("eq").is_some_and(|rest| rest.bytes().next().is_some_and(|b| b.is_ascii_digit())) {
        Some(NativeProcessorDomain::Eq)
    } else { None };
    domain.is_some_and(|domain| snapshot.blocks.iter().any(|b| b.strip_key == key && b.domain == domain))
}

impl Engine {
    pub(in crate::engine) async fn capture_scene_native_processors(&self, driver: &str, generation: u64) -> anyhow::Result<Option<NativeProcessorSnapshot>> {
        let native_generation = self.native_control_generation.load(Ordering::Acquire);
        let check = || self.active_driver() == driver && self.connection_gen.load(Ordering::Acquire) == generation
            && !self.reads_suspended.load(Ordering::Acquire)
            && !self.native_control_in_flight.load(Ordering::Acquire)
            && self.native_control_generation.load(Ordering::Acquire) == native_generation;
        anyhow::ensure!(check(), "native processor capture context changed");
        let snapshot = self.drv(driver).capture_native_processors(&check).await?;
        if let Some(snapshot) = &snapshot {
            anyhow::ensure!(snapshot.provider == driver, "native processor provider mismatch");
            openfader_core::native_processor::validate_structure(snapshot)?;
            self.drv(driver).validate_native_processors(snapshot)?;
        }
        anyhow::ensure!(check(), "native processor capture context changed");
        Ok(snapshot)
    }

    pub(in crate::engine) async fn revalidate_scene_native_processors(&self, driver: &str, generation: u64, native_generation: u64, snapshot: &NativeProcessorSnapshot) -> anyhow::Result<()> {
        let check = || self.active_driver() == driver && self.connection_gen.load(Ordering::Acquire) == generation
            && !self.reads_suspended.load(Ordering::Acquire)
            && !self.native_control_in_flight.load(Ordering::Acquire)
            && self.native_control_generation.load(Ordering::Acquire) == native_generation;
        anyhow::ensure!(check(), "native processor capture context changed");
        self.drv(driver).revalidate_captured_native_processors(snapshot, &check).await?;
        anyhow::ensure!(check(), "native processor capture context changed");
        Ok(())
    }
}

impl Engine {
    pub(in crate::engine) async fn prepare_scene_native_processors(
        &self,
        scene: &StoredScene,
        scope: Option<&RecallScope>,
        driver: &str,
        generation: u64,
        writer: &str,
        topology: &[openfader_core::StripFamily],
    ) -> anyhow::Result<Option<NativeProcessorSnapshot>> {
        let drv = self.drv(driver);
        let snapshot = scene.native_processors.as_ref().filter(|snapshot| {
            snapshot.blocks.iter().any(|block| {
                let field = match block.domain { NativeProcessorDomain::Gate => "gate_mode", NativeProcessorDomain::Dynamics => "comp_mode", NativeProcessorDomain::Eq => "eq_mode" };
                scope.is_none_or(|s| s.allows_param_for(drv.as_ref(), field))
            })
        });
        if let Some(snapshot) = snapshot {
            anyhow::ensure!(snapshot.provider == driver, "native processor snapshot belongs to another provider");
            openfader_core::native_processor::validate_structure(snapshot)?;
            drv.validate_native_processors(snapshot)?;
            for block in &snapshot.blocks {
                anyhow::ensure!(scene.strips.iter().any(|strip| strip.key.as_deref() == Some(&block.strip_key)), "native processor block has no matching scene strip");
                anyhow::ensure!(openfader_core::strip_index_for_key(topology, &block.strip_key).is_some(), "native processor strip unavailable");
            }
        }
        // Old scenes may contain a model selector without its processor state.
        for strip in &scene.strips {
            let index = strip.key.as_deref().and_then(|key| openfader_core::strip_index_for_key(topology, key)).unwrap_or(strip.strip as usize);
            if scope.is_some_and(|s| !s.matches_strip(index as u16, topology)) { continue; }
            for (field, raw) in &strip.params {
                if scope.is_some_and(|s| !s.allows_param_for(drv.as_ref(), field)) { continue; }
                let value = json_to_param_value(raw)?;
                if let Some(domain) = drv.legacy_native_processor_required(field, &value)? {
                    let key = strip.key.clone().or_else(|| openfader_core::strip_key_for_index(topology, index));
                    anyhow::ensure!(key.as_deref().is_some_and(|key| snapshot.is_some_and(|snapshot| snapshot.blocks.iter().any(|block| block.strip_key == key && block.domain == domain))), "snapshot lacks native processor settings for strip {index} {field}");
                }
            }
        }
        let Some(snapshot) = snapshot else { return Ok(None) };
        let mut selected = snapshot.clone();
        selected.blocks.retain(|block| {
            let index = openfader_core::strip_index_for_key(topology, &block.strip_key).unwrap();
            let field = match block.domain { NativeProcessorDomain::Gate => "gate_mode", NativeProcessorDomain::Dynamics => "comp_mode", NativeProcessorDomain::Eq => "eq_mode" };
            scope.is_none_or(|s| s.matches_strip(index as u16, topology) && s.allows_param_for(drv.as_ref(), field))
        });
        if selected.blocks.is_empty() { return Ok(None); }
        let mut members = std::collections::BTreeSet::new();
        for block in &selected.blocks {
            let index = openfader_core::strip_index_for_key(topology, &block.strip_key).unwrap() as u16;
            let field = match block.domain { NativeProcessorDomain::Gate => "gate_mode", NativeProcessorDomain::Dynamics => "comp_mode", NativeProcessorDomain::Eq => "eq_mode" };
            // Existing native callbacks are synchronous and cannot prove an
            // asynchronously changing linked-member set on every leaf write.
            // Reject linked or linkable targets until a provider supplies a
            // guarded member-aware native recall contract.
            anyhow::ensure!(!drv.stereo_pairs().iter().any(|pair| pair.contains(&index)), "native processor recall unavailable for linkable targets");
            let affected = drv.control_members(StripId(index), field).await?;
            anyhow::ensure!(affected.as_slice() == [StripId(index)], "native processor recall unavailable for linked targets");
            members.insert(index);
        }
        let check = || self.active_driver() == driver && self.connection_gen.load(Ordering::Acquire) == generation
            && !self.reads_suspended.load(Ordering::Acquire) && self.policy_of(writer).allow_recall().is_ok()
            && members.iter().all(|ch| self.policy_of(writer).allow_strip(StripId(*ch)).is_ok() && self.lock_holder(*ch).is_none_or(|owner| owner == writer));
        anyhow::ensure!(check(), "native processor recall access, locks or session changed");
        drv.preflight_native_processors(&selected, &check).await?;
        anyhow::ensure!(check(), "native processor recall access, locks or session changed");
        Ok(Some(selected))
    }
}

impl Engine {
    pub(in crate::engine) async fn restore_scene_native_processors(
        &self, snapshot: &NativeProcessorSnapshot, driver: &str, generation: u64,
        writer: &str, topology: &[openfader_core::StripFamily],
    ) -> anyhow::Result<()> {
        // Console-control writes use this same lane after the routing guard.
        // Hold it until ProcessorAttempt has retired its in-flight marker.
        let _lane = self.native_lane.lock().await;
        anyhow::ensure!(
            self.active_driver() == driver
                && self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "native processor recall session changed while waiting"
        );
        let drv = self.drv(driver);
        let mut members = std::collections::BTreeSet::new();
        for block in &snapshot.blocks {
            let index = openfader_core::strip_index_for_key(topology, &block.strip_key)
                .ok_or_else(|| anyhow::anyhow!("native processor strip unavailable"))? as u16;
            let field = match block.domain { NativeProcessorDomain::Gate => "gate_mode", NativeProcessorDomain::Dynamics => "comp_mode", NativeProcessorDomain::Eq => "eq_mode" };
            // Existing native callbacks are synchronous and cannot prove an
            // asynchronously changing linked-member set on every leaf write.
            // Reject linked or linkable targets until a provider supplies a
            // guarded member-aware native recall contract.
            anyhow::ensure!(!drv.stereo_pairs().iter().any(|pair| pair.contains(&index)), "native processor recall unavailable for linkable targets");
            let affected = drv.control_members(StripId(index), field).await?;
            anyhow::ensure!(affected.as_slice() == [StripId(index)], "native processor recall unavailable for linked targets");
            members.insert(index);
        }
        let check = || self.active_driver() == driver && self.connection_gen.load(Ordering::Acquire) == generation
            && !self.reads_suspended.load(Ordering::Acquire) && self.policy_of(writer).allow_recall().is_ok()
            && members.iter().all(|ch| self.policy_of(writer).allow_strip(StripId(*ch)).is_ok() && self.lock_holder(*ch).is_none_or(|owner| owner == writer));
        anyhow::ensure!(check(), "native processor recall access, locks or session changed");
        {
            let _sync = self.sync_state.lock().unwrap();
            anyhow::ensure!(check() && !self.native_control_in_flight.load(Ordering::Acquire), "native processor recall context changed");
            self.native_control_generation.fetch_add(1, Ordering::AcqRel);
            self.native_control_in_flight.store(true, Ordering::Release);
        }
        let attempt = ProcessorAttempt { engine: self, driver, generation };
        let result = drv.recall_native_processors(snapshot, &check).await;
        // Errors and cancellation can follow a model or leaf write.
        drop(attempt);
        result.map_err(|error|
            anyhow::anyhow!("native processor recall failed; mixer may be partially changed: {error:#}"))?;
        anyhow::ensure!(check(), "native processor recall interrupted; mixer may be partially changed");
        Ok(())
    }
}
