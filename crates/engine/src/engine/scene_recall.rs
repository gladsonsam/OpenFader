//! Scene recall operations.

use super::*;

impl Engine {
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
                // after local moves. Params with no proof at all are omitted
                // (never fabricated); fader/mute keep their historical
                // silence defaults only when nothing ever proved them.
                let fader = self.capture_read_param(&driver, strip, "fader").await;
                let mute = self.capture_read_param(&driver, strip, "mute").await;
                let name_v = self.capture_read_param(&driver, strip, "name").await;
                let mut params = HashMap::new();
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
                    let captured = match self.drv(&driver).capture_param_override(strip, id).await {
                        Some(result) => result.ok(),
                        None => self.capture_read_param(&driver, strip, id).await,
                    };
                    if let Some(pv) = captured {
                        // Skip catalogue defaults? No, full plane means
                        // every value, so restores are exact.
                        params.insert(id.to_string(), param_value_json(&pv));
                    }
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
        let fx = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .map(|s| StoredFx {
                slot: s.slot,
                fx_type: s.fx_type.clone(),
                on: s.on,
                bypass: s.bypass,
                params: s.params.clone(),
            })
            .collect();
        let routing = self.routing_snapshot_on(&driver).await;
        Ok(StoredScene {
            number,
            name: name.to_string(),
            saved_at: now_unix(),
            main_fader: self.main_fader_on(&driver).await,
            strips,
            fx,
            routing,
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
        let topo = self.driver_topology(&driver);
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
        // Faders ride the single shared fade (legacy order: pre-unmute
        // targets, ramp, snap, target mutes); params/FX/routing/main follow.
        let want_fader = scope.is_none_or(|sc| sc.allows_param("fader"));
        let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
        if want_fader && !strips.is_empty() {
            let targets: Vec<_> = strips
                .iter()
                .map(|(s, r)| (StripId(*r), s.fader, s.mute))
                .collect();
            let fade = ScopedSceneFade {
                engine: self,
                recall_mutes: want_mute,
            };
            openfader_core::show::fade_strips(&fade, &targets, fade_ms).await?;
        } else if want_mute {
            for (s, r) in &strips {
                self.drv_set_mute(StripId(*r), s.mute).await?;
            }
        }
        for (s, r) in &strips {
            let want_mute = scope.is_none_or(|sc| sc.allows_param("mute"));
            if want_mute && s.mute {
                self.drv_set_mute(StripId(*r), true).await?;
            }
            let key = openfader_core::strip_key_for_index(&topo, *r as usize);
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
                if scope.is_some_and(|sc| !sc.allows_param_for(self.drv(&driver).as_ref(), id)) {
                    continue;
                }
                let value = s.params.get(id).expect("key just listed");
                let pv = json_to_param_value(value)?;
                // Best-effort per param on hardware (a desk that went
                // silent mid-recall must not abort the rest); strict on
                // the model.
                let strict = !self.active_drv().is_hardware();
                let drv = self.active_drv();
                let expected: BTreeMap<_, _> = drv
                    .param_write_context(id)
                    .iter()
                    .filter_map(|field| {
                        s.params
                            .get(*field)
                            .and_then(|v| json_to_param_value(v).ok())
                            .map(|v| ((*field).to_string(), v))
                    })
                    .collect();
                let rr = if expected.is_empty() {
                    self.drv_set_param(StripId(*r), id, pv).await
                } else {
                    drv.set_param_guarded(StripId(*r), id, pv, &expected).await
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
                // Recall-path echo: provenance per the `proven` flag above
                // (no readback on this path).
                out.push(Event::Param {
                    ch: *r,
                    field: (*id).clone(),
                    value: value.clone(),
                    by: Some(writer.to_string()),
                    key: openfader_core::strip_key_for_index(&topo, *r as usize),
                    authority: authority(),
                });
            }
            if s.name.is_empty() || scope.is_some_and(|sc| !sc.allows_param("name")) {
                continue;
            }
            let rr = self
                .drv_set_param(StripId(*r), "name", ParamValue::Text(s.name.clone()))
                .await;
            if let Err(error) = rr {
                if !self.active_drv().is_hardware() {
                    return Err(error);
                }
                failed.push(format!("strip {r} name: {error}"));
            }
        }
        // FX + routing snapshots (model-strict, best-effort elsewhere),
        // gated on their globals; Main LR on its own global.
        if scope.is_none_or(|sc| sc.allows_global("fx")) {
            self.recall_fx(&scene.fx).await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("routing")) {
            self.recall_routing(&scene.routing, writer, out).await?;
        }
        if scope.is_none_or(|sc| sc.allows_global("main")) {
            if let Some(m) = scene.main_fader {
                self.drv_set_main(m).await?;
            }
        }
        self.refresh_fx_cache().await;
        let mut overrode: Vec<u16> = Vec::new();
        for (_, r) in &strips {
            if let Some(holder) = self.lock_holder(*r) {
                if holder != writer && !overrode.contains(r) {
                    overrode.push(*r);
                }
            }
        }
        overrode.sort_unstable();
        if !failed.is_empty() {
            out.push(Event::Error {
                message: format!(
                    "Scene recall skipped {} setting(s): {}",
                    failed.len(),
                    failed
                        .iter()
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("; ")
                ),
                locked_by: None,
                ch: None,
            });
        }
        Ok(overrode)
    }
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
    engine: &'a Engine,
    recall_mutes: bool,
}
impl openfader_core::show::FadeDriver for ScopedSceneFade<'_> {
    async fn fade_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        if self.recall_mutes {
            self.engine.drv_set_mute(strip, muted).await
        } else {
            Ok(())
        }
    }
    async fn fade_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.engine.drv_get_fader(strip).await
    }
    async fn fade_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.engine.drv_set_fader(strip, value).await
    }
}
