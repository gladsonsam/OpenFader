//! Mackie DL32S driver: thin mapping over [`MackieSession`].
//!
//! This crate owns ONLY the DL32S address table ([`addrs`]) plus argument
//! mapping. All sockets, framing, keepalive, caching and the init handshake
//! live in `openmix-transport::mackie`. New providers add a crate shaped
//! exactly like this one: table + mapping, zero core changes.

pub mod addrs;

use addrs::{dl32s_inputs, INPUT_COUNT};
use openmix_core::{
    validate_fader, ChannelStrip, MixerDriver, MixerInfo, MixerSnapshot, ParamValue, StripId,
};
use openmix_transport::mackie::{MackieSession, MUTE_GROUP_COUNT, REQUEST_TIMEOUT, VALUE_TYPE_PARAM};
use tokio::sync::{Mutex, MutexGuard};

/// Live DL32S-class mixer on TCP 50001.
pub struct Dl32sDriver {
    session: Mutex<Option<MackieSession>>,
    addr: Mutex<Option<String>>,
}

impl Default for Dl32sDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Dl32sDriver {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
            addr: Mutex::new(None),
        }
    }

    /// Core [`StripId`] is 0-based; the mixer table is 1-based channels.
    fn channel(strip: StripId) -> anyhow::Result<u32> {
        let ch = strip.0 as u32 + 1;
        if !(1..=INPUT_COUNT).contains(&ch) {
            anyhow::bail!("channel {} out of range 1..={INPUT_COUNT}", strip.0);
        }
        Ok(ch)
    }

    fn check_filename(filename: &str) -> anyhow::Result<()> {
        if filename.is_empty() || filename.len() > 128 {
            anyhow::bail!("dl32s: filename must be 1..=128 chars");
        }
        if filename.contains(['/', '\\', '\0']) {
            anyhow::bail!("dl32s: filename {filename:?} must not contain path separators");
        }
        Ok(())
    }

    async fn session(&self) -> anyhow::Result<MutexGuard<'_, Option<MackieSession>>> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            anyhow::bail!("not connected: call connect() first");
        }
        Ok(guard)
    }

    /// Write one table field, encoding the value per its wire type.
    /// Mirrors the proven `set_parameter` rules: bool/1-0 ints as u32,
    /// enums/ints as u32, dB/floats as BE floats with limit checks.
    /// Value-shape mistakes are reported before the connection is touched,
    /// so mapping bugs surface even offline.
    async fn set_field(&self, strip: StripId, key: &str, value: ParamValue) -> anyhow::Result<()> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let addr = table.address(Self::channel(strip)?, key)?;
        enum Out {
            Int(u32),
            Float(f32),
        }
        let out = match (field.encoding, value) {
            (addrs::Encoding::Bool, ParamValue::Bool(b)) => Out::Int(u32::from(b)),
            (addrs::Encoding::Enum | addrs::Encoding::Int, ParamValue::Float(v)) => {
                if !v.is_finite() {
                    anyhow::bail!("{key} must be finite, got {v}");
                }
                Out::Int(v as u32)
            }
            (enc, ParamValue::Float(v))
                if enc == addrs::Encoding::Db || enc == addrs::Encoding::Float =>
            {
                if !v.is_finite() {
                    anyhow::bail!("{key} must be finite, got {v}");
                }
                if let Some((lo, hi)) = field.limits {
                    if !(lo..=hi).contains(&v) {
                        anyhow::bail!("{key} must be within {lo}..{hi}, got {v}");
                    }
                }
                Out::Float(v)
            }
            (enc, got) => anyhow::bail!("{key} has {enc:?} encoding, got {got:?}"),
        };
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        match out {
            Out::Int(v) => session.write_int(addr, v, VALUE_TYPE_PARAM).await,
            Out::Float(v) => session.write_float(addr, v).await,
        }
    }

    /// Read one table field back from the push cache.
    async fn get_field(&self, strip: StripId, key: &str) -> anyhow::Result<ParamValue> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let addr = table.address(Self::channel(strip)?, key)?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("{key}: no value pushed yet"))?;
        if field.encoding.is_float() {
            Ok(ParamValue::Float(f32::from_bits(raw)))
        } else if field.encoding == addrs::Encoding::Bool {
            Ok(ParamValue::Bool(raw != 0))
        } else {
            Ok(ParamValue::Float(raw as f32))
        }
    }

    /// Mute/unmute a mute group master (fixed protocol address, no table needed).
    pub async fn set_mute_group(&self, group_1_based: u32, muted: bool) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_mute_group(group_1_based, muted)
            .await
    }

    pub async fn get_mute_group(&self, group_1_based: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_mute_group(group_1_based)
            .ok_or_else(|| anyhow::anyhow!("mute group {group_1_based}: no value pushed yet"))
    }

    /// Recall a mixer show snapshot the way the mixer app does (op 1 + ack).
    pub async fn recall_show(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .recall_show_snapshot(snapshot_1_based)
            .await
    }

    /// Save current state into a snapshot slot under `name` (op 5).
    pub async fn save_show(&self, snapshot_1_based: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .save_show_snapshot(snapshot_1_based, name)
            .await
    }

    // ---- aux sends (per input strip) ----

    pub async fn set_aux_mute(&self, channel: u32, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_mute"))?;
        let _ = aux; // validation via address table
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_aux_fader_db(&self, channel: u32, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_level"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    pub async fn set_aux_pan(&self, channel: u32, aux: u32, pan: f32) -> anyhow::Result<()> {
        let pan = pan.clamp(-1.0, 1.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_pan"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, pan).await
    }

    // ---- subgroups ----

    pub async fn set_subgroup_mute(&self, group: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_subgroups().address(group, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_subgroup_fader_db(&self, group: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_subgroups().address(group, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- main ----

    pub async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_main().address(1, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_main_fader_db(&self, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_main().address(1, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- aux masters ----

    pub async fn set_auxmaster_mute(&self, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_auxmasters().address(aux, "mute")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(muted), VALUE_TYPE_PARAM).await
    }

    pub async fn set_auxmaster_fader_db(&self, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_auxmasters().address(aux, "fader")?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    // ---- rename (channel names) ----

    pub async fn set_channel_name(&self, slot: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_channel_name(slot, name).await
    }

    // ---- mute group (already) + snapshot delete/reboot/etc via session ----

    pub async fn delete_snapshot(&self, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").delete_snapshot(slot).await
    }

    pub async fn reboot(&self, mode: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").reboot(mode).await
    }

    pub async fn disconnect_notice(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").disconnect_notice().await
    }

    // ---- file transfer ----

    pub async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").download_file(filename, REQUEST_TIMEOUT).await
    }

    pub async fn upload_file(&self, filename: &str, data: &[u8], file_type: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").upload_file(filename, data, file_type, REQUEST_TIMEOUT).await
    }

    pub async fn set_file_crc(&self, address: u32, crc: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_file_crc(address, crc).await
    }

    pub async fn process_files(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").process_files().await
    }

    pub async fn refresh_snapshot_names(&self) -> anyhow::Result<std::collections::HashMap<u32, String>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let _ = session.download_file("Show.zip", REQUEST_TIMEOUT).await;
        Ok(session.snapshot_names())
    }

    // ---- device admin ----

    pub async fn get_device_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_device_name(REQUEST_TIMEOUT).await
    }
    pub async fn set_device_name(&self, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_device_name(name, REQUEST_TIMEOUT).await
    }
    pub async fn get_subdevice_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_subdevice_name(REQUEST_TIMEOUT).await
    }
    pub async fn get_ip_address(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_ip_address(REQUEST_TIMEOUT).await
    }
    pub async fn get_lock_status(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_lock_status(REQUEST_TIMEOUT).await
    }
    pub async fn get_static_ip_config(&self) -> anyhow::Result<(bool, String, String, String)> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_static_ip_config(REQUEST_TIMEOUT).await
    }
    pub async fn set_static_ip_config(&self, is_static: bool, address: &str, mask: &str, gateway: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_static_ip_config(is_static, address, mask, gateway, REQUEST_TIMEOUT).await
    }
    pub async fn get_max_message_words(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_max_message_words(REQUEST_TIMEOUT).await
    }
    pub async fn get_network_config(&self) -> anyhow::Result<Vec<u32>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_network_config(REQUEST_TIMEOUT).await
    }
    pub async fn site_survey(&self) -> anyhow::Result<Vec<[u32; 11]>> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").site_survey(REQUEST_TIMEOUT).await
    }
    pub async fn get_transport_status(&self, kind: &str) -> anyhow::Result<openmix_transport::mackie::Message> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let subtype = match kind {
            "transport" => 8u32,
            "multitrack" => 9u32,
            "music" => 10u32,
            _ => anyhow::bail!("kind must be transport, multitrack or music"),
        };
        session.request_general_info(subtype, REQUEST_TIMEOUT).await
    }
    pub async fn get_show_info(&self) -> anyhow::Result<openmix_transport::mackie::Message> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_show_info(REQUEST_TIMEOUT).await
    }
    pub async fn show_ok(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").show_ok().await
    }
    pub async fn get_show_crc(&self, pid: u32) -> anyhow::Result<openmix_transport::mackie::Message> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").get_show_crc(pid, REQUEST_TIMEOUT).await
    }
    pub async fn set_show_parameter(&self, pid: u32, value: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_show_parameter(pid, value).await
    }
    pub async fn set_show_name(&self, pid: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_show_name(pid, name).await
    }
    pub async fn enable_meter_reports(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").enable_meter_reports().await
    }
    pub async fn recall_snapshot_addr(&self, address: u32, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").recall_snapshot(address, slot).await
    }
    pub async fn set_values(&self, values: &std::collections::HashMap<u32, u32>, value_type: u8) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").set_values(values, value_type).await
    }
    pub async fn raw_set_value_int(&self, address: u32, value: i64) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(address, value as u32, VALUE_TYPE_PARAM).await
    }
    pub async fn raw_set_value_float(&self, address: u32, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(address, value).await
    }
    pub async fn request_value_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        Ok(session.cached(address))
    }
}

impl Dl32sDriver {
    fn dl_check_bus(bus: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_auxmasters().count;
        if (bus as u32) >= count {
            anyhow::bail!("dl32s: bus {bus} out of range 0..{count} (0-based)");
        }
        Ok(bus as u32 + 1)
    }

    fn dl_check_subgroup(group: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_subgroups().count;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: subgroup {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }

    fn dl_check_mute_group(group: u8) -> anyhow::Result<u32> {
        let count = MUTE_GROUP_COUNT;
        if (group as u32) >= count {
            anyhow::bail!("dl32s: mute group {group} out of range 0..{count} (0-based)");
        }
        Ok(group as u32 + 1)
    }
}

impl MixerDriver for Dl32sDriver {
    fn meter_taps(&self) -> &'static [&'static str] {
        &[]
    }

    fn name(&self) -> &'static str {
        "dl32s"
    }

    /// DL32S strip map: 32 input channels only (honest — the address table
    /// covers inputs; mixes/LR are not mapped, so no topology is advertised
    /// for them and `caps` stays fader/mute/name only).
    fn topology(&self) -> Vec<openmix_core::StripFamily> {
        vec![openmix_core::StripFamily {
            key: "ch",
            kind: openmix_core::StripKind::In,
            label: "Ch 1-32",
            first: 0,
            count: INPUT_COUNT as usize,
            name_prefix: "Ch",
            color_default: 2,
        }]
    }

    fn strip_count(&self) -> usize {
        INPUT_COUNT as usize
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        addrs::model_status("dl32s")?;
        let session = MackieSession::connect(addr).await?;
        *self.session.lock().await = Some(session);
        *self.addr.lock().await = Some(addr.to_string());
        Ok(())
    }

    async fn set_fader(&self, strip: StripId, value: f32) -> anyhow::Result<()> {
        let level = validate_fader(value)?;
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "fader")?;
        let db = addrs::DL32S_FADER_SCALE.forward(level);
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_float(addr, db).await
    }

    async fn get_fader(&self, strip: StripId) -> anyhow::Result<f32> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "fader")?;
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached(addr)
            .ok_or_else(|| anyhow::anyhow!("fader: no value pushed yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    async fn set_mute(&self, strip: StripId, muted: bool) -> anyhow::Result<()> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }
    async fn set_name(&self, strip: StripId, name: &str) -> anyhow::Result<()> {
        let ch = Self::channel(strip)?;
        self.set_channel_name(ch, name).await
    }
    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = Self::channel(strip)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no value pushed yet"))
    }
    async fn set_main_fader(&self, value: f32) -> anyhow::Result<()> {
        self.set_main_fader_db(addrs::DL32S_FADER_SCALE.forward(value)).await
    }
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

    async fn get_mute(&self, strip: StripId) -> anyhow::Result<bool> {
        let ch = Self::channel(strip)?;
        let addr = dl32s_inputs().address(ch, "mute")?;
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
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("sub{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    async fn get_subgroup_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_subgroup(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("sub{g}"))?;
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

    async fn get_mute_group(&self, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        self.get_mute_group(g).await
    }

    async fn set_mute_group_assign(&self, ch: StripId, group: u8, member: bool) -> anyhow::Result<()> {
        let g = Self::dl_check_mute_group(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").write_int(addr, u32::from(member), VALUE_TYPE_PARAM).await
    }

    async fn get_mute_group_assign(&self, ch: StripId, group: u8) -> anyhow::Result<bool> {
        let g = Self::dl_check_mute_group(group)?;
        let ch1 = Self::channel(ch)?;
        let addr = dl32s_inputs().address(ch1, &format!("mutegroup{g}"))?;
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

    async fn set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match (param, value) {
            ("fader", ParamValue::Float(v)) => self.set_fader(strip, v).await,
            ("mute", ParamValue::Bool(m)) => self.set_mute(strip, m).await,
            ("name", ParamValue::Text(n)) => self.set_name(strip, &n).await,
            ("name", _) => anyhow::bail!("dl32s: set_name requires text"),
            (key, v) => self.set_field(strip, key, v).await,
        }
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            _ => self.get_field(strip, param).await,
        }
    }

    async fn snapshot(&self) -> anyhow::Result<MixerSnapshot> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let table = dl32s_inputs();
        let mut strips = Vec::with_capacity(INPUT_COUNT as usize);
        for i in 0..INPUT_COUNT {
            let ch = i + 1;
            let fader_addr = table.address(ch, "fader").expect("table static");
            let mute_addr = table.address(ch, "mute").expect("table static");
            let pan_addr = table.address(ch, "pan").expect("table static");
            let fader = session
                .cached(fader_addr)
                .map(|raw| addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
                .unwrap_or(0.0);
            let mute = session.cached(mute_addr).map(|v| v != 0).unwrap_or(false);
            let pan = session.cached(pan_addr).map(f32::from_bits).unwrap_or(0.0);
            let name = session
                .name(ch)
                .unwrap_or_else(|| format!("CH{ch:02}"));
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
    // Buses / main / meter subscribe keep the core defaults (unsupported):
    // output-strip fields are structurally mapped but deliberately unlabelled
    // per SPEC, and meter value encoding is unconfirmed.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn not_connected_errors() {
        let d = Dl32sDriver::new();
        assert_eq!(d.name(), "dl32s");
        assert!(d.get_fader(StripId(0)).await.is_err());
        assert!(d.get_mute(StripId(0)).await.is_err());
        assert!(d.get_name(StripId(0)).await.is_err());
        assert!(d.get_mute_group(1).await.is_err());
        assert!(d.set_fader(StripId(0), 0.5).await.is_err());
    }

    #[test]
    fn strip_index_is_zero_based() {
        assert_eq!(Dl32sDriver::channel(StripId(0)).unwrap(), 1);
        assert_eq!(Dl32sDriver::channel(StripId(31)).unwrap(), 32);
        assert!(Dl32sDriver::channel(StripId(32)).is_err());
    }

    #[tokio::test]
    async fn unknown_field_errors_without_network() {
        let d = Dl32sDriver::new();
        // Field resolution happens before the connection check, so mapping
        // mistakes surface even offline.
        let err = d
            .set_param(StripId(0), "nope", ParamValue::Float(1.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown"), "{err}");
        let err = d.get_param(StripId(0), "nope").await.unwrap_err().to_string();
        assert!(err.contains("unknown"), "{err}");
    }

    #[tokio::test]
    async fn wrong_value_kind_errors() {
        let d = Dl32sDriver::new();
        let err = d
            .set_field(StripId(0), "gain", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Db"), "{err}");
    }

    #[test]
    fn capabilities_are_fader_mute_name_only() {
        // MS param plane: the DL32S relay advertises only the always-
        // available triple (unchanged driver behaviour otherwise — `set_param`
        // still passes other table fields through when connected).
        assert_eq!(Dl32sDriver::new().capabilities(), &["fader", "mute", "name"]);
    }
}
