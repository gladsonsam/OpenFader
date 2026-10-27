//! Capabilities operations.

use super::*;

impl Engine {
    /// catalogue filtered by the driver's capabilities, `unsupported` the
    /// rest in catalogue order; `topology` + `meter_taps` are additive
    /// (old fields byte-identical). Both sides resolve through the
    /// driver's merged [`ParamRegistry`](params::ParamRegistry), so
    /// provider-contributed rows ride `caps` once the driver lists them in
    /// its capabilities.
    pub(super) fn caps_message(&self) -> Event {
        self.caps_message_filtered(false)
    }

    /// Slim `caps` for the attach snapshot: identical to
    /// [`caps_message`](Engine::caps_message) except `params` carries only
    /// the essentials lane (`fader`/`mute`/`name`/`pan`/`color`, the ids
    /// first paint and strip interaction resolve) instead of the ~180-row
    /// catalogue (~30 KB, three quarters of the snapshot). The full
    /// catalogue follows on the client's first `hello` (sent right after
    /// subscribe on every transport) as a unicast `caps` rebroadcast, the
    /// same frame shape as a param-drop caps update, which every consumer
    /// already applies idempotently, and `push_snapshot` (resume) keeps
    /// the full frame, so a resumed UI never downgrades its catalogue.
    pub(super) fn caps_slim_message(&self) -> Event {
        self.caps_message_filtered(true)
    }

    /// Shared `caps` builder: `slim` selects the essentials-only `params`
    /// (see [`caps_slim_message`](Engine::caps_slim_message)); every other
    /// field is identical in both shapes.
    pub(super) fn caps_message_filtered(&self, slim: bool) -> Event {
        let driver = self.active_driver();
        let supported = self.driver_capabilities(&driver);
        let reg = self.registry(&driver);
        let dropped = self.param_dropped.read().unwrap();
        let live: Vec<&&str> = supported
            .iter()
            .filter(|id| !dropped.contains(&(driver.clone(), id.to_string())))
            .filter(|id| !slim || is_essentials_id(id))
            .collect();
        let presentations = self.drv(&driver).param_presentations();
        let labels = self.drv(&driver).param_label_overrides();
        let params: Vec<ParamDef> = live
            .iter()
            .filter_map(|id| reg.find(id).map(|def| {
                let mut frame = param_def_frame(def);
                if let Some((_, label)) = labels.iter().find(|(id, label)| *id == def.id && !label.is_empty()) {
                    frame.label.clone_from(label);
                }
                let swatches = self.drv(&driver).param_swatches(def.id);
                let symbols = self.drv(&driver).param_symbols(def.id);
                let text = self.drv(&driver).param_text_rules(def.id);
                frame.presentations = presentations.iter().filter(|p| p.id == def.id).map(|p| {
                    json!({
                        "when": p.when.map(presentation_condition_frame),
                        "hidden": p.hidden,
                        "curve": p.curve,
                        "limiter_value": p.limiter_value,
                        "value_labels": p.value_labels.iter().map(|c| json!({"value": c.value, "label": c.label})).collect::<Vec<_>>(),
                        "choices": p.choices.iter().map(|c| json!({"value": c.value, "label": c.label})).collect::<Vec<_>>()
                    })
                }).collect();
                if let Some(slot) = def.id.strip_prefix("send").and_then(|id| id.strip_suffix("_level")).and_then(|id| id.parse::<u16>().ok()) {
                    if let Some(destination) = self.drv(&driver).send_destination(slot) {
                        frame.presentations.insert(0, json!({ "destination": {
                            "key": destination.key, "label": destination.label, "stereo": destination.stereo, "allow_self": destination.allow_self,
                        }}));
                    }
                }
                if let Some(group) = self.drv(&driver).param_recall_group(def.id) {
                    frame.presentations.insert(0, json!({ "recall_group": group }));
                }
                if !swatches.is_empty() {
                    frame.presentations.insert(0, json!({
                        "swatches": swatches.iter().map(|s| json!({
                            "value": s.value, "label": s.label, "rgb": s.rgb,
                        })).collect::<Vec<_>>()
                    }));
                }
                if !symbols.is_empty() {
                    frame.presentations.insert(0, json!({
                        "symbols": symbols.iter().map(|s| json!({
                            "value": s.value, "label": s.label, "symbol": s.symbol,
                        })).collect::<Vec<_>>()
                    }));
                }
                if let Some(text) = text {
                    frame.presentations.insert(0, json!({
                        "text": { "max_utf8_bytes": text.max_utf8_bytes,
                            "allow_empty": text.allow_empty,
                            "allow_edge_whitespace": text.allow_edge_whitespace }
                    }));
                }
                frame
            }))
            .collect();
        let mut unsupported: Vec<String> = reg
            .iter()
            .map(|d| d.id.to_string())
            .filter(|id| {
                !supported.contains(&id.as_str()) || dropped.contains(&(driver.clone(), id.clone()))
            })
            .collect();
        unsupported.sort();
        let topo_fams = self.driver_topology(&driver);
        let topology = topo_fams
            .iter()
            .map(|f| crate::protocol::TopologyFamily {
                key: f.key.to_string(),
                kind: f.kind.as_str().to_string(),
                label: f.label.to_string(),
                first: f.first,
                count: f.count,
                name_prefix: f.name_prefix.to_string(),
                color_default: f.color_default,
                params_scope: vec![f.kind.as_str().to_string()],
                strip_keys: (f.first..f.first + f.count)
                    .filter_map(|i| openfader_core::strip_key_for_index(&topo_fams, i))
                    .collect(),
            })
            .collect();
        Event::Caps {
            input_processor_guard: self.drv(&driver).input_processor_guard(),
            driver: driver.clone(),
            params,
            unsupported,
            topology,
            stereo_pairs: self.drv(&driver).stereo_pairs(),
            meter_taps: self.driver_meter_taps(&driver),
            dynamics_meters: self.drv(&driver).poll_dynamics_meters().is_some(),
            fx: self.fx_caps(&driver),
            routing: self.routing_caps(&driver),
            groups: self.groups_caps(&driver),
            files: self.files_caps(&driver),
            native_snapshots: self.native_caps(&driver),
        }
    }

    // ================= R2: devices / link / info =================

    /// This client's meter-tap preference (set by `hello {"taps":true}`).
    pub fn client_taps(&self, id: &str) -> bool {
        self.clients.read().unwrap().get(id).is_some_and(|e| e.taps)
    }

    /// True while any connected client negotiated meter taps.
    pub(super) fn any_taps_client(&self) -> bool {
        self.clients.read().unwrap().values().any(|e| e.taps)
    }

    /// Max concurrent poller queries for one tick (config override or
    /// per-driver default; clamped to 1..=16).
    pub fn max_inflight(&self, driver: &str) -> usize {
        self.poll_inflight
            .unwrap_or_else(|| self.drv(driver).poll_inflight())
            .clamp(1, 16)
    }

    /// Adaptive effective inflight: halves while the batch-RTT EMA exceeds
    /// the poll tick (the desk is saturated, back off instead of piling
    /// on), otherwise the configured max.
    pub fn effective_inflight(&self, driver: &str) -> usize {
        let max = self.max_inflight(driver);
        let ema_us = self.rtt_ema_us.load(Ordering::Relaxed);
        let tick_us = self.poll_interval.as_micros() as u64;
        if tick_us > 0 && ema_us > tick_us {
            (max / 2).max(1)
        } else {
            max
        }
    }
}

/// Serialize provider conditions without coupling the core catalogue to JSON.
pub(super) fn presentation_condition_frame(condition: params::ParamCondition) -> Value {
    use params::ParamCondition::*;
    match condition {
        Equals { id, value } => json!({"id": id, "value": value}),
        Range { id, min, max } => json!({"id": id, "min": min, "max": max}),
        All { all } => {
            json!({"all": all.iter().copied().map(presentation_condition_frame).collect::<Vec<_>>()})
        }
        Any { any } => {
            json!({"any": any.iter().copied().map(presentation_condition_frame).collect::<Vec<_>>()})
        }
    }
}

/// One `caps` catalogue entry.
pub(super) fn param_def_frame(def: &params::ParamDef) -> ParamDef {
    use params::ParamKind;
    let (min, max) = match def.kind {
        ParamKind::Float | ParamKind::Enum => (json!(def.min), json!(def.max)),
        ParamKind::Bool | ParamKind::Text => (Value::Null, Value::Null),
    };
    ParamDef {
        id: def.id.to_string(),
        kind: def.kind.as_str().to_string(),
        scope: def.scope.as_str().to_string(),
        scopes: def.scopes.iter().map(|k| k.as_str().to_string()).collect(),
        min,
        max,
        unit: def.unit.to_string(),
        default: param_value_json(&def.default.as_param_value()),
        label: def.label.to_string(),
        presentations: Vec::new(),
    }
}
