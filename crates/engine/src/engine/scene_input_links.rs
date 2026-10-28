//! XR18 shared input-link preparation and verified recall.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct InputLink {
    pair: [u16; 2],
    wanted: bool,
}

impl Engine {
    pub(super) async fn prepare_scene_input_links(
        &self,
        scene: &StoredScene,
        scope: Option<&RecallScope>,
        topo: &[openfader_core::StripFamily],
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<Vec<InputLink>> {
        let drv = self.drv("xair");
        if drv.param_read_only("link")
            || scope.is_some_and(|s| !s.allows_param_for(drv.as_ref(), "link"))
        {
            return Ok(Vec::new());
        }
        let resolve = |s: &StoredStrip| {
            s.key
                .as_deref()
                .and_then(|k| openfader_core::strip_index_for_key(topo, k))
                .map(|i| i as u16)
                .unwrap_or(s.strip)
        };
        let pairs = drv.stereo_pairs();
        let mut selected = BTreeSet::new();
        for s in &scene.strips {
            let index = resolve(s);
            if s.params.contains_key("link") && scope.is_none_or(|sc| sc.matches_strip(index, topo))
            {
                let pair = pairs
                    .iter()
                    .find(|p| p.contains(&index))
                    .ok_or_else(|| anyhow::anyhow!("snapshot link target has no input pair"))?;
                selected.insert(*pair);
            }
        }
        let mut result = Vec::new();
        for pair in selected {
            anyhow::ensure!(
                pair[0] != pair[1]
                    && pair.iter().all(|i| topo
                        .iter()
                        .any(|f| f.kind == openfader_core::StripKind::In
                            && usize::from(*i) >= f.first
                            && usize::from(*i) < f.first + f.count)),
                "invalid snapshot input link pair"
            );
            let mut wanted = None;
            for row in &scene.strips {
                if pair.contains(&resolve(row)) {
                    if let Some(value) = row.params.get("link") {
                        let value = value.as_bool().ok_or_else(|| {
                            anyhow::anyhow!("snapshot input link must be boolean")
                        })?;
                        anyhow::ensure!(
                            wanted.is_none_or(|v| v == value),
                            "conflicting snapshot input link values"
                        );
                        wanted = Some(value);
                    }
                }
            }
            if let Some(wanted) = wanted {
                self.read_scene_input_link(pair, check).await?;
                result.push(InputLink { pair, wanted });
            }
        }
        Ok(result)
    }

    pub(super) async fn read_scene_input_link(
        &self,
        pair: [u16; 2],
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<bool> {
        let mut state = None;
        for member in pair {
            check()?;
            let value = self.drv("xair").get_param(StripId(member), "link").await?;
            check()?;
            let ParamValue::Bool(value) = value else {
                anyhow::bail!("snapshot input link unavailable");
            };
            anyhow::ensure!(
                state.is_none_or(|v| v == value),
                "snapshot input link members disagree"
            );
            state = Some(value);
        }
        Ok(state.unwrap())
    }

    pub(super) async fn restore_scene_input_links(
        &self,
        links: &[InputLink],
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
        affected: &std::sync::Mutex<BTreeSet<u16>>,
        writer: &str,
        topo: &[openfader_core::StripFamily],
        out: &mut Vec<Event>,
    ) -> anyhow::Result<()> {
        for link in links {
            let operation = async {
                if self.read_scene_input_link(link.pair, check).await? != link.wanted {
                    check()?;
                    self.drv("xair")
                        .set_param_guarded_checked(
                            StripId(link.pair[0]),
                            "link",
                            ParamValue::Bool(link.wanted),
                            &BTreeMap::new(),
                            &|members| {
                                check()?;
                                let actual: BTreeSet<_> = members.iter().map(|m| m.0).collect();
                                anyhow::ensure!(
                                    members.len() == 2 && actual == BTreeSet::from(link.pair),
                                    "snapshot input link members changed"
                                );
                                affected.lock().unwrap().extend(actual);
                                Ok(())
                            },
                        )
                        .await?;
                    check()?;
                }
                let mut verified = None;
                for attempt in 0..5 {
                    check()?;
                    let query_started = Instant::now();
                    if self.read_scene_input_link(link.pair, check).await.ok() == Some(link.wanted)
                    {
                        verified = Some(query_started);
                        break;
                    }
                    check()?;
                    if attempt < 4 {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        check()?;
                    }
                }
                let verified = verified
                    .ok_or_else(|| anyhow::anyhow!("snapshot input link readback failed"))?;
                for member in link.pair {
                    // A conservative query-start stamp prevents old-session
                    // proof being inserted after a concurrent SyncEpoch.
                    self.apply_received(
                        member,
                        "link",
                        &ParamValue::Bool(link.wanted),
                        verified,
                        Some(verified),
                    );
                    out.push(Event::Param {
                        ch: member,
                        field: "link".into(),
                        value: json!(link.wanted),
                        by: Some(writer.to_string()),
                        key: openfader_core::strip_key_for_index(topo, member as usize),
                        authority: Some("verified".into()),
                    });
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            operation.map_err(|e| anyhow::anyhow!("{e}; mixer may be partly changed"))?;
        }
        Ok(())
    }

    pub(super) async fn verify_scene_input_links(
        &self,
        links: &[InputLink],
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        for link in links {
            let state = self
                .read_scene_input_link(link.pair, check)
                .await
                .map_err(|e| anyhow::anyhow!("{e}; mixer may be partly changed"))?;
            anyhow::ensure!(
                state == link.wanted,
                "snapshot input link final readback changed; mixer may be partly changed"
            );
        }
        Ok(())
    }
}
