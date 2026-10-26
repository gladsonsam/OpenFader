//! Behringer X-Air OSC driver (XR12/16/18, MR18).
//!
//! Thin mapping over [`openfader_transport::OscSession`]: this crate owns ONLY the
//! X-Air address table + arg mapping. Sockets, keepalive, meter parsing, and
//! request/response plumbing all live in `openfader-transport`.
use openfader_core::{MixerDriver, MixerInfo, ParamValue, StripId};
use openfader_transport::{KeepaliveConfig, OscSession, osc};
use rosc::{OscMessage, OscType};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, broadcast};

pub mod automix;
pub mod codec;
pub mod comp;
pub mod dyn_gr;
pub mod eq;
pub mod fx;
pub mod gate;
pub mod groups;
pub mod headamp;
pub mod input;
pub mod insert;
pub mod look;
pub mod mix;
pub mod rta;
pub mod solo;
mod sync_decode;

pub mod addrs;
mod assignments;
mod discovery;
mod fx_driver;
mod inbound;
mod lifecycle;
mod mapping;
mod metadata;
mod monitor;
mod param_read;
mod param_write;
mod surface;
mod telemetry;

use discovery::*;
pub use inbound::parse_inbound;
use mapping::*;
pub use metadata::{XAIR_CAPABILITIES, XAIR_STRIPS, XAIR_TOPOLOGY};

type LevelSample = Option<(std::time::Instant, Vec<f32>)>;

/// Live XR18-class mixer on UDP 10024.
pub struct XAirDriver {
    receive_since: std::sync::Arc<std::sync::Mutex<std::time::Instant>>,
    tasks: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
    connection_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    rta: std::sync::Mutex<Option<std::sync::Arc<rta::RtaStream>>>,
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
    /// Desk-originated mix-surface updates (see [`parse_inbound`]): stable
    /// across reconnects, fed by one ingest task per connection. The engine
    /// subscribes once and applies these to its param cache live.
    update_tx: broadcast::Sender<openfader_core::ParamUpdate>,
    /// Latest `/meters/1` levels in decoded blob order (40 linear levels:
    /// input bank `0..16`, aux-return post at idx 26 — rig-verified
    /// 2026-10-20; families past the channels are NOT mapped to strips —
    /// see [`XAirDriver::poll_meters`]). `None` until the first blob
    /// arrives. Filled by the cache-drain task that
    /// [`MixerDriver::subscribe_meters`] installs; read by `poll_meters`.
    meter_latest: std::sync::Arc<std::sync::Mutex<LevelSample>>,
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
            rta: std::sync::Mutex::new(None),
            session: Mutex::new(None),
            peer: Mutex::new(String::new()),
            model,
            dyn_cache: std::sync::Arc::new(Mutex::new(None)),
            update_tx,
            meter_latest: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
    /// `/-action/clearsolo` clears the bank. Only channel strips
    /// (`0..16`) map (1-based index `strip + 1`); buses/mains/DCAs have no
    /// verified solo leaf and bail out (never faked).
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        self.driver_set_solo(strip, soloed).await
    }
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        self.driver_get_solo(strip).await
    }
    /// Strip-ordered meter levels (one entry per strip, [`XAIR_STRIPS`]).
    /// The `/meters/1` blob (the only level stream subscribed) decodes in
    /// transport to 40 linear levels: the input bank `0..16` feeds ch 1..16
    /// (pre-mute taps — live-verified pattern 2026-10-20: hot while the
    /// channels sit muted, ch15-anchored at idx 14, aux pre pair at 16/17),
    /// and idx 26 feeds the aux-return strip 35 (`/rtn/aux` mute-gated,
    /// live-verified 2026-10-20). Strips past the channels (FX/bus/main/
    /// DCA) and the remaining blob slots have no rig-verified strip
    /// mapping, so they read honest silence here — never a neighbor's
    /// level. `None` (no blob yet) reads back full-map silence, like the
    /// core default.
    fn poll_meters(&self) -> openfader_core::MeterFrame {
        self.driver_poll_meters()
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
    /// [`strip_root`]: channels (`/ch/NN`) carry the whole set; FX returns
    /// (`/rtn/N`), buses and LR carry fader/mute/name/color plus the
    /// kind-scoped gate/comp/EQ sets; FX-send masters and DCAs carry
    /// fader/mute/name/color only (catalogue scopes). Every address/codec is
    /// confirmed against the owner XR18 (FW 1.20) — see
    /// XR18 baseline notes for the readback table.
    ///
    /// GR reads ride the param path (`gate_gr`/`comp_gr` serve dB reduction
    /// `>= 0` from the last `/meters/6` blob; no GR query address exists).
    /// `delay_on`/`delay_ms` (X32-only), `hpf_slope` (X32-only), `source`
    /// (`insrc` live-silent, FW 1.20), `icon` (no firmware leaf), `link`
    /// (global `/config/chlink`, no per-channel leaf), per-channel `eq5_*`
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
