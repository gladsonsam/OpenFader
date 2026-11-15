//! Provider-owned host USB patch inventory and reversible main/alternate
//! soundcheck. Source widths come from live M/ST/M-S modes. Unsupported
//! processed/user output patches never become guessed physical input routes.
//! Protocol: WING Remote Protocols FW 3.0.6, /io/out/USB, /io/in and
//! /$globals/usbacfg (older firmware uses /cfg/usbacfg).
use crate::{
    mapping::{root, Mapping, WireKind},
    native, WingDriver, ROUTING_GROUPS,
};
use anyhow::{ensure, Context};
use openfader_core::{MixerDriver, ParamValue, StripId};
use rosc::OscType;
use std::collections::BTreeMap;
const GROUPS: &[&str] = &[
    "OFF", "LCL", "AUX", "A", "B", "C", "SC", "USB", "CRD", "MOD", "PLAY", "AES", "USR", "OSC",
    "BUS", "MAIN", "MTX", "SEND", "MON",
];
const MODES: &[&str] = &["M", "ST", "M/S"];
const WIDTHS: &[u16] = &[2, 8, 16, 32, 48];
pub struct RecorderInventory {
    pub channels: u16,
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
    pub dependencies: BTreeMap<String, Vec<String>>,
}
fn physical(group: &str) -> bool {
    matches!(group, "LCL" | "AUX" | "A" | "B" | "C" | "SC" | "AES")
}
fn identity(group: &str, index: i32) -> String {
    format!("wing:io/in/{group}/{index}:source")
}
fn group(value: i32) -> anyhow::Result<&'static str> {
    GROUPS
        .get(usize::try_from(value)?)
        .copied()
        .context("Invalid WING source group")
}
fn valid_key(key: &str) -> bool {
    if matches!(key, "/cfg/usbacfg" | "/$globals/usbacfg") {
        return true;
    }
    if let Some(rest) = key.strip_prefix("/io/out/USB/") {
        return rest.split_once('/').is_some_and(|(index, field)| {
            index.parse::<u16>().is_ok_and(|i| (1..=48).contains(&i))
                && matches!(field, "grp" | "in")
        });
    }
    if let Some(rest) = key.strip_prefix("/io/in/") {
        let parts = rest.split('/').collect::<Vec<_>>();
        return parts.len() == 3
            && parts[2] == "mode"
            && ROUTING_GROUPS.iter().any(|(g, _, count)| {
                *g == parts[0]
                    && parts[1]
                        .parse::<u8>()
                        .is_ok_and(|index| (1..=*count).contains(&index))
            });
    }
    (0..48).any(|strip| {
        root(StripId(strip)).is_ok_and(|(path, _)| {
            [
                "in/conn/grp",
                "in/conn/in",
                "in/conn/altgrp",
                "in/conn/altin",
                "in/set/altsrc",
                "in/set/$mode",
                "in/set/srcauto",
            ]
            .iter()
            .any(|field| key == format!("{path}/{field}"))
        })
    })
}
fn writable(key: &str) -> bool {
    key.contains("/in/conn/")
        || key.ends_with("/in/set/altsrc")
        || key.ends_with("/in/set/srcauto")
        || (key.starts_with("/io/in/USB/") && key.ends_with("/mode"))
}
impl WingDriver {
    async fn soundcheck_check_values(&self, values: &[(String, i32)]) -> anyhow::Result<()> {
        // Bound query concurrency avoids serial network round trips during
        // routing transitions without flooding the console control socket.
        for chunk in values.chunks(4) {
            let verify = |index: usize| async move {
                if let Some((path, expected)) = chunk.get(index) {
                    ensure!(valid_key(path), "Invalid soundcheck dependency");
                    ensure!(
                        self.soundcheck_value(path).await? == *expected,
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

    async fn soundcheck_value(&self, key: &str) -> anyhow::Result<i32> {
        ensure!(valid_key(key), "Invalid WING soundcheck key");
        let kind = if key.ends_with("usbacfg") {
            WireKind::Text(16)
        } else if key.ends_with("grp") {
            WireKind::Enum(GROUPS)
        } else if key.ends_with("/mode") || key.ends_with("/$mode") {
            WireKind::Enum(MODES)
        } else if key.ends_with("srcauto") {
            WireKind::Bool
        } else if key.ends_with("altsrc") {
            WireKind::Int(0, 1)
        } else {
            WireKind::Int(1, 64)
        };
        let value = self
            .read(&Mapping {
                path: key.into(),
                kind,
                model: None,
            })
            .await?;
        match value {
            ParamValue::Text(value) if key.ends_with("usbacfg") => {
                let (a, b) = value
                    .split_once('/')
                    .context("Invalid WING USB configuration")?;
                let width: u16 = a.parse()?;
                ensure!(
                    b.parse::<u16>()? == width && WIDTHS.contains(&width),
                    "Unsupported WING USB channel configuration"
                );
                Ok(i32::from(width))
            }
            ParamValue::Float(value) => Ok(value as i32),
            ParamValue::Bool(value) => Ok(i32::from(value)),
            _ => anyhow::bail!("Unsupported WING soundcheck reply"),
        }
    }
    pub async fn recorder_inventory(&self) -> anyhow::Result<RecorderInventory> {
        let session = self.session()?;
        let mut expected = BTreeMap::new();
        let (config, channels) = match self.soundcheck_value("/$globals/usbacfg").await {
            Ok(channels) => ("/$globals/usbacfg", channels as u16),
            Err(_) => (
                "/cfg/usbacfg",
                self.soundcheck_value("/cfg/usbacfg").await? as u16,
            ),
        };
        expected.insert(config.into(), i32::from(channels));
        // Native descriptors ensure the active USB lanes actually exist on
        // this model/firmware rather than inheriting another variant's roster.
        let defs = self.native_defs(&session, "/io/out/USB").await?;
        for lane in 1..=channels {
            ensure!(
                defs.iter().any(|def| matches!(def.kind, native::Kind::Node)
                    && def
                        .address_name()
                        .is_ok_and(|name| name == lane.to_string())),
                "WING USB lane {lane} is not present in native output inventory"
            );
        }
        let mut capture_sources = Vec::new();
        let mut channel_names = Vec::new();
        for lane in 1..=channels {
            let prefix = format!("/io/out/USB/{lane}");
            let grp = self.soundcheck_value(&format!("{prefix}/grp")).await?;
            let input = self.soundcheck_value(&format!("{prefix}/in")).await?;
            expected.insert(format!("{prefix}/grp"), grp);
            expected.insert(format!("{prefix}/in"), input);
            let source = group(grp)?;
            capture_sources.push(identity(source, input));
            channel_names.push(format!("USB {lane} ({source} {input})"));
        }
        let mut capture_strip_ids = vec![Vec::new(); usize::from(channels)];
        let mut input_channel_names = BTreeMap::new();
        for strip in 0..48 {
            let id = StripId(strip);
            let prefix = root(id)?.0;
            let mut values = Vec::new();
            for field in [
                "in/conn/grp",
                "in/conn/in",
                "in/conn/altgrp",
                "in/conn/altin",
                "in/set/altsrc",
                "in/set/$mode",
                "in/set/srcauto",
            ] {
                let key = format!("{prefix}/{field}");
                let value = self.soundcheck_value(&key).await?;
                expected.insert(key, value);
                values.push(value);
            }
            let selected = values[4] as usize * 2;
            let source_group = group(values[selected])?;
            let source_index = values[selected + 1];
            let name = self.get_name(id).await?;
            if strip < 40 {
                input_channel_names.insert(strip, name.clone());
            }
            let width = if values[5] == 0 { 1 } else { 2 };
            for component in 0..width {
                let source = identity(source_group, source_index + component);
                for (lane, captured) in capture_sources.iter().enumerate() {
                    if captured == &source && source_group != "OFF" {
                        if capture_strip_ids[lane].is_empty() && !name.trim().is_empty() {
                            channel_names[lane] = if width == 1 {
                                name.clone()
                            } else {
                                format!("{name} {}", if component == 0 { "L" } else { "R" })
                            };
                        }
                        capture_strip_ids[lane].push(strip);
                    }
                }
            }
        }
        self.ensure_current(&session)?;
        Ok(RecorderInventory {
            channels,
            capture_sources,
            channel_names,
            capture_strip_ids,
            input_channel_names,
            expected,
        })
    }
    pub async fn soundcheck_plan(&self) -> anyhow::Result<SoundcheckPlan> {
        let mut inventory = self.recorder_inventory().await?;
        let mut preparation = Vec::new();
        let mut mode_changes = BTreeMap::new();
        let mut auto_changes = Vec::new();
        let mut dependencies = BTreeMap::new();
        let mut selection = Vec::new();
        let mut output_mask = vec![false; usize::from(inventory.channels)];
        let mut reserved = vec![false; usize::from(inventory.channels)];
        let mut reserved_guards = Vec::new();
        for strip in 0..48 {
            let prefix = root(StripId(strip))?.0;
            let selected = inventory.expected[&format!("{prefix}/in/set/altsrc")];
            let active = if selected == 0 { "" } else { "alt" };
            let source_group = group(inventory.expected[&format!("{prefix}/in/conn/{active}grp")])?;
            ensure!(
                source_group != "USR",
                "Input {} uses a user signal with unresolved USB dependencies",
                strip + 1
            );
            if source_group == "USB" {
                let lane = usize::try_from(
                    inventory.expected[&format!("{prefix}/in/conn/{active}in")] - 1,
                )?;
                let width = if inventory.expected[&format!("{prefix}/in/set/$mode")] == 0 {
                    1
                } else {
                    2
                };
                ensure!(
                    lane + width <= reserved.len(),
                    "Live USB input {} exceeds the active USB interface width",
                    strip + 1
                );
                reserved[lane..lane + width].fill(true);
                reserved_guards.extend(
                    inventory
                        .expected
                        .keys()
                        .filter(|key| key.starts_with(&format!("{prefix}/in/")))
                        .cloned(),
                );
            }
        }
        for strip in 0..48 {
            let prefix = root(StripId(strip))?.0;
            let selected = inventory.expected[&format!("{prefix}/in/set/altsrc")];
            let active = if selected == 0 { "" } else { "alt" };
            let inactive = if selected == 0 { "alt" } else { "" };
            let mut source_dependencies = inventory
                .expected
                .keys()
                .filter(|path| {
                    path.starts_with(&format!("{prefix}/in/")) || path.ends_with("usbacfg")
                })
                .cloned()
                .collect::<Vec<_>>();
            let source_group = group(inventory.expected[&format!("{prefix}/in/conn/{active}grp")])?;
            if source_group == "OFF" {
                continue;
            }
            if !physical(source_group) {
                continue;
            }
            let input = inventory.expected[&format!("{prefix}/in/conn/{active}in")];
            let source_identity = identity(source_group, input);
            let lane = inventory
                .capture_sources
                .iter()
                .position(|source| source == &source_identity)
                .context(format!(
                    "Input {} ({source_group} {input}) is absent from USB capture",
                    strip + 1
                ))?;
            let mode_key = format!("/io/in/{source_group}/{input}/mode");
            let mode = self.soundcheck_value(&mode_key).await?;
            source_dependencies.push(mode_key.clone());
            inventory.expected.insert(mode_key, mode);
            let effective = inventory.expected[&format!("{prefix}/in/set/$mode")];
            ensure!(
                effective == mode,
                "Input {} has a mode override; automatic stereo mapping needs review",
                strip + 1
            );
            let width = if mode == 0 { 1 } else { 2 };
            if width == 2 {
                ensure!(
                    inventory.capture_sources.get(lane + 1)
                        == Some(&identity(source_group, input + 1)),
                    "Stereo input {} is not captured on adjacent USB lanes",
                    strip + 1
                );
            }
            let usb_mode_key = format!("/io/in/USB/{}/mode", lane + 1);
            let usb_mode = self.soundcheck_value(&usb_mode_key).await?;
            source_dependencies.push(usb_mode_key.clone());
            inventory.expected.insert(usb_mode_key.clone(), usb_mode);
            for offset in 0..width {
                source_dependencies.extend([
                    format!("/io/out/USB/{}/grp", lane + offset + 1),
                    format!("/io/out/USB/{}/in", lane + offset + 1),
                ]);
            }
            let affected_width = if mode == 0 && usb_mode == 0 { 1 } else { 2 };
            ensure!(lane + affected_width <= reserved.len() && reserved[lane..lane + affected_width].iter().all(|used| !used), "USB return {} is already selected by a live input or exceeds the USB channel range", lane + 1);
            if usb_mode != mode {
                let definition = self.parameter_definition(&usb_mode_key).await?;
                ensure!(
                    !definition.read_only
                        && matches!(&definition.kind, native::Kind::Enum(choices) if choices.iter().any(|(name, _)| name == MODES[mode as usize])),
                    "USB return {} mode is unavailable on this firmware",
                    lane + 1
                );
                if let Some(previous) = mode_changes.insert(usb_mode_key.clone(), mode) {
                    ensure!(previous == mode, "Conflicting USB source mode requirements");
                }
            }
            auto_changes.push((format!("{prefix}/in/set/srcauto"), 0));
            source_dependencies.extend(reserved_guards.clone());
            output_mask[lane..lane + width].fill(true);
            preparation.push((
                format!("{prefix}/in/conn/{inactive}grp"),
                GROUPS.iter().position(|g| *g == "USB").unwrap() as i32,
            ));
            preparation.push((format!("{prefix}/in/conn/{inactive}in"), lane as i32 + 1));
            selection.push((format!("{prefix}/in/set/altsrc"), 1 - selected));
            for key in [
                format!("{prefix}/in/conn/{inactive}grp"),
                format!("{prefix}/in/conn/{inactive}in"),
                format!("{prefix}/in/set/altsrc"),
                format!("{prefix}/in/set/srcauto"),
            ] {
                dependencies.insert(key, source_dependencies.clone());
            }
        }
        ensure!(
            output_mask.iter().any(|used| *used),
            "No captured physical input is available for soundcheck"
        );
        // Shared USB source modes are prepared while no changed channel
        // selects them. Journal automatic-source selection too, so the desk
        // cannot silently switch a channel back while playback is active.
        let all_guards = inventory.expected.keys().cloned().collect::<Vec<_>>();
        for key in mode_changes.keys() {
            dependencies.insert(key.clone(), all_guards.clone());
        }
        let mut changes = auto_changes;
        changes.extend(mode_changes);
        changes.extend(preparation);
        changes.extend(selection);
        let output_mapping = output_mask
            .iter()
            .enumerate()
            .map(|(lane, used)| used.then_some(lane))
            .collect();
        Ok(SoundcheckPlan {
            inventory,
            changes,
            output_mask,
            output_mapping,
            dependencies,
        })
    }
    async fn soundcheck_recovery_state(&self) -> anyhow::Result<serde_json::Value> {
        // Recovery must accept the current USB/alternate state. Building a
        // new soundcheck plan here would reject the state we need to undo.
        let mut expected = BTreeMap::new();
        for config in ["/$globals/usbacfg", "/cfg/usbacfg"] {
            if let Ok(width) = self.soundcheck_value(config).await {
                expected.insert(config.into(), width);
            }
        }
        for lane in 1..=48 {
            for field in ["grp", "in"] {
                let key = format!("/io/out/USB/{lane}/{field}");
                expected.insert(key.clone(), self.soundcheck_value(&key).await?);
            }
        }
        for strip in 0..48 {
            let prefix = root(StripId(strip))?.0;
            for field in [
                "in/conn/grp",
                "in/conn/in",
                "in/conn/altgrp",
                "in/conn/altin",
                "in/set/altsrc",
                "in/set/$mode",
                "in/set/srcauto",
            ] {
                let key = format!("{prefix}/{field}");
                expected.insert(key.clone(), self.soundcheck_value(&key).await?);
            }
        }
        let mut mode_keys = std::collections::BTreeSet::new();
        for lane in 1..=48 {
            mode_keys.insert(format!("/io/in/USB/{lane}/mode"));
        }
        for strip in 0..48 {
            let prefix = root(StripId(strip))?.0;
            for target in ["", "alt"] {
                let group = group(expected[&format!("{prefix}/in/conn/{target}grp")])?;
                if physical(group) {
                    let index = expected[&format!("{prefix}/in/conn/{target}in")];
                    mode_keys.insert(format!("/io/in/{group}/{index}/mode"));
                }
            }
        }
        for key in mode_keys {
            expected.insert(key.clone(), self.soundcheck_value(&key).await?);
        }
        let guards = expected.keys().cloned().collect::<Vec<_>>();
        let dependencies = expected
            .keys()
            .filter(|key| writable(key))
            .map(|key| (key.clone(), guards.clone()))
            .collect::<BTreeMap<_, _>>();
        Ok(serde_json::json!({"expected": expected, "dependencies": dependencies}))
    }
    pub async fn soundcheck_read(
        &self,
        key: &str,
        state: &serde_json::Value,
        check: &(dyn Fn(&[StripId]) -> anyhow::Result<()> + Send + Sync),
    ) -> anyhow::Result<i32> {
        ensure!(valid_key(key), "Invalid WING soundcheck route");
        let members = (0..48).map(StripId).collect::<Vec<_>>();
        check(&members)?;
        if !state.is_null() {
            let expected = state["expected"]
                .as_object()
                .context("Missing WING soundcheck guards")?;
            ensure!(expected.contains_key(key), "Unjournaled WING routing key");
            ensure!(
                self.soundcheck_value(key).await? as i64
                    == expected[key]
                        .as_i64()
                        .context("Invalid WING routing guard")?,
                "WING routing changed: {key}"
            );
        }
        let value = self.soundcheck_value(key).await?;
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
            "Invalid or unguarded WING soundcheck write"
        );
        let recovered;
        let state = if state.is_null() {
            recovered = self.soundcheck_recovery_state().await?;
            &recovered
        } else {
            state
        };
        let arg = if key.ends_with("/mode") {
            let mode = MODES
                .get(usize::try_from(desired)?)
                .context("Invalid WING source mode")?;
            let definition = self.parameter_definition(key).await?;
            ensure!(
                !definition.read_only
                    && matches!(&definition.kind, native::Kind::Enum(choices) if choices.iter().any(|(name, _)| name == *mode)),
                "USB source mode unavailable on this firmware"
            );
            OscType::String((*mode).into())
        } else if key.ends_with("grp") {
            OscType::String(group(desired)?.into())
        } else {
            ensure!(
                if key.ends_with("altsrc") || key.ends_with("srcauto") {
                    (0..=1).contains(&desired)
                } else {
                    (1..=64).contains(&desired)
                },
                "WING routing value out of range"
            );
            OscType::Int(desired)
        };
        let session = self.session()?;
        let _guard = self.writes.lock().await;
        ensure!(
            self.soundcheck_read(key, state, check).await? == expected,
            "WING routing conflict"
        );
        let guards = state["expected"]
            .as_object()
            .context("Missing WING routing guards")?;
        let dependencies = state["dependencies"][key]
            .as_array()
            .context("Missing WING source dependencies")?;
        let guard_values = dependencies
            .iter()
            .map(|dependency| {
                let path = dependency
                    .as_str()
                    .context("Invalid WING source dependency")?;
                let value = guards
                    .get(path)
                    .and_then(serde_json::Value::as_i64)
                    .context("Missing WING source dependency value")?;
                Ok((path.to_string(), i32::try_from(value)?))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.soundcheck_check_values(&guard_values).await?;
        self.ensure_current(&session)?;
        check(&(0..48).map(StripId).collect::<Vec<_>>())?;
        session.send(key, vec![arg]).await?;
        ensure!(
            self.soundcheck_value(key).await? == desired,
            "WING routing readback mismatch"
        );
        self.ensure_current(&session)?;
        let mut next = state.clone();
        next["expected"][key] = desired.into();
        Ok(next)
    }
}
