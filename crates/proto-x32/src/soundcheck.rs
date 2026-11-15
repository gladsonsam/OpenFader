//! Host USB recording and REC/PLAY switching. The card identity, channel
//! mode, capture banks and channel selectors are read from the live desk.
//! Physical card feeds have no channel-processing tap; processed/user feeds
//! retain their native identity but cannot establish an automatic return map.
use crate::X32Driver;
use anyhow::{ensure, Context};
use openfader_core::{MixerDriver, StripId};
use rosc::OscType;
use std::collections::BTreeMap;

const BANKS: [&str; 4] = ["1-8", "9-16", "17-24", "25-32"];
pub struct RecorderInventory {
    pub interface_names: Vec<String>,
    pub capture_channels: u16,
    pub output_channels: u16,
    pub capture_sources: Vec<String>,
    pub channel_names: Vec<String>,
    pub capture_strip_ids: Vec<Vec<u16>>,
    pub input_channel_names: BTreeMap<u16, String>,
    pub expected: BTreeMap<String, i32>,
}
pub struct SoundcheckPlan {
    pub inventory: RecorderInventory,
    pub changes: Vec<(String, i32)>,
    pub output_mask: Vec<bool>,
    pub output_mapping: Vec<Option<usize>>,
}
fn physical(bank: i32, offset: usize) -> Option<String> {
    if bank == 25 && offset < 6 {
        return Some(format!("x32:aux/{}:input", offset + 1));
    }
    let (group, base) = match bank {
        0..=3 => ("local", bank * 8),
        4..=9 => ("aes50a", (bank - 4) * 8),
        10..=15 => ("aes50b", (bank - 10) * 8),
        _ => return None,
    };
    Some(format!("x32:{group}/{}:preamp", base as usize + offset + 1))
}
/// AUX remaps replace only the first 2/4/6 sockets; remaining sockets
/// continue receiving their native aux inputs (routing protocol v4.09-02).
fn aux_width(bank: i32) -> usize {
    if bank == 0 {
        0
    } else {
        [2, 4, 6][(bank as usize - 1) % 3]
    }
}
fn aux_physical(bank: i32, offset: usize) -> Option<String> {
    if offset >= aux_width(bank) {
        return Some(format!("x32:aux/{}:input", offset + 1));
    }
    match bank {
        1..=3 => physical(0, offset),
        4..=6 => physical(4, offset),
        7..=9 => physical(10, offset),
        _ => None,
    }
}
fn source_values(expected: &BTreeMap<String, i32>) -> impl Iterator<Item = i32> + '_ {
    expected
        .iter()
        .filter(|(key, _)| key.starts_with("/ch/") || key.starts_with("/auxin/"))
        .map(|(_, value)| *value)
}
fn valid_key(key: &str) -> bool {
    matches!(
        key,
        "/-stat/xcardtype"
            | "/-prefs/card/UFmode"
            | "/-prefs/card/USBmode"
            | "/-prefs/card/URECplayb"
            | "/config/routing/routswitch"
            | "/config/routing/IN/AUX"
            | "/config/routing/PLAY/AUX"
    ) || ["IN", "PLAY", "CARD"].iter().any(|bank| {
        BANKS
            .iter()
            .any(|range| key == format!("/config/routing/{bank}/{range}"))
    }) || (1..=32).any(|ch| key == format!("/ch/{ch:02}/config/source"))
        || (1..=8).any(|ch| key == format!("/auxin/{ch:02}/config/source"))
}
fn writable(key: &str) -> bool {
    key.starts_with("/config/routing/PLAY/")
        || key.starts_with("/config/routing/IN/")
        || key == "/config/routing/routswitch"
}
impl X32Driver {
    async fn soundcheck_check_values(&self, values: &[(String, i32)]) -> anyhow::Result<()> {
        // Bound query concurrency avoids serial network round trips during
        // routing transitions without flooding the console control socket.
        for chunk in values.chunks(4) {
            let verify = |index: usize| async move {
                if let Some((path, expected)) = chunk.get(index) {
                    ensure!(valid_key(path), "Invalid soundcheck dependency");
                    ensure!(
                        self.read_int(path).await? == *expected,
                        "Routing changed: {path}"
                    );
                }
                anyhow::Ok(())
            };
            let (a, b, c, d) = tokio::join!(verify(0), verify(1), verify(2), verify(3));
            a?;
            b?;
            c?;
            d?;
        }
        Ok(())
    }

    pub async fn recorder_inventory(&self) -> anyhow::Result<RecorderInventory> {
        let session = self.session()?;
        let mut expected = BTreeMap::new();
        let card = self.read_int("/-stat/xcardtype").await?;
        expected.insert("/-stat/xcardtype".into(), card);
        let (mode_path, aliases) = match card {
            1 => (Some("/-prefs/card/UFmode"), vec!["X-UF"]),
            2 => (Some("/-prefs/card/USBmode"), vec!["X-USB", "XUSB"]),
            6 => (Some("/-prefs/card/USBmode"), vec!["DN32-USB"]),
            10 => (None, vec!["X-LIVE", "XLIVE"]),
            _ => anyhow::bail!(
                "Installed expansion card does not advertise a supported host USB interface"
            ),
        };
        let (capture_channels, output_channels) = if let Some(path) = mode_path {
            let mode = self.read_int(path).await?;
            expected.insert(path.into(), mode);
            // X32 USB Interface Operation Guide: 32-in/8-out is the
            // recording/overdub host mode; 8-in/32-out is host mixdown.
            match mode {
                0 => (32, 32),
                1 => (16, 16),
                2 => (32, 8),
                3 => (8, 32),
                4 if card != 1 => (8, 8),
                5 if card != 1 => (2, 2),
                _ => anyhow::bail!("Unrecognized installed-card USB channel mode"),
            }
        } else {
            let source = self.read_int("/-prefs/card/URECplayb").await?;
            expected.insert("/-prefs/card/URECplayb".into(), source);
            (32, 32)
        };
        let mode = self.read_int("/config/routing/routswitch").await?;
        ensure!((0..=1).contains(&mode), "Unrecognized REC/PLAY state");
        expected.insert("/config/routing/routswitch".into(), mode);
        for plane in ["IN", "PLAY", "CARD"] {
            for range in BANKS {
                let path = format!("/config/routing/{plane}/{range}");
                let value = self.read_int(&path).await?;
                ensure!(
                    (0..if plane == "CARD" { 36 } else { 24 }).contains(&value),
                    "Unrecognized {plane} routing bank"
                );
                expected.insert(path, value);
            }
        }
        for plane in ["IN", "PLAY"] {
            let path = format!("/config/routing/{plane}/AUX");
            let value = self.read_int(&path).await?;
            ensure!((0..16).contains(&value), "Unrecognized aux routing bank");
            expected.insert(path, value);
        }
        let mut capture_sources = Vec::new();
        for lane in 0..usize::from(capture_channels) {
            let bank = expected[&format!("/config/routing/CARD/{}", BANKS[lane / 8])];
            capture_sources.push(
                physical(bank, lane % 8)
                    .unwrap_or_else(|| format!("x32:card-bank/{bank}/{}", lane % 8 + 1)),
            );
        }
        let mut channel_names = capture_sources
            .iter()
            .enumerate()
            .map(|(lane, source)| format!("Card {} ({source})", lane + 1))
            .collect::<Vec<_>>();
        let mut capture_strip_ids = vec![Vec::new(); usize::from(capture_channels)];
        let mut input_channel_names = BTreeMap::new();
        for channel in 0..40u16 {
            let path = if channel < 32 {
                format!("/ch/{:02}/config/source", channel + 1)
            } else {
                format!("/auxin/{:02}/config/source", channel - 31)
            };
            let source = self.read_int(&path).await?;
            ensure!((0..=64).contains(&source), "Unrecognized input source");
            expected.insert(path, source);
            let name = self.get_name(StripId(channel)).await?;
            if channel < 32 {
                input_channel_names.insert(channel, name.clone());
            }
            let plane = if mode == 0 { "IN" } else { "PLAY" };
            let identity = if (1..=32).contains(&source) {
                let lane = (source - 1) as usize;
                let bank = expected[&format!("/config/routing/{plane}/{}", BANKS[lane / 8])];
                physical(bank, lane % 8)
            } else if (33..=38).contains(&source) {
                aux_physical(
                    expected[&format!("/config/routing/{plane}/AUX")],
                    (source - 33) as usize,
                )
            } else {
                None
            };
            if let Some(identity) = identity {
                for (capture, captured) in capture_sources.iter().enumerate() {
                    if captured == &identity {
                        if capture_strip_ids[capture].is_empty() && !name.trim().is_empty() {
                            channel_names[capture] = name.clone();
                        }
                        capture_strip_ids[capture].push(channel);
                    }
                }
            }
        }
        self.ensure_current(&session)?;
        Ok(RecorderInventory {
            interface_names: aliases.into_iter().map(String::from).collect(),
            capture_channels,
            output_channels,
            capture_sources,
            channel_names,
            capture_strip_ids,
            input_channel_names,
            expected,
        })
    }
    pub async fn soundcheck_plan(&self) -> anyhow::Result<SoundcheckPlan> {
        let inventory = self.recorder_inventory().await?;
        let expected = &inventory.expected;
        if let Some(source) = expected.get("/-prefs/card/URECplayb") {
            ensure!(
                *source == 1,
                "X-LIVE card returns currently select SD playback; select USB for host soundcheck"
            );
        }
        let original_mode = expected["/config/routing/routswitch"];
        let (active, inactive) = if original_mode == 0 {
            ("IN", "PLAY")
        } else {
            ("PLAY", "IN")
        };
        let mut changes = Vec::new();
        let mut output_mask = vec![false; usize::from(inventory.output_channels)];
        let mut reserved = vec![false; usize::from(inventory.output_channels)];
        let available = usize::from(inventory.capture_channels.min(inventory.output_channels));
        let aux_bank = expected[&format!("/config/routing/{active}/AUX")];
        // Preserve card returns that are already live, including channels
        // routed through the AUX remap. They are never playback targets.
        for source in source_values(expected) {
            let return_lane = if (1..=32).contains(&source) {
                let lane = (source - 1) as usize;
                let bank = expected[&format!("/config/routing/{active}/{}", BANKS[lane / 8])];
                if (16..=19).contains(&bank) {
                    Some((bank - 16) as usize * 8 + lane % 8)
                } else {
                    None
                }
            } else if (33..=38).contains(&source)
                && (10..=12).contains(&aux_bank)
                && ((source - 33) as usize) < aux_width(aux_bank)
            {
                Some((source - 33) as usize)
            } else {
                None
            };
            if let Some(lane) = return_lane {
                if let Some(slot) = reserved.get_mut(lane) {
                    *slot = true;
                }
            }
        }
        for (bank_index, range) in BANKS.iter().enumerate() {
            let original = expected[&format!("/config/routing/{active}/{range}")];
            let used_offsets = source_values(expected)
                .filter_map(|value| {
                    ((1..=32).contains(&value) && (value as usize - 1) / 8 == bank_index)
                        .then_some((value as usize).wrapping_sub(1) % 8)
                })
                .collect::<Vec<_>>();
            let desired = if !used_offsets.is_empty() && original < 16 {
                let capture_bank = BANKS.iter().enumerate().find(|(index, capture_range)| {
                    used_offsets.iter().all(|offset| index * 8 + offset < available && !reserved[index * 8 + offset])
                        && expected[&format!("/config/routing/CARD/{capture_range}")] == original
                }).map(|(index, _)| index).context(format!("Input bank {range} has no captured USB return bank free of live card inputs"))?;
                for offset in used_offsets {
                    output_mask[capture_bank * 8 + offset] = true;
                }
                16 + capture_bank as i32
            } else {
                ensure!(
                    used_offsets.is_empty() || original < 20,
                    "Input bank {range} uses user inputs with unresolved card-return dependencies"
                );
                original
            };
            changes.push((format!("/config/routing/{inactive}/{range}"), desired));
        }
        let aux_offsets = source_values(expected)
            .filter(|source| (33..=38).contains(source))
            .map(|source| (source - 33) as usize)
            .collect::<Vec<_>>();
        let physical_aux_offsets = aux_offsets
            .iter()
            .copied()
            .filter(|offset| aux_physical(aux_bank, *offset).is_some())
            .collect::<Vec<_>>();
        ensure!(
            !aux_offsets
                .iter()
                .any(|offset| (13..=15).contains(&aux_bank) && *offset < aux_width(aux_bank)),
            "AUX remap uses user inputs with unresolved card-return dependencies"
        );
        let desired_aux = if physical_aux_offsets.is_empty() {
            aux_bank
        } else {
            for offset in &physical_aux_offsets {
                let source = aux_physical(aux_bank, *offset).unwrap();
                ensure!(*offset < available && !reserved[*offset] && inventory.capture_sources[*offset] == source, "AUX input {} needs its physical capture on USB {}; AUX card returns address channels 1 through 6", offset + 1, offset + 1);
                output_mask[*offset] = true;
            }
            let needed = physical_aux_offsets.iter().max().unwrap() + 1;
            let width = if (10..=12).contains(&aux_bank) {
                needed.max(aux_width(aux_bank))
            } else {
                needed
            };
            if width <= 2 {
                10
            } else if width <= 4 {
                11
            } else {
                12
            }
        };
        changes.push((format!("/config/routing/{inactive}/AUX"), desired_aux));
        // All inactive banks are prepared before the one audible selector.
        changes.push(("/config/routing/routswitch".into(), 1 - original_mode));
        ensure!(
            output_mask.iter().any(|used| *used),
            "No physical input bank is captured for soundcheck"
        );
        let output_mapping = (0..usize::from(inventory.capture_channels))
            .map(|lane| {
                output_mask
                    .get(lane)
                    .copied()
                    .unwrap_or(false)
                    .then_some(lane)
            })
            .collect();
        Ok(SoundcheckPlan {
            inventory,
            changes,
            output_mask,
            output_mapping,
        })
    }
    async fn soundcheck_recovery_state(&self) -> anyhow::Result<serde_json::Value> {
        let mut expected = BTreeMap::new();
        // Restoring REC/PLAY banks does not require a USB-capable card or
        // the recording channel mode that was present before disconnect.
        for plane in ["IN", "PLAY", "CARD"] {
            for range in BANKS {
                let key = format!("/config/routing/{plane}/{range}");
                expected.insert(key.clone(), self.read_int(&key).await?);
            }
        }
        for key in [
            "/config/routing/IN/AUX",
            "/config/routing/PLAY/AUX",
            "/config/routing/routswitch",
            "/-stat/xcardtype",
        ] {
            expected.insert(key.into(), self.read_int(key).await?);
        }
        for key in [
            "/-prefs/card/UFmode",
            "/-prefs/card/USBmode",
            "/-prefs/card/URECplayb",
        ] {
            if let Ok(value) = self.read_int(key).await {
                expected.insert(key.into(), value);
            }
        }
        for channel in 0..40 {
            let key = if channel < 32 {
                format!("/ch/{:02}/config/source", channel + 1)
            } else {
                format!("/auxin/{:02}/config/source", channel - 31)
            };
            expected.insert(key.clone(), self.read_int(&key).await?);
        }
        Ok(serde_json::json!({"expected": expected}))
    }
    pub async fn soundcheck_read(
        &self,
        key: &str,
        state: &serde_json::Value,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<i32> {
        ensure!(valid_key(key), "Invalid X32 soundcheck route");
        let members = (0..40).map(StripId).collect::<Vec<_>>();
        check(&members)?;
        if !state.is_null() {
            let expected = state["expected"]
                .as_object()
                .context("Missing X32 soundcheck guards")?;
            ensure!(expected.contains_key(key), "Unjournaled X32 routing key");
            ensure!(
                self.read_int(key).await? as i64
                    == expected[key].as_i64().context("Invalid routing guard")?,
                "X32 routing changed: {key}"
            );
        }
        let value = self.read_int(key).await?;
        check(&members)?;
        Ok(value)
    }
    pub async fn soundcheck_write(
        &self,
        key: &str,
        state: &serde_json::Value,
        expected: i32,
        desired: i32,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<serde_json::Value> {
        ensure!(
            valid_key(key) && writable(key),
            "Invalid or unguarded X32 soundcheck write"
        );
        let recovered;
        let state = if state.is_null() {
            recovered = self.soundcheck_recovery_state().await?;
            &recovered
        } else {
            state
        };
        let max = if key.ends_with("routswitch") {
            1
        } else if key.ends_with("/AUX") {
            15
        } else {
            23
        };
        ensure!(
            (0..=max).contains(&desired),
            "X32 routing value out of range"
        );
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        ensure!(
            self.soundcheck_read(key, state, check).await? == expected,
            "X32 routing conflict"
        );
        let guard_values = state["expected"]
            .as_object()
            .context("Missing X32 guards")?
            .iter()
            .map(|(path, value)| {
                Ok((
                    path.clone(),
                    i32::try_from(value.as_i64().context("Invalid X32 routing guard")?)?,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.soundcheck_check_values(&guard_values).await?;
        self.ensure_current(&session)?;
        check(&(0..40).map(StripId).collect::<Vec<_>>())?;
        session.send(key, vec![OscType::Int(desired)]).await?;
        ensure!(
            self.read_int(key).await? == desired,
            "X32 routing readback mismatch"
        );
        self.ensure_current(&session)?;
        let mut next = state.clone();
        next["expected"][key] = desired.into();
        Ok(next)
    }
}
