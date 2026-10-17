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

/// Keep reading frames until a `param` echo with the expected
/// ch/field/value arrives (siblings of the mutation broadcast).
async fn wait_param(rx: &mut WsRx, ch: u64, field: &str, want: &Value) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last = json!(null);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for param echo ch{ch} {field}={want}; last: {last}");
        }
        let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) != Some("param") {
            continue;
        }
        last = v;
        let ch_ok = last.get("ch") == Some(&json!(ch));
        let field_ok = last.get("field") == Some(&json!(field));
        let val_ok = match (last.get("value"), want) {
            (Some(Value::Number(a)), Value::Number(b)) => {
                a.as_f64().zip(b.as_f64()).is_some_and(|(x, y)| (x - y).abs() < 1e-6)
            }
            (Some(a), b) => a == b,
            (None, _) => false,
        };
        if ch_ok && field_ok && val_ok {
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
    let _state = wait_state(&mut rx, |s| {
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

    // Generic param: fader (float) maps through set_param; the server
    // echoes {"type":"param","ch":1,"field":"fader","value":0.25} so
    // sibling UIs learn about the change without re-polling.
    tx.send(Message::Text(
        json!({"cmd":"param","ch":1,"field":"fader","value":0.25}).to_string(),
    ))
    .await
    .unwrap();
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_param = false;
        let mut saw_state = false;
        while !(saw_param && saw_state) {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for param echo + state (param={saw_param}, state={saw_state})");
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").and_then(Value::as_str) {
                Some("param") => {
                    let ch_ok = v.get("ch") == Some(&json!(1));
                    let field_ok = v.get("field") == Some(&json!("fader"));
                    let val_ok = v
                        .get("value")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - 0.25).abs() < 1e-6);
                    if ch_ok && field_ok && val_ok {
                        saw_param = true;
                    }
                }
                Some("state") => {
                    if strip(&v, 1)
                        .get("fader")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - 0.25).abs() < 1e-6)
                    {
                        saw_state = true;
                    }
                }
                _ => {}
            }
        }
    }

    // Bool + string params echo with identical ch/field/value.
    tx.send(Message::Text(
        json!({"cmd":"param","ch":2,"field":"mute","value":true}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 2, "mute", &json!(true)).await;
    assert_eq!(echo.get("value"), Some(&json!(true)));
    tx.send(Message::Text(
        json!({"cmd":"param","ch":2,"field":"mute","value":false}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 2, "mute", &json!(false)).await;
    assert_eq!(echo.get("value"), Some(&json!(false)));

    tx.send(Message::Text(
        json!({"cmd":"param","ch":3,"field":"name","value":"Vox"}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 3, "name", &json!("Vox")).await;
    assert_eq!(echo.get("value"), Some(&json!("Vox")));

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
    // Recall moves faders and re-emits each applied param so sibling UIs
    // catch up; expect both the state snap-back and the ch0 fader echo.
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_param = false;
        let mut saw_state = false;
        while !(saw_param && saw_state) {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for recall param echo + state (param={saw_param}, state={saw_state})");
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").and_then(Value::as_str) {
                Some("param") => {
                    let ch_ok = v.get("ch") == Some(&json!(0));
                    let field_ok = v.get("field") == Some(&json!("fader"));
                    let val_ok = v
                        .get("value")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - cap_fader).abs() < 1e-6);
                    if ch_ok && field_ok && val_ok {
                        saw_param = true;
                    }
                }
                Some("state") => {
                    if strip(&v, 0)
                        .get("fader")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - cap_fader).abs() < 1e-6)
                    {
                        saw_state = true;
                    }
                }
                _ => {}
            }
        }
    }

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

/// Presence: two clients share one roster; mutations carry the writer id.
///
/// c1 joins (?name=alice), c2 joins (?name=bob) and immediately sees
/// count==2; c1 gets the join broadcast. A fader/mute/param mutation
/// from c1 arrives at c2 with `"by"` set, and a third join observes c1
/// in `writers` (last-write-wins, 10 s window).
#[tokio::test]
async fn ws_client_presence_and_writer_tag() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openmix_serve::run(
        listener,
        openmix_serve::ServeOptions::default(),
    ));
    use tokio_tungstenite::tungstenite::Message;

    async fn connect(url: String) -> (WsTx, WsRx) {
        let (stream, _) = tokio_tungstenite::connect_async(url).await.expect("WS connect");
        stream.split()
    }
    type WsTx = futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >;

    async fn wait_clients(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for expected clients; last: {last}");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("clients") {
                continue;
            }
            last = v;
            if want(&last) {
                return last.clone();
            }
        }
    }

    // c1 joins with a name; initial roster is itself alone.
    let (mut tx1, mut rx1) = connect(format!("ws://{addr}/ws?name=alice")).await;
    let _ = next_typed(&mut rx1, "state").await;
    let _ = next_typed(&mut rx1, "shows").await;
    let c1 = next_typed(&mut rx1, "clients").await;
    assert_eq!(c1.get("count"), Some(&json!(1)));

    // c2 joins (no name -> client-N fallback) and sees both clients.
    let (mut tx2, mut rx2) = connect(format!("ws://{addr}/ws")).await;
    let _ = next_typed(&mut rx2, "state").await;
    let _ = next_typed(&mut rx2, "shows").await;
    let c2 = next_typed(&mut rx2, "clients").await;
    assert_eq!(
        c2.get("count"),
        Some(&json!(2)),
        "second client sees first in clients frame"
    );

    // c1 learns about the join via broadcast.
    let joined = wait_clients(&mut rx1, |v| v.get("count") == Some(&json!(2))).await;
    assert_eq!(joined.get("count"), Some(&json!(2)));

    // Fader mutation from c1 arrives at c2 tagged with the writer id.
    tx1.send(Message::Text(
        json!({"cmd":"fader","ch":0,"value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let st = wait_state(&mut rx2, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.5).abs() < 1e-6)
    })
    .await;
    let by_fader = st
        .get("by")
        .and_then(Value::as_str)
        .expect("state carries by-field");
    assert!(!by_fader.is_empty());

    // Mute mutation is tagged too.
    tx1.send(Message::Text(
        json!({"cmd":"mute","ch":0,"muted":true}).to_string(),
    ))
    .await
    .unwrap();
    let st = wait_state(&mut rx2, |s| strip(s, 0).get("muted") == Some(&json!(true))).await;
    assert_eq!(st.get("by"), Some(&json!(by_fader)));

    // Param echo carries the same writer.
    tx1.send(Message::Text(
        json!({"cmd":"param","ch":1,"field":"fader","value":0.25}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx2, 1, "fader", &json!(0.25)).await;
    assert_eq!(echo.get("by"), Some(&json!(by_fader)));

    // A third join observes c1 as a recent writer.
    let (_tx3, mut rx3) = connect(format!("ws://{addr}/ws?name=carol")).await;
    let _ = next_typed(&mut rx3, "state").await;
    let _ = next_typed(&mut rx3, "shows").await;
    let c3 = next_typed(&mut rx3, "clients").await;
    assert_eq!(c3.get("count"), Some(&json!(3)));
    let writers = c3
        .get("writers")
        .and_then(Value::as_array)
        .expect("clients carries writers");
    assert!(
        writers.iter().any(|w| w == &json!(by_fader)),
        "writers include the recent mutator, got {writers:?}"
    );

    // Leave broadcast: c2 closing brings the roster back to 2.
    tx2.send(Message::Close(None)).await.ok();
    let _ = wait_clients(&mut rx1, |v| v.get("count") == Some(&json!(2))).await;

    let _ = &mut tx1;
    server.abort();
}
