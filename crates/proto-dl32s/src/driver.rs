//! MixerDriver adapter; protocol responsibilities live in the sibling modules.
use crate::{addrs, Dl32sDriver, DL32S_CAPABILITIES};
use crate::assignments::INPUT_SOLO_FIRST_ADDRESS;
use openfader_core::{validate_fader, AudioSpectrum, ChannelStrip, MeterFrame, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue, StripId};
use openfader_transport::mackie::{VALUE_TYPE_PARAM, VALUE_TYPE_MUTE_GROUP, MUTE_GROUP_COUNT};
use tokio::sync::broadcast;

impl MixerDriver for Dl32sDriver {
    fn name(&self) -> &'static str {
        "dl32s"
    }

    fn topology(&self) -> Vec<openfader_core::StripFamily> { crate::strips::topology() }
    fn strip_count(&self) -> usize { crate::strips::STRIP_COUNT }

    /// Wave-1+2 verified surface: every id names a confirmed DL32S leaf
    /// (see [`DL32S_CAPABILITIES`]). The poller reads exactly this set.
    fn capabilities(&self) -> &'static [&'static str] {
        DL32S_CAPABILITIES
    }

    fn param_label_overrides(&self) -> Vec<(&'static str, String)> {
        self.native_param_label_overrides()
    }

    fn input_processor_guard(&self) -> bool { true }
    async fn input_processor(&self, strip: StripId, processor: &str, write: Option<(&str, ParamValue)>, expected: Option<&serde_json::Value>, check: openfader_core::input_processors::RecallCheck<'_>) -> anyhow::Result<serde_json::Value> {
        self.guarded_input_processor(strip, processor, write, expected, check).await
    }
    fn destination_controls(&self) -> Vec<openfader_core::destinations::DestinationControl> {
        let mut controls = self.collect_destination_controls();
        let generation = self.generation.load(std::sync::atomic::Ordering::Acquire);
        if let Ok(native) = self.native.lock() {
            if native.generation == generation && native.view.connected {
                controls.extend(native.output_processing.controls(generation));
                controls.extend(native.globals.controls(generation));
                controls.extend(native.solos.controls(generation));
            }
        }
        controls
    }

    async fn set_fx_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        self.write_fx_guarded(target, control, value, expected, snapshot, context).await
    }

    async fn set_target_solo(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl], context: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<openfader_core::destinations::DestinationControl> {
        self.write_target_solo(target, control, value, expected, snapshot, context).await
    }

    async fn set_destination_control_guarded(&self, target: &str, control: &str, value: f32, expected: f32, snapshot: &[openfader_core::destinations::DestinationControl]) -> anyhow::Result<Option<openfader_core::destinations::DestinationControl>> {
        if let Some(receipt) = self.write_global_control(target, control, value, expected, Some(snapshot)).await? { return Ok(Some(receipt)); }
        anyhow::ensure!(self.write_output_processing_guarded(target, control, value, expected, Some(snapshot)).await?, "Output processor unavailable");
        Ok(None)
    }

    async fn set_destination_control(&self, target: &str, control: &str, value: f32, expected: f32) -> anyhow::Result<()> {
        if self.write_global_control(target, control, value, expected, None).await?.is_some() { return Ok(()); }
        if self.write_output_processing(target, control, value, expected).await? {
            return Ok(());
        }
        anyhow::ensure!(!target.starts_with("fx/"), "FX requires a complete current configuration guard");
        self.write_aux_control(target, control, value, expected).await
    }

    /// Provider rows extend the canonical catalogue with source IDs,
    /// model selection and additional assignment groups. Canonical IDs
    /// retain precedence when a future catalogue adds an equivalent row.
    fn param_defs(&self) -> &'static [openfader_core::params::ParamDef] {
        self.metadata_param_defs()
    }

    /// Documented modern-DL32S engineering-unit ranges (Master Fader 5.1
    /// manual; the desk runs native Master Fader 5.2 / mixer firmware 1.6).
    /// Applied over the canonical (X-Air-shaped) catalogue in this
    /// provider's registry only — sim and xair keep canonical ranges.
    /// Every entry below has known units and a direct wire encoding (dB
    /// leaves ride `Encoding::Db`, Hz/unitless leaves ride
    /// `Encoding::Float`); kinds, scopes, labels and defaults are
    /// untouched. Except for the live-verified HPF endpoints and Gate controls
    /// domain noted in `DL32S_RANGE_OVERRIDES`, bounds are documentation-
    /// based (hardware endpoint writes pending); the desk is believed to
    /// clamp outside its maxima.
    /// Deliberately absent (canonical kept, with reason):
    /// - Line-input `-20..40` effective attenuation (p90): no
    ///   input-socket detection exists, so the single mic/balanced `0..60`
    ///   slider is served (CH7 MicPre native 0 observed, consistent); the
    ///   existing wire bound is retained without assuming how the physical
    ///   offset is represented.
    ///
    /// Compressor endpoints, model/knee enums and Vintage choices are
    /// independently hardware-verified on CH7, 2026-10-25. Native inf:1 is
    /// the finite wire value 101, labelled through presentation metadata.
    /// EQ mode labels and curve semantics remain unverified.
    fn param_range_overrides(&self) -> &'static [(&'static str, f32, f32)] {
        self.metadata_param_range_overrides()
    }

    fn param_symbols(&self, param: &str) -> &'static [openfader_core::params::ParamSymbol] {
        self.metadata_param_symbols(param)
    }

    fn param_text_rules(&self, param: &str) -> Option<openfader_core::params::ParamTextRules> {
        self.metadata_param_text_rules(param)
    }

    fn param_swatches(&self, param: &str) -> &'static [openfader_core::params::ParamSwatch] {
        self.metadata_param_swatches(param)
    }

    fn param_presentations(&self) -> &'static [openfader_core::params::ParamPresentation] {
        self.metadata_param_presentations()
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        self.lifecycle_connect(addr).await
    }

    /// Live desk-originated strip updates (another client's moves arrive
    /// without a poll sweep). Values are engineering units, same as
    /// `get_param`; the push plane equals the caps plane.
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<ParamUpdate>> {
        Some(self.update_tx.subscribe())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let level = validate_fader(value)?;
        let (table, ch) = crate::strips::resolve(strip)?;
        let addr = table.address(ch, "fader")?;
        let db = addrs::DL32S_FADER_SCALE.forward(level);
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    /// DISPLAY continuity read (attach/snapshot/health/state): inclusive —
    /// own writes included, no proof claimed. Verification uses `get_param`
    /// (desk-proven) instead.
    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let (table, ch) = crate::strips::resolve(strip)?;
        let addr = table.address(ch, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_solo(&self, strip: StripId, soloed: bool) -> anyhow::Result<()> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .write_int(address, u32::from(soloed), VALUE_TYPE_MUTE_GROUP).await
    }

    /// Display continuity only; polling uses `get_solo_verified`.
    async fn get_solo(&self, strip: StripId) -> anyhow::Result<bool> {
        let address = INPUT_SOLO_FIRST_ADDRESS + Self::channel(strip)? - 1;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above")
            .cached_type2(address)
            .filter(|raw| *raw<=1)
            .map(|raw| raw != 0)
            .ok_or_else(|| anyhow::anyhow!("solo: no value pushed yet"))
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let (table, ch) = crate::strips::resolve(strip)?;
        let addr = table.address(ch, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let ch = crate::strips::name_slot(strip)?;
        self.set_channel_name(ch, name).await
    }
    /// DISPLAY continuity read (inclusive).
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = crate::strips::name_slot(strip)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name_slot(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no value pushed yet"))
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.set_main_fader_db(addrs::DL32S_FADER_SCALE.forward(value)).await
    }
    /// DISPLAY continuity read (inclusive).
    async fn get_main_fader(&self) -> anyhow::Result<f32> {
        let addr = addrs::dl32s_main().address(1, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("main fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    /// Main LR master mute: the inherent `set_main_mute` leaf exists; the
    /// read rides the pushed-value cache like every other DL32S mute.
    async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        self.set_main_mute(muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_main_mute(&self) -> anyhow::Result<bool> {
        let addr = addrs::dl32s_main().address(1, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("main mute: no value pushed yet"))
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let (table, ch) = crate::strips::resolve(strip)?;
        let addr = table.address(ch, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("mute: no value pushed yet"))
    }

    // ---- R2 mix-bus masters (aux masters 1..8, 0-based bus) ----
    fn bus_count(&self) -> u8 {
        addrs::dl32s_auxmasters().count as u8
    }

    async fn set_bus_fader(&self, bus: u8, value: f32) -> anyhow::Result<()> {
        let aux = Self::dl_check_bus(bus)?;
        self.set_auxmaster_fader_db(aux, addrs::DL32S_FADER_SCALE.forward(validate_fader(value)?)).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_bus_fader(&self, bus: u8) -> anyhow::Result<f32> {
        let aux = Self::dl_check_bus(bus)?;
        let addr = addrs::dl32s_auxmasters().address(aux, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("bus fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_bus_mute(&self, bus: u8, muted: bool) -> anyhow::Result<()> {
        let aux = Self::dl_check_bus(bus)?;
        self.set_auxmaster_mute(aux, muted).await
    }

    async fn get_bus_mute(&self, bus: u8) -> anyhow::Result<bool> {
        let aux = Self::dl_check_bus(bus)?;
        let addr = addrs::dl32s_auxmasters().address(aux, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("bus mute: no value pushed yet"))
    }

    // ---- R2 subgroup masters (1..6, 0-based group) + membership ----
    fn subgroup_count(&self) -> u8 {
        addrs::dl32s_subgroups().count as u8
    }

    async fn set_subgroup_fader(&self, group: u8, value: f32) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        self.set_subgroup_fader_db(g, addrs::DL32S_FADER_SCALE.forward(validate_fader(value)?)).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_fader(&self, group: u8) -> anyhow::Result<f32> {
        let g = Self::dl_check_subgroup(group)?;
        let addr = addrs::dl32s_subgroups().address(g, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("subgroup fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_subgroup_mute(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        self.set_subgroup_mute(g, muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_mute(&self, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_subgroup(group)?;
        let addr = addrs::dl32s_subgroups().address(g, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("subgroup mute: no value pushed yet"))
    }

    async fn set_subgroup_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_subgroup(group)?;
        let (table, ch1) = crate::strips::resolve(ch)?;
        let addr = table.address(ch1, &format!("sub{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_subgroup(group)?;
        let (table, ch1) = crate::strips::resolve(ch)?;
        let addr = table.address(ch1, &format!("sub{g}"))?;
        let v = self.request_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("subgroup assign: no value pushed yet"))?;
        Ok(v != 0)
    }

    // ---- R2 mute-group masters + membership ----
    fn mute_group_count(&self) -> u8 {
        MUTE_GROUP_COUNT as u8
    }

    async fn set_mute_group(&self, group: u8, muted: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_mute_group(group)?;
        self.set_mute_group(g, muted).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        self.get_mute_group(g).await
    }

    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_mute_group(group)?;
        let (table, ch1) = crate::strips::resolve(ch)?;
        let addr = table.address(ch1, &format!("mutegroup{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    /// DISPLAY continuity read (inclusive).
    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        let (table, ch1) = crate::strips::resolve(ch)?;
        let addr = table.address(ch1, &format!("mutegroup{g}"))?;
        let v = self.request_value_u32(addr).await?.ok_or_else(|| anyhow::anyhow!("mute-group assign: no value pushed yet"))?;
        Ok(v != 0)
    }

    // ---- R2 file transfer (Show.zip up/download) ----
    fn file_transfer(&self) -> bool {
        true
    }

    async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        Self::check_filename(filename)?;
        self.download_file(filename).await
    }

    async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        Self::check_filename(filename)?;
        self.upload_file(filename, data, file_type).await
    }

    fn stereo_pairs(&self) -> Vec<[u16; 2]> {
        (0..34_u16).step_by(2).map(|left| [left, left + 1]).collect()
    }

    async fn control_members(&self, strip: StripId, param: &str) -> anyhow::Result<Vec<StripId>> {
        self.assignments_control_members(strip, param).await
    }

    async fn set_stereo_link(&self, source: StripId, linked: bool)
        -> anyhow::Result<Vec<(StripId, String, ParamValue)>> {
        self.assignments_set_stereo_link(source, linked).await
    }

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        self.input_set_param(strip, param, value).await
    }

    /// Authoritative param read (verification/poll/refresh path): desk-proven
    /// only — own-write-pending words withheld. Deliberately NOT routed via
    /// the inclusive display getters above (continuity without proof); each
    /// arm reads the verified cache directly, including unit conversion.
    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        self.input_get_param(strip, param).await
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let mut strips = Vec::with_capacity(crate::strips::STRIP_COUNT);
        for i in 0..crate::strips::STRIP_COUNT {
            let (table, ch) = crate::strips::resolve(StripId(i as u16))?;
            let fader_addr = table.address(ch, "fader").expect("table static");
            let mute_addr = table.address(ch, "mute").expect("table static");
            let pan_addr = table.address(ch, "pan").ok();
            // No fabricated defaults: a missing word means the bulk push
            // hasn't covered this strip yet, and reporting 0/unmuted as
            // real would paint the wrong desk state. Surface the read
            // error so callers hold their loading state instead.
            // Continuity, not proof: this reads the optimistic-inclusive
            // `cached` (own-write-pending words included) so a snapshot
            // right after local moves still reflects them. Provenance for
            // per-param confirmation lives one layer up (engine `authority`
            // on `param`/`params`); the typed `get_*` display getters above
            // are likewise inclusive, while `get_param` stays verified-only.
            let fader = session
                .cached(fader_addr)
                .map(|raw| addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
                .ok_or_else(|| anyhow::anyhow!("snapshot: ch{ch} fader: no value pushed yet"))?;
            let mute = session
                .cached(mute_addr)
                .map(|v| v != 0)
                .ok_or_else(|| anyhow::anyhow!("snapshot: ch{ch} mute: no value pushed yet"))?;
            let pan = if let Some(address) = pan_addr {
                session.cached(address).map(|raw| f32::from_bits(raw) * 100.0)
                    .ok_or_else(|| anyhow::anyhow!("snapshot: {} {ch} balance unavailable", table.name))?
            } else { 0.0 };
            let name = session
                .name_slot(crate::strips::name_slot(StripId(i as u16))?)
                .unwrap_or_else(|| format!("{} {ch}", table.name));
            strips.push(ChannelStrip {
                id: StripId(i as u16),
                name,
                fader,
                mute,
                pan,
            });
        }
        Ok(MixerSnapshot { strips })
    }

    async fn recall(&self, snapshot: &MixerSnapshot) -> anyhow::Result<()> {
        for strip in &snapshot.strips {
            // Names are deliberately not written back: the rename encoding is
            // unconfirmed and a wrong count word blanks neighbour slots.
            self.set_fader(strip.id, strip.fader).await?;
            self.set_mute(strip.id, strip.mute).await?;
        }
        Ok(())
    }

    async fn query_info(&self) -> anyhow::Result<MixerInfo> {
        let addr = self.addr.lock().await.clone().unwrap_or_default();
        Ok(MixerInfo {
            name: String::new(),
            model: "DL32S".into(),
            // GENERAL_INFO model/firmware strings were never decoded from the
            // wire; report the driver identity rather than guessing.
            firmware: "unparsed".into(),
            addr,
        })
    }

    /// Only the verified pre-fader input tap: the first 32 meter words are
    /// hardware-pinned input dB (see [`crate::telemetry::decode_input_meters`]). No `post`
    /// (never `in` x fader — unconfirmed), no GR, no RTA.
    fn meter_taps(&self) -> &'static [&'static str] {
        &["in"]
    }

    /// Read-only native snapshot view: the push-observed model above
    /// (`Some` whenever this driver is selected — supported but possibly
    /// unconnected/loading — so the engine can render honest status
    /// instead of guessing). A mismatched private generation withholds
    /// the view (unavailable default) so a newer session never reads the
    /// older session's slots. No write path exists behind this hook.
    fn native_snapshots(&self) -> Option<openfader_core::NativeSnapshots> {
        self.read_native_snapshots()
    }

    /// Non-blocking input-meter sample: the cached linear levels (one per
    /// input strip), or empty when no fresh push backs this generation —
    /// empty means unavailable, never a fabricated silence row. The engine
    /// skips both the `meters` and the `in` tap frames on empty, so a dead
    /// stream goes quiet instead of frozen. No polling here: pushes arrive
    /// on the existing broadcast feed, and the shell's 80 ms ticker stays
    /// the only meter clock.
    fn output_meter_targets(&self) -> Vec<String> { crate::output_meters::targets() }
    fn poll_output_meters(&self) -> Option<openfader_core::OutputMeterSnapshot> { Some(self.telemetry_poll_outputs()) }
    fn poll_meters(&self) -> MeterFrame {
        self.telemetry_poll_meters()
    }

    fn poll_dynamics_meters(&self) -> Option<Vec<openfader_core::DynamicsMeterFrame>> {
        self.telemetry_poll_dynamics_meters()
    }

    /// Passive shared RTA: the desk's MF-selected source,
    /// followed read-only — no acquisition, no restore writes. Returns the
    /// global (`strip: None`) frame for the confirmed selector 5000
    /// (`DL32S Return 1–2 (shared)` / `shared`); input selectors 4000–4031
    /// attribute to the corresponding zero-based input [`StripId`].
    /// Unknown selectors (and the unsettled/stale windows) report empty —
    /// unavailable, never fabricated. Source changes clear the cache, bump
    /// the epoch (monotonic across source/generation changes) and withhold
    /// frames for [`crate::telemetry::RTA_SETTLE`] so in-flight old-source words are never
    /// misattributed. Stale (>1 s, same bound as input meters) reads
    /// empty. Band edges are nominal (see [`crate::telemetry::rta_band_edges_hz`]). No
    /// Return-strip id or param is invented here — return processing stays
    /// a later slice.
    fn poll_spectrum(&self) -> Vec<AudioSpectrum> {
        self.telemetry_poll_spectrum()
    }

    /// Passive shared RTA source: release (`None`) is a no-op Ok — there is
    /// nothing owned to restore. A named acquire errors explicitly (no
    /// default success pretence): the source is MF-selected and this stage
    /// performs no selector writes. The frontend disables automatic DL32S
    /// target requests.
    async fn set_spectrum_target(
        &self,
        _owner: &str,
        strip: Option<StripId>,
    ) -> anyhow::Result<()> {
        if strip.is_none() {
            return Ok(());
        }
        anyhow::bail!(
            "dl32s: RTA source is MF-selected (shared Return 1–2); automatic acquisition is disabled"
        )
    }

    async fn disconnect(&self) -> anyhow::Result<()> {
        self.lifecycle_disconnect().await
    }
    // Buses / main / meter subscribe keep the core defaults (unsupported):
    // output-strip fields are structurally mapped but deliberately unlabelled
    // per SPEC.
}
