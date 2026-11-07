//! Desk-evidenced parameter cache ingestion and event reconciliation.

use super::*;

impl Engine {
    fn invalidate_preamp(&self, ch: u16, received: Instant, source: bool) {
        let candidates = self.active_drv().preamp_invalidation_fields(source);
        let mut fields = Vec::new();
        {
            let mut sync = self.sync_state.lock().unwrap();
            if received < sync.epoch {
                return;
            }
            let mut cache = self.param_cache.write().unwrap();
            for &field in candidates {
                let field = field.to_string();
                let key = (ch, field.clone());
                if sync
                    .received
                    .get(&key)
                    .is_some_and(|(at, _)| *at > received)
                {
                    continue;
                }
                sync.revision += 1;
                let revision = sync.revision;
                // Advance the barrier even if there was no cached value: a
                // pre-invalidation query must not repopulate stale preamp data.
                sync.received.insert(key, (received, revision));
                if let Some(values) = cache.get_mut(&ch) {
                    values.remove(&field);
                }
                fields.push(field);
            }
        }
        if fields.is_empty() {
            return;
        }
        let driver = self.active_driver();
        {
            let mut queue = self.focus_queue.write().unwrap();
            for field in &fields {
                if !queue
                    .iter()
                    .any(|(d, s, p, _)| d == &driver && *s == ch && p == field)
                {
                    queue.push_front((driver.clone(), ch, field.clone(), String::new()));
                }
            }
        }
        self.broadcast(Event::ParamInvalidated { ch, fields });
    }
    // ================= R2: desk-broadcast ingest =================

    /// Apply one desk-originated update (broadcast ingest): same
    /// compare-and-broadcast as a poll read, minus the I/O.
    pub(super) fn apply_inbound(&self, ch: u16, param: &str, pv: &ParamValue) {
        self.apply_received(ch, param, pv, Instant::now(), None);
    }

    pub(super) fn apply_received(
        &self,
        ch: u16,
        param: &str,
        pv: &ParamValue,
        received: Instant,
        query_started: Option<Instant>,
    ) {
        if param.starts_with("_node/") {
            return;
        }
        if param == "_fx_dirty" {
            self.fx_tick.store(0, Ordering::Release);
            return;
        }
        if param == "_preamp_dirty" || param == "_preamp_routes_dirty" {
            let count = self.active_drv().preamp_invalidation_count();
            if count == 0 {
                return;
            }
            if ch == u16::MAX {
                for strip in 0..count.min(u16::MAX as usize) as u16 {
                    self.invalidate_preamp(strip, received, param == "_preamp_routes_dirty");
                }
            } else if usize::from(ch) < count {
                self.invalidate_preamp(ch, received, true);
            }
            return;
        }
        if param == "source" && self.active_drv().preamp_invalidation_count() > 0 {
            let previous = self
                .param_cache
                .read()
                .unwrap()
                .get(&ch)
                .and_then(|values| values.get("source"))
                .cloned();
            if previous.is_some_and(|old| old != param_value_json(pv)) {
                self.invalidate_preamp(ch, received, true);
            }
        }
        if param == "_sync_dirty" {
            if let ParamValue::Text(suffix) = pv {
                let container = suffix
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or(suffix);
                let container = if [
                    "config", "preamp", "gate", "dyn", "eq", "mix", "grp", "insert", "automix",
                ]
                .contains(&suffix.as_str())
                    || suffix.ends_with("/filter")
                    || suffix
                        .rsplit('/')
                        .next()
                        .is_some_and(|n| n.parse::<u8>().is_ok())
                {
                    suffix.as_str()
                } else {
                    container
                };
                let id = format!("_node/{container}");
                let driver = self.active_driver();
                let mut queue = self.focus_queue.write().unwrap();
                if !queue
                    .iter()
                    .any(|(d, s, p, _)| d == &driver && *s == ch && p == &id)
                {
                    queue.push_front((driver, ch, id, String::new()));
                }
                return;
            }
            if ch == u16::MAX {
                self.repair_dirty.store(true, Ordering::Release);
            } else {
                self.queue_strip_repair(ch);
            }
            return;
        }
        let mut sync = self.sync_state.lock().unwrap();
        if received < sync.epoch {
            return;
        }
        let key = (ch, param.to_string());
        if let Some((last, _)) = sync.received.get(&key) {
            if *last > query_started.unwrap_or(received) {
                return;
            }
        }
        sync.revision += 1;
        let revision = sync.revision;
        sync.received.insert(key, (received, revision));
        // Internal provider IDs bridge already-existing typed library events.
        if param == "_fx_type" || param.starts_with("_fx_par") {
            let ParamValue::Float(value) = pv else {
                return;
            };
            let slot = ch as u8;
            let mut cache = self.fx_cache.write().unwrap();
            let Some(row) = cache.iter_mut().find(|r| r.slot == slot) else {
                return;
            };
            let changed = if param == "_fx_type" {
                let name = self
                    .active_drv()
                    .fx_type_name_for_slot(slot, *value as u8)
                    .unwrap_or("-")
                    .to_string();
                if row.fx_type != name {
                    row.fx_type = name;
                    row.params.clear();
                    self.fx_refill_requested.store(true, Ordering::Release);
                    true
                } else {
                    false
                }
            } else {
                let type_received = sync
                    .received
                    .get(&(ch, "_fx_type".to_string()))
                    .map(|(at, _)| *at);
                if type_received.is_some_and(|at| at > received) {
                    return;
                }
                let name = param.trim_start_matches("_fx_").to_string();
                if row.params.get(&name) == Some(value) {
                    false
                } else {
                    row.params.insert(name, *value);
                    true
                }
            };
            drop(cache);
            if changed {
                self.broadcast(self.fx_message_cached());
            }
            return;
        }
        let v = param_value_json(pv);
        let changed = cache_store(&mut self.param_cache.write().unwrap(), ch, param, v.clone());
        if changed && self.active_drv().routing_param_ids().contains(&param) {
            self.routing_dirty.store(true, Ordering::Release);
        }
        let event = if param == "_mute_group" {
            pv_bool(pv).map(|muted| Event::MuteGroup {
                group: ch as u8,
                muted,
                key: Some(format!("mgroup/{:02}", ch + 1)),
            })
        } else if let Some(group) = param.strip_prefix("_mg").and_then(|s| s.parse::<u8>().ok()) {
            pv_bool(pv).map(|member| Event::MuteGroupAssign {
                ch,
                group: group - 1,
                member,
                key: self.active_strip_key(ch),
            })
        } else if param == "solo" {
            pv_bool(pv).map(|soloed| Event::Solo {
                ch,
                soloed,
                key: self.active_strip_key(ch),
            })
        } else {
            None
        };
        if let Some(event) = event {
            sync.typed.insert(format!("{ch}/{param}"), event.clone());
            if changed {
                self.broadcast(event);
            }
            return;
        }
        if changed {
            // Desk-evidenced by construction (poll read or desk push
            // `apply_received` is never called with a write fallback): every
            // emitted key is explicitly marked `verified` (see `authority`).
            // Absent/unknown is NEVER verified, the field is optional only
            // for backward compatibility with old frames.
            let authority = Some(
                [(param.to_string(), "verified".to_string())]
                    .into_iter()
                    .collect(),
            );
            self.broadcast(Event::Params {
                ch,
                values: [(param.to_string(), v)].into_iter().collect(),
                key: self.active_strip_key(ch),
                authority,
            });
        }
        // Main/bus library mirrors must follow exactly the same canonical cache.
        if matches!(param, "fader" | "mute") {
            let cache = self.param_cache.read().unwrap();
            let values = cache.get(&ch);
            let pair = values
                .and_then(|v| Some((v.get("fader")?.as_f64()? as f32, v.get("mute")?.as_bool()?)));
            if let Some((fader, muted)) = pair {
                let topo = self.driver_topology(&self.active_driver());
                if let Some(fam) = topo
                    .iter()
                    .find(|f| (f.first..f.first + f.count).contains(&(ch as usize)))
                {
                    let event = match fam.kind {
                        openfader_core::StripKind::Main if ch as usize == fam.first => {
                            Some(Event::Main {
                                fader: Some(fader),
                                muted: Some(muted),
                            })
                        }
                        openfader_core::StripKind::Bus => Some(Event::Bus {
                            bus: (ch as usize - fam.first) as u8,
                            fader,
                            muted,
                            key: self.active_strip_key(ch),
                        }),
                        _ => None,
                    };
                    if let Some(event) = event {
                        sync.typed.insert(format!("master/{ch}"), event.clone());
                        if changed {
                            self.broadcast(event);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn reconcile_event(
        &self,
        mut event: Event,
        sync: &SyncState,
        started: Instant,
    ) -> Event {
        let cache = self.param_cache.read().unwrap();
        let fresh = |ch: u16, id: &str| {
            sync.received
                .get(&(ch, id.to_string()))
                .filter(|(at, _)| *at >= started)
                .and_then(|_| cache.get(&ch)?.get(id))
                .cloned()
        };
        match &mut event {
            Event::DestinationControls { epoch, controls } => {
                if *epoch == self.connection_gen.load(Ordering::Acquire) {
                    *controls = Self::destination_rows(&self.active_drv().destination_controls());
                }
            }
            Event::State {
                strips, main_fader, ..
            } => {
                for row in strips {
                    if let Some(v) = fresh(row.ch, "fader").and_then(|v| v.as_f64()) {
                        row.fader = v as f32;
                    }
                    if let Some(v) = fresh(row.ch, "mute").and_then(|v| v.as_bool()) {
                        row.muted = v;
                    }
                    if let Some(Value::String(v)) = fresh(row.ch, "name") {
                        row.name = v;
                    }
                }
                if let Some(fam) = self
                    .driver_topology(&self.active_driver())
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Main)
                {
                    if let Some(v) = fresh(fam.first as u16, "fader").and_then(|v| v.as_f64()) {
                        *main_fader = Some(v as f32);
                    }
                }
            }
            Event::Param {
                ch,
                field,
                value,
                authority,
                ..
            } => {
                // A genuinely newer desk read (poll/push landing between
                // the echo and this reconcile) replaces the value, and the
                // provenance with it: the value is now desk-evidenced.
                if let Some(v) = fresh(*ch, field) {
                    *value = v;
                    *authority = Some("verified".to_string());
                }
            }
            Event::Solo { ch, soloed, .. } => {
                if let Some(v) = fresh(*ch, "solo").and_then(|v| v.as_bool()) {
                    *soloed = v;
                }
            }
            Event::Main { fader, muted } => {
                if let Some(fam) = self
                    .driver_topology(&self.active_driver())
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Main)
                {
                    if let Some(v) = fresh(fam.first as u16, "fader").and_then(|v| v.as_f64()) {
                        *fader = Some(v as f32);
                    }
                    if let Some(v) = fresh(fam.first as u16, "mute").and_then(|v| v.as_bool()) {
                        *muted = Some(v);
                    }
                }
            }
            Event::Bus {
                bus, fader, muted, ..
            } => {
                if let Some(fam) = self
                    .driver_topology(&self.active_driver())
                    .iter()
                    .find(|f| f.kind == openfader_core::StripKind::Bus)
                {
                    let ch = fam.first as u16 + *bus as u16;
                    if let Some(v) = fresh(ch, "fader").and_then(|v| v.as_f64()) {
                        *fader = v as f32;
                    }
                    if let Some(v) = fresh(ch, "mute").and_then(|v| v.as_bool()) {
                        *muted = v;
                    }
                }
            }
            Event::MuteGroup { group, muted, .. } => {
                if let Some(v) = fresh(*group as u16, "_mute_group").and_then(|v| v.as_bool()) {
                    *muted = v;
                }
            }
            Event::MuteGroupAssign {
                ch, group, member, ..
            } => {
                if let Some(v) = fresh(*ch, &format!("_mg{}", *group + 1)).and_then(|v| v.as_bool())
                {
                    *member = v;
                }
            }
            Event::Fx { .. } => {
                drop(cache);
                return self.fx_message_cached();
            }
            _ => {}
        }
        event
    }
}

/// Compare-and-store one poll read; true when the value is new or
/// changed (i.e. a `params` frame is due).
pub(super) fn cache_store(
    cache: &mut HashMap<u16, HashMap<String, Value>>,
    ch: u16,
    id: &str,
    v: Value,
) -> bool {
    let entry = cache.entry(ch).or_default();
    match entry.get(id) {
        Some(old) if old == &v => false,
        _ => {
            entry.insert(id.to_string(), v);
            true
        }
    }
}

/// Post-write re-read/write agreement (see `refresh_param`): floats agree
/// within the desk's ~10-bit quantization step (2e-3 covers the measured
/// 1e-3 readback epsilon plus f32 noise), bools/text must match exactly.
/// A disagreeing re-read is the desk's stale pre-image, not a rejection.
pub(super) fn refresh_agrees(read: &Value, written: &Value) -> bool {
    match (read, written) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() <= 2e-3,
            _ => false,
        },
        _ => read == written,
    }
}
