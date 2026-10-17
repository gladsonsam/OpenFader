//! Meter parsing, ring buffer, and broadcast hub shared by all drivers.
//!
//! X-Air streams `/meters/0`..`/meters/3` blobs (~50ms). Each blob is a run of
//! little-endian i16 samples normalized by 32767. The parser below also accepts
//! float/int arg lists so future providers reuse it without bespoke code.
//!
//! Two consumers share this module: [`MeterRing`] keeps the latest N frames in
//! memory (sync polled UIs, tests); [`MeterHub`] broadcasts frames every
//! [`METER_INTERVAL`] over [`tokio::sync::broadcast`] (async push UIs;
//! lagging receivers skip, which is the desired ring-buffer behavior).
use openmix_core::MeterFrame;
use rosc::{OscMessage, OscType};
use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::broadcast;

/// Prefix matching any meter stream address.
pub const METER_PREFIX: &str = "/meters";

/// Meter tick interval (50ms => 20 fps, cheap enough for canvas meters).
pub const METER_INTERVAL: Duration = Duration::from_millis(50);

/// Default broadcast capacity (frames retained for slow consumers).
pub const METER_CAPACITY: usize = 32;

pub fn is_meter_addr(addr: &str) -> bool {
    addr == "/meters" || addr.starts_with("/meters/")
}

/// Normalize one LE i16 sample to 0.0..1.0.
fn sample_to_level(raw: i16) -> f32 {
    (raw as f32 / 32767.0).clamp(0.0, 1.0)
}

/// Parse a blob payload (LE i16 run) into levels.
pub fn parse_meter_blob(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(2)
        .map(|c| sample_to_level(i16::from_le_bytes([c[0], c[1]])))
        .collect()
}

/// Build levels for tests / fake mixers.
pub fn build_meter_blob(levels: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(levels.len() * 2);
    for l in levels {
        let raw = (l.clamp(0.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&raw.to_le_bytes());
    }
    out
}

/// Parse any meter-style message into a frame. Returns None for non-meter addrs
/// or empty payloads.
pub fn parse_meter_message(msg: &OscMessage) -> Option<MeterFrame> {
    if !is_meter_addr(&msg.addr) {
        return None;
    }
    // Blob form (X-Air).
    for arg in &msg.args {
        if let OscType::Blob(b) = arg {
            let levels = parse_meter_blob(b);
            if !levels.is_empty() {
                return Some(MeterFrame { levels });
            }
        }
    }
    // Arg-list form (floats or ints) for future providers.
    if !msg.args.is_empty() {
        let levels: Vec<f32> = msg
            .args
            .iter()
            .map(|a| match a {
                OscType::Float(f) => f.clamp(0.0, 1.0),
                OscType::Double(d) => (*d as f32).clamp(0.0, 1.0),
                OscType::Int(i) => (*i as f32 / 32768.0).clamp(0.0, 1.0),
                _ => 0.0,
            })
            .collect();
        if !levels.is_empty() {
            return Some(MeterFrame { levels });
        }
    }
    None
}

/// Fixed-capacity ring of recent meter frames (UI reads latest, drops old).
pub struct MeterRing {
    buf: VecDeque<MeterFrame>,
    cap: usize,
}

impl MeterRing {
    pub fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap.max(1)),
            cap: cap.max(1),
        }
    }

    pub fn push(&mut self, frame: MeterFrame) {
        if self.buf.len() >= self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(frame);
    }

    pub fn latest(&self) -> Option<&MeterFrame> {
        self.buf.back()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// Broadcast hub for meter frames.
pub struct MeterHub {
    tx: broadcast::Sender<MeterFrame>,
}

impl MeterHub {
    pub fn new() -> Self {
        Self::with_capacity(METER_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(1));
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<MeterFrame> {
        self.tx.subscribe()
    }

    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Push one frame; returns receiver count. Never blocks: a full buffer
    /// drops the oldest frame (broadcast lag), which is fine for meters.
    /// Fails only when there are no receivers.
    pub fn push(&self, frame: MeterFrame) -> anyhow::Result<usize> {
        let n = self.tx.receiver_count();
        if n == 0 {
            anyhow::bail!("no meter subscribers");
        }
        // `send` fails only with zero receivers; checked above, but map
        // the error anyway instead of panicking.
        self.tx.send(frame).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(n)
    }

    /// Best-effort push: ok when nobody listens (headless use).
    pub fn push_lossy(&self, frame: MeterFrame) {
        let _ = self.tx.send(frame);
    }

    /// Spawn a 50ms ticker calling `generate` and pushing each frame.
    /// Returns the task handle; abort it to stop. Uses
    /// [`tokio::time::interval`] (first tick immediate).
    pub fn spawn_50ms<F>(&self, generate: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn() -> MeterFrame + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(METER_INTERVAL);
            loop {
                tick.tick().await;
                let frame = generate();
                let _ = tx.send(frame);
            }
        })
    }
}

impl Default for MeterHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_roundtrip() {
        let levels = vec![0.0, 0.5, 1.0, 0.25];
        let blob = build_meter_blob(&levels);
        let back = parse_meter_blob(&blob);
        assert_eq!(back.len(), 4);
        for (a, b) in levels.iter().zip(back.iter()) {
            assert!((a - b).abs() < 0.001);
        }
    }

    #[test]
    fn message_forms() {
        let blob_msg = OscMessage {
            addr: "/meters/1".into(),
            args: vec![OscType::Blob(build_meter_blob(&[0.5, 0.5]))],
        };
        assert_eq!(parse_meter_message(&blob_msg).unwrap().levels.len(), 2);
        let float_msg = OscMessage {
            addr: "/meters/0".into(),
            args: vec![OscType::Float(0.3), OscType::Float(0.7)],
        };
        assert_eq!(parse_meter_message(&float_msg).unwrap().levels.len(), 2);
        let other = OscMessage {
            addr: "/ch/01/mix/fader".into(),
            args: vec![OscType::Float(0.5)],
        };
        assert!(parse_meter_message(&other).is_none());
    }

    #[test]
    fn ring_evicts_oldest() {
        let mut ring = MeterRing::new(2);
        ring.push(MeterFrame { levels: vec![0.1] });
        ring.push(MeterFrame { levels: vec![0.2] });
        ring.push(MeterFrame { levels: vec![0.3] });
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.latest().unwrap().levels, vec![0.3]);
    }

    #[test]
    fn hub_push_needs_subscriber() {
        let hub = MeterHub::new();
        assert!(hub.push(MeterFrame::silent(2)).is_err());
        let mut rx = hub.subscribe();
        assert_eq!(hub.receiver_count(), 1);
        let n = hub.push(MeterFrame { levels: vec![0.5, 0.25] }).unwrap();
        assert_eq!(n, 1);
        let frame = rx.try_recv().unwrap();
        assert_eq!(frame.levels, vec![0.5, 0.25]);
        // Lossy push never errors, even with nobody listening.
        drop(rx);
        hub.push_lossy(MeterFrame::silent(2));
    }
}
