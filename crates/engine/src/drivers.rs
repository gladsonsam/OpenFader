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
//! flags). The `impl_dyn_driver` macro forwards the whole
//! [`MixerDriver`] surface; each provider then overrides only the
//! extensions it supports (defaults are honestly unsupported).
//!
//! A new provider: implement [`DynDriver`] here (mirror macro +
//! extension overrides) and append one [`RegisteredDriver`] row in
//! [`registered_drivers`]. `engine.rs` needs zero edits.

pub mod routing;
mod send_destinations;

use std::{
    collections::{BTreeMap, HashMap},
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use openfader_core::show::Scene;
use openfader_core::{
    params::{ParamDef, ParamRegistry},
    MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue, StripFamily,
    StripId, StripKind,
};

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

/// Accepted route plus its parameter-plane projection, when one exists.
/// The engine uses its normal accepted/verified provenance for that projection.
pub struct RoutingPatch {
    pub label: String,
    pub param: Option<(&'static str, ParamValue)>,
}

/// A provider-named patch destination within each input (e.g. A/B).
#[derive(Clone, Debug)]
pub struct RoutingTarget {
    pub key: &'static str,
    pub label: &'static str,
    pub clearable: bool,
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

/// Provider-owned generic-send destination. Stable strip keys preserve the
/// destination's live name without assuming a particular console's slot order.
#[derive(Clone, Debug)]
pub struct SendDestination {
    pub key: String,
    pub label: String,
    pub stereo: bool,
    pub allow_self: bool,
}

/// Object-safe mirror of [`MixerDriver`] plus the engine-only extensions.
/// Defaults are honestly unsupported; providers override what they serve.
/// Provider-neutral description of a soundcheck route and its guards.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SoundcheckProfile {
    pub capture_channels: u16,
    pub output_channels: u16,
    pub output_mask: Vec<bool>,
    pub route_changes: Vec<(String, i32)>,
    pub snapshot_keys: Vec<String>,
    pub driver_state: serde_json::Value,
}

/// Provider-owned USB interface hints and verified physical capture labels.
#[derive(Clone, Debug, serde::Serialize)]
pub struct RecorderDescriptor {
    pub interface_names: Vec<String>,
    pub capture_channels: u16,
    pub output_channels: u16,
    pub channel_names: Vec<String>,
    pub capture_strip_ids: Vec<Vec<u16>>,
}

impl SoundcheckProfile {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (1..=64).contains(&self.capture_channels),
            "invalid capture channel count"
        );
        anyhow::ensure!(
            (1..=64).contains(&self.output_channels),
            "invalid output channel count"
        );
        anyhow::ensure!(
            self.output_mask.len() == usize::from(self.output_channels),
            "output mask length mismatch"
        );
        Ok(())
    }
}

/// One provider-validated selector to restore, in provider-defined order.
#[derive(Debug, Clone)]
pub struct SceneRouteChange {
    pub target: &'static str,
    pub desired: i32,
    pub max: i64,
    pub source: Option<(&'static str, u8)>,
}

#[async_trait::async_trait]
pub trait DynDriver: Send + Sync {
    fn console_controls_capability(
        &self,
    ) -> openfader_core::console_controls::ConsoleControlsCapability {
        openfader_core::console_controls::ConsoleControlsCapability {
            available: false,
            unavailable_reason: Some(
                "Native console controls are not available for this provider.".into(),
            ),
        }
    }
    async fn console_controls_node(
        &self,
        _path: &str,
    ) -> anyhow::Result<openfader_core::console_controls::ConsoleControlNode> {
        anyhow::bail!("native console controls unavailable")
    }
    async fn console_control_write(
        &self,
        _request: &openfader_core::console_controls::ConsoleControlWrite,
        _check: openfader_core::native_library::Check<'_>,
    ) -> anyhow::Result<openfader_core::console_controls::ConsoleControlResult> {
        anyhow::bail!("native console controls unavailable")
    }
    async fn diagnostics(
        &self,
        _offset: usize,
        _limit: usize,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(
            serde_json::json!({"supported": false, "reason": "This provider does not expose raw diagnostics.", "rows": [], "offset": 0, "total": 0}),
        )
    }

    /// Read-only identification, independent of the active mixer session.
    /// New providers implement this and discovery alongside their driver.
    async fn identify(
        &self,
        _ip: IpAddr,
        _budget: std::time::Duration,
    ) -> Option<openfader_transport::discovery::DiscoveredDevice> {
        None
    }
    async fn discover(
        &self,
        _budget: std::time::Duration,
    ) -> Vec<openfader_transport::discovery::DiscoveredDevice> {
        Vec::new()
    }
    fn discovered_name(&self, device: &openfader_transport::discovery::DiscoveredDevice) -> String {
        self.display_name(&device.model)
    }
    /// Read-only control-path liveness, independent of cached display values.
    /// Providers with push caches must override this with a bounded wire query.
    async fn control_health(&self) -> anyhow::Result<()> {
        self.get_main_fader().await.map(|_| ())
    }
    // ---- MixerDriver mirror (every provider forwards these) ----
    fn name(&self) -> &'static str;
    fn channel_count(&self) -> usize;
    fn strip_count(&self) -> usize;
    fn topology(&self) -> Vec<StripFamily>;
    fn stereo_pairs(&self) -> Vec<[u16; 2]> {
        Vec::new()
    }
    /// All strips affected by a shared linked control. Providers keep
    /// independent input controls local; the engine checks every member
    /// before any write. An unavailable link state must fail closed.
    async fn control_members(&self, strip: StripId, _param: &str) -> anyhow::Result<Vec<StripId>> {
        Ok(vec![strip])
    }

    fn current_control_members(
        &self,
        _strip: StripId,
        _param: &str,
    ) -> anyhow::Result<Option<Vec<StripId>>> {
        Ok(None)
    }

    async fn set_stereo_link(
        &self,
        _source: StripId,
        _linked: bool,
    ) -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        anyhow::bail!("stereo linking unavailable")
    }
    fn capabilities(&self) -> &'static [&'static str];
    /// Provider-contributed param definitions, merged with the canonical
    /// catalogue at engine boot (see [`RegisteredDriver::param_registry`]).
    /// Defaults to empty (canonical ids only); mirrors
    /// [`MixerDriver::param_defs`].
    fn param_defs(&self) -> &'static [ParamDef] {
        &[]
    }
    /// Optional per-channel automix gain parameter used by telemetry.
    fn automix_gain_param(&self) -> Option<&'static str> {
        None
    }
    /// Timeout budget and concurrency policy for initial master readback.
    fn initial_read_timeout(&self) -> std::time::Duration {
        openfader_transport::session::DEFAULT_QUERY_TIMEOUT
    }
    fn concurrent_initial_master_reads(&self) -> bool {
        false
    }
    fn capture_input_links_with_global(&self) -> bool {
        false
    }
    fn guarded_bus_writes(&self) -> bool {
        false
    }
    fn guarded_link_writes(&self) -> bool {
        false
    }
    fn guarded_send_writes(&self) -> bool {
        false
    }
    fn guarded_preamp_writes(&self) -> bool {
        false
    }
    fn canonical_send_field<'a>(&self, field: &'a str) -> std::borrow::Cow<'a, str> {
        std::borrow::Cow::Borrowed(field)
    }
    /// Fields invalidated together by provider preamp updates.
    fn preamp_invalidation_fields(&self, _source: bool) -> &'static [&'static str] {
        &[]
    }
    /// Number of input strips addressed by provider preamp invalidations.
    fn preamp_invalidation_count(&self) -> usize {
        0
    }
    /// Derived metadata remains readable but must not become a recall write.
    fn param_read_only(&self, _param: &str) -> bool {
        false
    }
    /// Stored companion fields that bind a local-scene write to its source.
    fn param_write_context(&self, _param: &str) -> &'static [&'static str] {
        &[]
    }
    /// Whether a parameter is meaningful for this strip's current source.
    /// Errors reject capture; false omits a genuinely inapplicable value.
    async fn capture_param_applicable(
        &self,
        _strip: StripId,
        _param: &str,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }

    /// Fresh capture override for shared/dynamically read-only hardware fields.
    /// None keeps the existing cache/display capture path. An error omits the
    /// value rather than turning unavailable hardware state into a recall write.
    async fn capture_param_override(
        &self,
        _strip: StripId,
        _param: &str,
    ) -> Option<anyhow::Result<ParamValue>> {
        None
    }

    /// Semantic comparison for a structured provider value after a genuine
    /// desk read. None retains the canonical numeric/bool/text comparison.
    fn param_readback_equivalent(
        &self,
        _param: &str,
        _read: &ParamValue,
        _written: &ParamValue,
    ) -> Option<bool> {
        None
    }

    /// Optional local-scene recall group override. Console send numbering and
    /// contributed IDs need not follow the historical bus/FX catalogue split.
    /// Unspecified IDs retain the engine's legacy classification.
    fn param_recall_group(&self, _param: &str) -> Option<&'static str> {
        None
    }

    fn send_destination(&self, _slot: u16) -> Option<SendDestination> {
        None
    }
    /// Driver-specific canonical range overrides, merged into this
    /// provider's [`ParamRegistry`]
    /// at boot (see [`RegisteredDriver::param_registry`]). Defaults to
    /// empty (canonical ranges); mirrors
    /// [`MixerDriver::param_range_overrides`].
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        &[]
    }
    fn legacy_native_processor_required(&self, _field: &str, _value: &ParamValue) -> anyhow::Result<Option<openfader_core::native_processor::NativeProcessorDomain>> { Ok(None) }
    async fn capture_native_processors(&self, _check: openfader_core::native_library::Check<'_>) -> anyhow::Result<Option<openfader_core::native_processor::NativeProcessorSnapshot>> { Ok(None) }
    fn validate_native_processors(&self, _snapshot: &openfader_core::native_processor::NativeProcessorSnapshot) -> anyhow::Result<()> { anyhow::bail!("native processor snapshots unavailable") }
    async fn revalidate_captured_native_processors(&self, _snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, _check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { anyhow::bail!("native processor snapshots unavailable") }
    async fn preflight_native_processors(&self, _snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, _check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { anyhow::bail!("native processor snapshots unavailable") }
    async fn recall_native_processors(&self, _snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, _check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { anyhow::bail!("native processor snapshots unavailable") }

    fn param_kind_ranges(&self) -> &'static [openfader_core::params::ParamKindRange] {
        &[]
    }
    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        &[]
    }
    fn param_label_overrides(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
    fn param_swatches(&self, _param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        &[]
    }
    fn param_symbols(&self, _param: &str) -> &'static [openfader_core::params::ParamSymbol] {
        &[]
    }
    fn param_text_rules(&self, _param: &str) -> Option<openfader_core::params::ParamTextRules> {
        None
    }
    fn param_numeric_display(&self, _param: &str) -> Option<(f32, f32, &'static str)> {
        None
    }
    fn param_dependents(&self, _param: &str) -> &'static [&'static str] {
        &[]
    }
    async fn get_param_display(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        self.get_param(strip, param).await
    }
    async fn set_param_guarded(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &BTreeMap<String, ParamValue>,
    ) -> anyhow::Result<()> {
        for (id, wanted) in expected {
            anyhow::ensure!(
                self.get_param_display(strip, id).await? == *wanted,
                "Input source changed; reopen the control."
            );
        }
        self.set_param(strip, param, value).await
    }
    /// Recheck authorization immediately before a provider dispatches a
    /// shared control. Providers with shared state invoke this under their
    /// serialization lane with the freshly resolved affected members.
    async fn set_param_guarded_checked(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &BTreeMap<String, ParamValue>,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        check(&[strip])?;
        self.set_param_guarded(strip, param, value, expected).await
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
    /// [`MixerDriver::native_snapshots`]):
    /// `None` = the driver does not model native slots (honest
    /// `unsupported`, never faked). Sync lock-local read, no I/O.
    fn native_library(&self) -> Option<serde_json::Value> {
        None
    }
    async fn read_lan(&self) -> anyhow::Result<openfader_core::lan::LanSnapshot>;
    async fn write_lan(
        &self,
        expected: openfader_core::lan::LanSnapshot,
        desired: openfader_core::lan::LanConfiguration,
        authorize: &(dyn Fn() -> bool + Send + Sync),
        apply_requested: bool,
    ) -> Result<openfader_core::lan::LanWriteResult, openfader_core::lan::LanWriteError>;
    async fn native_library_action(
        &self,
        _action: &serde_json::Value,
        _expected: &serde_json::Value,
        _check: openfader_core::native_library::Check<'_>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("Native library unsupported")
    }
    fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        None
    }
    /// Fail closed unless the provider serializes writes and fresh independent
    /// readback, checking the entire captured block and observed revisions.
    fn input_processor_guard(&self) -> bool {
        false
    }
    /// Complete EQ/filter/gate/compressor block, including enable flags.
    /// Identity, preamps, sources, mix levels and assignments are excluded.
    fn channel_processing_guard(&self) -> bool {
        false
    }
    async fn input_processor(
        &self,
        _strip: StripId,
        _processor: &str,
        _write: Option<(&str, ParamValue)>,
        _expected: Option<&serde_json::Value>,
        _check: openfader_core::input_processors::RecallCheck<'_>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("guarded input processor recall unavailable for this provider")
    }
    fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> {
        Vec::new()
    }
    async fn set_destination_control_guarded(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
    ) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
        let _ = (target, control, value, expected, snapshot);
        anyhow::bail!("guarded processor recall not supported")
    }
    async fn set_fx_control_guarded(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        let _ = (target, control, value, expected, snapshot, context);
        anyhow::bail!("guarded FX unavailable")
    }
    async fn set_routing_control_guarded(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        let _ = (target, control, value, expected, snapshot, context);
        anyhow::bail!("guarded routing control unavailable")
    }
    async fn read_link_settings(
        &self,
    ) -> anyhow::Result<Option<openfader_core::link_settings::StereoLinkSettings>> {
        Ok(None)
    }

    async fn read_automix_settings(&self) -> anyhow::Result<Option<[bool; 4]>> {
        Ok(None)
    }
    /// Fresh complete per-target GEQ inventory; None means unsupported.
    async fn read_output_geq(
        &self,
        _target: &str,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        Ok(None)
    }
    async fn read_output_taps(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::routing::OutputTapState>>> {
        Ok(None)
    }

    /// Fresh complete monitor inventory; None means unsupported, incomplete reads error.
    /// Independent OSC reads cannot form an atomic snapshot during desk edits.
    async fn read_monitor_settings(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        Ok(None)
    }

    /// Fresh complete RTA inventory; None means unsupported.
    async fn read_rta_settings(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        Ok(None)
    }

    /// Source-bound physical tap recall; unsupported providers reject it.
    async fn set_output_tap_guarded(
        &self,
        desired: &openfader_core::routing::OutputTapState,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        let _ = (desired, expected, snapshot, context);
        anyhow::bail!("source-bound output tap recall unavailable")
    }
    async fn set_output_processor_guarded(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
        anyhow::ensure!(context(), "Output processor context changed");
        let result = self
            .set_destination_control_guarded(target, control, value, expected, snapshot)
            .await?;
        anyhow::ensure!(
            context(),
            "Output processor result uncertain: context changed"
        );
        Ok(result)
    }
    async fn set_target_solo(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        let _ = (target, control, value, expected, snapshot, context);
        anyhow::bail!("target solo unsupported")
    }
    async fn set_destination_control(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
    ) -> anyhow::Result<()> {
        let _ = (target, control, value, expected);
        anyhow::bail!("{}: destination controls not supported", self.name())
    }
    async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()>;
    async fn connect(&self, addr: &str) -> anyhow::Result<()>;
    async fn disconnect(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn refresh_streams(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn telemetry_timestamps(
        &self,
    ) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> {
        None
    }
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
    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool)
        -> anyhow::Result<()>;
    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool>;
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()>;
    async fn get_mute_group_name(&self, group: u8) -> anyhow::Result<String>;
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool>;
    async fn set_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()>;
    async fn set_mute_group_assign_checked(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        check(&[ch])?;
        self.set_mute_group_assign(ch, group, member).await
    }
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool>;
    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>>;
    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()>;
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()>;
    async fn get_main_fader(&self) -> anyhow::Result<f32>;
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()>;
    async fn get_main_mute(&self) -> anyhow::Result<bool>;
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()>;
    async fn set_solo_checked(
        &self,
        strip: StripId,
        soloed: bool,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        check(&[strip])?;
        self.set_solo(strip, soloed).await
    }
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool>;
    /// Verification/poll read: push-cache providers must withhold own writes.
    async fn get_solo_verified(&self, strip: StripId) -> anyhow::Result<bool> {
        self.get_solo(strip).await
    }
    async fn set_param(&self, strip: StripId, param: &str, value: ParamValue)
        -> anyhow::Result<()>;
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue>;
    /// Use confirmed observations for mutation echoes, avoiding a full scan.
    /// Incomplete caches emit no legacy rows; explicit snapshots still read live.
    fn mutation_state_from_observations(&self) -> bool {
        false
    }
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot>;
    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()>;
    async fn recall_scene(&self, scene: &Scene, fade_ms: u64) -> anyhow::Result<()>;
    async fn query_info(&self) -> anyhow::Result<MixerInfo>;
    /// Observed console clock rate. None means unknown; shared code must
    /// not infer hardware clocking from the engine's DSP/simulator rate.
    async fn reported_sample_rate(&self) -> Option<u32> {
        None
    }

    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<tokio::sync::broadcast::Receiver<MeterFrame>>;
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

    /// Sparse solo snapshot, preserving the legacy bank for other drivers.
    async fn solo_states(&self, channels: usize) -> anyhow::Result<Vec<(StripId, bool)>> {
        Ok(self
            .solo_bank(channels)
            .await?
            .into_iter()
            .enumerate()
            .map(|(i, value)| (StripId(i as u16), value))
            .collect())
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
    /// Whether numbered snapshots can read and restore a slot insert switch.
    fn fx_insert_supported(&self) -> bool {
        false
    }
    async fn get_fx_insert(&self, _slot: u8) -> anyhow::Result<bool> {
        anyhow::bail!("{}: FX insert unavailable", self.name())
    }
    async fn set_fx_insert(&self, _slot: u8, _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("{}: FX insert unavailable", self.name())
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
    async fn fx_push(
        &self,
        strip: u8,
        param: &str,
        value: &serde_json::Value,
    ) -> anyhow::Result<()> {
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
    /// Preserve provider-defined legacy target aliases at the write guard.
    fn canonical_routing_target<'a>(&self, target: &'a str) -> &'a str {
        target
    }
    fn routing_control(&self, _target: &str) -> &str {
        "source"
    }
    fn output_route(&self, _id: &str) -> anyhow::Result<openfader_core::routing::RouteState> {
        anyhow::bail!("Output routing unsupported")
    }
    async fn set_output_route(
        &self,
        _id: &str,
        _source: &str,
        _expected: &openfader_core::routing::RouteState,
        _context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::bail!("Output routing unsupported")
    }
    fn routing_planes(&self) -> Vec<crate::protocol::RoutingPlaneCaps> {
        routing::input_planes(self)
    }
    fn routing_targets(&self) -> Vec<RoutingTarget> {
        if self.routing_groups().is_empty() {
            Vec::new()
        } else {
            vec![RoutingTarget {
                key: "",
                label: "Input",
                clearable: true,
            }]
        }
    }
    async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> {
        anyhow::ensure!(target.is_empty(), "unknown routing target {target:?}");
        match self.get_param(strip, "source").await? {
            ParamValue::Text(label) => Ok(label),
            _ => anyhow::bail!("routing source is not a label"),
        }
    }
    async fn recorder_descriptor(&self) -> anyhow::Result<Option<RecorderDescriptor>> {
        Ok(None)
    }

    /// Build a fresh provider-owned soundcheck profile, or None when unsupported.
    async fn soundcheck_profile(&self) -> anyhow::Result<Option<SoundcheckProfile>> {
        Ok(None)
    }

    /// Read a provider-owned soundcheck route using its opaque guard state.
    async fn soundcheck_route_read(
        &self,
        _key: &str,
        _driver_state: &serde_json::Value,
        _check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<i32> {
        anyhow::bail!("soundcheck routing unsupported")
    }

    /// Compare and write a route, returning the updated opaque guard state.
    async fn soundcheck_route_write(
        &self,
        _key: &str,
        _driver_state: &serde_json::Value,
        _expected: i32,
        _desired: i32,
        _check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("soundcheck routing unsupported")
    }

    /// Fresh provider-owned soundcheck preflight, or None when unsupported.
    async fn soundcheck_readback(&self) -> anyhow::Result<Option<serde_json::Value>> {
        Ok(None)
    }
    /// Whether input patch writes require a current opaque route guard.
    fn input_patch_requires_guard(&self) -> bool {
        false
    }
    /// Validate a displayed row against its captured provider route state.
    fn input_route_guard_matches(
        &self,
        _target: &str,
        _group: &str,
        _index: u8,
        _guard: &serde_json::Value,
    ) -> bool {
        true
    }
    /// Parse provider-owned scene input routes; unsupported drivers return none.
    fn scene_input_routes(
        &self,
        _routing: &[crate::engine::StoredRoute],
        _strips: &[crate::engine::StoredStrip],
        _topo: &[openfader_core::StripFamily],
    ) -> anyhow::Result<Option<std::collections::BTreeMap<u16, Vec<SceneRouteChange>>>> {
        Ok(None)
    }
    fn scene_input_route_target(&self, _target: &str) -> bool {
        false
    }
    /// Legacy typed selector adapter, available only for providers implementing it.
    fn selector_change(
        &self,
        _expected: crate::engine::selector_session::InputRouteSelectors,
        _desired: crate::engine::selector_session::InputRouteSelectors,
    ) -> anyhow::Result<(&'static str, Option<(&'static str, u8)>)> {
        anyhow::bail!("input selector unsupported")
    }
    fn selector_pair_valid(&self, _left: u16) -> bool {
        false
    }
    /// Optional provider-owned opaque input-route state for guarded commands.
    async fn input_route_guard(
        &self,
        _strip: StripId,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        Ok(None)
    }
    /// Fresh link state for an even, zero-based input pair; None means unsupported.
    async fn input_pair_link(&self, _left: StripId) -> anyhow::Result<Option<bool>> {
        Ok(None)
    }
    /// Guarded pair-bank write; expected contains both complete route JSON objects.
    async fn linked_input_bank_guarded(
        &self,
        _left: StripId,
        _expected: [serde_json::Value; 2],
        _bank: i32,
        _check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::bail!("linked input bank unsupported")
    }
    async fn input_routes_bulk(
        &self,
    ) -> Option<std::collections::HashMap<(u16, String), (String, serde_json::Value)>> {
        None
    }
    async fn patch_target_guarded(
        &self,
        strip: StripId,
        target: &str,
        source: Option<(&str, u8)>,
        _guard: Option<&serde_json::Value>,
        check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<RoutingPatch> {
        check(&[strip])?;
        match source {
            Some((group, index)) => self.patch_target_set(strip, target, group, index).await,
            None => self.patch_target_clear(strip, target).await,
        }
    }
    async fn patch_target_set(
        &self,
        strip: StripId,
        target: &str,
        group: &str,
        index: u8,
    ) -> anyhow::Result<RoutingPatch> {
        anyhow::ensure!(target.is_empty(), "unknown routing target {target:?}");
        self.patch_set(strip, group, index)
            .map(|label| RoutingPatch { label, param: None })
    }
    async fn patch_target_clear(
        &self,
        strip: StripId,
        target: &str,
    ) -> anyhow::Result<RoutingPatch> {
        anyhow::ensure!(target.is_empty(), "unknown routing target {target:?}");
        self.patch_clear(strip).map(|()| RoutingPatch {
            label: "-".to_string(),
            param: None,
        })
    }
    /// Param changes that also invalidate the routing frame.
    fn routing_param_ids(&self) -> &'static [&'static str] {
        &[]
    }
    /// Per-strip meter taps behind the `taps` frames (`None` = synthesize
    /// from `poll_meters`, like hardware does).
    fn poll_taps(&self) -> Option<Vec<TapLevels>> {
        None
    }
    /// Serves gate/comp GR over the `gate_gr`/`comp_gr` param path.
    fn output_meter_targets(&self) -> Vec<String> {
        Vec::new()
    }
    fn poll_output_meters(&self) -> Option<openfader_core::OutputMeterSnapshot> {
        None
    }
    fn poll_dynamics_meters(&self) -> Option<Vec<openfader_core::DynamicsMeterFrame>> {
        None
    }
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
    /// A provider can opt into atomic whole-mask local snapshot handling.
    fn mute_group_snapshot_supported(&self) -> bool {
        false
    }
    fn mute_group_snapshot_applies(&self, _kind: StripKind) -> bool {
        false
    }
    async fn snapshot_mute_group_mask(&self, _strip: StripId) -> anyhow::Result<u32> {
        anyhow::bail!("{}: mute-group snapshot unsupported", self.name())
    }
    async fn restore_mute_group_mask(
        &self,
        _strip: StripId,
        _mask: u32,
        _check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::bail!("{}: mute-group snapshot unsupported", self.name())
    }
    async fn restore_mute_group_master(
        &self,
        _group: u8,
        _muted: bool,
        _check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<()> {
        anyhow::bail!("{}: mute-group snapshot unsupported", self.name())
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
    /// Advertised clear-solo coverage; unknown providers withhold the action.
    fn clear_solo_scope(&self) -> Option<&'static str> {
        None
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
    /// Strip families served by the existing input patch contracts.
    fn routing_strip_kind(&self, kind: StripKind) -> bool {
        kind == StripKind::In
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
    /// Slot-aware reverse lookup: effect name back to its desk type index
    /// for one slot. Rack type tables can differ by slot domain, so the
    /// bare [`fx_type_index_of`](DynDriver::fx_type_index_of) answer is
    /// ambiguous there. Defaults to the slot-independent table;
    /// slot-domained racks override.
    fn fx_type_index_for_slot(&self, slot: u8, fx_type: &str) -> Option<u8> {
        let _ = slot;
        self.fx_type_index_of(fx_type)
    }
    /// Slot-aware desk-native FX type name. Rack type tables can differ by
    /// slot domain (X32 slots 1..4 vs 5..8); the bare
    /// [`fx_type_name`](DynDriver::fx_type_name) index is ambiguous there.
    /// Defaults to the slot-independent table; slot-domained racks override.
    fn fx_type_name_for_slot(&self, slot: u8, type_index: u8) -> Option<&'static str> {
        let _ = slot;
        self.fx_type_name(type_index)
    }
    /// Slot-aware param count for a desk-native effect index. Defaults to
    /// the slot-independent table; slot-domained racks override.
    fn fx_type_param_count_for_slot(&self, slot: u8, type_index: u8) -> Option<usize> {
        let _ = slot;
        self.fx_type_param_count(type_index)
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
        fn stereo_pairs(&self) -> Vec<[u16; 2]> { MixerDriver::stereo_pairs(self) }
        async fn control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
            MixerDriver::control_members(self, strip, param).await
        }
        async fn set_stereo_link(&self, source: StripId, linked: bool)
            -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
            MixerDriver::set_stereo_link(self, source, linked).await
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
        fn legacy_native_processor_required(&self, field: &str, value: &ParamValue) -> anyhow::Result<Option<openfader_core::native_processor::NativeProcessorDomain>> { MixerDriver::legacy_native_processor_required(self, field, value) }
        async fn capture_native_processors(&self, check: openfader_core::native_library::Check<'_>) -> anyhow::Result<Option<openfader_core::native_processor::NativeProcessorSnapshot>> { MixerDriver::capture_native_processors(self, check).await }
        fn validate_native_processors(&self, snapshot: &openfader_core::native_processor::NativeProcessorSnapshot) -> anyhow::Result<()> { MixerDriver::validate_native_processors(self, snapshot) }
        async fn revalidate_captured_native_processors(&self, snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { MixerDriver::revalidate_captured_native_processors(self, snapshot, check).await }
        async fn preflight_native_processors(&self, snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { MixerDriver::preflight_native_processors(self, snapshot, check).await }
        async fn recall_native_processors(&self, snapshot: &openfader_core::native_processor::NativeProcessorSnapshot, check: openfader_core::native_library::Check<'_>) -> anyhow::Result<()> { MixerDriver::recall_native_processors(self, snapshot, check).await }

        fn param_kind_ranges(&self) -> &'static [openfader_core::params::ParamKindRange] {
            MixerDriver::param_kind_ranges(self)
        }
        fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
            MixerDriver::param_presentations(self)
        }
        fn param_label_overrides(&self) -> Vec<(&'static str, String)> {
            MixerDriver::param_label_overrides(self)
        }
        fn param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
            MixerDriver::param_swatches(self, param)
        }
        fn param_symbols(&self, param: &str) -> &'static [openfader_core::params::ParamSymbol] {
            MixerDriver::param_symbols(self, param)
        }
        fn param_text_rules(&self, param: &str) -> Option<openfader_core::params::ParamTextRules> {
            MixerDriver::param_text_rules(self, param)
        }
        fn param_numeric_display(&self, param: &str) -> Option<(f32, f32, &'static str)> {
            MixerDriver::param_numeric_display(self, param)
        }
        async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()> {
            MixerDriver::set_spectrum_target(self, owner, strip).await
        }
        fn output_meter_targets(&self) -> Vec<String> { MixerDriver::output_meter_targets(self) }
        fn poll_output_meters(&self) -> Option<openfader_core::OutputMeterSnapshot> { MixerDriver::poll_output_meters(self) }
        fn poll_dynamics_meters(&self) -> Option<Vec<openfader_core::DynamicsMeterFrame>> {
            MixerDriver::poll_dynamics_meters(self)
        }
        fn poll_spectrum(&self) -> Vec<openfader_core::AudioSpectrum> {
            MixerDriver::poll_spectrum(self)
        }
        fn meter_taps(&self) -> &'static [&'static str] {
            MixerDriver::meter_taps(self)
        }
        fn native_library(&self) -> Option<serde_json::Value> { MixerDriver::native_library(self) }
        fn console_controls_capability(&self) -> openfader_core::console_controls::ConsoleControlsCapability { MixerDriver::console_controls_capability(self) }
        async fn console_controls_node(&self, path: &str) -> anyhow::Result<openfader_core::console_controls::ConsoleControlNode> { MixerDriver::console_controls_node(self, path).await }
        async fn console_control_write(&self, request: &openfader_core::console_controls::ConsoleControlWrite, check: openfader_core::native_library::Check<'_>) -> anyhow::Result<openfader_core::console_controls::ConsoleControlResult> { MixerDriver::console_control_write(self, request, check).await }
        async fn read_lan(&self) -> anyhow::Result<openfader_core::lan::LanSnapshot> { MixerDriver::read_lan(self).await }
        async fn write_lan(&self, expected: openfader_core::lan::LanSnapshot, desired: openfader_core::lan::LanConfiguration,
            authorize: &(dyn Fn() -> bool + Send + Sync), apply_requested: bool) -> Result<openfader_core::lan::LanWriteResult, openfader_core::lan::LanWriteError> {
            MixerDriver::write_lan(self, expected, desired, authorize, apply_requested).await
        }
        async fn native_library_action(&self, action: &serde_json::Value, expected: &serde_json::Value,
            check: openfader_core::native_library::Check<'_>) -> anyhow::Result<serde_json::Value> {
            MixerDriver::native_library_action(self, action, expected, check).await
        }
        fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
            MixerDriver::native_snapshots(self)
        }
    fn input_processor_guard(&self) -> bool { MixerDriver::input_processor_guard(self) }
    fn channel_processing_guard(&self) -> bool { MixerDriver::channel_processing_guard(self) }
    async fn input_processor(&self, strip: StripId, processor: &str, write: Option<(&str, ParamValue)>, expected: Option<&serde_json::Value>, check: openfader_core::input_processors::RecallCheck<'_>) -> anyhow::Result<serde_json::Value> {
        MixerDriver::input_processor(self, strip, processor, write, expected, check).await
    }
        fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> {
            MixerDriver::destination_controls(self)
        }
    async fn set_destination_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl]) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
        MixerDriver::set_destination_control_guarded(self, target, control, value, expected, snapshot).await
    }
        async fn set_fx_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
            MixerDriver::set_fx_control_guarded(self, target, control, value, expected, snapshot, context).await
        }
        async fn set_routing_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
            MixerDriver::set_routing_control_guarded(self, target, control, value, expected, snapshot, context).await
        }
        async fn read_link_settings(&self) -> anyhow::Result<Option<openfader_core::link_settings::StereoLinkSettings>> { MixerDriver::read_link_settings(self).await }
        async fn read_automix_settings(&self) -> anyhow::Result<Option<[bool; 4]>> { MixerDriver::read_automix_settings(self).await }
        async fn read_output_geq(&self, target: &str) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> { MixerDriver::read_output_geq(self, target).await }
        async fn read_output_taps(&self) -> anyhow::Result<Option<Vec<openfader_core::routing::OutputTapState>>> { MixerDriver::read_output_taps(self).await }
        async fn read_rta_settings(&self) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> { MixerDriver::read_rta_settings(self).await }
        async fn read_monitor_settings(&self) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> { MixerDriver::read_monitor_settings(self).await }
        async fn set_output_tap_guarded(&self, desired: &openfader_core::routing::OutputTapState, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
            MixerDriver::set_output_tap_guarded(self, desired, expected, snapshot, context).await
        }
        async fn set_output_processor_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
            MixerDriver::set_output_processor_guarded(self, target, control, value, expected, snapshot, context).await
        }
        async fn set_target_solo(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
            MixerDriver::set_target_solo(self, target, control, value, expected, snapshot, context).await
        }
        async fn set_destination_control(&self, target: &str, control: &str, value: f32, expected: f32) -> anyhow::Result<()> {
            MixerDriver::set_destination_control(self, target, control, value, expected).await
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
        async fn get_mute_group_name(&self, group: u8) -> anyhow::Result<String> {
            MixerDriver::get_mute_group_name(self, group).await
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

fn soundcheck_key(key: &str, count: u16) -> anyhow::Result<(u16, usize)> {
    let parts: Vec<_> = key.split('/').collect();
    anyhow::ensure!(
        parts.len() == 3 && parts[0] == "input",
        "invalid routing key"
    );
    let channel: u16 = parts[1].parse()?;
    anyhow::ensure!(channel < count, "invalid input channel");
    let index = match parts[2] {
        "analog" => 0,
        "usb" => 1,
        "bank" => 2,
        _ => anyhow::bail!("invalid selector"),
    };
    Ok((channel, index))
}
fn soundcheck_guard(
    state: &serde_json::Value,
    channel: u16,
    route: [i32; 3],
) -> anyhow::Result<()> {
    if !state.is_null() {
        let expected = &state["expected"][channel.to_string()];
        anyhow::ensure!(
            *expected == serde_json::json!({"analog":route[0],"usb":route[1],"bank":route[2]}),
            "routing conflict: input/{channel}"
        );
    }
    Ok(())
}
fn soundcheck_update(
    state: &serde_json::Value,
    channel: u16,
    route: [i32; 3],
) -> serde_json::Value {
    let mut updated = state.clone();
    if !updated.is_null() {
        updated["expected"][channel.to_string()] =
            serde_json::json!({"analog":route[0],"usb":route[1],"bank":route[2]});
    }
    updated
}

impl_dyn_driver!(openfader_sim::SimDriver {
    async fn reported_sample_rate(&self) -> Option<u32> { Some(crate::engine::DESK_SAMPLE_RATE) }
    fn selector_change(&self, expected: crate::engine::selector_session::InputRouteSelectors, desired: crate::engine::selector_session::InputRouteSelectors) -> anyhow::Result<(&'static str, Option<(&'static str, u8)>)> {
        crate::engine::providers::xair_routes::selector_change(expected, desired)
    }
    fn selector_pair_valid(&self, left: u16) -> bool { left < 16 && left.is_multiple_of(2) }
    async fn soundcheck_route_read(&self, key: &str, state: &serde_json::Value, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<i32> {
        let (channel, index) = soundcheck_key(key, 32)?;
        check(&[StripId(channel)])?;
        let route = self.selector_read(channel)?;
        check(&[StripId(channel)])?;
        soundcheck_guard(state, channel, route)?;
        Ok(route[index])
    }
    async fn soundcheck_route_write(&self, key: &str, state: &serde_json::Value, expected: i32, desired: i32, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<serde_json::Value> {
        let (channel, index) = soundcheck_key(key, 32)?;
        check(&[StripId(channel)])?;
        let route = self.selector_read(channel)?;
        soundcheck_guard(state, channel, route)?;
        anyhow::ensure!(route[index] == expected, "routing conflict");
        let mut next = route; next[index] = desired;
        self.selector_write(channel, route, next, check)?;
        check(&[StripId(channel)])?;
        anyhow::ensure!(self.selector_read(channel)? == next, "selector readback mismatch");
        check(&[StripId(channel)])?;
        Ok(soundcheck_update(state, channel, next))
    }

    async fn recorder_descriptor(&self) -> anyhow::Result<Option<RecorderDescriptor>> {
        let profile = self.soundcheck_profile().await?.ok_or_else(|| anyhow::anyhow!("simulation profile unavailable"))?;
        let mut names = vec![String::new(); usize::from(profile.capture_channels)];
        for strip in MixerDriver::snapshot(self).await?.strips {
            if strip.id.0 < profile.capture_channels {
                let [analog, _, bank] = self.selector_read(strip.id.0)?;
                if bank == 0 {
                    if let Some(name) = names.get_mut(analog as usize) { *name = strip.name; }
                }
            }
        }
        Ok(Some(RecorderDescriptor {
            // Explicit simulation-only interface labels, including the existing
            // generic multichannel backend fixture; never a hardware fallback.
            interface_names: vec!["OpenFader simulation fixture".into(), "generic".into()],
            capture_channels: profile.capture_channels,
            output_channels: profile.output_channels,
            channel_names: names,
            capture_strip_ids: Vec::new(),
        }))
    }

    async fn soundcheck_profile(&self) -> anyhow::Result<Option<SoundcheckProfile>> {
        let count = openfader_sim::SIM_TOPOLOGY.iter().find(|f| f.kind == StripKind::In)
            .ok_or_else(|| anyhow::anyhow!("missing simulator input topology"))?.count;
        let count = u16::try_from(count)?;
        let mut mask = vec![false; usize::from(count)];
        let mut changes = Vec::new();
        let mut keys = Vec::new();
        let mut expected = serde_json::Map::new();
        for channel in 0..count {
            let [analog, usb, bank] = self.selector_read(channel)?;
            if bank == 0 && (0..i32::from(count)).contains(&analog) {
                mask[analog as usize] = true;
                changes.push((format!("input/{channel}/usb"), analog));
                changes.push((format!("input/{channel}/bank"), 1));
                keys.extend(["analog", "usb", "bank"].map(|key| format!("input/{channel}/{key}")));
                expected.insert(channel.to_string(), serde_json::json!({"analog": analog, "usb": usb, "bank": bank}));
            }
        }
        let profile = SoundcheckProfile { capture_channels: count, output_channels: count, output_mask: mask, route_changes: changes, snapshot_keys: keys, driver_state: serde_json::json!({"expected": expected, "links": []}) };
        profile.validate()?;
        Ok(Some(profile))
    }

    async fn input_pair_link(&self, _left: StripId) -> anyhow::Result<Option<bool>> { Ok(Some(false)) }
    async fn input_route_guard(&self, strip: StripId) -> anyhow::Result<Option<serde_json::Value>> {
        let [analog, usb, bank] = self.selector_read(strip.0)?;
        Ok(Some(serde_json::json!({"analog": analog, "usb": usb, "bank": bank})))
    }
    async fn patch_target_guarded(&self, strip: StripId, target: &str, source: Option<(&str, u8)>, guard: Option<&serde_json::Value>, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<RoutingPatch> {
        let guard = guard.ok_or_else(|| anyhow::anyhow!("missing route guard"))?;
        let field = |name| -> anyhow::Result<i32> { Ok(i32::try_from(guard[name].as_i64().ok_or_else(|| anyhow::anyhow!("invalid route guard"))?)?) };
        let expected = [field("analog")?, field("usb")?, field("bank")?];
        let mut desired = expected;
        let (index, value) = match (target, source) {
            ("analog", None) => (0, 18),
            ("analog", Some(("Mic", n @ 1..=16))) => (0, i32::from(n) - 1),
            ("analog", Some(("Aux", n @ 1..=2))) => (0, i32::from(n) + 15),
            ("usb", Some(("USB", n @ 1..=18))) => (1, i32::from(n) - 1),
            ("bank", Some(("Bank", n @ 1..=2))) => (2, i32::from(n) - 1),
            _ => anyhow::bail!("invalid selector"),
        };
        desired[index] = value;
        self.selector_write(strip.0, expected, desired, check)?;
        Ok(RoutingPatch { label: value.to_string(), param: None })
    }

    fn clear_solo_scope(&self) -> Option<&'static str> { Some("advertised") }
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> { openfader_sim::strip_schema::scopes() }
    fn routing_strip_kind(&self, kind: StripKind) -> bool { kind == StripKind::In || kind == openfader_sim::strip_schema::RETURN_KIND }
    fn routing_planes(&self) -> Vec<crate::protocol::RoutingPlaneCaps> {
        let mut planes = routing::input_planes(self);
        planes.extend(routing::output_planes(openfader_sim::output_routing::planes()));
        planes
    }
    fn output_route(&self, id: &str) -> anyhow::Result<openfader_core::routing::RouteState> {
        self.output_route(id)
    }
    async fn set_output_route(&self, id: &str, source: &str, expected: &openfader_core::routing::RouteState, context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, context)
    }

    fn send_destination(&self, slot: u16) -> Option<SendDestination> {
        send_destinations::legacy_send_destination(slot, 6, 4)
    }


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
    fn selector_change(&self, expected: crate::engine::selector_session::InputRouteSelectors, desired: crate::engine::selector_session::InputRouteSelectors) -> anyhow::Result<(&'static str, Option<(&'static str, u8)>)> {
        crate::engine::providers::xair_routes::selector_change(expected, desired)
    }
    fn selector_pair_valid(&self, left: u16) -> bool { left < 16 && left.is_multiple_of(2) }
    async fn soundcheck_route_read(&self, key: &str, state: &serde_json::Value, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<i32> {
        if let Some(rest) = key.strip_prefix("input_pair/") {
            let (left, field) = rest.split_once('/').ok_or_else(|| anyhow::anyhow!("invalid pair"))?;
            let left: u16 = left.parse()?;
            anyhow::ensure!(left < 16 && left.is_multiple_of(2), "invalid pair");
            let members = [StripId(left), StripId(left + 1)];
            check(&members)?;
            let linked = matches!(MixerDriver::get_param(self, members[0], "link").await?, ParamValue::Bool(true));
            check(&members)?;
            if !state.is_null() { anyhow::ensure!(state["links"][usize::from(left / 2)].as_bool() == Some(linked), "input link changed"); }
            if field == "link" { return Ok(i32::from(linked)); }
            anyhow::ensure!(field == "bank" && linked, "input pair link changed");
            let a = self.input_route(members[0]).await?; check(&members)?;
            let b = self.input_route(members[1]).await?; check(&members)?;
            soundcheck_guard(state, left, [a.analog,a.usb,a.bank])?;
            soundcheck_guard(state, left + 1, [b.analog,b.usb,b.bank])?;
            anyhow::ensure!(a.bank == b.bank, "unequal pair banks");
            return Ok(a.bank);
        }
        let (channel,index) = soundcheck_key(key, 18)?;
        check(&[StripId(channel)])?;
        let route = self.input_route(StripId(channel)).await?;
        check(&[StripId(channel)])?;
        let route = [route.analog,route.usb,route.bank];
        soundcheck_guard(state, channel, route)?;
        Ok(route[index])
    }
    async fn soundcheck_route_write(&self, key: &str, state: &serde_json::Value, expected: i32, desired: i32, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<serde_json::Value> {
        if let Some(rest) = key.strip_prefix("input_pair/") {
            let (left, field) = rest.split_once('/').ok_or_else(|| anyhow::anyhow!("invalid pair"))?;
            let left: u16 = left.parse()?;
            anyhow::ensure!(field == "bank" && left < 16 && left.is_multiple_of(2), "invalid pair");
            anyhow::ensure!(DynDriver::soundcheck_route_read(self, key, state, check).await? == expected, "routing conflict");
            let members = [StripId(left), StripId(left + 1)];
            let a = self.input_route(members[0]).await?; check(&members)?;
            let b = self.input_route(members[1]).await?; check(&members)?;
            soundcheck_guard(state, left, [a.analog,a.usb,a.bank])?;
            soundcheck_guard(state, left + 1, [b.analog,b.usb,b.bank])?;
            anyhow::ensure!(a.bank == expected && b.bank == expected, "routing conflict");
            self.set_linked_input_bank_guarded(members[0], [a,b], desired, check).await?;
            check(&members)?;
            let updated = soundcheck_update(state, left, [a.analog,a.usb,desired]);
            return Ok(soundcheck_update(&updated, left + 1, [b.analog,b.usb,desired]));
        }
        let (channel,index) = soundcheck_key(key, 18)?;
        let member = StripId(channel); check(&[member])?;
        let route = self.input_route(member).await?; check(&[member])?;
        let values = [route.analog,route.usb,route.bank];
        soundcheck_guard(state, channel, values)?;
        anyhow::ensure!(values[index] == expected, "routing conflict");
        let mut next = route;
        match index { 0 => next.analog = desired, 1 => next.usb = desired, _ => next.bank = desired };
        self.set_input_route_guarded(member, route, next, check).await?;
        check(&[member])?;
        let actual = self.input_route(member).await?; check(&[member])?;
        anyhow::ensure!(actual == next, "selector readback mismatch");
        Ok(soundcheck_update(state, channel, [next.analog,next.usb,next.bank]))
    }

    async fn recorder_descriptor(&self) -> anyhow::Result<Option<RecorderDescriptor>> {
        let readback = openfader_proto_xair::XAirDriver::soundcheck_readback(self).await?;
        let preflight = openfader_proto_xair::soundcheck_profile::preflight(&readback)?;
        let mut tracks: Vec<Vec<u16>> = vec![Vec::new(); 18];
        for (&strip, route) in &preflight.expected {
            if route.bank == 0 {
                if let Some(ids) = tracks.get_mut(route.analog as usize) {
                    ids.push(strip);
                }
            }
        }
        for ids in &mut tracks { ids.sort_unstable(); }
        Ok(Some(RecorderDescriptor {
            // Preflight accepts only the adapter's verified model and firmware.
            interface_names: vec!["XR18".into()],
            capture_channels: 18,
            output_channels: 18,
            channel_names: vec![String::new(); tracks.len()],
            capture_strip_ids: tracks,
        }))
    }

    async fn soundcheck_profile(&self) -> anyhow::Result<Option<SoundcheckProfile>> {
        let readback = openfader_proto_xair::XAirDriver::soundcheck_readback(self).await?;
        let preflight = openfader_proto_xair::soundcheck_profile::preflight(&readback)?;
        let expected: serde_json::Map<String, serde_json::Value> = preflight.expected.iter().map(|(channel, route)| (channel.to_string(), serde_json::json!({"analog": route.analog, "usb": route.usb, "bank": route.bank}))).collect();
        let profile = SoundcheckProfile { capture_channels: 18, output_channels: 18, output_mask: preflight.output_mask.to_vec(), route_changes: preflight.plan, snapshot_keys: preflight.keys, driver_state: serde_json::json!({"expected": expected, "links": preflight.links}) };
        profile.validate()?;
        Ok(Some(profile))
    }

    async fn set_solo_checked(&self, strip: StripId, soloed: bool, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        openfader_proto_xair::XAirDriver::set_solo_checked(self, strip, soloed, check).await
    }

    fn current_control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Option<Vec<StripId>>> {
        self.current_output_control_members(strip, param).map(Some)
    }
    async fn input_routes_bulk(&self) -> Option<std::collections::HashMap<(u16, String), (String, serde_json::Value)>> {
        let reads = futures::future::join_all((0..16_u16).map(|i| self.input_route(StripId(i)))).await;
        let mut out = std::collections::HashMap::new();
        for (i, result) in reads.into_iter().enumerate() {
            let Ok(route) = result else { continue };
            let guard = serde_json::json!({"analog": route.analog, "usb": route.usb, "bank": route.bank});
            let analog = match route.analog { 0..=15 => format!("Mic {}", route.analog + 1), 16..=17 => format!("Aux {}", route.analog - 15), 18 => "-".to_string(), _ => continue };
            out.insert((i as u16, "analog".into()), (analog, guard.clone()));
            out.insert((i as u16, "usb".into()), (format!("USB {}", route.usb + 1), guard.clone()));
            out.insert((i as u16, "bank".into()), (format!("Bank {}", route.bank + 1), guard));
        }
        Some(out)
    }
    async fn input_pair_link(&self, left: StripId) -> anyhow::Result<Option<bool>> {
        match MixerDriver::get_param(self, left, "link").await? { ParamValue::Bool(value) => Ok(Some(value)), _ => anyhow::bail!("invalid link state") }
    }
    async fn linked_input_bank_guarded(&self, left: StripId, expected: [serde_json::Value; 2], bank: i32, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        let parse = |v: &serde_json::Value| -> anyhow::Result<openfader_proto_xair::input_routing::InputRoute> {
            let field = |key: &str| -> anyhow::Result<i32> { Ok(i32::try_from(v[key].as_i64().ok_or_else(|| anyhow::anyhow!("invalid route"))?)?) };
            Ok(openfader_proto_xair::input_routing::InputRoute { analog: field("analog")?, usb: field("usb")?, bank: field("bank")? })
        };
        self.set_linked_input_bank_guarded(left, [parse(&expected[0])?, parse(&expected[1])?], bank, check).await
    }
    async fn soundcheck_readback(&self) -> anyhow::Result<Option<serde_json::Value>> { Ok(Some(openfader_proto_xair::XAirDriver::soundcheck_readback(self).await?)) }
    async fn input_route_guard(&self, strip: StripId) -> anyhow::Result<Option<serde_json::Value>> {
        let route = self.input_route(strip).await?;
        Ok(Some(serde_json::json!({"analog": route.analog, "usb": route.usb, "bank": route.bank})))
    }
    async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> {
        let route = self.input_route(strip).await?;
        match target {
            "analog" => Ok(match route.analog { 0..=15 => format!("Mic {}", route.analog + 1), 16..=17 => format!("Aux {}", route.analog - 15), 18 => "-".to_string(), _ => anyhow::bail!("xair: invalid analog source") }),
            "usb" => Ok(format!("USB {}", route.usb + 1)),
            "bank" => Ok(format!("Bank {}", route.bank + 1)),
            _ => anyhow::bail!("xair: invalid input routing target"),
        }
    }
    async fn patch_target_guarded(&self, strip: StripId, target: &str, source: Option<(&str, u8)>, guard: Option<&serde_json::Value>, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<RoutingPatch> {
        let guard = guard.ok_or_else(|| anyhow::anyhow!("xair: input routing requires a captured route"))?;
        let field = |id| -> anyhow::Result<i32> {
            Ok(i32::try_from(guard.get(id).and_then(|v| v.as_i64()).ok_or_else(|| anyhow::anyhow!("xair: invalid captured input route"))?)?)
        };
        let expected = openfader_proto_xair::input_routing::InputRoute { analog: field("analog")?, usb: field("usb")?, bank: field("bank")? };
        let mut desired = expected;
        let label = match (target, source) {
            ("analog", None) => { desired.analog = 18; "-".to_string() },
            ("analog", Some(("Mic", n @ 1..=16))) => { desired.analog = i32::from(n) - 1; format!("Mic {n}") },
            ("analog", Some(("Aux", n @ 1..=2))) => { desired.analog = i32::from(n) + 15; format!("Aux {n}") },
            ("usb", Some(("USB", n @ 1..=18))) => { desired.usb = i32::from(n) - 1; format!("USB {n}") },
            ("bank", Some(("Bank", n @ 1..=2))) => { desired.bank = i32::from(n) - 1; format!("Bank {n}") },
            _ => anyhow::bail!("xair: invalid input route"),
        };
        self.set_input_route_guarded(strip, expected, desired, check).await?;
        Ok(RoutingPatch { label, param: None })
    }
    fn routing_planes(&self) -> Vec<crate::protocol::RoutingPlaneCaps> {
        // Keep both dormant banks and the active selector independently
        // addressable. Input rows are enabled only with guarded dispatch.
        use crate::protocol::{RoutingDestinationCaps, RoutingGroupCaps, RoutingPlaneCaps};
        let mut input = Vec::new();
        for (target, label, clearable, groups) in [
            ("analog", "Analog inputs", true, vec![("Mic", "Mic inputs", 16), ("Aux", "Aux L / R", 2)]),
            ("usb", "USB returns", false, vec![("USB", "USB returns", 18)]),
            ("bank", "Input mode", false, vec![("Bank", "Input mode", 2)]),
        ] {
            let plane = format!("input/{target}");
            let destinations = (0..16_u16).map(|strip| {
                let key = format!("ch/{:02}", strip + 1);
                RoutingDestinationCaps { output: false, id: format!("{plane}/{key}"), key, strip, label: format!("Channel {}", strip + 1), target: target.into(), clearable,
                    source_groups: groups.iter().map(|(key, label, count)| RoutingGroupCaps { key: (*key).into(), label: (*label).into(), count: *count,
                        options: if target == "bank" { vec!["Analog".into(), "USB".into()] } else { Vec::new() },
                        mode: target == "bank",
                    }).collect() }
            }).collect();
            input.push(RoutingPlaneCaps { key: plane, label: label.into(), destinations });
        }
        let stereo = openfader_core::routing::OutputPlane {
            key: "physical/stereo", label: "Main and phones", count: 2,
            clearable: false, sources: openfader_proto_xair::output_routing::stereo_sources(),
        };
        let mut outputs = routing::output_planes(vec![stereo]);
        for (destination, (id, label)) in outputs[0].destinations.iter_mut().zip([
            ("physical/main", "Main output"), ("physical/phones", "Phones output")
        ]) {
            destination.id = id.into(); destination.key = id.into(); destination.label = label.into();
        }
        outputs.extend(routing::output_planes(openfader_proto_xair::output_routing::planes()));
        input.extend(outputs);
        input
    }
    fn output_route(&self, id: &str) -> anyhow::Result<openfader_core::routing::RouteState> {
        self.output_route(id)
    }
    async fn set_output_route(&self, id: &str, source: &str, expected: &openfader_core::routing::RouteState, context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, context).await
    }
    fn param_write_context(&self, param: &str) -> &'static [&'static str] {
        if matches!(param, "gain" | "phantom") { &["source"] } else { &[] }
    }
    async fn capture_param_applicable(&self, strip: StripId, param: &str) -> anyhow::Result<bool> {
        if matches!(param, "gain" | "phantom") { self.preamp_capture_applicable(strip, param).await } else { Ok(true) }
    }
    async fn set_param_guarded(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>) -> anyhow::Result<()> {
        if matches!(param, "gain" | "phantom") {
            return self.write_active_preamp_guarded(strip, param, value, expected).await;
        }
        for (id, wanted) in expected {
            anyhow::ensure!(MixerDriver::get_param(self, strip, id).await? == *wanted, "Input source changed; reopen the control.");
        }
        MixerDriver::set_param(self, strip, param, value).await
    }
    async fn set_param_guarded_checked(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        if matches!(param, "gain" | "phantom") {
            return self.write_active_preamp_guarded_checked(strip, param, value, expected, check).await;
        }
        if matches!(param, "fader" | "mute") && self.current_bus_control_members(strip)?.is_some() {
            for (id, wanted) in expected {
                anyhow::ensure!(MixerDriver::get_param(self, strip, id).await? == *wanted, "Control changed; reopen the control.");
            }
            return self.set_bus_master_checked(strip, param, value, check).await;
        }
        if self.current_bus_control_members(strip)?.is_some() {
            for (id, wanted) in expected {
                anyhow::ensure!(MixerDriver::get_param(self, strip, id).await? == *wanted, "Control changed; reopen the control.");
            }
            return self.set_bus_param_checked(strip, param, value, check).await;
        }
        if is_xair_bus_send(param) {
            for (id, wanted) in expected {
                anyhow::ensure!(MixerDriver::get_param(self, strip, id).await? == *wanted, "Input source changed; reopen the control.");
            }
            return self.set_bus_send_checked(strip, param, value, check).await;
        }
        check(&[strip])?;
        self.set_param_guarded(strip, param, value, expected).await
    }
    async fn set_mute_group_assign_checked(&self, ch: StripId, group: u8, member: bool, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        openfader_proto_xair::XAirDriver::set_mute_group_assign_checked(self, ch, group, member, check).await
    }
    fn mute_group_snapshot_supported(&self) -> bool { true }
    fn mute_group_snapshot_applies(&self, kind: StripKind) -> bool {
        matches!(kind, StripKind::In | StripKind::Ret | StripKind::Bus | StripKind::FxSnd)
    }
    async fn snapshot_mute_group_mask(&self, strip: StripId) -> anyhow::Result<u32> {
        openfader_proto_xair::XAirDriver::snapshot_mute_group_mask(self, strip).await
    }
    async fn restore_mute_group_mask(&self, strip: StripId, mask: u32, check: &(dyn for<'a> Fn(&'a [StripId]) -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        openfader_proto_xair::XAirDriver::restore_mute_group_mask(self, strip, mask, check).await
    }
    async fn restore_mute_group_master(&self, group: u8, muted: bool, check: &(dyn Fn() -> anyhow::Result<()> + Send + Sync)) -> anyhow::Result<()> {
        openfader_proto_xair::XAirDriver::restore_mute_group_master(self, group, muted, check).await
    }
    fn fx_insert_supported(&self) -> bool { true }
    async fn get_fx_insert(&self, slot: u8) -> anyhow::Result<bool> {
        openfader_proto_xair::XAirDriver::get_fx_insert(self, slot).await
    }
    async fn set_fx_insert(&self, slot: u8, enabled: bool) -> anyhow::Result<()> {
        openfader_proto_xair::XAirDriver::set_fx_insert(self, slot, enabled).await
    }
    fn capture_input_links_with_global(&self) -> bool { true }
    fn guarded_bus_writes(&self) -> bool { true }
    fn guarded_link_writes(&self) -> bool { true }
    fn guarded_send_writes(&self) -> bool { true }
    fn guarded_preamp_writes(&self) -> bool { true }
    fn canonical_send_field<'a>(&self, field: &'a str) -> std::borrow::Cow<'a, str> {
        if let Some(n) = field.strip_prefix("aux").and_then(|s| s.strip_suffix("_level")) {
            if matches!(n, "1" | "2" | "3" | "4" | "5" | "6") {
                return std::borrow::Cow::Owned(format!("send{n}_level"));
            }
        }
        std::borrow::Cow::Borrowed(field)
    }
    fn scene_input_routes(&self, routing: &[crate::engine::StoredRoute], strips: &[crate::engine::StoredStrip], topo: &[openfader_core::StripFamily]) -> anyhow::Result<Option<std::collections::BTreeMap<u16, Vec<SceneRouteChange>>>> {
        crate::engine::providers::xair_routes::scene_input_routes(routing, strips, topo)
    }
    fn scene_input_route_target(&self, target: &str) -> bool { matches!(target, "analog" | "usb" | "bank") }
    fn input_patch_requires_guard(&self) -> bool { true }
    fn input_route_guard_matches(&self, target: &str, group: &str, index: u8, guard: &serde_json::Value) -> bool {
        let value = match target {
            "analog" => guard.get("analog").and_then(serde_json::Value::as_i64).map(|n| match n {
                0..=15 => ("Mic", n + 1), 16..=17 => ("Aux", n - 15), 18 => ("", 0), _ => ("?", -1),
            }),
            "usb" => guard.get("usb").and_then(serde_json::Value::as_i64).map(|n| ("USB", n + 1)),
            "bank" => guard.get("bank").and_then(serde_json::Value::as_i64).map(|n| ("Bank", n + 1)),
            _ => None,
        };
        value == Some((group, i64::from(index)))
    }
    fn preamp_invalidation_fields(&self, source: bool) -> &'static [&'static str] {
        if source { &["source", "xair:preamp_state", "gain", "phantom"] } else { &["gain", "phantom"] }
    }
    fn preamp_invalidation_count(&self) -> usize { 16 }
    fn automix_gain_param(&self) -> Option<&'static str> { Some("xair:automix_gain") }
    fn initial_read_timeout(&self) -> std::time::Duration { openfader_transport::session::CONNECT_BURST_TIMEOUT }
    fn concurrent_initial_master_reads(&self) -> bool { true }
    fn param_read_only(&self, param: &str) -> bool {
        // The bool loses the distinction between groups X and Y; snapshots
        // capture and restore the group enum instead. The source label is
        // derived from multiple desk fields and is not a routing write.
        matches!(param, "automix_on" | "source" | "xair:automix_gain" | "xair:preamp_state" | "usbreturn")
    }
    fn send_destination(&self, slot: u16) -> Option<SendDestination> {
        send_destinations::legacy_send_destination(slot, 6, 4)
    }

    async fn identify(&self, ip: IpAddr, budget: std::time::Duration) -> Option<openfader_transport::discovery::DiscoveredDevice> {
        use openfader_transport::discovery::{probe_addr, PROBE_TABLE};
        let probes = PROBE_TABLE.iter().filter(|s| s.driver == DynDriver::name(self))
            .map(|s| probe_addr(s, ip, budget));
        futures::future::join_all(probes).await.into_iter()
            .filter_map(Result::ok).flatten().next()
    }
    async fn discover(&self, budget: std::time::Duration) -> Vec<openfader_transport::discovery::DiscoveredDevice> {
        openfader_transport::discovery::discover_xair(budget).await.unwrap_or_default()
    }

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
        const DCA: &[StripKind] = &[StripKind::In, StripKind::Ret, StripKind::Bus, StripKind::FxSnd];
        vec![("comp_key_src", SCOPES), ("dca1_assign", DCA), ("dca2_assign", DCA), ("dca3_assign", DCA), ("dca4_assign", DCA)]
    }
    fn param_plane_snapshot(&self) -> bool {
        true
    }
    fn dial_addr(&self, ip: &str) -> String {
        openfader_transport::session::dial_addr_with_port(ip, openfader_proto_xair::addrs::PORT)
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
                let position = xfx::fx_position_def(index, par);
                let mut label = position.and_then(|p| p.registered_label).unwrap_or(def.label).to_string();
                if let Some(bank) = def.bank {
                    if position.is_none() || !label.ends_with(bank) {
                        label.push(' ');
                        label.push_str(bank);
                    }
                }
                if def.inferred && position.is_none() {
                    label.push('?');
                }
                let choices: Vec<_> = def.options.unwrap_or(&[]).iter().enumerate()
                    .map(|(i, label)| crate::protocol::FxChoiceCaps {
                        value: if def.options.unwrap_or(&[]).len() > 1 {
                            i as f32 / (def.options.unwrap_or(&[]).len() - 1) as f32
                        } else { 0.0 },
                        label: (*label).to_string(),
                    }).collect();
                params.push(crate::protocol::FxParamCaps {
                    choices,
                    position: position.map(|p| crate::protocol::FxPositionCaps {
                        steps: p.steps,
                        curve: if p.logarithmic { crate::protocol::FxPositionCurve::Log } else { crate::protocol::FxPositionCurve::Linear },
                        precision: p.precision,
                    }),
                    id: format!("par{par:02}"),
                    label,
                    unit: position.and_then(|p| p.unit).unwrap_or(def.unit).to_string(),
                    min: position.map_or(0.0, |h| h.min),
                    max: position.map_or(1.0, |h| h.max),
                    default: 0.5,
                    scale: if position.is_some() { "xair_positions" } else { "normalized" }.to_string(),
                });
            }
            let tap_target = xfx::fx_tap_target(index).and_then(|target| {
                let id = format!("par{:02}", target.par);
                params.iter().find(|p| p.id == id && p.unit == "ms" && p.position.as_ref().is_some_and(|position| position.steps > 0) && p.min.is_finite() && p.max.is_finite() && p.min == target.min_ms as f32 && p.max == target.max_ms as f32).map(|_| id)
            });
            out.push(crate::protocol::FxTypeCaps {
                tap_target,
                    slots: Vec::new(),
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
    async fn solo_states(&self, _channels: usize) -> anyhow::Result<Vec<(StripId, bool)>> {
        openfader_proto_xair::XAirDriver::solo_states(self).await
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
        let effect = DynDriver::fx_type_index_of(self, fx_type)
            .ok_or_else(|| anyhow::anyhow!("xair: unknown FX type {fx_type:?}"))?;
        let mut values = Vec::with_capacity(params.len());
        for (id, value) in params {
            let idx = parse_par_id(id).ok_or_else(|| anyhow::anyhow!("xair: invalid FX parameter {id:?}"))?;
            openfader_proto_xair::fx::check_value(*value)?;
            anyhow::ensure!(openfader_proto_xair::fx::fx_param_def(effect, idx + 1).is_some(), "xair: {id} unavailable for {fx_type}");
            values.push((idx, *value));
        }
        if self.get_fx_type(slot).await? != effect {
            self.set_fx_type(slot, effect).await?;
            let mut confirmed = false;
            for _ in 0..20 {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if self.get_fx_type(slot).await? == effect {
                    confirmed = true;
                    break;
                }
            }
            anyhow::ensure!(confirmed, "xair: FX {slot} type change unconfirmed; parameters were not sent");
        }
        for (idx, value) in values {
            self.set_fx_param_checked(slot, idx, value, Some(effect)).await?;
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
    async fn diagnostics(&self, offset: usize, limit: usize) -> anyhow::Result<serde_json::Value> {
        self.read_diagnostics(offset, limit).await
    }

    async fn soundcheck_profile(&self) -> anyhow::Result<Option<SoundcheckProfile>> {
        openfader_proto_dl32s::soundcheck::assess()?;
        Ok(None)
    }
    fn routing_strip_kind(&self, kind: StripKind) -> bool { matches!(kind, StripKind::In | StripKind::Ret) }
    fn scope_restrictions(&self) -> Vec<(&'static str, &'static [StripKind])> { openfader_proto_dl32s::strips::scopes() }
    fn routing_planes(&self) -> Vec<crate::protocol::RoutingPlaneCaps> {
        let mut planes = routing::input_planes(self);
        planes.extend(routing::output_planes(openfader_proto_dl32s::output_routing::planes()));
        planes
    }
    fn output_route(&self, id: &str) -> anyhow::Result<openfader_core::routing::RouteState> {
        self.output_route(id)
    }
    async fn set_output_route(&self, id: &str, source: &str, expected: &openfader_core::routing::RouteState, context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        self.set_output_route_guarded(id, source, expected, context).await
    }

    fn clear_solo_scope(&self) -> Option<&'static str> { Some("advertised") }
    fn send_destination(&self, slot: u16) -> Option<SendDestination> {
        send_destinations::legacy_send_destination(slot, 8, 4)
    }


    fn param_dependents(&self, param: &str) -> &'static [&'static str] {
        if matches!(param, "source_a" | "source_b" | "source_select") { &["gain", "phantom"] } else { &[] }
    }
    async fn get_param_display(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        if matches!(param, "gain" | "phantom") { self.preamp_display(strip, param).await }
        else { MixerDriver::get_param(self, strip, param).await }
    }
    async fn set_param_guarded(&self, strip: StripId, param: &str, value: ParamValue, expected: &BTreeMap<String, ParamValue>) -> anyhow::Result<()> {
        self.set_preamp_guarded(strip, param, value, expected).await
    }
    fn routing_groups(&self) -> Vec<RoutingGroup> {
        vec![RoutingGroup { key: "Mic", label: "Mic Pre", count: 32 },
             RoutingGroup { key: "USB", label: "USB", count: 32 },
             RoutingGroup { key: "Osc", label: "Oscillator", count: 1 }]
    }
    fn routing_targets(&self) -> Vec<RoutingTarget> {
        vec![RoutingTarget { key: "a", label: "A", clearable: true },
             RoutingTarget { key: "b", label: "B", clearable: true }]
    }
    fn routing_control(&self, target: &str) -> &str {
        match target { "a" => "source_a", "b" => "source_b", _ => "source" }
    }
    fn routing_param_ids(&self) -> &'static [&'static str] { &["source_a", "source_b"] }
    async fn routing_source(&self, strip: StripId, target: &str) -> anyhow::Result<String> {
        let raw = self.input_source_display(strip, target).await?;
        let (group, index) = match raw {
            0 => return Ok("-".to_string()),
            17000 => ("Osc", 1),
            v @ 1000..=1031 => ("Mic", v - 999),
            v @ 2000..=2031 => ("USB", v - 1999),
            _ => anyhow::bail!("dl32s: unlabelled source ID {raw}"),
        };
        Ok(format!("{group} {index}"))
    }
    async fn patch_target_set(&self, strip: StripId, target: &str, group: &str, index: u8) -> anyhow::Result<RoutingPatch> {
        let leaf = match target { "a" => "source_a", "b" => "source_b", _ => anyhow::bail!("dl32s: choose routing target A or B") };
        let raw = match (group, index) {
            ("Mic", 1..=32) => 999 + u32::from(index),
            ("USB", 1..=32) => 1999 + u32::from(index),
            ("Osc", 1) => 17000,
            _ => anyhow::bail!("dl32s: invalid source socket {group:?}/{index}"),
        };
        MixerDriver::set_param(self, strip, leaf, ParamValue::Float(raw as f32)).await?;
        Ok(RoutingPatch { label: format!("{group} {index}"), param: Some((leaf, ParamValue::Float(raw as f32))) })
    }
    async fn patch_target_clear(&self, strip: StripId, target: &str) -> anyhow::Result<RoutingPatch> {
        let leaf = match target { "a" => "source_a", "b" => "source_b", _ => anyhow::bail!("dl32s: choose routing target A or B") };
        MixerDriver::set_param(self, strip, leaf, ParamValue::Float(0.0)).await?;
        Ok(RoutingPatch { label: "-".to_string(), param: Some((leaf, ParamValue::Float(0.0))) })
    }
    fn essentials_extra_ids(&self, kind: StripKind) -> &'static [&'static str] {
        if matches!(kind, StripKind::In) { &["solo"] } else { &[] }
    }
    async fn get_solo_verified(&self, strip: StripId) -> anyhow::Result<bool> {
        self.get_solo_verified(strip).await
    }
    fn live_solo_bank(&self) -> bool { true }
    async fn solo_bank(&self, channels: usize) -> anyhow::Result<Vec<bool>> {
        // Never turn a missing/pending word into a confirmed Solo OFF.
        let mut bank = Vec::with_capacity(channels);
        for ch in 0..channels as u16 {
            bank.push(self.get_solo_verified(StripId(ch)).await?);
        }
        Ok(bank)
    }

    async fn control_health(&self) -> anyhow::Result<()> {
        self.check_control_health().await
    }

    fn discovered_name(&self, device: &openfader_transport::discovery::DiscoveredDevice) -> String {
        if device.raw.trim().is_empty() { "Mackie DL mixer".into() } else { device.raw.clone() }
    }

    async fn identify(&self, ip: IpAddr, budget: std::time::Duration) -> Option<openfader_transport::discovery::DiscoveredDevice> {
        let name = openfader_transport::mackie::identify(ip, budget).await.ok()?;
        Some(openfader_transport::discovery::DiscoveredDevice {
            driver: DynDriver::name(self),
            addr: SocketAddr::new(ip, openfader_transport::mackie::DEFAULT_PORT),
            model: "Mackie DL series".into(),
            raw: name,
        })
    }
    async fn discover(&self, budget: std::time::Duration) -> Vec<openfader_transport::discovery::DiscoveredDevice> {
        // No captured DL broadcast protocol yet. Search bounded directly
        // attached subnets with protocol identification, never an OUI guess.
        openfader_transport::discovery::discover_local(budget, |ip, wait| self.identify(ip, wait)).await
    }

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

mod wing;
mod x32;

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
        RegisteredDriver {
            name: "wing",
            default_addr: "192.168.1.1",
            driver: Arc::new(openfader_proto_wing::WingDriver::new()),
        },
        RegisteredDriver {
            name: "x32",
            default_addr: "192.168.1.1",
            driver: Arc::new(openfader_proto_x32::X32Driver::new()),
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

fn is_xair_bus_send(param: &str) -> bool {
    param
        .strip_prefix("send")
        .and_then(|s| s.split_once('_'))
        .is_some_and(|(n, leaf)| {
            matches!(n, "1" | "2" | "3" | "4" | "5" | "6")
                && matches!(leaf, "level" | "tap" | "pan" | "grpon")
        })
}

#[cfg(test)]
mod soundcheck_profile_tests {
    use super::*;

    #[test]
    fn validates_channel_counts_and_masks() {
        for count in [18, 32] {
            let mut profile = SoundcheckProfile {
                capture_channels: count,
                output_channels: count,
                output_mask: vec![true; count as usize],
                route_changes: vec![],
                snapshot_keys: vec![],
                driver_state: serde_json::Value::Null,
            };
            assert!(profile.validate().is_ok());
            profile.output_mask.pop();
            assert!(profile.validate().is_err());
            profile.output_mask.push(true);
            profile.capture_channels = 0;
            assert!(profile.validate().is_err());
            profile.capture_channels = 65;
            assert!(profile.validate().is_err());
        }
    }

    #[tokio::test]
    async fn mackie_soundcheck_reports_unverified_signal_path() {
        let driver = openfader_proto_dl32s::Dl32sDriver::new();
        let error = DynDriver::soundcheck_profile(&driver).await.unwrap_err();
        assert_eq!(
            error.downcast_ref::<openfader_proto_dl32s::soundcheck::UnavailableReason>(),
            Some(&openfader_proto_dl32s::soundcheck::UnavailableReason::UnverifiedSignalPath)
        );
    }
}

#[cfg(test)]
mod soundcheck_driver_tests {
    use super::*;

    #[tokio::test]
    async fn sim_channel_31_transaction_rejects_stale_state() {
        let sim = openfader_sim::SimDriver::new();
        let driver: &dyn DynDriver = &sim;
        let check = |_: &[StripId]| Ok(());
        let profile = driver.soundcheck_profile().await.unwrap().unwrap();
        assert_eq!(profile.capture_channels, 32);
        assert_eq!(
            driver
                .soundcheck_route_read("input/31/usb", &profile.driver_state, &check)
                .await
                .unwrap(),
            31
        );
        let state = driver
            .soundcheck_route_write("input/31/usb", &profile.driver_state, 31, 5, &check)
            .await
            .unwrap();
        assert!(driver
            .soundcheck_route_write("input/31/bank", &profile.driver_state, 0, 1, &check)
            .await
            .is_err());
        let state = driver
            .soundcheck_route_write("input/31/bank", &state, 0, 1, &check)
            .await
            .unwrap();
        assert_eq!(
            driver
                .soundcheck_route_read("input/31/bank", &state, &check)
                .await
                .unwrap(),
            1
        );
        assert!(driver
            .soundcheck_route_write("input/31/usb", &serde_json::Value::Null, 31, 0, &check)
            .await
            .is_err());
    }
}
