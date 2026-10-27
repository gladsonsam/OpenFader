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

/// Full `caps` catalogue over WS: attach ships the slim caps
/// (essentials lane only), so send `hello` first, then skip the slim frame
/// and take the unicast full catalogue the first hello triggers (queued
/// before its ack).
async fn next_full_caps(
    tx: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
    rx: &mut WsRx,
) -> Value {
    use futures::SinkExt;
    use tokio_tungstenite::tungstenite::Message;
    tx.send(Message::text(json!({"cmd":"hello"}).to_string()))
        .await
        .expect("hello send");
    loop {
        let v = next_typed(rx, "caps").await;
        if v.get("params").and_then(Value::as_array).is_some_and(|params| {
            params.iter().any(|param| param.get("id").and_then(Value::as_str) == Some("gain"))
        }) {
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
async fn ws_sim_studio_preset() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let opts = openfader_serve::ServeOptions {
        preset: Some("studio".to_string()),
        ..Default::default()
    };
    let server = tokio::spawn(openfader_serve::run(listener, opts));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (_tx, mut rx) = stream.split();

    let state = next_typed(&mut rx, "state").await;
    assert_eq!(
        strip(&state, 0).get("name"),
        Some(&json!("BASS")),
        "studio preset mirrors the rig"
    );
    server.abort();
}

#[tokio::test]
async fn ws_sim_connect_fader_mute() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    use tokio_tungstenite::tungstenite::Message;

    let state = next_typed(&mut rx, "state").await;
    assert_eq!(state.get("driver"), Some(&json!("sim")));
    assert!(
        state
            .get("strips")
            .and_then(Value::as_array)
            .is_some_and(|s| !s.is_empty()),
        "state carries strips"
    );

    tx.send(Message::text(
        json!({"cmd":"connect","driver":"sim"}).to_string(),
    ))
    .await
    .unwrap();
    let state = next_typed(&mut rx, "state").await;
    assert_eq!(state.get("driver"), Some(&json!("sim")));

    tx.send(Message::text(
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

    tx.send(Message::text(
        json!({"cmd":"mute","ch":0,"muted":true}).to_string(),
    ))
    .await
    .unwrap();
    let _state = wait_state(&mut rx, |s| strip(s, 0).get("muted") == Some(&json!(true))).await;

    // Mute off again (keeps the shared sim clean for other tests).
    tx.send(Message::text(
        json!({"cmd":"mute","ch":0,"muted":false}).to_string(),
    ))
    .await
    .unwrap();
    let _state =
        wait_state(&mut rx, |s| strip(s, 0).get("muted") == Some(&json!(false))).await;

    // Generic param: fader (float) maps through set_param; the server
    // echoes {"type":"param","ch":1,"field":"fader","value":0.25} so
    // sibling UIs learn about the change without re-polling.
    tx.send(Message::text(
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
                Some("state")
                    if strip(&v, 1)
                        .get("fader")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - 0.25).abs() < 1e-6)
                    => {
                        saw_state = true;
                    }
                _ => {}
            }
        }
    }

    tx.send(Message::text(
        json!({"cmd":"param","ch":2,"field":"mute","value":true}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 2, "mute", &json!(true)).await;
    assert_eq!(echo.get("value"), Some(&json!(true)));
    tx.send(Message::text(
        json!({"cmd":"param","ch":2,"field":"mute","value":false}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 2, "mute", &json!(false)).await;
    assert_eq!(echo.get("value"), Some(&json!(false)));

    tx.send(Message::text(
        json!({"cmd":"param","ch":3,"field":"name","value":"Vox"}).to_string(),
    ))
    .await
    .unwrap();
    let echo = wait_param(&mut rx, 3, "name", &json!("Vox")).await;
    assert_eq!(echo.get("value"), Some(&json!("Vox")));

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
    // so the relay sends idle 0.0, still proves the frames flow).
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
    tx.send(Message::text(json!({"cmd":"bogus"}).to_string()))
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
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    use tokio_tungstenite::tungstenite::Message;

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

    tx.send(Message::text(
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
    tx.send(Message::text(
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

    tx.send(Message::text(json!({"cmd":"list_shows"}).to_string()))
        .await
        .unwrap();
    let listed = wait_shows(&mut rx, |s| find_scene(s, "gig", "verse").is_some()).await;
    assert!(find_scene(&listed, "gig", "verse").is_some());

    let moved = (cap_fader + 0.3).min(1.0);
    assert!((moved - cap_fader).abs() > 0.05, "moved value differs");
    tx.send(Message::text(
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
    tx.send(Message::text(
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
                Some("state")
                    if strip(&v, 0)
                        .get("fader")
                        .and_then(Value::as_f64)
                        .is_some_and(|f| (f - cap_fader).abs() < 1e-6)
                    => {
                        saw_state = true;
                    }
                _ => {}
            }
        }
    }

    tx.send(Message::text(
        json!({"cmd":"recall_scene","show":"gig","name":"nope","fade_ms":0}).to_string(),
    ))
    .await
    .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some());

    // Delete the scene; the show stays with an empty scene list.
    tx.send(Message::text(
        json!({"cmd":"delete_scene","show":"gig","name":"verse"}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_shows(&mut rx, |s| find_scene(s, "gig", "verse").is_none()).await;
    tx.send(Message::text(json!({"cmd":"list_shows"}).to_string()))
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
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
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
    tx1.send(Message::text(
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
    tx1.send(Message::text(
        json!({"cmd":"mute","ch":0,"muted":true}).to_string(),
    ))
    .await
    .unwrap();
    let st = wait_state(&mut rx2, |s| strip(s, 0).get("muted") == Some(&json!(true))).await;
    assert_eq!(st.get("by"), Some(&json!(by_fader)));

    // Param echo carries the same writer.
    tx1.send(Message::text(
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

/// Strip locks: holder pins, others are denied, holder writes through,
/// unlock releases, recall bypasses with a report, disconnect auto-releases.
///
/// c1 locks ch0 -> c2 fader/mute/param + lock + unlock all denied with
/// `locked_by`; c1 writes through; c1 unlocks -> c2 allowed. Then a
/// lock/recall round-trip proves `recall_scene` bypasses but reports
/// `overrode_locks`, and a disconnect proves auto-release.
#[tokio::test]
async fn ws_strip_locks_deny_unlock_recall_disconnect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
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

    async fn wait_locks(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for expected locks; last: {last}");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("locks") {
                continue;
            }
            last = v;
            if want(&last) {
                return last.clone();
            }
        }
    }

    async fn wait_error_locked(rx: &mut WsRx, holder: &str) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for locked_by={holder} error; last: {last}");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("error") {
                continue;
            }
            last = v;
            if last.get("locked_by") == Some(&json!(holder)) {
                return last.clone();
            }
        }
    }

    async fn wait_recalled(rx: &mut WsRx) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for recalled report");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) == Some("recalled") {
                return v;
            }
        }
    }

    async fn wait_shows_with(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for shows; last: {last}");
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

    fn holds_ch(locks: &Value, ch: u64) -> Option<String> {
        locks
            .get("holds")?
            .get(ch.to_string())?
            .as_str()
            .map(str::to_string)
    }

    // Two clients; drain the 4-frame snapshot (state/shows/clients/locks).
    let (mut tx1, mut rx1) = connect(format!("ws://{addr}/ws?name=alice")).await;
    let _ = next_typed(&mut rx1, "state").await;
    let _ = next_typed(&mut rx1, "shows").await;
    let _ = next_typed(&mut rx1, "clients").await;
    let _ = next_typed(&mut rx1, "locks").await;
    let (mut tx2, mut rx2) = connect(format!("ws://{addr}/ws?name=bob")).await;
    let _ = next_typed(&mut rx2, "state").await;
    let _ = next_typed(&mut rx2, "shows").await;
    let _ = next_typed(&mut rx2, "clients").await;
    let _ = next_typed(&mut rx2, "locks").await;
    // c1 sees the join (clients broadcast, count 2).
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for join broadcast");
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx1)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) == Some("clients")
                && v.get("count") == Some(&json!(2))
            {
                break;
            }
        }
    }

    // c1 locks ch0; both sides observe holds {"0": holder}.
    tx1.send(Message::text(json!({"cmd":"lock","ch":0}).to_string()))
        .await
        .unwrap();
    let l1 = wait_locks(&mut rx1, |v| holds_ch(v, 0).is_some()).await;
    let holder = holds_ch(&l1, 0).expect("holder id");
    let l2 = wait_locks(&mut rx2, |v| holds_ch(v, 0).as_deref() == Some(holder.as_str())).await;
    assert_eq!(holds_ch(&l2, 0).as_deref(), Some(holder.as_str()));

    // Locked strip rejects other writers' fader/mute/param with locked_by.
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":0,"value":0.9}).to_string(),
    ))
    .await
    .unwrap();
    let err = wait_error_locked(&mut rx2, &holder).await;
    assert_eq!(err.get("locked_by"), Some(&json!(holder)));

    tx2.send(Message::text(
        json!({"cmd":"mute","ch":0,"muted":true}).to_string(),
    ))
    .await
    .unwrap();
    let err = wait_error_locked(&mut rx2, &holder).await;
    assert_eq!(err.get("locked_by"), Some(&json!(holder)));

    tx2.send(Message::text(
        json!({"cmd":"param","ch":0,"field":"gain","value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let err = wait_error_locked(&mut rx2, &holder).await;
    assert_eq!(err.get("locked_by"), Some(&json!(holder)));

    // Contending lock + foreign unlock are denied the same way.
    tx2.send(Message::text(json!({"cmd":"lock","ch":0}).to_string()))
        .await
        .unwrap();
    let err = wait_error_locked(&mut rx2, &holder).await;
    assert_eq!(err.get("locked_by"), Some(&json!(holder)));
    tx2.send(Message::text(json!({"cmd":"unlock","ch":0}).to_string()))
        .await
        .unwrap();
    let err = wait_error_locked(&mut rx2, &holder).await;
    assert_eq!(err.get("locked_by"), Some(&json!(holder)));

    // Holder writes through on the pinned strip.
    tx1.send(Message::text(
        json!({"cmd":"fader","ch":0,"value":0.6}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx2, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.6).abs() < 1e-6)
    })
    .await;

    // Holder unlocks; both sides see the empty table; c2 writes freely.
    tx1.send(Message::text(json!({"cmd":"unlock","ch":0}).to_string()))
        .await
        .unwrap();
    let _ = wait_locks(&mut rx1, |v| holds_ch(v, 0).is_none()).await;
    let _ = wait_locks(&mut rx2, |v| holds_ch(v, 0).is_none()).await;
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":0,"value":0.1}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx1, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.1).abs() < 1e-6)
    })
    .await;

    // recall_scene bypasses locks but reports overrides. Capture ch1=0.3
    // while unlocked, pin it, move it as the holder, recall as the other.
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":1,"value":0.3}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx1, |s| {
        strip(s, 1)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.3).abs() < 1e-6)
    })
    .await;
    tx2.send(Message::text(
        json!({"cmd":"capture_scene","show":"gig","name":"verse"}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_shows_with(&mut rx2, |_| true).await;
    tx1.send(Message::text(json!({"cmd":"lock","ch":1}).to_string()))
        .await
        .unwrap();
    let locked = wait_locks(&mut rx2, |v| holds_ch(v, 1).is_some()).await;
    let holder1 = holds_ch(&locked, 1).expect("ch1 holder");
    assert_eq!(holder1, holder, "same allocator order: c1 still holder");
    tx1.send(Message::text(
        json!({"cmd":"fader","ch":1,"value":0.8}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx2, |s| {
        strip(s, 1)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.8).abs() < 1e-6)
    })
    .await;
    tx2.send(Message::text(
        json!({"cmd":"recall_scene","show":"gig","name":"verse","fade_ms":0}).to_string(),
    ))
    .await
    .unwrap();
    let reported = wait_recalled(&mut rx2).await;
    assert_eq!(
        reported.get("overrode_locks"),
        Some(&json!([1])),
        "recall reports the locked strip it changed"
    );
    let _ = wait_state(&mut rx2, |s| {
        strip(s, 1)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.3).abs() < 1e-6)
    })
    .await;

    // Disconnect auto-releases: c1 (holder of ch1) leaves, c2 sees the
    // pin vanish and can write the strip.
    tx1.send(Message::Close(None)).await.ok();
    let _ = wait_locks(&mut rx2, |v| holds_ch(v, 1).is_none()).await;
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":1,"value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx2, |s| {
        strip(s, 1)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.5).abs() < 1e-6)
    })
    .await;

    server.abort();
}

/// Strip-lock expiry uses a short test TTL (`run_with_lock_ttl`) instead
/// of waiting out the 60 s production [`openfader_serve::LOCK_TTL`]: c1 pins
/// ch2, c2 is denied, then after the TTL the pin auto-releases (sweeper
/// broadcast) and c2 writes freely.
#[tokio::test]
async fn ws_strip_locks_expiry() {
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run_with_lock_ttl(
        listener,
        openfader_serve::ServeOptions::default(),
        std::time::Duration::from_millis(200),
    ));

    let (stream1, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?name=alice"))
        .await
        .expect("WS connect");
    let (mut tx1, mut rx1) = stream1.split();
    let _ = next_typed(&mut rx1, "state").await;
    let _ = next_typed(&mut rx1, "shows").await;
    let _ = next_typed(&mut rx1, "clients").await;
    let _ = next_typed(&mut rx1, "locks").await;
    let (stream2, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws?name=bob"))
        .await
        .expect("WS connect");
    let (mut tx2, mut rx2) = stream2.split();
    let _ = next_typed(&mut rx2, "state").await;
    let _ = next_typed(&mut rx2, "shows").await;
    let _ = next_typed(&mut rx2, "clients").await;
    let _ = next_typed(&mut rx2, "locks").await;

    async fn wait_locks(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut last = json!(null);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for locks; last: {last}");
            }
            let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("locks") {
                continue;
            }
            last = v;
            if want(&last) {
                return last.clone();
            }
        }
    }

    tx1.send(Message::text(json!({"cmd":"lock","ch":2}).to_string()))
        .await
        .unwrap();
    let pinned = wait_locks(&mut rx1, |v| {
        v.get("holds")
            .and_then(|h| h.get("2"))
            .and_then(Value::as_str)
            .is_some()
    })
    .await;
    let holder = pinned
        .get("holds")
        .and_then(|h| h.get("2"))
        .and_then(Value::as_str)
        .expect("holder")
        .to_string();
    let _ = wait_locks(&mut rx2, |v| {
        v.get("holds")
            .and_then(|h| h.get("2"))
            .and_then(Value::as_str)
            .is_some_and(|h| h == holder)
    })
    .await;

    // Still pinned: c2 is denied.
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":2,"value":0.9}).to_string(),
    ))
    .await
    .unwrap();
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for locked_by error");
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx2)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) == Some("error")
                && v.get("locked_by") == Some(&json!(holder))
            {
                break;
            }
        }
    }

    // After the 200 ms TTL the sweeper releases + broadcasts; c2 writes.
    let _ = wait_locks(&mut rx2, |v| {
        v.get("holds")
            .and_then(|h| h.get("2"))
            .is_none()
    })
    .await;
    tx2.send(Message::text(
        json!({"cmd":"fader","ch":2,"value":0.4}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx2, |s| {
        strip(s, 2)
            .get("fader")
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.4).abs() < 1e-6)
    })
    .await;

    server.abort();
}

/// Main fader end-to-end (sim): `state` carries `main_fader`, the
/// `main_fader` command (and its `set_main_fader` legacy alias) drives it with 0.0..=1.0 validation, and invalid
/// values error without moving it.
#[tokio::test]
async fn ws_main_fader_set_and_state() {
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();

    fn main_of(state: &Value) -> Option<f64> {
        state.get("main_fader")?.as_f64()
    }

    // Initial snapshot carries the sim default main (0.8).
    let state = next_typed(&mut rx, "state").await;
    assert!(
        main_of(&state).is_some_and(|m| (m - 0.8).abs() < 1e-6),
        "state carries sim default main_fader 0.8, got {state}"
    );

    // Drive the main fader; the broadcast is writer-tagged like strips.
    tx.send(Message::text(
        json!({"cmd":"main_fader","value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let state = wait_state(&mut rx, |s| {
        main_of(s).is_some_and(|m| (m - 0.5).abs() < 1e-6)
    })
    .await;
    assert!(
        state.get("by").and_then(Value::as_str).is_some_and(|b| !b.is_empty()),
        "main_fader mutation is writer-tagged"
    );

    // `set_main_fader` alias (legacy command name) works the same.
    tx.send(Message::text(
        json!({"cmd":"set_main_fader","value":0.3}).to_string(),
    ))
    .await
    .unwrap();
    let _ = wait_state(&mut rx, |s| {
        main_of(s).is_some_and(|m| (m - 0.3).abs() < 1e-6)
    })
    .await;

    // Out-of-range is rejected and the fader does not move.
    tx.send(Message::text(
        json!({"cmd":"main_fader","value":1.5}).to_string(),
    ))
    .await
    .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some(), "range error explains itself");
    tx.send(Message::text(json!({"cmd":"list_shows"}).to_string()))
        .await
        .unwrap();
    // Drain to a fresh shows frame, then confirm main is still 0.3 via
    // the next state tick (mutations only broadcast on success, so no
    // state frame follows the rejected command, poll explicitly).
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for shows after rejected main_fader");
        }
        let next = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) == Some("shows") {
            break;
        }
    }
    tx.send(Message::text(json!({"cmd":"connect","driver":"sim"}).to_string()))
        .await
        .unwrap();
    let state = wait_state(&mut rx, |s| {
        main_of(s).is_some_and(|m| (m - 0.3).abs() < 1e-6)
    })
    .await;
    assert!(
        main_of(&state).is_some_and(|m| (m - 0.3).abs() < 1e-6),
        "rejected value did not move the main fader"
    );

    // Missing value errors too; connection stays alive.
    tx.send(Message::text(json!({"cmd":"main_fader"}).to_string()))
        .await
        .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some());

    server.abort();
}

/// Self-id (`you`): the first frame on connect is the unicast
/// `{"type":"you","id":"<client-N>"}`, preceding the initial `state`
/// snapshot, and that id later appears verbatim in `by` tags. The
/// `connect` command re-unicasts the same id.
#[tokio::test]
async fn ws_you_self_id_precedes_state_and_tags_writes() {
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();

    // First frame must be `you` (it precedes the initial state snapshot).
    let you = next_msg(&mut rx).await;
    assert_eq!(you.get("type"), Some(&serde_json::json!("you")));
    let me = you
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("you carries id")
        .to_string();
    assert!(!me.is_empty(), "self id is non-empty");

    // Snapshot follows.
    let state = next_typed(&mut rx, "state").await;
    assert_eq!(state.get("driver"), Some(&serde_json::json!("sim")));

    // Our own mutation comes back tagged with the `you` id.
    tx.send(Message::text(
        serde_json::json!({"cmd":"fader","ch":0,"value":0.5}).to_string(),
    ))
    .await
    .unwrap();
    let st = wait_state(&mut rx, |s| {
        strip(s, 0)
            .get("fader")
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|f| (f - 0.5).abs() < 1e-6)
    })
    .await;
    assert_eq!(
        st.get("by").and_then(serde_json::Value::as_str),
        Some(me.as_str()),
        "by tag matches the you id"
    );

    // `connect` re-unicasts the same self id (plus a state broadcast).
    tx.send(Message::text(
        serde_json::json!({"cmd":"connect","driver":"sim"}).to_string(),
    ))
    .await
    .unwrap();
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_you = false;
        let mut saw_state = false;
        while !(saw_you && saw_state) {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for you + state after connect (you={saw_you}, state={saw_state})");
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").and_then(serde_json::Value::as_str) {
                Some("you") if v.get("id") == Some(&serde_json::json!(me)) => {
                    saw_you = true;
                }
                Some("state")
                    if v.get("by").and_then(serde_json::Value::as_str) == Some(me.as_str()) =>
                {
                    saw_state = true;
                }
                _ => {}
            }
        }
    }

    // A second client gets a distinct id.
    let (stream2, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("second WS connect");
    let (_, mut rx2) = stream2.split();
    let you2 = next_msg(&mut rx2).await;
    assert_eq!(you2.get("type"), Some(&serde_json::json!("you")));
    let other = you2
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("second you carries id");
    assert_ne!(other, me, "client ids are distinct");

    server.abort();
}

/// Next `params` frame matching `want` (skips meters/spectrum/gr/state).
async fn wait_params(rx: &mut WsRx, want: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let mut last = json!(null);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for expected params frame; last: {last}");
        }
        let next = timeout(Duration::from_millis(500), next_msg(rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) != Some("params") {
            continue;
        }
        last = v;
        if want(&last) {
            return last.clone();
        }
    }
}

/// Param plane over WS against the sim driver: `caps` in the initial
/// snapshot (full v2 catalogue, nothing unsupported), a `connect`
/// command re-broadcasts `caps`, a `param` write echoes the confirmed
/// (re-read) value, and a second connection gets a full `params`
/// snapshot including the written value.
#[tokio::test]
async fn ws_caps_and_params_snapshot() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run(
        listener,
        openfader_serve::ServeOptions::default(),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();

    // Initial snapshot ships the slim caps (essentials lane only 
    // first paint never waits for the catalogue); hello completes the
    // full catalogue (unicast, before the ack).
    let caps = next_full_caps(&mut tx, &mut rx).await;
    assert_eq!(caps.get("driver"), Some(&json!("sim")));
    let params = caps.get("params").and_then(Value::as_array).unwrap();
    assert_eq!(params.len(), 186, "sim caps include stereo balance and six VCA memberships");
    for p in params {
        for key in ["id", "kind", "scope", "scopes", "min", "max", "unit", "default", "label"] {
            assert!(p.get(key).is_some(), "caps entry misses {key}: {p}");
        }
    }
    assert_eq!(
        caps.get("unsupported"),
        Some(&json!([])),
        "sim supports everything"
    );

    // A `connect` command re-broadcasts caps to every client, plus the
    // state echo. State and caps ride separate broadcasts so their order
    // is a race, collect both in either order instead of assuming caps
    // lands first (consuming the lone state echo while scanning for caps
    // would starve the drain below).
    {
        use tokio_tungstenite::tungstenite::Message;
        tx.send(Message::text(
            json!({"cmd":"connect","driver":"sim"}).to_string(),
        ))
        .await
        .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_caps = false;
        let mut saw_state = false;
        while !(saw_caps && saw_state) {
            if std::time::Instant::now() > deadline {
                panic!(
                    "timed out waiting for connect caps + state echo (caps={saw_caps}, state={saw_state})"
                );
            }
            let next = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            match v.get("type").and_then(Value::as_str) {
                Some("caps") => {
                    assert_eq!(v.get("driver"), Some(&json!("sim")));
                    assert_eq!(
                        v.get("params").and_then(Value::as_array).map(Vec::len),
                        Some(186)
                    );
                    saw_caps = true;
                }
                Some("state") => {
                    saw_state = true;
                }
                _ => {}
            }
        }
    }

    // `param` write: the echo carries the confirmed (re-read) value.
    {
        use tokio_tungstenite::tungstenite::Message;
        tx.send(Message::text(
            json!({"cmd":"param","ch":0,"field":"gain","value":12.5}).to_string(),
        ))
        .await
        .unwrap();
        wait_param(&mut rx, 0, "gain", &json!(12.5)).await;
    }

    // A second connection gets caps plus a full params snapshot with it.
    let (stream2, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("second WS connect");
    let (_, mut rx2) = stream2.split();
    let caps3 = next_typed(&mut rx2, "caps").await;
    assert_eq!(caps3.get("driver"), Some(&json!("sim")));
    let snap = wait_params(&mut rx2, |v| {
        v.get("ch") == Some(&json!(0))
            && v.get("values")
                .and_then(|vals| vals.get("gain"))
                .and_then(Value::as_f64)
                .is_some_and(|g| (g - 12.5).abs() < 1e-6)
    })
    .await;
    assert_eq!(snap.get("ch"), Some(&json!(0)));

    server.abort();
}

/// Param-plane diff over WS: a change made outside the param-write path
/// (here a `fader` command, which does not touch the param cache) surfaces
/// as a `params` frame from the background poller. Fast test poll interval
/// (1 ms) keeps the sweep to ~2 s worst case.
#[tokio::test]
async fn ws_params_diff_broadcast() {
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run_with_poll(
        listener,
        openfader_serve::ServeOptions::default(),
        openfader_serve::LOCK_TTL,
        Duration::from_millis(1),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    let _ = next_typed(&mut rx, "caps").await;

    tx.send(Message::text(
        json!({"cmd":"fader","ch":5,"value":0.6}).to_string(),
    ))
    .await
    .unwrap();
    // The confirmed singular `param` echo for the write arrives first (the
    // write primes the poller cache, so no background `params` diff
    // follows, same contract as `engine_api::params_diff_broadcast`)...
    let echo = {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if std::time::Instant::now() > deadline {
                panic!("timed out waiting for param echo");
            }
            let v = timeout(Duration::from_millis(500), next_msg(&mut rx)).await;
            let v = match v {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) == Some("param")
                && v.get("ch") == Some(&json!(5))
                && v.get("field") == Some(&json!("fader"))
            {
                break v;
            }
        }
    };
    assert!((echo.get("value").and_then(Value::as_f64).unwrap() - 0.6).abs() < 1e-6);
    wait_state(&mut rx, |s| {
        s.get("strips")
            .and_then(Value::as_array)
            .and_then(|strips| {
                strips
                    .iter()
                    .find(|s| s.get("ch") == Some(&json!(5)))
            })
            .and_then(|s| s.get("fader"))
            .and_then(Value::as_f64)
            .is_some_and(|f| (f - 0.6).abs() < 1e-6)
    })
    .await;

    server.abort();
}

/// Focus jumps the poll queue over WS: after `focus` ch5, all of ch5's
/// supported params arrive before ch0's complete set (ch0 still partial
/// at that moment); the `chs` array form works the same; out-of-range
/// focus yields an error frame and the socket stays open. Fast test poll
/// interval (2 ms) so a channel block drains in ~100 ms.
#[tokio::test]
async fn ws_focus_prioritizes_channel() {
    use std::collections::{HashMap, HashSet};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run_with_poll(
        listener,
        openfader_serve::ServeOptions::default(),
        openfader_serve::LOCK_TTL,
        Duration::from_millis(2),
    ));

    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    // Slim attach caps + hello-completed full catalogue (see
    // `next_full_caps`).
    let caps = next_full_caps(&mut tx, &mut rx).await;
    let params = caps
        .get("params")
        .and_then(Value::as_array)
        .expect("caps carries params");
    let nparams = params.len();
    // v2 catalogue size (was 54 in v1).
    assert_eq!(nparams, 186, "sim caps include stereo balance and six VCA memberships");
    // Channel strips (ch5/ch7 below) only ever receive the In-scoped
    // subset, bus/main-only EQ bands 5..6 never land on a channel
    // (firmware band counts).
    let nchan = params
        .iter()
        .filter(|p| {
            p.get("scopes")
                .and_then(Value::as_array)
                .map(|s| s.iter().any(|k| k.as_str() == Some("in")))
                .unwrap_or(false)
        })
        .count();
    assert!(nchan < nparams && nchan > 100, "chan-scoped subset sane: {nchan}");

    // Merge params frames into per-channel key sets until `ch` is complete.
    async fn wait_channel_complete(
        rx: &mut WsRx,
        keys: &mut HashMap<u64, HashSet<String>>,
        ch: u64,
        nparams: usize,
        what: &str,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while keys.get(&ch).map(HashSet::len).unwrap_or(0) < nparams {
            if std::time::Instant::now() > deadline {
                panic!(
                    "timed out waiting for ch{ch} full set ({what}); progress: {:?}",
                    keys.get(&ch).map(HashSet::len)
                );
            }
            let next = timeout(std::time::Duration::from_millis(500), next_msg(rx)).await;
            let v = match next {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("type").and_then(Value::as_str) != Some("params") {
                continue;
            }
            let Some(got_ch) = v.get("ch").and_then(Value::as_u64) else {
                continue;
            };
            let entry = keys.entry(got_ch).or_default();
            if let Some(values) = v.get("values").and_then(Value::as_object) {
                entry.extend(values.keys().cloned());
            }
        }
    }

    // Jump the queue to channel 5 before its round-robin slot.
    tx.send(Message::text(
        json!({"cmd":"focus","ch":5}).to_string(),
    ))
    .await
    .unwrap();
    let mut keys: HashMap<u64, HashSet<String>> = HashMap::new();
    wait_channel_complete(&mut rx, &mut keys, 5, nchan, "focus ch5").await;
    let ch0 = keys.get(&0).map(HashSet::len).unwrap_or(0);
    assert!(
        ch0 < nchan,
        "ch5 completed ({nchan} params) while ch0 still partial ({ch0})"
    );

    // The `chs` array form focuses channel 7 the same way.
    tx.send(Message::text(
        json!({"cmd":"focus","chs":[7]}).to_string(),
    ))
    .await
    .unwrap();
    wait_channel_complete(&mut rx, &mut keys, 7, nchan, "focus chs [7]").await;

    // Out-of-range focus: error frame, socket stays open.
    tx.send(Message::text(json!({"cmd":"focus","ch":999}).to_string()))
        .await
        .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(err.get("message").is_some(), "focus error explains itself");
    tx.send(Message::text(json!({"cmd":"list_shows"}).to_string()))
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("socket died after out-of-range focus");
        }
        let next = timeout(std::time::Duration::from_millis(500), next_msg(&mut rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) == Some("shows") {
            break;
        }
    }

    // Mixed valid + out-of-range: error for the ignored one, the valid
    // channel still jumps the queue.
    tx.send(Message::text(
        json!({"cmd":"focus","chs":[3, 999]}).to_string(),
    ))
    .await
    .unwrap();
    let err = next_typed(&mut rx, "error").await;
    assert!(
        err.get("message").is_some_and(|m| m.as_str().is_some_and(|s| s.contains("999"))),
        "mixed focus error names the ignored channel: {err}"
    );
    wait_channel_complete(&mut rx, &mut keys, 3, nchan, "mixed focus ch3").await;

    server.abort();
}

/// Focus flood over WS (CATALOGUE-V2 §5): 30 focus commands flood the
/// queue, then focusing strip 33 makes its whole set drain first, ahead
/// of the flood backlog. A second phase at the default 50 ms budget shows
/// the first focused params landing within ~3 s.
#[tokio::test]
async fn ws_focus_flood_superseded() {
    use std::collections::{HashMap, HashSet};
    use tokio_tungstenite::tungstenite::Message;

    // Phase 1 (fast poll): strip 33's full set drains before any flooded
    // channel completes.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run_with_poll(
        listener,
        openfader_serve::ServeOptions::default(),
        openfader_serve::LOCK_TTL,
        Duration::from_millis(2),
    ));
    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    // Slim attach caps + hello-completed full catalogue (see
    // `next_full_caps`).
    let caps = next_full_caps(&mut tx, &mut rx).await;
    let params = caps.get("params").and_then(Value::as_array).unwrap();
    // Strip 40 is an extended input (Ch33): it supports the In-scoped
    // subset (bus/main-only EQ bands 5..6 never land on a channel).
    let nparams = params
        .iter()
        .filter(|p| {
            p.get("scopes")
                .and_then(Value::as_array)
                .map(|s| s.iter().any(|k| k.as_str() == Some("in")))
                .unwrap_or(false)
        })
        .count();
    for i in 0..30u64 {
        tx.send(Message::text(json!({"cmd":"focus","ch": i % 32}).to_string()))
            .await
            .unwrap();
    }
    tx.send(Message::text(json!({"cmd":"focus","ch":40}).to_string()))
        .await
        .unwrap();
    let mut keys: HashMap<u64, HashSet<String>> = HashMap::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while keys.get(&40).map(HashSet::len).unwrap_or(0) < nparams {
        if std::time::Instant::now() > deadline {
            panic!("flooded focus never yielded to ch40: {:?} ", keys.get(&40).map(HashSet::len));
        }
        let next = timeout(std::time::Duration::from_millis(500), next_msg(&mut rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) != Some("params") {
            continue;
        }
        let Some(got) = v.get("ch").and_then(Value::as_u64) else { continue };
        if let Some(values) = v.get("values").and_then(Value::as_object) {
            keys.entry(got).or_default().extend(values.keys().cloned());
        }
    }
    // No flooded channel completed first.
    for (ch, set) in &keys {
        if *ch != 40 {
            assert!(set.len() < nparams, "ch{ch} completed before ch40");
        }
    }
    server.abort();

    // Phase 2 (default 50 ms budget): the first focused params land well
    // within ~3 s despite the flood backlog.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(openfader_serve::run_with_poll(
        listener,
        openfader_serve::ServeOptions::default(),
        openfader_serve::LOCK_TTL,
        Duration::from_millis(50),
    ));
    let (stream, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .expect("WS connect");
    let (mut tx, mut rx) = stream.split();
    let _ = next_typed(&mut rx, "caps").await;
    for i in 0..30u64 {
        tx.send(Message::text(json!({"cmd":"focus","ch": i % 32}).to_string()))
            .await
            .unwrap();
    }
    tx.send(Message::text(json!({"cmd":"focus","ch":40}).to_string()))
        .await
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("no focused ch40 params within 3 s at the default budget");
        }
        let next = timeout(std::time::Duration::from_millis(500), next_msg(&mut rx)).await;
        let v = match next {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(Value::as_str) == Some("params")
            && v.get("ch").and_then(Value::as_u64) == Some(40)
        {
            break;
        }
    }
    server.abort();
}
