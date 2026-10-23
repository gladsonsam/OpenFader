//! Engine API tests: drive the engine in-process (no sockets) through
//! `attach`/`send`/`events` with the same scenarios as the serve WS
//! integration tests, so future adapters are covered by construction.

use std::{sync::Arc, time::Duration};

use openmix_core::access::Policy;
use openmix_engine::{ClientHandle, Cmd, Engine, EngineConfig, Event, SharedEvent};
use serde_json::{json, Value};

fn ev_json(event: &Event) -> Value {
    serde_json::to_value(event).unwrap()
}

async fn new_engine() -> Arc<Engine> {
    let engine = Engine::new(EngineConfig::default());
    engine.connect("sim", None).await.unwrap();
    engine
}

async fn attach(engine: &Arc<Engine>) -> ClientHandle {
    engine.attach(None).await
}

async fn next_event(handle: &mut ClientHandle) -> SharedEvent {
    tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
        .await
        .expect("event in time")
        .expect("mailbox open")
}

/// Next event matching `want`, skipping interleaved frames (meters, …).
async fn wait_for(handle: &mut ClientHandle, want: impl Fn(&Event) -> bool) -> SharedEvent {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for matching event");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), handle.events.recv()).await;
        let event = match next {
            Ok(Some(e)) => e,
            _ => continue,
        };
        if want(&event) {
            return event;
        }
    }
}

/// Drain the 6-frame attach snapshot (you/state/shows/clients/locks/caps),
/// returning it in order. Fails when a params frame arrives first (cache
/// must start empty) — snapshot order is part of the contract.
async fn drain_snapshot(handle: &mut ClientHandle) -> Vec<SharedEvent> {
    let mut out = Vec::new();
    for _ in 0..6 {
        out.push(next_event(handle).await);
    }
    let tags: Vec<&str> = out.iter().map(|e| e.tag()).collect();
    assert_eq!(
        tags,
        ["you", "state", "shows", "clients", "locks", "caps"],
        "snapshot order"
    );
    out
}

#[tokio::test]
async fn attach_snapshot_order_and_self_id() {
    let engine = new_engine().await;
    let mut client = attach(&engine).await;
    let snap = drain_snapshot(&mut client).await;
    let you_id = snap[0].clone();
    let Event::You { id } = &*you_id else {
        panic!("first frame is you");
    };
    assert_eq!(id, &client.id, "you id matches the handle id");
    assert!(!client.id.is_empty());
    // State carries the sim default main fader (0.8).
    assert_eq!(ev_json(&snap[1]).get("driver"), Some(&json!("sim")));
}

#[tokio::test]
async fn connect_fader_mute_param_roundtrip() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    // Explicit connect re-selects sim and unicasts our id back.
    a.send(Cmd::Connect {
        driver: "sim".to_string(),
        addr: None,
    })
    .await
    .unwrap();
    let you = wait_for(&mut a, |e| matches!(e, Event::You { .. })).await;
    assert_eq!(ev_json(&you).get("id"), Some(&json!(a.id)));

    // Fader: ch0 -> 0.5, writer-tagged.
    a.send(Cmd::Fader { ch: 0, value: 0.5, key: None}).await.unwrap();
    let st = wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;
    let stv = ev_json(&st);
    assert_eq!(stv.get("by"), Some(&json!(a.id)));

    // Mute on/off.
    a.send(Cmd::Mute { ch: 0, muted: true, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| s.muted),
        _ => false,
    })
    .await;
    a.send(Cmd::Mute { ch: 0, muted: false, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| !s.muted),
        _ => false,
    })
    .await;

    // Param write echoes the confirmed (re-read) value, writer-tagged.
    a.send(Cmd::Param {
        ch: 1,
        field: "gain".to_string(),
        value: json!(12.5), key: None})
    .await
    .unwrap();
    let echo = wait_for(&mut a, |e| match e {
        Event::Param { ch, field, value, .. } => {
            *ch == 1
                && field == "gain"
                && value.as_f64().is_some_and(|v| (v - 12.5).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&echo).get("by"), Some(&json!(a.id)));

    // Unknown driver errors to the sender only; the session stays alive.
    a.send(Cmd::Connect {
        driver: "nope".to_string(),
        addr: None,
    })
    .await
    .unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
    a.send(Cmd::ListShows).await.unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Shows { .. })).await;
}

#[tokio::test]
async fn caps_and_params_snapshot() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    let snap = drain_snapshot(&mut a).await;
    let caps = ev_json(&snap[5].clone());
    assert_eq!(caps.get("driver"), Some(&json!("sim")));
    // Attach ships the slim caps (essentials lane only — first paint
    // never waits for the catalogue); the full catalogue follows on the
    // first hello as a unicast rebroadcast.
    let params = caps.get("params").and_then(Value::as_array).unwrap();
    assert_eq!(
        params.len(),
        6,
        "attach caps carry the essentials lane only (got {params:?})"
    );
    let ids: Vec<&str> = params
        .iter()
        .filter_map(|p| p.get("id").and_then(Value::as_str))
        .collect();
    for id in ["fader", "mute", "name", "pan", "balance", "color"] {
        assert!(ids.contains(&id), "slim caps carry {id}");
    }
    assert_eq!(caps.get("unsupported"), Some(&json!([])));

    // First hello completes the catalogue (unicast, before the ack).
    a.send(Cmd::Hello { binary_meters: None, taps: None })
        .await
        .unwrap();
    let full = wait_for(&mut a, |e| matches!(e, Event::Caps { .. })).await;
    let full_json = ev_json(&full);
    let full_params = full_json.get("params").and_then(Value::as_array).unwrap();
    assert_eq!(full_params.len(), openmix_core::params::CATALOGUE.len() + 1, "hello completes the catalogue plus stereo balance");
    // The ack still follows, and a second hello is a pure re-ack (no
    // second catalogue — the flag is per client).
    wait_for(&mut a, |e| matches!(e, Event::Hello { .. })).await;
    a.send(Cmd::Hello { binary_meters: None, taps: None })
        .await
        .unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Hello { .. })).await;

    // A param write, then a second attach sees the full snapshot with it.
    a.send(Cmd::Param {
        ch: 0,
        field: "gain".to_string(),
        value: json!(12.5), key: None})
    .await
    .unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Param { .. })).await;

    let mut b = attach(&engine).await;
    drain_snapshot(&mut b).await;
    let seen = wait_for(&mut b, |e| match e {
        Event::Params { ch, values, ..} => {
            *ch == 0
                && values
                    .get("gain")
                    .and_then(Value::as_f64)
                    .is_some_and(|g| (g - 12.5).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&seen).get("ch"), Some(&json!(0)));
}

#[tokio::test]
async fn params_diff_broadcast() {
    let engine = Engine::new(EngineConfig {
        poll_interval: Duration::from_millis(1),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    // A fader move carries its confirmed value as a singular `param` echo
    // (same as the param-write path), which also primes the poller cache;
    // the follow-up `state` still carries the strip.
    a.send(Cmd::Fader { ch: 5, value: 0.6, key: None}).await.unwrap();
    let echo = wait_for(&mut a, |e| match e {
        Event::Param { ch, field, value, .. } => {
            *ch == 5
                && field == "fader"
                && value.as_f64().is_some_and(|f| (f - 0.6).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&echo).get("type"), Some(&json!("param")));
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 5)
            .is_some_and(|s| (s.fader - 0.6).abs() < 1e-6),
        _ => false,
    })
    .await;
}

#[tokio::test]
async fn focus_prioritizes_channel() {
    let engine = Engine::new(EngineConfig {
        poll_interval: Duration::from_millis(2),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    a.send(Cmd::Focus {
        ch: Some(5),
        chs: None,
    })
    .await
    .unwrap();

    // Merge params frames until ch5 is complete. Channels carry every
    // in-scope id (bus/main-only EQ bands 5..6 never land on a channel —
    // firmware band counts), so the target is the In-scoped catalogue set,
    // not the full catalogue.
    let mut seen: std::collections::HashMap<u16, std::collections::HashSet<String>> =
        Default::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let full = openmix_core::params::CATALOGUE
        .iter()
        .filter(|d| openmix_core::params::supports_kind(d, openmix_core::StripKind::In))
        .count();
    while seen.get(&5).map(|s: &std::collections::HashSet<String>| s.len()).unwrap_or(0) < full {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for ch5 full set; progress: {seen:?}");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), a.events.recv()).await;
        let Ok(Some(frame)) = next else {
            continue;
        };
        let Event::Params { ch, values, .. } = &*frame else {
            continue;
        };
        seen.entry(*ch).or_default().extend(values.keys().cloned());
    }
    let ch0 = seen.get(&0).map(|s| s.len()).unwrap_or(0);
    assert!(ch0 < full, "ch5 completed ({full} params) while ch0 still partial ({ch0})");

    // The `chs` array form focuses channel 7 the same way.
    a.send(Cmd::Focus {
        ch: None,
        chs: Some(vec![json!(7)]),
    })
    .await
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while seen.get(&7).map(|s: &std::collections::HashSet<String>| s.len()).unwrap_or(0) < full {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for ch7 full set");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), a.events.recv()).await;
        let Ok(Some(frame)) = next else {
            continue;
        };
        let Event::Params { ch, values, .. } = &*frame else {
            continue;
        };
        seen.entry(*ch).or_default().extend(values.keys().cloned());
    }

    // Out-of-range focus: error event, session stays alive.
    a.send(Cmd::Focus {
        ch: Some(999),
        chs: None,
    })
    .await
    .unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
    a.send(Cmd::ListShows).await.unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Shows { .. })).await;

    // Mixed valid + out-of-range: error names the ignored channel, the
    // valid channel still jumps the queue.
    a.send(Cmd::Focus {
        ch: None,
        chs: Some(vec![json!(3), json!(999)]),
    })
    .await
    .unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err)
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("999")),
        "mixed focus error names the ignored channel"
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while seen.get(&3).map(|s: &std::collections::HashSet<String>| s.len()).unwrap_or(0) < full {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for ch3 full set");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), a.events.recv()).await;
        let Ok(Some(frame)) = next else {
            continue;
        };
        let Event::Params { ch, values, .. } = &*frame else {
            continue;
        };
        seen.entry(*ch).or_default().extend(values.keys().cloned());
    }
}

#[tokio::test]
async fn locks_deny_allow_recall_disconnect() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;
    let mut b = attach(&engine).await;
    drain_snapshot(&mut b).await;
    // Join roster: a sees the second attach.
    let joined = wait_for(&mut a, |e| match e {
        Event::Clients { count, .. } => *count == 2,
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&joined).get("count"), Some(&json!(2)));
    let holder = a.id.clone();

    // a locks ch0; both sides observe holds {"0": holder}.
    a.send(Cmd::Lock { ch: 0, key: None}).await.unwrap();
    let l1 = wait_for(&mut a, |e| match e {
        Event::Locks { holds } => holds.get("0") == Some(&holder),
        _ => false,
    })
    .await;
    assert_eq!(
        ev_json(&l1).get("holds").and_then(|h| h.get("0")),
        Some(&json!(holder))
    );
    wait_for(&mut b, |e| match e {
        Event::Locks { holds } => holds.get("0") == Some(&holder),
        _ => false,
    })
    .await;

    // Locked strip rejects other writers with locked_by.
    for cmd in [
        Cmd::Fader { ch: 0, value: 0.9, key: None},
        Cmd::Mute { ch: 0, muted: true, key: None},
        Cmd::Param {
            ch: 0,
            field: "gain".to_string(),
            value: json!(0.5), key: None},
        Cmd::Lock { ch: 0, key: None},
        Cmd::Unlock { ch: 0, key: None},
    ] {
        b.send(cmd).await.unwrap();
        let err = wait_for(&mut b, |e| match e {
            Event::Error { locked_by, .. } => locked_by.as_deref() == Some(holder.as_str()),
            _ => false,
        })
        .await;
        assert_eq!(
            ev_json(&err).get("locked_by"),
            Some(&json!(holder)),
            "denial names the holder"
        );
    }

    // Holder writes through; sibling sees it.
    a.send(Cmd::Fader { ch: 0, value: 0.6, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.6).abs() < 1e-6),
        _ => false,
    })
    .await;

    // Holder unlocks; both see the empty table; b writes freely.
    a.send(Cmd::Unlock { ch: 0, key: None}).await.unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Locks { holds } if holds.is_empty())).await;
    wait_for(&mut b, |e| matches!(e, Event::Locks { holds } if holds.is_empty())).await;
    b.send(Cmd::Fader { ch: 0, value: 0.1, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.1).abs() < 1e-6),
        _ => false,
    })
    .await;

    // recall_scene bypasses locks but reports overrides.
    b.send(Cmd::Fader { ch: 1, value: 0.3, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 1)
            .is_some_and(|s| (s.fader - 0.3).abs() < 1e-6),
        _ => false,
    })
    .await;
    b.send(Cmd::CaptureScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
    })
    .await
    .unwrap();
    wait_for(&mut b, |e| matches!(e, Event::Shows { .. })).await;
    a.send(Cmd::Lock { ch: 1, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::Locks { holds } => holds.get("1") == Some(&holder),
        _ => false,
    })
    .await;
    a.send(Cmd::Fader { ch: 1, value: 0.8, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 1)
            .is_some_and(|s| (s.fader - 0.8).abs() < 1e-6),
        _ => false,
    })
    .await;
    b.send(Cmd::RecallScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
        fade_ms: Some(0),
    })
    .await
    .unwrap();
    let reported = wait_for(&mut b, |e| matches!(e, Event::Recalled { .. })).await;
    assert_eq!(
        ev_json(&reported).get("overrode_locks"),
        Some(&json!([1])),
        "recall reports the locked strip it changed"
    );
    wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 1)
            .is_some_and(|s| (s.fader - 0.3).abs() < 1e-6),
        _ => false,
    })
    .await;

    // Disconnect auto-releases: dropping a (holder of ch1) lets b see the
    // pin vanish and write the strip.
    drop(a);
    wait_for(&mut b, |e| match e {
        Event::Locks { holds } => holds.get("1").is_none(),
        _ => false,
    })
    .await;
    b.send(Cmd::Fader { ch: 1, value: 0.5, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 1)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;
}

#[tokio::test]
async fn locks_expire() {
    let engine = Engine::new(EngineConfig {
        lock_ttl: Duration::from_millis(200),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;
    let mut b = attach(&engine).await;
    drain_snapshot(&mut b).await;

    a.send(Cmd::Lock { ch: 2, key: None}).await.unwrap();
    let pinned = wait_for(&mut a, |e| match e {
        Event::Locks { holds } => holds.get("2").is_some(),
        _ => false,
    })
    .await;
    let holder = ev_json(&pinned)
        .get("holds")
        .and_then(|h| h.get("2"))
        .and_then(Value::as_str)
        .expect("holder")
        .to_string();
    wait_for(&mut b, |e| match e {
        Event::Locks { holds } => holds.get("2") == Some(&holder),
        _ => false,
    })
    .await;

    // Still pinned: b is denied.
    b.send(Cmd::Fader { ch: 2, value: 0.9, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::Error { locked_by, .. } => locked_by.as_deref() == Some(holder.as_str()),
        _ => false,
    })
    .await;

    // After the TTL the sweeper releases + broadcasts; b writes freely.
    wait_for(&mut b, |e| match e {
        Event::Locks { holds } => holds.get("2").is_none(),
        _ => false,
    })
    .await;
    b.send(Cmd::Fader { ch: 2, value: 0.4, key: None}).await.unwrap();
    wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 2)
            .is_some_and(|s| (s.fader - 0.4).abs() < 1e-6),
        _ => false,
    })
    .await;
}

#[tokio::test]
async fn shows_capture_recall_delete() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    a.send(Cmd::Fader { ch: 0, value: 0.5, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;
    a.send(Cmd::CaptureScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
    })
    .await
    .unwrap();
    let shows = wait_for(&mut a, |e| match e {
        Event::Shows { shows } => shows.iter().any(|s| {
            s.name == "gig" && s.scenes.iter().any(|sc| sc.name == "verse")
        }),
        _ => false,
    })
    .await;
    let shows_v = ev_json(&shows);
    let scene = shows_v
        .get("shows")
        .and_then(Value::as_array)
        .and_then(|l| l.iter().find(|s| s.get("name") == Some(&json!("gig"))))
        .and_then(|s| s.get("scenes"))
        .and_then(Value::as_array)
        .and_then(|l| l.iter().find(|s| s.get("name") == Some(&json!("verse"))))
        .expect("captured scene");
    let cap_fader = scene
        .get("strips")
        .and_then(Value::as_array)
        .and_then(|ss| ss.iter().find(|s| s.get("ch") == Some(&json!(0))))
        .and_then(|s| s.get("fader"))
        .and_then(Value::as_f64)
        .expect("scene carries ch0 fader");
    assert!((cap_fader - 0.5).abs() < 1e-6, "captured live fader");
    assert!(
        scene.get("main_fader").is_some(),
        "captured scene has a main_fader field"
    );

    // Library survives a fresh attach (server-side, no local copy).
    let mut peer = attach(&engine).await;
    let peer_snap = drain_snapshot(&mut peer).await;
    assert!(
        ev_json(&peer_snap[2]).get("shows").and_then(Value::as_array).is_some_and(|l| l
            .iter()
            .any(|s| s.get("name") == Some(&json!("gig")))),
        "second attach sees the server-side library"
    );

    // Explicit pull also returns the library.
    a.send(Cmd::ListShows).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::Shows { shows } => shows.iter().any(|s| {
            s.name == "gig" && s.scenes.iter().any(|sc| sc.name == "verse")
        }),
        _ => false,
    })
    .await;

    // Move away, recall with fade 0: state snap-back + per-param echo.
    let moved = ((cap_fader + 0.3).min(1.0)) as f32;
    a.send(Cmd::Fader { ch: 0, value: moved, key: None}).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - moved).abs() < 1e-6f32),
        _ => false,
    })
    .await;
    a.send(Cmd::RecallScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
        fade_ms: Some(0),
    })
    .await
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut saw_param = false;
    let mut saw_state = false;
    while !(saw_param && saw_state) {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for recall echo + state");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), a.events.recv()).await;
        let Ok(Some(e)) = next else { continue };
        match &*e {
            Event::Param { ch, field, value, .. } => {
                if *ch == 0
                    && field == "fader"
                    && value.as_f64().is_some_and(|f| (f - cap_fader).abs() < 1e-6)
                {
                    saw_param = true;
                }
            }
            Event::State { strips, .. }
                if strips
                    .iter()
                    .find(|s| s.ch == 0)
                    .is_some_and(|s| (s.fader as f64 - cap_fader).abs() < 1e-6)
                => {
                    saw_state = true;
                }
            _ => {}
        }
    }

    // Unknown scene errors; the session stays alive.
    a.send(Cmd::RecallScene {
        show: "gig".to_string(),
        name: "nope".to_string(),
        fade_ms: Some(0),
    })
    .await
    .unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());

    // Delete; the scene stays gone.
    a.send(Cmd::DeleteScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
    })
    .await
    .unwrap();
    wait_for(&mut a, |e| match e {
        Event::Shows { shows } => !shows.iter().any(|s| {
            s.name == "gig" && s.scenes.iter().any(|sc| sc.name == "verse")
        }),
        _ => false,
    })
    .await;
    a.send(Cmd::ListShows).await.unwrap();
    let listed = wait_for(&mut a, |e| matches!(e, Event::Shows { .. })).await;
    assert!(
        ev_json(&listed).get("shows").and_then(Value::as_array).is_some_and(|l| !l
            .iter()
            .any(|s| s.get("scenes").and_then(Value::as_array).is_some_and(|sc| sc
                .iter()
                .any(|e| e.get("name") == Some(&json!("verse")))))),
        "deleted scene stays gone"
    );
}

#[tokio::test]
async fn main_fader_set_and_state() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    let snap = drain_snapshot(&mut a).await;
    assert!(
        ev_json(&snap[1]).get("main_fader").and_then(Value::as_f64).is_some_and(|m| (m - 0.8).abs() < 1e-6),
        "snapshot carries the sim default main 0.8"
    );

    a.send(Cmd::MainFader { value: 0.5 }).await.unwrap();
    let st = wait_for(&mut a, |e| match e {
        Event::State { main_fader, .. } => {
            main_fader.is_some_and(|m| (m - 0.5).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&st).get("by"), Some(&json!(a.id)));

    // set_main_fader alias (legacy command name) works the same.
    a.send_json(r#"{"cmd":"set_main_fader","value":0.3}"#).await.unwrap();
    wait_for(&mut a, |e| match e {
        Event::State { main_fader, .. } => {
            main_fader.is_some_and(|m| (m - 0.3).abs() < 1e-6)
        }
        _ => false,
    })
    .await;

    // Out-of-range is rejected and the fader does not move.
    a.send(Cmd::MainFader { value: 1.5 }).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
    a.send(Cmd::Connect {
        driver: "sim".to_string(),
        addr: None,
    })
    .await
    .unwrap();
    let st = wait_for(&mut a, |e| match e {
        Event::State { main_fader, .. } => {
            main_fader.is_some_and(|m| (m - 0.3).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert!(
        ev_json(&st).get("main_fader").and_then(Value::as_f64).is_some_and(|m| (m - 0.3).abs() < 1e-6),
        "rejected value did not move the main fader"
    );

    // Missing value errors too; the session stays alive.
    a.send_json(r#"{"cmd":"main_fader"}"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
}

#[tokio::test]
async fn presence_writer_tag_and_unicast() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;
    let mut b = attach(&engine).await;
    drain_snapshot(&mut b).await;

    // a learns about the join via broadcast.
    let joined = wait_for(&mut a, |e| match e {
        Event::Clients { count, .. } => *count == 2,
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&joined).get("count"), Some(&json!(2)));

    // Mutation from a arrives at b tagged with the writer id.
    a.send(Cmd::Fader { ch: 0, value: 0.5, key: None}).await.unwrap();
    let st = wait_for(&mut b, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&st).get("by"), Some(&json!(a.id)));

    // Param echo carries the same writer.
    a.send(Cmd::Param {
        ch: 1,
        field: "fader".to_string(),
        value: json!(0.25), key: None})
    .await
    .unwrap();
    let echo = wait_for(&mut b, |e| match e {
        Event::Param { ch, field, value, .. } => {
            *ch == 1
                && field == "fader"
                && value.as_f64().is_some_and(|f| (f - 0.25).abs() < 1e-6)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&echo).get("by"), Some(&json!(a.id)));

    // A third attach observes a as a recent writer.
    let mut c = attach(&engine).await;
    let snap = drain_snapshot(&mut c).await;
    let writers = ev_json(&snap[3].clone());
    assert!(
        writers.get("writers").and_then(Value::as_array).is_some_and(|w| w.contains(&json!(a.id))),
        "writers include the recent mutator"
    );

    // Errors unicast: b's bad command reaches b only.
    b.send_json(r#"{"cmd":"bogus"}"#).await.unwrap();
    wait_for(&mut b, |e| matches!(e, Event::Error { .. })).await;
    let quiet = tokio::time::timeout(Duration::from_millis(300), a.events.recv()).await;
    assert!(
        !matches!(&quiet, Ok(Some(frame)) if matches!(&**frame, Event::Error { .. })),
        "a must not see b's error"
    );

    // Leave broadcast: dropping b brings the roster back to 2.
    let b_id = b.id.clone();
    drop(b);
    let left = wait_for(&mut a, |e| match e {
        Event::Clients { count, writers, .. } => {
            *count == 2 && !writers.contains(&b_id)
        }
        _ => false,
    })
    .await;
    assert_eq!(ev_json(&left).get("count"), Some(&json!(2)));
}

#[tokio::test]
async fn access_policy_musician_restrictions() {
    let engine = new_engine().await;
    let mut admin = attach(&engine).await;
    drain_snapshot(&mut admin).await;
    let mut mus = attach(&engine).await;
    drain_snapshot(&mut mus).await;
    assert!(engine.set_client_policy(&mus.id, Policy::musician([0])));
    assert!(!engine.set_client_policy("client-999", Policy::musician([0])));

    // Disallowed strip: fader + mute + param all denied.
    mus.send(Cmd::Fader { ch: 1, value: 0.5, key: None}).await.unwrap();
    let err = wait_for(&mut mus, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("not allowed")),
        "musician strip denial explains itself"
    );
    // Admin on the same strip is unaffected.
    admin.send(Cmd::Fader { ch: 1, value: 0.5, key: None}).await.unwrap();
    wait_for(&mut admin, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 1)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;

    // Fader cap: 0.9 exceeds the musician ceiling.
    mus.send(Cmd::Fader { ch: 0, value: 0.9, key: None}).await.unwrap();
    let err = wait_for(&mut mus, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("exceeds cap")),
        "musician cap denial explains itself"
    );
    // Under the cap: allowed.
    mus.send(Cmd::Fader { ch: 0, value: 0.5, key: None}).await.unwrap();
    wait_for(&mut mus, |e| match e {
        Event::State { strips, .. } => strips
            .iter()
            .find(|s| s.ch == 0)
            .is_some_and(|s| (s.fader - 0.5).abs() < 1e-6),
        _ => false,
    })
    .await;

    // Main LR is blocked for musicians.
    mus.send(Cmd::MainFader { value: 0.5 }).await.unwrap();
    let err = wait_for(&mut mus, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("main LR")),
        "musician main denial explains itself"
    );

    // Scene recall is blocked for musicians (admin captures first).
    admin
        .send(Cmd::CaptureScene {
            show: "gig".to_string(),
            name: "verse".to_string(),
        })
        .await
        .unwrap();
    wait_for(&mut admin, |e| matches!(e, Event::Shows { .. })).await;
    mus.send(Cmd::RecallScene {
        show: "gig".to_string(),
        name: "verse".to_string(),
        fade_ms: Some(0),
    })
    .await
    .unwrap();
    let err = wait_for(&mut mus, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("recall")),
        "musician recall denial explains itself"
    );
    // Admin recall works.
    admin
        .send(Cmd::RecallScene {
            show: "gig".to_string(),
            name: "verse".to_string(),
            fade_ms: Some(0),
        })
        .await
        .unwrap();
    wait_for(&mut admin, |e| matches!(e, Event::State { .. })).await;
}

#[tokio::test]
async fn meter_ticks_flow() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    let meters = wait_for(&mut a, |e| matches!(e, Event::Meters { .. })).await;
    let levels = ev_json(&meters).get("levels").and_then(Value::as_array).map(Vec::len);
    assert!(levels.is_some_and(|n| n > 0), "meters carry levels");

    let spec = wait_for(&mut a, |e| matches!(e, Event::Spectrum { .. })).await;
    let spec_v = ev_json(&spec);
    let ch = spec_v.get("ch").and_then(Value::as_u64).expect("spectrum ch");
    assert!(ch < 16, "spectrum capped at first 16 channels");
    let bins = spec_v.get("bins").and_then(Value::as_array).expect("bins");
    assert_eq!(bins.len(), 64, "spectrum carries 64 bins");
    for b in bins {
        let v = b.as_f64().expect("bins are numbers");
        assert!((0.0..=1.0).contains(&v), "bins normalized to [0,1]");
    }

    let gr = wait_for(&mut a, |e| matches!(e, Event::Gr { .. })).await;
    let gr_v = ev_json(&gr);
    assert!(gr_v.get("ch").and_then(Value::as_u64).is_some_and(|c| c < 16));
    for key in ["gate", "comp"] {
        assert!(
            gr_v.get(key).and_then(Value::as_f64).is_some_and(|v| v >= 0.0),
            "gr {key} is dB reduction >= 0"
        );
    }
}

#[tokio::test]
async fn raw_json_commands_and_invalid_input() {
    let engine = new_engine().await;
    let mut a = attach(&engine).await;
    drain_snapshot(&mut a).await;

    // Focus via raw JSON (both shapes), unknown cmd, invalid JSON:
    // errors unicast, session stays alive throughout.
    a.send_json(r#"{"cmd":"focus","chs":[0]}"#).await.unwrap();
    a.send_json(r#"{"cmd":"bogus"}"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("unknown cmd")),
        "unknown cmd keeps its message"
    );
    a.send_json(r#"{"cmd":"focus","ch":999}"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
    a.send_json(r#"not json"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(
        ev_json(&err).get("message").and_then(Value::as_str).is_some_and(|m| m.contains("invalid JSON")),
        "invalid JSON keeps its message"
    );
    a.send_json(r#"[1,2]"#).await.unwrap();
    let err = wait_for(&mut a, |e| matches!(e, Event::Error { .. })).await;
    assert!(ev_json(&err).get("message").is_some());
    a.send(Cmd::ListShows).await.unwrap();
    wait_for(&mut a, |e| matches!(e, Event::Shows { .. })).await;
}
