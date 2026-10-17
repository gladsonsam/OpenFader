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
use openmix_transport::mackie::{MackieSession, VALUE_TYPE_PARAM};
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
}

impl MixerDriver for Dl32sDriver {
    fn name(&self) -> &'static str {
        "dl32s"
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

    async fn get_name(&self, strip: StripId) -> anyhow::Result<String> {
        let ch = Self::channel(strip)?;
        let guard = self.session().await?;
        guard
            .as_ref()
            .expect("checked above")
            .name(ch)
            .ok_or_else(|| anyhow::anyhow!("name: no value pushed yet"))
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
            ("name", _) => anyhow::bail!("dl32s: set_name not supported (rename write encoding unconfirmed)"),
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
            let pan = session.cached(pan_addr).map(f32::from_bits).unwrap_or(0.5);
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
}
