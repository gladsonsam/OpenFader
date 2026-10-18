//! MR1 realtime integration tests (serve side):
//! no-deflate handshake, `hello` + binary meter fan-out, `stats` + golden
//! shape, slow-consumer drop counting (fast path; the full stall proof is
//! the node slow-consumer script, which can truly pause TCP reads).

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;
type WsRx = futures::stream::SplitStream<WsStream>;
type WsTx = futures::stream::SplitSink<WsStream, tokio_tungstenite::tungstenite::Message>;

async fn serve() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(openmix_serve::run(
        listener,
        openmix_serve::ServeOptions::default(),
    ));
    addr
}

async fn connect(addr: &std::net::SocketAddr) -> (WsTx, WsRx) {
    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    stream.split()
}

/// Next TEXT frame parsed as JSON (binary frames skipped).
async fn next_text(rx: &mut WsRx) -> Value {
    use tokio_tungstenite::tungstenite::Message;
    loop {
        let msg = timeout(Duration::from_secs(5), rx.next())
            .await
            .expect("timed out waiting for WS message")
            .expect("WS stream ended")
            .expect("WS read error");
        if let Message::Text(text) = msg {
            return serde_json::from_str(&text).expect("server sent invalid JSON");
        }
    }
}

async fn next_typed(rx: &mut WsRx, want: &str) -> Value {
    loop {
        let v = next_text(rx).await;
        if v.get("type").and_then(Value::as_str) == Some(want) {
            return v;
        }
    }
}

/// The handshake never negotiates permessage-deflate: even when the client
/// offers it, the 101 response carries no `Sec-WebSocket-Extensions`
/// header (the upgrade path is a raw hyper connection + axum extractor
/// with no extension support, so frames always go out uncompressed).
#[tokio::test]
async fn handshake_has_no_deflate_even_when_offered() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let addr = serve().await;
    let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
    let key = "dGhlIHNhbXBsZSBub25jZQ==";
    let req = format!(
        "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Extensions: permessage-deflate; client_max_window_bits\r\n\r\n"
    );
    sock.write_all(req.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = timeout(Duration::from_secs(5), sock.read(&mut buf))
        .await
        .expect("handshake reply in time")
        .unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).to_lowercase();
    assert!(
        head.starts_with("http/1.1 101"),
        "upgrade accepted, got: {}",
        head.lines().next().unwrap_or("")
    );
    assert!(
        !head.contains("sec-websocket-extensions"),
        "no deflate negotiated, got headers:\n{head}"
    );
}

/// `hello` negotiates binary meters: the unicast ack arrives, then
/// `Meters`/`Gr` arrive as WS BINARY with the documented header while
/// their JSON text forms stop; `spectrum` stays JSON text.
#[tokio::test]
async fn hello_binary_meters_flow() {
    use tokio_tungstenite::tungstenite::Message;
    let addr = serve().await;
    let (mut tx, mut rx) = connect(&addr).await;

    // Drain the attach snapshot (state/shows arrive before any binary).
    let _ = next_typed(&mut rx, "state").await;

    tx.send(Message::Text(
        json!({"cmd":"hello","binary_meters":true}).to_string(),
    ))
    .await
    .unwrap();
    let ack = next_typed(&mut rx, "hello").await;
    assert_eq!(ack.get("binary_meters"), Some(&json!(true)));

    // First binary meters frame: header [type=1, version=1, n, seq, t_ms].
    let frame = timeout(Duration::from_secs(5), async {
        loop {
            let msg = rx.next().await.expect("stream") .expect("read");
            if let Message::Binary(b) = msg {
                return b;
            }
        }
    })
    .await
    .expect("binary meters in time");
    assert!(frame.len() >= 12, "header length");
    assert_eq!(frame[0], 1, "type=meters");
    assert_eq!(frame[1], 1, "version=1");
    let n = u16::from_le_bytes([frame[2], frame[3]]) as usize;
    assert!(n > 0, "non-empty levels");
    assert_eq!(frame.len(), 12 + n * 2, "n x u16 payload");
    let seq0 = u32::from_le_bytes([frame[4], frame[5], frame[6], frame[7]]);
    let t0 = u32::from_le_bytes([frame[8], frame[9], frame[10], frame[11]]);

    // Second binary frame: seq advances, t_ms is monotonic.
    let frame2 = timeout(Duration::from_secs(5), async {
        loop {
            let msg = rx.next().await.expect("stream").expect("read");
            if let Message::Binary(b) = msg {
                return b;
            }
        }
    })
    .await
    .expect("second binary frame in time");
    let seq1 = u32::from_le_bytes([frame2[4], frame2[5], frame2[6], frame2[7]]);
    let t1 = u32::from_le_bytes([frame2[8], frame2[9], frame2[10], frame2[11]]);
    assert!(seq1 > seq0, "seq advances ({seq0} -> {seq1})");
    assert!(t1 >= t0, "t_ms monotonic ({t0} -> {t1})");

    // GR also arrives binary (type=2, n=1, 3-byte payload).
    let gr = timeout(Duration::from_secs(5), async {
        loop {
            let msg = rx.next().await.expect("stream").expect("read");
            if let Message::Binary(b) = msg {
                if !b.is_empty() && b[0] == 2 {
                    return b;
                }
            }
        }
    })
    .await
    .expect("binary gr in time");
    assert_eq!(gr[1], 1, "gr version=1");
    assert_eq!(u16::from_le_bytes([gr[2], gr[3]]), 1, "gr n=1");
    assert_eq!(gr.len(), 12 + 3, "gr payload is ch+gate+comp");

    // JSON meters/gr stop in binary mode, but spectrum stays JSON text.
    let deadline = std::time::Instant::now() + Duration::from_millis(600);
    let mut saw_json_meters = false;
    let mut saw_json_gr = false;
    let mut saw_spectrum = false;
    while std::time::Instant::now() < deadline {
        let next = timeout(Duration::from_millis(200), async {
            loop {
                let msg = rx.next().await.expect("stream").expect("read");
                if let Message::Text(t) = msg {
                    return serde_json::from_str::<Value>(&t).unwrap();
                }
            }
        })
        .await;
        let Ok(v) = next else { continue };
        match v.get("type").and_then(Value::as_str) {
            Some("meters") => saw_json_meters = true,
            Some("gr") => saw_json_gr = true,
            Some("spectrum") => saw_spectrum = true,
            _ => {}
        }
    }
    assert!(!saw_json_meters, "no JSON meters in binary mode");
    assert!(!saw_json_gr, "no JSON gr in binary mode");
    assert!(saw_spectrum, "spectrum stays JSON text");
}

/// `stats` is answered locally with the pinned shape; counters move with
/// traffic (meters sent, none dropped on a fast client).
#[tokio::test]
async fn stats_reports_counters() {
    use tokio_tungstenite::tungstenite::Message;
    let addr = serve().await;
    let (mut tx, mut rx) = connect(&addr).await;
    let _ = next_typed(&mut rx, "state").await;

    tx.send(Message::Text(json!({"cmd":"stats"}).to_string()))
        .await
        .unwrap();
    let st = next_typed(&mut rx, "stats").await;
    for key in [
        "meters_sent",
        "meters_dropped",
        "gr_sent",
        "gr_dropped",
        "spectrum_sent",
        "spectrum_dropped",
        "queue_depth",
        "binary",
    ] {
        assert!(st.get(key).is_some(), "stats carries {key}: {st}");
    }
    assert_eq!(st.get("binary"), Some(&json!(false)), "default is JSON");
    assert_eq!(st.get("type"), Some(&json!("stats")));

    // Let meter traffic flow, then re-query: sends advance, drops stay 0
    // on a fast client, queue stays drained.
    tokio::time::sleep(Duration::from_millis(300)).await;
    tx.send(Message::Text(json!({"cmd":"stats"}).to_string()))
        .await
        .unwrap();
    let st = next_typed(&mut rx, "stats").await;
    assert!(
        st.get("meters_sent").and_then(Value::as_u64).unwrap_or(0) > 0,
        "meters flow on a live client: {st}"
    );
    assert_eq!(st.get("meters_dropped"), Some(&json!(0)));
    assert_eq!(st.get("gr_dropped"), Some(&json!(0)));
    assert!(
        st.get("queue_depth").and_then(Value::as_u64).unwrap_or(999) < 32,
        "fast client queue stays drained: {st}"
    );
}

/// The committed `stats` example parses and carries the pinned keys.
#[test]
fn stats_golden_shape() {
    let raw = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/stats.json"),
    )
    .expect("stats golden fixture");
    let v: Value = serde_json::from_str(&raw).expect("valid JSON");
    assert_eq!(v.get("type"), Some(&json!("stats")));
    for key in [
        "meters_sent",
        "meters_dropped",
        "gr_sent",
        "gr_dropped",
        "spectrum_sent",
        "spectrum_dropped",
        "queue_depth",
        "binary",
    ] {
        assert!(v.get(key).is_some(), "golden stats carries {key}");
    }
    // The live constructor produces the same key set (byte-stable order).
    let live: Value = serde_json::from_str(&openmix_serve::stats_json(
        &openmix_serve::StatsSnapshot {
            meters_sent: 1,
            ..Default::default()
        },
    ))
    .unwrap();
    let mut a: Vec<_> = v.as_object().unwrap().keys().collect();
    let mut b: Vec<_> = live.as_object().unwrap().keys().collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "live stats keys match the golden fixture");
}
