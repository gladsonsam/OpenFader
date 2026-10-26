//! Routing operations.

use super::*;

impl Engine {
    /// Resolve only provider-owned stable IDs. Positional routing remains a
    /// compatibility write adapter, never the source of advertised destinations.
    pub(super) fn routing_command(
        &self,
        driver: &str,
        epoch: u64,
        destination: &str,
        source: Option<crate::protocol::RoutingSource>,
    ) -> anyhow::Result<Cmd> {
        anyhow::ensure!(
            driver == self.active_driver() && epoch == self.connection_gen.load(Ordering::Acquire),
            "routing session changed"
        );
        let dest = self
            .drv(driver)
            .routing_planes()
            .into_iter()
            .flat_map(|p| p.destinations)
            .find(|d| d.id == destination)
            .ok_or_else(|| anyhow::anyhow!("unknown routing destination"))?;
        Ok(if let Some(source) = source {
            // The shared write guard checks the destination-specific allowlist.
            Cmd::PatchSet {
                strip: dest.strip,
                key: Some(dest.key),
                target: dest.target,
                group: source.group,
                index: source.index,
            }
        } else {
            Cmd::PatchClear {
                strip: dest.strip,
                key: Some(dest.key),
                target: dest.target,
            }
        })
    }

    pub(super) fn validate_routing_context(
        &self,
        context: &Option<(String, u64, String)>,
        current: &Value,
    ) -> anyhow::Result<()> {
        if let Some((driver, epoch, expected)) = context {
            anyhow::ensure!(
                *driver == self.active_driver()
                    && *epoch == self.connection_gen.load(Ordering::Acquire),
                "routing session changed"
            );
            let current = current
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("routing state unknown"))?;
            anyhow::ensure!(
                current == expected
                    || (parse_source(current).is_some()
                        && parse_source(current) == parse_source(expected)),
                "routing source changed; select the destination again"
            );
        }
        Ok(())
    }

    /// Validate legacy and identity-addressed writes against the same provider contract.
    pub(super) async fn guard_routing_write(
        &self,
        strip: u16,
        target: &str,
        source: Option<(&str, u8)>,
        policy: &Policy,
        writer: &str,
    ) -> anyhow::Result<()> {
        let drv = self.active_drv();
        let dest = drv
            .routing_planes()
            .into_iter()
            .flat_map(|p| p.destinations)
            .find(|d| d.strip == strip && d.target == target)
            .ok_or_else(|| {
                anyhow::anyhow!("legacy patch requires advertised input strips and targets")
            })?;
        if let Some((group, index)) = source {
            let allowed = dest
                .source_groups
                .iter()
                .find(|g| g.key == group)
                .ok_or_else(|| {
                    anyhow::anyhow!("unknown patch group {group:?} for routing destination")
                })?;
            anyhow::ensure!(
                index > 0 && usize::from(index) <= allowed.count,
                "patch index {index} out of range 1..={}",
                allowed.count
            );
        } else {
            anyhow::ensure!(dest.clearable, "destination cannot be cleared");
        }
        for member in drv
            .control_members(StripId(strip), drv.routing_control(target))
            .await?
        {
            policy.allow_strip(member)?;
            if let Some(holder) = self.lock_holder(member.0) {
                anyhow::ensure!(holder == writer, "strip {} locked by {}", member.0, holder);
            }
        }
        Ok(())
    }

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
        let driver = self.active_driver();
        let catalog = self.drv(&driver).routing_planes();
        let destinations: Vec<PatchEntry> = self
            .routing_snapshot_on(&driver)
            .await
            .into_iter()
            .map(|r| PatchEntry {
                destination: catalog
                    .iter()
                    .flat_map(|p| &p.destinations)
                    .find(|d| d.strip == r.strip && d.target == r.target)
                    .map(|d| d.id.clone()),
                // Provider display may include accepted writes. Never label it verified.
                authority: Some("provider_display".to_string()),
                target: r.target,
                strip: r.strip,
                key: r.key,
                label: if r.group.is_empty() && r.index == 0 {
                    "—".to_string()
                } else {
                    format!("{} {:02}", r.group, r.index)
                },
                group: r.group,
                index: r.index,
            })
            .collect();
        let patches = destinations
            .iter()
            // Preserve legacy absence-as-unpatched behavior for single-target clients.
            .filter(|r| !r.target.is_empty() || !r.group.is_empty() || r.index != 0)
            .cloned()
            .collect();
        Event::Routing {
            patches,
            destinations,
        }
    }

    /// Read provider destinations on one pinned driver, so scene saves and
    /// routing frames cannot mix one desk's catalog with another's source state.
    pub(super) async fn routing_snapshot_on(&self, driver: &str) -> Vec<StoredRoute> {
        let drv = self.drv(driver);
        let planes = drv.routing_planes();
        if planes.is_empty() {
            return Vec::new();
        }
        // The executable provider contract owns the destination universe; input
        // topology is only used by the compatibility adapter, never by this reader.
        let bulk = drv.source_bulk();
        let mut out = Vec::new();
        for dest in planes.into_iter().flat_map(|p| p.destinations) {
            let label = if dest.target.is_empty() {
                match &bulk {
                    Some(map) => map.get(&dest.strip).cloned(),
                    None => drv
                        .routing_source(StripId(dest.strip), &dest.target)
                        .await
                        .ok(),
                }
            } else {
                drv.routing_source(StripId(dest.strip), &dest.target)
                    .await
                    .ok()
            };
            let Some(label) = label else { continue };
            let route = if label == "—" {
                Some((String::new(), 0))
            } else {
                parse_source(&label)
            };
            if let Some((group, index)) = route {
                out.push(StoredRoute {
                    target: dest.target,
                    strip: dest.strip,
                    key: Some(dest.key),
                    group,
                    index,
                });
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
        if drv.routing_planes().is_empty() {
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
