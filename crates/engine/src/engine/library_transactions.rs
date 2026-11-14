//! Serialized library candidates, mutations, and durable publication.

use super::*;

impl Engine {
    /// Build the commit candidate for one library action (legacy and new
    /// commands share this): clones live memory and applies the mutation.
    /// Capture actions read the driver here, under the caller's `lib_lock`,
    /// pinned to the `pin` desk session (generation + driver sampled under
    /// the `sync_state` barrier before the build); preflight/capture errors
    /// leave disk and memory untouched. `reset` makes no driver call.
    pub(super) async fn build_library_candidate(
        &self,
        action: &LibraryAction,
        pin: Option<(u64, String, u64)>,
    ) -> anyhow::Result<(HashMap<u16, StoredScene>, Vec<Show>, Option<u16>)> {
        let mut numbered = self.scenes_lib.read().unwrap().clone();
        let mut shows = self.shows.read().unwrap().clone();
        let mut current = *self.scenes_current.read().unwrap();
        match action {
            LibraryAction::Save { n, name } => {
                let n = validate_scene_n(*n)?;
                let name = clean_name(name, "name")?;
                let (pin_gen, pin_driver) = pin
                    .as_ref()
                    .map(|(g, d, _)| (*g, d.clone()))
                    .unwrap_or_else(|| {
                        (
                            self.connection_gen.load(Ordering::Acquire),
                            self.active_driver(),
                        )
                    });
                let scene = self.capture_full_on(n, &name, &pin_driver, pin_gen).await?;
                numbered.insert(n, scene);
            }
            LibraryAction::Rename { n, name } => {
                let n = validate_scene_n(*n)?;
                let name = clean_name(name, "name")?;
                let slot = numbered
                    .get_mut(&n)
                    .ok_or_else(|| anyhow::anyhow!("unknown scene {n:03}"))?;
                slot.name = name;
                slot.saved_at = now_unix();
            }
            LibraryAction::Delete { n } => {
                let n = validate_scene_n(*n)?;
                if numbered.remove(&n).is_none() {
                    anyhow::bail!("unknown scene {n:03}");
                }
                if current == Some(n) {
                    current = None;
                }
            }
            LibraryAction::ClearNumbered => {
                numbered.clear();
                current = None;
            }
            LibraryAction::Capture { show, name } => {
                let show = clean_name(show, "show")?;
                let name = clean_name(name, "name")?;
                let (pin_gen, pin_driver) = pin
                    .as_ref()
                    .map(|(g, d, _)| (*g, d.clone()))
                    .unwrap_or_else(|| {
                        (
                            self.connection_gen.load(Ordering::Acquire),
                            self.active_driver(),
                        )
                    });
                let snapshot = self.capture_full_on(1, &name, &pin_driver, pin_gen).await?;
                let scene = Scene::from_full(snapshot);
                match shows.iter_mut().find(|s| s.name == show) {
                    Some(slot) => slot.add_scene(scene),
                    None => {
                        let mut fresh = Show::new(&show);
                        fresh.add_scene(scene);
                        shows.push(fresh);
                    }
                }
            }
            LibraryAction::DeleteNamed { show, name } => {
                let show = clean_name(show, "show")?;
                let name = clean_name(name, "name")?;
                let slot = shows
                    .iter_mut()
                    .find(|s| s.name == show)
                    .ok_or_else(|| anyhow::anyhow!("unknown show {show:?}"))?;
                if !slot.remove(&name) {
                    anyhow::bail!("unknown scene {name:?} in show {show:?}");
                }
            }
            LibraryAction::Reset => {
                numbered.clear();
                shows.clear();
                current = None;
            }
        }
        Ok((numbered, shows, current))
    }

    async fn revalidate_candidate_native(
        &self, action: &LibraryAction, pin: &Option<(u64, String, u64)>,
        numbered: &HashMap<u16, StoredScene>, shows: &[Show],
    ) -> anyhow::Result<()> {
        let Some((generation, driver, native_generation)) = pin else { return Ok(()); };
        let scene = match action {
            LibraryAction::Save { n, .. } => numbered.get(n),
            LibraryAction::Capture { show, name } => {
                let show = clean_name(show, "show")?;
                let name = clean_name(name, "name")?;
                shows.iter().find(|s| s.name == show).and_then(|s| s.get(&name)).and_then(|s| s.full())
            }
            _ => None,
        };
        if let Some(snapshot) = scene.and_then(|s| s.native_processors.as_ref()) {
            self.revalidate_scene_native_processors(driver, *generation, *native_generation, snapshot).await?;
        }
        Ok(())
    }

    /// Run one library action through the serialized candidate commit:
    /// lock, block-check, build, commit-and-swap. Pre-replace failures are
    /// `Err` with disk AND memory untouched.
    pub(super) async fn library_apply_action(
        &self,
        action: &LibraryAction,
    ) -> anyhow::Result<CommitDisposition> {
        let _guard = self.lib_lock.lock().await;
        self.ensure_library_writable(action.is_reset())?;
        // r58 P2(2): capture actions bind to one desk session. The pin is
        // sampled under the `sync_state` barrier; a suspended read lane
        // fails fast before any driver I/O. Pure-local actions skip this.
        let pin_full = Self::action_needs_capture_pin(action).then(|| self.sample_capture_pin());
        if let Some((_, _, suspended, _)) = &pin_full {
            if *suspended {
                anyhow::bail!("scene capture rejected: desk session changed (reads suspended)");
            }
        }
        let pin = pin_full.map(|(g, d, _, n)| (g, d, n));
        let (numbered, shows, current) = self.build_library_candidate(action, pin.clone()).await?;
        self.revalidate_candidate_native(action, &pin, &numbered, &shows).await?;
        self.commit_with_capture_guard(pin, numbered, shows, current)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Explicit durable mutation: revision-guarded serialized commit with a
    /// correlated unicast result on EVERY path (including preflight and
    /// capture errors — a list refresh never counts as success). Committed
    /// broadcasts the `scenes`, `shows` and status frames; rejected sends
    /// no broadcasts; uncertain broadcasts only the latched status.
    pub(super) async fn library_mutate(
        &self,
        request_id: String,
        expected_revision: u64,
        action: LibraryAction,
        writer: &str,
        out: &mut Vec<Event>,
    ) {
        let operation = action.op_name().to_string();
        let mode = self.lib_health.read().unwrap().mode.as_str().to_string();
        let rq = request_id.clone();
        let op = operation.clone();
        let md = mode.clone();
        let reject = |error: String| {
            let revision = self.lib_revision.load(Ordering::Relaxed);
            self.unicast(
                writer,
                Event::LocalLibraryResult {
                    request_id: rq.clone(),
                    operation: op.clone(),
                    revision,
                    status: "rejected".to_string(),
                    mode: md.clone(),
                    error: Some(error.clone()),
                },
            );
            self.unicast(
                writer,
                Event::Error {
                    message: error,
                    locked_by: None,
                    ch: None,
                },
            );
        };
        let current = self.lib_revision.load(Ordering::Relaxed);
        if expected_revision != current {
            reject(format!(
                "stale revision: expected {expected_revision}, current {current}"
            ));
            return;
        }
        let _guard = self.lib_lock.lock().await;
        // Re-check under the lock: legacy commands share the revision.
        let current = self.lib_revision.load(Ordering::Relaxed);
        if expected_revision != current {
            reject(format!(
                "stale revision: expected {expected_revision}, current {current}"
            ));
            return;
        }
        if let Err(e) = self.ensure_library_writable(action.is_reset()) {
            reject(format!("{e:#}"));
            return;
        }
        // r58 P2(2): same desk-session binding as the legacy path. The pin
        // is sampled under the `sync_state` barrier while holding `lib_lock`;
        // suspension fails fast with the correlated result+error preserved.
        // Pure-local actions skip the pin entirely.
        let pin_full = Self::action_needs_capture_pin(&action).then(|| self.sample_capture_pin());
        if let Some((_, _, suspended, _)) = &pin_full {
            if *suspended {
                reject(
                    "scene capture rejected: desk session changed (reads suspended)".to_string(),
                );
                return;
            }
        }
        let pin = pin_full.map(|(g, d, _, n)| (g, d, n));
        let candidate = self.build_library_candidate(&action, pin.clone()).await;
        let (numbered, shows, current_slot) = match candidate {
            Ok(v) => v,
            Err(e) => {
                reject(format!("{e:#}"));
                return;
            }
        };
        if let Err(error) = self.revalidate_candidate_native(&action, &pin, &numbered, &shows).await {
            reject(format!("{error:#}"));
            return;
        }
        match self.commit_with_capture_guard(pin, numbered, shows, current_slot) {
            Ok(CommitDisposition::Committed { revision }) => {
                self.unicast(
                    writer,
                    Event::LocalLibraryResult {
                        request_id,
                        operation,
                        revision,
                        status: "committed".to_string(),
                        mode,
                        error: None,
                    },
                );
                out.push(self.scenes_message());
                out.push(self.shows_message());
                out.push(self.local_library_status_message());
            }
            Ok(CommitDisposition::Uncertain { revision, error }) => {
                self.unicast(
                    writer,
                    Event::LocalLibraryResult {
                        request_id,
                        operation,
                        revision,
                        status: "uncertain".to_string(),
                        mode,
                        error: Some(error.clone()),
                    },
                );
                self.unicast(
                    writer,
                    Event::Error {
                        message: format!("local library uncertain: {error}"),
                        locked_by: None,
                        ch: None,
                    },
                );
                out.push(self.local_library_status_message());
            }
            Err(e) => reject(e),
        }
    }
}
