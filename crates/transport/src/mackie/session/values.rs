//! Raw writes, cached readback, and own-write provenance accessors.

use super::*;

impl MackieSession {
    // ---- raw value access (driver maps fields to addresses) ----

    /// Write one address as a raw u32 (bools, enums, ints, mute group).
    ///
    /// The write ACK only proves the desk accepted the frame. The mixer
    /// never echoes our own writes back, so the word is cached for
    /// continuity but marked as own-write-pending: `cached` still serves it
    /// (snapshots stay continuous) while `cached_verified` withholds it
    /// until a desk push covers the address.
    pub async fn write_int(&self, address: u32, value: u32, value_type: u8) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Int(value), value_type);
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        if value_type == VALUE_TYPE_MUTE_GROUP {
            self.caches
                .type2_values
                .lock()
                .map(|mut s| {
                    s.values.insert(address, value);
                    s.pending.insert(address);
                })
                .expect("type2 cache poisoned");
        } else {
            self.caches
                .values
                .lock()
                .map(|mut s| {
                    s.values.insert(address, value);
                    s.pending.insert(address);
                })
                .expect("value cache poisoned");
        }
        Ok(())
    }

    /// Write one address as a big-endian float (dB faders, gains, freqs).
    /// Same accepted-but-unverified provenance as [`write_int`](Self::write_int).
    pub async fn write_float(&self, address: u32, value: f32) -> anyhow::Result<()> {
        let body = build_single_set(address, RawValue::Float(value), VALUE_TYPE_PARAM);
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        self.caches
            .values
            .lock()
            .map(|mut s| {
                s.values.insert(address, value.to_bits());
                s.pending.insert(address);
            })
            .expect("value cache poisoned");
        Ok(())
    }

    /// App-style multi-address write of raw u32 pairs.
    /// Same accepted-but-unverified provenance as [`write_int`](Self::write_int).
    pub async fn write_multi(&self, pairs: &[(u32, u32)], value_type: u8) -> anyhow::Result<()> {
        let body = build_multi_set(pairs, value_type)?;
        self.send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        if value_type == VALUE_TYPE_MUTE_GROUP {
            if let Ok(mut s) = self.caches.type2_values.lock() {
                for (addr, raw) in pairs {
                    s.values.insert(*addr, *raw);
                    s.pending.insert(*addr);
                }
            }
        } else if let Ok(mut s) = self.caches.values.lock() {
            for (addr, raw) in pairs {
                s.values.insert(*addr, *raw);
                s.pending.insert(*addr);
            }
        }
        Ok(())
    }

    /// Request a fresh desk push of the value space on this session.
    /// This repeats only the final read-only initialization request; it does
    /// not change meter layout or broadcast settings. The response is an ACK,
    /// not value evidence: consumers must still observe a subsequent push.
    pub async fn request_value_snapshot(&self) -> anyhow::Result<()> {
        self.send_request(
            Command::ChannelInfoControl,
            6u32.to_be_bytes().to_vec(),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    /// Read one global word using the native single-parameter GET (0x12).
    /// manufacturer mixer software 5.2 `singleParameterGetMandolinMessage` at 0x1001d0f90
    /// builds [PID, 0x00010000 | target << 8]. Unlike sync-current-state
    /// (0x06/6), this explicitly selects the global/type-2 bank. Only an
    /// inbound value payload proves readback; an empty ACK proves nothing.
    pub async fn request_global_value(&self, address: u32) -> anyhow::Result<()> {
        self.send_request(
            Command::ShowStatus,
            build_read_request(address, 1, VALUE_TYPE_MUTE_GROUP),
            REQUEST_TIMEOUT,
        )
        .await?;
        Ok(())
    }

    /// Probe read (the mixer answers reads with an empty body; the live
    /// value space only arrives via pushes). Noncontiguous sparse replies
    /// are withheld because this API has no way to return their addresses.
    pub async fn read_values(&self, start: u32, count: u32) -> anyhow::Result<Vec<u32>> {
        let body = build_read_request(start, count, VALUE_TYPE_PARAM);
        let resp = self
            .send_request(Command::ChannelValues, body, REQUEST_TIMEOUT)
            .await?;
        Ok(parse_value_block(&resp.body)
            .map(|b| b.raws)
            .unwrap_or_default())
    }

    /// Cached type-5 word, if a push (or our own write) has populated it.
    /// Includes own-write-pending words (continuity for snapshots); use
    /// [`cached_verified`](Self::cached_verified) when desk proof is needed.
    pub fn cached(&self, address: u32) -> Option<u32> {
        self.caches
            .values
            .lock()
            .ok()?
            .values
            .get(&address)
            .copied()
    }

    /// Atomic display/intent snapshot of adjacent parameter words.
    /// Includes acknowledged own writes; this never supplies verification.
    pub fn cached_range<const N: usize>(&self, start: u32) -> Option<[u32; N]> {
        let store = self.caches.values.lock().ok()?;
        (0..N)
            .map(|i| store.values.get(&(start + i as u32)).copied())
            .collect::<Option<Vec<_>>>()?
            .try_into()
            .ok()
    }

    /// Receipt baseline remains available while an own write is pending.
    pub fn observed_value_sequence(&self, address: u32) -> Option<u64> {
        Some(self.caches.values.lock().ok()?.observed.get(&address)?.2)
    }
    /// One-lock complete type-5 capture: raw value, observed-change revision,
    /// and receipt sequence. Own ACK/cache writes never advance this evidence.
    /// Revisions detect only transitions received by this transport session.
    pub fn verified_value_snapshot(&self, addresses: &[u32]) -> Option<Vec<(u32, u64, u64)>> {
        let s = self.caches.values.lock().ok()?;
        addresses
            .iter()
            .map(|a| {
                if s.pending.contains(a) {
                    None
                } else {
                    s.observed.get(a).copied()
                }
            })
            .collect()
    }

    /// Cached type-5 word with desk provenance: `Some` only when a desk push
    /// covered the address after our last own write. `None` means no proof
    /// (never pushed, or only our own unverified write is cached). Callers
    /// mapping this to an error must avoid the engine's drop-class
    /// substrings (`not supported` / `timed out`) so pollers keep polling
    /// instead of evicting the param. Atomic with [`cached`](Self::cached):
    /// one lock covers the pending check and the value read.
    pub fn cached_verified(&self, address: u32) -> Option<u32> {
        let s = self.caches.values.lock().ok()?;
        if s.pending.contains(&address) {
            return None;
        }
        s.values.get(&address).copied()
    }

    /// Cached type-2 word (mute-group masters). See [`cached`](Self::cached)
    /// for the provenance split.
    pub fn cached_type2(&self, address: u32) -> Option<u32> {
        self.caches
            .type2_values
            .lock()
            .ok()?
            .values
            .get(&address)
            .copied()
    }

    /// Desk-proven type-2 word (see [`cached_verified`](Self::cached_verified)).
    /// One lock covers the pending check and the value read.
    pub fn cached_type2_verified(&self, address: u32) -> Option<u32> {
        let s = self.caches.type2_values.lock().ok()?;
        if s.pending.contains(&address) {
            return None;
        }
        s.values.get(&address).copied()
    }

    /// Cached channel name slot (empty/unknown slots report `None`).
    pub fn name(&self, slot: u32) -> Option<String> {
        self.name_slot(slot).filter(|name| !name.is_empty())
    }

    /// Inclusive slot value. A pushed empty string is a known unnamed slot;
    /// None means no value has arrived. Own writes remain display-only.
    pub fn name_slot(&self, slot: u32) -> Option<String> {
        self.caches.names.lock().ok()?.names.get(&slot).cloned()
    }

    /// Desk-proven channel name (withholds own-write-pending renames).
    /// One lock covers the pending check and the value read.
    pub fn name_verified(&self, slot: u32) -> Option<String> {
        self.name_slot_verified(slot)
            .filter(|name| !name.is_empty())
    }

    /// Desk-proven slot value, preserving known blanks and withholding own
    /// pending writes. This is the parameter plane's actual string value.
    pub fn name_slot_verified(&self, slot: u32) -> Option<String> {
        let s = self.caches.names.lock().ok()?;
        if s.pending.contains(&slot) {
            return None;
        }
        s.names.get(&slot).cloned()
    }

    pub fn snapshot_names(&self) -> HashMap<u32, String> {
        self.caches
            .snapshot_names
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    pub fn current_snapshot(&self) -> Option<u32> {
        self.caches.current_snapshot.lock().ok().and_then(|g| *g)
    }

    /// Push-evidenced snapshot names: ONLY genuine desk snapshot pushes
    /// populate this (see the `snapshot_names_pushed` store docs). Own
    /// `save_show_snapshot` optimism and `delete_snapshot` removals never
    /// touch it. Passive read (no `Show.zip` download, no wire traffic).
    pub fn snapshot_names_pushed(&self) -> HashMap<u32, String> {
        self.caches
            .snapshot_names_pushed
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Push-evidenced current snapshot marker: ONLY a genuine desk recall
    /// relay sets this. `None` means no recall observed, never a slot-0
    /// default. Passive read (no wire traffic).
    pub fn current_snapshot_pushed(&self) -> Option<u32> {
        self.caches
            .current_snapshot_pushed
            .lock()
            .ok()
            .and_then(|g| *g)
    }

    // ---- mute groups (fixed protocol addresses) ----

    pub async fn set_mute_group(&self, group_1_based: u32, muted: bool) -> anyhow::Result<()> {
        let address = mute_group_master_address(group_1_based)?;
        self.write_int(address, u32::from(muted), VALUE_TYPE_MUTE_GROUP)
            .await
    }

    pub fn cached_mute_group(&self, group_1_based: u32) -> Option<bool> {
        let address = mute_group_master_address(group_1_based).ok()?;
        self.cached_type2(address).map(|v| v != 0)
    }

    /// Desk-proven mute-group master (withholds own-write-pending values).
    pub fn cached_mute_group_verified(&self, group_1_based: u32) -> Option<bool> {
        let address = mute_group_master_address(group_1_based).ok()?;
        self.cached_type2_verified(address).map(|v| v != 0)
    }
}
