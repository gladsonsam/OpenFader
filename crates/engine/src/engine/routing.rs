//! Routing operations.

use super::*;

/// Display-reader input snapshot: (strip, target) -> (label, value).
type InputBulkMap = std::collections::HashMap<(u16, String), (String, Value)>;

impl Engine {
    pub(super) async fn patch_input_guarded(
        &self,
        strip: u16,
        target: &str,
        source: Option<(&str, u8)>,
        context: &Option<(String, u64, String, Option<Value>)>,
        writer: &str,
    ) -> anyhow::Result<crate::drivers::RoutingPatch> {
        let (driver, generation, _, guard) = context
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("input route requires a current routing selection"))?;
        let check = |members: &[StripId]| -> anyhow::Result<()> {
            anyhow::ensure!(
                self.active_driver() == *driver
                    && self.connection_gen.load(Ordering::Acquire) == *generation
                    && !self.reads_suspended.load(Ordering::Acquire),
                "routing session changed"
            );
            let policy = self.policy_of(writer);
            for member in members {
                policy.allow_strip(*member)?;
                if let Some(holder) = self.lock_holder(member.0) {
                    anyhow::ensure!(holder == writer, "strip {} locked by {}", member.0, holder);
                }
            }
            Ok(())
        };
        let patch = self
            .active_drv()
            .patch_target_guarded(StripId(strip), target, source, guard.as_ref(), &check)
            .await?;
        self.apply_inbound(strip, "_preamp_routes_dirty", &ParamValue::Bool(true));
        Ok(patch)
    }
    /// Sample native revisions on the existing shell/control cadence. No new ticker.
    pub(super) fn output_routing_fingerprint(&self, driver: &str) -> Value {
        let drv = self.drv(driver);
        json!(drv
            .routing_planes()
            .into_iter()
            .flat_map(|p| p.destinations)
            .filter(|d| d.output)
            .map(|d| {
                let state = drv.output_route(&d.id).ok().map(|s| route_guard(&s));
                json!([d.id, state])
            })
            .collect::<Vec<_>>())
    }

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
        anyhow::ensure!(!dest.output, "output requires guarded stable command");
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
        context: &Option<(String, u64, String, Option<Value>)>,
        current: &Value,
    ) -> anyhow::Result<()> {
        if let Some((driver, epoch, expected, _)) = context {
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
            .find(|d| {
                !d.output && d.strip == strip && d.target == drv.canonical_routing_target(target)
            })
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
        let mut destinations: Vec<PatchEntry> = Vec::new();
        let input_bulk = if catalog
            .iter()
            .flat_map(|p| &p.destinations)
            .any(|d| !d.output)
        {
            self.drv(&driver).input_routes_bulk().await
        } else {
            None
        };
        for r in self.routing_snapshot_on(&driver, input_bulk.as_ref()).await {
            let drv = self.drv(&driver);
            let guard = if drv.input_patch_requires_guard() {
                input_bulk
                    .as_ref()
                    .and_then(|m| m.get(&(r.strip, r.target.clone())))
                    .map(|(_, guard)| guard.clone())
            } else {
                None
            };
            if drv.input_patch_requires_guard()
                && !guard.as_ref().is_some_and(|guard| {
                    drv.input_route_guard_matches(&r.target, &r.group, r.index, guard)
                })
            {
                continue;
            }
            destinations.push(PatchEntry {
                guard,
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
            });
        }
        for dest in catalog
            .iter()
            .flat_map(|p| &p.destinations)
            .filter(|d| d.output)
        {
            let Ok(state) = self.drv(&driver).output_route(&dest.id) else {
                continue;
            };
            let (group, index) = if state.source == "—" {
                (String::new(), 0)
            } else if let Some(source) = parse_source(&state.source) {
                source
            } else {
                continue;
            };
            destinations.push(PatchEntry {
                guard: Some(route_guard(&state)),
                destination: Some(dest.id.clone()),
                authority: Some("verified".into()),
                target: dest.target.clone(),
                strip: dest.strip,
                key: Some(dest.key.clone()),
                group,
                index,
                label: state.source,
            });
        }
        let patches = destinations
            .iter()
            // Preserve legacy absence-as-unpatched behavior for single-target clients.
            .filter(|r| {
                r.guard.is_none() && (!r.target.is_empty() || !r.group.is_empty() || r.index != 0)
            })
            .cloned()
            .collect();
        Event::Routing {
            patches,
            destinations,
        }
    }

    /// Best-effort input rows for display frames. Durable captures use the
    /// strict reader below, including output ports under stable destination IDs.
    pub(super) async fn routing_snapshot_on(
        &self,
        driver: &str,
        input_bulk: Option<&InputBulkMap>,
    ) -> Vec<StoredRoute> {
        let drv = self.drv(driver);
        let planes = drv.routing_planes();
        if planes.is_empty() {
            return Vec::new();
        }
        // The executable provider contract owns the destination universe; input
        // topology is only used by the compatibility adapter, never by this reader.
        let bulk = drv.source_bulk();
        let mut out = Vec::new();
        for dest in planes
            .into_iter()
            .flat_map(|p| p.destinations)
            .filter(|d| !d.output)
        {
            let label = if let Some(input_bulk) = input_bulk {
                input_bulk
                    .get(&(dest.strip, dest.target.clone()))
                    .map(|(label, _)| label.clone())
            } else if dest.target.is_empty() {
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

    /// Read every advertised route on the pinned provider. Output IDs are saved
    /// in `target` with no strip key; recall obtains a fresh provider guard.
    pub(super) async fn capture_routing_on(
        &self,
        driver: &str,
    ) -> anyhow::Result<Vec<StoredRoute>> {
        let drv = self.drv(driver);
        let planes = drv.routing_planes();
        if planes.is_empty() {
            return Ok(Vec::new());
        }
        // The executable provider contract owns the destination universe; input
        // topology is only used by the compatibility adapter, never by this reader.
        let bulk = drv.source_bulk();
        let input_bulk = if planes
            .iter()
            .flat_map(|p| &p.destinations)
            .any(|d| !d.output)
        {
            drv.input_routes_bulk().await
        } else {
            None
        };
        let mut out = Vec::new();
        for dest in planes.into_iter().flat_map(|p| p.destinations) {
            let label = if dest.output {
                Some(drv.output_route(&dest.id)?.source)
            } else if let Some(input_bulk) = &input_bulk {
                input_bulk
                    .get(&(dest.strip, dest.target.clone()))
                    .map(|(label, _)| label.clone())
            } else if dest.target.is_empty() {
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
            let label = label.ok_or_else(|| {
                anyhow::anyhow!("snapshot capture incomplete: route {} unavailable", dest.id)
            })?;
            let route = if label == "—" {
                Some((String::new(), 0))
            } else {
                parse_source(&label)
            };
            let (group, index) = route.ok_or_else(|| {
                anyhow::anyhow!(
                    "snapshot capture incomplete: route {} source {label:?} unsupported",
                    dest.id
                )
            })?;
            out.push(StoredRoute {
                target: if dest.output { dest.id } else { dest.target },
                strip: dest.strip,
                key: if dest.output { None } else { Some(dest.key) },
                group,
                index,
            });
        }
        Ok(out)
    }

    /// Recall routing snapshot rows (routing-capable drivers only;
    /// skipped elsewhere). Rows resolve their stable key first (desk-switch
    /// survival) and fall back to the positional strip.
    pub(super) async fn recall_routing(
        &self,
        routing: &[StoredRoute],
        writer: &str,
        out: &mut Vec<Event>,
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        let drv = self.active_drv();
        if drv.routing_planes().is_empty() {
            return Ok(());
        }
        let driver = self.active_driver();
        let topo = self.driver_topology(&driver);
        for row in routing {
            check()?;
            let output_destination = drv
                .routing_planes()
                .into_iter()
                .flat_map(|p| p.destinations)
                .find(|d| d.output && d.id == row.target);
            if let Some(dest) = output_destination {
                let source = if row.group.is_empty() && row.index == 0 {
                    anyhow::ensure!(dest.clearable, "snapshot output cannot be cleared");
                    "—".to_string()
                } else {
                    anyhow::ensure!(
                        dest.source_groups.iter().any(|g| g.key == row.group
                            && row.index > 0
                            && usize::from(row.index) <= g.count),
                        "snapshot output source unavailable"
                    );
                    format!("{} {}", row.group, row.index)
                };
                let state = drv.output_route(&row.target)?;
                drv.set_output_route(&row.target, &source, &state, &|| check().is_ok())
                    .await?;
                continue;
            }
            anyhow::ensure!(
                !row.target.starts_with("output/") && !row.target.starts_with("physical/"),
                "snapshot output {} unavailable",
                row.target
            );
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

/// Exact provider snapshot serialized for the client roundtrip.
pub(super) fn route_guard(state: &openfader_core::routing::RouteState) -> Value {
    json!({ "source": state.source, "revision": state.revision, "generation": state.generation })
}
