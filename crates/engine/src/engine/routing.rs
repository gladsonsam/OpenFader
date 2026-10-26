//! Routing operations.

use super::*;

impl Engine {
    /// True when the FX cache needs no fill: design racks are in-memory
    /// complete and rack-less rows are honest placeholders; only a numbered
    /// row with a known type but a short param set wants the worker.
    pub(super) fn fx_cache_complete(&self) -> bool {
        if self.active_drv().fx_flavor() != FxFlavor::Numbered {
            return true;
        }
        self.fx_cache.read().unwrap().iter().all(|row| {
            match self.active_drv().fx_type_index_of(&row.fx_type) {
                None => true,
                Some(index) => {
                    let used = self.active_drv().fx_type_param_count(index).unwrap_or(0);
                    row.params.len() >= used
                }
            }
        })
    }

    pub(super) async fn project_param_dependents(
        &self,
        strip: u16,
        field: &str,
        writer: &str,
        out: &mut Vec<Event>,
    ) {
        let drv = self.active_drv();
        for id in drv.param_dependents(field) {
            self.evict_param_unless_fresh(StripId(strip), id, Instant::now());
            if let Ok(value) = drv.get_param_display(StripId(strip), id).await {
                out.push(Event::Param {
                    ch: strip,
                    field: id.to_string(),
                    value: param_value_json(&value),
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(strip),
                    authority: Some("accepted".to_string()),
                });
            }
        }
    }

    /// Share the parameter projection for patch and clear writes without
    /// promoting an acknowledged cache word into mixer verification.
    pub(super) async fn project_routing_patch(
        &self,
        strip: u16,
        writer: &str,
        patch: &crate::drivers::RoutingPatch,
        out: &mut Vec<Event>,
    ) {
        if let Some((field, written)) = &patch.param {
            let written = param_value_json(written);
            let refreshed = self.refresh_param(StripId(strip), field, &written).await;
            let (value, authority) = match refreshed {
                Some(v) => (v, "verified"),
                None => (written, "accepted"),
            };
            out.push(Event::Param {
                ch: strip,
                field: field.to_string(),
                value,
                by: Some(writer.to_string()),
                key: self.active_strip_key(strip),
                authority: Some(authority.to_string()),
            });
            self.project_param_dependents(strip, field, writer, out)
                .await;
        }
    }

    /// Current routing display: one row per input and named patch target.
    /// Providers may retain accepted-write continuity; this is not verified
    /// parameter evidence (that remains in the provenance-aware param plane).
    pub(super) async fn routing_message(&self) -> Event {
        Event::Routing {
            patches: self
                .routing_snapshot()
                .await
                .into_iter()
                // Legacy single-target clients use absence to mean unpatched.
                // Named targets retain an explicit row so A and B stay distinct.
                .filter(|r| !r.target.is_empty() || !r.group.is_empty() || r.index != 0)
                .map(|r| PatchEntry {
                    target: r.target,
                    strip: r.strip,
                    key: r.key.clone(),
                    label: if r.group.is_empty() && r.index == 0 {
                        "—".to_string()
                    } else {
                        format!("{} {:02}", r.group, r.index)
                    },
                    group: r.group,
                    index: r.index,
                })
                .collect(),
        }
    }

    /// (strip + stable key, group, 1-based index) rows for patched inputs.
    pub(super) async fn routing_snapshot(&self) -> Vec<StoredRoute> {
        let driver = self.active_driver();
        self.routing_snapshot_on(&driver).await
    }

    /// Pinned routing snapshot (r58 P2(2)): same rows as
    /// [`Engine::routing_snapshot`] but read from `driver`, so a numbered
    /// save cannot mix one desk's strips with another's routing.
    pub(super) async fn routing_snapshot_on(&self, driver: &str) -> Vec<StoredRoute> {
        let drv = self.drv(driver);
        if drv.routing_groups().is_empty() {
            return Vec::new();
        }
        let topo = drv.topology();
        // Bulk `source` plane when the driver serves it (one lock instead
        // of a per-strip `get_param` await); same rows in the same order.
        let bulk = drv.source_bulk();
        let mut out = Vec::new();
        for fam in &topo {
            if fam.kind != openfader_core::StripKind::In {
                continue;
            }
            for s in fam.first..fam.first + fam.count {
                for target in drv.routing_targets() {
                    let label = if target.key.is_empty() {
                        match &bulk {
                            Some(map) => map.get(&(s as u16)).cloned(),
                            None => drv.routing_source(StripId(s as u16), target.key).await.ok(),
                        }
                    } else {
                        drv.routing_source(StripId(s as u16), target.key).await.ok()
                    };
                    let Some(label) = label else { continue };
                    let route = if label == "—" {
                        Some((String::new(), 0))
                    } else {
                        parse_source(&label)
                    };
                    if let Some((group, index)) = route {
                        out.push(StoredRoute {
                            target: target.key.to_string(),
                            strip: s as u16,
                            key: openfader_core::strip_key_for_index(&topo, s),
                            group,
                            index,
                        });
                    }
                }
            }
        }
        out
    }

    /// Recall routing snapshot rows (routing-capable drivers only;
    /// skipped elsewhere). Rows resolve their stable key first (desk-switch
    /// survival) and fall back to the positional strip.
    pub(super) async fn recall_routing(
        &self,
        routing: &[StoredRoute],
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<()> {
        let drv = self.active_drv();
        if drv.routing_groups().is_empty() {
            return Ok(());
        }
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        for row in routing {
            let strip = row
                .key
                .as_deref()
                .and_then(|k| openfader_core::strip_index_for_key(&topo, k))
                .map(|i| i as u16)
                .unwrap_or(row.strip);
            let patch = if row.group.is_empty() && row.index == 0 {
                drv.patch_target_clear(StripId(strip), &row.target).await?
            } else {
                drv.patch_target_set(StripId(strip), &row.target, &row.group, row.index)
                    .await?
            };
            self.project_routing_patch(strip, writer, &patch, out).await;
        }
        out.push(self.routing_message().await);
        Ok(())
    }
}

/// Parse a patch label (`Lcl 01`, `Net 12`, `USB 03`) into (group, 1-based
/// index). The design's unpatched tile (` `) and anything else → `None`.
pub(super) fn parse_source(label: &str) -> Option<(String, u8)> {
    let (group, num) = label.split_once(' ')?;
    if group.is_empty() || group.len() > 64 {
        return None;
    }
    let index: u8 = num.parse().ok()?;
    if index > 0 {
        Some((group.to_string(), index))
    } else {
        None
    }
}
