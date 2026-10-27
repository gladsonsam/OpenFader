//! Established native operations only. No native file codec or safe writer is implied.
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
        Some(json!({
            "authority":"mixer", "label":"DL32S current show", "connected":connected,
            "generation":generation, "revision":cache.library_revision,
            "capacity":99, "default_slot":0, "complete":false,
            "source":"desk_push", "show_identity":if connected {cache.library_show_info.clone()} else {None},
            "operations":if connected && cache.library_show_info.is_some() {vec!["inspect","read_show","refresh","recall","rename"]} else {vec!["inspect","read_show","refresh"]},
            "slots":if connected {cache.view.slots.iter().filter(|s| (1..=99).contains(&s.slot)).map(|s| json!({"slot":s.slot,"name":s.name})).collect::<Vec<_>>()} else {vec![]},
            "current":if connected {cache.view.current} else {None},
            "limitations":"Partial desk reports, not an empty/full show index. Show name, notes, images, dirty state and safes unavailable. Native store/delete and file import/export are not advertised: restorable file evidence required. Names use a conservative 63-byte UTF-8 envelope, not an asserted native UI character limit."
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
                let info = session.get_show_info_guarded(Duration::from_secs(5), &valid).await?;
                // Native handler accepts a >=4-word FileInfo reply. Keep the
                // complete reply opaque: no guessed CRC/name offsets or show codec.
                anyhow::ensure!((16..=4096).contains(&info.body.len()) && info.body.len() % 4 == 0,
                    "Native show identity reply unsupported; writes remain unavailable");
                anyhow::ensure!(valid(), "Native session/inventory changed during identity query");
                {
                    let mut cache = self.native.lock().unwrap();
                    if cache.library_show_info.as_ref() != Some(&info.body) {
                        // Previously observed names cannot be reassigned to a
                        // different (or previously unidentified) native show.
                        cache.view.slots.clear();
                        cache.view.current = None;
                        cache.view.received = false;
                        cache.library_revision += 1;
                    }
                    cache.library_show_info = Some(info.body);
                }
                Ok(receipt("confirmed", "Current show checked. The provider will check that the same show is loaded before a change. No backup was saved.", self.library_inventory().unwrap_or(Value::Null)))
            }
            Some("refresh") => {
                // This is explicitly consented transfer traffic, not a passive
                // inventory query or a saved/restorable backup. Never parse
                // undocumented zip data or promote optimistic transport caches.
                let result = session.download_file_guarded("Show.zip", Duration::from_secs(5), &authorize).await;
                let inventory = self.library_inventory().unwrap_or(Value::Null);
                Ok(match result {
                    Ok(_) if check() => receipt("confirmed", "Show transfer completed. Inventory still contains only desk-reported names; no backup file was saved.", inventory),
                    Ok(_) => receipt("uncertain", "Session/access changed during transfer. Inspect again; no replay.", Value::Null),
                    Err(e) => receipt(if dispatched.load(Ordering::Acquire) {"uncertain"} else {"rejected"}, format!("Show transfer unconfirmed: {e}"), inventory),
                })
            }
            Some("recall") | Some("rename") => {
                let slot = action["slot"].as_u64().filter(|s| (0..=99).contains(s))
                    .ok_or_else(|| anyhow::anyhow!("Snapshot identifier must be 0..99"))? as u32;
                anyhow::ensure!(slot == 0 || expected["slots"].as_array().is_some_and(|rows| rows.iter().any(|r| r["slot"] == slot)), "Snapshot has not been reported by this desk session");
                let name = if action["op"] == "rename" {
                    anyhow::ensure!(slot != 0, "Default cannot be renamed");
                    let name = action["name"].as_str().ok_or_else(||anyhow::anyhow!("Missing name"))?;
                    openfader_core::native_library::validate_name(name)?;
                    Some(name)
                } else { None };
                let identity = expected["show_identity"].as_array()
                    .ok_or_else(||anyhow::anyhow!("Read native show identity before mutation"))?;
                let fresh = session.get_show_info_guarded(Duration::from_secs(5), &valid).await?;
                let bytes: Vec<Value> = fresh.body.iter().map(|v| json!(v)).collect();
                if identity != &bytes {
                    {
                        let mut cache = self.native.lock().unwrap();
                        cache.library_show_info = None;
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
                    session.rename_show_snapshot_guarded(slot, name, &dispatch).await
                } else { session.recall_show_snapshot_guarded(slot, &dispatch).await };
                let mut pushes = pushes.into_inner().unwrap();
                if let Err(e) = result {
                    return Ok(receipt(if dispatched.load(Ordering::Acquire) {"uncertain"} else {"rejected"}, format!("Recall unconfirmed: {e}"), Value::Null));
                }
                let observed = tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        match pushes.recv().await {
                            Ok(PushEvent::SnapshotRecall(n)) if name.is_none() && n == slot => return true,
                            Ok(PushEvent::SnapshotRename { slot: n, name: observed }) if n == slot && name == Some(observed.as_str()) => return true,
                            Ok(_) => {},
                            Err(_) => return false,
                        }
                    }
                }).await.unwrap_or(false);
                Ok(if observed && check() {
                    receipt("confirmed", "The mixer reported the requested operation. This does not verify every mixer setting or the effect of safes.", self.library_inventory().unwrap_or(Value::Null))
                } else {
                    receipt("uncertain", "Operation was sent without a fresh matching desk report, or authority changed. Mixer may have changed. Inspect; do not automatically retry.", Value::Null)
                })
            }
            _ => anyhow::bail!("Native operation not advertised; save/delete require validated restorable native files"),
        }
    }
}
