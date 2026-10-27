//! In-memory output and USB patch plane, separate from input-strip parameters.
use crate::SimDriver;
use openfader_core::routing::{OutputPlane, RouteState};
use std::sync::atomic::Ordering;

pub fn planes() -> Vec<OutputPlane> {
    [
        ("output/socket", "Outputs", 16),
        ("output/usb", "USB sends", 32),
    ]
    .into_iter()
    .map(|(key, label, count)| OutputPlane {
        key,
        label,
        count,
        clearable: true,
        sources: vec![
            ("Input", "Input post DSP", 32),
            ("Bus", "Mix", 8),
            ("Main", "Main", 2),
        ],
    })
    .collect()
}
fn validate(id: &str, source: Option<&str>) -> anyhow::Result<()> {
    let (key, slot) = id
        .rsplit_once('/')
        .ok_or_else(|| anyhow::anyhow!("Unknown output"))?;
    let slot: u16 = slot.parse()?;
    let plane = planes()
        .into_iter()
        .find(|p| p.key == key)
        .ok_or_else(|| anyhow::anyhow!("Unknown output plane"))?;
    anyhow::ensure!(
        slot > 0 && slot <= plane.count && id == format!("{key}/{slot:02}"),
        "Unknown output slot"
    );
    if let Some(source) = source.filter(|s| *s != "—") {
        let (group, index) = source
            .split_once(' ')
            .ok_or_else(|| anyhow::anyhow!("Unknown output source"))?;
        let index: usize = index.parse()?;
        anyhow::ensure!(
            plane
                .sources
                .iter()
                .any(|(g, _, n)| *g == group && index > 0 && index <= *n),
            "Output source not allowed"
        );
    }
    Ok(())
}
impl SimDriver {
    pub fn output_route(&self, id: &str) -> anyhow::Result<RouteState> {
        validate(id, None)?;
        let rows = self.output_routes.read().unwrap();
        let generation = self.processor_generation.load(Ordering::Acquire);
        let (source, revision) = rows.get(id).cloned().unwrap_or(("—".into(), 0));
        Ok(RouteState {
            source,
            revision,
            generation,
        })
    }
    pub fn set_output_route(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
    ) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, &|| true)
    }

    pub fn set_output_route_guarded(
        &self,
        id: &str,
        source: &str,
        expected: &RouteState,
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        validate(id, Some(source))?;
        let mut rows = self.output_routes.write().unwrap();
        // A queued writer must use the engine context that is current after
        // acquiring the mutation lock, just like the native provider lane.
        anyhow::ensure!(context(), "Output engine session or policy changed");
        let generation = self.processor_generation.load(Ordering::Acquire);
        let (current, revision) = rows.get(id).cloned().unwrap_or(("—".into(), 0));
        anyhow::ensure!(
            expected.generation == generation
                && expected.revision == revision
                && expected.source == current,
            "Output source/revision/session changed"
        );
        rows.insert(id.into(), (source.into(), revision.wrapping_add(1)));
        Ok(())
    }
}
