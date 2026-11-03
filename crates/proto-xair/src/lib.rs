//! Behringer X-Air OSC driver (XR12/16/18, MR18).
//!
//! Thin mapping over [`openfader_transport::OscSession`]: this crate owns ONLY the
//! X-Air address table + arg mapping. Sockets, keepalive, meter parsing, and
//! request/response plumbing all live in `openfader-transport`.
use openfader_core::{MixerDriver, MixerInfo, ParamValue, StripId};
use openfader_transport::{osc, KeepaliveConfig, OscSession};
use rosc::{OscMessage, OscType};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, Mutex};

pub mod automix;
mod automix_settings;
pub mod codec;
pub mod comp;
pub mod dyn_gr;
pub mod eq;
pub mod fx;
pub mod gate;
pub mod groups;
pub mod headamp;
pub mod input;
pub mod input_routing;
pub mod insert;
pub mod look;
pub mod mix;
mod native_library;
mod output_geq;
mod output_misc;
pub mod output_routing;
mod preamp;
mod presentations;
pub mod rta;
mod rta_settings;
pub mod solo;
pub mod soundcheck_profile;
pub mod soundcheck_readback;
mod sync_decode;

pub mod addrs;
mod assignments;
mod clock_rate;
mod discovery;
mod fx_driver;
mod inbound;
pub mod lan;
mod lifecycle;
mod link_policy;
mod link_settings;
mod mapping;
mod metadata;
mod midi_setup;
mod monitor;
mod monitor_settings;
mod mute_behavior;
mod param_read;
mod param_write;
mod safe_levels;
mod surface;
mod target_solo;
mod telemetry;
mod usb_interface;

use discovery::*;
pub use inbound::parse_inbound;
use mapping::*;
pub use metadata::{XAIR_CAPABILITIES, XAIR_STRIPS, XAIR_TOPOLOGY};

type LevelSample = Option<(std::time::Instant, Vec<f32>)>;
type AutomixGainCache = std::sync::Arc<tokio::sync::Mutex<Option<(std::time::Instant, [f32; 16])>>>;
type LinkCache = std::sync::Arc<std::sync::Mutex<[Option<(std::time::Instant, bool)>; 8]>>;

type BusLinkCache =
    std::sync::Arc<std::sync::Mutex<[(u64, Option<(std::time::Instant, bool)>); 3]>>;

/// Live XR18-class mixer on UDP 10024.
pub struct XAirDriver {
    receive_since: std::sync::Arc<std::sync::Mutex<std::time::Instant>>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    connection_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    connected: std::sync::atomic::AtomicBool,
    lan_operations: tokio::sync::Mutex<()>,
    link_cache: LinkCache,
    safe_levels: std::sync::Arc<std::sync::Mutex<safe_levels::Cache>>,
    safe_levels_write: tokio::sync::Mutex<()>,
    usb_interface: std::sync::Arc<std::sync::Mutex<usb_interface::Cache>>,
    clock_rate: std::sync::Arc<std::sync::Mutex<clock_rate::Cache>>,
    clock_rate_write: tokio::sync::Mutex<()>,
    main_mute_write: tokio::sync::Mutex<()>,
    usb_interface_write: tokio::sync::Mutex<()>,
    target_solo: std::sync::Arc<std::sync::Mutex<target_solo::Cache>>,
    target_solo_write: tokio::sync::Mutex<()>,
    midi_setup: std::sync::Arc<std::sync::Mutex<midi_setup::Cache>>,
    midi_setup_write: tokio::sync::Mutex<()>,
    mute_behavior: std::sync::Arc<std::sync::Mutex<mute_behavior::Cache>>,
    mute_behavior_write: tokio::sync::Mutex<()>,
    link_policy_cache: link_policy::Cache,
    link_policy_pending: std::sync::Arc<std::sync::atomic::AtomicU8>,
    link_policy_write: tokio::sync::Mutex<()>,
    bus_link_cache: BusLinkCache,
    bus_link_observations: BusLinkCache,
    bus_link_pending: std::sync::Arc<std::sync::atomic::AtomicU8>,
    bus_link_write: tokio::sync::Mutex<()>,
    bus_dispatch: tokio::sync::RwLock<()>,
    link_writes: std::sync::Mutex<[Option<std::time::Instant>; 8]>,
    preamp_write: tokio::sync::Mutex<()>,
    output_routes: std::sync::Arc<std::sync::Mutex<output_routing::Cache>>,
    output_route_write: tokio::sync::Mutex<()>,
    output_geq: std::sync::Arc<std::sync::Mutex<output_geq::Cache>>,
    output_geq_write: tokio::sync::Mutex<()>,
    automix_settings: std::sync::Arc<std::sync::Mutex<automix_settings::Cache>>,
    automix_settings_write: tokio::sync::Mutex<()>,
    output_misc: std::sync::Arc<std::sync::Mutex<output_misc::Cache>>,
    output_misc_write: tokio::sync::Mutex<()>,
    monitor_settings: std::sync::Arc<std::sync::Mutex<monitor_settings::Cache>>,
    monitor_settings_write: tokio::sync::Mutex<()>,
    rta_settings: std::sync::Arc<std::sync::Mutex<rta_settings::Cache>>,
    rta_settings_write: tokio::sync::Mutex<()>,
    rta: std::sync::Mutex<Option<std::sync::Arc<rta::RtaStream>>>,
    native: std::sync::Mutex<native_library::Inventory>,
    session: Mutex<Option<std::sync::Arc<OscSession>>>,
    peer: Mutex<String>,
    model: addrs::XAirModel,
    /// Last decoded `/meters/6` ALL DYN frame (`None` until the first blob
    /// is ingested). Serves the `gate_gr` / `comp_gr` param reads — the
    /// desk has no GR query address, so there is nothing else to read.
    /// Shared with the per-connection ingest task (see `connect`), which
    /// routes every streamed DYN blob here; the `/meters/6` subscribe in
    /// `subscribe_meters_once` starts that stream.
    dyn_cache: std::sync::Arc<tokio::sync::Mutex<Option<(std::time::Instant, dyn_gr::DynMeters)>>>,
    automix_gain_cache: AutomixGainCache,
    /// Desk-originated mix-surface updates (see [`parse_inbound`]): stable
    /// across reconnects, fed by one ingest task per connection. The engine
    /// subscribes once and applies these to its param cache live.
    update_tx: broadcast::Sender<openfader_core::ParamUpdate>,
    /// Latest `/meters/1` levels in decoded blob order (40 linear levels:
    /// channels, aux, FX returns, buses, FX sends, main and monitor —
    /// see [`XAirDriver::poll_meters`]). `None` until the first blob
    /// arrives. Filled by the cache-drain task that
    /// [`MixerDriver::subscribe_meters`] installs; read by `poll_meters`.
    meter_latest: std::sync::Arc<std::sync::Mutex<LevelSample>>,
    /// Timestamped `/meters/5` physical-output bank, separate from strip meters.
    output_latest: std::sync::Arc<std::sync::Mutex<LevelSample>>,
    input_latest: std::sync::Arc<std::sync::Mutex<LevelSample>>,
}

impl Default for XAirDriver {
    fn default() -> Self {
        Self::with_model(addrs::XAirModel::Xr18)
    }
}

impl XAirDriver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a driver for an explicit geometry. Default ([`XAirDriver::new`])
    /// is XR18 (16 channels, hardware-verified); pass
    /// [`addrs::XAirModel::X32Kind`] only for 32-strip maps (not
    /// hardware-verified on X-Air).
    pub fn with_model(model: addrs::XAirModel) -> Self {
        let (update_tx, _) = broadcast::channel(256);
        Self {
            receive_since: std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now())),
            tasks: std::sync::Mutex::new(Vec::new()),
            connection_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            connected: std::sync::atomic::AtomicBool::new(false),
            lan_operations: Default::default(),
            link_cache: std::sync::Arc::new(std::sync::Mutex::new([None; 8])),
            link_writes: std::sync::Mutex::new([None; 8]),
            safe_levels: std::sync::Arc::new(std::sync::Mutex::new(safe_levels::Cache::default())),
            safe_levels_write: tokio::sync::Mutex::new(()),
            usb_interface: std::sync::Arc::new(std::sync::Mutex::new(
                usb_interface::Cache::default(),
            )),
            target_solo: Default::default(),
            target_solo_write: Default::default(),
            clock_rate: Default::default(),
            clock_rate_write: Default::default(),
            main_mute_write: Default::default(),
            usb_interface_write: tokio::sync::Mutex::new(()),
            midi_setup: std::sync::Arc::new(std::sync::Mutex::new(midi_setup::Cache::default())),
            midi_setup_write: tokio::sync::Mutex::new(()),
            mute_behavior: std::sync::Arc::new(std::sync::Mutex::new(
                mute_behavior::Cache::default(),
            )),
            mute_behavior_write: tokio::sync::Mutex::new(()),
            link_policy_cache: std::sync::Arc::new(std::sync::Mutex::new([(0, None); 4])),
            link_policy_pending: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            link_policy_write: tokio::sync::Mutex::new(()),
            bus_link_cache: std::sync::Arc::new(std::sync::Mutex::new([(0, None); 3])),
            bus_link_observations: std::sync::Arc::new(std::sync::Mutex::new([(0, None); 3])),
            bus_link_pending: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
            bus_link_write: tokio::sync::Mutex::new(()),
            bus_dispatch: tokio::sync::RwLock::new(()),
            preamp_write: tokio::sync::Mutex::new(()),
            output_routes: std::sync::Arc::new(std::sync::Mutex::new(
                output_routing::Cache::default(),
            )),
            output_route_write: tokio::sync::Mutex::new(()),
            output_geq: std::sync::Arc::new(std::sync::Mutex::new(output_geq::Cache::default())),
            monitor_settings: std::sync::Arc::new(std::sync::Mutex::new(
                monitor_settings::Cache::default(),
            )),
            monitor_settings_write: tokio::sync::Mutex::new(()),
            rta_settings: std::sync::Arc::new(
                std::sync::Mutex::new(rta_settings::Cache::default()),
            ),
            rta_settings_write: tokio::sync::Mutex::new(()),
            output_geq_write: tokio::sync::Mutex::new(()),
            automix_settings: std::sync::Arc::new(std::sync::Mutex::new(
                automix_settings::Cache::default(),
            )),
            automix_settings_write: tokio::sync::Mutex::new(()),
            output_misc: std::sync::Arc::new(std::sync::Mutex::new(output_misc::Cache::default())),
            output_misc_write: tokio::sync::Mutex::new(()),
            rta: std::sync::Mutex::new(None),
            native: std::sync::Mutex::new(native_library::Inventory::default()),
            session: Mutex::new(None),
            peer: Mutex::new(String::new()),
            model,
            dyn_cache: std::sync::Arc::new(Mutex::new(None)),
            automix_gain_cache: std::sync::Arc::new(Mutex::new(None)),
            update_tx,
            meter_latest: std::sync::Arc::new(std::sync::Mutex::new(None)),
            output_latest: std::sync::Arc::new(std::sync::Mutex::new(None)),
            input_latest: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// 0-based strip index to 1-based wire channel, validated against this
    /// driver's model bound (XR18: strips 0..=15; `/ch/17+` is silent on
    /// hardware, T3).
    fn check_channel(&self, strip: u16) -> anyhow::Result<u16> {
        addrs::channel_in(strip, self.model.channels())
    }

    /// Strip root for group-assignment leaves (`grp/dca`, `grp/mute`):
    /// channels, FX returns, buses and FX-send masters answer (rig-verified
    /// XR18 FW 1.20); main LR and DCA strips are live-silent there and bail
    /// locally so callers never burn a 2 s query timeout on hardware.
    fn assign_root(
        &self,
        strip: u16,
        what: &str,
    ) -> anyhow::Result<(openfader_core::StripKind, String)> {
        let (kind, root) = strip_root(strip)?;
        match kind {
            openfader_core::StripKind::In
            | openfader_core::StripKind::Ret
            | openfader_core::StripKind::Bus
            | openfader_core::StripKind::FxSnd => Ok((kind, root)),
            _ => anyhow::bail!("xair: {what} not supported on {kind:?} strips (no grp leaf)"),
        }
    }

    async fn session(
        &self,
    ) -> anyhow::Result<tokio::sync::MutexGuard<'_, Option<std::sync::Arc<OscSession>>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        Ok(guard)
    }
}

impl MixerDriver for XAirDriver {
    fn param_text_rules(&self, param: &str) -> Option<openfader_core::params::ParamTextRules> {
        (self.model == addrs::XAirModel::Xr18 && param == "name").then_some(
            openfader_core::params::ParamTextRules {
                max_utf8_bytes: None,
                allow_empty: true,
                allow_edge_whitespace: true,
            },
        )
    }

    fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> {
        let mut controls = self.routing_controls();
        controls.extend(self.geq_controls());
        controls.extend(self.automix_setting_controls());
        controls.extend(self.rta_setting_controls());
        controls.extend(self.monitor_setting_controls());
        controls.extend(self.misc_controls());
        controls.extend(self.bus_link_controls());
        controls.extend(self.link_policy_controls());
        controls.extend(self.usb_interface_controls());
        controls.extend(self.safe_levels_controls());
        controls.extend(self.clock_rate_controls());
        controls.extend(self.midi_setup_controls());
        controls.extend(self.mute_behavior_controls());
        controls.extend(self.target_solo_controls());
        controls
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
        self.set_target_solo_checked(target, control, value, expected, snapshot, context)
            .await
    }

    async fn read_link_settings(
        &self,
    ) -> anyhow::Result<Option<openfader_core::link_settings::StereoLinkSettings>> {
        if self.model != addrs::XAirModel::Xr18 {
            return Ok(None);
        }
        self.read_link_settings_snapshot().await.map(Some)
    }

    async fn read_automix_settings(&self) -> anyhow::Result<Option<[bool; 4]>> {
        self.read_automix_snapshot().await.map(Some)
    }

    async fn read_output_geq(
        &self,
        target: &str,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        if self.model != addrs::XAirModel::Xr18 {
            return Ok(None);
        }
        self.read_output_geq_snapshot(target).await.map(Some)
    }

    async fn read_output_taps(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::routing::OutputTapState>>> {
        if self.model != addrs::XAirModel::Xr18 {
            return Ok(None);
        }
        self.read_output_tap_snapshot().await.map(Some)
    }

    async fn read_monitor_settings(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        if self.model != addrs::XAirModel::Xr18 {
            return Ok(None);
        }
        self.read_monitor_snapshot().await.map(Some)
    }

    async fn read_rta_settings(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::destinations::DestinationControl>>> {
        if self.model != addrs::XAirModel::Xr18 {
            return Ok(None);
        }
        self.read_rta_snapshot().await.map(Some)
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
        if target == "mute_behavior" {
            self.set_mute_behavior_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "midi_setup" {
            self.set_midi_setup_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "device_setup" && control == "ponmute" {
            self.set_safe_levels_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "device_setup" && control == "clockrate" {
            self.set_clock_rate_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "device_setup" {
            self.set_usb_interface_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "link_policies" {
            self.set_link_policy_checked(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "bus_links" {
            self.set_bus_link_control(control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "automix" {
            self.set_automix_setting(target, control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "monitor" {
            self.set_monitor_setting(target, control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if target == "rta" {
            self.set_rta_setting(target, control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else if snapshot
            .first()
            .is_some_and(|row| matches!(row.surface.as_str(), "output_controls" | "input_insert"))
        {
            self.set_misc_control(target, control, value, expected, snapshot, context)
                .await
                .map(Some)
        } else {
            self.set_geq_control(target, control, value, expected, snapshot, context)
                .await
                .map(Some)
        }
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
        anyhow::ensure!(
            target != "mute_behavior"
                && target != "midi_setup"
                && target != "device_setup"
                && target != "bus_links"
                && target != "link_policies"
                && !snapshot.iter().any(|row| matches!(
                    row.surface.as_str(),
                    "bus_link_state" | "link_policy_state"
                )),
            "Bus link state is read-only"
        );
        anyhow::ensure!(control == "tap", "Unknown output control");
        self.set_routing_tap(target, value, expected, snapshot, context, None)
            .await
    }
    async fn set_output_tap_guarded(
        &self,
        desired: &openfader_core::routing::OutputTapState,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        anyhow::ensure!(
            desired.target != "mute_behavior" && desired.target != "midi_setup",
            "MIDI setup requires guarded processor writes"
        );
        self.set_routing_tap(
            &desired.target,
            f32::from(desired.tap),
            expected,
            snapshot,
            context,
            Some(&desired.source),
        )
        .await
    }
    fn stereo_pairs(&self) -> Vec<[u16; 2]> {
        (0..16).step_by(2).map(|left| [left, left + 1]).collect()
    }

    async fn control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
        if let Some(members) = self.bus_control_members(strip).await? {
            return Ok(members);
        }
        if matches!(param, "gain" | "phantom") {
            return self.preamp_control_members(strip).await;
        }
        self.stereo_control_members(strip, param)
    }

    async fn set_stereo_link(
        &self,
        strip: StripId,
        linked: bool,
    ) -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        self.write_stereo_link(strip, linked).await
    }
    fn native_library(&self) -> Option<serde_json::Value> {
        Some(self.native_inventory())
    }

    async fn read_lan(&self) -> anyhow::Result<openfader_core::lan::LanSnapshot> {
        self.read_lan_snapshot().await
    }
    async fn write_lan(
        &self,
        expected: openfader_core::lan::LanSnapshot,
        desired: openfader_core::lan::LanConfiguration,
        authorize: &(dyn Fn() -> bool + Send + Sync),
        apply_requested: bool,
    ) -> Result<openfader_core::lan::LanWriteResult, openfader_core::lan::LanWriteError> {
        self.write_lan_checked(expected, desired, authorize, apply_requested)
            .await
    }
    async fn native_library_action(
        &self,
        action: &serde_json::Value,
        expected: &serde_json::Value,
        check: openfader_core::native_library::Check<'_>,
    ) -> anyhow::Result<serde_json::Value> {
        self.native_action(action, expected, check).await
    }

    fn meter_taps(&self) -> &'static [&'static str] {
        &["post", "gr_gate", "gr_comp"]
    }

    fn name(&self) -> &'static str {
        "xair"
    }

    /// Loopback-tested param-plane set (see [`XAIR_CAPABILITIES`]).
    fn capabilities(&self) -> &'static [&'static str] {
        XAIR_CAPABILITIES
    }

    fn param_defs(&self) -> &'static [openfader_core::params::ParamDef] {
        use openfader_core::params::{ParamDef, ParamDefault, ParamKind, ParamScope, SCOPES_INPUT};
        static DEFS: &[ParamDef] = &[
            ParamDef {
                id: "xair:automix_gain",
                kind: ParamKind::Float,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: -128.0,
                max: 0.0,
                unit: "dB",
                default: ParamDefault::Float(0.0),
                label: "Automix gain",
            },
            ParamDef {
                id: "automix_group",
                kind: ParamKind::Enum,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: 0.0,
                max: 2.0,
                unit: "",
                default: ParamDefault::Float(0.0),
                label: "automix group",
            },
            ParamDef {
                id: "xair:preamp_state",
                kind: ParamKind::Text,
                scope: ParamScope::Channel,
                scopes: SCOPES_INPUT,
                min: 0.0,
                max: 0.0,
                unit: "",
                default: ParamDefault::Text(""),
                label: "Active preamp availability",
            },
        ];
        DEFS
    }
    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        presentations::PRESENTATIONS
    }

    fn param_numeric_display(&self, param: &str) -> Option<(f32, f32, &'static str)> {
        // XR18 FW 1.20 / mixer control software: -6 dB -> .25, 0 dB -> .5,
        // +3 dB -> .625 (2026-10-28; original weight restored).
        (param == "automix_weight").then_some((-12.0, 12.0, "dB"))
    }

    fn param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        if param == "color" {
            metadata::XAIR_COLOR_SWATCHES
        } else {
            &[]
        }
    }

    /// XR18 strip map (16 in + 4 FX ret + 6 bus + 4 FX send + LR + 4 DCA).
    fn topology(&self) -> Vec<openfader_core::StripFamily> {
        XAIR_TOPOLOGY.to_vec()
    }

    /// Total addressable strips (36 on the XR18 map).
    fn strip_count(&self) -> usize {
        XAIR_STRIPS
    }

    /// Addressable channel strips for this model (XR18: 16; `/rtn/N`,
    /// `/rtn/aux`, buses/main are separate strips, not channels). Keeps
    /// `list`/sweeps off the phantom `/ch/17+` range the desk never
    /// answers (T3).
    fn channel_count(&self) -> usize {
        self.model.channels() as usize
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.driver_connect(addr).await
    }
    fn telemetry_timestamps(
        &self,
    ) -> Option<(Option<std::time::Instant>, Option<std::time::Instant>)> {
        self.driver_telemetry_timestamps()
    }
    async fn refresh_streams(&self) -> anyhow::Result<()> {
        self.driver_refresh_streams().await
    }
    async fn disconnect(&self) -> anyhow::Result<()> {
        self.driver_disconnect().await
    }
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.driver_set_fader(strip, value).await
    }
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        self.driver_get_fader(strip).await
    }
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        self.driver_set_mute(strip, muted).await
    }
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        self.driver_get_mute(strip).await
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        self.driver_set_name(strip, name).await
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        self.driver_get_name(strip).await
    }
    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        self.driver_set_bus_fader(bus, value).await
    }
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        self.driver_get_bus_fader(bus).await
    }
    /// XR18 bus masters are strips 20..25 (`/bus/N`): mute rides the same
    /// `/bus/N/mix/on` leaf as any strip (verified present on FW 1.20).
    fn bus_count(&self) -> u8 {
        addrs::MAX_BUS
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        self.driver_set_bus_mute(bus, muted).await
    }
    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        self.driver_get_bus_mute(bus).await
    }
    /// Mute-group master count, rig-verified (XR18 FW 1.20, 2026-10-18):
    /// `/config/mute/1..4` answer, `/config/mute/5..8` are silent.
    fn mute_group_count(&self) -> u8 {
        addrs::MUTE_GROUPS
    }

    /// Mute-group master mute (`/config/mute/N`, int). Polarity is
    /// INFERRED from the leaf name (`1` = group muted): only `0` has ever
    /// been observed live and the master has never been toggled on hardware
    /// (toggling could mute member channels audibly), so the `1` direction
    /// is UNVERIFIED-ON-RIG. Same-value writes round-trip bit-exact.
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        self.driver_set_mute_group(group, muted).await
    }
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        self.driver_get_mute_group(group).await
    }
    /// Channel -> mute-group membership, bit `group` of the strip's
    /// `{root}/grp/mute` bitmask (rig-verified XR18 FW 1.20, 2026-10-20:
    /// write `1` → `1`, `2` → `2`, restore `0`; out-of-range writes
    /// saturate on the desk, so the group is validated locally first).
    /// Read-modify-write: the desk has no per-group leaf. Main-LR and DCA
    /// strips carry no `grp` leaf (live-silent) and bail locally instead
    /// of timing out on hardware — this also keeps the engine attach
    /// snapshot (which scans every strip) fast.
    async fn set_mute_group_assign(
        &self,
        ch: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        self.driver_set_mute_group_assign(ch, group, member).await
    }
    /// Channel -> mute-group membership read (same leaf; groups with no
    /// desk leaf bail locally — see [`Self::set_mute_group_assign`]).
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        self.driver_get_mute_group_assign(ch, group).await
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.driver_set_main_fader(value).await
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        self.driver_get_main_fader().await
    }
    /// Main LR master mute (`/lr/mix/on`, int `1` = unmuted / `0` = muted,
    /// same polarity as channel `/mix/on`). The leaf answers queries
    /// (rig-verified XR18 FW 1.20, 2026-10-18: reads `1`).
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        self.driver_set_main_mute(muted).await
    }
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        self.driver_get_main_mute().await
    }
    /// Per-channel solo via the `/-stat/solosw` switch bank: live-verified
    /// on the owner rig (XR18 FW 1.20, 2026-10-18) — writing `1` to the
    /// 1-based `/-stat/solosw/NN` sets bank bit `NN - 1` (soloing ch16 set
    /// bits 14+15: ch15/16 are stereo-linked on that desk), and
    /// `/-action/clearsolo` clears the bank. Inputs map to bank indices 0..15; XR18 Aux and FX returns map to
    /// indices 16..20. Other strip types are unsupported.
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        self.driver_set_solo(strip, soloed).await
    }
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        self.driver_get_solo(strip).await
    }
    /// Strip-ordered meter levels (one entry per strip, [`XAIR_STRIPS`]).
    /// The `/meters/1` blob has 40 levels: channels 0–15, aux 16–17,
    /// FX returns 18–25, buses 26–31, FX sends 32–35 and main LR 36–37.
    /// Stereo strips use the louder side; DCA levels remain unmapped.
    /// Missing or stale data returns an empty frame.
    fn poll_meters(&self) -> openfader_core::MeterFrame {
        self.driver_poll_meters()
    }
    fn output_meter_targets(&self) -> Vec<String> {
        let mut targets = Self::physical_output_targets();
        targets.extend(Self::physical_input_targets());
        targets
    }
    fn poll_output_meters(&self) -> Option<openfader_core::OutputMeterSnapshot> {
        Some(self.driver_poll_output_meters())
    }
    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        self.driver_query_info().await
    }
    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        self.driver_set_fx_param(slot, param, value).await
    }
    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        self.driver_get_fx_param(slot, param).await
    }
    fn poll_spectrum(&self) -> Vec<openfader_core::AudioSpectrum> {
        self.driver_poll_spectrum()
    }
    async fn set_spectrum_target(&self, owner: &str, strip: Option<StripId>) -> anyhow::Result<()> {
        self.driver_set_spectrum_target(owner, strip).await
    }
    async fn subscribe_meters(
        &self,
    ) -> anyhow::Result<broadcast::Receiver<openfader_core::MeterFrame>> {
        self.driver_subscribe_meters().await
    }
    /// Desk-originated mix-surface updates (see [`parse_inbound`]): the
    /// stable channel fed by the per-connection ingest task. The engine
    /// subscribes once and applies these live; reconnects never disturb
    /// the receiver.
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<openfader_core::ParamUpdate>> {
        Some(self.update_tx.subscribe())
    }

    /// Full param plane (see [`XAIR_CAPABILITIES`]). Strip roots come from
    /// `strip_root`: channels (`/ch/NN`) carry the whole set; FX returns
    /// (`/rtn/N`), buses and LR carry fader/mute/name/color plus the
    /// kind-scoped gate/comp/EQ sets; FX-send masters and DCAs carry
    /// fader/mute/name/color only (catalogue scopes). Every address/codec is
    /// confirmed against the owner XR18 (FW 1.20) — see
    /// XR18 baseline notes for the readback table.
    ///
    /// GR reads ride the param path (`gate_gr`/`comp_gr` serve dB reduction
    /// `>= 0` from the last `/meters/6` blob; no GR query address exists).
    /// `delay_on`/`delay_ms` (X32-only), `hpf_slope` (X32-only), `source`
    /// (input source selection not yet integrated), `icon` (no firmware leaf),
    /// per-channel `eq5_*`
    /// (channels/returns carry 4 bands; `eq5/6_*` ride buses + main),
    /// `aux7/8_*`, `send11..16_*` and all legacy `auxN_mute`/`fxN_mute` plus
    /// the v2 `send1..10_on` (the send block has no on/off leaf and
    /// `/ch/NN/mix/MM/on` is live-silent — baseline T7) stay unsupported.
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.driver_set_param(strip, param, value).await
    }
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        self.driver_get_param(strip, param).await
    }
}

#[cfg(test)]
mod tests;
