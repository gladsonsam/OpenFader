//! Input field validation, canonical mapping, verified reads and push decoding.
use crate::{addrs, Dl32sDriver, DL32S_CAPABILITIES};
use crate::assignments::{MEMBERSHIP_ALIASES, INPUT_SOLO_FIRST_ADDRESS};
use crate::preamp::selected_preamp;
use crate::addrs::{dl32s_inputs, INPUT_COUNT};
use openfader_core::{ParamUpdate, ParamValue, StripId, MixerDriver};
use openfader_transport::mackie::{PushEvent, MUTE_GROUP_BASE_ADDRESS, MUTE_GROUP_COUNT, VALUE_TYPE_PARAM, VALUE_TYPE_MUTE_GROUP};

/// Canonical id -> native table key for renamed leaves (`None` = same name).
/// Pure renames only — unit conversions (pan) and domain guards (modes)
/// stay as explicit arms in `set_param`/`get_param`/decode.
pub(super) fn alias_target(param: &str) -> Option<&'static str> {
    if let Some((_, native)) = MEMBERSHIP_ALIASES.iter().find(|(id, _)| *id == param) { return Some(*native); }
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

pub(super) fn canonical_for_table_key(key: &str) -> Option<&'static str> {
    if let Some((id, _)) = MEMBERSHIP_ALIASES.iter().find(|(_, native)| *native == key) { return Some(*id); }
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
pub(super) fn send_table_key(param: &str) -> Option<String> {
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

pub(super) fn decode_colour(raw: u32) -> f32 {
    if raw == u32::MAX { -1.0 } else { raw as f32 }
}

/// Decode one pushed raw into canonical `(strip, param, value)` triples,
/// mirroring `get_param` exactly (fader dB->level, pan x100, renames, send
/// mirrors). Only [`DL32S_CAPABILITIES`] ids are emitted — the push plane
/// equals the caps plane, so the engine never learns an id it didn't
/// advertise. Master/type-2/meter leaves have no strip plane and are
/// skipped (documented limitation: masters refresh on open + own writes).
pub(super) fn decode_push_value(addr: u32, raw: u32) -> Vec<(StripId, String, ParamValue)> {
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
        // Physical preamp addresses need routing-aware fanout, not ch=socket.
        "gain" | "phantom" => return out,
        "fader" => push("fader".to_string(), ParamValue::Float(addrs::DL32S_FADER_SCALE.inverse(f))),
        "mute" => push("mute".to_string(), ParamValue::Bool(raw != 0)),
        "pan" => push("pan".to_string(), ParamValue::Float(f * 100.0)),
        "stereo_link" => {
            if ch0.is_multiple_of(2) {
                return vec![(strip, "link".to_string(), ParamValue::Bool(raw != 0)),
                    (StripId(ch0 + 1), "link".to_string(), ParamValue::Bool(raw != 0))];
            }
            return out;
        }
        // Native None is u32::MAX, which cannot round-trip through f32.
        "colour" => push("color".to_string(), ParamValue::Float(decode_colour(raw))),
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

pub(super) fn send_slot_for_table_key(key: &str) -> Option<(u32, &'static str)> {
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
pub(super) fn decode_push(push: PushEvent) -> Vec<ParamUpdate> {
    let received_at = std::time::Instant::now();
    match push {
        PushEvent::Values {
            value_type,
            start,
            raws,
        } => {
            // Mute-group masters ride their own address space (type 2):
            // master for 1-based group g lives at
            // MUTE_GROUP_BASE_ADDRESS + g. The engine already routes
            // `_mute_group` ingest updates to MuteGroup events (same
            // strip convention as `get_mute_group`), so another client's
            // master change converges live instead of waiting for the
            // next masters refresh.
            if value_type == VALUE_TYPE_MUTE_GROUP {
                let mut out = Vec::new();
                for (i, raw) in raws.iter().enumerate() {
                    let address = start + i as u32;
                    let input0 = address.wrapping_sub(INPUT_SOLO_FIRST_ADDRESS);
                    if input0 < INPUT_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(input0 as u16),
                            param: "solo".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                    let group0 = address.wrapping_sub(MUTE_GROUP_BASE_ADDRESS + 1);
                    if group0 < MUTE_GROUP_COUNT {
                        out.push(ParamUpdate {
                            received_at,
                            strip: StripId(group0 as u16),
                            param: "_mute_group".to_string(),
                            value: ParamValue::Bool(*raw != 0),
                        });
                    }
                }
                return out;
            }
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

impl Dl32sDriver {
    pub(super) fn channel(strip: StripId) -> anyhow::Result<u32> {
        let ch = strip.0 as u32 + 1;
        if !(1..=INPUT_COUNT).contains(&ch) {
            anyhow::bail!("channel {} out of range 1..={INPUT_COUNT}", strip.0);
        }
        Ok(ch)
    }

    /// Write one table field, encoding the value per its wire type.
    /// Value-shape mistakes are reported before the connection is touched,
    /// so mapping bugs surface even offline.
    pub(super) async fn set_field(&self, strip: StripId, key: &str, value: ParamValue) -> anyhow::Result<()> {
        self.set_field_checked(strip, key, value, None).await
    }

    pub(super) async fn set_field_checked(&self, strip: StripId, key: &str, value: ParamValue, expected: Option<&std::collections::BTreeMap<String, ParamValue>>) -> anyhow::Result<()> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let channel = Self::channel(strip)?;
        enum Out {
            Int(u32),
            Float(f32),
        }
        let out = match (field.encoding, value) {
            (addrs::Encoding::Int, ParamValue::Float(v)) if key == "icon" => {
                anyhow::ensure!(v.is_finite() && v.fract() == 0.0 &&
                    (v == 0.0 || (1000.0..=1032.0).contains(&v)),
                    "icon must be Clear (0) or a stock symbol 1000..1032, got {v}");
                Out::Int(v as u32)
            }
            (addrs::Encoding::Int, ParamValue::Float(v)) if key == "colour" => {
                anyhow::ensure!(v.is_finite() && v.fract() == 0.0 && (-1.0..=16.0).contains(&v),
                    "colour must be None (-1) or an integer swatch 0..16, got {v}");
                Out::Int(if v == -1.0 { u32::MAX } else { v as u32 })
            }
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
        let route = if matches!(key, "gain" | "phantom") {
            Some(session.cached_range::<3>(table.address(channel, "source_a")?)
                .ok_or_else(|| anyhow::anyhow!("preamp routing context is unavailable"))?)
        } else { None };
        if let Some(expected) = expected {
            let route = route.ok_or_else(|| anyhow::anyhow!("guarded preamp control requires gain or phantom"))?;
            for (id, wanted) in expected {
                let i = match id.as_str() { "source_a" => 0, "source_b" => 1, "source_select" => 2, _ => anyhow::bail!("invalid preamp context field {id}") };
                anyhow::ensure!(*wanted == ParamValue::Float(route[i] as f32), "Input source changed; reopen the control.");
            }
        }
        let physical = if let Some([a,b,selected]) = route {
            let source = match selected { 1 => a, 0 => b, _ => anyhow::bail!("preamp source selection is unavailable") };
            anyhow::ensure!((1000..=1031).contains(&source), "preamp unavailable for selected input source");
            source - 999
        } else { channel };
        let addr = table.address(physical, key)?;

        match out {
            Out::Int(v) => session.write_int(addr, v, VALUE_TYPE_PARAM).await,
            Out::Float(v) => session.write_float(addr, v).await,
        }
    }

    /// Read one table field back from the desk-proven push cache.
    ///
    /// Uses [`cached_verified`](openfader_transport::mackie::MackieSession::cached_verified):
    /// own-write-pending words are withheld (no desk proof), so a post-write
    /// read reports "no desk-confirmed value yet" instead of echoing our own
    /// optimism back as confirmation. The error text stays out of the
    /// engine's drop-class substrings (`not supported`/`timed out`) so the
    /// poller keeps polling instead of evicting the param.
    pub(super) async fn get_field(&self, strip: StripId, key: &str) -> anyhow::Result<ParamValue> {
        self.get_field_mode(strip, key, true).await
    }

    pub(super) async fn get_field_mode(&self, strip: StripId, key: &str, verified: bool) -> anyhow::Result<ParamValue> {
        let table = dl32s_inputs();
        let field = table.field(key)?;
        let channel = Self::channel(strip)?;
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        let physical = if key == "stereo_link" {
            if !channel.is_multiple_of(2) { channel } else { channel - 1 }
        } else if matches!(key, "gain" | "phantom") { selected_preamp(session, channel)? } else { channel };
        let addr = table.address(physical, key)?;
        let raw = if verified { session.cached_verified(addr) } else { session.cached(addr) }
            .ok_or_else(|| anyhow::anyhow!("{key}: no desk-confirmed value yet"))?;
        if key == "colour" {
            Ok(ParamValue::Float(decode_colour(raw)))
        } else if field.encoding.is_float() {
            Ok(ParamValue::Float(f32::from_bits(raw)))
        } else if field.encoding == addrs::Encoding::Bool {
            Ok(ParamValue::Bool(raw != 0))
        } else {
            Ok(ParamValue::Float(raw as f32))
        }
    }

    /// Desk-proven fader read (verification path): withholds own-write-pending
    /// words. Display/health paths use the inclusive
    /// [`MixerDriver::get_fader`] instead (continuity, no proof claimed).
    pub(super) async fn verified_fader(&self, addr: u32) -> anyhow::Result<f32> {
        let guard = self.session().await?;
        let raw = guard
            .as_ref()
            .expect("checked above")
            .cached_verified(addr)
            .ok_or_else(|| anyhow::anyhow!("fader: no desk-confirmed value yet"))?;
        Ok(addrs::DL32S_FADER_SCALE.inverse(f32::from_bits(raw)))
    }

    /// Desk-proven mute read (verification path — see [`verified_fader`](Self::verified_fader)).
    pub(super) async fn verified_mute(&self, addr: u32) -> anyhow::Result<bool> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .cached_verified(addr)
            .map(|v| v != 0)
            .ok_or_else(|| anyhow::anyhow!("mute: no desk-confirmed value yet"))
    }

    /// Desk-proven name read (verification path — see [`verified_fader`](Self::verified_fader)).
    pub(super) async fn verified_name(&self, ch: u32) -> anyhow::Result<String> {
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name_slot_verified(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no desk-confirmed value yet"))
    }

    /// Desk-proven raw u32 read (verification path for assign membership).
    /// Display paths use [`request_value_u32`](Self::request_value_u32).
    pub(super) async fn verified_value_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        self.request_value_verified_u32(address).await
    }

    /// Display/snapshot continuity for routing. Includes an acknowledged
    /// own write; get_param remains desk-evidenced and withholds pending words.
    pub async fn input_source_display(&self, strip: StripId, target: &str) -> anyhow::Result<u32> {
        let leaf = match target { "a" => "source_a", "b" => "source_b", _ => anyhow::bail!("dl32s: choose routing target A or B") };
        let address = dl32s_inputs().address(Self::channel(strip)?, leaf)?;
        self.request_value_u32(address).await?.ok_or_else(|| anyhow::anyhow!("dl32s: source unavailable"))
    }

    pub async fn request_value_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        Ok(session.cached(address))
    }

    /// Desk-proven variant of [`request_value_u32`](Self::request_value_u32):
    /// withholds own-write-pending words (no desk proof).
    pub async fn request_value_verified_u32(&self, address: u32) -> anyhow::Result<Option<u32>> {
        let guard = self.session().await?;
        let session = guard.as_ref().expect("checked above");
        Ok(session.cached_verified(address))
    }
}
impl Dl32sDriver {
    pub(super) async fn input_set_param(
        &self,
        strip: StripId,
        param: &str,
        value: ParamValue,
    ) -> anyhow::Result<()> {
        match (param, value) {
            ("link", ParamValue::Bool(linked)) => self.set_stereo_link(strip, linked).await.map(|_| ()),
            (leaf @ ("source_a" | "source_b"), ParamValue::Float(v)) => {
                anyhow::ensure!(v.is_finite() && v.fract() == 0.0 &&
                    (v == 0.0 || v == 17000.0 || (1000.0..=1031.0).contains(&v) || (2000.0..=2031.0).contains(&v)),
                    "dl32s: source must be None, Oscillator, or a verified mic preamp/USB socket");
                self.set_field(strip, leaf, ParamValue::Float(v)).await
            }
            ("source_select", ParamValue::Float(v)) => {
                if v != 0.0 && v != 1.0 {
                    anyhow::bail!("dl32s: source_select must be 1 (A) or 0 (B), got {v}");
                }
                self.set_field(strip, "source_select", ParamValue::Float(v))
                    .await
            }
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
            // Gate attack uses the canonical alias path below (gate_attack
            // in the native table); modes retain their explicit domain guards.
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
            ("eq1_type" | "eq2_type" | "eq3_type" | "eq4_type", ParamValue::Float(v)) => {
                if !(0.0..=5.0).contains(&v) {
                    anyhow::bail!("dl32s: {param} must be 0..=5, got {v}");
                }
                self.set_field(strip, param, ParamValue::Float(v)).await
            }
            (key, ParamValue::Float(v)) => {
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

    /// Authoritative param read (verification/poll/refresh path): desk-proven
    /// only — own-write-pending words withheld. Deliberately NOT routed via
    /// the inclusive display getters above (continuity without proof); each
    /// arm reads the verified cache directly, including unit conversion.
    pub(super) async fn input_get_param(&self, strip: StripId, param: &str) -> anyhow::Result<ParamValue> {
        match param {
            "fader" => {
                let ch = Self::channel(strip)?;
                let addr = dl32s_inputs().address(ch, "fader")?;
                self.verified_fader(addr).await.map(ParamValue::Float)
            }
            "mute" => {
                let ch = Self::channel(strip)?;
                let addr = dl32s_inputs().address(ch, "mute")?;
                self.verified_mute(addr).await.map(ParamValue::Bool)
            }
            "name" => {
                let ch = Self::channel(strip)?;
                self.verified_name(ch).await.map(ParamValue::Text)
            }
            // Wire pan -1.0..+1.0 back out to canonical -100..+100.
            "pan" => match self.get_field(strip, "pan").await? {
                ParamValue::Float(v) => Ok(ParamValue::Float(v * 100.0)),
                other => Ok(other),
            },
            key => {
                // Renames, then send slots, then the raw table key.
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
}
