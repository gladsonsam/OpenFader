//! File download/upload framing and completion operations.

use super::*;

impl MackieSession {
    // ---- file transfer ----

    pub async fn download_file(
        &self,
        filename: &str,
        timeout: std::time::Duration,
    ) -> anyhow::Result<Vec<u8>> {
        let name = filename.as_bytes();
        let mut padded = name.to_vec();
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut open_body = Vec::with_capacity(4 + padded.len());
        open_body.extend_from_slice(&0x80000004u32.to_be_bytes());
        open_body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::FileOpen, open_body, timeout)
            .await?;

        let info = self
            .send_request(
                Command::FileInfo,
                [2u32.to_be_bytes(), 0u32.to_be_bytes()].concat(),
                timeout,
            )
            .await?;
        if info.body.len() < 12 {
            anyhow::bail!("File info response too short");
        }
        let size = u32::from_be_bytes(info.body[8..12].try_into().unwrap()) as usize;
        if size == 0 {
            let _ = self
                .send_request(Command::FileClose, 0u32.to_be_bytes().to_vec(), timeout)
                .await;
            return Ok(Vec::new());
        }

        let mut out = Vec::with_capacity(size);
        const CHUNK: usize = 0x2FEC;
        while out.len() < size {
            let length = CHUNK.min(size - out.len());
            let mut req = Vec::with_capacity(16);
            req.extend_from_slice(&0u32.to_be_bytes());
            req.extend_from_slice(&(size as u32).to_be_bytes());
            req.extend_from_slice(&(out.len() as u32).to_be_bytes());
            req.extend_from_slice(&(length as u32).to_be_bytes());
            let resp = self.send_request(Command::FileRead, req, timeout).await?;
            if resp.body.len() <= 16 {
                anyhow::bail!("File read returned no data");
            }
            let chunk = word_swap(&resp.body[16..]);
            let take = length.min(chunk.len());
            if take == 0 {
                anyhow::bail!("File read returned an empty chunk");
            }
            out.extend_from_slice(&chunk[..take]);
        }
        let _ = self
            .send_request(Command::FileClose, 0u32.to_be_bytes().to_vec(), timeout)
            .await;
        out.truncate(size);
        Ok(out)
    }

    pub async fn upload_file(
        &self,
        filename: &str,
        data: &[u8],
        file_type: u32,
        timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        let payload = data;
        let total = payload.len();
        let name = filename.as_bytes();
        let mut padded_name = name.to_vec();
        while !padded_name.len().is_multiple_of(4) {
            padded_name.push(0);
        }
        let mut open_body = Vec::with_capacity(4 + padded_name.len());
        open_body.extend_from_slice(&file_type.to_be_bytes());
        open_body.extend_from_slice(&word_swap(&padded_name));
        let resp = self
            .send_request(Command::FileOpen, open_body, timeout)
            .await?;
        if resp.body.len() < 8 || u32::from_be_bytes(resp.body[0..4].try_into().unwrap()) == 0 {
            anyhow::bail!("File open for upload refused");
        }
        let file_id = u32::from_be_bytes(resp.body[4..8].try_into().unwrap());

        const CHUNK: usize = 0x2FEC;
        let mut offset = 0usize;
        while offset < total {
            let length = CHUNK.min(total - offset);
            let chunk_raw = &payload[offset..offset + length];
            let mut chunk = word_swap(chunk_raw);
            while !chunk.len().is_multiple_of(4) {
                chunk.push(0);
            }
            let mut body = Vec::with_capacity(16 + chunk.len());
            body.extend_from_slice(&file_id.to_be_bytes());
            body.extend_from_slice(&(total as u32).to_be_bytes());
            body.extend_from_slice(&(offset as u32).to_be_bytes());
            body.extend_from_slice(&(length as u32).to_be_bytes());
            body.extend_from_slice(&chunk);
            let ack = self.send_request(Command::PostFile, body, timeout).await?;
            if ack.body.len() < 4 || u32::from_be_bytes(ack.body[0..4].try_into().unwrap()) == 0 {
                anyhow::bail!("POST chunk not acknowledged");
            }
            offset += length;
        }
        let mut end_body = Vec::with_capacity(16);
        end_body.extend_from_slice(&file_id.to_be_bytes());
        end_body.extend_from_slice(&(total as u32).to_be_bytes());
        end_body.extend_from_slice(&(total as u32).to_be_bytes());
        end_body.extend_from_slice(&0u32.to_be_bytes());
        let end = self
            .send_request(Command::PostFile, end_body, timeout)
            .await?;
        if end.body.len() < 4 || u32::from_be_bytes(end.body[0..4].try_into().unwrap()) != 0 {
            anyhow::bail!("END of POST not accepted");
        }
        let _ = self
            .send_request(Command::FileClose, file_id.to_be_bytes().to_vec(), timeout)
            .await;
        Ok(())
    }

    pub async fn set_file_crc(&self, address: u32, crc32: u32) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Int(crc32), VALUE_TYPE_FILE);
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    pub async fn process_files(&self) -> anyhow::Result<()> {
        self.send_request(
            Command::ChannelInfoControl,
            10u32.to_be_bytes().to_vec(),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }
}
