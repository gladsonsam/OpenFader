//! General information, device configuration, and show metadata.

use super::*;

impl MackieSession {
    // ---- device admin (0x0E / 0x0F) ----

    pub async fn request_general_info(
        &self,
        info_type: u32,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Message> {
        self.send_request(
            Command::GeneralInfo,
            info_type.to_be_bytes().to_vec(),
            timeout,
        )
        .await
    }

    pub async fn request_firmware_info(
        &self,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Message> {
        self.send_request(Command::FirmwareInfo, vec![], timeout)
            .await
    }

    pub fn info_string(body: &[u8]) -> String {
        if body.len() < 8 {
            return String::new();
        }
        let swapped = word_swap(&body[4..]);
        String::from_utf8_lossy(swapped.split(|b| *b == 0).next().unwrap_or(&[])).to_string()
    }

    pub fn info_ipv4(word: u32) -> String {
        use std::net::Ipv4Addr;
        Ipv4Addr::from(word).to_string()
    }

    pub async fn get_device_name(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(3, timeout).await?.body;
        Ok(Self::info_string(&body))
    }

    pub async fn get_subdevice_name(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(7, timeout).await?.body;
        Ok(Self::info_string(&body))
    }

    pub async fn get_ip_address(&self, timeout: std::time::Duration) -> anyhow::Result<String> {
        let body = self.request_general_info(4, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("IP address reply too short");
        }
        let word = u32::from_be_bytes(body[4..8].try_into().unwrap());
        Ok(Self::info_ipv4(word))
    }

    pub async fn get_lock_status(&self, timeout: std::time::Duration) -> anyhow::Result<u32> {
        let body = self.request_general_info(5, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("Lock status reply too short");
        }
        Ok(u32::from_be_bytes(body[4..8].try_into().unwrap()))
    }

    pub async fn get_static_ip_config(
        &self,
        timeout: std::time::Duration,
    ) -> anyhow::Result<(bool, String, String, String)> {
        let body = self.request_general_info(6, timeout).await?.body;
        if body.len() < 20 {
            anyhow::bail!("Static IP config reply too short");
        }
        let flag = u32::from_be_bytes(body[4..8].try_into().unwrap());
        let ip = u32::from_be_bytes(body[8..12].try_into().unwrap());
        let mask = u32::from_be_bytes(body[12..16].try_into().unwrap());
        let gw = u32::from_be_bytes(body[16..20].try_into().unwrap());
        Ok((
            flag != 0,
            Self::info_ipv4(ip),
            Self::info_ipv4(mask),
            Self::info_ipv4(gw),
        ))
    }

    pub async fn get_max_message_words(&self, timeout: std::time::Duration) -> anyhow::Result<u32> {
        let body = self.request_general_info(2, timeout).await?.body;
        if body.len() < 8 {
            anyhow::bail!("Max message words reply too short");
        }
        Ok(u32::from_be_bytes(body[4..8].try_into().unwrap()))
    }

    pub async fn get_network_config(
        &self,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Vec<u32>> {
        let body = self.request_general_info(0x12, timeout).await?.body;
        if body.len() < 4 + 65 * 4 {
            anyhow::bail!("Network config reply too short");
        }
        let mut out = Vec::with_capacity(65);
        for i in 0..65 {
            let off = 4 + i * 4;
            out.push(u32::from_be_bytes(body[off..off + 4].try_into().unwrap()));
        }
        Ok(out)
    }

    pub async fn site_survey(
        &self,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Vec<[u32; 11]>> {
        let body = self.request_general_info(0x13, timeout).await?.body;
        if body.len() < 8 {
            return Ok(Vec::new());
        }
        let count = u32::from_be_bytes(body[4..8].try_into().unwrap()) as usize;
        let mut out = Vec::new();
        let max = count.min((body.len().saturating_sub(8)) / 44);
        for i in 0..max {
            let mut entry = [0u32; 11];
            for (j, slot) in entry.iter_mut().enumerate() {
                let off = 8 + i * 44 + j * 4;
                *slot = u32::from_be_bytes(body[off..off + 4].try_into().unwrap());
            }
            out.push(entry);
        }
        Ok(out)
    }

    pub async fn set_device_name(
        &self,
        name: &str,
        timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        let text = name.as_bytes();
        let clipped = &text[..text.len().min(1019)];
        let mut padded = clipped.to_vec();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        if padded.len() == clipped.len() {
            padded.push(0);
            while !padded.len().is_multiple_of(4) {
                padded.push(0);
            }
        }
        let mut body = Vec::with_capacity(4 + padded.len());
        body.extend_from_slice(&3u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::SetSystemState, body, timeout)
            .await?;
        Ok(())
    }

    pub async fn set_static_ip_config(
        &self,
        is_static: bool,
        address: &str,
        mask: &str,
        gateway: &str,
        timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        fn pack_ip(s: &str) -> anyhow::Result<u32> {
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() != 4 {
                anyhow::bail!("bad ip");
            }
            let mut v = 0u32;
            for p in parts {
                v = (v << 8) | p.parse::<u32>().map_err(|_| anyhow::anyhow!("bad ip"))?;
            }
            Ok(v)
        }
        let body_bytes: [u8; 20] = {
            let mut b = [0u8; 20];
            b[0..4].copy_from_slice(&6u32.to_be_bytes());
            b[4..8].copy_from_slice(&(if is_static { 1u32 } else { 0u32 }).to_be_bytes());
            b[8..12].copy_from_slice(&pack_ip(address)?.to_be_bytes());
            b[12..16].copy_from_slice(&pack_ip(mask)?.to_be_bytes());
            b[16..20].copy_from_slice(&pack_ip(gateway)?.to_be_bytes());
            b
        };
        self.send_request(Command::SetSystemState, body_bytes.to_vec(), timeout)
            .await?;
        Ok(())
    }

    pub async fn get_show_info(&self, timeout: std::time::Duration) -> anyhow::Result<Message> {
        self.get_show_info_guarded(timeout, &|| true).await
    }

    pub async fn get_show_info_guarded(
        &self,
        timeout: std::time::Duration,
        authorize: &(dyn Fn() -> bool + Send + Sync),
    ) -> anyhow::Result<Message> {
        self.send_request_guarded(
            Command::FileInfo,
            5u32.to_be_bytes().to_vec(),
            timeout,
            authorize,
        )
        .await
    }

    pub async fn show_ok(&self) -> anyhow::Result<()> {
        self.send_frame_no_reply(
            MessageType::Request,
            Command::ShowSnapshot,
            10u32.to_be_bytes().to_vec(),
        )
        .await
    }

    pub async fn get_show_crc(
        &self,
        pid: u32,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Message> {
        let mut body = Vec::with_capacity(8);
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010400u32.to_be_bytes());
        self.send_request(Command::ShowStatus, body, timeout).await
    }

    pub async fn set_show_parameter(&self, pid: u32, value: u32) -> anyhow::Result<()> {
        let mut body = Vec::with_capacity(12);
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010300u32.to_be_bytes());
        body.extend_from_slice(&value.to_be_bytes());
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    pub async fn set_show_name(&self, pid: u32, name: &str) -> anyhow::Result<()> {
        openfader_core::native_library::validate_name(name)?;
        let text = name.as_bytes();
        let clipped = text;
        let mut padded = clipped.to_vec();
        padded.push(0);
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = Vec::with_capacity(8 + padded.len());
        body.extend_from_slice(&pid.to_be_bytes());
        body.extend_from_slice(&0x00010300u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::ChannelNames, body, REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }
}
