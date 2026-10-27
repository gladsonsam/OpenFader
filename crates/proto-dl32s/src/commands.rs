//! Native device, show, file and direct output commands.
use crate::addrs::dl32s_inputs;
use crate::{addrs, Dl32sDriver};
use openfader_transport::mackie::{REQUEST_TIMEOUT, VALUE_TYPE_PARAM};

impl Dl32sDriver {
    pub(super) fn check_filename(filename: &str) -> anyhow::Result<()> {
        if filename.is_empty() || filename.len() > 128 {
            anyhow::bail!("dl32s: filename must be 1..=128 chars");
        }
        if filename.contains(['/', '\\', '\0']) {
            anyhow::bail!("dl32s: filename {filename:?} must not contain path separators");
        }
        Ok(())
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

    /// Historical misnomer: op 5 names an existing snapshot, not a state capture.
    pub async fn save_show(&self, snapshot_1_based: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .save_show_snapshot(snapshot_1_based, name)
            .await
    }

    pub async fn set_aux_mute(&self, channel: u32, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_mute"))?;
        let _ = aux; // validation via address table
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }

    pub async fn set_aux_fader_db(&self, channel: u32, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_level"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(addr, db)
            .await
    }

    pub async fn set_aux_pan(&self, channel: u32, aux: u32, pan: f32) -> anyhow::Result<()> {
        let pan = pan.clamp(-1.0, 1.0);
        let addr = dl32s_inputs().address(channel, &format!("aux{aux}_pan"))?;
        let _ = aux;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(addr, pan)
            .await
    }

    pub async fn set_subgroup_mute(&self, group: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_subgroups().address(group, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }

    pub async fn set_subgroup_fader_db(&self, group: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_subgroups().address(group, "fader")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(addr, db)
            .await
    }

    pub async fn set_main_mute(&self, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_main().address(1, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }

    pub async fn set_main_fader_db(&self, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_main().address(1, "fader")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(addr, db)
            .await
    }

    pub async fn set_auxmaster_mute(&self, aux: u32, muted: bool) -> anyhow::Result<()> {
        let addr = addrs::dl32s_auxmasters().address(aux, "mute")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(addr, u32::from(muted), VALUE_TYPE_PARAM)
            .await
    }

    pub async fn set_auxmaster_fader_db(&self, aux: u32, db: f32) -> anyhow::Result<()> {
        let db = db.clamp(-120.0, 10.0);
        let addr = addrs::dl32s_auxmasters().address(aux, "fader")?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(addr, db)
            .await
    }

    pub async fn set_channel_name(&self, slot: u32, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            name.len() <= 63,
            "channel name must not exceed 63 UTF-8 bytes"
        );
        anyhow::ensure!(
            !name.chars().any(char::is_control),
            "channel name must not contain control characters"
        );
        anyhow::ensure!(
            name == name.trim(),
            "channel name must not start or end with whitespace"
        );
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_channel_name(slot, name)
            .await
    }

    pub async fn delete_snapshot(&self, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .delete_snapshot(slot)
            .await
    }

    pub async fn reboot(&self, mode: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").reboot(mode).await
    }

    pub async fn disconnect_notice(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .disconnect_notice()
            .await
    }

    pub async fn download_file(&self, filename: &str) -> anyhow::Result<Vec<u8>> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .download_file(filename, REQUEST_TIMEOUT)
            .await
    }

    pub async fn upload_file(
        &self,
        filename: &str,
        data: &[u8],
        file_type: u32,
    ) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .upload_file(filename, data, file_type, REQUEST_TIMEOUT)
            .await
    }

    pub async fn set_file_crc(&self, address: u32, crc: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_file_crc(address, crc)
            .await
    }

    pub async fn process_files(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").process_files().await
    }

    pub async fn refresh_snapshot_names(
        &self,
    ) -> anyhow::Result<std::collections::HashMap<u32, String>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        session.download_file("Show.zip", REQUEST_TIMEOUT).await?;
        // A successful transfer is not a parsed, complete show inventory.
        Ok(session.snapshot_names_pushed())
    }

    pub async fn get_device_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_device_name(REQUEST_TIMEOUT)
            .await
    }

    pub async fn set_device_name(&self, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_device_name(name, REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_subdevice_name(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_subdevice_name(REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_ip_address(&self) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_ip_address(REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_lock_status(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_lock_status(REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_static_ip_config(&self) -> anyhow::Result<(bool, String, String, String)> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_static_ip_config(REQUEST_TIMEOUT)
            .await
    }

    pub async fn set_static_ip_config(
        &self,
        is_static: bool,
        address: &str,
        mask: &str,
        gateway: &str,
    ) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_static_ip_config(is_static, address, mask, gateway, REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_max_message_words(&self) -> anyhow::Result<u32> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_max_message_words(REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_network_config(&self) -> anyhow::Result<Vec<u32>> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_network_config(REQUEST_TIMEOUT)
            .await
    }

    pub async fn site_survey(&self) -> anyhow::Result<Vec<[u32; 11]>> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .site_survey(REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_transport_status(
        &self,
        kind: &str,
    ) -> anyhow::Result<openfader_transport::mackie::Message> {
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

    pub async fn get_show_info(&self) -> anyhow::Result<openfader_transport::mackie::Message> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_show_info(REQUEST_TIMEOUT)
            .await
    }

    pub async fn show_ok(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard.as_ref().expect("checked above").show_ok().await
    }

    pub async fn get_show_crc(
        &self,
        pid: u32,
    ) -> anyhow::Result<openfader_transport::mackie::Message> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .get_show_crc(pid, REQUEST_TIMEOUT)
            .await
    }

    pub async fn set_show_parameter(&self, pid: u32, value: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_show_parameter(pid, value)
            .await
    }

    pub async fn set_show_name(&self, pid: u32, name: &str) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_show_name(pid, name)
            .await
    }

    pub async fn enable_meter_reports(&self) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .enable_meter_reports()
            .await
    }

    pub async fn recall_snapshot_addr(&self, address: u32, slot: u32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .recall_snapshot(address, slot)
            .await
    }

    pub async fn set_values(
        &self,
        values: &std::collections::HashMap<u32, u32>,
        value_type: u8,
    ) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .set_values(values, value_type)
            .await
    }

    pub async fn raw_set_value_int(&self, address: u32, value: i64) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_int(address, value as u32, VALUE_TYPE_PARAM)
            .await
    }

    pub async fn raw_set_value_float(&self, address: u32, value: f32) -> anyhow::Result<()> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .write_float(address, value)
            .await
    }

    pub(super) fn dl_check_bus(bus: u8) -> anyhow::Result<u32> {
        let count = addrs::dl32s_auxmasters().count;
        if (bus as u32) >= count {
            anyhow::bail!("dl32s: bus {bus} out of range 0..{count} (0-based)");
        }
        Ok(bus as u32 + 1)
    }
}
