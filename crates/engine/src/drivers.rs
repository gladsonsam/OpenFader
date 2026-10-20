//! Erased driver registry: every desk provider behind one object-safe
//! trait, so the session holds a `HashMap<String, RegisteredDriver>` and
//! never names a concrete driver type.
//!
//! [`MixerDriver`] itself is not dyn-compatible (async methods in traits
//! desugar to per-method `impl Future`, which is not object-safe), so this
//! module mirrors its full surface on [`DynDriver`] with
//! [`async_trait`](async_trait::async_trait) erasure and adds the
//! engine-only extensions that used to be concrete-type branches in
//! `engine.rs` (FX design rack, patch plane, meter taps, GR, hardware
//! flags). The [`impl_dyn_driver`] macro forwards the whole
//! [`MixerDriver`] surface; each provider then overrides only the
//! extensions it supports (defaults are honestly unsupported).
//!
//! A new provider: implement [`DynDriver`] here (mirror macro +
//! extension overrides) and append one [`RegisteredDriver`] row in
//! [`registered_drivers`]. `engine.rs` needs zero edits.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use openmix_core::{
    params::{ParamDef, ParamRegistry},
    MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue, StripFamily, StripId,
};
use openmix_core::show::Scene;

/// Neutral FX slot view: the sim design rack projected off the sim type so
/// session logic never names a provider type.
#[derive(Clone, Debug, Default)]
pub struct FxSlotView {
    pub fx_type: String,
    pub on: bool,
    pub bypass: bool,
    pub params: Vec<f32>,
}

/// Neutral routing group behind `caps.routing` (key/label/count).
#[derive(Clone, Debug)]
pub struct RoutingGroup {
    pub key: &'static str,
    pub label: &'static str,
    pub count: usize,
}

/// Neutral meter taps behind the `taps` frames (sim `poll_taps` projected
/// off the sim type).
#[derive(Clone, Debug)]
pub struct TapLevels {
    pub input: f32,
    pub post: f32,
    pub gr_gate_db: f32,
    pub gr_comp_db: f32,
}

/// FX rack flavor: which write path a rack supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxFlavor {
    /// No FX rack (zero slots, every write honestly unsupported).
    None,
    /// Design rack with string param ids (full read/write mirror).
    Design,
    /// Desk-native rack: numbered `parNN` writes only.
    Numbered,
}

/// Object-safe mirror of [`MixerDriver`] plus the engine-only extensions.
/// Defaults are honestly unsupported; providers override what they serve.
#[async_trait::async_trait]
pub trait DynDriver: Send + Sync {
    // ---- MixerDriver mirror (every provider forwards these) ----
    fn name(&self) -> &'static str;
    fn channel_count(&self) -> usize;
    fn strip_count(&self) -> usize;
    fn topology(&self) -> Vec<StripFamily>;
    fn capabilities(&self) -> &'static [&'static str];
    /// Provider-contributed param definitions, merged with the canonical
    /// catalogue at engine boot (see [`RegisteredDriver::param_registry`]).
    /// Defaults to empty (canonical ids only); mirrors
    /// [`MixerDriver::param_defs`].
    fn param_defs(&self) -> &'static [ParamDef] {
        &[]
    }
    fn meter_taps(&self) -> &'static [&'static str];
    fn bus_count(&self) -> u8;
    fn subgroup_count(&self) -> u8;
    fn mute_group_count(&self) -> u8;
    fn file_transfer(&self) -> bool;
    fn subscribe_updates(&self) -> Option<tokio::sync::broadcast::Receiver<ParamUpdate>>;
    fn poll_meters(&self) -> MeterFrame;
    async fn connect(&self, addr: &str) -> anyhow::Result<()>;
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()>;
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32>;
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()>;
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool>;
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()>;
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String>;
    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()>;
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32>;
    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()>;
    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool>;
    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()>;
    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32>;
    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()>;
    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool>;
    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()>;
    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool>;
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()>;
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool>;
    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()>;
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool>;
    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>>;
    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()>;
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()>;
    async fn get_main_fader(&self) -> anyhow::Result<f32>;
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()>;
    async fn get_main_mute(&self) -> anyhow::Result<bool>;
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()>;
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool>;
    async fn set_param(&self, strip: StripId, param: &str, value: ParamValue) -> anyhow::Result<()>;
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue>;
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot>;
    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()>;
    async fn recall_scene(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()>;
    async fn query_info(&self) -> anyhow::Result<MixerInfo>;
    async fn subscribe_meters(&self) -> anyhow::Result<tokio::sync::broadcast::Receiver<MeterFrame>>;
    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()>;
    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32>;
    /// Single-query FX param read with an already-known effect index
    /// (xair: skips the per-param type re-query; default: ignores the hint
    /// and pays the two-query cost).
    async fn get_fx_param_with_type(
        &self,
        slot: u8,
        param: u8,
        effect: Option<u8>,
    ) -> anyhow::Result<f32> {
        let _ = effect;
        self.get_fx_param(slot, param).await
    }
    /// Live solo bank in one query (xair override; default loops
    /// `get_solo`). `channels` entries, index `i` = strip `i` soloed;
    /// unreadable strips read back unsoloed, never fail the snapshot.
    async fn solo_bank(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        let mut out = Vec::with_capacity(channels);
        for ch in 0..channels as u16 {
            out.push(self.get_solo(StripId(ch)).await.unwrap_or(false));
        }
        Ok(out)
    }

    // ---- engine extensions (defaults: honestly unsupported) ----

    /// FX rack flavor behind the `FxSetType`/`FxParam`/recall write paths.
    fn fx_flavor(&self) -> FxFlavor {
        FxFlavor::None
    }
    /// FX slot count behind `caps.fx` and slot range checks.
    fn fx_slot_count(&self) -> usize {
        0
    }
    /// Design-rack snapshot behind the FX cache (empty unless Design).
    fn fx_snapshot(&self) -> Vec<FxSlotView> {
        Vec::new()
    }
    /// Design-rack type label (Design only).
    fn fx_get_type(&self, slot: u8) -> anyhow::Result<String> {
        let _ = slot;
        anyhow::bail!("{}: fx type select not supported", self.name())
    }
    /// Design-rack type select (Design only).
    async fn fx_set_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        let _ = (slot, fx_type);
        anyhow::bail!("{}: fx type select not supported", self.name())
    }
    /// Design-rack on flag (Design only).
    fn fx_set_on(&self, slot: u8, on: bool) -> anyhow::Result<()> {
        let _ = (slot, on);
        anyhow::bail!("{}: fx on/off not supported", self.name())
    }
    /// Design-rack bypass flag (Design only).
    fn fx_set_bypass(&self, slot: u8, bypass: bool) -> anyhow::Result<()> {
        let _ = (slot, bypass);
        anyhow::bail!("{}: fx bypass not supported", self.name())
    }
    /// Design-rack param read by string id (Design only).
    fn fx_get_param_by_id(&self, slot: u8, id: &str) -> anyhow::Result<f32> {
        let _ = (slot, id);
        anyhow::bail!("{}: fx params not supported", self.name())
    }
    /// Design-rack param write by string id (Design only).
    fn fx_set_param_by_id(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        let _ = (slot, id, value);
        anyhow::bail!("{}: fx params not supported", self.name())
    }
    /// Loaded desk-native effect-type index for one FX slot (Numbered
    /// racks query the desk; Design racks resolve their in-memory type).
    /// `None` = unknown/unreadable (placeholder rows stay honest).
    async fn fx_type_index(&self, _slot: u8) -> Option<u8> {
        None
    }
    /// `FxParam` write by string id: Design writes the design id,
    /// Numbered parses `parNN`, the rest honestly bail.
    async fn fx_set_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        let _ = (slot, id, value);
        anyhow::bail!("{}: fx params not supported", self.name())
    }
    /// Apply one scene FX row (Design mirrors the rack, Numbered writes
    /// the numbered params, the rest honestly bail).
    async fn fx_apply_row(
        &self,
        slot: u8,
        fx_type: &str,
        on: bool,
        bypass: bool,
        params: &BTreeMap<String, f32>,
    ) -> anyhow::Result<()> {
        let _ = (slot, fx_type, on, bypass, params);
        anyhow::bail!("{}: fx recall not supported", self.name())
    }
    /// Apply one offline `fx/...` change (per-driver legacy messages).
    async fn fx_push(&self, strip: u8, param: &str, value: &serde_json::Value) -> anyhow::Result<()> {
        let _ = (strip, param, value);
        anyhow::bail!("{}: fx push not supported", self.name())
    }
    /// Patch one input strip (returns the desk label); routing-capable
    /// drivers only.
    fn patch_set(&self, strip: StripId, group: &str, index: u8) -> anyhow::Result<String> {
        let _ = (strip, group, index);
        anyhow::bail!("{}: routing patch not supported", self.name())
    }
    /// Clear one input strip's patch; routing-capable drivers only.
    fn patch_clear(&self, strip: StripId) -> anyhow::Result<()> {
        let _ = strip;
        anyhow::bail!("{}: routing patch not supported", self.name())
    }
    /// Routing groups behind `caps.routing` (empty = no routing plane).
    fn routing_groups(&self) -> Vec<RoutingGroup> {
        Vec::new()
    }
    /// Per-strip meter taps behind the `taps` frames (`None` = synthesize
    /// from `poll_meters`, like hardware does).
    fn poll_taps(&self) -> Option<Vec<TapLevels>> {
        None
    }
    /// Serves gate/comp GR over the `gate_gr`/`comp_gr` param path.
    fn dyn_gr(&self) -> bool {
        false
    }
    /// Hardware desk (linked-desk verify, latency probe, meter kick).
    /// The sim model is not hardware.
    fn is_hardware(&self) -> bool {
        false
    }
    /// Needs an explicit `subscribe_meters` kick after connect (the meter
    /// blob cache stays silent without it).
    fn wants_meter_subscribe(&self) -> bool {
        false
    }
    /// Probe-table driver for manual add-by-IP (`None` = no probe table,
    /// an honest error). The sim-backed relay probes with the xair table.
    fn probe_table(&self) -> Option<&'static str> {
        None
    }
    /// Whether a discovered device row belongs to this driver (id prefix
    /// plus model substrings; the registry checks drivers in order and the
    /// sim fallback never claims).
    fn claims_device(&self, id: &str, _model: &str) -> bool {
        id.starts_with(&format!("{}-", self.name()))
    }
    /// (mute-group assign, subgroup assign) support behind `caps.groups`.
    fn group_assign_caps(&self) -> (bool, bool) {
        (false, false)
    }
    /// Solos live in the desk: the snapshot re-reads the live solo bank
    /// (cached alongside). Drivers with memory solos serve them from the
    /// membership scan instead.
    fn live_solo_bank(&self) -> bool {
        false
    }
    /// Desk fire-and-forget clear-solo.
    async fn clear_solo(&self) -> anyhow::Result<()> {
        anyhow::bail!("{}: clear solo not supported", self.name())
    }
    /// Named-show recall on this driver (no default: every provider
    /// forwards to [`Scene::recall_to`]).
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()>;
}

/// Implement [`DynDriver`] for one concrete driver: forward the whole
/// [`MixerDriver`] surface (fully-qualified calls dodge inherent-method
/// shadowing — some drivers expose native same-named methods with
/// different signatures) plus the driver-specific `$ext` items. Emitted as
/// a single `#[async_trait]` impl so the attribute sees every `async fn`.
macro_rules! impl_dyn_driver {
    ($t:ty { $($ext:item)* }) => {
        #[async_trait::async_trait]
        impl DynDriver for $t {
        fn name(&self) -> &'static str {
            MixerDriver::name(self)
        }
        fn channel_count(&self) -> usize {
            MixerDriver::channel_count(self)
        }
        fn strip_count(&self) -> usize {
            MixerDriver::strip_count(self)
        }
        fn topology(&self) -> Vec<StripFamily> {
            MixerDriver::topology(self)
        }
        fn capabilities(&self) -> &'static [&'static str] {
            MixerDriver::capabilities(self)
        }
        fn param_defs(&self) -> &'static [ParamDef] {
            MixerDriver::param_defs(self)
        }
        fn meter_taps(&self) -> &'static [&'static str] {
            MixerDriver::meter_taps(self)
        }
        fn bus_count(&self) -> u8 {
            MixerDriver::bus_count(self)
        }
        fn subgroup_count(&self) -> u8 {
            MixerDriver::subgroup_count(self)
        }
        fn mute_group_count(&self) -> u8 {
            MixerDriver::mute_group_count(self)
        }
        fn file_transfer(&self) -> bool {
            MixerDriver::file_transfer(self)
        }
        fn subscribe_updates(&self) -> Option<tokio::sync::broadcast::Receiver<ParamUpdate>> {
            MixerDriver::subscribe_updates(self)
        }
        fn poll_meters(&self) -> MeterFrame {
            MixerDriver::poll_meters(self)
        }
        async fn connect(&self, addr: &str) -> anyhow::Result<()> {
            MixerDriver::connect(self, addr).await
        }
        async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
            MixerDriver::set_fader(self, strip, value).await
        }
        async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
            MixerDriver::get_fader(self, strip).await
        }
        async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
            MixerDriver::set_mute(self, strip, muted).await
        }
        async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
            MixerDriver::get_mute(self, strip).await
        }
        async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
            MixerDriver::set_name(self, strip, name).await
        }
        async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
            MixerDriver::get_name(self, strip).await
        }
        async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
            MixerDriver::set_bus_fader(self, bus, value).await
        }
        async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
            MixerDriver::get_bus_fader(self, bus).await
        }
        async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
            MixerDriver::set_bus_mute(self, bus, muted).await
        }
        async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
            MixerDriver::get_bus_mute(self, bus).await
        }
        async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
            MixerDriver::set_subgroup_fader(self, group, value).await
        }
        async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
            MixerDriver::get_subgroup_fader(self, group).await
        }
        async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
            MixerDriver::set_subgroup_mute(self, group, muted).await
        }
        async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
            MixerDriver::get_subgroup_mute(self, group).await
        }
        async fn set_subgroup_assign(
            &self,
            ch: StripId,
            group: u8,
            member: bool,
        ) -> anyhow::Result<()> {
            MixerDriver::set_subgroup_assign(self, ch, group, member).await
        }
        async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
            MixerDriver::get_subgroup_assign(self, ch, group).await
        }
        async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
            MixerDriver::set_mute_group(self, group, muted).await
        }
        async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
            MixerDriver::get_mute_group(self, group).await
        }
        async fn set_mute_group_assign(
            &self,
            ch: StripId,
            group: u8,
            member: bool,
        ) -> anyhow::Result<()> {
            MixerDriver::set_mute_group_assign(self, ch, group, member).await
        }
        async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
            MixerDriver::get_mute_group_assign(self, ch, group).await
        }
        async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
            MixerDriver::download_file(self, filename).await
        }
        async fn upload_file(
            &self,
            filename: &str,
            data: &[u8],
            file_type: u32,
        ) -> anyhow::Result<()> {
            MixerDriver::upload_file(self, filename, data, file_type).await
        }
        async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
            MixerDriver::set_main_fader(self, value).await
        }
        async fn get_main_fader(&self) -> anyhow::Result<f32> {
            MixerDriver::get_main_fader(self).await
        }
        async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
            MixerDriver::set_main_mute(self, muted).await
        }
        async fn get_main_mute(&self) -> anyhow::Result<bool> {
            MixerDriver::get_main_mute(self).await
        }
        async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
            MixerDriver::set_solo(self, strip, soloed).await
        }
        async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
            MixerDriver::get_solo(self, strip).await
        }
        async fn set_param(
            &self,
            strip: StripId,
            param: &str,
            value: ParamValue,
        ) -> anyhow::Result<()> {
            MixerDriver::set_param(self, strip, param, value).await
        }
        async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
            MixerDriver::get_param(self, strip, param).await
        }
        async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
            MixerDriver::snapshot(self).await
        }
        async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
            MixerDriver::recall(self, snapshot).await
        }
        async fn recall_scene(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
            MixerDriver::recall_scene(self, scene, fade_ms).await
        }
        async fn query_info(&self) -> anyhow::Result<MixerInfo> {
            MixerDriver::query_info(self).await
        }
        async fn subscribe_meters(
            &self,
        ) -> anyhow::Result<tokio::sync::broadcast::Receiver<MeterFrame>> {
            MixerDriver::subscribe_meters(self).await
        }
        async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
            MixerDriver::set_fx_param(self, slot, param, value).await
        }
        async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
            MixerDriver::get_fx_param(self, slot, param).await
        }
        $($ext)*
        }
    };
}

impl_dyn_driver!(openmix_sim::SimDriver {

    fn fx_flavor(&self) -> FxFlavor {
        FxFlavor::Design
    }
    fn fx_slot_count(&self) -> usize {
        openmix_sim::SIM_FX_SLOTS
    }
    fn fx_snapshot(&self) -> Vec<FxSlotView> {
        self.fx_slots()
            .into_iter()
            .map(|s| FxSlotView {
                fx_type: s.fx_type,
                on: s.on,
                bypass: s.bypass,
                params: s.params,
            })
            .collect()
    }
    fn fx_get_type(&self, slot: u8) -> anyhow::Result<String> {
        self.get_fx_type(slot)
    }
    async fn fx_set_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        self.set_fx_type(slot, fx_type)
    }
    fn fx_set_on(&self, slot: u8, on: bool) -> anyhow::Result<()> {
        self.set_fx_on(slot, on)
    }
    fn fx_set_bypass(&self, slot: u8, bypass: bool) -> anyhow::Result<()> {
        self.set_fx_bypass(slot, bypass)
    }
    fn fx_get_param_by_id(&self, slot: u8, id: &str) -> anyhow::Result<f32> {
        self.get_fx_param(slot, id)
    }
    fn fx_set_param_by_id(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        self.set_fx_param(slot, id, value)
    }
    async fn fx_set_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        self.set_fx_param(slot, id, value)
    }
    async fn fx_apply_row(
        &self,
        slot: u8,
        fx_type: &str,
        on: bool,
        bypass: bool,
        params: &BTreeMap<String, f32>,
    ) -> anyhow::Result<()> {
        // Type first (resets params to defaults), then values.
        let cur = self.get_fx_type(slot).unwrap_or_default();
        if cur != fx_type {
            self.set_fx_type(slot, fx_type)?;
        }
        self.set_fx_on(slot, on)?;
        self.set_fx_bypass(slot, bypass)?;
        let mut ids: Vec<&String> = params.keys().collect();
        ids.sort();
        for id in ids {
            self.set_fx_param(slot, id, params[id])?;
        }
        Ok(())
    }
    async fn fx_push(
        &self,
        strip: u8,
        param: &str,
        value: &serde_json::Value,
    ) -> anyhow::Result<()> {
        match param {
            "type" => {
                let serde_json::Value::String(t) = value else {
                    anyhow::bail!("fx type push value must be a string")
                };
                self.set_fx_type(strip, t)?;
                Ok(())
            }
            "on" => {
                let serde_json::Value::Bool(b) = value else {
                    anyhow::bail!("fx on push value must be a bool")
                };
                self.set_fx_on(strip, *b)?;
                Ok(())
            }
            "bypass" => {
                let serde_json::Value::Bool(b) = value else {
                    anyhow::bail!("fx bypass push value must be a bool")
                };
                self.set_fx_bypass(strip, *b)?;
                Ok(())
            }
            _ => {
                let serde_json::Value::Number(n) = value else {
                    anyhow::bail!("fx push value must be a number")
                };
                self.set_fx_param(strip, param, n.as_f64().unwrap_or(0.0) as f32)?;
                Ok(())
            }
        }
    }
    fn patch_set(&self, strip: StripId, group: &str, index: u8) -> anyhow::Result<String> {
        self.set_patch(strip, group, index)
    }
    fn patch_clear(&self, strip: StripId) -> anyhow::Result<()> {
        self.clear_patch(strip)
    }
    fn routing_groups(&self) -> Vec<RoutingGroup> {
        openmix_sim::SIM_PATCH_GROUPS
            .iter()
            .map(|(key, label, count)| RoutingGroup {
                key,
                label,
                count: *count,
            })
            .collect()
    }
    fn poll_taps(&self) -> Option<Vec<TapLevels>> {
        Some(
            self.poll_taps()
                .into_iter()
                .map(|t| TapLevels {
                    input: t.input,
                    post: t.post,
                    gr_gate_db: t.gr_gate_db,
                    gr_comp_db: t.gr_comp_db,
                })
                .collect(),
        )
    }
    fn probe_table(&self) -> Option<&'static str> {
        // The sim-backed relay has no desks of its own: probe with the
        // xair table, the only broadcast table so far.
        Some("xair")
    }
    fn group_assign_caps(&self) -> (bool, bool) {
        (true, true)
    }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        scene.recall_to(self, fade_ms).await
    }
});

impl_dyn_driver!(openmix_proto_xair::XAirDriver {

    /// Mute-group membership rides the strip `{root}/grp/mute` bitmask
    /// (rig-verified XR18 FW 1.20, 2026-10-20 — the driver serves
    /// `get/set_mute_group_assign`, so attach snapshots and the
    /// `MuteGroupAssign` command work). No subgroup leaf exists on X-Air
    /// (subgroups are a DL32S/sim concept), so subgroup assign stays
    /// honestly unsupported.
    fn group_assign_caps(&self) -> (bool, bool) {
        (true, false)
    }

    fn fx_flavor(&self) -> FxFlavor {
        FxFlavor::Numbered
    }
    fn fx_slot_count(&self) -> usize {
        4
    }
    /// Live effect-type read (`/fx/N/type`): the desk answers queries
    /// here with an int `0..=60`; unreadable slots stay `None` (honest
    /// placeholders, never guessed).
    async fn fx_type_index(&self, slot: u8) -> Option<u8> {
        self.get_fx_type(slot).await.ok()
    }
    /// Single-query param read with a known effect index (connect burst /
    /// poller fast path): skips the per-param `/fx/N/type` re-query.
    async fn get_fx_param_with_type(
        &self,
        slot: u8,
        param: u8,
        effect: Option<u8>,
    ) -> anyhow::Result<f32> {
        openmix_proto_xair::XAirDriver::get_fx_param_with_type(self, slot, param, effect).await
    }
    /// Live solo bank in one query (connect-burst fan-out): 1 query, not 16.
    async fn solo_bank(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        let _ = channels;
        openmix_proto_xair::XAirDriver::solo_bank(self).await
    }
    /// Desk-native type select (`/fx/N/type`, rig-verified 2026-10-19):
    /// resolve the caps-advertised name (short code `id` like `HALL`/`VRM`
    /// or full display `label` like `Hall Reverb` — same `proto-xair`
    /// effect table behind `caps.fx`) to the firmware index, then write
    /// it. Unknown names bail locally, never hitting the desk.
    async fn fx_set_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        let index = (0..=openmix_proto_xair::fx::FX_TYPE_MAX)
            .find(|&i| {
                openmix_proto_xair::fx::fx_effect_name(i) == Some(fx_type)
                    || openmix_proto_xair::fx::fx_effect_full_name(i) == Some(fx_type)
            })
            .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
        openmix_proto_xair::XAirDriver::set_fx_type(self, slot, index).await
    }
    async fn fx_set_param(&self, slot: u8, id: &str, value: f32) -> anyhow::Result<()> {
        // Numbered pars only (design ids have no verified mapping).
        let idx = parse_par_id(id).ok_or_else(|| {
            anyhow::anyhow!("{id:?} has no xair mapping (use par01..par64; UNVERIFIED-ON-RIG)")
        })?;
        MixerDriver::set_fx_param(self, slot, idx, value).await
    }
    async fn fx_apply_row(
        &self,
        slot: u8,
        fx_type: &str,
        _on: bool,
        _bypass: bool,
        params: &BTreeMap<String, f32>,
    ) -> anyhow::Result<()> {
        let _ = fx_type;
        // Numbered pars only (design ids have no verified mapping).
        let mut ids: Vec<&String> = params.keys().collect();
        ids.sort();
        for id in ids {
            if let Some(idx) = parse_par_id(id) {
                MixerDriver::set_fx_param(self, slot, idx, params[id]).await?;
            }
        }
        Ok(())
    }
    async fn fx_push(
        &self,
        strip: u8,
        param: &str,
        value: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let Some(idx) = parse_par_id(param) else {
            anyhow::bail!("fx push id {param:?} has no xair mapping")
        };
        let serde_json::Value::Number(n) = value else {
            anyhow::bail!("fx push value must be a number")
        };
        MixerDriver::set_fx_param(self, strip, idx, n.as_f64().unwrap_or(0.0) as f32).await
    }
    fn patch_set(&self, _strip: StripId, _group: &str, _index: u8) -> anyhow::Result<String> {
        anyhow::bail!("xair: routing patch not supported (insrc is live-silent on FW 1.20)")
    }
    fn patch_clear(&self, _strip: StripId) -> anyhow::Result<()> {
        anyhow::bail!("xair: routing patch not supported (insrc is live-silent on FW 1.20)")
    }
    fn claims_device(&self, id: &str, model: &str) -> bool {
        let m = model.to_lowercase();
        id.starts_with("xair-") || m.contains("x-air") || m.contains("xr18")
    }
    fn dyn_gr(&self) -> bool {
        true
    }
    fn live_solo_bank(&self) -> bool {
        true
    }
    fn is_hardware(&self) -> bool {
        true
    }
    fn wants_meter_subscribe(&self) -> bool {
        true
    }
    fn probe_table(&self) -> Option<&'static str> {
        Some("xair")
    }
    async fn clear_solo(&self) -> anyhow::Result<()> {
        self.clear_solo().await
    }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        scene.recall_to(self, fade_ms).await
    }
});

impl_dyn_driver!(openmix_proto_dl32s::Dl32sDriver {

    fn claims_device(&self, id: &str, model: &str) -> bool {
        let m = model.to_lowercase();
        id.starts_with("dl32s-") || m.contains("dl32s") || m.contains("dl ")
    }
    fn is_hardware(&self) -> bool {
        true
    }
    fn group_assign_caps(&self) -> (bool, bool) {
        (true, true)
    }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        scene.recall_to(self, fade_ms).await
    }
});

/// One registered provider: its key, default address, and erased driver.
/// New providers append one row in [`registered_drivers`].
pub struct RegisteredDriver {
    /// Driver key (`sim`/`xair`/`dl32s`), also the `caps.driver` value.
    pub name: &'static str,
    /// Address used when `connect` omits one.
    pub default_addr: &'static str,
    /// The erased driver.
    pub driver: Arc<dyn DynDriver>,
}

impl RegisteredDriver {
    /// Merged param registry for this provider: the canonical catalogue
    /// plus the driver's contributed [`DynDriver::param_defs`] rows
    /// (canonical ids win on collision — see
    /// [`ParamRegistry::register`](openmix_core::params::ParamRegistry::register)).
    /// The engine builds one per driver at boot and resolves every caps /
    /// poll / snapshot / recall path through it, so a mixer with unique
    /// DSP arrives without editing core.
    pub fn param_registry(&self) -> ParamRegistry {
        let mut reg = ParamRegistry::canonical();
        reg.register(self.driver.param_defs());
        reg
    }
}

/// Build the registry in connect-device precedence order (sim first as the
/// fallback, then hardware in the historical if/else priority). The engine
/// holds these in a `HashMap<String, RegisteredDriver>`; the `Vec` order
/// is the device-claim and display order.
pub fn registered_drivers(preset: Option<openmix_sim::SimPreset>) -> Vec<RegisteredDriver> {
    let sim = match preset {
        Some(p) => openmix_sim::SimDriver::with_preset(p),
        None => openmix_sim::SimDriver::new(),
    };
    vec![
        RegisteredDriver {
            name: "sim",
            default_addr: "sim://local",
            driver: Arc::new(sim),
        },
        RegisteredDriver {
            name: "xair",
            default_addr: "192.168.1.1",
            driver: Arc::new(openmix_proto_xair::XAirDriver::new()),
        },
        RegisteredDriver {
            name: "dl32s",
            default_addr: "192.168.1.1",
            driver: Arc::new(openmix_proto_dl32s::Dl32sDriver::new()),
        },
    ]
}

/// Registry as a name-keyed map for the session.
pub fn driver_map(preset: Option<openmix_sim::SimPreset>) -> HashMap<String, RegisteredDriver> {
    registered_drivers(preset)
        .into_iter()
        .map(|r| (r.name.to_string(), r))
        .collect()
}

/// Sorted driver keys for `unknown driver` diagnostics.
pub fn driver_names(map: &HashMap<String, RegisteredDriver>) -> Vec<String> {
    let mut names: Vec<String> = map.keys().cloned().collect();
    names.sort();
    names
}

/// Parse an xair numbered-param id (`par01`..`par64`, 1-based wire) into
/// the 0-based trait index. Design ids (`mix`, `rate`, …) have no verified
/// xair mapping and return `None`.
pub(crate) fn parse_par_id(id: &str) -> Option<u8> {
    let rest = id.strip_prefix("par")?;
    let n: u8 = rest.parse().ok()?;
    if (1..=64).contains(&n) {
        Some(n - 1)
    } else {
        None
    }
}
