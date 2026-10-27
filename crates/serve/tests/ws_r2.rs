//! R2 WS integration tests against the sim driver: devices/link/info,
//! numbered scenes, FX, routing, offline diff/push, and meter taps (JSON +
//! binary type 3). Mirrors the in-process `r2_api` scenarios over real
//! sockets so the serve adapter path (outbox slots, hello negotiation) is
//! covered where it matters.

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};
use tokio_tungstenite::tungstenite::Message;

type WsTx = futures::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    Message,
>;
type WsRx = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
>;

async fn serve() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let opts = openfader_serve::ServeOptions {
        driver: "sim".to_string(),
        ..openfader_serve::ServeOptions::default()
    };
    tokio::spawn(openfader_serve::run(listener, opts));
    format!("ws://{addr}/ws")
}

async fn connect(url: &str) -> (WsTx, WsRx) {
    let (stream, _) = tokio_tungstenite::connect_async(url).await.expect("WS connect");
    stream.split()
}

async fn send(tx: &mut WsTx, v: Value) {
    tx.send(Message::text(v.to_string())).await.unwrap();
}

/// Next TEXT frame parsed as JSON (binary meter frames skipped).
async fn next_msg(rx: &mut WsRx) -> Value {
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
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for WS frame type {want}");
        }
        let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) == Some(want) {
            return v;
        }
    }
}

/// Drain the attach snapshot prefix (you/state/shows/clients/locks/caps +
/// R2 devices/link/info/scenes/fx/routing), asserting its order.
async fn drain_snapshot(rx: &mut WsRx) {
    for want in [
        "you", "state", "shows", "clients", "locks", "caps", "devices", "link", "info",
        "scenes", "fx", "routing",
    ] {
        let v = next_typed(rx, want).await;
        assert_eq!(v.get("type").and_then(Value::as_str), Some(want));
    }
}

#[tokio::test]
async fn ws_r2_devices_link_info() {
    // Honest boot over WS: no sim rows in the snapshot, link disconnected.
    let url = serve().await;
    let (mut tx, mut rx) = connect(&url).await;
    drain_snapshot(&mut rx).await;
    send(&mut tx, json!({"cmd":"scan"})).await;
    let devices = next_typed(&mut rx, "devices").await;
    let list = devices.get("list").and_then(Value::as_array).unwrap();
    // Whatever the LAN holds, NOTHING is ever sim: every row carries its
    // source (broadcast/linked/manual); the seed rows had none.
    assert!(
        list.iter()
            .all(|d| d.get("via").and_then(Value::as_str).is_some()),
        "scan rows are all sourced, never sim seeds (got {list:?})"
    );
    assert!(
        devices
            .get("scanned_at")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    );

    send(&mut tx, json!({"cmd":"connect_device","id":"mon"})).await;
    let err = next_typed(&mut rx, "error").await;
    assert!(err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .contains("unknown device"));
    send(&mut tx, json!({"cmd":"fader","ch":0,"value":0.5})).await;
    next_typed(&mut rx, "state").await;
    send(&mut tx, json!({"cmd":"info"})).await;
    let info = next_typed(&mut rx, "info").await;
    assert_eq!(info.get("sample_rate"), Some(&json!(48000)));
    assert_eq!(info.get("driver"), Some(&json!("sim")));
}

#[tokio::test]
async fn ws_r2_scene_fx_routing_roundtrip() {
    let url = serve().await;
    let (mut tx, mut rx) = connect(&url).await;
    drain_snapshot(&mut rx).await;
    send(&mut tx, json!({"cmd":"fader","ch":0,"value":0.9})).await;
    next_typed(&mut rx, "state").await;
    send(&mut tx, json!({"cmd":"param","ch":0,"field":"eq1_gain","value":5.0})).await;
    next_typed(&mut rx, "param").await;
    send(&mut tx, json!({"cmd":"scene_save","n":4,"name":"WS Set"})).await;
    let scenes = next_typed(&mut rx, "scenes").await;
    assert!(scenes
        .get("scenes")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .any(|s| s.get("number") == Some(&json!(4))));
    send(&mut tx, json!({"cmd":"fader","ch":0,"value":0.1})).await;
    next_typed(&mut rx, "state").await;
    send(&mut tx, json!({"cmd":"scene_recall","n":4,"fade_ms":0})).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for recalled fader echo");
        }
        let v = next_msg(&mut rx).await;
        if v.get("type").and_then(Value::as_str) == Some("param")
            && v.get("ch") == Some(&json!(0))
            && v.get("field") == Some(&json!("fader"))
        {
            let got = v.get("value").and_then(Value::as_f64).unwrap();
            assert!((got - 0.9).abs() < 1e-6, "recalled fader {got}");
            break;
        }
    }
    send(&mut tx, json!({"cmd":"fx_set_type","slot":2,"type":"Delay"})).await;
    let fx = next_typed(&mut rx, "fx").await;
    let slots = fx.get("slots").and_then(Value::as_array).unwrap();
    assert_eq!(slots[1].get("type"), Some(&json!("Delay")));
    send(&mut tx, json!({"cmd":"fx_param","slot":2,"id":"mix","value":0.25})).await;
    let fx = next_typed(&mut rx, "fx").await;
    let slots = fx.get("slots").and_then(Value::as_array).unwrap();
    assert_eq!(
        slots[1].get("params").and_then(|p| p.get("mix")),
        Some(&json!(0.25f32))
    );
    send(&mut tx, json!({"cmd":"patch_set","strip":3,"group":"USB","index":7})).await;
    let routing = next_typed(&mut rx, "routing").await;
    assert!(routing
        .get("patches")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .any(|p| p.get("strip") == Some(&json!(3)) && p.get("label") == Some(&json!("USB 07"))));
    send(&mut tx, json!({"cmd":"patch_clear","strip":3})).await;
    let routing = next_typed(&mut rx, "routing").await;
    assert!(routing
        .get("patches")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .all(|p| p.get("strip") != Some(&json!(3))));
}

#[tokio::test]
async fn ws_r2_offline_diff_and_push() {
    let url = serve().await;
    let (mut tx, mut rx) = connect(&url).await;
    drain_snapshot(&mut rx).await;
    send(&mut tx, json!({"cmd":"offline","model":"xair"})).await;
    let link = next_typed(&mut rx, "link").await;
    assert_eq!(link.get("state"), Some(&json!("offline")));
    send(&mut tx, json!({"cmd":"param","ch":1,"field":"gain","value":33.0})).await;
    next_typed(&mut rx, "param").await;
    send(&mut tx, json!({"cmd":"connect","driver":"sim"})).await;
    let diff = next_typed(&mut rx, "offline_diff").await;
    assert_eq!(diff.get("count"), Some(&json!(1)));
    send(&mut tx, json!({"cmd":"offline_push","confirm":true})).await;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for pushed gain echo");
        }
        let v = next_msg(&mut rx).await;
        if v.get("type").and_then(Value::as_str) == Some("param")
            && v.get("ch") == Some(&json!(1))
            && v.get("field") == Some(&json!("gain"))
        {
            let got = v.get("value").and_then(Value::as_f64).unwrap();
            assert!((got - 33.0).abs() < 1e-6, "pushed gain {got}");
            break;
        }
    }
}

#[tokio::test]
async fn ws_r2_taps_json_and_binary() {
    let url = serve().await;
    let (mut tx_a, mut rx_a) = connect(&url).await;
    drain_snapshot(&mut rx_a).await;
    let (_tx_b, mut rx_b) = connect(&url).await;
    drain_snapshot(&mut rx_b).await;
    // Client A negotiates taps: its taps frames arrive as WS BINARY
    // type 3, while client B (no flag) keeps getting JSON text.
    send(&mut tx_a, json!({"cmd":"hello","taps":true})).await;
    let ack = next_typed(&mut rx_a, "hello").await;
    assert_eq!(ack.get("taps"), Some(&json!(true)));
    // A: binary type-3 frame (header like meters + tap id + n×u16).
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for binary taps on A");
        }
        let next = timeout(Duration::from_millis(500), rx_a.next()).await;
        let msg = match next {
            Ok(Some(Ok(m))) => m,
            _ => continue,
        };
        if let Message::Binary(bytes) = msg {
            assert_eq!(bytes[0], 3, "binary taps type");
            assert_eq!(bytes[1], 1, "binary version");
            let n = u16::from_le_bytes([bytes[2], bytes[3]]);
            assert_eq!(n, 81, "taps cover every sim strip");
            assert!(bytes[12] <= 3, "tap id in/post/gr_gate/gr_comp");
            assert_eq!(bytes.len(), 12 + 1 + 81 * 2);
            break;
        }
    }
    // B: JSON taps text with a valid tap name and 81 levels.
    let taps = next_typed(&mut rx_b, "taps").await;
    let tap = taps.get("tap").and_then(Value::as_str).unwrap();
    assert!(["in", "post", "gr_gate", "gr_comp"].contains(&tap));
    assert_eq!(taps.get("levels").and_then(Value::as_array).unwrap().len(), 81);
}
