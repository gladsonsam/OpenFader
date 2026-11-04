//! X32/M32 capability projection into the shared engine contracts.
use super::*;

impl_dyn_driver!(openfader_proto_x32::X32Driver {
    async fn reported_sample_rate(&self) -> Option<u32> { self.observed_sample_rate().await.ok() }
    fn param_write_context(&self, param: &str) -> &'static [&'static str] {
        if matches!(param, "gain" | "phantom") { &["source", "x32:preamp_state"] } else { &[] }
    }
    async fn capture_param_applicable(&self, strip: StripId, param: &str) -> anyhow::Result<bool> {
        if matches!(param, "gain" | "phantom") {
            self.preamp_capture_applicable(strip, param).await
        } else {
            Ok(true)
        }
    }
    fn param_read_only(&self, param: &str) -> bool {
        // The source label derives from the routed input selector and is
        // not a patch write; the band count derives from the strip family;
        // the preamp state is a computed readout; automix gain is a live
        // meter value with no write leaf.
        matches!(
            param,
            "source" | "x32:eq_bands" | "x32:eq_type_count" | "x32:preamp_state" | "x32:automix_gain" | "gate_gr" | "comp_gr"
        )
    }
    fn param_recall_group(&self, param: &str) -> Option<&'static str> {
        if param == "source" { return Some("source"); }
        if param == "x32:mono_assign" || param == "x32:mono_level" { return Some("sends_main"); }
        if param == "lr_assign" { return Some("sends_main"); }
        let rest = param.strip_prefix("send")?;
        let (number, _) = rest.split_once('_')?;
        match number.parse::<u16>().ok()? {
            1..=16 => Some("sends_bus"),
            17..=22 => Some("sends_matrix"),
            _ => None,
        }
    }
    fn param_dependents(&self, param: &str) -> &'static [&'static str] {
        if param == "source" { &["gain", "phantom"] } else { &[] }
    }
    async fn get_param_display(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        if matches!(param, "gain" | "phantom") {
            self.preamp_display(strip, param).await
        } else {
            MixerDriver::get_param(self, strip, param).await
        }
    }
    async fn set_param_guarded(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>) -> anyhow::Result<()> {
        self.set_guarded(strip, param, value, expected).await
    }
    async fn set_param_guarded_checked(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        if matches!(param, "gain" | "phantom") {
            return self.write_active_preamp_guarded_checked(strip, param, value, expected, check).await;
        }
        // Linked pairs share the desk control surface: authorize every
        // affected member before the write. Unreadable link state rejects
        // the write because the partner may also be affected.
        let members = self.linked_members(strip).await?;
        check(&members)?;
        self.set_guarded(strip, param, value, expected).await
    }
    async fn set_mute_group_assign_checked(&self, ch: StripId, group: u8, member: bool, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        check(&[ch])?;
        MixerDriver::set_mute_group_assign(self, ch, group, member).await
    }
    fn group_assign_caps(&self) -> (bool, bool) { (true, false) }
    fn send_destination(&self, slot: u16) -> Option<SendDestination> {
        // Buses and matrices are mono (links pair them optionally); no
        // destination is marked stereo. Slots 17..22 are matrix
        // destinations on distinct canonical ids so one global slot
        // identity never means bus and matrix at once.
        let (key, label) = openfader_proto_x32::sends::send_destination(slot)?;
        Some(SendDestination { key, label, stereo: false, allow_self: false })
    }
    fn preamp_invalidation_fields(&self, source: bool) -> &'static [&'static str] {
        if source {
            &["source", "x32:preamp_state", "gain", "phantom"]
        } else {
            &["gain", "phantom"]
        }
    }
    fn preamp_invalidation_count(&self) -> usize { 32 }
    /// Live automix gain alias behind `provider:automix_gain` (channels
    /// 1..8; honestly unavailable until the /meters/16 stream lands).
    fn automix_gain_param(&self) -> Option<&'static str> { Some("x32:automix_gain") }
    fn guarded_preamp_writes(&self) -> bool { true }
    fn guarded_send_writes(&self) -> bool { true }
    fn guarded_link_writes(&self) -> bool { true }
    fn initial_read_timeout(&self) -> std::time::Duration { openfader_transport::session::CONNECT_BURST_TIMEOUT }
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> { Self::param_scopes() }
    fn param_plane_snapshot(&self) -> bool { true }
    fn dial_addr(&self, ip: &str) -> String {
        if ip.parse::<SocketAddr>().is_ok() {
            return ip.to_string();
        }
        if let Some((h, p)) = ip.rsplit_once(':') {
            if p.parse::<u16>().is_ok() && h.trim_matches(['[', ']']).parse::<IpAddr>().is_ok() {
                return ip.to_string();
            }
        }
        if ip.parse::<IpAddr>().is_ok() && !ip.contains(':') {
            return format!("{ip}:{}", openfader_proto_x32::addrs::PORT);
        }
        ip.to_string()
    }
    fn claims_device(&self, id: &str, model: &str) -> bool {
        let m = model.to_lowercase();
        id.starts_with("x32-") || m.contains("x32") || m.contains("m32")
    }
    fn display_name(&self, model: &str) -> String {
        if model.to_uppercase().starts_with("M32") {
            format!("Midas M32 ({model})")
        } else {
            format!("Behringer X32 ({model})")
        }
    }
    async fn identify(&self, ip: IpAddr, budget: std::time::Duration) -> Option<openfader_transport::discovery::DiscoveredDevice> {
        Self::identify(ip, budget).await
    }
    async fn discover(&self, budget: std::time::Duration) -> Vec<openfader_transport::discovery::DiscoveredDevice> {
        Self::discover(budget).await
    }
    async fn control_health(&self) -> anyhow::Result<()> {
        self.check_control_health().await
    }
    fn discovered_name(&self, device: &openfader_transport::discovery::DiscoveredDevice) -> String {
        Self::discovered_name(device)
    }
    fn is_hardware(&self) -> bool { true }
    fn wants_meter_subscribe(&self) -> bool { true }
    fn dyn_gr(&self) -> bool { true }
    fn needs_hydration(&self) -> bool { true }
    fn probe_table(&self) -> Option<&'static str> { Some("x32") }
    fn live_solo_bank(&self) -> bool { true }
    async fn solo_bank(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        self.solo_bank_states(channels).await
    }
    async fn clear_solo(&self) -> anyhow::Result<()> { self.clear_solos().await }
    fn clear_solo_scope(&self) -> Option<&'static str> { Some("advertised") }
    fn essentials_extra_ids(&self, kind: StripKind) -> &'static [&'static str] {
        if matches!(kind, StripKind::In) { &["solo"] } else { &[] }
    }
    fn fx_flavor(&self) -> FxFlavor { FxFlavor::Numbered }
    fn fx_slot_count(&self) -> usize { 8 }
    fn fx_insert_supported(&self) -> bool { false }
    fn fx_type_name(&self, type_index: u8) -> Option<&'static str> {
        // Bare indices are slot-domain ambiguous (28 = GEQ on slots 1..4,
        // DRV on slots 5..8); shared paths must use `fx_type_name_for_slot`.
        // The full-catalogue read keeps legacy global-identity callers
        // (name-keyed maps) resolving every slot 5..8 effect.
        openfader_proto_x32::fx::fx_type_name(1, type_index)
            .or_else(|| openfader_proto_x32::fx::fx_type_name(5, type_index))
    }
    fn fx_type_name_for_slot(&self, slot: u8, type_index: u8) -> Option<&'static str> {
        openfader_proto_x32::fx::fx_type_name(slot, type_index)
    }
    fn fx_type_param_count(&self, _type_index: u8) -> Option<usize> {
        // Bare indices are slot-domain ambiguous; shared paths must use
        // `fx_type_param_count_for_slot`. No global answer is offered.
        None
    }
    fn fx_type_param_count_for_slot(&self, slot: u8, type_index: u8) -> Option<usize> {
        let code = openfader_proto_x32::fx::fx_type_name(slot, type_index)?;
        openfader_proto_x32::fx::catalogue::param_count(code)
    }
    fn fx_type_index_of(&self, fx_type: &str) -> Option<u8> {
        openfader_proto_x32::fx::fx_type_index(1, fx_type)
            .or_else(|| openfader_proto_x32::fx::fx_type_index(5, fx_type))
    }
    fn fx_type_index_for_slot(&self, slot: u8, fx_type: &str) -> Option<u8> {
        openfader_proto_x32::fx::fx_type_index(slot, fx_type)
    }
    async fn fx_type_index(&self, slot: u8) -> Option<u8> {
        self.get_fx_type_index(slot).await.ok()
    }
    async fn fx_set_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        let index = openfader_proto_x32::fx::fx_type_index(slot, fx_type)
            .ok_or_else(|| anyhow::anyhow!("x32: unknown FX type {fx_type:?} for slot {slot}"))?;
        self.set_fx_type_index(slot, index).await
    }
    async fn get_fx_param_with_type(&self, slot: u8, param: u8, effect: Option<u8>) -> anyhow::Result<f32> {
        self.read_fx_par_typed(slot, param, effect).await
    }
    async fn fx_set_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        let param = parse_par_id(id)
            .ok_or_else(|| anyhow::anyhow!("{id:?} has no x32 mapping (use par01..par64)"))?;
        // Type-bound write against the live slot type: enum pars encode as
        // OSC ints, continuous pars as normalized floats, out-of-count pars
        // bail before any I/O.
        self.write_fx_par_checked(slot, param, value, None).await
    }
    async fn fx_apply_row(&self, slot: u8, fx_type: &str, on: bool, bypass: bool, params: &BTreeMap<String, f32>) -> anyhow::Result<()> {
        // The desk documents no per-slot on/bypass leaf, so requested
        // non-default states are rejected BEFORE any write; the engine's
        // honest placeholders (on, no bypass) pass through as no-ops.
        // Never silently ignore a requested state.
        anyhow::ensure!(
            on && !bypass,
            "x32: FX slot {slot} has no on/bypass leaf (requested on={on} bypass={bypass})"
        );
        let index = openfader_proto_x32::fx::fx_type_index(slot, fx_type)
            .ok_or_else(|| anyhow::anyhow!("x32: unknown FX type {fx_type:?} for slot {slot}"))?;
        let code = openfader_proto_x32::fx::fx_type_name(slot, index)
            .ok_or_else(|| anyhow::anyhow!("x32: FX type index {index} out of range for slot {slot}"))?;
        let count = openfader_proto_x32::fx::catalogue::param_count(code).ok_or_else(|| {
            anyhow::anyhow!("x32: {code} has no documented parameter count")
        })?;
        let mut values = Vec::with_capacity(params.len());
        for (id, value) in params {
            let param = parse_par_id(id)
                .ok_or_else(|| anyhow::anyhow!("x32: invalid FX parameter {id:?}"))?;
            anyhow::ensure!(
                (param as usize) < count,
                "x32: {id:?} unavailable for {code} ({count} documented params)"
            );
            // Encoded up front (bounds + range validated before any write);
            // the per-param write below re-verifies the live type.
            let arg = openfader_proto_x32::fx::wire::encode_fx_par(code, param + 1, *value)?;
            values.push((param, *value, arg));
        }
        if self.get_fx_type_index(slot).await? != index {
            self.set_fx_type_index(slot, index).await?;
        }
        let total = values.len();
        for (applied, (param, value, arg)) in values.into_iter().enumerate() {
            if let Err(error) = self
                .write_fx_par_encoded(slot, param, code, index, value, arg)
                .await
            {
                anyhow::bail!(
                    "x32: FX slot {slot} {code} recall partial ({applied}/{total} params applied): {error:#}"
                );
            }
        }
        Ok(())
    }
    async fn fx_push(&self, strip: u8, param: &str, value: &serde_json::Value) -> anyhow::Result<()> {
        let Some(param) = parse_par_id(param) else {
            anyhow::bail!("x32: fx push id {param:?} has no mapping")
        };
        let serde_json::Value::Number(n) = value else {
            anyhow::bail!("x32: fx push value must be a number")
        };
        MixerDriver::set_fx_param(self, strip, param, n.as_f64().unwrap_or(0.0) as f32).await
    }
    fn fx_type_caps(&self) -> Vec<crate::protocol::FxTypeCaps> {
        use openfader_proto_x32::fx as xfx;
        let mut out = Vec::new();
        for (slot, table) in [(1u8, xfx::FX_TYPES_1_4), (5u8, xfx::FX_TYPES_5_8)] {
            for name in table {
                // Full documented catalogue: actual counts, names, wire
                // types, ranges and options from the Effects Parameters
                // tables (no raw-`parNN`-only rows). Units are absent from
                // the doc, so none are advertised; tap targets are not
                // documented either. Defaults are neutral UI midpoints
                // (0.5 continuous, first option), never desk defaults.
                let effect = xfx::catalogue::effect_by_code(name);
                let count = effect
                    .and_then(|e| xfx::catalogue::param_count(e.code))
                    .unwrap_or(0);
                let mut params = Vec::with_capacity(count);
                for par in 1..=count as u8 {
                    let Some(info) = effect
                        .and_then(|e| xfx::catalogue::param_info(e.code, par))
                    else {
                        continue;
                    };
                    let choices = info
                        .options
                        .unwrap_or(&[])
                        .iter()
                        .enumerate()
                        .map(|(i, label)| crate::protocol::FxChoiceCaps {
                            value: if info.options.unwrap_or(&[]).len() > 1 {
                                i as f32 / (info.options.unwrap_or(&[]).len() - 1) as f32
                            } else {
                                0.0
                            },
                            label: (*label).to_string(),
                        })
                        .collect();
                    let is_enum = info.kind == xfx::catalogue::FxParamKind::Enum;
                    params.push(crate::protocol::FxParamCaps {
                        choices,
                        position: None,
                        id: format!("par{par:02}"),
                        label: info.label,
                        unit: String::new(),
                        min: info.min,
                        max: info.max,
                        default: if is_enum { 0.0 } else { 0.5 },
                        scale: if is_enum {
                            "enum".to_string()
                        } else if info.kind == xfx::catalogue::FxParamKind::Log {
                            "log".to_string()
                        } else {
                            "lin".to_string()
                        },
                    });
                }
                out.push(crate::protocol::FxTypeCaps {
                    tap_target: None,
                    slots: ((slot)..(slot + 4)).collect(),
                    id: (*name).to_string(),
                    label: effect.map_or((*name).to_string(), |e| e.full.to_string()),
                    params,
                });
            }
        }
        // Deduplicate names present in both tables (insertable effects),
        // merging their slot lists.
        let mut merged: Vec<crate::protocol::FxTypeCaps> = Vec::new();
        for caps in out {
            if let Some(existing) = merged.iter_mut().find(|e| e.id == caps.id) {
                existing.slots.extend(caps.slots);
                existing.slots.sort_unstable();
                existing.slots.dedup();
            } else {
                merged.push(caps);
            }
        }
        merged
    }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        scene.recall_to(self, fade_ms).await
    }
    fn routing_groups(&self) -> Vec<RoutingGroup> {
        openfader_proto_x32::routing::INPUT_SOURCE_GROUPS
            .iter()
            .map(|(key, label, count)| RoutingGroup { key, label, count: *count })
            .collect()
    }
    fn routing_targets(&self) -> Vec<RoutingTarget> {
        // One source selector per channel/aux strip; writing OFF clears it.
        vec![RoutingTarget { key: "", label: "Input source", clearable: true }]
    }
    fn routing_control(&self, _target: &str) -> &str { "source" }
    fn routing_param_ids(&self) -> &'static [&'static str] { &["source"] }
    fn routing_strip_kind(&self, kind: StripKind) -> bool {
        // Source selection exists on channels and aux inputs only; FX
        // returns carry no source leaf (the custom plane below lists ch +
        // auxin strips exactly, so no silent destination is advertised).
        matches!(kind, StripKind::In | StripKind::Ret)
    }
    fn routing_planes(&self) -> Vec<crate::protocol::RoutingPlaneCaps> {
        use crate::protocol::{RoutingDestinationCaps, RoutingGroupCaps, RoutingPlaneCaps};
        let groups: Vec<RoutingGroupCaps> = self
            .routing_groups()
            .into_iter()
            .map(|g| RoutingGroupCaps {
                key: g.key.into(),
                label: g.label.into(),
                count: g.count,
                options: Vec::new(),
                mode: false,
            })
            .collect();
        let topo = MixerDriver::topology(self);
        let mut destinations = Vec::new();
        // Channels 1..32 (strips 0..32) and aux inputs 1..8 (strips
        // 32..40): the only strips with a source leaf.
        for i in 0..40u16 {
            let key = openfader_core::strip_key_for_index(&topo, i as usize)
                .unwrap_or_else(|| format!("in/{:02}", i + 1));
            let label = if i < 32 {
                format!("Channel {}", i + 1)
            } else {
                format!("Aux in {}", i - 31)
            };
            destinations.push(RoutingDestinationCaps {
                output: false,
                id: format!("input/{key}"),
                key,
                strip: i,
                label,
                target: String::new(),
                clearable: true,
                source_groups: groups.clone(),
            });
        }
        let mut planes = vec![RoutingPlaneCaps {
            key: "input".into(),
            label: "Input source".into(),
            destinations,
        }];
        // FX insert-source selectors (slots 1..4 × L/R = 8 routes):
        // canonical 1-based `Source {n}` identities (native `0` =
        // `Source 1` = `INS`); the native desk-facing labels
        // (`INS`, `MIX1..16`, `M/C`) ride the caps `options` list so the
        // shared `{Group} {index}` capture/recall path parses them
        // without a model branch.
        planes.push(RoutingPlaneCaps {
            key: "fx/source".into(),
            label: "FX source".into(),
            destinations: openfader_proto_x32::routing::fx_source_ids()
                .into_iter()
                .map(|id| {
                    let slot: u8 = id["fx/source/".len()..][..2]
                        .parse()
                        .unwrap_or(1);
                    let side = &id[id.len() - 1..];
                    let side_label = if side == "l" { "L" } else { "R" };
                    RoutingDestinationCaps {
                        output: true,
                        id: id.clone(),
                        key: id.clone(),
                        strip: (u16::from(slot) - 1) * 2
                            + if side == "r" { 1 } else { 0 },
                        label: format!("FX {slot} {side_label} source"),
                        target: "fx/source".into(),
                        clearable: false,
                        source_groups: vec![RoutingGroupCaps {
                            key: "Source".into(),
                            label: "FX source".into(),
                            count: openfader_proto_x32::routing::FX_SOURCE_GROUPS
                                .iter()
                                .map(|(_, _, count)| *count)
                                .sum(),
                            options: openfader_proto_x32::addrs::FX_SOURCES
                                .iter()
                                .map(|s| s.to_string())
                                .collect(),
                            mode: false,
                        }],
                    }
                })
                .collect(),
        });
        for bank in openfader_proto_x32::routing::BANK_PLANES {
            planes.push(RoutingPlaneCaps {
                key: bank.key.into(),
                label: bank.label.into(),
                destinations: bank
                    .destinations
                    .iter()
                    .enumerate()
                    .map(|(slot, dest)| RoutingDestinationCaps {
                        output: true,
                        id: dest.id.into(),
                        key: dest.id.into(),
                        strip: slot as u16,
                        label: dest.label.into(),
                        target: dest.target.into(),
                        clearable: false,
                        source_groups: vec![RoutingGroupCaps {
                            key: "Bank".into(),
                            label: "Bank source".into(),
                            count: dest.options.len(),
                            options: dest.options.iter().map(|s| s.trim().to_string()).collect(),
                            mode: false,
                        }],
                    })
                    .collect(),
            });
        }
        planes.extend(crate::drivers::routing::output_planes(
            openfader_proto_x32::routing::planes(),
        ));
        planes
    }
    async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> {
        anyhow::ensure!(target.is_empty(), "x32: unknown routing target {target:?}");
        self.input_source_routing(strip).await
    }
    async fn patch_target_set(&self, strip: StripId, target: &str, group: &str, index: u8) -> anyhow::Result<RoutingPatch> {
        anyhow::ensure!(target.is_empty(), "x32: unknown routing target {target:?}");
        let live = self.input_source_routing(strip).await?;
        let label = self
            .patch_input_source_guarded(strip, Some((group, index)), &live, &|| true)
            .await?;
        Ok(RoutingPatch { label: label.clone(), param: Some(("source", ParamValue::Text(label))) })
    }
    async fn patch_target_clear(&self, strip: StripId, target: &str) -> anyhow::Result<RoutingPatch> {
        anyhow::ensure!(target.is_empty(), "x32: unknown routing target {target:?}");
        let live = self.input_source_routing(strip).await?;
        let label = self
            .patch_input_source_guarded(strip, None, &live, &|| true)
            .await?;
        Ok(RoutingPatch { label: label.clone(), param: Some(("source", ParamValue::Text(label))) })
    }
    fn output_route(&self, id: &str) -> anyhow::Result<openfader_core::routing::RouteState> {
        self.output_route_sync(id)
    }
    async fn set_output_route(&self, id: &str, source: &str, expected: &openfader_core::routing::RouteState, context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, context).await
    }
    async fn soundcheck_profile(&self) -> anyhow::Result<Option<SoundcheckProfile>> {
        // No verified routing profile exists: automatic soundcheck routing
        // stays unsupported and manual virtual soundcheck stays independent
        // of the desk. Returning None (rather than an invented profile) is
        // the honest contract here.
        Ok(None)
    }
});
