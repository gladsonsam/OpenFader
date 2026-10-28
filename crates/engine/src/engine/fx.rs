//! Fx operations.

use super::*;

impl Engine {
    // ================= R2: FX =================

    /// `caps.fx` for the named driver: slot count + type tables (design
    /// racks carry the six design tables from `core::fx`; numbered
    /// desk-native racks carry their documented algorithms with labeled
    /// `parNN` params; rack-less drivers carry none, see README matrix).
    pub(super) fn fx_caps(&self, driver: &str) -> crate::protocol::FxCaps {
        let drv = self.drv(driver);
        let types = if drv.fx_flavor() == FxFlavor::Design {
            openfader_core::fx::FX_TYPES
                .iter()
                .map(|t| crate::protocol::FxTypeCaps {
                    slots: Vec::new(),
                    id: t.id.to_string(),
                    label: t.label.to_string(),
                    params: t
                        .params
                        .iter()
                        .map(|p| crate::protocol::FxParamCaps {
                            position: None,
                            choices: Vec::new(),
                            id: p.id.to_string(),
                            label: p.label.to_string(),
                            unit: p.unit.to_string(),
                            min: p.min,
                            max: p.max,
                            default: p.default,
                            scale: p.scale.to_string(),
                        })
                        .collect(),
                })
                .collect()
        } else if drv.fx_flavor() == FxFlavor::Numbered {
            drv.fx_type_caps()
        } else {
            Vec::new()
        };
        crate::protocol::FxCaps {
            slots: drv.fx_slot_count(),
            types,
            state_controls: drv.fx_flavor() == FxFlavor::Design,
        }
    }

    /// Current `fx` event from the engine-side cache + return levels.
    pub(super) async fn fx_message(&self) -> Event {
        self.fx_message_cached()
    }

    pub(super) fn fx_message_cached(&self) -> Event {
        let levels = self.active_drv().poll_meters().levels;
        let design = self.active_drv().fx_flavor() == FxFlavor::Design;
        let slots = self
            .fx_cache
            .read()
            .unwrap()
            .iter()
            .map(|s| {
                let level = if design {
                    let idx = 56 + ((s.slot as usize - 1) % 4);
                    levels.get(idx).copied().unwrap_or(0.0)
                } else {
                    0.0
                };
                FxSlotState {
                    slot: s.slot,
                    fx_type: s.fx_type.clone(),
                    on: s.on,
                    bypass: s.bypass,
                    ret_label: format!("Ret {}/{}", 2 * s.slot - 1, 2 * s.slot),
                    level,
                    params: s.params.clone(),
                }
            })
            .collect();
        Event::Fx { slots }
    }

    /// Rebuild the FX cache from the active driver (Design: full rack;
    /// numbered: live types + used params; elsewhere: honest placeholder
    /// rows, type unknown, no params).
    pub(super) async fn refresh_fx_cache(&self) {
        if self.active_drv().fx_flavor() == FxFlavor::Numbered {
            let generation = self.connection_gen.load(Ordering::Acquire);
            let drv = self.active_drv();
            for slot in 1..=drv.fx_slot_count() as u8 {
                let started = Instant::now();
                let Some(effect) = drv.fx_type_index(slot).await else {
                    continue;
                };
                if self.connection_gen.load(Ordering::Acquire) != generation {
                    return;
                }
                self.apply_received(
                    slot as u16,
                    "_fx_type",
                    &ParamValue::Float(effect as f32),
                    Instant::now(),
                    Some(started),
                );
                let param_count = drv.fx_type_param_count(effect).unwrap_or(0);
                for index in 0..param_count {
                    if self
                        .fx_cache
                        .read()
                        .unwrap()
                        .iter()
                        .find(|r| r.slot == slot)
                        .is_some_and(|r| r.params.contains_key(&format!("par{:02}", index + 1)))
                    {
                        continue;
                    }
                    if self.writes_in_flight.load(Ordering::Relaxed) != 0 {
                        self.fx_refill_requested.store(true, Ordering::Release);
                        return;
                    }
                    let started = Instant::now();
                    let result = drv
                        .get_fx_param_with_type(slot, index as u8, Some(effect))
                        .await;
                    if self.connection_gen.load(Ordering::Acquire) != generation {
                        return;
                    }
                    if !self
                        .fx_cache
                        .read()
                        .unwrap()
                        .iter()
                        .find(|r| r.slot == slot)
                        .is_some_and(|r| drv.fx_type_index_of(&r.fx_type) == Some(effect))
                    {
                        break;
                    }
                    if let Ok(value) = result {
                        self.apply_received(
                            slot as u16,
                            &format!("_fx_par{:02}", index + 1),
                            &ParamValue::Float(value),
                            Instant::now(),
                            Some(started),
                        );
                    }
                }
            }
            return;
        }
        let rows = self.read_fx_rows().await;
        *self.fx_cache.write().unwrap() = rows;
    }

    /// Connect-time FX seed: in-memory Design rows, honest placeholders
    /// otherwise. No FX wire reads precede levels/mutes/names/EQ; the
    /// deferred fill and FX poller supply types and params afterwards.
    pub(super) async fn seed_fx_cache(&self) {
        let started = Instant::now();
        let generation = self.connection_gen.load(Ordering::Acquire);
        let rows = self.read_fx_seed_rows().await;
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        let sync = self.sync_state.lock().unwrap();
        let mut cache = self.fx_cache.write().unwrap();
        if self.active_drv().fx_flavor() != FxFlavor::Numbered {
            *cache = rows;
            return;
        }
        for row in rows {
            if let Some(existing) = cache.iter_mut().find(|r| r.slot == row.slot) {
                if !sync
                    .received
                    .get(&(row.slot as u16, "_fx_type".into()))
                    .is_some_and(|(at, _)| *at > started)
                {
                    *existing = row;
                }
            } else {
                cache.push(row);
            }
        }
    }

    /// Seed rows without I/O: Design racks read memory; other racks keep
    /// placeholders until the deferred fill reads their actual state.
    pub(super) async fn read_fx_seed_rows(&self) -> Vec<FxSlotRt> {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Design {
            return drv
                .fx_snapshot()
                .iter()
                .enumerate()
                .map(|(i, s)| FxSlotRt {
                    slot: (i + 1) as u8,
                    fx_type: s.fx_type.clone(),
                    on: s.on,
                    bypass: s.bypass,
                    params: param_vec_to_map(&s.fx_type, &s.params),
                })
                .collect();
        }
        (1..=drv.fx_slot_count() as u8)
            .map(|slot| FxSlotRt {
                slot,
                fx_type: "—".to_string(),
                on: true,
                bypass: false,
                params: Default::default(),
            })
            .collect()
    }

    /// Synchronous FX cache rebuild for construction (the default driver
    /// is always sim here).
    pub(super) fn refresh_fx_cache_sync(&self) {
        let rows: Vec<FxSlotRt> = self
            .drv("sim")
            .fx_snapshot()
            .iter()
            .enumerate()
            .map(|(i, s)| FxSlotRt {
                slot: (i + 1) as u8,
                fx_type: s.fx_type.clone(),
                on: s.on,
                bypass: s.bypass,
                params: param_vec_to_map(&s.fx_type, &s.params),
            })
            .collect();
        *self.fx_cache.write().unwrap() = rows;
    }

    /// Read FX rows from the active driver (design racks fully mirrored;
    /// the desk-native numbered rack reads the live type + used params per
    /// slot over the verified type + `parNN` path
    /// best-effort, unreadable slots stay honest placeholders; rack-less
    /// drivers keep placeholders).
    pub(super) async fn read_fx_rows(&self) -> Vec<FxSlotRt> {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Design {
            return drv
                .fx_snapshot()
                .iter()
                .enumerate()
                .map(|(i, s)| FxSlotRt {
                    slot: (i + 1) as u8,
                    fx_type: s.fx_type.clone(),
                    on: s.on,
                    bypass: s.bypass,
                    params: param_vec_to_map(&s.fx_type, &s.params),
                })
                .collect();
        }
        if drv.fx_flavor() == FxFlavor::Numbered {
            let mut rows = Vec::new();
            for slot in 1..=drv.fx_slot_count() as u8 {
                rows.push(self.read_numbered_slot(slot).await);
            }
            return rows;
        }
        // Rack-less drivers are not polled (no verified state
        // path, writes are fire-and-forget there). Placeholder rows stay
        // placeholders: never inherit sim-seeded rows (or any unverified
        // state) as `prev`, the UI renders " " honestly.
        (1..=drv.fx_slot_count() as u8)
            .map(|slot| FxSlotRt {
                slot,
                fx_type: "—".to_string(),
                on: true,
                bypass: false,
                params: Default::default(),
            })
            .collect()
    }

    /// Best-effort live read of one numbered-rack FX slot: type first
    /// (unknown on error), then each used param of the loaded effect
    /// (`parNN` keys). On/off has no verified leaf and stays at the honest
    /// placeholder (`on: true`, `bypass: false`) so the UI keeps the toggle
    /// hidden. The type query runs once per slot: param reads reuse the
    /// known effect index (single-query path), so a slot costs 1+N queries.
    pub(super) async fn read_numbered_slot(&self, slot: u8) -> FxSlotRt {
        let drv = self.active_drv();
        let index = drv.fx_type_index(slot).await;
        let name = index
            .and_then(|i| drv.fx_type_name(i))
            .unwrap_or("—")
            .to_string();
        let mut params = BTreeMap::new();
        if let Some(type_idx) = index {
            let used = drv.fx_type_param_count(type_idx).unwrap_or(0);
            for i in 0..used as u8 {
                if let Ok(v) = drv.get_fx_param_with_type(slot, i, Some(type_idx)).await {
                    params.insert(format!("par{:02}", i + 1), v);
                }
            }
        }
        FxSlotRt {
            slot,
            fx_type: name,
            on: true,
            bypass: false,
            params,
        }
    }

    /// One FX poll read (round-robin over the design rack, or one live
    /// numbered-rack slot param per tick; silent unless a value changed).
    /// Other rack-less racks are not polled (no verified state path
    /// writes are fire-and-forget there).
    pub(super) async fn fx_refresh_one(&self) {
        let drv = self.active_drv();
        if drv.fx_flavor() == FxFlavor::Numbered {
            self.fx_refresh_one_numbered().await;
            return;
        }
        if drv.fx_flavor() != FxFlavor::Design {
            return;
        }
        let slots = drv.fx_snapshot();
        if slots.is_empty() {
            return;
        }
        let i = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize % slots.len();
        let slot = &slots[i];
        let changed = {
            let mut cache = self.fx_cache.write().unwrap();
            let Some(row) = cache.iter_mut().find(|r| r.slot == (i + 1) as u8) else {
                return;
            };
            let fresh = param_vec_to_map(&slot.fx_type, &slot.params);
            let diff = row.fx_type != slot.fx_type
                || row.on != slot.on
                || row.bypass != slot.bypass
                || row.params != fresh;
            if diff {
                row.fx_type = slot.fx_type.clone();
                row.on = slot.on;
                row.bypass = slot.bypass;
                row.params = fresh;
            }
            diff
        };
        if changed {
            self.broadcast(self.fx_message().await);
        }
    }

    /// One numbered-rack poll tick: a single live read (type when the cached
    /// row is still a placeholder, else one rotating param of the loaded
    /// effect). One query per tick keeps LAN load flat; desk-side moves
    /// converge within a few ticks and broadcasts only on change.
    pub(super) async fn fx_refresh_one_numbered(&self) {
        let count = self.active_drv().fx_slot_count();
        if count == 0 {
            return;
        }
        let generation = self.connection_gen.load(Ordering::Acquire);
        let n = self.fx_cursor.fetch_add(1, Ordering::Relaxed) as usize;
        let slot = (n % count) as u8 + 1;
        let drv = self.active_drv();
        let started = Instant::now();
        let Some(effect) = drv.fx_type_index(slot).await else {
            return;
        };
        if self.connection_gen.load(Ordering::Acquire) != generation {
            return;
        }
        self.apply_received(
            slot as u16,
            "_fx_type",
            &ParamValue::Float(effect as f32),
            Instant::now(),
            Some(started),
        );
        let used = drv.fx_type_param_count(effect).unwrap_or(0);
        if used == 0 {
            return;
        }
        let index = {
            let cache = self.fx_cache.read().unwrap();
            let Some(row) = cache.iter().find(|r| r.slot == slot) else {
                return;
            };
            (0..used)
                .find(|i| !row.params.contains_key(&format!("par{:02}", i + 1)))
                .map(|i| i as u8)
                .unwrap_or(((n / count) % used) as u8)
        };
        let started = Instant::now();
        if let Ok(value) = drv.get_fx_param_with_type(slot, index, Some(effect)).await {
            if self.connection_gen.load(Ordering::Acquire) == generation {
                // A type push received during the query invalidates this decode.
                let same_type = self
                    .fx_cache
                    .read()
                    .unwrap()
                    .iter()
                    .find(|r| r.slot == slot)
                    .is_some_and(|r| drv.fx_type_index_of(&r.fx_type) == Some(effect));
                if same_type {
                    self.apply_received(
                        slot as u16,
                        &format!("_fx_par{:02}", index + 1),
                        &ParamValue::Float(value),
                        Instant::now(),
                        Some(started),
                    );
                }
            }
        }
    }

    /// Apply saved FX to the pinned provider; a failed row is never success.
    pub(super) async fn recall_fx(
        &self,
        fx: &[StoredFx],
        drv: &dyn DynDriver,
        check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        for row in fx {
            if let Some(insert) = &row.insert {
                if insert.provider == drv.name() {
                    anyhow::ensure!(
                        drv.fx_insert_supported(),
                        "FX insert unavailable on {}",
                        drv.name()
                    );
                    anyhow::ensure!(
                        (1..=drv.fx_slot_count()).contains(&usize::from(row.slot)),
                        "invalid FX insert slot {}",
                        row.slot
                    );
                }
            }
        }
        for row in fx {
            check()?;
            drv.fx_apply_row(row.slot, &row.fx_type, row.on, row.bypass, &row.params)
                .await?;
            check()?;
            if let Some(insert) = row
                .insert
                .as_ref()
                .filter(|insert| insert.provider == drv.name())
            {
                drv.set_fx_insert(row.slot, insert.enabled).await?;
                check()?;
            }
        }
        Ok(())
    }
}

/// Map a sim slot's positional param vector onto design param ids.
pub(super) fn param_vec_to_map(fx_type: &str, values: &[f32]) -> BTreeMap<String, f32> {
    match openfader_core::fx::fx_type_meta(fx_type) {
        Some(meta) => meta
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id.to_string(),
                    values.get(i).copied().unwrap_or(p.default),
                )
            })
            .collect(),
        None => values
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("p{i}"), *v))
            .collect(),
    }
}
