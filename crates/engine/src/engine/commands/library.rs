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
                let _lib_guard = self.lib_lock.lock().await;
                let scene = {
                    let shows = self.shows.read().unwrap();
                    shows
                        .iter()
                        .find(|s| s.name == show)
                        .and_then(|s| s.get(&name))
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("unknown scene {name:?} in show {show:?}"))?
                };
                // Named captures have no FX/routing plane. Reuse the pinned,
                // cache-coherent recall path for the values they do store.
                let stored = StoredScene {
                    number: 0,
                    name: scene.name,
                    saved_at: 0,
                    main_fader: scene.main_fader,
                    strips: scene
                        .strips
                        .into_iter()
                        .map(|s| StoredStrip {
                            strip: s.strip.0,
                            key: None,
                            name: s.name,
                            fader: s.fader,
                            mute: s.mute,
                            params: s
                                .params
                                .into_iter()
                                .map(|(id, v)| (id, param_value_json(&v)))
                                .collect(),
                        })
                        .collect(),
                    fx: Vec::new(),
                    routing: Vec::new(),
                    mute_groups: None,
                    automix: None,
                    output_taps: None,
                    monitor: None,
                    rta: None,
                };
                let overrode = self
                    .recall_full(&stored, fade_ms, None, writer, out)
                    .await?;
                self.note_write(writer);
                self.touch_holder_locks(writer);
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
