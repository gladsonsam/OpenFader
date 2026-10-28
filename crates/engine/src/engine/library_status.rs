//! Library status operations.

use super::*;

impl Engine {
    // ================= R2: numbered scenes =================

    /// Current `scenes` event (slots sorted by number).
    pub(super) fn scenes_message(&self) -> Event {
        let lib = self.scenes_lib.read().unwrap();
        let current = *self.scenes_current.read().unwrap();
        let mut scenes: Vec<crate::protocol::SceneEntry> = lib
            .values()
            .map(|s| crate::protocol::SceneEntry {
                number: s.number,
                name: s.name.clone(),
                current: current == Some(s.number),
                saved_at: s.saved_at,
            })
            .collect();
        scenes.sort_by_key(|e| e.number);
        Event::Scenes { scenes }
    }

    /// Synchronous read-only view of the local-library durability state
    /// (mode, status, monotonic revision, latch error, owned file bytes).
    /// Later FRB boot calls this before publishing the singleton and
    /// rejects on `error`/`uncertain` instead of serving a silently empty
    /// library.
    pub fn local_library_status(&self) -> local_library::LibraryStatusView {
        let h = self.lib_health.read().unwrap().clone();
        let bytes =
            local_library::storage_bytes(self.data_dir.as_deref()).unwrap_or_else(|error| {
                eprintln!("engine: storage accounting unavailable: {error}");
                None
            });
        local_library::LibraryStatusView {
            mode: h.mode.as_str().to_string(),
            status: h.state.as_str().to_string(),
            revision: self.lib_revision.load(Ordering::Relaxed),
            error: h.error.clone(),
            bytes,
        }
    }

    /// Current `local_library_status` event (explicit queries, device-mode
    /// snapshots, and post-commit broadcasts share this constructor).
    pub(super) fn local_library_status_message(&self) -> Event {
        let v = self.local_library_status();
        Event::LocalLibraryStatus {
            mode: v.mode,
            status: v.status,
            revision: v.revision,
            error: v.error,
            bytes: v.bytes,
        }
    }

    /// `local_library_status` for the attach/resume snapshots: `Some` in
    /// device mode only, so the default memory snapshots stay
    /// byte-identical for existing goldens (an explicit status query still
    /// answers in memory mode).
    pub(super) fn device_library_status(&self) -> Option<Event> {
        self.data_dir
            .as_ref()
            .map(|_| self.local_library_status_message())
    }

    /// Writes-blocked check: a latched `error`/`uncertain` library refuses
    /// every overwriting write until an explicit reset (or a restart, which
    /// re-reads the file). `reset` itself always passes.
    pub(super) fn ensure_library_writable(&self, is_reset: bool) -> anyhow::Result<()> {
        if is_reset {
            return Ok(());
        }
        let h = self.lib_health.read().unwrap();
        if h.state != LibraryStatus::Ready {
            anyhow::bail!(
                "local library {}: {}; writes blocked until reset or restart",
                h.state.as_str(),
                h.error.as_deref().unwrap_or("unavailable")
            );
        }
        Ok(())
    }

    /// r58 P2(2): capture pin for `Save`/`Capture` actions. Samples the
    /// engine connection generation + active driver + `reads_suspended`
    /// under the existing `sync_state` barrier (brief, never across
    /// `.await`). `gen`-only checks are insufficient: a capture sampled
    /// after `invalidate_connection` but before the active-driver swap
    /// would see the new generation on the old desk, so `suspended` must
    /// gate too. Pure-local actions (rename/delete/reset) never take a
    /// pin and are never blocked by desk state.
    ///
    /// Lock order: `lib_lock` (caller, async) -> `sync_state` (here,
    /// briefly) -> `active` (read). Matches the existing
    /// `sync_state` -> `active` direction in `reconcile_event` /
    /// `deferred_fill`; no path acquires `sync_state` while holding
    /// `scenes_lib`/`shows`/`active`, so no inversion.
    pub(super) fn sample_capture_pin(&self) -> (u64, String, bool) {
        let sync = self.sync_state.lock().unwrap();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let driver = self.active.read().unwrap().clone();
        let suspended = self.reads_suspended.load(Ordering::Acquire);
        drop(sync);
        (generation, driver, suspended)
    }

    /// Whether a library action reads the desk (`Save` captures the full
    /// plane, `Capture` snapshots it). All other actions are pure-local.
    pub(super) fn action_needs_capture_pin(action: &LibraryAction) -> bool {
        matches!(
            action,
            LibraryAction::Save { .. } | LibraryAction::Capture { .. }
        )
    }

    /// Recheck the capture pin and commit under the SAME `sync_state`
    /// barrier, so `invalidate_connection` cannot slip between the check
    /// and the publish. Call with [`Engine::lib_lock`] held; `None` pin
    /// (pure-local actions) commits exactly like before. Synchronous
    /// disk commit may hold the barrier briefly, never across `.await`.
    /// Preserves the retained session on failed connects: only a
    /// mid-capture desk change rejects, never a steady state.
    pub(super) fn commit_with_capture_guard(
        &self,
        pin: Option<(u64, String)>,
        numbered: HashMap<u16, StoredScene>,
        shows: Vec<Show>,
        current: Option<u16>,
    ) -> Result<CommitDisposition, String> {
        if let Some((pin_gen, pin_driver)) = pin {
            let sync = self.sync_state.lock().unwrap();
            let cur_gen = self.connection_gen.load(Ordering::Acquire);
            let cur_driver = self.active.read().unwrap().clone();
            let suspended = self.reads_suspended.load(Ordering::Acquire);
            if suspended {
                drop(sync);
                return Err(
                    "scene capture rejected: desk session changed (reads suspended)".to_string(),
                );
            }
            if cur_gen != pin_gen || cur_driver != pin_driver {
                drop(sync);
                return Err("scene capture rejected: desk changed during capture".to_string());
            }
            let res = self.commit_locked(numbered, shows, current);
            drop(sync);
            res
        } else {
            self.commit_locked(numbered, shows, current)
        }
    }

    /// Validate + durably commit candidate libraries, then swap memory.
    /// Call with [`Engine::lib_lock`] held. Pre-replace failures return
    /// `Err` with disk AND memory untouched; post-replace directory-sync /
    /// stat failures still swap (the rename landed) but latch `uncertain`
    /// and block further writes — never a silent rollback. Memory was
    /// swapped in both success cases, so callers publish success frames
    /// only on [`CommitDisposition::Committed`].
    pub(super) fn commit_locked(
        &self,
        numbered: HashMap<u16, StoredScene>,
        shows: Vec<Show>,
        current: Option<u16>,
    ) -> Result<CommitDisposition, String> {
        let mode = self.lib_health.read().unwrap().mode;
        if mode == LibraryMode::Device {
            let Some(data_dir) = self.data_dir.clone() else {
                return Err("local library device mode without a data dir".to_string());
            };
            let temp_id = self.lib_temp.fetch_add(1, Ordering::Relaxed);
            match local_library::commit_file(&data_dir, &numbered, &shows, temp_id) {
                Err(e) => Err(e),
                Ok(local_library::CommitResult::Committed { bytes }) => {
                    *self.scenes_lib.write().unwrap() = numbered;
                    *self.shows.write().unwrap() = shows;
                    *self.scenes_current.write().unwrap() = current;
                    let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
                    *self.lib_health.write().unwrap() = LibraryHealth {
                        mode,
                        state: LibraryStatus::Ready,
                        error: None,
                        bytes: Some(bytes),
                    };
                    Ok(CommitDisposition::Committed { revision })
                }
                Ok(local_library::CommitResult::Uncertain { bytes, error }) => {
                    *self.scenes_lib.write().unwrap() = numbered;
                    *self.shows.write().unwrap() = shows;
                    *self.scenes_current.write().unwrap() = current;
                    let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
                    *self.lib_health.write().unwrap() = LibraryHealth {
                        mode,
                        state: LibraryStatus::Uncertain,
                        error: Some(error.clone()),
                        bytes,
                    };
                    Ok(CommitDisposition::Uncertain { revision, error })
                }
            }
        } else {
            local_library::validate_candidate(&numbered, &shows)?;
            *self.scenes_lib.write().unwrap() = numbered;
            *self.shows.write().unwrap() = shows;
            *self.scenes_current.write().unwrap() = current;
            let revision = self.lib_revision.fetch_add(1, Ordering::Relaxed) + 1;
            debug_assert_eq!(
                self.lib_health.read().unwrap().state,
                LibraryStatus::Ready,
                "memory mode never latches"
            );
            Ok(CommitDisposition::Committed { revision })
        }
    }

    /// Full `shows` event: every show with its scenes.
    pub(super) fn shows_message(&self) -> Event {
        let shows = self.shows.read().unwrap();
        Event::Shows {
            shows: shows.iter().map(show_frame).collect(),
        }
    }
}

/// Trimmed non-empty arg (`"show"` / `"name"` for scene ops).
pub(super) fn clean_name(raw: &str, key: &str) -> anyhow::Result<String> {
    let clean = raw.trim().to_string();
    if clean.is_empty() {
        anyhow::bail!("scene command needs non-empty \"{key}\"");
    }
    Ok(clean)
}

/// One show in the `shows` event, in library order.
pub(super) fn show_frame(show: &Show) -> ShowFrame {
    ShowFrame {
        name: show.name.clone(),
        scenes: show.scenes.iter().map(scene_frame).collect(),
    }
}

/// One scene in the `shows` event.
pub(super) fn scene_frame(scene: &Scene) -> SceneFrame {
    if let Some(full) = scene.full() {
        return SceneFrame {
            name: full.name.clone(),
            main_fader: full.main_fader,
            strips: full
                .strips
                .iter()
                .map(|s| crate::protocol::SceneStrip {
                    ch: s.strip,
                    key: s.key.clone(),
                    name: s.name.clone(),
                    fader: s.fader,
                    muted: s.mute,
                })
                .collect(),
        };
    }
    SceneFrame {
        name: scene.name.clone(),
        main_fader: scene.main_fader,
        strips: scene
            .strips
            .iter()
            .map(|s| crate::protocol::SceneStrip {
                ch: s.strip.0,
                // Legacy named shows are index-based (core snapshots carry
                // no topology); the numbered-scene plane carries keys.
                key: None,
                name: s.name.clone(),
                fader: s.fader,
                muted: s.mute,
            })
            .collect(),
    }
}
