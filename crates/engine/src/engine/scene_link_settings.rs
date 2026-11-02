//! Provider-qualified coupling settings around scene capture and recall.
use super::*;
use openfader_core::destinations::DestinationControl;
use openfader_core::link_settings::StereoLinkSettings;
use std::collections::BTreeSet;

type Check<'a> = &'a (dyn Fn() -> anyhow::Result<()> + Send + Sync);
const BUS_IDS: [&str; 3] = ["pair_1_2", "pair_3_4", "pair_5_6"];
const POLICY_IDS: [&str; 4] = ["preamp", "eq", "dyn", "fdrmute"];

pub(super) struct CapturedLinks {
    global: Option<StereoLinkSettings>,
    inputs: Vec<([u16; 2], bool)>,
}

pub(super) struct PreparedLinks {
    wanted: StereoLinkSettings,
    before: StereoLinkSettings,
    native_generation: u64,
}

fn fields(values: StereoLinkSettings) -> Vec<(&'static str, &'static str, &'static str, bool)> {
    POLICY_IDS
        .iter()
        .enumerate()
        .map(|(i, id)| ("link_policies", "link_policy", *id, values.policies[i]))
        .chain(
            BUS_IDS
                .iter()
                .enumerate()
                .map(|(i, id)| ("bus_links", "bus_link", *id, values.bus_links[i])),
        )
        .collect()
}

fn cached_controls(
    drv: &dyn DynDriver,
    values: StereoLinkSettings,
    topo: &[openfader_core::StripFamily],
    generation: Option<u64>,
    pending: Option<(&str, &str)>,
) -> anyhow::Result<Vec<DestinationControl>> {
    let controls: Vec<_> = drv
        .destination_controls()
        .into_iter()
        .filter(|c| {
            matches!(
                (c.target.as_str(), c.surface.as_str()),
                ("bus_links", "bus_link") | ("link_policies", "link_policy")
            )
        })
        .collect();
    anyhow::ensure!(
        controls.len() == 7,
        "complete link setting descriptors unavailable"
    );
    let native_generation = generation
        .or(controls[0].generation)
        .ok_or_else(|| anyhow::anyhow!("link setting generation unavailable"))?;
    let all: BTreeSet<_> = topo
        .iter()
        .flat_map(|f| f.first..f.first + f.count)
        .map(|i| {
            openfader_core::strip_key_for_index(topo, i)
                .ok_or_else(|| anyhow::anyhow!("link setting topology incomplete"))
        })
        .collect::<anyhow::Result<_>>()?;
    for (target, surface, id, value) in fields(values) {
        let rows: Vec<_> = controls
            .iter()
            .filter(|c| c.target == target && c.id == id)
            .collect();
        anyhow::ensure!(
            rows.len() == 1,
            "link setting descriptor missing or duplicated"
        );
        let row = rows[0];
        let expected: BTreeSet<_> = if target == "link_policies" {
            all.clone()
        } else {
            let index = BUS_IDS
                .iter()
                .position(|candidate| *candidate == id)
                .unwrap();
            [
                openfader_core::strip_key("bus", (index * 2 + 1) as u16),
                openfader_core::strip_key("bus", (index * 2 + 2) as u16),
            ]
            .into_iter()
            .collect()
        };
        let actual: BTreeSet<_> = row.members.iter().cloned().collect();
        anyhow::ensure!(
            row.surface == surface
                && row.generation == Some(native_generation)
                && row.revision.is_some()
                && actual == expected
                && actual.len() == row.members.len()
                && actual
                    .iter()
                    .all(|key| openfader_core::strip_index_for_key(topo, key).is_some())
                && row.choices.len() == 2
                && [0.0, 1.0]
                    .iter()
                    .all(|v| row.choices.iter().any(|c| c.value == *v)),
            "link setting metadata changed: {id} (surface={}, generation={:?}/{native_generation}, revision={:?}, members={}/{}, unique={}, resolvable={}, choices={})",
            row.surface, row.generation, row.revision, actual.len(), expected.len(),
            actual.len() == row.members.len(), actual.iter().all(|key| openfader_core::strip_index_for_key(topo,key).is_some()), row.choices.len()
        );
        if pending != Some((target, id)) {
            anyhow::ensure!(
                row.current
                    .as_ref()
                    .is_some_and(|v| v.verified && v.value == f32::from(u8::from(value))),
                "link setting readback unavailable or changed: {id}"
            );
        }
    }
    Ok(controls)
}

impl Engine {
    fn check_link_capture(&self, driver: &str, generation: u64) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.active_driver() == driver
                && self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "desk changed during link settings capture"
        );
        Ok(())
    }

    pub(super) async fn begin_scene_link_capture(
        &self,
        driver: &str,
        generation: u64,
    ) -> anyhow::Result<CapturedLinks> {
        let check = || self.check_link_capture(driver, generation);
        check()?;
        let drv = self.drv(driver);
        let global = drv.read_link_settings().await?;
        check()?;
        let mut inputs = Vec::new();
        if drv.capture_input_links_with_global() && global.is_some() && !drv.param_read_only("link")
        {
            let topo = self.driver_topology(driver);
            let mut seen = BTreeSet::new();
            for pair in drv.stereo_pairs() {
                for member in pair {
                    anyhow::ensure!(
                        seen.insert(member)
                            && topo.iter().any(|f| f.kind == openfader_core::StripKind::In
                                && usize::from(member) >= f.first
                                && usize::from(member) < f.first + f.count),
                        "input link capture topology invalid"
                    );
                }
                inputs.push((
                    pair,
                    self.read_scene_input_link(driver, pair, &check).await?,
                ));
            }
            anyhow::ensure!(
                seen.len()
                    == topo
                        .iter()
                        .filter(|f| f.kind == openfader_core::StripKind::In)
                        .map(|f| f.count)
                        .sum::<usize>(),
                "input link capture topology incomplete"
            );
        }
        check()?;
        Ok(CapturedLinks { global, inputs })
    }

    pub(super) async fn finish_scene_link_capture(
        &self,
        before: &CapturedLinks,
        strips: &[StoredStrip],
        driver: &str,
        generation: u64,
    ) -> anyhow::Result<Option<StoredLinkSettings>> {
        let check = || self.check_link_capture(driver, generation);
        check()?;
        let topo = self.driver_topology(driver);
        for (pair, value) in &before.inputs {
            anyhow::ensure!(
                self.read_scene_input_link(driver, *pair, &check).await? == *value,
                "input links changed during capture"
            );
            for member in pair {
                let rows: Vec<_> = strips
                    .iter()
                    .filter(|s| {
                        s.key
                            .as_deref()
                            .and_then(|k| openfader_core::strip_index_for_key(&topo, k))
                            .map(|i| i as u16)
                            .unwrap_or(s.strip)
                            == *member
                    })
                    .collect();
                anyhow::ensure!(
                    rows.len() == 1
                        && rows[0].params.get("link").and_then(Value::as_bool) == Some(*value),
                    "captured input links disagree with verified context"
                );
            }
        }
        let after = self.drv(driver).read_link_settings().await?;
        check()?;
        anyhow::ensure!(
            after == before.global,
            "link settings changed during capture"
        );
        Ok(after.map(|settings| StoredLinkSettings {
            provider: driver.to_string(),
            settings,
        }))
    }

    async fn ready_link_controls(
        &self,
        driver: &str,
        values: StereoLinkSettings,
        native_generation: Option<u64>,
        check: Check<'_>,
    ) -> anyhow::Result<Vec<DestinationControl>> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            check()?;
            match cached_controls(
                self.drv(driver).as_ref(),
                values,
                &self.driver_topology(driver),
                native_generation,
                None,
            ) {
                Ok(controls) => return Ok(controls),
                Err(error) if Instant::now() >= deadline => return Err(error),
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    pub(super) async fn prepare_scene_link_settings(
        &self,
        saved: Option<&StoredLinkSettings>,
        scope: Option<&RecallScope>,
        driver: &str,
        check: Check<'_>,
    ) -> anyhow::Result<Option<PreparedLinks>> {
        let Some(saved) = saved.filter(|s| s.provider == driver).filter(|_| {
            scope.is_none_or(|s| {
                s.globals
                    .as_ref()
                    .is_some_and(|g| g.iter().any(|id| id == "link_settings"))
            })
        }) else {
            return Ok(None);
        };
        crate::local_library::validate_stored_link_settings(saved, "link_settings")
            .map_err(anyhow::Error::msg)?;
        check()?;
        let before = self
            .drv(driver)
            .read_link_settings()
            .await?
            .ok_or_else(|| anyhow::anyhow!("snapshot link settings unsupported"))?;
        check()?;
        let controls = self
            .ready_link_controls(driver, before, None, check)
            .await?;
        Ok(Some(PreparedLinks {
            wanted: saved.settings,
            before,
            native_generation: controls[0].generation.unwrap(),
        }))
    }

    pub(super) async fn restore_scene_link_settings(
        &self,
        prepared: Option<&PreparedLinks>,
        driver: &str,
        check: Check<'_>,
        affected: &std::sync::Mutex<BTreeSet<u16>>,
    ) -> anyhow::Result<()> {
        let Some(prepared) = prepared else {
            return Ok(());
        };
        let operation = async {
            let drv = self.drv(driver);
            let topo = self.driver_topology(driver);
            let mut expected = prepared.before;
            // Establish copy policy before changing bus pairing, then restore
            // ordinary route/level/processing state under the requested coupling.
            for (target, surface, id, value) in fields(prepared.wanted) {
                check()?;
                anyhow::ensure!(
                    drv.read_link_settings().await? == Some(expected),
                    "link settings changed after prevalidation"
                );
                check()?;
                let old = fields(expected)
                    .into_iter()
                    .find(|f| f.0 == target && f.2 == id)
                    .unwrap()
                    .3;
                if old == value {
                    continue;
                }
                let controls = self
                    .ready_link_controls(driver, expected, Some(prepared.native_generation), check)
                    .await?;
                let block: Vec<_> = controls
                    .into_iter()
                    .filter(|c| c.target == target)
                    .collect();
                let selected = block.iter().find(|c| c.id == id).unwrap();
                let members: Vec<_> = selected
                    .members
                    .iter()
                    .map(|key| {
                        openfader_core::strip_index_for_key(&topo, key)
                            .and_then(|i| u16::try_from(i).ok())
                            .ok_or_else(|| anyhow::anyhow!("link setting member unavailable"))
                    })
                    .collect::<anyhow::Result<_>>()?;
                let context = || {
                    if check().is_err()
                        || cached_controls(
                            drv.as_ref(),
                            expected,
                            &topo,
                            Some(prepared.native_generation),
                            Some((target, id)),
                        )
                        .is_err()
                    {
                        return false;
                    }
                    affected.lock().unwrap().extend(members.iter().copied());
                    true
                };
                let receipt = drv
                    .set_output_processor_guarded(
                        target,
                        id,
                        f32::from(u8::from(value)),
                        f32::from(u8::from(old)),
                        &block,
                        &context,
                    )
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("link setting recall unconfirmed"))?;
                check()?;
                anyhow::ensure!(
                    receipt.target == target
                        && receipt.surface == surface
                        && receipt.generation == Some(prepared.native_generation)
                        && receipt.id == id
                        && receipt
                            .current
                            .as_ref()
                            .is_some_and(|v| v.verified && v.value == f32::from(u8::from(value))),
                    "link setting receipt unconfirmed"
                );
                if target == "bus_links" {
                    expected.bus_links[BUS_IDS
                        .iter()
                        .position(|candidate| *candidate == id)
                        .unwrap()] = value;
                } else {
                    expected.policies[POLICY_IDS
                        .iter()
                        .position(|candidate| *candidate == id)
                        .unwrap()] = value;
                }
                anyhow::ensure!(
                    drv.read_link_settings().await? == Some(expected),
                    "link setting recall readback changed"
                );
                check()?;
            }
            anyhow::ensure!(
                expected == prepared.wanted,
                "link settings recall incomplete"
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        operation.map_err(|e| {
            anyhow::anyhow!(
                "Snapshot link settings recall incomplete; mixer may be partly changed: {e}"
            )
        })
    }

    pub(super) async fn verify_scene_link_settings(
        &self,
        prepared: Option<&PreparedLinks>,
        driver: &str,
        check: Check<'_>,
    ) -> anyhow::Result<()> {
        if let Some(prepared) = prepared {
            check()?;
            let actual = self.drv(driver).read_link_settings().await.map_err(|e| {
                anyhow::anyhow!(
                    "link settings final readback failed; mixer may be partly changed: {e}"
                )
            })?;
            check()?;
            anyhow::ensure!(
                actual == Some(prepared.wanted),
                "link settings final readback changed; mixer may be partly changed"
            );
        }
        Ok(())
    }
}
