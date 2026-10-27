//! Offline operations.

use super::*;

impl Engine {
    // ================= R2: offline editing =================

    /// Record a param-plane write into the offline change list (no-op
    /// unless offline editing is active). Same (strip, id) keeps its
    /// original `from` and takes the newest `to`; the stable key is
    /// captured alongside for desk-switch survival.
    pub(super) async fn offline_note(&self, strip: u16, id: &str, from: Value, to: Value) {
        let key = self.active_strip_key(strip);
        let mut off = self.offline.write().unwrap();
        if !off.active {
            return;
        }
        match off
            .changes
            .iter_mut()
            .find(|c| c.strip == strip && c.id == id)
        {
            Some(e) => {
                e.to = to;
                e.key = key;
            }
            None => off.changes.push(OfflineChange {
                strip,
                key,
                id: id.to_string(),
                from,
                to,
            }),
        }
    }

    /// Current `offline_diff` event (pending change list).
    pub(super) fn offline_diff_message(&self) -> Event {
        let off = self.offline.read().unwrap();
        Event::OfflineDiff {
            count: off.changes.len(),
            changes: off.changes.clone(),
        }
    }

    /// Push the pending offline change list to the active driver, in
    /// record order. Namespaced ids: `fx/<slot-param>` for FX writes,
    /// `patch` for routing writes, `main_fader` (strip 0xFFFF) for main.
    /// Design-strict, best-effort elsewhere; returns the applied Param echoes.
    pub(super) async fn offline_push_apply(
        &self,
        writer: &str,
        out: &mut Vec<Event>,
    ) -> anyhow::Result<()> {
        let changes = self.offline.read().unwrap().changes.clone();
        // Offline rows carry only a display label, not the XR18's three-field
        // route guard. Reject before applying or clearing any changes.
        if self.active_driver() == "xair"
            && changes
                .iter()
                .any(|c| c.id == "patch" || c.id.starts_with("patch/"))
        {
            anyhow::bail!("XR18 offline routing changes need a fresh routing selection");
        }
        let strict = self.active_drv().fx_flavor() == FxFlavor::Design;
        // Same provenance rule as recall: sim applies to real in-memory
        // state (`verified`); hardware pushes are write-ACK only
        // (`accepted`, no readback on this path).
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
        let mut failures = Vec::new();
        for c in &changes {
            let r = self.offline_apply_one(c).await;
            if let Err(e) = r {
                failures.push(format!("strip {} {}: {e:#}", c.strip, c.id));
                continue;
            }
            // Echo the resolved strip (keys may have moved the write).
            let ch = c
                .key
                .as_deref()
                .and_then(|k| {
                    let topo = self.driver_topology(&self.active_driver());
                    openfader_core::strip_index_for_key(&topo, k)
                })
                .map(|i| i as u16)
                .unwrap_or(c.strip);
            out.push(Event::Param {
                ch,
                field: c.id.clone(),
                value: c.to.clone(),
                by: Some(writer.to_string()),
                key: self.active_strip_key(ch),
                // Offline-push echo: provenance per the flag above.
                authority: authority(),
            });
        }
        {
            let mut off = self.offline.write().unwrap();
            off.changes.clear();
            off.pending_push = false;
        }
        self.refresh_fx_cache().await;
        if failures.is_empty() {
            Ok(())
        } else if strict {
            anyhow::bail!(
                "offline push had {} failure(s): {}",
                failures.len(),
                failures.join("; ")
            )
        } else {
            Ok(())
        }
    }

    /// Apply one recorded offline change to the active driver. Strip keys
    /// resolve first (desk-switch survival); the positional strip is the
    /// fallback for key-less rows.
    pub(super) async fn offline_apply_one(&self, c: &OfflineChange) -> anyhow::Result<()> {
        if c.id == "main_fader" {
            let Value::Number(n) = &c.to else {
                anyhow::bail!("main_fader push value must be a number")
            };
            let v = n.as_f64().unwrap_or(0.0) as f32;
            return self.drv_set_main(v).await;
        }
        let strip = c
            .key
            .as_deref()
            .and_then(|k| {
                let topo = self.driver_topology(&self.active_driver());
                openfader_core::strip_index_for_key(&topo, k)
            })
            .map(|i| i as u16)
            .unwrap_or(c.strip);
        if c.id == "patch" || c.id.starts_with("patch/") {
            let target = c.id.strip_prefix("patch/").unwrap_or("");
            let drv = self.active_drv();
            if drv.routing_groups().is_empty() {
                anyhow::bail!("patch push not supported here");
            }
            let Value::String(label) = &c.to else {
                anyhow::bail!("patch push value must be a label")
            };
            if label == "—" {
                return drv
                    .patch_target_clear(StripId(strip), target)
                    .await
                    .map(|_| ());
            }
            let Some((group, index)) = parse_source(label) else {
                anyhow::bail!("unparseable patch label {label:?}")
            };
            drv.patch_target_set(StripId(strip), target, &group, index)
                .await?;
            return Ok(());
        }
        if let Some(param) = c.id.strip_prefix("fx/") {
            return self.active_drv().fx_push(c.strip as u8, param, &c.to).await;
        }
        let pv = json_to_param_value(&c.to)?;
        self.drv_set_param(StripId(strip), &c.id, pv).await
    }

    // ================= R2: driver write helpers =================

    pub(super) async fn drv_set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.active_drv().set_fader(strip, value).await
    }

    pub(super) async fn drv_get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.active_drv().get_fader(strip).await
    }

    pub(super) async fn drv_set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.active_drv().set_mute(strip, muted).await
    }

    pub(super) async fn drv_set_param(
        &self,
        strip: StripId,
        id: &str,
        pv: ParamValue,
    ) -> anyhow::Result<()> {
        self.active_drv().set_param(strip, id, pv).await
    }

    pub(super) async fn drv_set_main(&self, value: f32) -> anyhow::Result<()> {
        self.active_drv().set_main_fader(value).await
    }
}
