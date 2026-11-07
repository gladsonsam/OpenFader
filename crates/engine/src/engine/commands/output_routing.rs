//! Stable output route execution: no positional/strip-key compatibility writes.
use super::*;
impl Engine {
    pub(super) async fn apply_output_route(
        &self,
        cmd: &Cmd,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<bool> {
        let Cmd::RoutePatch {
            driver,
            epoch,
            destination,
            expected,
            guard,
            source,
        } = cmd
        else {
            return Ok(false);
        };
        anyhow::ensure!(
            *driver == self.active_driver()
                && *epoch == self.connection_gen.load(Ordering::Acquire)
                && !self.reads_suspended.load(Ordering::Acquire),
            "routing session changed"
        );
        let drv = self.drv(driver);
        let dest = drv
            .routing_planes()
            .into_iter()
            .flat_map(|p| p.destinations)
            .find(|d| d.id == *destination)
            .ok_or_else(|| anyhow::anyhow!("unknown routing destination"))?;
        if !dest.output {
            return Ok(false);
        }
        self.policy_of(writer).allow_master()?;
        let guard = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Output requires source/revision/session guard"))?;
        let state = drv.output_route(destination)?;
        anyhow::ensure!(
            state.source == *expected && *guard == route_guard(&state),
            "Output source/revision/session changed"
        );
        let label = if let Some(source) = source {
            anyhow::ensure!(
                dest.source_groups.iter().any(|g| g.key == source.group
                    && source.index > 0
                    && usize::from(source.index) <= g.count),
                "Output source is not advertised"
            );
            format!("{} {}", source.group, source.index)
        } else {
            anyhow::ensure!(dest.clearable, "Output cannot be cleared");
            "-".into()
        };
        // The provider repeats the exact guard while holding its write lane,
        // preserving revisions against ABA edits and queued local writers.
        let context = || {
            *driver == self.active_driver()
                && *epoch == self.connection_gen.load(Ordering::Acquire)
                && !self.reads_suspended.load(Ordering::Acquire)
                && self.policy_of(writer).allow_master().is_ok()
        };
        drv.set_output_route(destination, &label, &state, &context)
            .await?;
        anyhow::ensure!(
            *driver == self.active_driver()
                && *epoch == self.connection_gen.load(Ordering::Acquire),
            "Output session changed; result uncertain"
        );
        self.note_write(writer);
        self.touch_holder_locks(writer);
        out.push(self.routing_message().await);
        Ok(true)
    }
}
