//! WING capability projection into the shared engine contracts.
use super::*;

impl_dyn_driver!(openfader_proto_wing::WingDriver {
    fn param_write_context(&self, param: &str) -> &'static [&'static str] {
        if matches!(param, "gain" | "phantom") { &["source_a", "source_b", "source_select"] } else { &[] }
    }

    async fn capture_param_override(&self, strip: StripId, param: &str) -> Option<anyhow::Result<ParamValue>> {
        if param == "wing:source_mode" { Some(self.source_mode_capture(strip).await.map(ParamValue::Text)) } else { None }
    }

    fn param_readback_equivalent(&self, param: &str, read: &ParamValue, written: &ParamValue) -> Option<bool> {
        if param == "wing:source_mode" {
            return Some(match (read,written) {
                (ParamValue::Text(a),ParamValue::Text(b)) => openfader_proto_wing::source::tagged_agree(a,b),
                _ => false,
            });
        }
        if param != "wing:delay_value" { return None; }
        Some(match (read, written) {
            (ParamValue::Text(a), ParamValue::Text(b)) => openfader_proto_wing::delay::tagged_agree(a, b),
            _ => false,
        })
    }

    fn param_recall_group(&self, param: &str) -> Option<&'static str> {
        if param.starts_with("wing:source_") || param == "wing:input_mode" { return Some("source"); }
        if param == "wing:input_balance" { return Some("input"); }
        if param == "wing:width" { return Some("fader_pan"); }
        if param.starts_with("wing:hcf_") || param.starts_with("wing:filter_") { return Some("eq"); }
        if param.starts_with("wing:delay_") { return Some("input"); }
        if matches!(param, "source_a" | "source_b" | "source_select") {
            return Some("source");
        }
        if param == "lr_assign" { return Some("sends_main"); }
        if let Some(rest) = param.strip_prefix("eq") {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && rest[digits..].starts_with('_') { return Some("eq"); }
        }
        let rest = param.strip_prefix("send")?;
        let (number, _) = rest.split_once('_')?;
        match number.parse::<u16>().ok()? {
            1..=16 => Some("sends_bus"),
            17..=24 => Some("sends_matrix"),
            25..=28 => Some("sends_main"),
            _ => None,
        }
    }

    fn group_assign_caps(&self) -> (bool, bool) { (true, false) }
    fn send_destination(&self, slot: u16) -> Option<SendDestination> {
        let (family, index, prefix) = openfader_proto_wing::sends::target(slot)?;
        let topology = MixerDriver::topology(self);
        let local = index - topology.iter().find(|f| f.key == family)?.first + 1;
        Some(SendDestination {
            key: openfader_core::strip_key_for_index(&topology, index)?,
            label: format!("{prefix} {local}"), stereo: true, allow_self: false,
        })
    }
    fn param_read_only(&self, param: &str) -> bool {
        matches!(param, "wing:input_mode" | "wing:source_mode_writable" | "wing:delay_state" | "source" | "wing:eq_bands" | "eq2_type" | "eq3_type" | "eq4_type" | "eq5_type" | "eq7_type")
    }
    async fn clear_solo(&self) -> anyhow::Result<()> { self.clear_solos().await }
    fn live_solo_bank(&self) -> bool { true }
    fn essentials_extra_ids(&self, _kind: StripKind) -> &'static [&'static str] { &["solo"] }
    async fn set_param_guarded(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>) -> anyhow::Result<()> { self.set_guarded(strip, param, value, expected).await }
    fn param_dependents(&self, param: &str) -> &'static [&'static str] { match param {
        "wing:filter_model" => &["wing:filter_tilt", "wing:filter_low", "wing:filter_high", "wing:filter_ap1_freq", "wing:filter_ap2_freq", "wing:filter_ap2_q"],
        "wing:delay_unit" => &["wing:delay_value", "wing:delay_state"],
        "wing:delay_value" | "delay_on" => &["wing:delay_state"],
        "source_select" | "source_a" | "source_b" | "wing:source_auto" => &["source_select", "source", "gain", "phantom", "wing:source_mode", "wing:source_mode_writable", "wing:input_mode"],
        "wing:source_mode" => &["wing:input_mode", "wing:source_mode_writable"],
        "lr_assign" => &["send25_on"], "send25_on" => &["lr_assign"], _ => &[],
    } }
    fn fx_flavor(&self) -> FxFlavor { FxFlavor::Numbered }
    fn fx_slot_count(&self) -> usize { 16 }
    fn fx_type_name(&self, index: u8) -> Option<&'static str> { openfader_proto_wing::fx::MODELS.get(index as usize).map(|m| m.id) }
    fn fx_type_param_count(&self, index: u8) -> Option<usize> { openfader_proto_wing::fx::MODELS.get(index as usize).map(|m| m.params.len()) }
    fn fx_type_index_of(&self, name: &str) -> Option<u8> { openfader_proto_wing::fx::MODELS.iter().position(|m| m.id == name || m.label == name).map(|i| i as u8) }
    async fn fx_type_index(&self, slot: u8) -> Option<u8> { self.fx_model(slot).await.ok() }
    async fn fx_set_type(&self, slot: u8, model: &str) -> anyhow::Result<()> { self.set_fx_model(slot, model).await }
    async fn get_fx_param_with_type(&self, slot: u8, param: u8, effect: Option<u8>) -> anyhow::Result<f32> { self.read_fx(slot, param, effect).await }
    async fn fx_set_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        let param = parse_par_id(id).ok_or_else(|| anyhow::anyhow!("WING: invalid FX parameter id"))?;
        MixerDriver::set_fx_param(self, slot, param, value).await
    }
    async fn fx_apply_row(&self, slot: u8, model: &str, _on: bool, _bypass: bool, params: &BTreeMap<String, f32>) -> anyhow::Result<()> {
        let index = self.fx_type_index_of(model).ok_or_else(|| anyhow::anyhow!("WING: unknown FX model"))?;
        let defs = openfader_proto_wing::fx::MODELS[index as usize].params;
        for (id, value) in params {
            let p = parse_par_id(id).ok_or_else(|| anyhow::anyhow!("WING: invalid FX parameter id"))?;
            defs.get(p as usize).ok_or_else(|| anyhow::anyhow!("WING: invalid FX parameter"))?.encode(*value)?;
        }
        self.set_fx_model(slot, model).await?;
        for (id, value) in params { self.fx_set_param(slot, id, *value).await?; }
        Ok(())
    }
    fn fx_type_caps(&self) -> Vec<crate::protocol::FxTypeCaps> {
        openfader_proto_wing::fx::MODELS.iter().map(|model| crate::protocol::FxTypeCaps {
            id: model.id.into(), label: model.label.into(), slots: (1..=if model.premium { 8 } else { 16 }).collect(),
            params: model.params.iter().enumerate().map(|(index, p)| crate::protocol::FxParamCaps {
                            position: None,
                id: format!("par{:02}", index+1), label: p.label.into(), unit: p.unit.into(), min: p.min, max: p.max,
                default: 0., scale: p.scale().into(),
                choices: p.choices.iter().enumerate().map(|(i, (_, label))| crate::protocol::FxChoiceCaps {
                    value: if p.choices.len()>1 { i as f32 / (p.choices.len()-1) as f32 } else { 0. }, label: (*label).into(),
                }).collect(),
            }).collect(),
        }).collect()
    }
    fn poll_taps(&self) -> Option<Vec<TapLevels>> {
        self.meter_sample().map(|s| s.levels.into_iter().map(|v| TapLevels { input: v.input, post: v.post, gr_gate_db: 0., gr_comp_db: 0. }).collect())
    }
    async fn identify(&self, ip: IpAddr, budget: std::time::Duration) -> Option<openfader_transport::discovery::DiscoveredDevice> {
        Self::identify(ip, budget).await
    }
    async fn discover(&self, budget: std::time::Duration) -> Vec<openfader_transport::discovery::DiscoveredDevice> {
        Self::discover(budget).await
    }
    fn discovered_name(&self, device: &openfader_transport::discovery::DiscoveredDevice) -> String { Self::discovered_name(device) }
    fn display_name(&self, model: &str) -> String { format!("Behringer WING ({model})") }
    fn is_hardware(&self) -> bool { true }
    fn needs_hydration(&self) -> bool { true }
    fn param_plane_snapshot(&self) -> bool { true }
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> { Self::param_scopes() }
    fn routing_groups(&self) -> Vec<RoutingGroup> {
        openfader_proto_wing::ROUTING_GROUPS.iter().filter(|(key, _, _)| *key != "OFF")
            .map(|&(key, label, count)| RoutingGroup { key, label, count: count as usize }).collect()
    }
    fn canonical_routing_target<'a>(&self, target: &'a str) -> &'a str {
        if target.is_empty() { "main" } else { target }
    }
    fn routing_control(&self, _target: &str) -> &str { "source_select" }
    fn routing_targets(&self) -> Vec<RoutingTarget> {
        vec![RoutingTarget { key: "main", label: "Main input", clearable: true }, RoutingTarget { key: "alt", label: "Alternate input", clearable: true }]
    }
    async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> { Self::routing_source(self, strip, target).await }
    async fn patch_target_set(&self, strip: StripId, target: &str, group: &str, index: u8) -> anyhow::Result<RoutingPatch> {
        let label = self.patch(strip, target, group, index).await?;
        Ok(RoutingPatch { label, param: None })
    }
    async fn patch_target_clear(&self, strip: StripId, target: &str) -> anyhow::Result<RoutingPatch> {
        let label = self.patch(strip, target, "OFF", 0).await?;
        Ok(RoutingPatch { label, param: None })
    }
    fn routing_param_ids(&self) -> &'static [&'static str] { &["source_select"] }
    fn static_extra_rows(&self, _topo: &[StripFamily]) -> Vec<(u16, &'static str)> { (0..8).map(|n| (n, "_mute_group")).collect() }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> { scene.recall_to(self, fade_ms).await }
});
