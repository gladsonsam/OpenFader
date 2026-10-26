//! Processing command arms.

use super::*;

impl Engine {
    pub(super) async fn apply_processing(
        &self,
        cmd: Cmd,
        writer: &str,
        out: &mut Vec<Event>,
        policy: &Policy,
    ) -> anyhow::Result<Option<Event>> {
        match cmd {
            Cmd::FxSetType { slot, fx_type } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                let fx_type = fx_type.trim().to_string();
                if drv.fx_flavor() == FxFlavor::Design {
                    openfader_core::fx::fx_type_meta(&fx_type)
                        .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
                } else if drv.fx_flavor() == FxFlavor::Numbered {
                    // Desk-native rack: names come from the desk type
                    // table behind `caps.fx`, the short code (`id`, e.g.
                    // `HALL`) or the full display name (`label`, e.g.
                    // `Hall Reverb`); the UI sends the code. Unknown here
                    // means unknown there.
                    if drv.fx_type_index_of(&fx_type).is_none() {
                        anyhow::bail!("unknown fx type {fx_type:?}");
                    }
                } else {
                    anyhow::bail!(
                        "{driver}: fx type select not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = drv.fx_get_type(slot).unwrap_or_default();
                drv.fx_set_type(slot, &fx_type).await?;
                self.offline_note(slot as u16, "fx/type", json!(from), json!(fx_type.clone()))
                    .await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::FxParam { slot, id, value } => {
                policy.allow_recall()?;
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    anyhow::bail!("fx param {value} out of range 0.0..=1.0");
                }
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() == FxFlavor::Design {
                    let from = drv
                        .fx_get_param_by_id(slot, &id)
                        .map(|v| json!(v))
                        .unwrap_or(Value::Null);
                    drv.fx_set_param_by_id(slot, &id, value)?;
                    self.offline_note(slot as u16, &format!("fx/{id}"), from, json!(value))
                        .await;
                    self.refresh_fx_cache().await;
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.fx_message().await);
                    Ok(None)
                } else {
                    drv.fx_set_param(slot, &id, value).await?;
                    self.note_write(writer);
                    self.touch_holder_locks(writer);
                    out.push(self.fx_message().await);
                    Ok(None)
                }
            }
            Cmd::FxBypass { slot, bypass } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() != FxFlavor::Design {
                    anyhow::bail!(
                        "{driver}: fx bypass not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = self
                    .fx_cache
                    .read()
                    .unwrap()
                    .iter()
                    .find(|r| r.slot == slot)
                    .map(|r| r.bypass)
                    .unwrap_or(!bypass);
                drv.fx_set_bypass(slot, bypass)?;
                self.offline_note(slot as u16, "fx/bypass", json!(from), json!(bypass))
                    .await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::FxOn { slot, on } => {
                policy.allow_recall()?;
                let driver = self.active_driver();
                let drv = self.drv(&driver);
                let max = drv.fx_slot_count();
                if slot == 0 || (slot as usize) > max {
                    anyhow::bail!("fx slot {slot} out of range 1..={max} on {driver}");
                }
                if drv.fx_flavor() != FxFlavor::Design {
                    anyhow::bail!(
                        "{driver}: fx on/off not supported (no verified mapping; UNVERIFIED-ON-RIG)"
                    );
                }
                let from = self
                    .fx_cache
                    .read()
                    .unwrap()
                    .iter()
                    .find(|r| r.slot == slot)
                    .map(|r| r.on)
                    .unwrap_or(!on);
                drv.fx_set_on(slot, on)?;
                self.offline_note(slot as u16, "fx/on", json!(from), json!(on))
                    .await;
                self.refresh_fx_cache().await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.fx_message().await);
                Ok(None)
            }
            Cmd::PatchSet {
                strip,
                group,
                index,
                key,
                target,
            } => {
                let strip = self.resolve_strip(strip, key.as_deref())?;
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                let drv = self.active_drv();
                let mut from = Value::Null;
                if !drv.routing_groups().is_empty() {
                    from = drv
                        .routing_source(StripId(strip), &target)
                        .await
                        .map(Value::String)
                        .unwrap_or(Value::Null);
                }
                let patch = drv
                    .patch_target_set(StripId(strip), &target, &group, index)
                    .await?;
                self.project_routing_patch(strip, writer, &patch, out).await;
                self.offline_note(
                    strip,
                    &if target.is_empty() {
                        "patch".to_string()
                    } else {
                        format!("patch/{target}")
                    },
                    from,
                    json!(patch.label),
                )
                .await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.routing_message().await);
                Ok(None)
            }
            Cmd::PatchClear { strip, key, target } => {
                let strip = self.resolve_strip(strip, key.as_deref())?;
                policy.allow_strip(StripId(strip))?;
                if let Some(holder) = self.lock_holder(strip) {
                    if holder != writer {
                        return Ok(Some(Self::locked_error(strip, &holder)));
                    }
                }
                let drv = self.active_drv();
                let mut from = Value::Null;
                if !drv.routing_groups().is_empty() {
                    from = drv
                        .routing_source(StripId(strip), &target)
                        .await
                        .map(Value::String)
                        .unwrap_or(Value::Null);
                }
                let patch = drv.patch_target_clear(StripId(strip), &target).await?;
                self.project_routing_patch(strip, writer, &patch, out).await;
                self.offline_note(
                    strip,
                    &if target.is_empty() {
                        "patch".to_string()
                    } else {
                        format!("patch/{target}")
                    },
                    from,
                    json!("—"),
                )
                .await;
                self.note_write(writer);
                self.touch_holder_locks(writer);
                out.push(self.routing_message().await);
                Ok(None)
            }
            // R2 desk file transfer (drivers with a file plane natively,
            // honestly unsupported elsewhere).
            Cmd::FileDownload { filename } => {
                let filename = check_filename(&filename)?;
                let bytes = self.active_drv().download_file(&filename).await?;
                self.note_write(writer);
                out.push(Event::File {
                    filename,
                    size: bytes.len(),
                    file_type: 0,
                    data: Some(STANDARD.encode(&bytes)),
                });
                Ok(None)
            }
            Cmd::FileUpload {
                filename,
                data,
                file_type,
            } => {
                policy.allow_upload()?;
                let filename = check_filename(&filename)?;
                let bytes = STANDARD
                    .decode(data.trim())
                    .map_err(|e| anyhow::anyhow!("invalid base64 data: {e}"))?;
                if bytes.len() > FILE_MAX_BYTES {
                    anyhow::bail!(
                        "file {filename:?} too large ({} > {FILE_MAX_BYTES} bytes)",
                        bytes.len()
                    );
                }
                self.active_drv()
                    .upload_file(&filename, &bytes, file_type)
                    .await?;
                self.note_write(writer);
                out.push(Event::File {
                    filename,
                    size: bytes.len(),
                    file_type,
                    data: None,
                });
                Ok(None)
            }

            _ => unreachable!("command routed to the wrong family"),
        }
    }
}
