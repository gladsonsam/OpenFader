//! MR1 realtime tests (engine side):
//! write-priority latency under saturation, `hello` negotiation, and the
//! binary meter encoding (+ golden fixtures shared with the UI decoder test).

use std::sync::Arc;
use std::time::{Duration, Instant};

use openmix_engine::protocol::{
    db_to_u8, level_to_u16, u8_to_db, u16_to_level, BIN_HEADER_LEN, BIN_TYPE_GR,
    BIN_TYPE_METERS, BIN_VERSION, GR_DB_MAX,
};
use openmix_engine::{Cmd, Engine, EngineConfig, Event, SharedEvent};

fn golden_bin_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden_bin")
}

async fn new_engine() -> Arc<Engine> {
    let engine = Engine::new(EngineConfig::default());
    engine.connect("sim", None).await.unwrap();
    engine
}

/// Drain the 6-frame attach snapshot (`you`, `state`, `shows`, `clients`,
/// `locks`, `caps`).
async fn drain_snapshot(
    handle: &mut openmix_engine::engine::ClientHandle,
) -> Vec<SharedEvent> {
    let mut out = Vec::new();
    for _ in 0..6 {
        out.push(
            tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
                .await
                .expect("snapshot in time")
                .expect("mailbox open"),
        );
    }
    out
}

async fn wait_for(
    handle: &mut openmix_engine::engine::ClientHandle,
    want: impl Fn(&Event) -> bool,
) -> SharedEvent {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() > deadline {
            panic!("timed out waiting for matching event");
        }
        let next =
            tokio::time::timeout(Duration::from_millis(500), handle.events.recv()).await;
        match next {
            Ok(Some(e)) if want(&e) => return e,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn hello_negotiates_binary_meters() {
    let engine = new_engine().await;
    let mut a = engine.attach(None).await;
    drain_snapshot(&mut a).await;
    assert!(
        !engine.client_binary_meters(&a.id),
        "binary defaults off (JSON for old clients)"
    );

    // Opt in: unicast ack carries the new preference, flag flips.
    a.send_json(r#"{"cmd":"hello","binary_meters":true}"#)
        .await
        .unwrap();
    let ack = wait_for(&mut a, |e| matches!(e, Event::Hello { .. })).await;
    assert_eq!(
        ack.to_json(),
        r#"{"type":"hello","binary_meters":true}"#,
        "hello ack is byte-exact JSON"
    );
    assert!(engine.client_binary_meters(&a.id));

    // Bare hello re-acks the current preference without changing it.
    a.send_json(r#"{"cmd":"hello"}"#).await.unwrap();
    let ack = wait_for(&mut a, |e| matches!(e, Event::Hello { .. })).await;
    assert_eq!(
        ack.to_json(),
        r#"{"type":"hello","binary_meters":true}"#,
    );
    assert!(engine.client_binary_meters(&a.id));

    // Opt out again.
    a.send_json(r#"{"cmd":"hello","binary_meters":false}"#)
        .await
        .unwrap();
    let ack = wait_for(&mut a, |e| matches!(e, Event::Hello { .. })).await;
    assert_eq!(
        ack.to_json(),
        r#"{"type":"hello","binary_meters":false}"#,
    );
    assert!(!engine.client_binary_meters(&a.id));

    // Unknown clients read false; bad shapes stay unicast errors.
    assert!(!engine.client_binary_meters("client-999"));
    a.send_json(r#"{"cmd":"hello","binary_meters":"yes"}"#)
        .await
        .unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(err.to_json().contains("invalid command"));

    // `stats` is adapter-level diagnostics (serve answers it from its own
    // per-connection counters); the engine reports it as unknown.
    a.send_json(r#"{"cmd":"stats"}"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        err.to_json().contains("unknown cmd") && err.to_json().contains("stats"),
        "engine leaves stats to the adapter: {}",
        err.to_json()
    );
}

#[test]
fn binary_quantization_roundtrips() {
    // Meters: linear 0..1 <-> u16, worst-case error half an LSB.
    for v in [0.0f32, 0.25, 0.5, 0.75, 1.0, 0.123456] {
        let back = u16_to_level(level_to_u16(v));
        assert!(
            (back - v).abs() <= 1.0 / 65535.0,
            "level {v} -> {} -> {back}",
            level_to_u16(v)
        );
    }
    assert_eq!(level_to_u16(-1.0), 0, "clamps below");
    assert_eq!(level_to_u16(2.0), 65535, "clamps above");
    assert_eq!(u16_to_level(0), 0.0);
    assert_eq!(u16_to_level(65535), 1.0);
    // GR: dB >= 0 at 1/4 dB, saturating at 63.75 dB.
    for v in [0.0f32, 3.5, 12.0, 63.75] {
        let back = u8_to_db(db_to_u8(v));
        assert!((back - v).abs() <= 0.125 + 1e-6, "gr {v} -> {back}");
    }
    assert_eq!(db_to_u8(-5.0), 0, "negative GR clamps to 0");
    assert_eq!(db_to_u8(100.0), 255, "saturates at GR_DB_MAX");
    assert_eq!(GR_DB_MAX, 63.75);
}

/// Minimal reference decoder for the binary meter layout.
struct BinFrame {
    ftype: u8,
    version: u8,
    n: u16,
    seq: u32,
    t_ms: u32,
    payload: Vec<u8>,
}

fn decode_bin(bytes: &[u8]) -> BinFrame {
    assert!(bytes.len() >= BIN_HEADER_LEN, "frame shorter than header");
    let ftype = bytes[0];
    let version = bytes[1];
    let n = u16::from_le_bytes([bytes[2], bytes[3]]);
    let seq = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let t_ms = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    BinFrame {
        ftype,
        version,
        n,
        seq,
        t_ms,
        payload: bytes[BIN_HEADER_LEN..].to_vec(),
    }
}

fn decode_levels(f: &BinFrame) -> Vec<f32> {
    assert_eq!(f.ftype, BIN_TYPE_METERS);
    assert_eq!(f.version, BIN_VERSION);
    assert_eq!(f.payload.len(), f.n as usize * 2);
    f.payload
        .as_chunks::<2>().0.iter()
        .map(|c| u16_to_level(u16::from_le_bytes([c[0], c[1]])))
        .collect()
}

fn decode_gr(f: &BinFrame) -> Vec<(u8, f32, f32)> {
    assert_eq!(f.ftype, BIN_TYPE_GR);
    assert_eq!(f.version, BIN_VERSION);
    assert_eq!(f.payload.len(), f.n as usize * 3);
    f.payload
        .as_chunks::<3>().0.iter()
        .map(|c| (c[0], u8_to_db(c[1]), u8_to_db(c[2])))
        .collect()
}

fn hex_of(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 15) as usize] as char);
    }
    s
}

fn bytes_of(hex: &str) -> Vec<u8> {
    let h = hex.trim();
    assert!(h.len().is_multiple_of(2), "odd hex length");
    (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Golden binary frames: deterministic seq/t_ms so the hex is stable.
/// Regenerate with `BIN_UPDATE=1` (mirrors `GOLDEN_UPDATE=1`).
#[test]
fn binary_golden_fixtures() {
    let dir = golden_bin_dir();
    let update = std::env::var("BIN_UPDATE").is_ok();
    // Same example values as the JSON goldens (meters.json, gr.json).
    let cases: Vec<(&str, Event)> = vec![
        (
            "meters",
            Event::Meters {
                levels: vec![0.5, 0.25, 0.0, 0.75],
            },
        ),
        (
            "gr",
            Event::Gr {
                ch: 1,
                gate: 3.5,
                comp: 0.0,
            },
        ),
    ];
    for (name, event) in &cases {
        // Fixed seq/t_ms keep the fixture byte-stable across runs.
        let bytes = event.to_binary(7, 123_456).unwrap();
        let path = dir.join(format!("{name}.bin.hex"));
        if update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, hex_of(&bytes) + "\n").unwrap();
            continue;
        }
        let raw =
            std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!("missing golden binary {name}.bin.hex (run with BIN_UPDATE=1)")
            });
        assert_eq!(
            bytes,
            bytes_of(&raw),
            "golden binary {name}.bin.hex matches Event::to_binary"
        );
    }

    // Decode the committed fixtures with the reference decoder and check
    // the values (this is what the UI test re-implements in TypeScript).
    let meters_raw =
        std::fs::read_to_string(dir.join("meters.bin.hex")).unwrap();
    let meters = decode_bin(&bytes_of(&meters_raw));
    assert_eq!((meters.ftype, meters.version, meters.n), (1, 1, 4));
    assert_eq!((meters.seq, meters.t_ms), (7, 123_456));
    let levels = decode_levels(&meters);
    assert_eq!(levels.len(), 4);
    for (got, want) in levels.iter().zip([0.5f32, 0.25, 0.0, 0.75]) {
        assert!((got - want).abs() <= 1.0 / 65535.0, "level {got} != {want}");
    }

    let gr_raw = std::fs::read_to_string(dir.join("gr.bin.hex")).unwrap();
    let gr = decode_bin(&bytes_of(&gr_raw));
    assert_eq!((gr.ftype, gr.version, gr.n), (2, 1, 1));
    assert_eq!((gr.seq, gr.t_ms), (7, 123_456));
    let entries = decode_gr(&gr);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, 1, "gr channel");
    assert!((entries[0].1 - 3.5).abs() < 1e-6, "gr gate {}", entries[0].1);
    assert_eq!(entries[0].2, 0.0, "gr comp {}", entries[0].2);

    // Control frames never encode: the adapter falls back to JSON text.
    assert!(
        Event::State {
            driver: "sim".into(),
            strips: vec![],
            main_fader: None,
            by: None,
        }
        .to_binary(0, 0)
        .is_none()
    );
    assert!(
        Event::Spectrum { ch: 0, bins: vec![] }
            .to_binary(0, 0)
            .is_none(),
        "spectrum stays JSON even in binary mode"
    );
}

/// MR1 write-priority latency: with the param poller saturated (1 ms tick)
/// and the meter fan-out cloned to 9 clients at the 50 ms source rate,
/// every write lane (fader/mute/param/main) must dispatch in <= 5 ms p99.
///
/// Multi-thread runtime so the poller truly races the writes (the yield
/// path executes: see the printed `poll_skips`). Prints `p50/p95/p99`
/// (reviewer re-runs and checks the bound). n=2000: p99 tolerates a score
/// of scheduling stalls on a loaded shared box while still catching any
/// systematic queueing behind the poller.
#[tokio::test(flavor = "multi_thread")]
async fn write_latency_p99_under_saturation() {
    let engine = Engine::new(EngineConfig {
        poll_interval: Duration::from_millis(1),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    // Meter fan-out: 8 extra clients so every source-rate tick broadcasts
    // meters + 16 spectrum + 16 gr frames into 9 mailboxes. Fan-out
    // mailboxes are drained in the background (real clients read), keeping
    // the harness's own memory churn out of the dispatch measurement.
    for i in 0..8 {
        let mut c = engine.attach(Some(format!("fan{i}"))).await;
        tokio::spawn(async move {
            while c.events.recv().await.is_some() {}
        });
    }
    let mut me = engine.attach(Some("writer".to_string())).await;
    drain_snapshot(&mut me).await;

    // Warm up (caches, allocator, runtime).
    for i in 0..50u16 {
        me.send(Cmd::Fader {
            ch: i % 32,
            value: 0.5, key: None})
        .await
        .unwrap();
    }
    while me.events.try_recv().is_ok() {}

    // Measure all four write lanes round-robin.
    let n = 2000usize;
    let mut dts: Vec<Duration> = Vec::with_capacity(n);
    for i in 0..n {
        let ch = (i % 32) as u16;
        let v = 0.1 + 0.8 * ((i % 100) as f32 / 100.0);
        let t = Instant::now();
        match i % 4 {
            0 => me.send(Cmd::Fader { ch, value: v, key: None}).await.unwrap(),
            1 => {
                me.send(Cmd::Mute {
                    ch,
                    muted: i % 8 < 4, key: None})
                .await
                .unwrap()
            }
            2 => {
                me.send(Cmd::Param {
                    ch,
                    field: "gain".to_string(),
                    value: serde_json::json!(20.0 + (i % 10) as f64), key: None})
                .await
                .unwrap()
            }
            _ => me.send(Cmd::MainFader { value: v }).await.unwrap(),
        }
        dts.push(t.elapsed());
    }
    dts.sort_unstable();
    let pct = |p: f64| dts[((p * n as f64) as usize).min(n - 1)];
    let (p50, p95, p99) = (pct(0.50), pct(0.95), pct(0.99));
    println!(
        "write_latency n={n} p50={p50:?} p95={p95:?} p99={p99:?} max={:?} poll_skips={}",
        dts[n - 1],
        engine.poll_skips(),
    );
    assert!(
        p99 <= Duration::from_millis(5),
        "write dispatch p99 {p99:?} exceeds the 5 ms MR1 budget"
    );
}
