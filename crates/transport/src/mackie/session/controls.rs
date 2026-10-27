//! Snapshot, channel-name, meter, and session control operations.

use super::*;

impl MackieSession {
    // ---- show snapshots (command-level, no address table needed) ----

    /// Recall a show snapshot the way the mixer app does: REQUEST `[1, n]`
    /// then RESPONSE `[1]`, fire-and-forget (no correlated reply).
    pub async fn recall_show_snapshot(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        self.recall_show_snapshot_guarded(snapshot_1_based, &|| true).await
    }

    /// Both legs share a write lane and a final authority check. No retry.
    pub async fn recall_show_snapshot_guarded(&self, snapshot_1_based: u32,
        authorize: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        anyhow::ensure!(snapshot_1_based <= 99, "Snapshot must be 0..99");
        let mut lane = self.write.lock().await;
        anyhow::ensure!(authorize(), "Native recall authority changed before dispatch");
        let req = build_snapshot_recall_request(snapshot_1_based);
        for (kind, body) in [(MessageType::Request, req),
            (MessageType::Response, 1u32.to_be_bytes().to_vec())] {
            let msg = Message::new(self.next_seq(), kind, Command::ShowSnapshot, body)?;
            lane.write_all(&msg.encode()).await?;
        }
        lane.flush().await?;
        Ok(())
    }

    /// Historical API name: op 5 NAMES an existing snapshot; it does not
    /// capture current state. Native snapshot store is op 2. Retained
    /// for wire compatibility; never advertise this wrapper as native save.
    pub async fn save_show_snapshot(
        &self,
        snapshot_1_based: u32,
        name: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!((1..=99).contains(&snapshot_1_based), "snapshot must be 1..99");
        openfader_core::native_library::validate_name(name)?;
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let body = build_snapshot_save(snapshot_1_based, epoch, name);
        self.send_request(Command::ShowSnapshot, body, REQUEST_TIMEOUT)
            .await?;
        // No echo comes back for our own save; cache it like the relay would.
        if let Ok(mut c) = self.caches.snapshot_names.lock() {
            c.insert(snapshot_1_based, name.trim().to_string());
        }
        Ok(())
    }

    /// Native op 5 naming, validated without truncation. ACK is not a
    /// snapshot relay or evidence that any mixer state was stored.
    pub async fn rename_show_snapshot_guarded(&self, slot: u32, name: &str,
        authorize: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        anyhow::ensure!((1..=99).contains(&slot), "Default snapshot cannot be renamed");
        openfader_core::native_library::validate_name(name)?;
        let epoch = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        self.send_request_guarded(Command::ShowSnapshot, build_snapshot_save(slot, epoch, name),
            REQUEST_TIMEOUT, authorize).await?;
        Ok(())
    }

    /// Store current native state or delete a user position. ACK alone is not
    /// persistence proof; the provider must read the stored index back.
    pub async fn mutate_snapshot_guarded(&self, slot: u32, delete: bool,
        authorize: &(dyn Fn() -> bool + Send + Sync)) -> anyhow::Result<()> {
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as u32;
        let body = if delete { super::super::names::build_snapshot_delete(slot, timestamp)? }
            else { super::super::names::build_snapshot_store(slot, timestamp)? };
        self.send_request_guarded(Command::ShowSnapshot, body, REQUEST_TIMEOUT, authorize).await?;
        self.caches.snapshot_names.lock().unwrap().clear();
        Ok(())
    }

    // ---- channel names (rename) ----

    pub async fn set_channel_name(&self, slot: u32, name: &str) -> anyhow::Result<()> {
        if slot == 0 {
            anyhow::bail!("slot must be >= 1");
        }
        let text = name.as_bytes();
        // Keep the complete UTF-8 string and its cached readback identical.
        // Byte clipping could split a codepoint; an embedded NUL would make
        // the desk read only a prefix of the value cached below.
        if text.len() > 63 {
            anyhow::bail!("channel name must not exceed 63 UTF-8 bytes");
        }
        if text.contains(&0) {
            anyhow::bail!("channel name must not contain NUL");
        }
        let mut padded = text.to_vec();
        padded.push(0);
        while !padded.len().is_multiple_of(4) {
            padded.push(0);
        }
        let mut body = Vec::with_capacity(8 + padded.len());
        body.extend_from_slice(&slot.to_be_bytes());
        body.extend_from_slice(&0x00010500u32.to_be_bytes());
        body.extend_from_slice(&word_swap(&padded));
        self.send_request(Command::ChannelNames, body, REQUEST_TIMEOUT)
            .await?;
        // Own rename: cached for continuity but marked pending (no desk echo
        // comes back); `name_verified` withholds it until a push covers it.
        // One lock: value insert and pending mark are atomic.
        if let Ok(mut s) = self.caches.names.lock() {
            s.names.insert(slot, name.trim().to_string());
            s.pending.insert(slot);
        }
        Ok(())
    }

    // ---- mute groups already in separate methods; expose alias ----

    // ---- meters ----

    /// Per-client meter-layout override: replaces this connection's pushed
    /// meter block with `addresses` (one word per address, in order), then
    /// re-enables broadcast reports, the same layout-then-broadcast pairing
    /// (same [`BROADCAST_CONTROL_BODY`], same [`REQUEST_TIMEOUT`] ordering)
    /// the shared init handshake and [`enable_meter_reports`](Self::enable_meter_reports)
    /// use, sourced from the native MeterLayout -> ack -> BroadcastControl
    /// -> ack activation order. The DL32S driver uses this to request its
    /// 301-entry output/dynamics/RTA layout ([`dl32s_output_meter_addresses`]);
    /// the shared init handshake keeps the 60-entry family default. Each request error is
    /// propagated (no layout-success pretence); the caller decides whether
    /// the override is optional.
    pub async fn request_meter_layout(&self, addresses: &[i32]) -> anyhow::Result<()> {
        self.send_request(
            Command::MeterLayout,
            build_meter_layout(addresses),
            REQUEST_TIMEOUT,
        )
        .await?;
        self.send_request(
            Command::BroadcastControl,
            BROADCAST_CONTROL_BODY.to_vec(),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    pub async fn enable_meter_reports(&self) -> anyhow::Result<()> {
        let addrs = dl32_family_meter_addresses();
        let _ = self
            .send_request(
                Command::MeterLayout,
                build_meter_layout(&addrs),
                REQUEST_TIMEOUT,
            )
            .await;
        let _ = self
            .send_request(
                Command::BroadcastControl,
                BROADCAST_CONTROL_BODY.to_vec(),
                REQUEST_TIMEOUT,
            )
            .await;
        Ok(())
    }

    // ---- multi-address write (fan-out to singles per Python) ----
    // Python's set_values sends one single write per address because the pairs
    // layout is misparsed by DL32S firmware. We keep build_multi_set for
    // compatibility but implement as fan-out singles.

    pub async fn set_values(
        &self,
        values: &std::collections::HashMap<u32, u32>,
        value_type: u8,
    ) -> anyhow::Result<()> {
        for (addr, raw) in values {
            self.write_int(*addr, *raw, value_type).await?;
        }
        Ok(())
    }

    // ---- delete snapshot ----

    pub async fn delete_snapshot(&self, snapshot_1_based: u32) -> anyhow::Result<()> {
        if !(1..=99).contains(&snapshot_1_based) {
            anyhow::bail!("snapshot number must be 1..99");
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0) as u32;
        let body = super::super::names::build_snapshot_delete(snapshot_1_based, epoch)?;
        self.send_request(Command::ShowSnapshot, body, REQUEST_TIMEOUT)
            .await?;
        if let Ok(mut c) = self.caches.snapshot_names.lock() {
            // Deletion compacts positions; none of the old index is safe.
            c.clear();
        }
        Ok(())
    }

    // ---- reboot ----

    pub async fn reboot(&self, mode: u32) -> anyhow::Result<()> {
        if mode != 1 && mode != 2 {
            anyhow::bail!("reboot mode must be 1 or 2");
        }
        self.send_request(
            Command::Reboot,
            mode.to_be_bytes().to_vec(),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    // ---- recall snapshot with address ----

    pub async fn recall_snapshot(&self, address: u32, snapshot_1_based: u32) -> anyhow::Result<()> {
        if snapshot_1_based == 0 {
            anyhow::bail!("snapshot number must be >= 1");
        }
        if address != 0 {
            self.write_int(address, snapshot_1_based, VALUE_TYPE_PARAM)
                .await?;
            return Ok(());
        }
        self.recall_show_snapshot(snapshot_1_based).await
    }

    // ---- disconnect notice ----

    pub async fn disconnect_notice(&self) -> anyhow::Result<()> {
        self.send_request(
            Command::ChannelInfoControl,
            [1u32.to_be_bytes(), 4u32.to_be_bytes()].concat(),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }
}
