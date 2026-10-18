//! R2 engine tests: drive the engine in-process (no sockets) through
//! `attach`/`send`/`events` — devices/connection, info, numbered scenes
//! (roundtrip + persistence), FX, routing, offline editing, meter taps,
//! and poller pipelining/ordering.

use std::{sync::Arc, time::Duration};

use openmix_engine::{Cmd, Engine, EngineConfig, Event};
use serde_json::{json, Value};

fn ev_json(event: &Event) -> Value {
    serde_json::to_value(event).unwrap()
}

async fn new_engine() -> Arc<Engine> {
    let engine = Engine::new(EngineConfig::default());
    engine.connect("sim", None).await.unwrap();
    engine
}

async fn next_event(handle: &mut openmix_engine::engine::ClientHandle) -> Event {
    tokio::time::timeout(Duration::from_secs(5), handle.events.recv())
        .await
        .expect("event in time")
        .expect("mailbox open")
}

/// Next event matching `want`, skipping interleaved frames.
async fn wait_for(
    handle: &mut openmix_engine::engine::ClientHandle,
    want: impl Fn(&Event) -> bool,
) -> Event {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if std::time::Instant::now() > deadline {
            panic!("timed out waiting for matching event");
        }
        let next = tokio::time::timeout(Duration::from_millis(500), handle.events.recv()).await;
        match next {
            Ok(Some(e)) if want(&e) => return e,
            _ => continue,
        }
    }
}

/// Drain the 12-frame attach snapshot, asserting the R1 six-frame prefix
/// and the R2 suffix (devices/link/info/scenes/fx/routing).
async fn drain_snapshot(handle: &mut openmix_engine::engine::ClientHandle) -> Vec<Event> {
    let mut out = Vec::new();
    for _ in 0..12 {
        out.push(next_event(handle).await);
    }
    let tags: Vec<&str> = out.iter().map(Event::tag).collect();
    assert_eq!(
        &tags[..6],
        ["you", "state", "shows", "clients", "locks", "caps"],
        "R1 snapshot prefix unchanged"
    );
    assert_eq!(
        &tags[6..],
        ["devices", "link", "info", "scenes", "fx", "routing"],
        "R2 snapshot suffix"
    );
    out
}

fn is_error_containing(e: &Event, needle: &str) -> bool {
    matches!(e, Event::Error { message, .. } if message.contains(needle))
}

// ---------- devices / link / info ----------

#[tokio::test]
async fn scan_lists_design_devices() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    client.send(Cmd::Scan).await.unwrap();
    let event = wait_for(&mut client, |e| matches!(e, Event::Devices { .. })).await;
    let v = ev_json(&event);
    let list = v.get("list").and_then(Value::as_array).unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].get("name"), Some(&json!("FOH Console")));
    assert_eq!(list[0].get("ip"), Some(&json!("192.168.1.20")));
    assert_eq!(list[0].get("kind"), Some(&json!("console")));
    assert_eq!(list[0].get("state"), Some(&json!("connected")));
    assert_eq!(list[1].get("ip"), Some(&json!("192.168.1.21")));
    assert_eq!(list[1].get("state"), Some(&json!("available")));
    assert_eq!(list[2].get("name"), Some(&json!("Stage Box A")));
    assert_eq!(list[2].get("kind"), Some(&json!("io")));
    assert_eq!(list[2].get("state"), Some(&json!("in_use")));
    assert!(v.get("scanned_at").and_then(Value::as_u64).unwrap_or(0) > 0);
}

#[tokio::test]
async fn connect_device_moves_link() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    // I/O boxes cannot be mixing consoles.
    client
        .send(Cmd::ConnectDevice { id: "sb1".to_string() })
        .await
        .unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "I/O"));
    // Unknown ids error without dropping the session.
    client
        .send(Cmd::ConnectDevice { id: "nope".to_string() })
        .await
        .unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown device"));
    // Monitor console links.
    client
        .send(Cmd::ConnectDevice { id: "mon".to_string() })
        .await
        .unwrap();
    let link = wait_for(&mut client, |e| matches!(e, Event::Link { .. })).await;
    let v = ev_json(&link);
    assert_eq!(v.get("state"), Some(&json!("connected")));
    assert_eq!(v.get("name"), Some(&json!("Monitor Console")));
    assert_eq!(v.get("ip"), Some(&json!("192.168.1.21")));
    assert_eq!(v.get("fs"), Some(&json!(48000)));
    assert!(v.get("latency_ms").and_then(Value::as_f64).unwrap_or(0.0) > 0.0);
    // Devices now show mon connected, foh available (re-scan to see it).
    client.send(Cmd::Scan).await.unwrap();
    let devices = wait_for(&mut client, |e| matches!(e, Event::Devices { .. })).await;
    let v = ev_json(&devices);
    let list = v.get("list").and_then(Value::as_array).unwrap();
    assert_eq!(list[0].get("state"), Some(&json!("available")));
    assert_eq!(list[1].get("state"), Some(&json!("connected")));
    // Disconnect flips the link; the session survives.
    client.send(Cmd::Disconnect).await.unwrap();
    let link = wait_for(
        &mut client,
        |e| matches!(e, Event::Link { state, .. } if state == "disconnected"),
    )
    .await;
    assert_eq!(ev_json(&link).get("state"), Some(&json!("disconnected")));
    client.send(Cmd::Fader { ch: 0, value: 0.5 }).await.unwrap();
    wait_for(&mut client, |e| matches!(e, Event::State { .. })).await;
}

#[tokio::test]
async fn info_reports_desk_identity() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    let snap = drain_snapshot(&mut client).await;
    // Snapshot carries info (sim answers synchronously).
    let info = snap
        .iter()
        .find(|e| matches!(e, Event::Info { .. }))
        .expect("snapshot has info");
    let v = ev_json(info);
    assert_eq!(v.get("sample_rate"), Some(&json!(48000)));
    assert_eq!(v.get("driver"), Some(&json!("sim")));
    assert!(v.get("model").and_then(Value::as_str).unwrap_or("").contains("SimDriver"));
    // On-request info matches.
    client.send(Cmd::Info).await.unwrap();
    let again = wait_for(&mut client, |e| matches!(e, Event::Info { .. })).await;
    assert_eq!(ev_json(&again), ev_json(info));
}

// ---------- numbered scenes ----------

#[tokio::test]
async fn scene_save_recall_roundtrip_restores_plane() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    // Live values to capture: fader + EQ gain + comp threshold + send.
    client.send(Cmd::Fader { ch: 0, value: 0.9 }).await.unwrap();
    for (field, value) in [
        ("eq1_gain", json!(5.0)),
        ("comp_thr", json!(-25.0)),
        ("send1_level", json!(-10.0)),
    ] {
        client
            .send(Cmd::Param { ch: 0, field: field.to_string(), value })
            .await
            .unwrap();
    }
    // Drain the write echoes.
    while client.events.try_recv().is_ok() {}
    client
        .send(Cmd::SceneSave { n: 1, name: "Set 1".to_string() })
        .await
        .unwrap();
    let scenes = wait_for(&mut client, |e| matches!(e, Event::Scenes { .. })).await;
    let v = ev_json(&scenes);
    let list = v.get("scenes").and_then(Value::as_array).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].get("number"), Some(&json!(1)));
    assert_eq!(list[0].get("name"), Some(&json!("Set 1")));
    assert_eq!(list[0].get("current"), Some(&json!(false)));
    // Move away, then recall with no fade.
    client.send(Cmd::Fader { ch: 0, value: 0.1 }).await.unwrap();
    client
        .send(Cmd::Param { ch: 0, field: "eq1_gain".to_string(), value: json!(-5.0) })
        .await
        .unwrap();
    while client.events.try_recv().is_ok() {}
    client.send(Cmd::SceneRecall { n: 1, fade_ms: Some(0) }).await.unwrap();
    // Recall applies every captured value (fader/mute echoes + params).
    let mut got: std::collections::HashMap<String, Value> = Default::default();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !got.contains_key("eq1_gain") || !got.contains_key("comp_thr") || !got.contains_key("send1_level") {
        assert!(std::time::Instant::now() < deadline, "recall echoes arrive: {got:?}");
        if let Ok(Some(e)) = tokio::time::timeout(Duration::from_millis(500), client.events.recv()).await {
            if let Event::Param { ch: 0, field, value, .. } = e {
                got.insert(field, value);
            }
        }
    }
    assert_eq!(got.get("fader"), Some(&json!(0.9f32)));
    assert_eq!(got.get("eq1_gain"), Some(&json!(5.0f32)));
    assert_eq!(got.get("comp_thr"), Some(&json!(-25.0f32)));
    assert_eq!(got.get("send1_level"), Some(&json!(-10.0f32)));
    // Recalled slot is current.
    let scenes = wait_for(
        &mut client,
        |e| matches!(e, Event::Scenes { scenes } if scenes.iter().any(|s| s.number == 1 && s.current)),
    )
    .await;
    let _ = scenes;
    // Rename + delete + clear flow.
    client.send(Cmd::SceneRename { n: 1, name: "Set 1b".to_string() }).await.unwrap();
    let scenes = wait_for(&mut client, |e| matches!(e, Event::Scenes { .. })).await;
    assert_eq!(
        ev_json(&scenes).get("scenes").and_then(Value::as_array).unwrap()[0].get("name"),
        Some(&json!("Set 1b"))
    );
    client.send(Cmd::SceneDelete { n: 1 }).await.unwrap();
    let scenes = wait_for(&mut client, |e| matches!(e, Event::Scenes { .. })).await;
    assert_eq!(ev_json(&scenes).get("scenes").and_then(Value::as_array).unwrap().len(), 0);
    // Unknown slots + bad numbers error.
    client.send(Cmd::SceneRecall { n: 2, fade_ms: Some(0) }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown scene"));
    client.send(Cmd::SceneSave { n: 0, name: "x".to_string() }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "001..=999"));
    client.send(Cmd::SceneSave { n: 3, name: "   ".to_string() }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "non-empty"));
}

#[tokio::test]
async fn scenes_persist_across_restart() {
    let dir = std::env::temp_dir().join(format!(
        "openmix-r2-scenes-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let engine = Engine::new(EngineConfig {
        data_dir: Some(dir.clone()),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    client.send(Cmd::Fader { ch: 2, value: 0.77 }).await.unwrap();
    while client.events.try_recv().is_ok() {}
    client.send(Cmd::SceneSave { n: 7, name: "Persist".to_string() }).await.unwrap();
    wait_for(&mut client, |e| matches!(e, Event::Scenes { .. })).await;
    assert!(dir.join("scenes.json").exists(), "scene file written");
    drop(client);
    drop(engine);
    // A fresh engine on the same dir reloads the library.
    let engine2 = Engine::new(EngineConfig {
        data_dir: Some(dir.clone()),
        ..EngineConfig::default()
    });
    engine2.connect("sim", None).await.unwrap();
    let mut client2 = engine2.attach(None).await;
    let snap = drain_snapshot(&mut client2).await;
    let scenes = snap.iter().find(|e| matches!(e, Event::Scenes { .. })).unwrap();
    let list = ev_json(scenes).get("scenes").and_then(Value::as_array).unwrap().clone();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].get("number"), Some(&json!(7)));
    assert_eq!(list[0].get("name"), Some(&json!("Persist")));
    // And the restored fader value recalls.
    client2.send(Cmd::SceneRecall { n: 7, fade_ms: Some(0) }).await.unwrap();
    let echo = wait_for(
        &mut client2,
        |e| matches!(e, Event::Param { ch: 2, field, .. } if field == "fader"),
    )
    .await;
    assert_eq!(ev_json(&echo).get("value"), Some(&json!(0.77f32)));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------- FX ----------

#[tokio::test]
async fn fx_caps_state_and_commands() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    let snap = drain_snapshot(&mut client).await;
    // caps.fx: 8 slots + six design type tables with param rows.
    let caps = snap.iter().find(|e| matches!(e, Event::Caps { .. })).unwrap();
    let v = ev_json(caps);
    let fx = v.get("fx").expect("caps.fx");
    assert_eq!(fx.get("slots"), Some(&json!(8)));
    let types = fx.get("types").and_then(Value::as_array).unwrap();
    assert_eq!(types.len(), 6);
    let labels: Vec<&str> = types.iter().map(|t| t.get("label").and_then(Value::as_str).unwrap()).collect();
    assert_eq!(labels, vec!["Hall", "Plate", "Room", "Delay", "Chorus", "Flanger"]);
    let hall = &types[0];
    let params = hall.get("params").and_then(Value::as_array).unwrap();
    let plabels: Vec<&str> = params.iter().map(|p| p.get("label").and_then(Value::as_str).unwrap()).collect();
    assert_eq!(plabels, vec!["Decay", "Pre-delay", "Size", "Hi Damp", "Mix"]);
    for key in ["id", "label", "unit", "min", "max", "default", "scale"] {
        assert!(params[0].get(key).is_some(), "fx param row carries {key}");
    }
    // Snapshot fx: 8 design slots, first is Hall/on, last bypassed-off.
    let fx = snap.iter().find(|e| matches!(e, Event::Fx { .. })).unwrap();
    let slots = ev_json(fx).get("slots").and_then(Value::as_array).unwrap().clone();
    assert_eq!(slots.len(), 8);
    assert_eq!(slots[0].get("type"), Some(&json!("Hall")));
    assert_eq!(slots[0].get("ret_label"), Some(&json!("Ret 1/2")));
    assert_eq!(slots[7].get("on"), Some(&json!(false)));
    // set type -> state event; param -> state event; bypass/on toggles.
    client.send(Cmd::FxSetType { slot: 1, fx_type: "Chorus".to_string() }).await.unwrap();
    let fx = wait_for(
        &mut client,
        |e| matches!(e, Event::Fx { slots } if slots.iter().any(|s| s.slot == 1 && s.fx_type == "Chorus")),
    )
    .await;
    let slot1 = ev_json(&fx).get("slots").and_then(Value::as_array).unwrap()[0].clone();
    assert_eq!(slot1.get("params").and_then(Value::as_object).unwrap().get("rate"), Some(&json!(0.4f32)));
    client.send(Cmd::FxParam { slot: 1, id: "rate".to_string(), value: 0.75 }).await.unwrap();
    let fx = wait_for(
        &mut client,
        |e| matches!(e, Event::Fx { slots } if slots.iter().any(|s| s.slot == 1 && s.params.get("rate") == Some(&0.75))),
    )
    .await;
    let _ = fx;
    client.send(Cmd::FxBypass { slot: 1, bypass: true }).await.unwrap();
    wait_for(
        &mut client,
        |e| matches!(e, Event::Fx { slots } if slots.iter().any(|s| s.slot == 1 && s.bypass)),
    )
    .await;
    client.send(Cmd::FxOn { slot: 8, on: true }).await.unwrap();
    wait_for(
        &mut client,
        |e| matches!(e, Event::Fx { slots } if slots.iter().any(|s| s.slot == 8 && s.on)),
    )
    .await;
    // Errors: unknown type, unknown param id, bad slot, bad value.
    client.send(Cmd::FxSetType { slot: 1, fx_type: "Reverb".to_string() }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown fx type"));
    client.send(Cmd::FxParam { slot: 1, id: "decay".to_string(), value: 0.5 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown fx param"));
    client.send(Cmd::FxParam { slot: 9, id: "mix".to_string(), value: 0.5 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "out of range"));
    client.send(Cmd::FxParam { slot: 1, id: "mix".to_string(), value: 2.0 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "0.0..=1.0"));
}

// ---------- routing ----------

#[tokio::test]
async fn routing_patch_roundtrip() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    let snap = drain_snapshot(&mut client).await;
    let caps = snap.iter().find(|e| matches!(e, Event::Caps { .. })).unwrap();
    let routing = ev_json(caps).get("routing").cloned().unwrap();
    assert_eq!(routing.get("supported"), Some(&json!(true)));
    assert_eq!(routing.get("per_strip"), Some(&json!(true)));
    let groups = routing.get("groups").and_then(Value::as_array).unwrap();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].get("key"), Some(&json!("Lcl")));
    assert_eq!(groups[0].get("count"), Some(&json!(32)));
    // Defaults mirror the design (Ch n <- Lcl n): snapshot carries them.
    let routing = snap.iter().find(|e| matches!(e, Event::Routing { .. })).unwrap();
    let patches = ev_json(routing).get("patches").and_then(Value::as_array).unwrap().clone();
    assert!(!patches.is_empty());
    assert!(patches.iter().any(|p| p.get("strip") == Some(&json!(0)) && p.get("label") == Some(&json!("Lcl 01"))));
    // Patch + clear round-trip.
    client.send(Cmd::PatchSet { strip: 0, group: "Net".to_string(), index: 5 }).await.unwrap();
    let routing = wait_for(
        &mut client,
        |e| matches!(e, Event::Routing { patches } if patches.iter().any(|p| p.strip == 0 && p.label == "Net 05")),
    )
    .await;
    let _ = routing;
    client.send(Cmd::PatchClear { strip: 0 }).await.unwrap();
    wait_for(
        &mut client,
        |e| matches!(e, Event::Routing { patches } if patches.iter().all(|p| p.strip != 0)),
    )
    .await;
    // Errors: unknown group, bad index, non-input strip.
    client.send(Cmd::PatchSet { strip: 0, group: "AES".to_string(), index: 1 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown patch group"));
    client.send(Cmd::PatchSet { strip: 0, group: "Lcl".to_string(), index: 33 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "out of range"));
    client.send(Cmd::PatchSet { strip: 32, group: "Lcl".to_string(), index: 1 }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "input strips"));
}

// ---------- offline editing ----------

#[tokio::test]
async fn offline_diff_push_flow() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    // Go offline on the xair model (sim-backed, no hardware).
    client.send(Cmd::Offline { model: "xair".to_string() }).await.unwrap();
    let link = wait_for(
        &mut client,
        |e| matches!(e, Event::Link { state, .. } if state == "offline"),
    )
    .await;
    assert_eq!(ev_json(&link).get("name"), Some(&json!("Offline (xair)")));
    // Edit while offline: fader + param.
    client.send(Cmd::Fader { ch: 1, value: 0.55 }).await.unwrap();
    client
        .send(Cmd::Param { ch: 1, field: "gain".to_string(), value: json!(33.0) })
        .await
        .unwrap();
    while client.events.try_recv().is_ok() {}
    // Reconnect: the change list arrives as offline_diff, nothing pushed.
    client.send(Cmd::Connect { driver: "sim".to_string(), addr: None }).await.unwrap();
    let diff = wait_for(&mut client, |e| matches!(e, Event::OfflineDiff { .. })).await;
    let v = ev_json(&diff);
    assert_eq!(v.get("count"), Some(&json!(2)));
    let changes = v.get("changes").and_then(Value::as_array).unwrap();
    let ids: Vec<String> = changes
        .iter()
        .map(|c| format!("{}:{}", c.get("strip").unwrap(), c.get("id").and_then(Value::as_str).unwrap()))
        .collect();
    assert!(ids.contains(&"1:fader".to_string()), "{ids:?}");
    assert!(ids.contains(&"1:gain".to_string()), "{ids:?}");
    // Push needs explicit confirm.
    client.send(Cmd::OfflinePush { confirm: false }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "confirm:true"));
    client.send(Cmd::OfflinePush { confirm: true }).await.unwrap();
    let echo = wait_for(
        &mut client,
        |e| matches!(e, Event::Param { ch: 1, field, .. } if field == "gain"),
    )
    .await;
    assert_eq!(ev_json(&echo).get("value"), Some(&json!(33.0f32)));
    // Second push (or unknown model) errors; the session survives.
    client.send(Cmd::OfflinePush { confirm: true }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "no offline diff pending"));
    client.send(Cmd::Offline { model: "nope".to_string() }).await.unwrap();
    let err = wait_for(&mut client, |e| matches!(e, Event::Error { .. })).await;
    assert!(is_error_containing(&err, "unknown driver"));
    // Discard path: offline, edit, connect, discard.
    client.send(Cmd::Offline { model: "dl32s".to_string() }).await.unwrap();
    wait_for(&mut client, |e| matches!(e, Event::Link { state, .. } if state == "offline")).await;
    client.send(Cmd::Fader { ch: 2, value: 0.11 }).await.unwrap();
    while client.events.try_recv().is_ok() {}
    client.send(Cmd::Connect { driver: "sim".to_string(), addr: None }).await.unwrap();
    wait_for(&mut client, |e| matches!(e, Event::OfflineDiff { count, .. } if *count == 1)).await;
    client.send(Cmd::OfflineDiscard).await.unwrap();
    let diff = wait_for(&mut client, |e| matches!(e, Event::OfflineDiff { .. })).await;
    assert_eq!(ev_json(&diff).get("count"), Some(&json!(0)));
}

// ---------- meter taps ----------

#[tokio::test]
async fn taps_negotiation_and_frames() {
    let engine = new_engine().await;
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    // No taps by default; negotiate with hello.
    client.send_json(r#"{"cmd":"hello","taps":true}"#).await.unwrap();
    let ack = wait_for(&mut client, |e| matches!(e, Event::Hello { .. })).await;
    assert_eq!(ev_json(&ack).get("taps"), Some(&json!(true)));
    assert!(engine.client_taps(&client.id));
    // One frame per tap per tick; sim covers all 77 strips.
    for want in ["in", "post", "gr_gate", "gr_comp"] {
        let frame = wait_for(
            &mut client,
            |e| matches!(e, Event::Taps { tap, .. } if tap == want),
        )
        .await;
        if let Event::Taps { levels, .. } = frame {
            assert_eq!(levels.len(), 77, "tap {want} covers every strip");
        } else {
            panic!("expected taps");
        }
    }
    // Opt out again.
    client.send_json(r#"{"cmd":"hello","taps":false}"#).await.unwrap();
    let ack = wait_for(&mut client, |e| matches!(e, Event::Hello { .. })).await;
    assert_eq!(ev_json(&ack).get("taps"), None, "false taps stays off the wire");
    assert!(!engine.client_taps(&client.id));
}

// ---------- poller pipelining ----------

#[tokio::test]
async fn poller_inflight_defaults_and_focus_order() {
    let engine = new_engine().await;
    assert_eq!(engine.max_inflight("sim"), 4, "sim pipelines by default");
    assert_eq!(engine.max_inflight("xair"), 1, "xair stays sequential until rig-verified");
    assert_eq!(engine.max_inflight("dl32s"), 1);
    assert_eq!(engine.effective_inflight("sim"), 4, "no saturation yet");
    let capped = Engine::new(EngineConfig {
        poll_inflight: Some(99),
        ..EngineConfig::default()
    });
    assert_eq!(capped.max_inflight("sim"), 16, "inflight clamps at 16");

    // Ordering: focus drains ahead of the background sweep, pipelined.
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    client.send(Cmd::Focus { ch: Some(5), chs: None }).await.unwrap();
    // ~140 scoped pairs on an input strip; 40 pipelined ticks of 4 drain
    // the whole focus block (plus a little background).
    for _ in 0..40 {
        engine.poll_tick().await;
    }
    let mut chs: Vec<u16> = Vec::new();
    while let Ok(e) = client.events.try_recv() {
        if let Event::Params { ch, .. } = e {
            chs.push(ch);
        }
    }
    assert!(!chs.is_empty());
    // The focus block (ch5) arrives before any background strip.
    let first_other = chs.iter().position(|&c| c != 5);
    let nparams = chs.iter().filter(|&&c| c == 5).count();
    assert!(nparams > 100, "focus block lands first ({nparams} ch5 frames)");
    assert!(
        first_other.map_or(true, |i| i >= nparams),
        "no background strip interleaves the focus block"
    );
}

#[tokio::test]
async fn visible_lane_serves_focused_strips_first() {
    // Parked background poller (1 h tick): only manual poll_ticks drive
    // the poller, so the test is fully deterministic.
    let engine = Engine::new(EngineConfig {
        poll_interval: Duration::from_secs(3600),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut client = engine.attach(None).await;
    drain_snapshot(&mut client).await;
    // Focus 12 far input strips (40..52: background cursor sits near
    // strip 0..2, so the lanes are observably distinct), then drain the
    // whole focus backlog manually.
    let chs: Vec<Value> = (40..52u16).map(|c| json!(c)).collect();
    client.send(Cmd::Focus { ch: None, chs: Some(chs) }).await.unwrap();
    for _ in 0..600 {
        engine.poll_tick().await;
    }
    while client.events.try_recv().is_ok() {}
    // Reconnect: cache cleared, focus backlog long gone, visible set kept.
    // The next batches come from the visible-group lane first (the
    // background keeps a reserved share, so it is a majority assertion,
    // with the very first frame always visible — batch order).
    client.send(Cmd::Connect { driver: "sim".to_string(), addr: None }).await.unwrap();
    wait_for(&mut client, |e| matches!(e, Event::Routing { .. })).await;
    while client.events.try_recv().is_ok() {}
    for _ in 0..30 {
        engine.poll_tick().await;
    }
    let mut chs: Vec<u16> = Vec::new();
    while let Ok(e) = client.events.try_recv() {
        if let Event::Params { ch, .. } = e {
            chs.push(ch);
        }
    }
    assert!(!chs.is_empty());
    assert!(
        (40u16..52).contains(&chs[0]),
        "first frame after reset is a visible strip: {chs:?}"
    );
    let vis = chs.iter().filter(|c| (40u16..52).contains(*c)).count();
    assert!(
        vis * 4 >= chs.len() * 3,
        "visible lane dominates the background share ({vis}/{}): {chs:?}",
        chs.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pipelined_writes_still_preempt() {
    // 1 ms tick + sim inflight 4 (saturated poller): every write lane
    // must still dispatch in <= 5 ms p99 — writes never queue behind
    // pipelined poll reads.
    let engine = Engine::new(EngineConfig {
        poll_interval: Duration::from_millis(1),
        ..EngineConfig::default()
    });
    engine.connect("sim", None).await.unwrap();
    let mut me = engine.attach(Some("writer".to_string())).await;
    drain_snapshot(&mut me).await;
    for i in 0..20u16 {
        me.send(Cmd::Fader { ch: i % 32, value: 0.5 }).await.unwrap();
    }
    while me.events.try_recv().is_ok() {}
    let n = 500usize;
    let mut dts = Vec::with_capacity(n);
    for i in 0..n {
        let ch = (i % 32) as u16;
        let v = 0.1 + 0.8 * ((i % 100) as f32 / 100.0);
        let t = std::time::Instant::now();
        if i % 2 == 0 {
            me.send(Cmd::Fader { ch, value: v }).await.unwrap();
        } else {
            me.send(Cmd::Param {
                ch,
                field: "gain".to_string(),
                value: json!(20.0),
            })
            .await
            .unwrap();
        }
        dts.push(t.elapsed());
    }
    dts.sort_unstable();
    let p99 = dts[((0.99 * n as f64) as usize).min(n - 1)];
    println!("pipelined_write p99={p99:?} poll_skips={}", engine.poll_skips());
    assert!(p99 <= Duration::from_millis(5), "pipelined write p99 {p99:?} exceeds 5 ms");
    // Every write landed (last write wins on ch 19... check final state).
    me.send(Cmd::Fader { ch: 7, value: 0.77 }).await.unwrap();
    let state = wait_for(&mut me, |e| {
        matches!(e, Event::State { strips, .. } if strips.iter().any(|s| s.ch == 7 && (s.fader - 0.77).abs() < 1e-6))
    })
    .await;
    let _ = state;
}
