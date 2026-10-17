//! WS API integration test against the sim driver:
//! connect -> fader -> mute -> state assertions, plus a meters tick.

use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::time::{timeout, Duration};

async fn next_msg(
    rx: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) -> Value {
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

/// Next server message with `"type" == want`, skipping interleaved frames
/// (e.g. 10 Hz meters while waiting for state).
async fn next_typed(
    rx: &mut futures::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    want: &str,
) -> Value {
    loop {
        let v = next_msg(rx).await;
        if v.get("type").and_then(Value::as_str) == Some(want) {
            return v;
        }
    }
}

fn strip(state: &Value, ch: u64) -> &Value {
    state
        .get("strips")
        .and_then(Value::as_array)
        .and_then(|s| s.iter().find(|s| s.get("ch") == Some(&json!(ch))))
        .unwrap_or_else(|| panic!("no strip ch{ch} in {state}"))
}

type WsRx = futures::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
>;

/// Keep reading `state` frames until `want` holds (a pre-command snapshot
/// may still be in flight). Fails with the last-seen state on timeout.
async fn wait_state(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last = json!(null);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for expected state; last: {last}");
        }
        // Poll the stream with a short timeout so the deadline applies even
        // when only meters frames arrive.
        let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) != Some("state") {
            continue;
        }
        last = v;
        if want(&last) {
            return last.clone();
        }
    }
}

#[tokio::test]
async fn ws_sim_connect_fader_mute() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openmix_serve::run(
        listener,
        openmix_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    use tokio_tungstenite::tungstenite::Message;

    // Initial snapshot on connect.
    let state = next_typed(&mut rx, "state").await;
    assert_eq!(state.get("driver"), Some(&json!("sim")));
    assert!(
        state
            .get("strips")
            .and_then(Value::as_array)
            .is_some_and(|s| !s.is_empty()),
        "state carries strips"
    );

    // Explicit connect command re-selects sim.
    tx.send(Message::Text(
        json!({"cmd":"connect","driver":"sim"}).to_string(),
    ))
    .await
    .unwrap();
    let state = next_typed(&mut rx, "state").await;
    assert_eq!(state.get("driver"), Some(&json!("sim")));

    // Fader: ch0 (0-based) -> 0.5.
    tx.send(Message::Text(
        json!({"cmd":"fader","ch":0,"value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let state = wait_state(&mut rx, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.5).abs() < 1e-6)
    })
    .await;

    // Mute on.
    tx.send(Message::Text(
        json!({"cmd":"mute","ch":0,"muted":true}).to_string(),
    ))
    .await
    .unwrap();
    let _state = wait_state(&mut rx, |s| strip(s, 0).get("muted") == Some(&json!(true))).await;

    // Mute off again (keeps the shared sim clean for other tests).
    tx.send(Message::Text(
        json!({"cmd":"mute","ch":0,"muted":false}).to_string(),
    ))
    .await
    .unwrap();
    let _state =
        wait_state(&mut rx, |s| strip(s, 0).get("muted") == Some(&json!(false))).await;

    // Generic param: gain (float) maps through set_param.
    tx.send(Message::Text(
        json!({"cmd":"param","ch":1,"field":"fader","value":0.25}).to_string(),
    ))
    .await
    .unwrap();
    let _state = wait_state(&mut rx, |s| {
        strip(s, 1)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.25).abs() < 1e-6)
    })
    .await;

    // Meters tick at ~10 Hz.
    let meters = next_typed(&mut rx, "meters").await;
    assert!(
        meters
            .get("levels")
            .and_then(Value::as_array)
            .is_some_and(|l| !l.is_empty()),
        "meters carry levels"
    );

    // Spectrum: one message per channel per tick for the first 16
    // channels; each carries 64 peak-normalized bins in [0,1].
    let spec = next_typed(&mut rx, "spectrum").await;
    let ch = spec
        .get("ch")
        .and_then(Value::as_u64)
        .expect("spectrum carries ch");
    assert!(ch < 16, "spectrum capped at first 16 channels, got ch{ch}");
    let bins = spec
        .get("bins")
        .and_then(Value::as_array)
        .expect("spectrum carries bins");
    assert_eq!(bins.len(), 64, "spectrum carries 64 bins");
    for b in bins {
        let v = b.as_f64().expect("bins are numbers");
        assert!(
            (0.0..=1.0).contains(&v),
            "bins normalized to [0,1], got {v}"
        );
    }

    // Bad command gets an error frame, connection stays alive.
    tx.send(Message::Text(json!({"cmd":"bogus"}).to_string()))
        .await
        .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some());

    server.abort();
}
