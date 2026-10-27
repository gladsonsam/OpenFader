//! Meter parsing, ring buffer, and broadcast hub shared by all drivers.
//!
//! X-Air level blobs (`/meters/0`..`/meters/3`, `/meters/5`, `/meters/7`
//! ..`/meters/9`, ~20/s) share one layout (rig-verified 2026-10-20, XR18
//! FW 1.20, all ten streams; corroborated by the Behringer X AIR OSC doc
//! "signed integer 16 bit, resolution 1/256 dB" and the protocol reference
//! mixer-simulator meter threads): a 4-byte LE i32 sample count followed by
//! that many LE i16 samples, each dBFS x 256 (signed, `<= 0`;
//! `i16::MIN` = -128 dB floor = silence). Levels decode to linear
//! 0.0..1.0 via `10^(dB/20)`. [`parse_xair_meter_blob`] owns this form and
//! matches ONLY on an exact count word (`4 + 2*count == len`); everything
//! else (fake/sim desks, float/int arg lists) keeps flowing through the
//! legacy plain LE-i16 run below untouched.
//!
//! Address forms: driver-subscribes get blobs addressed `/meters/N`, while
//! `/batchsubscribe` gets the SAME blobs addressed `meters/N`
//! (no leading slash, rig-verified 2026-10-19, XR18 FW 1.20; see
//! XR18 protocol notes from desk research §4). Both forms route
//! into the parse path.
//!
//! Blob census (rig lengths): m0:20B m1:84B m2:76B m3:116B m4:204B
//! m5:92B m6:82B m7:36B m8:12B m9:12B; the first 4 bytes are the LE i32
//! sample count on all ten (the old "first le16 = sample-count − 2, no
//! header" reading saw the count word's low half, corrected 2026-10-20).
//! Per-strip index mapping is claimed ONLY where live-verified (see the
//! xair driver's `poll_meters`); this parser makes no per-strip claims.
//!
//! Two consumers share this module: [`MeterRing`] keeps the latest N frames in
//! memory (sync polled UIs, tests); [`MeterHub`] broadcasts frames every
//! [`METER_INTERVAL`] over [`tokio::sync::broadcast`] (async push UIs;
//! lagging receivers skip, which is the desired ring-buffer behavior).
use openfader_core::MeterFrame;
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
    // Level streams only: `/meters/4` is RTA100 audio-frequency data;
    // `/meters/6` is the ALL DYN (GR) blob, signed
    // 1/256 dB samples behind a 4-byte size word, not levels. It must not
    // enter the level hub (it would decode as ~41 garbage levels and
    // clobber the level cache); GR arrives via `ingest_dyn_message`.
    // The desk-addressed no-slash `meters/6` form is excluded too.
    // RTA100 is a frequency spectrum, never a strip-level frame.
    if matches!(addr, "/meters/4" | "meters/4" | "/meters/6" | "meters/6") {
        return false;
    }
    addr == "/meters"
        || addr.starts_with("/meters/")
        // Desk-addressed meter blobs arrive WITHOUT the leading slash
        // (`meters/N`, rig-verified 2026-10-19, the `/batchsubscribe`
        // reply form). No-slash traffic has no other known source, and the
        // payload gate in `parse_meter_message` (blob/float args required)
        // keeps stray messages out anyway.
        || addr == "meters"
        || addr.starts_with("meters/")
}

fn sample_to_level(raw: i16) -> f32 {
    (raw as f32 / 32767.0).clamp(0.0, 1.0)
}

fn xair_sample_to_level(raw: i16) -> f32 {
    if raw == i16::MIN {
        return 0.0; // -128 dB floor sentinel = silence
    }
    let db = (raw as f32 / 256.0).min(0.0);
    10f32.powf(db / 20.0)
}

/// Decode an X-Air meter blob (LE i32 sample count + that many LE i16
/// dBFSx256 samples) to linear levels. Returns `None` unless the count
/// word EXACTLY describes the payload (`4 + 2*count == len`,
/// `1 <= count <= 8192`): fake/sim blobs and garbage keep the legacy
/// path, never this one.
pub fn parse_xair_meter_blob(blob: &[u8]) -> Option<Vec<f32>> {
    if blob.len() < 6 {
        return None;
    }
    let count = i32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]);
    if !(1..=8192).contains(&count) {
        return None;
    }
    if 4 + (count as usize) * 2 != blob.len() {
        return None;
    }
    let mut out = Vec::with_capacity(count as usize);
    for c in blob[4..].as_chunks::<2>().0 {
        out.push(xair_sample_to_level(i16::from_le_bytes([c[0], c[1]])));
    }
    Some(out)
}

pub fn parse_meter_blob(blob: &[u8]) -> Vec<f32> {
    blob.as_chunks::<2>().0.iter()
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
    // Blob form: X-Air count-word + dBFSx256 samples when the shape
    // matches exactly, else the legacy plain LE-i16 run (fake/sim desks).
    for arg in &msg.args {
        if let OscType::Blob(b) = arg {
            let levels = match parse_xair_meter_blob(b) {
                Some(levels) => levels,
                None => parse_meter_blob(b),
            };
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
        drop(rx);
        hub.push_lossy(MeterFrame::silent(2));
    }

    /// Odd-length and garbage blob payloads must parse-or-skip, never
    /// panic (truncated venue datagrams hit this path).
    #[test]
    fn odd_length_blob_never_panics() {
        for len in 0..16usize {
            let blob = vec![0xABu8; len];
            let _ = parse_meter_blob(&blob);
        }
        assert_eq!(parse_meter_blob(&[0xFF, 0x7F, 0x00]).len(), 1);
        assert!(parse_meter_blob(&[]).is_empty());
    }

    /// Garbage meter-shaped messages: no panic; documented `None` cases
    /// (non-meter addrs, empty payloads) stay `None`.
    #[test]
    fn garbage_meter_messages_never_panic() {
        let junk = OscMessage {
            addr: "/ch/01/mix/fader".into(),
            args: vec![OscType::Blob(vec![0xFFu8; 37])],
        };
        assert!(parse_meter_message(&junk).is_none());
        let bare = OscMessage {
            addr: "/meters/1".into(),
            args: vec![],
        };
        assert!(parse_meter_message(&bare).is_none());
        for blob in [vec![], vec![0x00], vec![0xFFu8; 37], vec![0xABu8; 100]] {
            let msg = OscMessage {
                addr: "/meters/1".into(),
                args: vec![OscType::Blob(blob)],
            };
            let _ = parse_meter_message(&msg);
        }
        let mixed = OscMessage {
            addr: "/meters/0".into(),
            args: vec![
                OscType::String("garbage".into()),
                OscType::Int(-999999),
                OscType::Blob(vec![1, 2, 3]),
            ],
        };
        let _ = parse_meter_message(&mixed);
        // Non-UTF8-adjacent addr bytes can't occur post-decode, but an
        // empty addr must not match the meter prefix.
        assert!(!is_meter_addr(""));
        assert!(!is_meter_addr("/meter"));
        assert!(is_meter_addr("/meters"));
    }

    /// Blob census from mixer control software protocol map §4 (lengths
    /// rig-verified 2026-10-19; FORMAT corrected 2026-10-20: the first 4
    /// bytes are a LE i32 sample count, samples are dBFSx256, the old
    /// "first le16 = count − 2, no header" reading saw the count word's
    /// low half). Census-shaped blobs take the X-Air path with exactly
    /// `count` levels; a −256 raw sample reads −1 dB; the floor sentinel
    /// reads silence; legacy/fake shapes must NOT match the X-Air form;
    /// both the slashed and the desk no-slash address forms route into
    /// the parser, except the DYN `/meters/6` blob, which must stay out
    /// of the level path in both forms.
    #[test]
    fn census_lengths_parse_count_word_form() {
        const CENSUS: [(u8, usize); 10] = [
            (0, 20),
            (1, 84),
            (2, 76),
            (3, 116),
            (4, 204),
            (5, 92),
            (6, 82),
            (7, 36),
            (8, 12),
            (9, 12),
        ];
        for (n, bytes) in CENSUS {
            let count = (bytes - 4) / 2;
            let mut blob = Vec::with_capacity(bytes);
            blob.extend_from_slice(&(count as i32).to_le_bytes());
            for i in 0..count {
                // −1 dB at slot 0, floor sentinel at slot 1, 0 dBFS else.
                let raw: i16 = if i == 0 {
                    -256
                } else if i == 1 {
                    i16::MIN
                } else {
                    0
                };
                blob.extend_from_slice(&raw.to_le_bytes());
            }
            let levels = parse_xair_meter_blob(&blob).expect("census shape");
            assert_eq!(levels.len(), count, "meters/{n}");
            assert!((levels[0] - 10f32.powf(-1.0 / 20.0)).abs() < 1e-6);
            assert_eq!(levels[1], 0.0, "floor sentinel is silence");
            for addr in [format!("/meters/{n}"), format!("meters/{n}")] {
                let msg = OscMessage {
                    addr,
                    args: vec![OscType::Blob(blob.clone())],
                };
                if n == 4 || n == 6 {
                    assert!(
                        parse_meter_message(&msg).is_none(),
                        "RTA/DYN must not enter the level hub"
                    );
                } else {
                    assert_eq!(
                        parse_meter_message(&msg).unwrap().levels.len(),
                        count
                    );
                }
            }
        }
        // Legacy/fake shapes must keep the legacy path (never match the
        // count word): the fake's 3-level blob, a silent pair, garbage.
        assert!(parse_xair_meter_blob(&build_meter_blob(&[0.1, 0.5, 0.9])).is_none());
        assert!(parse_xair_meter_blob(&build_meter_blob(&[0.0, 0.0])).is_none());
        assert!(parse_xair_meter_blob(&[0xABu8; 37]).is_none());
        assert!(parse_xair_meter_blob(&[0xFFu8; 100]).is_none());
    }

    #[test]
    fn no_slash_addr_matching() {
        assert!(is_meter_addr("meters"));
        assert!(is_meter_addr("meters/0"));
        assert!(is_meter_addr("meters/9"));
        assert!(!is_meter_addr("meters/6"));
        assert!(!is_meter_addr("meter"));
        assert!(!is_meter_addr(""));
        assert!(!is_meter_addr("ch/01/mix/fader"));
        assert!(!is_meter_addr("xremote"));
    }

    /// Broadcast lag: a subscriber that never reads must not kill the
    /// relay. The slow reader sees `Lagged` (skip-and-continue, what the
    /// serve relay matches on) while pushes keep succeeding and fresh
    /// subscribers still get live frames.
    #[test]
    fn slow_subscriber_lag_does_not_kill_relay() {
        let hub = MeterHub::with_capacity(2);
        let mut slow = hub.subscribe();
        for i in 0..10 {
            hub.push(MeterFrame {
                levels: vec![i as f32],
            })
            .unwrap();
        }
        match slow.try_recv() {
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            other => panic!("expected lagged slow reader, got {other:?}"),
        }
        // Relay still alive: pushes succeed, fresh subscriber gets frames.
        hub.push(MeterFrame { levels: vec![99.0] }).unwrap();
        let mut fresh = hub.subscribe();
        hub.push(MeterFrame {
            levels: vec![100.0],
        })
        .unwrap();
        assert_eq!(fresh.try_recv().unwrap().levels, vec![100.0]);
    }
}
