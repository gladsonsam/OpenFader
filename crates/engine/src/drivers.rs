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
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use openfader_core::{
    params::{ParamDef, ParamRegistry},
    MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue, StripFamily, StripId,
    StripKind,
};
use openfader_core::show::Scene;

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
    /// Driver-specific canonical range overrides, merged into this
    /// provider's [`ParamRegistry`](openfader_core::params::ParamRegistry)
    /// at boot (see [`RegisteredDriver::param_registry`]). Defaults to
    /// empty (canonical ranges); mirrors
    /// [`MixerDriver::param_range_overrides`].
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        &[]
    }
    fn meter_taps(&self) -> &'static [&'static str];
    fn bus_count(&self) -> u8;
    fn subgroup_count(&self) -> u8;
    fn mute_group_count(&self) -> u8;
    fn file_transfer(&self) -> bool;
    fn subscribe_updates(&self) -> Option<tokio::sync::broadcast::Receiver<ParamUpdate>>;
    fn poll_meters(&self) -> MeterFrame;
    fn poll_spectrum(&self) -> Vec<openfader_core::AudioSpectrum>;
    /// Read-only native desk snapshot view (see
    /// [`MixerDriver::native_snapshots`](openfader_core::MixerDriver::native_snapshots)):
    /// `None` = the driver does not model native slots (honest
    /// `unsupported`, never faked). Sync lock-local read, no I/O.
    fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        None
    }
    async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()>;
    async fn connect(&self, addr: &str) -> anyhow::Result<()>;
    async fn disconnect(&self) -> anyhow::Result<()> { Ok(()) }
    async fn refresh_streams(&self) -> anyhow::Result<()> { Ok(()) }
    fn telemetry_timestamps(&self) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> { None }
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
    /// an honest error).
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
    /// Bulk subgroup-assign snapshot: ((strip, group), member) pairs, or
    /// `None` when the driver has no bulk source (the engine falls back to
    /// per-strip `get_subgroup_assign` reads). In-memory drivers serve this
    /// from one lock acquisition instead of ~strips × groups awaits.
    fn sub_assign_bulk(&self) -> Option<std::collections::HashMap<(u16, u8), bool>> {
        None
    }
    /// Bulk mute-group-assign snapshot, mirroring
    /// [`sub_assign_bulk`](DynDriver::sub_assign_bulk).
    fn mg_assign_bulk(&self) -> Option<std::collections::HashMap<(u16, u8), bool>> {
        None
    }
    /// Bulk solo snapshot: strip -> soloed, or `None` for the per-strip
    /// `get_solo` fallback. Memory-solo drivers serve this from one lock.
    fn solo_bulk(&self) -> Option<std::collections::HashMap<u16, bool>> {
        None
    }
    /// Bulk `source`-param snapshot: strip -> raw text value, or `None`
    /// for the per-strip `get_param` fallback. The engine still applies
    /// the `parse_source` filter, so unpatched strips stay silent.
    fn source_bulk(&self) -> Option<std::collections::HashMap<u16, String>> {
        None
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
    // ---- provider tuning (defaults keep session logic brand-free) ----
    /// Whether this provider's `get_param` reads back genuine desk state
    /// after a write (R2 preset-truth contract).
    ///
    /// `true` (default): the read path is independent of the write path
    /// (live OSC query on X-Air, in-memory model on sim), so a post-write
    /// re-read that agrees with the write is desk evidence and the engine
    /// may cache/broadcast it as `verified`.
    ///
    /// `false`: `get_param` serves the same push cache the writes populate
    /// (DL32S: the desk never echoes own writes and there is no per-param
    /// query, only pushes). Any post-write "agreement" would be our own
    /// optimism, not the desk, so the engine must NOT treat it as
    /// confirmation: `refresh_param` short-circuits to `None` and the write
    /// echo goes out as `accepted` (write ACK, no value proof). Standard
    /// providers keep the default; only push-cache-only drivers override.
    fn readback_confirms_writes(&self) -> bool {
        true
    }
    /// Desk-proven mute-group master for verification paths (`read_param`
    /// `_mute_group`, poll/refresh): withholds own-write-pending values.
    /// Default forwards to the display getter (providers with genuine
    /// readback); push-cache-only drivers override to their verified read.
    /// Display/snapshot/health paths keep using [`get_mute_group`](Self::get_mute_group)
    /// (continuity, no proof claimed).
    async fn get_mute_group_verified(&self, group: u8) -> anyhow::Result<bool> {
        self.get_mute_group(group).await
    }
    /// Desk-proven mute-group assign for verification paths (`read_param`
    /// `_mgN`): withholds own-write-pending words. Same default/override
    /// contract as [`get_mute_group_verified`](Self::get_mute_group_verified).
    async fn get_mute_group_assign_verified(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        self.get_mute_group_assign(ch, group).await
    }
    /// Display name for a discovered device.
    fn display_name(&self, model: &str) -> String {
        format!("{} {model}", self.name())
    }
    /// The link needs an explicit hydration pass after connect.
    fn needs_hydration(&self) -> bool {
        false
    }
    /// Explicit demo visualization, never a provider's audio-frequency feed.
    fn demo_spectrum(&self, _ch: usize) -> Option<Vec<f32>> {
        None
    }
    /// Strip count behind GR broadcast.
    fn gr_channel_count(&self) -> usize {
        self.strip_count()
    }
    /// Max concurrent poller queries for one tick.
    fn poll_inflight(&self) -> usize {
        // Rig-measured 2026-10-19 (XR18, routed LAN): 48 sequential
        // queries in 29 ms (~1600 q/s burst), so 4 sequential reads per
        // 50 ms tick (80 q/s average) runs with ~20x headroom; the
        // adaptive EMA still halves the batch while the desk is
        // saturated, so slow venue wifi backs off on its own.
        4
    }
    /// Focus-lane take cap for one tick (slow links take fewer).
    fn poll_focus_limit(&self, inflight: usize) -> usize {
        inflight
    }
    /// Visible-lane take for the remainder of one tick.
    fn poll_visible_take(&self, rest: usize) -> usize {
        rest
    }
    /// Guaranteed background share of one tick (0 = periodic only).
    fn poll_bg_share(&self, inflight: usize, tick: u64, remaining: usize) -> usize {
        if tick.is_multiple_of(inflight.max(2) as u64) {
            1.min(remaining)
        } else {
            0
        }
    }
    /// The priority lane stays armed across sweeps (slow subscribed links).
    fn prio_always_armed(&self) -> bool {
        false
    }
    /// Extra per-strip ids appended to the essentials/static poll lists.
    fn essentials_extra_ids(&self, kind: StripKind) -> &'static [&'static str] {
        let _ = kind;
        &[]
    }
    /// Static poll rows outside the per-strip plane (group planes, return
    /// switches, ...). Empty when the plane serves none.
    fn static_extra_rows(&self, topo: &[StripFamily]) -> Vec<(u16, &'static str)> {
        let _ = topo;
        Vec::new()
    }
    /// Scope restrictions applied to this provider's param registry
    /// (`(param id, allowed strip kinds)`; empty = none).
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> {
        Vec::new()
    }
    /// No native bulk snapshot: the engine may capture the canonical
    /// param plane instead (honest reads, never invented values).
    fn param_plane_snapshot(&self) -> bool {
        false
    }
    /// Discovered-device dial address (bare IPs may gain an explicit port
    /// when the desk answers there; other forms pass through untouched).
    fn dial_addr(&self, ip: &str) -> String {
        ip.to_string()
    }
    /// Desk-native FX type name for an effect index (Numbered racks).
    fn fx_type_name(&self, type_index: u8) -> Option<&'static str> {
        let _ = type_index;
        None
    }
    /// Param count for a desk-native effect index (Numbered racks).
    fn fx_type_param_count(&self, type_index: u8) -> Option<usize> {
        let _ = type_index;
        None
    }
    /// Reverse lookup: effect name back to its desk type index (Numbered
    /// racks validate names against the desk table).
    fn fx_type_index_of(&self, fx_type: &str) -> Option<u8> {
        let _ = fx_type;
        None
    }
    /// Desk-native FX type catalogue behind `caps.fx` (Numbered racks).
    fn fx_type_caps(&self) -> Vec<crate::protocol::FxTypeCaps> {
        Vec::new()
    }
    /// Named-show recall on this driver (no default: every provider
    /// forwards to [`Scene::recall_to`]).
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()>;
}

/// Implement [`DynDriver`] for one concrete driver: forward the whole
/// [`MixerDriver`] surface (fully-qualified calls dodge inherent-method
/// shadowing, some drivers expose native same-named methods with
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
        fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
            MixerDriver::param_range_overrides(self)
        }
        async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()> {
            MixerDriver::set_spectrum_target(self, owner, strip).await
        }
        fn poll_spectrum(&self) -> Vec<openfader_core::AudioSpectrum> {
            MixerDriver::poll_spectrum(self)
        }
        fn meter_taps(&self) -> &'static [&'static str] {
            MixerDriver::meter_taps(self)
        }
        fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
            MixerDriver::native_snapshots(self)
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
        async fn disconnect(&self) -> anyhow::Result<()> { MixerDriver::disconnect(self).await }
        async fn refresh_streams(&self) -> anyhow::Result<()> { MixerDriver::refresh_streams(self).await }
        fn telemetry_timestamps(&self) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> { MixerDriver::telemetry_timestamps(self) }
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

impl_dyn_driver!(openfader_sim::SimDriver {

    fn fx_flavor(&self) -> FxFlavor {
        FxFlavor::Design
    }
    fn fx_slot_count(&self) -> usize {
        openfader_sim::SIM_FX_SLOTS
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
        openfader_sim::SIM_PATCH_GROUPS
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
    fn demo_spectrum(&self, ch: usize) -> Option<Vec<f32>> {
        Some(openfader_sim::SimDriver::demo_spectrum(self, ch))
    }
    fn gr_channel_count(&self) -> usize {
        MixerDriver::strip_count(self).min(crate::engine::GR_CHANNELS)
    }
    /// In-memory bulk snapshots behind the engine's attach fast path
    /// (one lock each instead of ~1000 serial awaits; identical content).
    fn sub_assign_bulk(&self) -> Option<std::collections::HashMap<(u16, u8), bool>> {
        Some(self.bulk_sub_assign())
    }
    fn mg_assign_bulk(&self) -> Option<std::collections::HashMap<(u16, u8), bool>> {
        Some(self.bulk_mg_assign())
    }
    fn solo_bulk(&self) -> Option<std::collections::HashMap<u16, bool>> {
        Some(self.bulk_solos())
    }
    fn source_bulk(&self) -> Option<std::collections::HashMap<u16, String>> {
        Some(self.bulk_sources())
    }
    async fn recall_show(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()> {
        scene.recall_to(self, fade_ms).await
    }
});

impl_dyn_driver!(openfader_proto_xair::XAirDriver {

    /// Mute-group membership rides the strip `{root}/grp/mute` bitmask
    /// (rig-verified XR18 FW 1.20, 2026-10-20, the driver serves
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
    fn display_name(&self, model: &str) -> String {
        format!("X-Air {model}")
    }
    fn needs_hydration(&self) -> bool {
        true
    }
    fn poll_focus_limit(&self, inflight: usize) -> usize {
        inflight.saturating_sub(1).min(1)
    }
    fn poll_visible_take(&self, rest: usize) -> usize {
        rest.min(1)
    }
    fn poll_bg_share(&self, inflight: usize, tick: u64, remaining: usize) -> usize {
        if inflight > 1 || tick.is_multiple_of(inflight.max(2) as u64) {
            1.min(remaining)
        } else {
            0
        }
    }
    fn prio_always_armed(&self) -> bool {
        true
    }
    fn essentials_extra_ids(&self, kind: StripKind) -> &'static [&'static str] {
        if matches!(kind, StripKind::In) {
            &["solo"]
        } else {
            &[]
        }
    }
    fn static_extra_rows(&self, topo: &[StripFamily]) -> Vec<(u16, &'static str)> {
        let mut out = Vec::new();
        for group in 0..4 {
            out.push((group, "_mute_group"));
        }
        for fam in topo {
            if matches!(
                fam.kind,
                StripKind::In | StripKind::Ret | StripKind::Bus | StripKind::FxSnd
            ) {
                for strip in fam.first..fam.first + fam.count {
                    for id in ["_mg1", "_mg2", "_mg3", "_mg4"] {
                        out.push((strip as u16, id));
                    }
                }
            }
            if fam.kind == StripKind::In {
                for strip in fam.first..fam.first + fam.count {
                    out.push((strip as u16, "rtnsw"));
                }
            }
        }
        out
    }
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> {
        // XR18 LR has dynamics but no external key-source leaf.
        const SCOPES: &[StripKind] = &[StripKind::In, StripKind::Bus];
        vec![("comp_key_src", SCOPES)]
    }
    fn param_plane_snapshot(&self) -> bool {
        true
    }
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
            return format!("{ip}:{}", openfader_proto_xair::addrs::PORT);
        }
        ip.to_string()
    }
    fn fx_type_name(&self, type_index: u8) -> Option<&'static str> {
        openfader_proto_xair::fx::fx_effect_name(type_index)
    }
    fn fx_type_param_count(&self, type_index: u8) -> Option<usize> {
        openfader_proto_xair::fx::fx_param_count(type_index).map(|n| n as usize)
    }
    fn fx_type_index_of(&self, fx_type: &str) -> Option<u8> {
        (0..=openfader_proto_xair::fx::FX_TYPE_MAX).find(|&index| {
            openfader_proto_xair::fx::fx_effect_name(index) == Some(fx_type)
                || openfader_proto_xair::fx::fx_effect_full_name(index) == Some(fx_type)
        })
    }
    fn fx_type_caps(&self) -> Vec<crate::protocol::FxTypeCaps> {
        use openfader_proto_xair::fx as xfx;
        let mut out = Vec::new();
        for index in 0..=xfx::FX_TYPE_MAX {
            let Some(code) = xfx::fx_effect_name(index) else {
                continue;
            };
            let label = xfx::fx_effect_full_name(index).unwrap_or(code);
            let mut params = Vec::new();
            for par in 1..=xfx::PARAM_COUNT as u8 {
                let Some(def) = xfx::fx_param_def(index, par) else {
                    continue;
                };
                let mut label = def.label.to_string();
                if let Some(bank) = def.bank {
                    label.push(' ');
                    label.push_str(bank);
                }
                if def.inferred {
                    label.push('?');
                }
                params.push(crate::protocol::FxParamCaps {
                    id: format!("par{par:02}"),
                    label,
                    unit: def.unit.to_string(),
                    min: 0.0,
                    max: 1.0,
                    default: 0.5,
                    scale: "lin".to_string(),
                });
            }
            out.push(crate::protocol::FxTypeCaps {
                id: code.to_string(),
                label: label.to_string(),
                params,
            });
        }
        out
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
        openfader_proto_xair::XAirDriver::get_fx_param_with_type(self, slot, param, effect).await
    }
    /// Live solo bank in one query (connect-burst fan-out): 1 query, not 16.
    async fn solo_bank(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        let _ = channels;
        openfader_proto_xair::XAirDriver::solo_bank(self).await
    }
    /// Desk-native type select (`/fx/N/type`, rig-verified 2026-10-19):
    /// resolve the caps-advertised name (short code `id` like `HALL`/`VRM`
    /// or full display `label` like `Hall Reverb`, same `proto-xair`
    /// effect table behind `caps.fx`) to the firmware index, then write
    /// it. Unknown names bail locally, never hitting the desk.
    async fn fx_set_type(&self, slot: u8, fx_type: &str) -> anyhow::Result<()> {
        let index = (0..=openfader_proto_xair::fx::FX_TYPE_MAX)
            .find(|&i| {
                openfader_proto_xair::fx::fx_effect_name(i) == Some(fx_type)
                    || openfader_proto_xair::fx::fx_effect_full_name(i) == Some(fx_type)
            })
            .ok_or_else(|| anyhow::anyhow!("unknown fx type {fx_type:?}"))?;
        openfader_proto_xair::XAirDriver::set_fx_type(self, slot, index).await
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

impl_dyn_driver!(openfader_proto_dl32s::Dl32sDriver {

    fn claims_device(&self, id: &str, model: &str) -> bool {
        let m = model.to_lowercase();
        id.starts_with("dl32s-") || m.contains("dl32s") || m.contains("dl ")
    }
    /// DL32S `get_param` reads the same push cache its writes populate and
    /// the desk never echoes own writes: no readback can confirm a write.
    /// Post-write echoes stay `accepted` (see
    /// [`DynDriver::readback_confirms_writes`]).
    fn readback_confirms_writes(&self) -> bool {
        false
    }
    /// Desk-proven mute-group master (see [`DynDriver::get_mute_group_verified`]):
    /// withholds own-write-pending masters so `read_param` can never launder
    /// our own write as poll proof. Display paths keep the inclusive getter.
    async fn get_mute_group_verified(&self, group: u8) -> anyhow::Result<bool> {
        self.get_mute_group_verified(group as u32 + 1).await
    }
    /// Desk-proven mute-group assign (see [`DynDriver::get_mute_group_assign_verified`]).
    async fn get_mute_group_assign_verified(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        self.get_mute_group_assign_verified(ch, group).await
    }
    fn is_hardware(&self) -> bool {
        true
    }
    fn group_assign_caps(&self) -> (bool, bool) {
        (true, true)
    }
    fn poll_inflight(&self) -> usize {
        // Mackie correlated request/response: no concurrency headroom.
        1
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
    /// (canonical ids win on collision, see
    /// [`ParamRegistry::register`](openfader_core::params::ParamRegistry::register)),
    /// then the driver's
    /// [`DynDriver::param_range_overrides`] bounds over the canonical rows
    /// (ranges only, kinds, scopes and defaults are untouched, and every
    /// other provider keeps canonical ranges). The engine builds one per
    /// driver at boot and resolves every caps / poll / snapshot / recall
    /// path through it, so a mixer with unique DSP arrives without editing
    /// core.
    pub fn param_registry(&self) -> ParamRegistry {
        let mut reg = ParamRegistry::canonical();
        reg.register(self.driver.param_defs());
        for (id, lo, hi) in self.driver.param_range_overrides() {
            reg.override_range(id, *lo, *hi);
        }
        reg
    }
}

/// Build the registry in connect-device precedence order (sim first as the
/// fallback, then hardware in the historical if/else priority). The engine
/// holds these in a `HashMap<String, RegisteredDriver>`; the `Vec` order
/// is the device-claim and display order.
pub fn registered_drivers(preset: Option<openfader_sim::SimPreset>) -> Vec<RegisteredDriver> {
    let sim = match preset {
        Some(p) => openfader_sim::SimDriver::with_preset(p),
        None => openfader_sim::SimDriver::new(),
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
            driver: Arc::new(openfader_proto_xair::XAirDriver::new()),
        },
        RegisteredDriver {
            name: "dl32s",
            default_addr: "192.168.1.1",
            driver: Arc::new(openfader_proto_dl32s::Dl32sDriver::new()),
        },
    ]
}

/// Registry as a name-keyed map for the session.
pub fn driver_map(preset: Option<openfader_sim::SimPreset>) -> HashMap<String, RegisteredDriver> {
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
