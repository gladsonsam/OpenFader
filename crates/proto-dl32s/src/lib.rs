//! Mackie DL32S driver: thin mapping over [`MackieSession`].
//!
//! This crate owns ONLY the DL32S address table ([`addrs`]) plus argument
//! mapping. All sockets, framing, keepalive, caching and the init handshake
//! live in `openmix-transport::mackie`. New providers add a crate shaped
//! exactly like this one: table + mapping, zero core changes.

pub mod addrs;

use addrs::{dl32s_inputs, INPUT_COUNT};
use openmix_core::{
    validate_fader, ChannelStrip, MixerDriver, MixerInfo, MixerSnapshot, ParamUpdate, ParamValue,
    StripId,
};
use openmix_transport::mackie::{
    MackieSession, PushEvent, MUTE_GROUP_COUNT, REQUEST_TIMEOUT, VALUE_TYPE_PARAM,
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tokio::sync::{broadcast, Mutex, MutexGuard};

/// Wave-1+2 canonical surface: every id below names a DL32S leaf whose
/// position was confirmed on hardware AND whose value domain was confirmed
/// live — by the read-only push-cache survey (varied values prove the desk
/// stores the domain) or by write+relay with net-zero restore 2026-10-20.
/// See `docs/dl32s-protocol.md` and the `verified` flags in [`addrs`].
///
/// Deliberately absent (reachable only by raw table key for experts):
/// - `icon` — position verified, but the live domain is ~1000+ (app icon
///   table ids), not the canonical 0..31. Unmapped until the icon table is
///   reversed (Wave-2 REA residue).
/// - `comp_mode`/`comp_knee`/`gate_mode`/`eq_mode` polarity labels assumed
///   modern=0/vintage=1, soft=0/hard=1 per Mackie convention — both states
///   relay live for each; labels may swap (out-of-domain writes are
///   rejected, not faked).
/// - `comp_ratio` — direct floats confirmed (1.68..20.2 observed); canonical
///   allows to 100, desk clamps above its max.
/// - `eqN_type` — 0-5 write+relay proven on all three bands; int->filter
///   label mapping follows Mackie convention pending an app cross-check
///   (values relay exact, labels may swap).
/// - `viewN`/`vcaN`/`mutegroupN`/`subN` membership — write-proven live, but
///   served through the engine group `Cmd`s, not strip caps.
/// - `source_a`/`source_b`/`source_select` — positions + int domains verified
///   (1000+/2000+/A-B select), no canonical equivalent; raw passthrough only.
/// - `sendN_tap`/`sendN_grpon`/`sendN_on` — no confirmed leaf (the desk is
///   silent; the UI collapses them, never faked).
/// - `hpf_slope` — reads constant 2, meaning inferred.
pub const DL32S_CAPABILITIES: &[&str] = &[
    "fader",
    "mute",
    "name",
    "pan",
    "gain",
    "phantom",
    "trim",
    "polarity",
    "hpf_on",
    "hpf_freq",
    "lr_assign",
    "link",
    "color",
    "gate_on",
    "gate_thr",
    "gate_att",
    "gate_mode",
    "gate_rel",
    "gate_range",
    "gate_hold",
    "comp_on",
    "comp_mode",
    "comp_thr",
    "comp_ratio",
    "comp_knee",
    "comp_att",
    "comp_rel",
    "comp_makeup",
    "eq_on",
    "eq_mode",
    "eq1_gain",
    "eq1_freq",
    "eq1_q",
    "eq2_gain",
    "eq2_freq",
    "eq2_q",
    "eq2_type",
    "eq3_gain",
    "eq3_freq",
    "eq3_q",
    "eq3_type",
    "eq4_gain",
    "eq4_freq",
    "eq4_q",
    "eq4_type",
    "aux1_level",
    "aux1_mute",
    "aux2_level",
    "aux2_mute",
    "aux3_level",
    "aux3_mute",
    "aux4_level",
    "aux4_mute",
    "aux5_level",
    "aux5_mute",
    "aux6_level",
    "aux6_mute",
    "aux7_level",
    "aux7_mute",
    "aux8_level",
    "aux8_mute",
    "fx1_level",
    "fx1_mute",
    "fx2_level",
    "fx2_mute",
    "fx3_level",
    "fx3_mute",
    "fx4_level",
    "fx4_mute",
    "send1_level",
    "send1_pan",
    "send2_level",
    "send2_pan",
    "send3_level",
    "send3_pan",
    "send4_level",
    "send4_pan",
    "send5_level",
    "send5_pan",
    "send6_level",
    "send6_pan",
    "send7_level",
    "send7_pan",
    "send8_level",
    "send8_pan",
    "send9_level",
    "send10_level",
    "send11_level",
    "send12_level",
];

/// Canonical id -> native table key for renamed leaves (`None` = same name).
/// Pure renames only — unit conversions (pan) and domain guards (modes)
/// stay as explicit arms in `set_param`/`get_param`/decode.
fn alias_target(param: &str) -> Option<&'static str> {
    Some(match param {
        "lr_assign" => "main_assign",
        "link" => "stereo_link",
        "color" => "colour",
        "gate_thr" => "gate_threshold",
        "gate_att" => "gate_attack",
        "gate_rel" => "gate_release",
        "comp_thr" => "comp_threshold",
        "comp_makeup" => "comp_gain",
        "comp_att" => "comp_attack",
        "comp_rel" => "comp_release",
        _ => return None,
    })
}

/// Reverse of [`alias_target`]: native table key -> canonical id.
fn canonical_for_table_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "main_assign" => "lr_assign",
        "stereo_link" => "link",
        "colour" => "color",
        "gate_threshold" => "gate_thr",
        "gate_attack" => "gate_att",
        "gate_release" => "gate_rel",
        "comp_threshold" => "comp_thr",
        "comp_gain" => "comp_makeup",
        "comp_attack" => "comp_att",
        "comp_release" => "comp_rel",
        _ => return None,
    })
}

/// Generic send slot -> native table key. `send1-8` ride the aux sends
/// (level + pan), `send9-12` ride the FX sends (level only — FX sends have
/// no pan third word). Only confirmed leaves map; tap/grpon/on fall through
/// to the honest unknown-param error.
fn send_table_key(param: &str) -> Option<String> {
    let (slot, leaf) = param.strip_prefix("send")?.split_once('_')?;
    let n: u32 = slot.parse().ok()?;
    let native = match (n, leaf) {
        (1..=8, "level") => format!("aux{n}_level"),
        (1..=8, "pan") => format!("aux{n}_pan"),
        (9..=12, "level") => format!("fx{}_level", n - 8),
        _ => return None,
    };
    Some(native)
}

/// Decode one pushed raw into canonical `(strip, param, value)` triples,
/// mirroring `get_param` exactly (fader dB->level, pan x100, renames, send
/// mirrors). Only [`DL32S_CAPABILITIES`] ids are emitted — the push plane
/// equals the caps plane, so the engine never learns an id it didn't
/// advertise. Master/type-2/meter leaves have no strip plane and are
/// skipped (documented limitation: masters refresh on open + own writes).
fn decode_push_value(addr: u32, raw: u32) -> Vec<(StripId, String, ParamValue)> {
    use addrs::{INPUT_BASE, INPUT_STRIDE};
    if !(INPUT_BASE..INPUT_BASE + INPUT_COUNT * INPUT_STRIDE).contains(&addr) {
        return Vec::new();
    }
    let ch0 = ((addr - INPUT_BASE) / INPUT_STRIDE) as u16;
    let off = (addr - INPUT_BASE) % INPUT_STRIDE;
    let table = dl32s_inputs();
    let Some(field) = table.fields.iter().find(|f| f.offset == off) else {
        return Vec::new();
    };
    let strip = StripId(ch0);
    let f = f32::from_bits(raw);
    let mut out: Vec<(StripId, String, ParamValue)> = Vec::new();
    let mut push = |param: String, value: ParamValue| {
        out.push((strip, param, value));
    };
    match field.key {
        "fader" => push("fader".to_string(), ParamValue::Float(addrs::DL32S_FADER_SCALE.inverse(f))),
        "mute" => push("mute".to_string(), ParamValue::Bool(raw != 0)),
        "pan" => push("pan".to_string(), ParamValue::Float(f * 100.0)),
        key => {
            // Pure renames ride the canonical id.
            if let Some(canonical) = canonical_for_table_key(key) {
                let v = match field.encoding {
                    addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                    _ => ParamValue::Float(f),
                };
                push(canonical.to_string(), v);
                return out;
            }
            // Legacy + FX send leaves ride twice (native id and generic
            // slot), exactly like the poller reads them. Send pans convert
            // wire -1.0..+1.0 to canonical -100..+100.
            if let Some((slot, leaf)) = send_slot_for_table_key(key) {
                let val = if leaf == "pan" { f * 100.0 } else { f };
                push(format!("send{slot}_{leaf}"), ParamValue::Float(val));
            }
            if !DL32S_CAPABILITIES.contains(&key) {
                return out;
            }
            let value = match field.encoding {
                addrs::Encoding::Bool => ParamValue::Bool(raw != 0),
                addrs::Encoding::Db | addrs::Encoding::Float => ParamValue::Float(f),
                addrs::Encoding::Enum | addrs::Encoding::Int => {
                    ParamValue::Float(raw as f32)
                }
            };
            push(key.to_string(), value);
        }
    }
    out
}

/// Reverse of [`send_table_key`]: native send leaf -> (generic 1-based
/// slot, leaf name).
fn send_slot_for_table_key(key: &str) -> Option<(u32, &'static str)> {
    if let Some(rest) = key.strip_prefix("aux") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=8).contains(&n) {
            return None;
        }
        return match leaf {
            "level" => Some((n, "level")),
            "pan" => Some((n, "pan")),
            _ => None,
        };
    }
    if let Some(rest) = key.strip_prefix("fx") {
        let (n, leaf) = rest.split_once('_')?;
        let n: u32 = n.parse().ok()?;
        if !(1..=4).contains(&n) || leaf != "level" {
            return None;
        }
        return Some((n + 8, "level"));
    }
    None
}

/// Decode one transport push into engine `ParamUpdate`s (engineering units,
/// same as `get_param`). Type-5 strip leaves fan out per [`decode_push_value`];
/// name slots 1-32 become `name` updates; snapshot relays are recorded in
/// the session cache only (the poller converges the wholesale change).
fn decode_push(push: PushEvent) -> Vec<ParamUpdate> {
    let received_at = std::time::Instant::now();
    match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } => {
            if value_type != VALUE_TYPE_PARAM {
                return Vec::new();
            }
            let mut out = Vec::new();
            for (i, raw) in raws.iter().enumerate() {
                for (strip, param, value) in decode_push_value(start + i as u32, *raw) {
                    out.push(ParamUpdate { received_at, strip, param, value });
                }
            }
            out
        }
        PushEvent::Names { pairs } => pairs
            .into_iter()
            .filter(|(slot, _)| (1..=INPUT_COUNT).contains(slot))
            .map(|(slot, name)| ParamUpdate {
                received_at,
                strip: StripId((slot - 1) as u16),
                param: "name".to_string(),
                value: ParamValue::Text(name),
            })
            .collect(),
        PushEvent::SnapshotRecall(_) | PushEvent::SnapshotRename { .. } => Vec::new(),
    }
}

/// Live DL32S-class mixer on TCP 50001.
pub struct Dl32sDriver {
    session: Mutex<Option<MackieSession>>,
    addr: Mutex<Option<String>>,
    update_tx: broadcast::Sender<ParamUpdate>,
    generation: Arc<AtomicU64>,
}

impl Default for Dl32sDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Dl32sDriver {
    pub fn new() -> Self {
        let (update_tx, _) = broadcast::channel(256);
        Self {
            session: Mutex::new(None),
            addr: Mutex::new(None),
            update_tx,
            generation: Arc::new(AtomicU64::new(0)),
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

    /// DL32S strip map: 32 input channels only (honest — output-strip
    /// leaves are served through the bus/subgroup/mute-group/main
    /// methods below, not as pollable strips, so no extra topology is
    /// advertised).
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

    /// Wave-1+2 verified surface: every id names a confirmed DL32S leaf
    /// (see [`DL32S_CAPABILITIES`]). The poller reads exactly this set.
    fn capabilities(&self) -> &'static [&'static str] {
        DL32S_CAPABILITIES
    }

    /// Provider rows for confirmed leaves with no canonical equivalent:
    /// `eq_mode` (write-proven 0/1, modern/vintage per Mackie convention).
    /// Canonical ids win on collision, so a future canonical `eq_mode`
    /// upgrades this row seamlessly.
    fn param_defs(&self) -> &'static [openmix_core::params::ParamDef] {
        static DEFS: std::sync::OnceLock<[openmix_core::params::ParamDef; 1]> =
            std::sync::OnceLock::new();
        DEFS.get_or_init(|| {
            [openmix_core::params::ParamDef {
                id: "eq_mode",
                kind: openmix_core::params::ParamKind::Enum,
                scope: openmix_core::params::ParamScope::Channel,
                scopes: openmix_core::params::SCOPES_EQ,
                min: 0.0,
                max: 1.0,
                unit: "",
                default: openmix_core::params::ParamDefault::Float(0.0),
                label: "EQ modern/vintage",
            }]
        })
    }

    async fn connect(&self, addr: &str) -> anyhow::Result<()> {
        addrs::model_status("dl32s")?;
        let session = MackieSession::connect(addr).await?;
        // Desk-broadcast ingest: decode every pushed frame into the stable
        // update channel (one task per connection; it exits when the next
        // connect bumps the generation or the session drops). Mirrors the
        // X-Air session->driver forward path.
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut rx = session.subscribe_pushes();
        let tx = self.update_tx.clone();
        let gen = self.generation.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(push) => {
                        if gen.load(Ordering::Acquire) != generation {
                            break;
                        }
                        for update in decode_push(push) {
                            let _ = tx.send(update);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        *self.session.lock().await = Some(session);
        *self.addr.lock().await = Some(addr.to_string());
        Ok(())
    }

    /// Live desk-originated strip updates (another client's moves arrive
    /// without a poll sweep). Values are engineering units, same as
    /// `get_param`; the push plane equals the caps plane.
    fn subscribe_updates(&self) -> Option<broadcast::Receiver<ParamUpdate>> {
        Some(self.update_tx.subscribe())
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
            // Canonical pan is -100..+100; the wire leaf is -1.0..+1.0.
            ("pan", ParamValue::Float(v)) => {
                self.set_field(strip, "pan", ParamValue::Float(v / 100.0)).await
            }
            // Binary-confirmed modes (live survey 2026-10-20 saw only 0/1
            // across 32 channels): modern=0/vintage=1 per Mackie convention
            // (polarity assumed — the desk accepts both, labels may swap).
            // Anything else is rejected before it reaches the wire.
            ("comp_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: comp_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "comp_mode", ParamValue::Float(v)).await
            }
            ("comp_knee", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: comp_knee must be 0 (soft) or 1 (hard), got {v}");
                }
                self.set_field(strip, "comp_knee", ParamValue::Float(v)).await
            }
            // Write-proven 2026-10-20 (live relay, net-zero restored):
            // gate attack, gate modern/vintage, EQ modern/vintage.
            ("gate_att", ParamValue::Float(v)) => {
                self.set_field(strip, "gate_att", ParamValue::Float(v)).await
            }
            ("gate_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: gate_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "gate_mode", ParamValue::Float(v)).await
            }
            ("eq_mode", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: eq_mode must be 0 or 1, got {v}");
                }
                self.set_field(strip, "eq_mode", ParamValue::Float(v)).await
            }
            ("eq2_type" | "eq3_type" | "eq4_type", ParamValue::Float(v)) => {
                if !(0.0..=5.0).contains(&v) {
                    anyhow::bail!("dl32s: {param} must be 0..=5, got {v}");
                }
                self.set_field(strip, param, ParamValue::Float(v)).await
            }
            (key, ParamValue::Float(v)) => {
                // Pure renames ride the table key directly.
                if let Some(alias) = alias_target(key) {
                    return self.set_field(strip, alias, ParamValue::Float(v)).await;
                }
                // Generic send slots ride the aux/FX leaves; send pans are
                // canonical -100..+100 vs wire -1.0..+1.0.
                if let Some(mapped) = send_table_key(key) {
                    let w = if mapped.ends_with("_pan") { v / 100.0 } else { v };
                    return self.set_field(strip, &mapped, ParamValue::Float(w)).await;
                }
                self.set_field(strip, key, ParamValue::Float(v)).await
            }
            (key, v) => {
                // Pure renames ride the table key directly.
                if let Some(alias) = alias_target(key) {
                    return self.set_field(strip, alias, v).await;
                }
                // Generic send slots ride the aux/FX leaves; anything else
                // falls through to the raw table key (experts only).
                let owned;
                let table_key = match send_table_key(key) {
                    Some(mapped) => {
                        owned = mapped;
                        &owned
                    }
                    None => key,
                };
                self.set_field(strip, table_key, v).await
            }
        }
    }

    async fn get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => self.get_fader(strip).await.map(ParamValue::Float),
            "mute" => self.get_mute(strip).await.map(ParamValue::Bool),
            "name" => self.get_name(strip).await.map(ParamValue::Text),
            // Wire pan -1.0..+1.0 back out to canonical -100..+100.
            "pan" => match self.get_field(strip, "pan").await? {
                ParamValue::Float(v) => Ok(ParamValue::Float(v * 100.0)),
                other => Ok(other),
            },
            key => {
                // Pure renames (link/lr_assign/color/gate_thr/…), then generic send slots (with pan
                // unit conversion), then the raw table key.
                if let Some(alias) = alias_target(key) {
                    return self.get_field(strip, alias).await;
                }
                if let Some(mapped) = send_table_key(key) {
                    let pv = self.get_field(strip, &mapped).await?;
                    if mapped.ends_with("_pan") {
                        return match pv {
                            ParamValue::Float(v) => Ok(ParamValue::Float(v * 100.0)),
                            other => Ok(other),
                        };
                    }
                    return Ok(pv);
                }
                self.get_field(strip, key).await
            }
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
            let pan = session
                .cached(pan_addr)
                .map(|raw| f32::from_bits(raw) * 100.0)
                .unwrap_or(0.0);
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
    fn capabilities_are_wave1_verified_set() {
        // Wave 1+2: every advertised id names a confirmed DL32S leaf.
        // `fader`/`mute`/`name` ride the native methods; the rest map to
        // table keys in `set_param`/`get_param` (renames + pan units +
        // send slots + binary-guarded modes). Unadvertised on purpose, see
        // the `DL32S_CAPABILITIES` doc comment.
        let caps = Dl32sDriver::new().capabilities();
        for id in [
            "fader", "mute", "name", "pan", "gain", "phantom", "trim", "polarity",
            "hpf_on", "hpf_freq", "lr_assign", "link", "color",
            "gate_on", "gate_thr", "gate_att", "gate_mode", "gate_rel",
            "gate_range", "gate_hold",
            "comp_on", "comp_mode", "comp_thr", "comp_ratio", "comp_knee",
            "comp_att", "comp_rel", "comp_makeup", "eq_on", "eq_mode",
            "eq1_gain", "eq1_freq", "eq1_q",
            "eq2_gain", "eq2_freq", "eq2_q", "eq2_type",
            "eq3_gain", "eq3_freq", "eq3_q", "eq3_type",
            "eq4_gain", "eq4_freq", "eq4_q", "eq4_type",
            "aux1_level", "aux1_mute", "aux8_level", "aux8_mute",
            "fx1_level", "fx1_mute", "fx4_level", "fx4_mute",
            "send1_level", "send1_pan", "send8_level", "send8_pan",
            "send9_level", "send12_level",
        ] {
            assert!(caps.contains(&id), "caps miss {id}");
        }
        for id in [
            "icon", // live domain is ~1000+, unmapped
            "eq1_type", // band 1 has no type word
            "comp_auto", "comp_det", "comp_env", "comp_mix", // no leaf
            "gate_key_src", "gate_filter_on", "comp_key_src", // no leaf
            "eq_bands", "hpf_slope",
            "source_select", "aux1_pan", "send1_tap", "send1_grpon",
            "send1_on", "send9_pan", "send13_level", "send13_pan", "solo",
        ] {
            assert!(!caps.contains(&id), "caps must not advertise {id}");
        }
        // Every advertised id resolves in the canonical catalogue or the
        // driver's provider rows, so the engine renders a params row for it.
        let defs = Dl32sDriver::new().param_defs();
        for id in caps {
            let known = openmix_core::params::find(id).is_some()
                || defs.iter().any(|d| d.id == *id);
            assert!(known, "caps id {id} resolves nowhere");
        }
    }

    #[test]
    fn send_slot_targets() {
        assert_eq!(send_table_key("send1_level").as_deref(), Some("aux1_level"));
        assert_eq!(send_table_key("send1_pan").as_deref(), Some("aux1_pan"));
        assert_eq!(send_table_key("send8_level").as_deref(), Some("aux8_level"));
        assert_eq!(send_table_key("send8_pan").as_deref(), Some("aux8_pan"));
        assert_eq!(send_table_key("send9_level").as_deref(), Some("fx1_level"));
        assert_eq!(send_table_key("send9_pan"), None);
        assert_eq!(send_table_key("send12_level").as_deref(), Some("fx4_level"));
        assert_eq!(send_table_key("send13_level"), None);
        assert_eq!(send_table_key("send0_level"), None);
        assert_eq!(send_table_key("send1_tap"), None);
        assert_eq!(send_table_key("send1_grpon"), None);
        assert_eq!(send_table_key("send1_on"), None);
        assert_eq!(send_table_key("aux1_level"), None);
        assert_eq!(send_table_key("pan"), None);
    }

    #[test]
    fn send_slot_reverse_targets() {
        assert_eq!(send_slot_for_table_key("aux1_level"), Some((1, "level")));
        assert_eq!(send_slot_for_table_key("aux1_pan"), Some((1, "pan")));
        assert_eq!(send_slot_for_table_key("aux8_level"), Some((8, "level")));
        assert_eq!(send_slot_for_table_key("aux8_pan"), Some((8, "pan")));
        assert_eq!(send_slot_for_table_key("fx1_level"), Some((9, "level")));
        assert_eq!(send_slot_for_table_key("fx4_level"), Some((12, "level")));
        assert_eq!(send_slot_for_table_key("fx1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux1_mute"), None);
        assert_eq!(send_slot_for_table_key("aux9_level"), None);
        assert_eq!(send_slot_for_table_key("fx5_level"), None);
        assert_eq!(send_slot_for_table_key("pan"), None);
    }

    #[tokio::test]
    async fn unknown_send_slots_error_offline() {
        let d = Dl32sDriver::new();
        // Mapping/slot resolution happens before the connection check, so
        // these surface even offline.
        for id in ["send13_level", "send1_tap", "send9_pan", "nope"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(1.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
            let err = d
                .get_param(StripId(0), id)
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("unknown"), "{id}: {err}");
        }
    }

    #[tokio::test]
    async fn wrong_value_kind_errors_offline() {
        let d = Dl32sDriver::new();
        // Canonical pan takes a float; a bool must fail with an encoding
        // error, not a connection error.
        let err = d
            .set_param(StripId(0), "pan", ParamValue::Bool(true))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("Float"), "{err}");
    }

    #[tokio::test]
    async fn out_of_domain_modes_rejected_offline() {
        let d = Dl32sDriver::new();
        // Binary-confirmed modes: only 0/1 reach the wire.
        for id in ["gate_mode", "comp_mode", "comp_knee", "eq_mode"] {
            let err = d
                .set_param(StripId(0), id, ParamValue::Float(2.0))
                .await
                .unwrap_err()
                .to_string();
            assert!(err.contains("must be 0"), "{id}: {err}");
        }
        // EQ types: 0..=5 proven live.
        let err = d
            .set_param(StripId(0), "eq2_type", ParamValue::Float(6.0))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("0..=5"), "{err}");
    }

    #[test]
    fn decode_push_value_mirrors_get_param() {
        // ch1 fader (addr 9) -15 dB -> canonical level 0.5.
        let got = decode_push_value(9, (-15.0f32).to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, StripId(0));
        assert_eq!(got[0].1, "fader");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
        // ch1 mute (addr 8) = 1.
        let got = decode_push_value(8, 1);
        assert_eq!(
            got,
            vec![(StripId(0), "mute".to_string(), ParamValue::Bool(true))]
        );
        // ch1 pan (addr 10) 0.5 wire -> +50 canonical.
        let got = decode_push_value(10, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Renames ride canonical ids.
        // ch11 (strip 10) main_assign = 1+10*106+10 = 1071.
        let got = decode_push_value(1071, 1);
        assert_eq!(
            got,
            vec![(StripId(10), "lr_assign".to_string(), ParamValue::Bool(true))]
        );
        let got = decode_push_value(1 + 3 * 106 + 27, (-20.0f32).to_bits());
        assert_eq!(got[0].1, "comp_thr");
        // Aux send levels ride twice (native + generic slot).
        let got = decode_push_value(51, (-10.0f32).to_bits());
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|(_, p, _)| p == "aux1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send1_level"));
        let got = decode_push_value(75, (-10.0f32).to_bits());
        assert!(got.iter().any(|(_, p, _)| p == "fx1_level"));
        assert!(got.iter().any(|(_, p, _)| p == "send9_level"));
        // Aux pan mirrors to the generic send slot with x100 units.
        let got = decode_push_value(53, 0.5f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "send1_pan");
        assert!(matches!(got[0].2, ParamValue::Float(v) if (v - 50.0).abs() < 1e-6));
        // Write-proven leaves decode straight through.
        let got = decode_push_value(21, 50.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "gate_att");
        let got = decode_push_value(47, 2.0f32.to_bits());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1, "eq2_type");
        // Unadvertised + out-of-range leaves decode to nothing.
        assert!(decode_push_value(1 + 4, 1005).is_empty()); // icon
        assert!(decode_push_value(4041, 1).is_empty()); // subgroup master: no strip
        assert!(decode_push_value(4347, 0).is_empty()); // main: no strip
        assert!(decode_push_value(99999, 0).is_empty()); // unknown space
    }

    #[test]
    fn decode_push_names_and_snapshot() {
        use openmix_transport::mackie::PushEvent;
        let got = decode_push(PushEvent::Names {
            pairs: vec![(1, "Vox".to_string()), (33, "Aux".to_string())],
        });
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].strip, StripId(0));
        assert_eq!(got[0].param, "name");
        assert_eq!(got[0].value, ParamValue::Text("Vox".to_string()));
        // Snapshot relays decode to nothing (session cache records them;
        // the poller converges the wholesale change).
        assert!(decode_push(PushEvent::SnapshotRecall(7)).is_empty());
        assert!(decode_push(PushEvent::SnapshotRename {
            slot: 2,
            name: "Main".to_string()
        })
        .is_empty());
        // Non-param value types (meters, mute-group masters) are skipped.
        let got = decode_push(PushEvent::Values {
            value_type: 1,
            start: 114,
            raws: vec![1],
        });
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn broadcast_push_arrives_as_param_update_loopback() {
        use openmix_transport::mackie::{
            decode_one, word_swap, Command, Message, MessageType, RawValue,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn read_msg(
            stream: &mut tokio::net::TcpStream,
        ) -> anyhow::Result<Message> {
            let mut prefix = [0u8; 8];
            stream.read_exact(&mut prefix).await?;
            let chunks = u16::from_be_bytes([prefix[2], prefix[3]]) as usize;
            let mut rest = vec![0u8; chunks * 4 + if chunks == 0 { 0 } else { 4 }];
            if !rest.is_empty() {
                stream.read_exact(&mut rest).await?;
            }
            let mut buf = prefix.to_vec();
            buf.extend_from_slice(&rest);
            let (msg, _) =
                decode_one(&buf)?.ok_or_else(|| anyhow::anyhow!("truncated"))?;
            Ok(msg)
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fake = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut seq: u8 = 40;
            let mut cic = 0;
            let mut fired = false;
            loop {
                let req = match read_msg(&mut stream).await {
                    Ok(m) => m,
                    Err(_) => break,
                };
                let resp = Message::new(req.seq, MessageType::Response, req.command, vec![])
                    .unwrap();
                if stream.write_all(&resp.encode()).await.is_err() {
                    break;
                }
                if req.command == Command::ChannelInfoControl {
                    cic += 1;
                }
                if fired || cic < 2 {
                    continue;
                }
                fired = true;
                // Another client moved ch1 fader to -15 dB (addr 9).
                let body = openmix_transport::mackie::build_single_set(
                    9,
                    RawValue::Float(-15.0),
                    5,
                );
                seq = seq.wrapping_add(1);
                let push = Message::new(seq, MessageType::Broadcast, Command::ChannelValues, body)
                    .unwrap();
                stream.write_all(&push.encode()).await.unwrap();
                // ... and renamed slot 1 to "Vox".
                let mut padded = b"Vox\x00".to_vec();
                while !padded.len().is_multiple_of(4) {
                    padded.push(0);
                }
                let mut nbody = vec![];
                nbody.extend_from_slice(&1u32.to_be_bytes());
                nbody.extend_from_slice(&0x00010000u32.to_be_bytes());
                nbody.extend_from_slice(&word_swap(&padded));
                seq = seq.wrapping_add(1);
                let names = Message::new(seq, MessageType::Request, Command::ChannelNames, nbody)
                    .unwrap();
                stream.write_all(&names.encode()).await.unwrap();
            }
        });

        let d = Dl32sDriver::new();
        d.connect(&addr.to_string()).await.unwrap();
        let mut rx = d.subscribe_updates().expect("driver has update channel");
        let mut got_fader = false;
        let mut got_name = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while !(got_fader && got_name) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "push updates missing: fader={got_fader} name={got_name}"
            );
            let u = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
                .await
                .expect("update timeout")
                .expect("update channel closed");
            if u.strip == StripId(0) && u.param == "fader" {
                assert!(matches!(u.value, ParamValue::Float(v) if (v - 0.5).abs() < 1e-6));
                got_fader = true;
            }
            if u.strip == StripId(0) && u.param == "name" {
                assert_eq!(u.value, ParamValue::Text("Vox".to_string()));
                got_name = true;
            }
        }
        // Cache converged too: get_param reads the pushed values.
        assert!(matches!(
            d.get_param(StripId(0), "fader").await.unwrap(),
            ParamValue::Float(v) if (v - 0.5).abs() < 1e-6
        ));
        assert_eq!(
            d.get_param(StripId(0), "name").await.unwrap(),
            ParamValue::Text("Vox".to_string())
        );
        fake.abort();
    }
}
