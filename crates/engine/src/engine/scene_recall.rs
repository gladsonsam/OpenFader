//! Scene recall operations.

use super::*;

impl Engine {
    /// Sim reads prove the applied model value. Hardware recalls remain write-ACK
    /// only: discard pre-recall proof without adding thousands of wire queries.
    async fn reconcile_scene_write(&self, strip: StripId, id: &str, value: &Value) {
        if self.active_drv().is_hardware() {
            self.evict_param_unless_fresh(strip, id, Instant::now());
        } else {
            self.refresh_param(strip, id, value).await;
        }
    }

    fn check_scene_recall(
        &self,
        driver: &str,
        generation: u64,
        writer: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.active_driver() == driver
                && self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "snapshot recall interrupted: desk session changed; mixer may be partly changed"
        );
        self.policy_of(writer).allow_recall()
    }

    /// Pinned full-plane capture (r58 P2(2)): every driver read binds to
    /// `pin_driver` + `generation` — strips, routing and main alike — so a
    /// mid-capture connect cannot commit a mixed document. Per-strip
    /// fail-fast plus the commit-time barrier re-validation (see
    /// [`Engine::commit_with_capture_guard`]).
    pub(super) async fn capture_full_on(
        &self,
        number: u16,
        name: &str,
        pin_driver: &str,
        generation: u64,
    ) -> anyhow::Result<StoredScene> {
        let driver = pin_driver.to_string();
        let topo = self.driver_topology(&driver);
        let caps = self.driver_capabilities(&driver);
        let reg = self.registry(&driver);
        let mut strips = Vec::new();
        for fam in &topo {
            for s in fam.first..fam.first + fam.count {
                let strip = StripId(s as u16);
                // Capture reads (see `capture_read_param`): verified proof
                // first, honest display continuity for fader/mute/name right
                // after local moves. An unreadable advertised value rejects
                // the candidate instead of committing a partial snapshot.
                let fader = self.capture_read_param(&driver, strip, "fader").await;
                let mute = self.capture_read_param(&driver, strip, "mute").await;
                let name_v = self.capture_read_param(&driver, strip, "name").await;
                anyhow::ensure!(
                    matches!(fader, Some(ParamValue::Float(_)))
                        && matches!(mute, Some(ParamValue::Bool(_)))
                        && matches!(name_v, Some(ParamValue::Text(_))),
                    "snapshot capture incomplete: strip {s} level, mute or name unavailable"
                );
                let mut params = HashMap::new();
                let mut captured_context = std::collections::BTreeSet::new();
                for id in &caps {
                    if *id == "fader"
                        || *id == "mute"
                        || *id == "name"
                        || self.drv(&driver).param_read_only(id)
                    {
                        continue;
                    }
                    let scoped = reg
                        .find(id)
                        .map(|def| params::supports_kind(def, fam.kind))
                        .unwrap_or(true);
                    if !scoped {
                        continue;
                    }
                    let drv = self.drv(&driver);
                    for context in drv.param_write_context(id) {
                        captured_context.insert(*context);
                        if !params.contains_key(*context) {
                            let value = self.capture_read_param(&driver, strip, context).await.ok_or_else(|| {
                                anyhow::anyhow!("snapshot capture incomplete: strip {s} {context} unavailable")
                            })?;
                            params.insert(context.to_string(), param_value_json(&value));
                        }
                    }
                    if !drv.capture_param_applicable(strip, id).await? {
                        continue;
                    }
                    let captured = match self.drv(&driver).capture_param_override(strip, id).await {
                        Some(result) => Some(result.map_err(|error| {
                            anyhow::anyhow!("snapshot capture incomplete: strip {s} {id}: {error}")
                        })?),
                        None => self.capture_read_param(&driver, strip, id).await,
                    };
                    let pv = captured.ok_or_else(|| {
                        anyhow::anyhow!("snapshot capture incomplete: strip {s} {id} unavailable")
                    })?;
                    params.insert(id.to_string(), param_value_json(&pv));
                }
                for context in captured_context {
                    let current = self
                        .capture_read_param(&driver, strip, context)
                        .await
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "snapshot capture incomplete: strip {s} {context} unavailable"
                            )
                        })?;
                    anyhow::ensure!(
                        params.get(context) == Some(&param_value_json(&current)),
                        "snapshot capture incomplete: strip {s} {context} changed during capture"
                    );
                }
                // Legacy documents use an empty name to mean no name capture.
                // An explicit parameter preserves a deliberately blank name.
                if matches!(&name_v, Some(ParamValue::Text(name)) if name.is_empty()) {
                    params.insert("name".to_string(), json!(""));
                }
                strips.push(StoredStrip {
                    strip: s as u16,
                    key: openfader_core::strip_key_for_index(&topo, s),
                    name: match name_v {
                        Some(ParamValue::Text(n)) => n,
                        _ => format!("{}{}", fam.name_prefix, s - fam.first + 1),
                    },
                    fader: match fader {
                        Some(ParamValue::Float(v)) => v,
                        _ => 0.0,
                    },
                    mute: matches!(mute, Some(ParamValue::Bool(true))),
                    params,
                });
                // r58: fail fast when the desk changes mid-capture (the
                // commit barrier re-validates before publish in any case).
                anyhow::ensure!(
                    self.connection_gen.load(Ordering::Acquire) == generation,
                    "desk changed during scene capture"
                );
                anyhow::ensure!(
                    !self.reads_suspended.load(Ordering::Acquire),
                    "desk session changed during scene capture (reads suspended)"
                );
            }
        }
        // Capture from the pinned provider, never the asynchronous display cache.
        let drv = self.drv(&driver);
        let fx = match drv.fx_flavor() {
            FxFlavor::Design => drv
                .fx_snapshot()
                .iter()
                .enumerate()
                .map(|(i, row)| StoredFx {
                    slot: (i + 1) as u8,
                    fx_type: row.fx_type.clone(),
                    on: row.on,
                    bypass: row.bypass,
                    params: fx::param_vec_to_map(&row.fx_type, &row.params),
                    insert: None,
                })
                .collect(),
            FxFlavor::Numbered => {
                let mut rows = Vec::new();
                for slot in 1..=drv.fx_slot_count() as u8 {
                    let effect = drv.fx_type_index(slot).await.ok_or_else(|| {
                        anyhow::anyhow!("snapshot capture incomplete: FX {slot} type unavailable")
                    })?;
                    let fx_type = drv
                        .fx_type_name(effect)
                        .ok_or_else(|| {
                            anyhow::anyhow!("snapshot capture incomplete: FX {slot} unknown type")
                        })?
                        .to_string();
                    let count = drv.fx_type_param_count(effect).ok_or_else(|| {
                        anyhow::anyhow!(
                            "snapshot capture incomplete: FX {slot} parameter count unavailable"
                        )
                    })?;
                    let mut params = BTreeMap::new();
                    for index in 0..count {
                        let value = drv
                            .get_fx_param_with_type(slot, index as u8, Some(effect))
                            .await
                            .map_err(|error| {
                                anyhow::anyhow!(
                                    "snapshot capture incomplete: FX {slot} {fx_type} parameter {}: {error}",
                                    index + 1
                                )
                            })?;
                        params.insert(format!("par{:02}", index + 1), value);
                    }
                    anyhow::ensure!(
                        self.connection_gen.load(Ordering::Acquire) == generation
                            && !self.reads_suspended.load(Ordering::Acquire),
                        "desk changed during snapshot capture"
                    );
                    let insert = if drv.fx_insert_supported() {
                        let enabled = drv.get_fx_insert(slot).await.map_err(|error| {
                            anyhow::anyhow!(
                                "snapshot capture incomplete: FX {slot} insert: {error}"
                            )
                        })?;
                        anyhow::ensure!(
                            self.connection_gen.load(Ordering::Acquire) == generation
                                && !self.reads_suspended.load(Ordering::Acquire),
                            "desk changed during snapshot capture"
                        );
                        Some(StoredFxInsert {
                            provider: driver.clone(),
                            enabled,
                        })
                    } else {
                        None
                    };
                    anyhow::ensure!(
                        drv.fx_type_index(slot).await == Some(effect),
                        "snapshot capture incomplete: FX {slot} type changed or became unavailable"
                    );
                    anyhow::ensure!(
                        self.connection_gen.load(Ordering::Acquire) == generation
                            && !self.reads_suspended.load(Ordering::Acquire),
                        "desk changed during snapshot capture"
                    );
                    // Numbered racks advertise type/params, with no on/bypass controls.
                    rows.push(StoredFx {
                        slot,
                        fx_type,
                        on: true,
                        bypass: false,
                        params,
                        insert,
                    });
                }
                rows
            }
            _ => Vec::new(),
        };
        let output_taps = drv.read_output_taps().await?;
        anyhow::ensure!(
            self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "desk changed during snapshot capture"
        );
        let routing = self.capture_routing_on(&driver).await?;
        anyhow::ensure!(
            self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "desk changed during snapshot capture"
        );
        if let Some(values) = &output_taps {
            validate_scene_output_taps(values, values, &routing, drv.as_ref())?;
        }
        let output_taps = output_taps.map(|values| StoredOutputTaps {
            provider: driver.clone(),
            values,
        });
        if driver == "xair" {
            super::routing::xair_scene_input_routes(&routing, &strips, &topo)?;
        }
        let mute_groups = if drv.mute_group_snapshot_supported() {
            let mut masters = Vec::new();
            for group in 0..drv.mute_group_count() {
                let muted = drv.get_mute_group_verified(group).await.map_err(|error| {
                    anyhow::anyhow!(
                        "snapshot capture incomplete: mute group {}: {error}",
                        group + 1
                    )
                })?;
                anyhow::ensure!(
                    self.connection_gen.load(Ordering::Acquire) == generation
                        && !self.reads_suspended.load(Ordering::Acquire),
                    "desk changed during snapshot capture"
                );
                masters.push(muted);
            }
            let mut memberships = Vec::new();
            for family in &topo {
                if !drv.mute_group_snapshot_applies(family.kind) {
                    continue;
                }
                for index in family.first..family.first + family.count {
                    let strip = StripId(index as u16);
                    let mask = drv.snapshot_mute_group_mask(strip).await.map_err(|error| {
                        anyhow::anyhow!("snapshot capture incomplete: strip {index} mute-group membership: {error}")
                    })?;
                    anyhow::ensure!(
                        self.connection_gen.load(Ordering::Acquire) == generation
                            && !self.reads_suspended.load(Ordering::Acquire),
                        "desk changed during snapshot capture"
                    );
                    let key =
                        openfader_core::strip_key_for_index(&topo, index).ok_or_else(|| {
                            anyhow::anyhow!(
                                "snapshot capture incomplete: strip {index} has no stable key"
                            )
                        })?;
                    memberships.push(StoredMuteGroupMembership {
                        strip: strip.0,
                        key,
                        mask,
                    });
                }
            }
            Some(StoredMuteGroups {
                provider: driver.clone(),
                masters,
                memberships,
            })
        } else {
            None
        };
        let rta = drv
            .read_rta_settings()
            .await?
            .map(|block| stored_rta_from_controls(&driver, &block))
            .transpose()?;
        let monitor = drv
            .read_monitor_settings()
            .await?
            .map(|block| stored_monitor_from_controls(&driver, &block))
            .transpose()?;
        let automix = drv
            .read_automix_settings()
            .await?
            .map(|values| StoredAutomix {
                provider: driver.clone(),
                values,
            });
        anyhow::ensure!(
            self.connection_gen.load(Ordering::Acquire) == generation
                && !self.reads_suspended.load(Ordering::Acquire),
            "desk changed during snapshot capture"
        );
        let main_fader = self.main_fader_on(&driver).await;
        anyhow::ensure!(
            main_fader.is_some()
                || !topo
                    .iter()
                    .any(|f| f.kind == openfader_core::StripKind::Main),
            "snapshot capture incomplete: main level unavailable"
        );
        Ok(StoredScene {
            number,
            name: name.to_string(),
            saved_at: now_unix(),
            main_fader,
            strips,
            fx,
            routing,
            mute_groups,
            automix,
            output_taps,
            monitor,
            rta,
        })
    }

    /// Recall a full-plane scene: faders ride the shared fade
    /// ([`openfader_core::show::fade_strips`], same as named shows), every
    /// other param goes through `set_param` in [`safe_order_key`] order, then
    /// FX + routing + main. Locks are bypassed (show control) and reported
    /// via the returned `overrode_locks` (existing error semantics for
    /// direct writes are untouched).
    /// Scoped recall: `scope` restricts the recall to strips / param
    /// groups / globals (`None` = full plane, the historical behavior).
    /// Axes outside the scope receive no writes. Faders use the shared fade
    /// only when included; a mute-only recall applies mutes directly, and
    /// excluded mutes are suppressed in the fade adapter. Other excluded
    /// params are skipped; FX/routing/main apply only under their global.
    pub(super) async fn recall_full(
        &self,
        scene: &StoredScene,
        fade_ms: u64,
        scope: Option<&RecallScope>,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<Vec<u16>> {
        let driver = self.active_driver();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let check = || self.check_scene_recall(&driver, generation, writer);
        check()?;
        let drv = self.drv(&driver);
        let topo = self.driver_topology(&driver);
        let rta = scene
            .rta
            .as_ref()
            .filter(|r| r.provider == driver)
            .filter(|_| {
                scope.is_none_or(|sc| {
                    sc.globals
                        .as_ref()
                        .is_some_and(|globals| globals.iter().any(|g| g == "rta"))
                })
            });
        if let Some(saved) = rta {
            check()?;
            let block = drv
                .read_rta_settings()
                .await?
                .ok_or_else(|| anyhow::anyhow!("snapshot RTA settings unavailable"))?;
            check()?;
            validate_rta_recall(saved, &block)?;
        }
        let monitor = scene
            .monitor
            .as_ref()
            .filter(|m| m.provider == driver)
            .filter(|_| {
                scope.is_none_or(|sc| {
                    sc.globals
                        .as_ref()
                        .is_some_and(|globals| globals.iter().any(|g| g == "monitor"))
                })
            });
        if let Some(saved) = monitor {
            check()?;
            let block = drv
                .read_monitor_settings()
                .await?
                .ok_or_else(|| anyhow::anyhow!("snapshot monitor settings unavailable"))?;
            check()?;
            validate_monitor_recall(saved, &block)?;
        }
        let output_taps = scene
            .output_taps
            .as_ref()
            .filter(|t| t.provider == driver)
            .filter(|_| scope.is_none_or(|sc| sc.allows_global("routing")));
        if let Some(taps) = output_taps {
            check()?;
            let live = drv
                .read_output_taps()
                .await?
                .ok_or_else(|| anyhow::anyhow!("snapshot output taps unavailable"))?;
            check()?;
            validate_scene_output_taps(&taps.values, &live, &scene.routing, drv.as_ref())?;
        }
        let xair_input_routes = if driver == "xair" {
            super::routing::xair_scene_input_routes(&scene.routing, &scene.strips, &topo)?
        } else {
            None
        };
        if let Some(groups) = scene
            .mute_groups
            .as_ref()
            .filter(|groups| groups.provider == driver)
        {
            if scope.is_none_or(|sc| sc.allows_global("mute_groups") || sc.allows_group("config")) {
                validate_mute_group_snapshot(groups, &topo, drv.as_ref())?;
            }
        }
        let resolve = |s: &StoredStrip| -> u16 {
            s.key
                .as_deref()
                .and_then(|k| openfader_core::strip_index_for_key(&topo, k))
                .map(|i| i as u16)
                .unwrap_or(s.strip)
        };
        // Strips whose key resolves on this desk recall at the resolved
        // index (desk-switch survival); key-less rows and unresolvable keys
        // fall back to the positional strip (back-compat). Scope matching
        // runs on the resolved index (`strip_keys` name the same strips).
        //
        // Recall-path provenance: no per-param readback on this path
        // (best-effort fan-out on hardware), so hardware echoes are
        // `accepted` (write ACK, no value proof). The sim model applies
        // writes to real in-memory state, so its echoes are `verified`.
        let proven = !self.active_drv().is_hardware();
        let authority = || Some(if proven { "verified" } else { "accepted" }.to_string());
        let strips: Vec<(&StoredStrip, u16)> = scene
            .strips
            .iter()
            .map(|s| (s, resolve(s)))
            .filter(|(_, r)| scope.is_none_or(|sc| sc.matches_strip(*r, &topo)))
            .collect();
        // Resolve bus companions before any recall writes, including scoped
        // recalls whose linked partner lies outside the selected scope.
        let mut affected_bus = std::collections::BTreeSet::new();
        for (scene_strip, strip) in &strips {
            let is_bus = topo.iter().any(|family| {
                family.kind == openfader_core::StripKind::Bus
                    && usize::from(*strip) >= family.first
                    && usize::from(*strip) < family.first + family.count
            });
            let writes = scope.is_none_or(|sc| sc.allows_param("fader") || sc.allows_param("mute"))
                || scene_strip
                    .params
                    .keys()
                    .any(|id| scope.is_none_or(|sc| sc.allows_param_for(drv.as_ref(), id)));
            if is_bus && writes {
                affected_bus.extend(
                    drv.control_members(StripId(*strip), "fader")
                        .await?
                        .into_iter()
                        .map(|member| member.0),
                );
                check()?;
            }
        }
        for (scene_strip, _) in &strips {
            for id in scene_strip.params.keys() {
                if scope.is_none_or(|sc| sc.allows_param_for(drv.as_ref(), id)) {
                    affected_bus.extend(
                        self.send_bus_members(&driver, id)
                            .await?
                            .into_iter()
                            .map(|m| m.0),
                    );
                    check()?;
                }
            }
        }
        // XR18 preamp writes affect every channel sharing that physical
        // input, including channels outside the selected recall scope.
        // Resolve affected members before the first write so lock overrides
        // are reported for the complete preamp target.
        let mut affected_preamp = std::collections::BTreeSet::new();
        let late_affected_preamp = std::sync::Mutex::new(std::collections::BTreeSet::new());
        if scope.is_none_or(|sc| sc.allows_global("routing")) {
            if let Some(routes) = &xair_input_routes {
                for (strip, wanted) in routes {
                    for (target, desired) in [
                        ("analog", wanted.analog),
                        ("usb", wanted.usb),
                        ("bank", wanted.bank),
                    ] {
                        check()?;
                        let guard = drv
                            .input_route_guard(StripId(*strip))
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("snapshot input route unavailable"))?;
                        let current = guard
                            .get(target)
                            .and_then(Value::as_i64)
                            .ok_or_else(|| anyhow::anyhow!("snapshot input route unavailable"))?;
                        if current == i64::from(desired) {
                            continue;
                        }
                        let source = match (target, desired) {
                            ("analog", 18) => None,
                            ("analog", n @ 0..=15) => Some(("Mic", (n + 1) as u8)),
                            ("analog", n @ 16..=17) => Some(("Aux", (n - 15) as u8)),
                            ("usb", n @ 0..=17) => Some(("USB", (n + 1) as u8)),
                            ("bank", n @ 0..=1) => Some(("Bank", (n + 1) as u8)),
                            _ => anyhow::bail!("snapshot input route unavailable"),
                        };
                        drv.patch_target_guarded(
                            StripId(*strip),
                            target,
                            source,
                            Some(&guard),
                            &|members| {
                                check()?;
                                late_affected_preamp
                                    .lock()
                                    .unwrap()
                                    .extend(members.iter().map(|m| m.0));
                                Ok(())
                            },
                        )
                        .await?;
                        self.apply_inbound(*strip, "_preamp_routes_dirty", &ParamValue::Bool(true));
                    }
                }
            }
        }
        if driver == "xair" {
            for (scene_strip, strip) in &strips {
                let has_preamp = ["gain", "phantom"].iter().any(|id| {
                    scene_strip.params.contains_key(*id)
                        && scope.is_none_or(|sc| sc.allows_param_for(drv.as_ref(), id))
                });
                if has_preamp {
                    check()?;
                    for member in drv.control_members(StripId(*strip), "gain").await? {
                        affected_preamp.insert(member.0);
                    }
                }
            }
        }
        // Faders ride the single shared fade (legacy order: pre-unmute
        // targets, ramp, snap, target mutes); params/FX/routing/main follow.
        let want_fader = scope.is_none_or(|sc| sc.allows_param("fader"));
        let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
        let fade = ScopedSceneFade {
            recall_mutes: want_mute,
            driver: drv.clone(),
            check: &check,
            checked_buses: topo
                .iter()
                .filter(|family| driver == "xair" && family.kind == openfader_core::StripKind::Bus)
                .flat_map(|family| family.first..family.first + family.count)
                .map(|index| index as u16)
                .collect(),
            affected: &late_affected_preamp,
        };
        if want_fader && !strips.is_empty() {
            let targets: Vec<_> = strips
                .iter()
                .map(|(s, r)| (StripId(*r), s.fader, s.mute))
                .collect();
            openfader_core::show::fade_strips(&fade, &targets, fade_ms).await?;
        } else if want_mute {
            for (s, r) in &strips {
                check()?;
                fade.write(StripId(*r), "mute", ParamValue::Bool(s.mute))
                    .await?;
            }
        }
        for (s, r) in &strips {
            let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
            if want_mute && s.mute {
                check()?;
                fade.write(StripId(*r), "mute", ParamValue::Bool(true))
                    .await?;
            }
            let key = openfader_core::strip_key_for_index(&topo, *r as usize);
            if want_fader {
                check()?;
                self.reconcile_scene_write(StripId(*r), "fader", &json!(s.fader))
                    .await;
            }
            if want_mute {
                check()?;
                self.reconcile_scene_write(StripId(*r), "mute", &json!(s.mute))
                    .await;
            }
            if scope.is_none_or(|sc| sc.allows_param("fader")) {
                // No per-param readback on the recall path (best-effort fan
                // out on hardware): provenance per the `proven` flag above.
                out.push(Event::Param {
                    ch: *r,
                    field: "fader".to_string(),
                    value: json!(s.fader),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                    authority: authority(),
                });
            }
            if want_mute {
                out.push(Event::Param {
                    ch: *r,
                    field: "mute".to_string(),
                    value: json!(s.mute),
                    by: Some(writer.to_string()),
                    key: key.clone(),
                    authority: authority(),
                });
            }
        }
        let mut failed = Vec::new();
        // Remaining params in safe order (names ride along unless the
        // scope excludes them).
        for (s, r) in &strips {
            let mut ids: Vec<&String> = s.params.keys().collect();
            ids.sort_by_key(|id| safe_order_key(id));
            for id in ids {
                // A scene captured on another provider (or before a field
                // became derived/read-only) may still carry this identifier.
                if drv.param_read_only(id) {
                    continue;
                }
                if scope.is_some_and(|sc| !sc.allows_param_for(self.drv(&driver).as_ref(), id)) {
                    continue;
                }
                let value = s.params.get(id).expect("key just listed");
                let id = super::models::canonical_send_field(&driver, id).into_owned();
                let pv = json_to_param_value(value)?;
                // Best-effort per param on hardware (a desk that went
                // silent mid-recall must not abort the rest); strict on
                // the model.
                check()?;
                let strict = !drv.is_hardware();
                let expected: BTreeMap<_, _> = drv
                    .param_write_context(&id)
                    .iter()
                    .filter_map(|field| {
                        s.params
                            .get(*field)
                            .and_then(|v| json_to_param_value(v).ok())
                            .map(|v| ((*field).to_string(), v))
                    })
                    .collect();
                let send_members = self.send_bus_members(&driver, &id).await?;
                check()?;
                let bus_send = !send_members.is_empty();
                let bus_param = topo.iter().any(|family| {
                    family.kind == openfader_core::StripKind::Bus
                        && usize::from(*r) >= family.first
                        && usize::from(*r) < family.first + family.count
                });
                affected_bus.extend(send_members.into_iter().map(|m| m.0));
                let rr = if driver == "xair"
                    && (matches!(id.as_str(), "gain" | "phantom") && !expected.is_empty()
                        || bus_send
                        || bus_param)
                {
                    drv.set_param_guarded_checked(StripId(*r), &id, pv, &expected, &|members| {
                        check()?;
                        late_affected_preamp
                            .lock()
                            .unwrap()
                            .extend(members.iter().map(|m| m.0));
                        Ok(())
                    })
                    .await
                } else if expected.is_empty() {
                    drv.set_param(StripId(*r), &id, pv).await
                } else {
                    drv.set_param_guarded(StripId(*r), &id, pv, &expected).await
                };
                if let Err(error) = rr {
                    if strict {
                        return Err(error);
                    }
                    let key = openfader_core::strip_key_for_index(&topo, *r as usize)
                        .unwrap_or_else(|| r.to_string());
                    failed.push(format!("{key} {id}: {error}"));
                    continue;
                }
                check()?;
                self.reconcile_scene_write(StripId(*r), &id, value).await;
                // Recall-path echo: provenance per the `proven` flag above
                // (no readback on this path).
                out.push(Event::Param {
                    ch: *r,
                    field: id.clone(),
                    value: value.clone(),
                    by: Some(writer.to_string()),
                    key: openfader_core::strip_key_for_index(&topo, *r as usize),
                    authority: authority(),
                });
            }
            if s.params.contains_key("name")
                || s.name.is_empty()
                || scope.is_some_and(|sc| !sc.allows_param("name"))
            {
                continue;
            }
            check()?;
            let rr = drv
                .set_param(StripId(*r), "name", ParamValue::Text(s.name.clone()))
                .await;
            if let Err(error) = rr {
                if !self.active_drv().is_hardware() {
                    return Err(error);
                }
                failed.push(format!("strip {r} name: {error}"));
            } else {
                check()?;
                self.reconcile_scene_write(StripId(*r), "name", &json!(s.name))
                    .await;
                out.push(Event::Param {
                    ch: *r,
                    field: "name".to_string(),
                    value: json!(s.name),
                    by: Some(writer.to_string()),
                    key: self.active_strip_key(*r),
                    authority: authority(),
                });
            }
        }
        if let Some(automix) = scene.automix.as_ref().filter(|a| a.provider == driver) {
            if scope.is_none_or(|sc| sc.allows_global("automix") || sc.allows_group("config")) {
                check()?;
                let current = drv
                    .read_automix_settings()
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Automix snapshot unavailable on this mixer"))?;
                for (index, id) in ["enable_x", "enable_y", "lock_x", "lock_y"]
                    .iter()
                    .enumerate()
                {
                    if current[index] == automix.values[index] {
                        continue;
                    }
                    check()?;
                    let block: Vec<_> = drv
                        .destination_controls()
                        .into_iter()
                        .filter(|r| r.surface == "automix_settings" && r.target == "automix")
                        .collect();
                    anyhow::ensure!(
                        block.len() == 4
                            && block[index].current.as_ref().is_some_and(|v| v.verified),
                        "Automix snapshot readback unavailable"
                    );
                    let expected = block[index].current.as_ref().unwrap().value;
                    let context = || check().is_ok();
                    drv.set_output_processor_guarded(
                        "automix",
                        id,
                        if automix.values[index] { 1.0 } else { 0.0 },
                        expected,
                        &block,
                        &context,
                    )
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("Automix recall unconfirmed"))?;
                }
                check()?;
                anyhow::ensure!(
                    drv.read_automix_settings().await? == Some(automix.values),
                    "Automix recall readback changed; result uncertain"
                );
            }
        }
        // FX + routing snapshots report write failures on every provider,
        // gated on their globals; Main LR on its own global.
        if scope.is_none_or(|sc| sc.allows_global("fx")) {
            self.recall_fx(&scene.fx, drv.as_ref(), &check).await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("routing")) {
            check()?;
            let remaining: Vec<_> = scene
                .routing
                .iter()
                .filter(|r| {
                    driver != "xair" || !matches!(r.target.as_str(), "analog" | "usb" | "bank")
                })
                .cloned()
                .collect();
            self.recall_routing(&remaining, writer, out, &check).await?;
            check()?;
            if let Some(taps) = output_taps {
                let live = drv.read_output_taps().await?.ok_or_else(|| {
                    anyhow::anyhow!("snapshot output taps unavailable after routing recall")
                })?;
                check()?;
                validate_scene_output_taps(&taps.values, &live, &scene.routing, drv.as_ref())?;
                for saved in &taps.values {
                    let current = live.iter().find(|v| v.target == saved.target).unwrap();
                    anyhow::ensure!(
                        current.source == saved.source,
                        "snapshot output source changed for {}; mixer may be partly changed",
                        saved.target
                    );
                }
                for saved in &taps.values {
                    check()?;
                    let row = live.iter().find(|v| v.target == saved.target).unwrap();
                    anyhow::ensure!(
                        drv.output_route(&saved.target)?.source == saved.source,
                        "snapshot output source changed for {}; mixer may be partly changed",
                        saved.target
                    );
                    if row.tap == saved.tap {
                        continue;
                    }
                    let descriptors: Vec<_> = drv
                        .destination_controls()
                        .into_iter()
                        .filter(|d| {
                            d.surface == "routing" && d.id == "tap" && d.target == saved.target
                        })
                        .collect();
                    anyhow::ensure!(
                        descriptors.len() == 1
                            && descriptors[0]
                                .current
                                .as_ref()
                                .is_some_and(|v| v.verified && v.value == f32::from(row.tap)),
                        "snapshot output tap descriptor unavailable for {}",
                        saved.target
                    );
                    drv.set_output_tap_guarded(saved, f32::from(row.tap), &descriptors, &|| {
                        check().is_ok()
                    })
                    .await?;
                    check()?;
                }
                // Each changed target is independently checked/read back by its
                // native write. Confirm the complete final state once, including
                // targets that did not need a write.
                let confirmed = drv.read_output_taps().await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "snapshot output tap readback unavailable; mixer may be partly changed"
                    )
                })?;
                check()?;
                validate_scene_output_taps(&taps.values, &confirmed, &scene.routing, drv.as_ref())?;
                anyhow::ensure!(
                    taps.values.iter().all(|saved| confirmed.contains(saved)),
                    "snapshot output tap recall unconfirmed; mixer may be partly changed"
                );
            }
        }
        if let Some(saved) = rta {
            for (id, desired) in [
                ("detector", f32::from(saved.detector)),
                ("decay", saved.decay),
            ] {
                check()?;
                let block = drv.read_rta_settings().await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "snapshot RTA readback unavailable; mixer may be partly changed"
                    )
                })?;
                check()?;
                validate_rta_recall(saved, &block)?;
                let current = block
                    .iter()
                    .find(|c| c.id == id)
                    .unwrap()
                    .current
                    .as_ref()
                    .unwrap()
                    .value;
                if (current - desired).abs() <= 0.00001 {
                    continue;
                }
                let receipt = drv
                    .set_output_processor_guarded("rta", id, desired, current, &block, &|| {
                        check().is_ok()
                    })
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "snapshot RTA recall unconfirmed; mixer may be partly changed"
                        )
                    })?;
                check()?;
                anyhow::ensure!(
                    receipt.target == "rta"
                        && receipt.id == id
                        && receipt
                            .current
                            .as_ref()
                            .is_some_and(|v| v.verified && (v.value - desired).abs() <= 0.00001),
                    "snapshot RTA recall unconfirmed; mixer may be partly changed"
                );
            }
            let confirmed = drv.read_rta_settings().await?.ok_or_else(|| {
                anyhow::anyhow!(
                    "snapshot RTA final readback unavailable; mixer may be partly changed"
                )
            })?;
            check()?;
            let confirmed = validate_rta_recall(saved, &confirmed)?;
            anyhow::ensure!(
                confirmed.detector == saved.detector
                    && (confirmed.decay - saved.decay).abs() <= 0.00001,
                "snapshot RTA final readback changed; mixer may be partly changed"
            );
        }
        if let Some(saved) = monitor {
            for row in &saved.values {
                check()?;
                let block = drv.read_monitor_settings().await?.ok_or_else(|| {
                    anyhow::anyhow!(
                        "snapshot monitor readback unavailable; mixer may be partly changed"
                    )
                })?;
                check()?;
                validate_monitor_recall(saved, &block)?;
                let current = block
                    .iter()
                    .find(|c| c.id == row.id)
                    .unwrap()
                    .current
                    .as_ref()
                    .unwrap()
                    .value;
                if (current - row.value).abs() <= 0.00001 {
                    continue;
                }
                let receipt = drv
                    .set_output_processor_guarded(
                        "monitor",
                        &row.id,
                        row.value,
                        current,
                        &block,
                        &|| check().is_ok(),
                    )
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "snapshot monitor recall unconfirmed; mixer may be partly changed"
                        )
                    })?;
                check()?;
                anyhow::ensure!(
                    receipt.id == row.id
                        && receipt.target == "monitor"
                        && receipt
                            .current
                            .as_ref()
                            .is_some_and(|v| v.verified && (v.value - row.value).abs() <= 0.00001),
                    "snapshot monitor recall unconfirmed; mixer may be partly changed"
                );
            }
            let confirmed = drv.read_monitor_settings().await?.ok_or_else(|| {
                anyhow::anyhow!(
                    "snapshot monitor final readback unavailable; mixer may be partly changed"
                )
            })?;
            check()?;
            validate_monitor_recall(saved, &confirmed)?;
            anyhow::ensure!(
                saved.values.iter().all(|row| confirmed.iter().any(|c| {
                    c.id == row.id
                        && c.current
                            .as_ref()
                            .is_some_and(|v| v.verified && (v.value - row.value).abs() <= 0.00001)
                })),
                "snapshot monitor final readback changed; mixer may be partly changed"
            );
        }
        if let Some(groups) = scene
            .mute_groups
            .as_ref()
            .filter(|groups| groups.provider == driver)
        {
            self.recall_mute_groups(
                groups,
                scope,
                &topo,
                drv.as_ref(),
                &|members| {
                    check()?;
                    late_affected_preamp
                        .lock()
                        .unwrap()
                        .extend(members.iter().map(|m: &StripId| m.0));
                    Ok(())
                },
                out,
            )
            .await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("main")) {
            if let Some(m) = scene.main_fader {
                check()?;
                drv.set_main_fader(m).await?;
                if let Some(fam) = topo
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Main)
                {
                    check()?;
                    self.reconcile_scene_write(StripId(fam.first as u16), "fader", &json!(m))
                        .await;
                }
            }
        }
        check()?;
        self.refresh_fx_cache().await;
        check()?;
        out.push(self.fx_message_cached());
        let mut overrode: Vec<u16> = Vec::new();
        for (_, r) in &strips {
            if let Some(holder) = self.lock_holder(*r) {
                if holder != writer && !overrode.contains(r) {
                    overrode.push(*r);
                }
            }
        }
        affected_preamp.extend(affected_bus);
        affected_preamp.extend(late_affected_preamp.into_inner().unwrap());
        for r in affected_preamp {
            if self.lock_holder(r).is_some_and(|holder| holder != writer) && !overrode.contains(&r)
            {
                overrode.push(r);
            }
        }
        overrode.sort_unstable();
        if !failed.is_empty() {
            anyhow::bail!(
                "Snapshot recall incomplete: {} setting(s) failed: {}",
                failed.len(),
                failed
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        Ok(overrode)
    }

    async fn recall_mute_groups(
        &self,
        groups: &StoredMuteGroups,
        scope: Option<&RecallScope>,
        topo: &[openfader_core::StripFamily],
        drv: &dyn DynDriver,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
        out: &mut Vec<Event>,
    ) -> anyhow::Result<()> {
        if scope.is_some_and(|sc| !sc.allows_global("mute_groups") && !sc.allows_group("config")) {
            return Ok(());
        }
        validate_mute_group_snapshot(groups, topo, drv)?;
        let count = usize::from(drv.mute_group_count());
        anyhow::ensure!(
            drv.mute_group_snapshot_supported() && count <= 32 && groups.masters.len() == count,
            "mute-group snapshot is incompatible with this mixer"
        );
        let max_mask = if count == 32 {
            u32::MAX
        } else {
            (1u32 << count) - 1
        };
        let mut members = Vec::new();
        for member in &groups.memberships {
            anyhow::ensure!(member.mask <= max_mask, "invalid mute-group snapshot mask");
            let index = resolve_mute_group_strip(topo, &member.key).ok_or_else(|| {
                anyhow::anyhow!("mute-group snapshot strip {} unavailable", member.key)
            })?;
            let family = topo
                .iter()
                .find(|f| index >= f.first && index < f.first + f.count)
                .ok_or_else(|| {
                    anyhow::anyhow!("mute-group snapshot strip {} unavailable", member.key)
                })?;
            anyhow::ensure!(
                drv.mute_group_snapshot_applies(family.kind),
                "mute-group membership unavailable for {}",
                member.key
            );
            if scope.is_none_or(|sc| {
                let exact_key = sc
                    .strip_keys
                    .as_ref()
                    .is_some_and(|keys| keys.contains(&member.key));
                (exact_key || sc.matches_strip(index as u16, topo)) && sc.allows_group("config")
            }) {
                members.push((index as u16, member));
            }
        }
        for (index, member) in members {
            check(&[])?;
            let started = Instant::now();
            drv.restore_mute_group_mask(StripId(index), member.mask, check)
                .await
                .map_err(|error| anyhow::anyhow!("snapshot recall incomplete: mute-group membership for {}: {error}; mixer may be partly changed", member.key))?;
            check(&[])?;
            for group in 0..count {
                self.evict_param_unless_fresh(
                    StripId(index),
                    &format!("_mg{}", group + 1),
                    started,
                );
                out.push(Event::MuteGroupAssign {
                    ch: index,
                    group: group as u8,
                    member: member.mask & (1 << group) != 0,
                    key: Some(member.key.clone()),
                });
            }
        }
        if scope.is_none_or(|sc| sc.allows_global("mute_groups")) {
            for (group, &muted) in groups.masters.iter().enumerate() {
                check(&[])?;
                let started = Instant::now();
                drv.restore_mute_group_master(group as u8, muted, &|| check(&[])).await.map_err(|error| {
                    anyhow::anyhow!("snapshot recall incomplete: mute group {}: {error}; mixer may be partly changed", group + 1)
                })?;
                check(&[])?;
                self.evict_param_unless_fresh(StripId(group as u16), "_mute_group", started);
                out.push(Event::MuteGroup {
                    group: group as u8,
                    muted,
                    key: Some(format!("mgroup/{:02}", group + 1)),
                });
            }
        }
        Ok(())
    }
}

fn stored_rta_from_controls(
    provider: &str,
    block: &[openfader_core::destinations::DestinationControl],
) -> anyhow::Result<StoredRta> {
    anyhow::ensure!(block.len() == 2, "RTA snapshot roster incomplete");
    let generation = block[0]
        .generation
        .ok_or_else(|| anyhow::anyhow!("RTA snapshot generation unavailable"))?;
    for control in block {
        anyhow::ensure!(
            control.target == "rta"
                && control.surface == "output_rta"
                && control.generation == Some(generation)
                && control.revision.is_some()
                && control
                    .current
                    .as_ref()
                    .is_some_and(|v| v.verified && snapshot_control_allows(control, v.value)),
            "RTA snapshot metadata or readback invalid"
        );
    }
    let value = |id| -> anyhow::Result<f32> {
        let rows: Vec<_> = block.iter().filter(|c| c.id == id).collect();
        anyhow::ensure!(rows.len() == 1, "RTA snapshot ID missing or duplicated");
        Ok(rows[0].current.as_ref().unwrap().value)
    };
    let detector = value("detector")?;
    anyhow::ensure!(
        detector.fract() == 0.0 && (0.0..=255.0).contains(&detector),
        "RTA snapshot detector invalid"
    );
    let rta = StoredRta {
        provider: provider.to_string(),
        detector: detector as u8,
        decay: value("decay")?,
    };
    crate::local_library::validate_stored_rta(&rta, "RTA snapshot").map_err(anyhow::Error::msg)?;
    Ok(rta)
}

fn validate_rta_recall(
    saved: &StoredRta,
    block: &[openfader_core::destinations::DestinationControl],
) -> anyhow::Result<StoredRta> {
    crate::local_library::validate_stored_rta(saved, "RTA snapshot").map_err(anyhow::Error::msg)?;
    let live = stored_rta_from_controls(&saved.provider, block)?;
    for (id, desired) in [
        ("detector", f32::from(saved.detector)),
        ("decay", saved.decay),
    ] {
        anyhow::ensure!(
            block
                .iter()
                .any(|c| c.id == id && snapshot_control_allows(c, desired)),
            "RTA snapshot value not advertised: {}",
            id
        );
    }
    Ok(live)
}

fn snapshot_control_allows(
    control: &openfader_core::destinations::DestinationControl,
    value: f32,
) -> bool {
    if !value.is_finite() {
        return false;
    }
    if !control.choices.is_empty() {
        return control.choices.iter().any(|c| c.value == value);
    }
    control.range.as_ref().is_some_and(|range| {
        range.min.is_finite()
            && range.max.is_finite()
            && range.min <= range.max
            && (range.min..=range.max).contains(&value)
            && range.step.is_none_or(|step| {
                step.is_finite() && step > 0.0 && {
                    let n = (value - range.min) / step;
                    (n - n.round()).abs() <= 0.001
                }
            })
    })
}

fn stored_monitor_from_controls(
    provider: &str,
    block: &[openfader_core::destinations::DestinationControl],
) -> anyhow::Result<StoredMonitor> {
    anyhow::ensure!(
        !block.is_empty() && block.len() <= 128,
        "monitor snapshot roster incomplete"
    );
    let generation = block[0]
        .generation
        .ok_or_else(|| anyhow::anyhow!("monitor snapshot generation unavailable"))?;
    let mut values = Vec::new();
    for control in block {
        let current = control
            .current
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("monitor snapshot readback unavailable"))?;
        anyhow::ensure!(
            control.target == "monitor"
                && control.surface == "output_monitor"
                && control.generation == Some(generation)
                && control.revision.is_some()
                && current.verified
                && snapshot_control_allows(control, current.value),
            "monitor snapshot metadata or readback invalid"
        );
        values.push(StoredMonitorValue {
            id: control.id.clone(),
            value: current.value,
        });
    }
    let monitor = StoredMonitor {
        provider: provider.to_string(),
        values,
    };
    crate::local_library::validate_stored_monitor(&monitor, "monitor snapshot")
        .map_err(anyhow::Error::msg)?;
    Ok(monitor)
}

fn validate_monitor_recall(
    saved: &StoredMonitor,
    block: &[openfader_core::destinations::DestinationControl],
) -> anyhow::Result<()> {
    crate::local_library::validate_stored_monitor(saved, "monitor snapshot")
        .map_err(anyhow::Error::msg)?;
    let live = stored_monitor_from_controls(&saved.provider, block)?;
    anyhow::ensure!(
        saved.values.len() == live.values.len(),
        "monitor snapshot roster mismatch"
    );
    for row in &saved.values {
        anyhow::ensure!(
            block
                .iter()
                .any(|c| c.id == row.id && snapshot_control_allows(c, row.value)),
            "monitor snapshot ID or value not advertised: {}",
            row.id
        );
    }
    Ok(())
}

fn validate_scene_output_taps(
    saved: &[openfader_core::routing::OutputTapState],
    live: &[openfader_core::routing::OutputTapState],
    routes: &[StoredRoute],
    drv: &dyn DynDriver,
) -> anyhow::Result<()> {
    let mut live_targets = std::collections::HashSet::new();
    for row in live {
        anyhow::ensure!(
            live_targets.insert(&row.target),
            "duplicate live output tap target"
        );
    }
    anyhow::ensure!(
        !live.is_empty() && saved.len() == live.len(),
        "snapshot output tap roster incomplete"
    );
    let planes = drv.routing_planes();
    let controls = drv.destination_controls();
    let mut seen = std::collections::HashSet::new();
    for row in saved {
        anyhow::ensure!(
            seen.insert(&row.target) && live_targets.contains(&row.target),
            "invalid snapshot output tap target or value"
        );
        let tap_controls: Vec<_> = controls
            .iter()
            .filter(|c| c.surface == "routing" && c.id == "tap" && c.target == row.target)
            .collect();
        anyhow::ensure!(
            tap_controls.len() == 1
                && tap_controls[0]
                    .choices
                    .iter()
                    .any(|choice| choice.value == f32::from(row.tap)),
            "snapshot output tap value is not advertised for {}",
            row.target
        );
        let destinations: Vec<_> = planes
            .iter()
            .flat_map(|p| &p.destinations)
            .filter(|d| d.output && d.id == row.target)
            .collect();
        let matching: Vec<_> = routes.iter().filter(|r| r.target == row.target).collect();
        anyhow::ensure!(
            destinations.len() == 1 && matching.len() == 1,
            "snapshot output route missing or duplicated for {}",
            row.target
        );
        let route = matching[0];
        anyhow::ensure!(
            destinations[0]
                .source_groups
                .iter()
                .any(|g| g.key == route.group
                    && route.index > 0
                    && usize::from(route.index) <= g.count)
                && row.source == format!("{} {}", route.group, route.index),
            "snapshot output source invalid for {}",
            row.target
        );
    }
    Ok(())
}

fn validate_mute_group_snapshot(
    groups: &StoredMuteGroups,
    topo: &[openfader_core::StripFamily],
    drv: &dyn DynDriver,
) -> anyhow::Result<()> {
    let count = usize::from(drv.mute_group_count());
    anyhow::ensure!(
        drv.mute_group_snapshot_supported()
            && count > 0
            && count <= 32
            && groups.masters.len() == count,
        "mute-group snapshot is incompatible with this mixer"
    );
    let max_mask = if count == 32 {
        u32::MAX
    } else {
        (1u32 << count) - 1
    };
    let expected_count: usize = topo
        .iter()
        .filter(|f| drv.mute_group_snapshot_applies(f.kind))
        .map(|f| f.count)
        .sum();
    anyhow::ensure!(
        groups.memberships.len() == expected_count,
        "mute-group snapshot has incomplete memberships"
    );
    let mut seen = std::collections::HashSet::new();
    for member in &groups.memberships {
        anyhow::ensure!(member.mask <= max_mask, "invalid mute-group snapshot mask");
        let index = resolve_mute_group_strip(topo, &member.key).ok_or_else(|| {
            anyhow::anyhow!("mute-group snapshot strip {} unavailable", member.key)
        })?;
        let family = topo
            .iter()
            .find(|f| index >= f.first && index < f.first + f.count)
            .ok_or_else(|| {
                anyhow::anyhow!("mute-group snapshot strip {} unavailable", member.key)
            })?;
        anyhow::ensure!(
            drv.mute_group_snapshot_applies(family.kind) && seen.insert(index),
            "mute-group snapshot strip {} is invalid or duplicated",
            member.key
        );
    }
    Ok(())
}

fn resolve_mute_group_strip(topo: &[openfader_core::StripFamily], key: &str) -> Option<usize> {
    topo.iter()
        .flat_map(|family| family.first..family.first + family.count)
        .find(|&index| openfader_core::strip_key_for_index(topo, index).as_deref() == Some(key))
        .or_else(|| openfader_core::strip_index_for_key(topo, key))
}

/// Write-through [`openfader_core::show::FadeDriver`]: numbered-scene recall
/// shares the single [`openfader_core::show::fade_strips`] implementation with
/// named-show recall (which fades through the driver blanket impl).
impl openfader_core::show::FadeDriver for Engine {
    async fn fade_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.drv_set_mute(strip, muted).await
    }

    async fn fade_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        // Inclusive display getter (continuity: own writes included, no proof
        // claimed), never the stale pre-write cache entry, which would
        // start a named-show fade from the wrong pre-move state.
        self.drv_get_fader(strip).await
    }

    async fn fade_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.drv_set_fader(strip, value).await
    }
}

/// Keep the shared fader law while suppressing excluded mute writes entirely.
/// Rewriting a sampled mute could race a console-side edit during a fade.
struct ScopedSceneFade<'a> {
    recall_mutes: bool,
    driver: Arc<dyn DynDriver>,
    check: &'a (dyn Fn() -> anyhow::Result<()> + Send + Sync),
    checked_buses: std::collections::BTreeSet<u16>,
    affected: &'a std::sync::Mutex<std::collections::BTreeSet<u16>>,
}
impl ScopedSceneFade<'_> {
    async fn write(&self, strip: StripId, field: &str, value: ParamValue) -> anyhow::Result<()> {
        (self.check)()?;
        if self.checked_buses.contains(&strip.0) {
            return self
                .driver
                .set_param_guarded_checked(strip, field, value, &BTreeMap::new(), &|members| {
                    (self.check)()?;
                    self.affected
                        .lock()
                        .unwrap()
                        .extend(members.iter().map(|member| member.0));
                    Ok(())
                })
                .await;
        }
        match value {
            ParamValue::Float(value) => self.driver.set_fader(strip, value).await,
            ParamValue::Bool(muted) => self.driver.set_mute(strip, muted).await,
            _ => anyhow::bail!("unsupported fade control"),
        }
    }
}
impl openfader_core::show::FadeDriver for ScopedSceneFade<'_> {
    async fn fade_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        (self.check)()?;
        if self.recall_mutes {
            self.write(strip, "mute", ParamValue::Bool(muted)).await
        } else {
            Ok(())
        }
    }
    async fn fade_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        (self.check)()?;
        self.driver.get_fader(strip).await
    }
    async fn fade_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        (self.check)()?;
        self.write(strip, "fader", ParamValue::Float(value)).await
    }
}
