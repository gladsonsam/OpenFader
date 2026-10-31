//! Guarded native operations and lossless stored-file inspection.
//! No current-edit backup or safe restore is implied.
use crate::Dl32sDriver;
use openfader_core::native_library::{receipt, Check};
use openfader_transport::mackie::PushEvent;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

impl Dl32sDriver {
    pub(super) fn library_inventory(&self) -> Option<Value> {
        let generation = self.generation.load(Ordering::Acquire);
        let cache = self.native.lock().ok()?;
        let connected = cache.generation == generation && cache.view.connected;
        let slots = if connected {
            if let Some(index) = &cache.library_index {
                index["slots"].as_array().cloned().unwrap_or_default()
            } else {
                cache
                    .view
                    .slots
                    .iter()
                    .filter(|s| (1..=99).contains(&s.slot))
                    .map(|s| json!({"slot":s.slot,"name":s.name}))
                    .collect()
            }
        } else {
            vec![]
        };
        Some(json!({
            "authority":"mixer", "label":"DL32S current show", "connected":connected,
            "generation":generation, "revision":cache.library_revision,
            "capacity":99, "default_slot":0, "complete":connected && cache.library_index.is_some(),
            "source":if cache.library_index.is_some() {"stored_show"} else {"desk_push"},
            "stored_index":if connected {cache.library_index.clone()} else {None}, "show_identity":if connected {cache.library_show_info.clone()} else {None},
            "operations":if connected && cache.library_show_info.is_some() {if cache.library_index.is_some() {vec!["inspect","read_show","refresh","export_file","inspect_file","recall","rename","store","replace","delete"]} else {vec!["inspect","read_show","refresh","export_file","inspect_file","recall","rename"]}} else {vec!["inspect","read_show","refresh","export_file","inspect_file"]},
            "slots":slots,
            "current":if connected {cache.view.current} else {None},
            "limitations":"Stored index is complete only after explicit ZIP refresh. Store captures native current state; readback verifies stored files, not every current edit or safes. Default 0 is protected; delete compacts positions. Firmware restore remains unproved. Export preserves the original stored-show archive; importing inspects a file without writing the mixer. Names use a conservative 63-byte UTF-8 envelope, not an asserted native UI character limit."
        }))
    }

    pub(super) async fn execute_library(
        &self,
        action: &Value,
        expected: &Value,
        check: Check<'_>,
    ) -> anyhow::Result<Value> {
        // Share the processor lane so native recall cannot run inside a DSP
        // transaction. Session lock also pins the actual socket across dispatch.
        let _lane = self.output_write.lock().await;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked");
        let valid = || check() && self.library_inventory().as_ref() == Some(expected);
        anyhow::ensure!(
            valid(),
            "Native inventory/session changed; nothing dispatched"
        );
        let dispatched = AtomicBool::new(false);
        let authorize = || {
            let ok = valid();
            if ok {
                dispatched.store(true, Ordering::Release);
            }
            ok
        };
        match action["op"].as_str() {
            Some("read_show") => {
                let info = session
                    .get_show_info_guarded(Duration::from_secs(5), &valid)
                    .await?;
                // Native handler accepts a >=4-word FileInfo reply. Keep the
                // complete reply opaque: no guessed CRC/name offsets or show codec.
                anyhow::ensure!(
                    (16..=4096).contains(&info.body.len()) && info.body.len() % 4 == 0,
                    "Native show identity reply unsupported; writes remain unavailable"
                );
                anyhow::ensure!(
                    valid(),
                    "Native session/inventory changed during identity query"
                );
                {
                    let mut cache = self.native.lock().unwrap();
                    if cache.library_show_info.as_ref() != Some(&info.body) {
                        // Previously observed names cannot be reassigned to a
                        // different (or previously unidentified) native show.
                        cache.library_index = None;
                        cache.view.slots.clear();
                        cache.view.current = None;
                        cache.view.received = false;
                        cache.library_revision += 1;
                    }
                    cache.library_show_info = Some(info.body);
                }
                Ok(receipt("confirmed", "Current show checked. The provider will check that the same show is loaded before a change. No backup was saved.", self.library_inventory().unwrap_or(Value::Null)))
            }
            Some("inspect_file") => {
                let bytes = action["bytes"]
                    .as_array()
                    .filter(|b| b.len() <= 32 * 1024 * 1024)
                    .ok_or_else(|| anyhow::anyhow!("Missing/bounded native archive bytes"))?;
                let bytes = bytes
                    .iter()
                    .map(|v| {
                        v.as_u64()
                            .and_then(|n| u8::try_from(n).ok())
                            .ok_or_else(|| anyhow::anyhow!("Invalid native archive byte"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let archive = crate::native_archive::NativeArchive::parse(bytes)?;
                let index = crate::native_show::inspect(&archive)?;
                anyhow::ensure!(valid(), "Native file inspection context changed");
                let mut result = receipt("confirmed", "Native stored-show file inspected. Original bytes preserved; no mixer write or restore was performed. Current-edit restoration remains unverified.", expected.clone());
                result["file_index"] = index;
                Ok(result)
            }
            Some("refresh") | Some("export_file") => {
                // Explicit read-only traffic. Compare opaque identities around
                // the download; do not turn a historical file into a live index.
                let before = session
                    .get_show_info_guarded(Duration::from_secs(5), &valid)
                    .await?;
                anyhow::ensure!(
                    (16..=4096).contains(&before.body.len()) && before.body.len().is_multiple_of(4),
                    "Unsupported native show identity"
                );
                let result: anyhow::Result<(Vec<u8>, Value)> = async {
                    let bytes = session
                        .download_file_guarded("Show.zip", Duration::from_secs(5), &authorize)
                        .await?;
                    let after = session
                        .get_show_info_guarded(Duration::from_secs(5), &valid)
                        .await?;
                    anyhow::ensure!(
                        before.body == after.body && valid(),
                        "Native show/session/inventory changed during download"
                    );
                    let archive = crate::native_archive::NativeArchive::parse(bytes)?;
                    let index = crate::native_show::inspect(&archive)?;
                    Ok((archive.bytes().to_vec(), index))
                }
                .await;
                Ok(match result {
                    Ok((bytes, index)) if valid() => {
                        {
                            let mut cache = self.native.lock().unwrap();
                            // Publish only while the exact inventory and connection that
                            // authorized the read are still current. Pushes can race the
                            // final validation above.
                            anyhow::ensure!(
                                cache.view.connected
                                    && cache.generation == self.generation.load(Ordering::Acquire)
                                    && expected["generation"].as_u64() == Some(cache.generation)
                                    && expected["revision"].as_u64()
                                        == Some(cache.library_revision),
                                "Native session/inventory changed during publication"
                            );
                            cache.view.slots = index["slots"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|row| {
                                    Some(openfader_core::NativeSnapshotSlot {
                                        slot: u32::try_from(row["slot"].as_u64()?).ok()?,
                                        name: row["name"].as_str()?.to_owned(),
                                    })
                                })
                                .collect();
                            cache.view.received = true;
                            cache.view.complete = true;
                            cache.view.error = None;
                            cache.library_show_info = Some(before.body);
                            cache.library_index = Some(index.clone());
                            cache.library_revision += 1;
                        }
                        let inventory = self.library_inventory().unwrap_or(Value::Null);
                        let mut result = receipt("confirmed", "Stored-show archive downloaded and CRC/index checked. Current unsaved edits and exact restoration are NOT certified; this is not a full native backup.", inventory);
                        result["file_index"] = index;
                        if action["op"] == "export_file" {
                            result["file_bytes"] = json!(bytes);
                        }
                        result
                    }
                    Ok(_) => receipt(
                        "uncertain",
                        "Session/access changed during transfer. Inspect again; no replay.",
                        Value::Null,
                    ),
                    Err(e) => receipt(
                        if dispatched.load(Ordering::Acquire) {
                            "uncertain"
                        } else {
                            "rejected"
                        },
                        format!("Show archive unconfirmed: {e}"),
                        Value::Null,
                    ),
                })
            }
            Some("store") | Some("replace") | Some("delete") => {
                let index = &expected["stored_index"];
                let rows = index["slots"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Refresh the complete stored index first"))?;
                let append = action["op"] == "store";
                let delete = action["op"] == "delete";
                let slot = if append {
                    rows.len() as u32 + 1
                } else {
                    action["slot"]
                        .as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| anyhow::anyhow!("Invalid snapshot position"))?
                };
                anyhow::ensure!(
                    (1..=99).contains(&slot) && (append || slot as usize <= rows.len()),
                    "Protected/full/unknown native position"
                );
                let before_info = session
                    .get_show_info_guarded(Duration::from_secs(5), &valid)
                    .await?;
                anyhow::ensure!(
                    json!(before_info.body) == expected["show_identity"],
                    "Native show identity changed; refresh first"
                );
                let before = crate::native_archive::NativeArchive::parse(
                    session
                        .download_file_guarded("Show.zip", Duration::from_secs(5), &valid)
                        .await?,
                )?;
                let fresh = crate::native_show::inspect(&before)?;
                anyhow::ensure!(
                    same_stored_index(&fresh, index) && valid(),
                    "Stored show changed; nothing dispatched"
                );
                let identity = session
                    .get_show_info_guarded(Duration::from_secs(5), &valid)
                    .await?;
                anyhow::ensure!(
                    identity.body == before_info.body,
                    "Native identity changed before dispatch"
                );
                // After dispatch our own relays may advance the revision. The
                // access/session callback remains binding; readback proves the
                // requested index transition rather than trusting an ACK.
                let outcome: anyhow::Result<(Value, Vec<u8>)> = async {
                    session
                        .mutate_snapshot_guarded(slot, delete, &authorize)
                        .await?;
                    let read_identity = session
                        .get_show_info_guarded(Duration::from_secs(5), check)
                        .await?;
                    let after = crate::native_archive::NativeArchive::parse(
                        session
                            .download_file_guarded("Show.zip", Duration::from_secs(5), check)
                            .await?,
                    )?;
                    let updated = crate::native_show::inspect(&after)?;
                    verify_transition(&before, &fresh, &after, &updated, slot, delete, append)?;
                    let info = session
                        .get_show_info_guarded(Duration::from_secs(5), check)
                        .await?;
                    anyhow::ensure!(
                        check() && info.body == read_identity.body,
                        "Native authority/show changed during readback"
                    );
                    Ok((updated, info.body))
                }
                .await;
                // No cached positions survive a failed/compacting mutation.
                let mut cache = self.native.lock().unwrap();
                cache.library_index = None;
                cache.library_show_info = None;
                cache.view.slots.clear();
                cache.view.current = None;
                cache.library_revision += 1;
                match outcome {
                    Ok((updated, info)) => {
                        cache.library_index = Some(updated);
                        cache.library_show_info = Some(info);
                        drop(cache);
                        Ok(receipt("confirmed", "Stored snapshot index and unchanged snapshot bytes verified by ZIP readback. Native current-edit coverage and firmware restart persistence are not certified. Inspect positions again after delete; no automatic retry.", self.library_inventory().unwrap_or(Value::Null)))
                    }
                    Err(error) => Ok(receipt(
                        if dispatched.load(Ordering::Acquire) {
                            "uncertain"
                        } else {
                            "rejected"
                        },
                        format!(
                            "Snapshot change unconfirmed: {error}. Inspect again; do not replay."
                        ),
                        Value::Null,
                    )),
                }
            }
            Some("recall") | Some("rename") => {
                let slot = action["slot"]
                    .as_u64()
                    .filter(|s| (0..=99).contains(s))
                    .ok_or_else(|| anyhow::anyhow!("Snapshot identifier must be 0..99"))?
                    as u32;
                anyhow::ensure!(
                    slot == 0
                        || expected["slots"]
                            .as_array()
                            .is_some_and(|rows| rows.iter().any(|r| r["slot"] == slot)),
                    "Snapshot has not been reported by this desk session"
                );
                let name = if action["op"] == "rename" {
                    anyhow::ensure!(slot != 0, "Default cannot be renamed");
                    let name = action["name"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("Missing name"))?;
                    openfader_core::native_library::validate_name(name)?;
                    Some(name)
                } else {
                    None
                };
                let identity = expected["show_identity"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("Read native show identity before mutation"))?;
                let baseline = if expected["stored_index"].is_object() {
                    let archive = crate::native_archive::NativeArchive::parse(
                        session
                            .download_file_guarded("Show.zip", Duration::from_secs(5), &valid)
                            .await?,
                    )?;
                    let inspected = crate::native_show::inspect(&archive)?;
                    anyhow::ensure!(
                        same_stored_index(&inspected, &expected["stored_index"]),
                        "Stored snapshot index changed; refresh first"
                    );
                    Some(archive)
                } else {
                    None
                };
                let fresh = session
                    .get_show_info_guarded(Duration::from_secs(5), &valid)
                    .await?;
                let bytes: Vec<Value> = fresh.body.iter().map(|v| json!(v)).collect();
                if identity != &bytes {
                    {
                        let mut cache = self.native.lock().unwrap();
                        cache.library_show_info = None;
                        cache.library_index = None;
                        cache.view.slots.clear();
                        cache.view.current = None;
                        cache.view.received = false;
                        cache.library_revision += 1;
                    }
                    return Ok(receipt("rejected", "Native show identity changed. Nothing was mutated. Inspect and read identity again.", Value::Null));
                }
                // A pre-existing current marker must never confirm a new recall.
                let pushes = std::sync::Mutex::new(session.subscribe_pushes());
                let dispatch = || {
                    // Drop reports received while this operation waited for the
                    // socket lane. They cannot confirm this dispatch.
                    while pushes.lock().unwrap().try_recv().is_ok() {}
                    authorize()
                };
                let result = if let Some(name) = name {
                    session
                        .rename_show_snapshot_guarded(slot, name, &dispatch)
                        .await
                } else {
                    session.recall_show_snapshot_guarded(slot, &dispatch).await
                };
                let mut pushes = pushes.into_inner().unwrap();
                if let Err(e) = result {
                    return Ok(receipt(
                        if dispatched.load(Ordering::Acquire) {
                            "uncertain"
                        } else {
                            "rejected"
                        },
                        format!("Recall unconfirmed: {e}"),
                        Value::Null,
                    ));
                }
                let mut observed = tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        match pushes.recv().await {
                            Ok(PushEvent::SnapshotRecall(n)) if name.is_none() && n == slot => {
                                return true
                            }
                            Ok(PushEvent::SnapshotRename {
                                slot: n,
                                name: observed,
                            }) if n == slot && name == Some(observed.as_str()) => return true,
                            Ok(_) => {}
                            Err(_) => return false,
                        }
                    }
                })
                .await
                .unwrap_or(false);
                if let (Some(name), Some(before)) = (name, baseline) {
                    let verified: anyhow::Result<(Value, Vec<u8>)> = async {
                        let read_identity = session
                            .get_show_info_guarded(Duration::from_secs(5), check)
                            .await?;
                        let after = crate::native_archive::NativeArchive::parse(
                            session
                                .download_file_guarded("Show.zip", Duration::from_secs(5), check)
                                .await?,
                        )?;
                        let index = crate::native_show::inspect(&after)?;
                        let mut wanted = expected["stored_index"].clone();
                        wanted["slots"][slot as usize - 1]["name"] = json!(name);
                        anyhow::ensure!(
                            wanted["show_id"] == index["show_id"]
                                && wanted["slots"] == index["slots"],
                            "Rename index readback mismatch"
                        );
                        for (filename, bytes) in before.entries() {
                            if filename != "Show.dat" {
                                anyhow::ensure!(
                                    after.entries().get(filename) == Some(bytes),
                                    "Rename changed unrelated file bytes"
                                );
                            }
                        }
                        let identity = session
                            .get_show_info_guarded(Duration::from_secs(5), check)
                            .await?;
                        anyhow::ensure!(
                            check() && identity.body == read_identity.body,
                            "Rename authority/show changed during readback"
                        );
                        Ok((index, identity.body))
                    }
                    .await;
                    match verified {
                        Ok((index, identity)) => {
                            let mut cache = self.native.lock().unwrap();
                            cache.library_index = Some(index);
                            cache.library_show_info = Some(identity);
                            cache.library_revision += 1;
                            observed = true;
                        }
                        Err(_) => observed = false,
                    }
                }
                Ok(if observed && check() {
                    receipt("confirmed", "The mixer reported the requested operation. This does not verify every mixer setting or the effect of safes.", self.library_inventory().unwrap_or(Value::Null))
                } else {
                    receipt("uncertain", "Operation was sent without a fresh matching desk report, or authority changed. Mixer may have changed. Inspect; do not automatically retry.", Value::Null)
                })
            }
            _ => anyhow::bail!("Native operation not advertised"),
        }
    }
}

fn same_stored_index(a: &Value, b: &Value) -> bool {
    a["show_id"] == b["show_id"] && a["slots"] == b["slots"] && a["members"] == b["members"]
}

fn verify_transition(
    before: &crate::native_archive::NativeArchive,
    old: &Value,
    after: &crate::native_archive::NativeArchive,
    new: &Value,
    slot: u32,
    delete: bool,
    append: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        old["show_id"] == new["show_id"] && old["name"] == new["name"],
        "Stored show identity changed"
    );
    let old_rows = old["slots"].as_array().unwrap();
    let new_rows = new["slots"].as_array().unwrap();
    let expected_count = if delete {
        old_rows.len() - 1
    } else {
        old_rows.len() + usize::from(append)
    };
    anyhow::ensure!(
        new_rows.len() == expected_count,
        "Stored count did not match mutation"
    );
    for row in old_rows {
        let position = row["slot"].as_u64().unwrap() as u32;
        if position == slot {
            continue;
        }
        let new_position = if delete && position > slot {
            position - 1
        } else {
            position
        };
        let next = &new_rows[new_position as usize - 1];
        anyhow::ensure!(
            row["name"] == next["name"]
                && row["file_number"] == next["file_number"]
                && row["crc"] == next["crc"],
            "Unrelated stored snapshot changed"
        );
        let filename = format!("Snapshot{}.dat", row["file_number"].as_u64().unwrap());
        anyhow::ensure!(
            before.entries().get(&filename) == after.entries().get(&filename),
            "Unrelated snapshot bytes changed"
        );
    }
    // Only Show.dat and the selected snapshot payload may change. Include
    // unreferenced snapshots and opaque members in preservation checks too.
    let target_file = if append {
        None
    } else {
        Some(format!(
            "Snapshot{}.dat",
            old_rows[slot as usize - 1]["file_number"].as_u64().unwrap()
        ))
    };
    for (filename, bytes) in before.entries() {
        if filename != "Show.dat" && Some(filename) != target_file.as_ref() {
            anyhow::ensure!(
                after.entries().get(filename) == Some(bytes),
                "Unrelated archive member bytes changed"
            );
        }
    }
    if !delete {
        let target = &new_rows[slot as usize - 1];
        if !append {
            anyhow::ensure!(
                target["name"] == old_rows[slot as usize - 1]["name"],
                "Stored snapshot name changed unexpectedly"
            );
            let filename = format!("Snapshot{}.dat", target["file_number"].as_u64().unwrap());
            let original = target_file.as_ref().unwrap();
            anyhow::ensure!(
                after.entries().get(&filename) != before.entries().get(original),
                "Unchanged snapshot payload cannot prove current state was stored"
            );
        }
        // The inspector verifies the target's file geometry and both CRCs.
        anyhow::ensure!(target["slot"] == slot, "Target position missing");
    }
    Ok(())
}

/// One read-only catalogue load per connection; transport ownership is shared
/// without holding the driver's command/session mutex during file I/O.
pub(super) fn start_inventory_load(
    session: std::sync::Arc<openfader_transport::mackie::MackieSession>,
    cache: std::sync::Arc<std::sync::Mutex<crate::native::NativeCache>>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    expected_generation: u64,
) {
    tokio::spawn(async move {
        let revision = cache.lock().unwrap().library_revision;
        let valid = || {
            generation.load(Ordering::Acquire) == expected_generation
                && session.is_alive()
                && cache.lock().is_ok_and(|c| {
                    c.generation == expected_generation
                        && c.view.connected
                        && c.library_revision == revision
                })
        };
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let before = session
                .get_show_info_guarded(Duration::from_secs(5), &valid)
                .await?;
            anyhow::ensure!(
                (16..=4096).contains(&before.body.len()) && before.body.len().is_multiple_of(4),
                "unsupported show identity"
            );
            let bytes = session
                .download_file_guarded("Show.zip", Duration::from_secs(20), &valid)
                .await?;
            let after = session
                .get_show_info_guarded(Duration::from_secs(5), &valid)
                .await?;
            anyhow::ensure!(
                before.body == after.body && valid(),
                "show changed while reading catalogue"
            );
            let archive = crate::native_archive::NativeArchive::parse(bytes)?;
            let index = crate::native_show::inspect(&archive)?;
            Ok::<_, anyhow::Error>((before.body, index))
        })
        .await;
        let mut c = cache.lock().unwrap();
        if generation.load(Ordering::Acquire) != expected_generation
            || c.generation != expected_generation
            || !c.view.connected
            || c.library_revision != revision
        {
            return;
        }
        match result {
            Ok(Ok((identity, index))) => {
                publish_catalogue(&mut c, expected_generation, revision, identity, index);
            }
            Ok(Err(e)) => {
                c.view.error = Some(format!(
                    "Snapshot catalogue: {e}. Refresh the native library to retry."
                ))
            }
            Err(_) => {
                c.view.error = Some(
                    "Snapshot catalogue timed out. Refresh the native library to retry.".into(),
                )
            }
        }
    });
}

fn publish_catalogue(
    c: &mut crate::native::NativeCache,
    generation: u64,
    revision: u64,
    identity: Vec<u8>,
    index: Value,
) -> bool {
    if c.generation != generation || !c.view.connected || c.library_revision != revision {
        return false;
    }
    c.view.slots = index["slots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            Some(openfader_core::NativeSnapshotSlot {
                slot: u32::try_from(r["slot"].as_u64()?).ok()?,
                name: r["name"].as_str()?.to_owned(),
            })
        })
        .collect();
    c.view.received = true;
    c.view.complete = true;
    c.view.error = None;
    c.library_show_info = Some(identity);
    c.library_index = Some(index);
    c.library_revision += 1;
    true
}
#[cfg(test)]
mod catalogue_tests {
    use super::*;
    #[test]
    fn catalogue_names_publish_only_to_unchanged_live_session() {
        let mut c = crate::native::NativeCache {
            generation: 7,
            ..Default::default()
        };
        c.view.connected = true;
        let index = json!({"slots":[{"slot":1,"name":"Sunday"},{"slot":2,"name":"Vocals 🎤"}]});
        assert!(!publish_catalogue(&mut c, 6, 0, vec![], index.clone()));
        assert!(!publish_catalogue(&mut c, 7, 1, vec![], index.clone()));
        assert!(c.view.slots.is_empty());
        assert!(publish_catalogue(&mut c, 7, 0, vec![1; 16], index.clone()));
        assert_eq!(c.view.slots[1].name, "Vocals 🎤");
        assert!(c.view.complete && c.view.received);
        assert!(!publish_catalogue(&mut c, 7, 0, vec![], index.clone()));
        c.view.connected = false;
        assert!(!publish_catalogue(&mut c, 7, 1, vec![], index));
    }
}
