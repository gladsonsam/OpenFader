//! Library command arms.

use super::*;

impl Engine {
    pub(super) async fn apply_library(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
        policy: &Policy,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            // Server-side show library: scenes survive UI reloads and
            // driver switches with no localStorage round-trip.
            Cmd::ListShows => Ok(Some(self.shows_message())),
            Cmd::CaptureScene { show, name } => {
                // Named capture through the SAME serialized candidate
                // commit as the durable API: same tags, same rendered
                // collection frames, plus the status frame.
                let action = LibraryAction::Capture { show, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.shows_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::RecallScene {
                show,
                name,
                fade_ms,
            } => {
                policy.allow_recall()?;
                let show = clean_name(&show, "show")?;
                let name = clean_name(&name, "name")?;
                let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
                let scene = {
                    let shows = self.shows.read().unwrap();
                    shows
                        .iter()
                        .find(|s| s.name == show)
                        .and_then(|s| s.get(&name))
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {name:?} in show {show:?}"))?
                };
                self.recall_on_active(&scene, fade_ms).await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                // Bypass locks, but report which locked strips were changed:
                // scene strips locked by someone other than the recaller.
                let mut overrode: Vec<u16> = Vec::new();
                for s in &scene.strips {
                    if let Some(holder) = self.lock_holder(s.strip.0) {
                        if holder != writer && !overrode.contains(&s.strip.0) {
                            overrode.push(s.strip.0);
                        }
                    }
                }
                overrode.sort_unstable();
                // Named-show recall provenance (same rule as numbered
                // recall): the sim model applies writes to real in-memory
                // state (`verified`); hardware fades are write-ACK only
                // (`accepted`, no per-param readback on this path).
                let authority = || {
                    Some(
                        if self.active_drv().is_hardware() {
                            "accepted"
                        } else {
                            "verified"
                        }
                        .to_string(),
                    )
                };
                for s in &scene.strips {
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "fader".to_string(),
                        value: json!(s.fader),
                        by: Some(writer.to_string()),
                        key: None,
                        authority: authority(),
                    });
                    out.push(Event::Param {
                        ch: s.strip.0,
                        field: "mute".to_string(),
                        value: json!(s.mute),
                        by: Some(writer.to_string()),
                        key: None,
                        authority: authority(),
                    });
                    for (field, val) in &s.params {
                        out.push(Event::Param {
                            ch: s.strip.0,
                            field: field.clone(),
                            value: param_value_json(val),
                            by: Some(writer.to_string()),
                            key: None,
                            authority: authority(),
                        });
                    }
                }
                self.push_state_with_by(writer, out).await;
                if overrode.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Event::Recalled {
                        overrode_locks: overrode,
                    }))
                }
            }
            Cmd::DeleteScene { show, name } => {
                let action = LibraryAction::DeleteNamed { show, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.shows_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneSave { n, name } => {
                // Numbered save through the SAME serialized candidate
                // commit: capture, validate, durably commit, then swap.
                let action = LibraryAction::Save { n, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneRecall { n, fade_ms, scope } => {
                policy.allow_recall()?;
                let n = validate_scene_n(n)?;
                let fade_ms = fade_ms.unwrap_or(DEFAULT_FADE_MS);
                // r58 P2(1): the numbered recall holds the existing async
                // `lib_lock` from the slot clone through the current-marker
                // publication, so a racing Clear/Delete/Reset (which commit
                // under the same lock) cannot interleave between the clone
                // and the marker and resurrect `current` on a deleted slot.
                // The fade itself runs under the lock — mutations waiting
                // out a recall fade is the accepted serialization (no new
                // audio controls). No `std` lock is held across `.await`;
                // write provenance and audio-echo ordering below are exact.
                let _lib_guard = self.lib_lock.lock().await;
                let scene = {
                    self.scenes_lib
                        .read()
                        .unwrap()
                        .get(&n)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {n:03}"))?
                };
                let overrode = self
                    .recall_full(&scene, fade_ms, scope.as_ref(), writer, out)
                    .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                *self.scenes_current.write().unwrap() = Some(n);
                self.push_state_with_by(writer, out).await;
                out.push(self.scenes_message());
                if overrode.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(Event::Recalled {
                        overrode_locks: overrode,
                    }))
                }
            }
            Cmd::SceneRename { n, name } => {
                let action = LibraryAction::Rename { n, name };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneDelete { n } => {
                let action = LibraryAction::Delete { n };
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::SceneClear => {
                let action = LibraryAction::ClearNumbered;
                match self.library_apply_action(&action).await {
                    Ok(CommitDisposition::Committed { .. }) => {
                        out.push(self.scenes_message());
                        out.push(self.local_library_status_message());
                        Ok(None)
                    }
                    Ok(CommitDisposition::Uncertain { error, .. }) => {
                        out.push(self.local_library_status_message());
                        anyhow::bail!("local library uncertain: {error}")
                    }
                    Err(e) => Err(e),
                }
            }
            Cmd::LocalLibraryStatus => Ok(Some(self.local_library_status_message())),
            Cmd::LocalLibraryMutate {
                request_id,
                expected_revision,
                action,
            } => {
                self.library_mutate(request_id, expected_revision, action, writer, out)
                    .await;
                Ok(None)
            }

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
