//! `MixerDriver` adapter; domain operations live in sibling modules.
use crate::{
    addrs::{self, X32_STRIPS},
    mapping::{fam_of, mapping, Mapping, WireKind},
    presentation, X32Driver,
};
use anyhow::{ensure, Context};
use openfader_core::{
    params::{ParamDef, ParamTextRules},
    ChannelStrip, MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, OutputMeterSnapshot,
    ParamUpdate, ParamValue, StripFamily, StripId,
};
use std::time::Instant;
use tokio::sync::broadcast;

fn number(value: ParamValue) -> anyhow::Result<f32> {
    if let ParamValue::Float(v) = value {
        Ok(v)
    } else {
        anyhow::bail!("x32: expected number")
    }
}

fn boolean(value: ParamValue) -> anyhow::Result<bool> {
    if let ParamValue::Bool(v) = value {
        Ok(v)
    } else {
        anyhow::bail!("x32: expected bool")
    }
}

impl X32Driver {
    /// Native strip applicability, before considering live source availability.
    pub fn param_applicable(&self, strip: StripId, param: &str) -> anyhow::Result<bool> {
        let (_, family) = fam_of(strip)?;
        Ok(match param {
            "gain" | "phantom" | "source" | "x32:preamp_state" => family.is_ch || family.is_aux,
            "x32:eq_bands" | "x32:eq_type_count" => !family.is_dca,
            "x32:automix_gain" => family.is_ch && strip.0 < 8,
            "gate_gr" => family.is_ch,
            "comp_gr" => family.is_ch || family.is_bus || family.is_mtx || family.is_main,
            _ => mapping(strip, param).is_ok(),
        })
    }

    /// protocol reference v4.09-02 p.55: /-prefs/clockrate 0=48K, 1=44K1.
    /// A read failure is unknown, never a guessed 48 kHz clock.
    pub async fn observed_sample_rate(&self) -> anyhow::Result<u32> {
        match self.read_int("/-prefs/clockrate").await? {
            0 => Ok(48_000),
            1 => Ok(44_100),
            other => anyhow::bail!("x32: unrecognized sample-rate selector {other}"),
        }
    }

    /// Write one mapped param with a fresh independent readback of the
    /// same leaf. No successful send is treated as confirmation.
    async fn write_mapped(&self, m: &Mapping, value: ParamValue) -> anyhow::Result<()> {
        let arg = m.encode(value.clone())?;
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        self.ensure_current(&session)?;
        session.send(&m.path, vec![arg]).await?;
        let reply = session.query_timeout(&m.path, crate::QUERY_TIMEOUT).await?;
        self.ensure_current(&session)?;
        let read = m.decode(&reply)?;
        ensure!(
            readback_agrees(&value, &read, &m.kind),
            "x32: readback did not match the requested write at {}",
            m.path
        );
        Ok(())
    }
}

/// Engineering-unit agreement against the nearest documented desk
/// quantum. Exact for bool/text/int/enum leaves (enum indices compare
/// exactly; the encode side already rounds FX enums, mapping enums
/// require exact indices). Quantized float leaves compare in the wire
/// domain: an exact echo (emulator or already-on-quantum write) within
/// float roundoff, or a readback on the documented grid within half a
/// wire step of the request (desk rounded to the nearest quantum).
/// Anything else — including a refused write still sitting at its old
/// value — is a mismatch, never a confirm. Unknown `linf`/`logf`
/// quanta never guess: only an exact echo passes.
fn readback_agrees(written: &ParamValue, read: &ParamValue, kind: &WireKind) -> bool {
    use crate::codec;
    match (written, read) {
        (ParamValue::Bool(a), ParamValue::Bool(b)) => a == b,
        (ParamValue::Text(a), ParamValue::Text(b)) => a == b,
        (ParamValue::Float(a), ParamValue::Float(b)) => {
            if !a.is_finite() || !b.is_finite() {
                return false;
            }
            match kind {
                WireKind::Raw01 => codec::wire_agrees_stepped(*a, *b, codec::FADER_WIRE_STEPS),
                WireKind::LevelDb => {
                    let w_written = if *a <= -90.0 {
                        Some(0.0)
                    } else if *a <= 10.0 {
                        codec::db_to_fader(*a).ok()
                    } else {
                        None
                    };
                    let w_read = if *b <= -90.0 {
                        Some(0.0)
                    } else if *b <= 10.0 {
                        codec::db_to_fader(*b).ok()
                    } else {
                        None
                    };
                    match (w_written, w_read) {
                        (Some(w), Some(r)) => {
                            codec::wire_agrees_stepped(w, r, codec::LEVEL_WIRE_STEPS)
                        }
                        _ => false,
                    }
                }
                WireKind::Linf(min, max) => {
                    if !(a >= min && a <= max && b >= min && b <= max) {
                        return false;
                    }
                    let range = max - min;
                    if !range.is_finite() || range <= 0.0 {
                        return false;
                    }
                    let w_written = (*a - *min) / range;
                    let w_read = (*b - *min) / range;
                    match codec::linf_step(*min, *max) {
                        Some(step) => match codec::linf_wire_steps(*min, *max, step) {
                            Some(steps) => codec::wire_agrees_stepped(w_written, w_read, steps),
                            None => false,
                        },
                        // Undocumented quantum: exact echo only, never a guess.
                        None => (w_read - w_written).abs() <= codec::WIRE_EPS,
                    }
                }
                WireKind::Logf(min, max) => match codec::logf_steps(*min, *max) {
                    Some(steps) => {
                        let w_written = codec::logf_to_wire(*a, *min, *max).ok();
                        let w_read = codec::logf_to_wire(*b, *min, *max).ok();
                        match (w_written, w_read) {
                            (Some(w), Some(r)) => codec::wire_agrees_stepped(w, r, steps),
                            _ => false,
                        }
                    }
                    None => false,
                },
                WireKind::Q => {
                    let w_written = codec::q_to_wire(*a).ok();
                    let w_read = codec::q_to_wire(*b).ok();
                    match (w_written, w_read) {
                        (Some(w), Some(r)) => codec::wire_agrees_stepped(w, r, codec::Q_WIRE_STEPS),
                        _ => false,
                    }
                }
                WireKind::Gain => {
                    if !((-12.0..=60.0).contains(a) && (-12.0..=60.0).contains(b)) {
                        return false;
                    }
                    // 0.5 dB steps over 72 dB: 145 wire values.
                    let w_written = (*a + 12.0) / 72.0;
                    let w_read = (*b + 12.0) / 72.0;
                    codec::wire_agrees_stepped(w_written, w_read, 145)
                }
                WireKind::Ratio | WireKind::Int(..) | WireKind::Enum(_) => a == b,
                WireKind::Bool | WireKind::Text(_) => false,
            }
        }
        _ => false,
    }
}

impl MixerDriver for X32Driver {
    fn name(&self) -> &'static str {
        "x32"
    }
    fn channel_count(&self) -> usize {
        addrs::CHANNELS
    }
    fn strip_count(&self) -> usize {
        X32_STRIPS
    }
    fn topology(&self) -> Vec<StripFamily> {
        addrs::X32_TOPOLOGY.to_vec()
    }
    fn stereo_pairs(&self) -> Vec<[u16; 2]> {
        crate::link::STEREO_PAIRS.to_vec()
    }
    async fn control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
        if param == "link" {
            self.stereo_link_members(strip)
        } else if matches!(param, "gain" | "phantom") {
            self.preamp_control_members(strip).await
        } else {
            self.linked_members(strip).await
        }
    }
    async fn set_stereo_link(
        &self,
        strip: StripId,
        linked: bool,
    ) -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        self.write_stereo_link(strip, linked).await
    }
    fn bus_count(&self) -> u8 {
        16
    }
    fn mute_group_count(&self) -> u8 {
        addrs::MUTE_GROUPS
    }
    fn capabilities(&self) -> &'static [&'static str] {
        presentation::capabilities()
    }
    fn param_defs(&self) -> &'static [ParamDef] {
        presentation::defs()
    }
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        presentation::RANGES
    }
    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        presentation::presentations()
    }
    fn param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        if param == "color" {
            presentation::swatches()
        } else {
            &[]
        }
    }
    fn param_text_rules(&self, param: &str) -> Option<ParamTextRules> {
        (param == "name").then_some(ParamTextRules {
            max_utf8_bytes: Some(12),
            allow_empty: true,
            allow_edge_whitespace: true,
        })
    }
    fn param_numeric_display(&self, param: &str) -> Option<(f32, f32, &'static str)> {
        (param == "automix_weight").then_some((-12.0, 12.0, "dB"))
    }
    fn meter_taps(&self) -> &'static [&'static str] {
        // Only GR taps: the subscribed level blobs (`/meters/0/1/2/5`)
        // document no pre/post tap position, so `poll_meters` serves no
        // level tap (never an invented `in`/`post`); the selected-channel
        // `/meters/6` taps have no documented strip map. GR rides the
        // DYN cache behind `dyn_gr` (`poll_taps` stays default).
        &["gr_gate", "gr_comp"]
    }
    fn poll_meters(&self) -> MeterFrame {
        self.telemetry_poll()
    }
    fn output_meter_targets(&self) -> Vec<String> {
        crate::telemetry::output_targets()
    }
    fn poll_output_meters(&self) -> Option<OutputMeterSnapshot> {
        // Supported; empty frames mean supported-but-unavailable, never
        // fabricated levels.
        Some(self.telemetry_output_snapshot())
    }
    async fn subscribe_meters(&self) -> anyhow::Result<broadcast::Receiver<MeterFrame>> {
        self.telemetry_subscribe().await
    }
    async fn refresh_streams(&self) -> anyhow::Result<()> {
        self.telemetry_refresh().await
    }
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<ParamUpdate>> {
        Some(self.updates.subscribe())
    }
    fn telemetry_timestamps(&self) -> Option<(Option<Instant>, Option<Instant>)> {
        // Control-path stamp plus the freshest meter-stream stamp.
        // Replaying a cache never advances either clock.
        Some((*self.last_control.lock().unwrap(), self.last_meter_at()))
    }
    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.connect_session(addr).await
    }
    async fn disconnect(&self) -> anyhow::Result<()> {
        self.disconnect_session().await
    }
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        if param == "mute" {
            // The generic poller must use the same inverted mix/on
            // convention as snapshots, get_mute and desk-originated pushes.
            return self.get_mute(strip).await.map(ParamValue::Bool);
        }
        if matches!(param, "gain" | "phantom") {
            return self.preamp_display(strip, param).await;
        }
        if param == "source" {
            return self.source_label(strip).await;
        }
        if param == "x32:preamp_state" {
            return self.preamp_state_text(strip).await.map(ParamValue::Text);
        }
        if param == "x32:automix_gain" {
            // Live gain rides /meters/16; fresh cache only, never
            // synthesized. Stale reads stay honestly unavailable.
            return self.cached_automix_db(strip).map(ParamValue::Float);
        }
        if param == "gate_gr" {
            // Gate GR rides /meters/1 (dB reduction >= 0); fresh cache
            // only, like X-Air's GR param path. Needs a capabilities
            // entry in presentation.rs before the engine polls it.
            return self.cached_gate_gr(strip).map(ParamValue::Float);
        }
        if param == "comp_gr" {
            // Comp GR rides /meters/1 (ch) and /meters/2 (bus/mtx/main);
            // fresh cache only. Same capabilities note as gate_gr.
            return self.cached_comp_gr(strip).map(ParamValue::Float);
        }
        if param == "x32:eq_type_count" {
            self.session()?;
            let (_, fam) = fam_of(strip)?;
            ensure!(!fam.is_dca, "x32: DCA has no EQ");
            return Ok(ParamValue::Float(if fam.is_mtx || fam.is_main {
                14.
            } else {
                6.
            }));
        }
        if param == "x32:eq_bands" {
            self.session()?;
            let (_, fam) = fam_of(strip)?;
            ensure!(!fam.is_dca, "x32: DCA has no EQ");
            return Ok(ParamValue::Float(
                if fam.is_bus || fam.is_mtx || fam.is_main {
                    6.
                } else {
                    4.
                },
            ));
        }
        if param.starts_with("dca") && param.ends_with("_assign") {
            return self.group_member(strip, param).await.map(ParamValue::Bool);
        }
        if param.starts_with("_mg") {
            return self.group_member(strip, param).await.map(ParamValue::Bool);
        }
        let m = mapping(strip, param)?;
        self.read(&m).await
    }
    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.set_guarded(strip, param, value, &Default::default())
            .await
    }
    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        self.set_param(strip, "fader", ParamValue::Float(value))
            .await
    }
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        number(self.get_param(strip, "fader").await?)
    }
    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        // Desk `mix/on` ON = audible; canonical `mute` is inverted.
        let m = mapping(strip, "mute")?;
        self.write_mapped(&m, ParamValue::Bool(!muted)).await
    }
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        Ok(!boolean(self.read(&mapping(strip, "mute")?).await?)?)
    }
    async fn set_name(&self, strip: StripId, value: &str) -> anyhow::Result<()> {
        self.set_param(strip, "name", ParamValue::Text(value.into()))
            .await
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        if let ParamValue::Text(v) = self.get_param(strip, "name").await? {
            Ok(v)
        } else {
            anyhow::bail!("x32: invalid name")
        }
    }
    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        self.set_solo_state(strip, soloed).await
    }
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        self.get_solo_state(strip).await
    }
    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        ensure!(bus < 16, "x32: bus out of range");
        self.set_fader(StripId(48 + bus as u16), value).await
    }
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        ensure!(bus < 16, "x32: bus out of range");
        self.get_fader(StripId(48 + bus as u16)).await
    }
    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        ensure!(bus < 16, "x32: bus out of range");
        self.set_mute(StripId(48 + bus as u16), muted).await
    }
    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        ensure!(bus < 16, "x32: bus out of range");
        self.get_mute(StripId(48 + bus as u16)).await
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.set_fader(StripId(70), value).await
    }
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        self.get_fader(StripId(70)).await
    }
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        self.set_mute(StripId(70), muted).await
    }
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        self.get_mute(StripId(70)).await
    }
    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        ensure!(group < addrs::MUTE_GROUPS, "x32: mute group out of range");
        self.set_mute_master(group + 1, muted).await
    }
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        ensure!(group < addrs::MUTE_GROUPS, "x32: mute group out of range");
        self.get_mute_master(group + 1).await
    }
    async fn set_mute_group_assign(
        &self,
        strip: StripId,
        group: u8,
        member: bool,
    ) -> anyhow::Result<()> {
        ensure!(group < addrs::MUTE_GROUPS, "x32: mute group out of range");
        self.set_group_member(strip, &format!("_mg{}", group + 1), member)
            .await
    }
    async fn get_mute_group_assign(&self, strip: StripId, group: u8) -> anyhow::Result<bool> {
        ensure!(group < addrs::MUTE_GROUPS, "x32: mute group out of range");
        self.group_member(strip, &format!("_mg{}", group + 1)).await
    }
    async fn set_fx_param(&self, slot: u8, param: u8, value: f32) -> anyhow::Result<()> {
        self.write_fx_par(slot, param, value).await
    }
    async fn get_fx_param(&self, slot: u8, param: u8) -> anyhow::Result<f32> {
        self.read_fx_par(slot, param).await
    }
    /// Tap + invert + delay control rows behind every tap-carrying
    /// output: only fresh independently-verified rows on the live
    /// session, never guesses. Tap rows (42, source-bound) stay the
    /// snapshot contract; invert/delay rows are extra
    /// `surface: "routing"` descriptors with their own per-control
    /// revisions, never local-processor preset members.
    fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> {
        let mut rows = self.tap_control_rows();
        rows.extend(self.extra_control_rows());
        rows.extend(self.solo_control_rows());
        rows
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
        // Control writes bind to the just-read live source (single
        // revision check under the lane); recall binds to the captured
        // source instead (see `set_output_tap_guarded`). Tap rides the
        // source/tap triple guard; invert/delay ride their own
        // per-control guard — delay values never validate against the
        // tap guard.
        match control {
            "tap" => {
                self.write_output_tap_control(
                    target, control, value, expected, snapshot, None, context,
                )
                .await
            }
            "invert" | "delay_on" | "delay_time" => {
                self.write_output_extra_control(target, control, value, expected, snapshot, context)
                    .await
            }
            _ => anyhow::bail!("x32: unknown output control {control:?}"),
        }
    }
    /// Complete independently-read tap inventory; any unreadable output
    /// errors the whole capture rather than serving a partial roster.
    async fn read_output_taps(
        &self,
    ) -> anyhow::Result<Option<Vec<openfader_core::routing::OutputTapState>>> {
        self.read_output_taps_snapshot().await.map(Some)
    }
    /// Source-bound tap recall: the captured socket must still be live.
    async fn set_output_tap_guarded(
        &self,
        desired: &openfader_core::routing::OutputTapState,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        self.write_output_tap_control(
            &desired.target,
            "tap",
            f32::from(desired.tap),
            expected,
            snapshot,
            Some(&desired.source),
            context,
        )
        .await
    }
    /// Honest solo target write: guarded `solo` rows only (see `solo.rs`).
    /// The parent combines `solo_control_rows()` into
    /// `destination_controls`; this hook serves the engine advertised-clear
    /// path and direct target writes.
    async fn set_target_solo(
        &self,
        target: &str,
        control: &str,
        value: f32,
        expected: f32,
        snapshot: &[openfader_core::destinations::DestinationControl],
        context: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        self.write_target_solo(target, control, value, expected, snapshot, context)
            .await
    }
    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        self.link
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.info.clone())
            .context("x32: not connected")
    }
    fn native_library(&self) -> Option<serde_json::Value> {
        // Poisoned state reads as unavailable (never panics, never stale).
        self.native
            .lock()
            .ok()
            .map(|state| self.native_inventory_with(&state))
    }
    async fn native_library_action(
        &self,
        action: &serde_json::Value,
        expected: &serde_json::Value,
        check: openfader_core::native_library::Check<'_>,
    ) -> anyhow::Result<serde_json::Value> {
        self.native_action_with(&self.native, action, expected, check)
            .await
    }
    fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        // Poisoned state reads as unavailable (never panics, never stale).
        self.native
            .lock()
            .ok()
            .map(|state| self.native_snapshots_with(&state))
    }
    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        let mut strips = Vec::with_capacity(X32_STRIPS);
        for i in 0..X32_STRIPS as u16 {
            let id = StripId(i);
            strips.push(ChannelStrip {
                id,
                name: self.get_name(id).await?,
                fader: self.get_fader(id).await?,
                mute: self.get_mute(id).await?,
                pan: match fam_of(id)?.1.key {
                    "ch" | "auxin" | "fxrtn" | "bus" | "main" => {
                        number(self.get_param(id, "pan").await?)?
                    }
                    _ => 0.,
                },
            });
        }
        Ok(MixerSnapshot { strips })
    }
    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        // Validate all rows before emitting the first mutation.
        for row in &snapshot.strips {
            Mapping {
                path: String::new(),
                kind: WireKind::Raw01,
            }
            .encode(ParamValue::Float(row.fader))?;
            ensure!(
                row.name.len() <= 12 && !row.name.contains('\0'),
                "x32: invalid name"
            );
            if row.pan.is_finite() {
                anyhow::ensure!((-100.0..=100.0).contains(&row.pan), "x32: invalid pan");
            }
        }
        for row in &snapshot.strips {
            self.set_name(row.id, &row.name).await?;
            self.set_fader(row.id, row.fader).await?;
            self.set_mute(row.id, row.mute).await?;
            if matches!(
                fam_of(row.id)?.1.key,
                "ch" | "auxin" | "fxrtn" | "bus" | "main"
            ) {
                self.set_param(row.id, "pan", ParamValue::Float(row.pan))
                    .await?;
            }
        }
        Ok(())
    }
}

impl X32Driver {
    /// Guarded write used by both `set_param` and the engine's
    /// `set_param_guarded` override: group bitmasks go through RMW,
    /// preamp leaves through the shared-headamp path, everything else
    /// through a mapped write with fresh readback.
    pub async fn set_guarded(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
        expected: &std::collections::BTreeMap<String, ParamValue>,
    ) -> anyhow::Result<()> {
        if param == "link" {
            let ParamValue::Bool(linked) = value else {
                anyhow::bail!("x32: stereo link requires boolean")
            };
            ensure!(
                expected.is_empty(),
                "x32: link writes carry no source context"
            );
            return self.write_stereo_link(strip, linked).await.map(|_| ());
        }
        if matches!(param, "gain" | "phantom") {
            return self
                .write_active_preamp_guarded(strip, param, value, expected)
                .await;
        }
        if (param.starts_with("dca") && param.ends_with("_assign")) || param.starts_with("_mg") {
            let ParamValue::Bool(member) = value else {
                anyhow::bail!("x32: assignment requires boolean")
            };
            ensure!(
                expected.is_empty(),
                "x32: group writes carry no source context"
            );
            return self.set_group_member(strip, param, member).await;
        }
        ensure!(
            !matches!(
                param,
                "source"
                    | "x32:eq_bands"
                    | "x32:eq_type_count"
                    | "x32:preamp_state"
                    | "x32:automix_gain"
                    | "gate_gr"
                    | "comp_gr"
            ),
            "x32: {param} is read-only"
        );
        if param == "mute" {
            let ParamValue::Bool(muted) = value else {
                anyhow::bail!("x32: mute requires boolean")
            };
            for (id, wanted) in expected {
                ensure!(
                    self.get_param(strip, id).await? == *wanted,
                    "Input source changed; reopen the control."
                );
            }
            return self.set_mute(strip, muted).await;
        }
        let m = mapping(strip, param)?;
        for (id, wanted) in expected {
            ensure!(
                self.get_param(strip, id).await? == *wanted,
                "Input source changed; reopen the control."
            );
        }
        self.write_mapped(&m, value).await
    }
}
