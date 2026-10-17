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

    // GR: one message per channel at ~5 Hz (every 2nd meter tick), first
    // 16 channels; gate/comp are dB reduction >= 0 (sim has no GR source,
    // so the relay sends idle 0.0 — still proves the frames flow).
    let frame = next_typed(&mut rx, "gr").await;
    let ch = frame
        .get("ch")
        .and_then(Value::as_u64)
        .expect("gr carries ch");
    assert!(ch < 16, "gr capped at first 16 channels, got ch{ch}");
    for key in ["gate", "comp"] {
        let v = frame
            .get(key)
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("gr carries {key}"));
        assert!(
            v >= 0.0,
            "gr {key} is dB reduction >= 0, got {v}"
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

/// Scene ops round-trip on the server-side show library:
/// capture -> list -> move faders -> recall (jump) -> verify state -> delete.
#[tokio::test]
async fn ws_scenes_capture_recall_delete() {
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

    // Drain the initial snapshot pair (state + shows).
    let _ = next_typed(&mut rx, "state").await;
    let shows = next_typed(&mut rx, "shows").await;
    assert_eq!(shows.get("type"), Some(&json!("shows")));
    assert!(
        shows.get("shows").and_then(Value::as_array).is_some(),
        "shows carries a library array"
    );

    async fn wait_shows(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for expected shows; last: {last}");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("shows") {
                continue;
            }
            last = v;
            if want(&last) {
                return last.clone();
            }
        }
    }

    fn find_scene<'a>(shows: &'a Value, show: &str, scene: &str) -> Option<&'a Value> {
        shows
            .get("shows")?
            .as_array()?
            .iter()
            .find(|s| s.get("name") == Some(&json!(show)))?
            .get("scenes")?
            .as_array()?
            .iter()
            .find(|s| s.get("name") == Some(&json!(scene)))
    }

    // Pin ch0 to a known value, then capture it as gig/verse.
    tx.send(Message::Text(
        json!({"cmd":"fader","ch":0,"value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.5).abs() < 1e-6)
    })
    .await;
    tx.send(Message::Text(
        json!({"cmd":"capture_scene","show":"gig","name":"verse"}).to_string(),
    ))
    .await
    .unwrap();
    let shows = wait_shows(&mut rx, |s| find_scene(s, "gig", "verse").is_some()).await;
    let captured = find_scene(&shows, "gig", "verse").expect("captured scene");
    let cap_fader = captured
        .get("strips")
        .and_then(Value::as_array)
        .and_then(|ss| ss.iter().find(|s| s.get("ch") == Some(&json!(0))))
        .and_then(|s| s.get("fader"))
        .and_then(Value::as_f64)
        .expect("scene carries ch0 fader");
    assert!((cap_fader - 0.5).abs() < 1e-6, "captured live fader");

    // Library survives a fresh connection (no localStorage round-trip).
    let (stream2, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("second WS connect");
    let (_, mut rx2) = stream2.split();
    let _ = next_typed(&mut rx2, "state").await;
    let shows2 = next_typed(&mut rx2, "shows").await;
    assert!(
        find_scene(&shows2, "gig", "verse").is_some(),
        "second connection sees the server-side library"
    );

    // Explicit pull also returns the library.
    tx.send(Message::Text(json!({"cmd":"list_shows"}).to_string()))
        .await
        .unwrap();
    let listed = wait_shows(&mut rx, |s| find_scene(s, "gig", "verse").is_some()).await;
    assert!(find_scene(&listed, "gig", "verse").is_some());

    // Move ch0 away, then recall with fade 0 (jump) and verify the snap-back.
    let moved = (cap_fader + 0.3).min(1.0);
    assert!((moved - cap_fader).abs() > 0.05, "moved value differs");
    tx.send(Message::Text(
        json!({"cmd":"fader","ch":0,"value":moved}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - moved).abs() < 1e-6)
    })
    .await;
    tx.send(Message::Text(
        json!({"cmd":"recall_scene","show":"gig","name":"verse","fade_ms":0}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - cap_fader).abs() < 1e-6)
    })
    .await;

    // Unknown scene errors; connection stays alive.
    tx.send(Message::Text(
        json!({"cmd":"recall_scene","show":"gig","name":"nope","fade_ms":0}).to_string(),
    ))
    .await
    .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some());

    // Delete the scene; the show stays with an empty scene list.
    tx.send(Message::Text(
        json!({"cmd":"delete_scene","show":"gig","name":"verse"}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_shows(&mut rx, |s| find_scene(s, "gig", "verse").is_none()).await;
    tx.send(Message::Text(json!({"cmd":"list_shows"}).to_string()))
        .await
        .unwrap();
    let listed = wait_shows(&mut rx, |_| true).await;
    assert!(
        find_scene(&listed, "gig", "verse").is_none(),
        "deleted scene stays gone"
    );

    server.abort();
}
